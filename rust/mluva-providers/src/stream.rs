//! One streaming recognition connection, shared by the Microsoft and xAI wire protocols.
//!
//! The provider module supplies a [`StreamProtocol`]; this session owns the socket, audio queue,
//! cancellation and the rule that only finished segments become text. Volatile recognition is
//! preview-only. A stream that ends without finishing the words it previewed fails, so the caller
//! recognizes the retained recording again instead of losing them.

use crate::{
    ProviderError, Result, TranscriptionResult,
    realtime::{
        CommittedCallback, PreviewCallback, RealtimeCommittedSegment, RealtimeOptions,
        RealtimePreview, RealtimeSessionResult,
    },
};
use futures_util::{SinkExt, StreamExt, stream::SplitSink};
use mluva_audio::{SAMPLE_RATE, SAMPLE_WIDTH_BYTES};
use mluva_core::text::trim;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::sync::{Notify, mpsc};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::{Message, http::Request, protocol::WebSocketConfig};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tokio_util::sync::CancellationToken;

/// A server message, reduced to what the session acts on.
#[derive(Debug, PartialEq)]
pub(crate) enum StreamEvent {
    /// The server accepted the stream and audio may flow.
    Ready,
    /// Volatile text for the segment in progress; replaces the previous volatile text.
    Interim(String),
    /// Text to add to the volatile preview, for services that stream deltas rather than whole
    /// segments. Never committed on its own.
    Append(String),
    /// The service finished the turn and returned its complete text, which replaces any segments.
    Final(String),
    /// The segment in progress produced no text, such as Azure's `NoMatch`: drop its preview.
    Discard,
    /// A finished segment. A later segment with the same key replaces the earlier one, so a
    /// repeated delivery of the same segment is counted once.
    Segment { key: i64, text: String },
    /// The server finished the turn after the end of audio, with the segments it sent already
    /// delivered.
    Done,
    /// The service reported an error; the message is fixed text, never provider output.
    Failed(String),
}

/// The wire format of one provider.
pub(crate) trait StreamProtocol: Send + Sync + 'static {
    /// How the provider is named in messages, such as "Microsoft".
    fn name(&self) -> &'static str;
    /// Whether the server confirms the stream with its first message before audio may flow.
    fn waits_for_ready(&self) -> bool;
    /// Frames sent once the connection is open and, when the protocol waits for it, ready.
    fn opening(&self) -> Vec<Message>;
    /// Frames carrying one PCM chunk; `first` marks the first chunk of the session.
    fn audio(&self, pcm: &[u8], first: bool) -> Vec<Message>;
    /// Frames announcing that no more audio follows.
    fn end(&self) -> Vec<Message>;
    /// Decode one server message; messages that matter to nobody return no events.
    fn decode(&self, message: &Message) -> Vec<StreamEvent>;
}

type Connection = WebSocketStream<MaybeTlsStream<TcpStream>>;
type Sender = SplitSink<Connection, Message>;

const MAX_EVENT_BYTES: usize = crate::realtime::MAX_EVENT_BYTES;
/// A recording with this much audio and no text is treated as a failed stream, not as silence:
/// the retained recording is recognized again in batch before anything is called "no speech".
/// Services have answered a stream they could not serve (parallel free-tier sessions) with an
/// empty, otherwise successful result.
const AUDIO_EXPECTING_TEXT_BYTES: usize = SAMPLE_RATE as usize * SAMPLE_WIDTH_BYTES;

enum Audio {
    Frames(Vec<u8>),
    Finish,
}

#[derive(Default)]
struct Policy {
    segments: Vec<(i64, String)>,
    volatile: String,
    failure: Option<String>,
    finishing: bool,
    cancelled: bool,
    closing: bool,
    bytes_sent: usize,
    /// The server finished the turn.
    done: bool,
    sender_done: bool,
}

struct Shared {
    protocol: Arc<dyn StreamProtocol>,
    policy: Mutex<Policy>,
    changed: Notify,
    close: CancellationToken,
    closed: CancellationToken,
    language: String,
    identifier: String,
    preview: Mutex<Option<PreviewCallback>>,
    committed: Mutex<Option<CommittedCallback>>,
}

impl Shared {
    fn fail(&self, message: impl Into<String>) {
        let mut state = self.policy.lock().unwrap();
        if state.failure.is_none() && !state.cancelled {
            state.failure = Some(message.into());
        }
        drop(state);
        self.changed.notify_waiters();
    }
    fn failure(&self) -> Option<String> {
        self.policy.lock().unwrap().failure.clone()
    }
    fn snapshot(&self) -> RealtimePreview {
        let state = self.policy.lock().unwrap();
        RealtimePreview {
            committed_text: join(state.segments.iter().map(|(_, text)| text.as_str())),
            volatile_text: trim(&state.volatile).into(),
        }
    }
    fn expected_close(&self) -> bool {
        let state = self.policy.lock().unwrap();
        state.cancelled || state.closing || state.done
    }
    async fn preview(self: &Arc<Self>) {
        if self.policy.lock().unwrap().cancelled {
            return;
        }
        let shared = self.clone();
        let value = self.snapshot();
        let _ = tokio::task::spawn_blocking(move || {
            if shared.policy.lock().unwrap().cancelled {
                return;
            }
            let callback = shared.preview.lock().unwrap().take();
            if let Some(mut consumer) = callback {
                let succeeded = matches!(
                    catch_unwind(AssertUnwindSafe(|| consumer(value))),
                    Ok(Ok(()))
                );
                let state = shared.policy.lock().unwrap();
                if succeeded && !state.cancelled {
                    *shared.preview.lock().unwrap() = Some(consumer);
                }
            }
        })
        .await;
    }
    async fn committed(self: &Arc<Self>, segment: RealtimeCommittedSegment) {
        if self.policy.lock().unwrap().cancelled {
            return;
        }
        let shared = self.clone();
        let _ = tokio::task::spawn_blocking(move || {
            if shared.policy.lock().unwrap().cancelled {
                return;
            }
            let callback = shared.committed.lock().unwrap().take();
            if let Some(mut consumer) = callback {
                let succeeded = matches!(
                    catch_unwind(AssertUnwindSafe(|| consumer(segment))),
                    Ok(Ok(()))
                );
                let state = shared.policy.lock().unwrap();
                if succeeded && !state.cancelled {
                    *shared.committed.lock().unwrap() = Some(consumer);
                }
            }
        })
        .await;
    }

    async fn event(self: &Arc<Self>, event: StreamEvent) {
        match event {
            StreamEvent::Ready => {}
            StreamEvent::Failed(message) => self.fail(message),
            StreamEvent::Interim(text) => {
                {
                    let mut state = self.policy.lock().unwrap();
                    if state.cancelled || state.closing || state.done {
                        return;
                    }
                    state.volatile = trim(&text).into();
                }
                self.preview().await;
            }
            StreamEvent::Discard => {
                {
                    let mut state = self.policy.lock().unwrap();
                    if state.cancelled || state.closing || state.done {
                        return;
                    }
                    state.volatile.clear();
                }
                self.preview().await;
            }
            StreamEvent::Append(text) => {
                {
                    let mut state = self.policy.lock().unwrap();
                    if state.cancelled || state.closing || state.done {
                        return;
                    }
                    state.volatile.push_str(&text);
                }
                self.preview().await;
            }
            StreamEvent::Final(text) => {
                let name = self.protocol.name();
                let text = trim(&text).to_owned();
                {
                    let mut state = self.policy.lock().unwrap();
                    if state.cancelled || state.closing {
                        return;
                    }
                    if !state.finishing {
                        drop(state);
                        self.fail(format!(
                            "{name} ended the realtime stream before recording stopped."
                        ));
                        return;
                    }
                    if text.is_empty() && !trim(&state.volatile).is_empty() {
                        // Words were previewed but the final text came back empty.
                        drop(state);
                        self.fail(format!(
                            "{name} did not return final text for the last words."
                        ));
                        return;
                    }
                    state.volatile.clear();
                    state.segments = if text.is_empty() {
                        vec![]
                    } else {
                        vec![(0, text)]
                    };
                    state.done = true;
                }
                self.changed.notify_waiters();
                self.preview().await;
            }
            StreamEvent::Segment { key, text } => {
                let text = trim(&text);
                let new_segment = {
                    let mut state = self.policy.lock().unwrap();
                    if state.cancelled || state.closing || state.done {
                        return;
                    }
                    state.volatile.clear();
                    if text.is_empty() {
                        None
                    } else if let Some(existing) =
                        state.segments.iter_mut().find(|(known, _)| *known == key)
                    {
                        existing.1 = text.into();
                        None
                    } else {
                        let sequence = state.segments.len();
                        state.segments.push((key, text.into()));
                        Some(RealtimeCommittedSegment {
                            identifier: format!("{}-{sequence}", self.identifier),
                            sequence,
                            text: text.into(),
                        })
                    }
                };
                if let Some(segment) = new_segment {
                    self.committed(segment).await;
                }
                self.preview().await;
            }
            StreamEvent::Done => {
                let name = self.protocol.name();
                {
                    let mut state = self.policy.lock().unwrap();
                    if state.cancelled || state.closing {
                        return;
                    }
                    if !state.finishing {
                        drop(state);
                        self.fail(format!(
                            "{name} ended the realtime stream before recording stopped."
                        ));
                        return;
                    }
                    if !state.volatile.is_empty() {
                        // Words were previewed but never finished. Never promote them.
                        drop(state);
                        self.fail(format!(
                            "{name} did not return final text for the last words."
                        ));
                        return;
                    }
                    state.done = true;
                }
                self.changed.notify_waiters();
            }
        }
    }
}

struct Workers {
    sender: JoinHandle<Sender>,
    receiver: JoinHandle<()>,
}

/// A live streaming recognition. Mirrors the ElevenLabs realtime session's surface so capture
/// treats every realtime route alike.
pub struct StreamSession {
    shared: Arc<Shared>,
    audio: mpsc::Sender<Audio>,
    workers: Arc<Mutex<Option<Workers>>>,
    options: RealtimeOptions,
    runtime: tokio::runtime::Handle,
}

/// Open `request` (a client handshake with the provider's headers), send the protocol's opening frames and, when the protocol asks for it, wait for
/// the server to accept the stream.
pub(crate) async fn connect(
    request: Request<()>,
    protocol: Arc<dyn StreamProtocol>,
    language: &str,
    options: RealtimeOptions,
    on_preview: Option<PreviewCallback>,
    on_committed: Option<CommittedCallback>,
) -> Result<StreamSession> {
    let name = protocol.name();
    let unreachable = || {
        ProviderError(format!(
            "{name} realtime transcription could not reach the service."
        ))
    };
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_EVENT_BYTES))
        .max_frame_size(Some(MAX_EVENT_BYTES));
    let (mut connection, _) = tokio::time::timeout(
        options.session_timeout,
        tokio_tungstenite::connect_async_with_config(request, Some(config), false),
    )
    .await
    .map_err(|_| unreachable())?
    .map_err(|error| match error {
        tokio_tungstenite::tungstenite::Error::Http(response) => {
            match response.status().as_u16() {
                401 | 403 => ProviderError(format!(
                    "{name} rejected the credentials for realtime transcription."
                )),
                429 => ProviderError(format!(
                    "{name} realtime transcription is rate limited or already in use. Try again shortly."
                )),
                _ => unreachable(),
            }
        }
        _ => unreachable(),
    })?;
    if protocol.waits_for_ready() {
        let not_ready = || {
            ProviderError(format!(
                "{name} did not confirm that realtime transcription was ready."
            ))
        };
        let ready = tokio::time::timeout(options.session_timeout, async {
            while let Some(message) = connection.next().await {
                let message = message.map_err(|_| ())?;
                if matches!(message, Message::Close(_)) {
                    return Err(());
                }
                if matches!(
                    message,
                    Message::Ping(_) | Message::Pong(_) | Message::Frame(_)
                ) {
                    continue;
                }
                return Ok(protocol.decode(&message));
            }
            Err(())
        })
        .await;
        let accepted = match ready {
            Ok(Ok(events)) => {
                if let Some(StreamEvent::Failed(message)) = events
                    .iter()
                    .find(|event| matches!(event, StreamEvent::Failed(_)))
                {
                    let message = message.clone();
                    let _ =
                        tokio::time::timeout(Duration::from_secs(2), connection.close(None)).await;
                    return Err(ProviderError(message));
                }
                events.contains(&StreamEvent::Ready)
            }
            _ => false,
        };
        if !accepted {
            let _ = tokio::time::timeout(Duration::from_secs(2), connection.close(None)).await;
            return Err(not_ready());
        }
    }
    for frame in protocol.opening() {
        if !matches!(
            tokio::time::timeout(options.session_timeout, connection.send(frame)).await,
            Ok(Ok(()))
        ) {
            return Err(unreachable());
        }
    }
    Ok(StreamSession::start(
        connection,
        protocol,
        language.into(),
        options,
        on_preview,
        on_committed,
    ))
}

impl StreamSession {
    fn start(
        connection: Connection,
        protocol: Arc<dyn StreamProtocol>,
        language: String,
        options: RealtimeOptions,
        on_preview: Option<PreviewCallback>,
        on_committed: Option<CommittedCallback>,
    ) -> Self {
        let shared = Arc::new(Shared {
            protocol,
            policy: Mutex::new(Policy::default()),
            changed: Notify::new(),
            close: CancellationToken::new(),
            closed: CancellationToken::new(),
            language,
            identifier: uuid::Uuid::new_v4().simple().to_string(),
            preview: Mutex::new(on_preview),
            committed: Mutex::new(on_committed),
        });
        let (audio, queue) = mpsc::channel(options.maximum_queued_chunks);
        let (sender, mut receiver) = futures_util::StreamExt::split(connection);
        let sender = tokio::spawn(send_audio(sender, queue, shared.clone()));
        let receive_state = shared.clone();
        let receiver = tokio::spawn(async move {
            loop {
                let message = tokio::select! {biased; _ = receive_state.close.cancelled() => break, value = receiver.next() => value};
                match message {
                    Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_))) => continue,
                    Some(Ok(Message::Close(frame))) => {
                        if !receive_state.expected_close() {
                            let name = receive_state.protocol.name();
                            receive_state.fail(match frame {
                                Some(frame) => format!(
                                    "The {name} realtime connection closed before transcription completed (code {}).",
                                    u16::from(frame.code)
                                ),
                                None => format!(
                                    "The {name} realtime connection closed before transcription completed."
                                ),
                            });
                        }
                        break;
                    }
                    Some(Ok(message)) => {
                        for event in receive_state.protocol.decode(&message) {
                            receive_state.event(event).await;
                        }
                    }
                    _ => {
                        if !receive_state.expected_close() {
                            let name = receive_state.protocol.name();
                            receive_state.fail(format!(
                                "The {name} realtime connection closed before transcription completed."
                            ));
                        }
                        break;
                    }
                }
                let state = receive_state.policy.lock().unwrap();
                if state.cancelled || state.closing || state.done {
                    break;
                }
            }
        });
        Self {
            shared,
            audio,
            workers: Arc::new(Mutex::new(Some(Workers { sender, receiver }))),
            options,
            runtime: tokio::runtime::Handle::current(),
        }
    }

    pub fn bytes_sent(&self) -> usize {
        self.shared.policy.lock().unwrap().bytes_sent
    }
    pub fn is_healthy(&self) -> bool {
        let state = self.shared.policy.lock().unwrap();
        state.failure.is_none() && !state.cancelled
    }
    pub fn snapshot(&self) -> RealtimePreview {
        self.shared.snapshot()
    }
    pub fn submit_audio(&self, frames: &[u8]) -> Result<bool> {
        if frames.is_empty() {
            return Ok(true);
        }
        if !frames.len().is_multiple_of(SAMPLE_WIDTH_BYTES) {
            return Err(ProviderError::message(
                "Realtime audio chunks must contain complete PCM16 samples.",
            ));
        }
        {
            let state = self.shared.policy.lock().unwrap();
            if state.finishing || state.cancelled || state.failure.is_some() {
                return Ok(false);
            }
        }
        match self.audio.try_send(Audio::Frames(frames.to_vec())) {
            Ok(()) => Ok(true),
            Err(_) => {
                self.shared
                    .fail("Realtime transcription could not keep up with microphone audio.");
                Ok(false)
            }
        }
    }

    /// Announce the end of audio and return the finished segments joined with spaces. Fails, rather
    /// than guessing, when the service reports an error, stops early or leaves words unfinished.
    pub async fn finish(&self) -> Result<RealtimeSessionResult> {
        let started = Instant::now();
        let name = self.shared.protocol.name();
        {
            let mut state = self.shared.policy.lock().unwrap();
            if state.cancelled {
                return Err(ProviderError::message(
                    "Realtime transcription was cancelled.",
                ));
            }
            if state.finishing {
                return Err(ProviderError::message(
                    "Realtime transcription is already finalizing.",
                ));
            }
            state.finishing = true;
        }
        if let Some(failure) = self.shared.failure() {
            self.cancel();
            return Err(ProviderError(failure));
        }
        let deadline = tokio::time::Instant::now() + self.options.finalization_timeout;
        if !matches!(
            tokio::time::timeout_at(deadline, self.audio.send(Audio::Finish)).await,
            Ok(Ok(()))
        ) {
            self.shared
                .fail("Realtime transcription did not accept the final audio chunk.");
        }
        if !self.wait_for(deadline, |state| state.sender_done).await {
            self.shared
                .fail("Realtime transcription did not finish sending microphone audio.");
        }
        if self.shared.failure().is_none() && !self.wait_for(deadline, |state| state.done).await {
            self.shared.fail(format!(
                "{name} did not return a final realtime transcript in time."
            ));
        }
        if let Some(failure) = self.shared.failure() {
            self.cancel();
            return Err(ProviderError(failure));
        }
        let transcription = {
            let state = self.shared.policy.lock().unwrap();
            if state.cancelled {
                return Err(ProviderError::message(
                    "Realtime transcription was cancelled.",
                ));
            }
            let text = join(state.segments.iter().map(|(_, text)| text.as_str()));
            if text.is_empty() && state.bytes_sent >= AUDIO_EXPECTING_TEXT_BYTES {
                drop(state);
                self.cancel();
                return Err(ProviderError(format!(
                    "{name} returned no realtime text for the recording."
                )));
            }
            TranscriptionResult {
                text,
                language_code: if self.shared.language == "auto" {
                    "und".into()
                } else {
                    self.shared.language.clone()
                },
                language_probability: None,
                transcription_id: Some(self.shared.identifier.clone()),
                speaker_segments: vec![],
                words: vec![],
                audio_duration_seconds: Some(
                    state.bytes_sent as f64 / (f64::from(SAMPLE_RATE) * SAMPLE_WIDTH_BYTES as f64),
                ),
            }
        };
        let result = RealtimeSessionResult {
            transcription,
            finalization_seconds: started.elapsed().as_secs_f64(),
        };
        self.request_close();
        self.shared.closed.cancelled().await;
        Ok(result)
    }

    async fn wait_for(
        &self,
        deadline: tokio::time::Instant,
        condition: impl Fn(&Policy) -> bool,
    ) -> bool {
        loop {
            let notification = self.shared.changed.notified();
            tokio::pin!(notification);
            notification.as_mut().enable();
            {
                let state = self.shared.policy.lock().unwrap();
                if condition(&state) {
                    return true;
                }
                if state.cancelled || state.failure.is_some() {
                    return false;
                }
            }
            if tokio::time::timeout_at(deadline, notification)
                .await
                .is_err()
            {
                return false;
            }
        }
    }

    pub fn cancel(&self) {
        self.request_cancel();
        self.request_close();
    }

    fn request_close(&self) {
        // Cleanup outlives a dropped finishing future: only the runtime-owned closer takes the
        // transport tasks and acknowledges their completion.
        self.runtime
            .spawn(close_workers(self.shared.clone(), self.workers.clone()));
    }

    /// Capture owners await this before releasing a cancelled session.
    pub async fn cancel_and_wait(&self) {
        self.cancel();
        self.shared.closed.cancelled().await;
    }

    fn request_cancel(&self) {
        {
            let mut state = self.shared.policy.lock().unwrap();
            state.cancelled = true;
            state.finishing = true;
            state.volatile.clear();
        }
        self.shared.preview.lock().unwrap().take();
        self.shared.committed.lock().unwrap().take();
        self.shared.changed.notify_waiters();
        self.shared.close.cancel();
    }
}

impl Drop for StreamSession {
    fn drop(&mut self) {
        let closing = self.shared.policy.lock().unwrap().closing;
        if !closing {
            self.cancel();
        }
    }
}

async fn send_audio(
    mut sender: Sender,
    mut queue: mpsc::Receiver<Audio>,
    shared: Arc<Shared>,
) -> Sender {
    let mut first = true;
    loop {
        let chunk = tokio::select! {biased; _ = shared.close.cancelled() => break, value = queue.recv() => value};
        let Some(chunk) = chunk else {
            break;
        };
        let (frames, pcm_bytes, finishing) = match chunk {
            Audio::Frames(pcm) => (shared.protocol.audio(&pcm, first), pcm.len(), false),
            Audio::Finish => (shared.protocol.end(), 0, true),
        };
        if shared.policy.lock().unwrap().cancelled {
            break;
        }
        let mut sent_all = true;
        for frame in frames {
            let sent = tokio::select! {biased; _ = shared.close.cancelled() => None, result = sender.send(frame) => Some(result)};
            match sent {
                Some(Ok(())) => {}
                Some(Err(_)) => {
                    shared.fail(format!(
                        "The {} realtime connection closed while sending audio.",
                        shared.protocol.name()
                    ));
                    sent_all = false;
                    break;
                }
                None => {
                    sent_all = false;
                    break;
                }
            }
        }
        if !sent_all || finishing {
            break;
        }
        first = false;
        shared.policy.lock().unwrap().bytes_sent += pcm_bytes;
    }
    shared.policy.lock().unwrap().sender_done = true;
    shared.changed.notify_waiters();
    sender
}

async fn close_workers(shared: Arc<Shared>, workers: Arc<Mutex<Option<Workers>>>) {
    {
        let mut state = shared.policy.lock().unwrap();
        if state.closing {
            return;
        }
        state.closing = true;
    }
    shared.close.cancel();
    shared.changed.notify_waiters();
    let handles = workers.lock().unwrap().take();
    if let Some(mut handles) = handles {
        match tokio::time::timeout(Duration::from_secs(2), &mut handles.sender).await {
            Ok(Ok(mut sender)) => {
                let _ = tokio::time::timeout(Duration::from_secs(2), sender.close()).await;
            }
            Ok(Err(_)) => {}
            Err(_) => {
                handles.sender.abort();
                let _ = handles.sender.await;
            }
        }
        if tokio::time::timeout(Duration::from_secs(2), &mut handles.receiver)
            .await
            .is_err()
        {
            handles.receiver.abort();
            let _ = handles.receiver.await;
        }
    }
    shared.closed.cancel();
}

fn join<'a>(segments: impl IntoIterator<Item = &'a str>) -> String {
    segments
        .into_iter()
        .map(trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
