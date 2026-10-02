use crate::{ProviderError, Result, Secret, TranscriptionResult, USER_AGENT, transport};
use base64::{Engine, engine::general_purpose::STANDARD};
use futures_util::{SinkExt, StreamExt, stream::SplitSink};
use mluva_audio::{SAMPLE_RATE, SAMPLE_WIDTH_BYTES};
use mluva_core::text::trim;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::sync::{Notify, mpsc};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::{
    Message, client::IntoClientRequest, protocol::WebSocketConfig,
};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tokio_util::sync::CancellationToken;

pub const SCRIBE_REALTIME_ENDPOINT: &str = "wss://api.elevenlabs.io/v1/speech-to-text/realtime";
pub const SCRIBE_REALTIME_MODEL: &str = "scribe_v2_realtime";
pub const MAX_EVENT_BYTES: usize = 1_048_576;
pub const MANUAL_COMMIT_INTERVAL_BYTES: usize = 25 * SAMPLE_RATE as usize * SAMPLE_WIDTH_BYTES;
const INVALID_EVENT: &str = "ElevenLabs returned an invalid realtime transcription event.";
const NOT_READY: &str = "ElevenLabs did not confirm that realtime transcription was ready.";

type Connection = WebSocketStream<MaybeTlsStream<TcpStream>>;
type Sender = SplitSink<Connection, Message>;
pub type PreviewCallback =
    Box<dyn FnMut(RealtimePreview) -> std::result::Result<(), String> + Send>;
pub type CommittedCallback =
    Box<dyn FnMut(RealtimeCommittedSegment) -> std::result::Result<(), String> + Send>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealtimePreview {
    pub committed_text: String,
    pub volatile_text: String,
}
impl RealtimePreview {
    pub fn display_text(&self) -> String {
        join_segments([self.committed_text.as_str(), self.volatile_text.as_str()])
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealtimeCommittedSegment {
    pub identifier: String,
    pub sequence: usize,
    pub text: String,
}
#[derive(Debug)]
pub struct RealtimeSessionResult {
    pub transcription: TranscriptionResult,
    pub finalization_seconds: f64,
}

#[derive(Clone, Debug)]
pub struct RealtimeOptions {
    pub session_timeout: Duration,
    pub finalization_timeout: Duration,
    pub maximum_queued_chunks: usize,
}
impl Default for RealtimeOptions {
    fn default() -> Self {
        Self {
            session_timeout: Duration::from_secs(10),
            finalization_timeout: Duration::from_secs(8),
            maximum_queued_chunks: 64,
        }
    }
}

pub struct ElevenLabsRealtimeClient {
    api_key: Secret,
    endpoint: String,
    options: RealtimeOptions,
}
impl ElevenLabsRealtimeClient {
    pub fn new(api_key: Secret, endpoint: &str, options: RealtimeOptions) -> Result<Self> {
        if api_key.value().is_empty() {
            return Err(ProviderError::message("An ElevenLabs API key is required."));
        }
        let parsed = url::Url::parse(endpoint).map_err(|_| {
            ProviderError::message(
                "The ElevenLabs realtime endpoint must be a ws:// or wss:// URL.",
            )
        })?;
        if !matches!(parsed.scheme(), "ws" | "wss") || parsed.host_str().is_none() {
            return Err(ProviderError::message(
                "The ElevenLabs realtime endpoint must be a ws:// or wss:// URL.",
            ));
        }
        if endpoint.split_once("://").is_some_and(|(_, rest)| {
            rest.split(['/', '?', '#'])
                .next()
                .is_some_and(|authority| authority.contains('@'))
        }) {
            return Err(ProviderError::message(
                "The ElevenLabs realtime endpoint cannot contain credentials.",
            ));
        }
        if options.session_timeout.is_zero() || options.finalization_timeout.is_zero() {
            return Err(ProviderError::message(
                "Realtime timeouts must be positive.",
            ));
        }
        if options.maximum_queued_chunks == 0 {
            return Err(ProviderError::message(
                "The realtime audio queue must contain at least one chunk.",
            ));
        }
        Ok(Self {
            api_key,
            endpoint: endpoint.into(),
            options,
        })
    }

    pub async fn start(
        &self,
        language: &str,
        on_preview: Option<PreviewCallback>,
        on_committed: Option<CommittedCallback>,
    ) -> Result<RealtimeSession> {
        let unreachable = || {
            ProviderError::message("ElevenLabs realtime transcription could not reach the service.")
        };
        let uri = realtime_uri(&self.endpoint, language)?;
        if url::Url::parse(&uri)
            .ok()
            .is_some_and(|url| url.fragment().is_some_and(|fragment| !fragment.is_empty()))
        {
            return Err(unreachable());
        }
        let mut request = uri.into_client_request().map_err(|_| unreachable())?;
        request.headers_mut().insert(
            "xi-api-key",
            transport::secret_header(&self.api_key).map_err(|_| unreachable())?,
        );
        request
            .headers_mut()
            .insert("User-Agent", USER_AGENT.parse().unwrap());
        let config = WebSocketConfig::default()
            .max_message_size(Some(MAX_EVENT_BYTES))
            .max_frame_size(Some(MAX_EVENT_BYTES));
        let (mut connection, _) = tokio::time::timeout(
            self.options.session_timeout,
            tokio_tungstenite::connect_async_with_config(request, Some(config), false),
        )
        .await
        .map_err(|_| unreachable())?
        .map_err(|_| unreachable())?;
        let ready = tokio::time::timeout(
            self.options.session_timeout,
            receive_message(&mut connection),
        )
        .await;
        let event = match ready {
            Ok(Ok(Some(message))) => decode_event(&message),
            _ => Err(ProviderError::message(NOT_READY)),
        };
        let identifier = event.and_then(|event| {
            if event.get("message_type").and_then(Value::as_str) != Some("session_started") {
                return Err(ProviderError::message(
                    event_error(&event).unwrap_or(NOT_READY),
                ));
            }
            event
                .get("session_id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| {
                    ProviderError::message(
                        "ElevenLabs returned an invalid realtime session response.",
                    )
                })
        });
        let identifier = match identifier {
            Ok(identifier) => identifier,
            Err(error) => {
                let _ = tokio::time::timeout(Duration::from_secs(2), connection.close(None)).await;
                return Err(error);
            }
        };
        Ok(RealtimeSession::start(
            connection,
            language.into(),
            identifier,
            self.options.clone(),
            on_preview,
            on_committed,
        ))
    }
}

async fn receive_message(connection: &mut Connection) -> std::result::Result<Option<Vec<u8>>, ()> {
    while let Some(message) = connection.next().await {
        match message.map_err(|_| ())? {
            Message::Text(text) => return Ok(Some(text.as_bytes().to_vec())),
            Message::Binary(bytes) => return Ok(Some(bytes.to_vec())),
            Message::Close(_) => return Ok(None),
            _ => {}
        }
    }
    Ok(None)
}

enum Audio {
    Frames(Vec<u8>),
    Finish,
}
#[derive(Default)]
struct Policy {
    segments: Vec<String>,
    volatile: String,
    detected_language: Option<String>,
    failure: Option<String>,
    finishing: bool,
    cancelled: bool,
    closing: bool,
    bytes_sent: usize,
    bytes_since_commit: usize,
    commits_sent: usize,
    commits_dispatched: usize,
    final_target: Option<usize>,
    final_ready: bool,
    sender_done: bool,
}
struct Shared {
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
    fn fail(&self, message: &str) {
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
            committed_text: join_segments(state.segments.iter().map(String::as_str)),
            volatile_text: state.volatile.clone(),
        }
    }
    fn expected_close(&self) -> bool {
        let state = self.policy.lock().unwrap();
        state.cancelled || state.closing || state.final_ready
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

    async fn event(self: &Arc<Self>, event: Value) {
        let Some(kind) = event.get("message_type").and_then(Value::as_str) else {
            self.fail(INVALID_EVENT);
            return;
        };
        if let Some(error) = event_error(&event) {
            self.fail(error);
            return;
        }
        match kind {
            "partial_transcript" | "final_transcript" => {
                let Some(text) = event.get("text").and_then(Value::as_str) else {
                    self.fail(INVALID_EVENT);
                    return;
                };
                {
                    let mut state = self.policy.lock().unwrap();
                    if state.cancelled || state.closing {
                        return;
                    }
                    state.volatile = trim(text).into();
                }
                self.preview().await;
            }
            "committed_transcript" => {
                let Some(text) = event.get("text").and_then(Value::as_str) else {
                    self.fail(INVALID_EVENT);
                    return;
                };
                let text = trim(text);
                let segment = {
                    let mut state = self.policy.lock().unwrap();
                    if state.cancelled || state.closing {
                        return;
                    }
                    state.volatile.clear();
                    if text.is_empty() {
                        None
                    } else {
                        let sequence = state.segments.len();
                        state.segments.push(text.into());
                        Some(RealtimeCommittedSegment {
                            identifier: format!("{}-{sequence}", self.identifier),
                            sequence,
                            text: text.into(),
                        })
                    }
                };
                if let Some(segment) = segment {
                    self.committed(segment).await;
                }
                {
                    let mut state = self.policy.lock().unwrap();
                    state.commits_dispatched += 1;
                    if state
                        .final_target
                        .is_some_and(|target| state.commits_dispatched >= target)
                    {
                        state.final_ready = true;
                    }
                }
                self.changed.notify_waiters();
                self.preview().await;
            }
            "final_transcript_with_timestamps" | "committed_transcript_with_timestamps" => {
                if let Some(language) = event
                    .get("language_code")
                    .and_then(Value::as_str)
                    .filter(|language| !language.is_empty())
                {
                    let mut state = self.policy.lock().unwrap();
                    if !state.cancelled && !state.closing {
                        state.detected_language = Some(language.into());
                    }
                }
            }
            _ => {}
        }
    }
}

struct Workers {
    sender: JoinHandle<Sender>,
    receiver: JoinHandle<()>,
}
pub struct RealtimeSession {
    shared: Arc<Shared>,
    audio: mpsc::Sender<Audio>,
    workers: Arc<Mutex<Option<Workers>>>,
    options: RealtimeOptions,
    runtime: tokio::runtime::Handle,
}

impl RealtimeSession {
    fn start(
        connection: Connection,
        language: String,
        identifier: String,
        options: RealtimeOptions,
        on_preview: Option<PreviewCallback>,
        on_committed: Option<CommittedCallback>,
    ) -> Self {
        let shared = Arc::new(Shared {
            policy: Mutex::new(Policy::default()),
            changed: Notify::new(),
            close: CancellationToken::new(),
            closed: CancellationToken::new(),
            language,
            identifier,
            preview: Mutex::new(on_preview),
            committed: Mutex::new(on_committed),
        });
        let (audio, queue) = mpsc::channel(options.maximum_queued_chunks);
        let (sender, mut receiver) = connection.split();
        let send_state = shared.clone();
        let sender = tokio::spawn(send_audio(sender, queue, send_state));
        let receive_state = shared.clone();
        let receiver = tokio::spawn(async move {
            loop {
                let message = tokio::select! {biased;_ =receive_state.close.cancelled()=>break,value=receiver.next()=>value};
                let event = match message {
                    Some(Ok(Message::Text(text))) => decode_event(text.as_bytes()),
                    Some(Ok(Message::Binary(bytes))) => decode_event(&bytes),
                    Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_))) => continue,
                    _ => {
                        if !receive_state.expected_close() {
                            receive_state.fail("The ElevenLabs realtime connection closed before transcription completed.");
                        }
                        break;
                    }
                };
                match event {
                    Ok(event) => receive_state.event(event).await,
                    Err(_) => {
                        if !receive_state.expected_close() {
                            receive_state.fail("The ElevenLabs realtime connection closed before transcription completed.");
                        }
                        break;
                    }
                }
                let state = receive_state.policy.lock().unwrap();
                if state.cancelled || state.closing {
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

    pub async fn finish(&self) -> Result<RealtimeSessionResult> {
        let started = Instant::now();
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
        if self.shared.failure().is_none()
            && !self.wait_for(deadline, |state| state.final_ready).await
        {
            self.shared
                .fail("ElevenLabs did not return a committed realtime transcript in time.");
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
            TranscriptionResult {
                text: join_segments(state.segments.iter().map(String::as_str)),
                language_code: state.detected_language.clone().unwrap_or_else(|| {
                    if self.shared.language == "auto" {
                        "und".into()
                    } else {
                        self.shared.language.clone()
                    }
                }),
                language_probability: None,
                transcription_id: Some(self.shared.identifier.clone()),
                speaker_segments: vec![],
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
                if state.cancelled {
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
        // Cleanup outlives a dropped finishing future. Only the runtime-owned
        // closer may take the transport tasks and acknowledge their completion.
        self.runtime
            .spawn(close_workers(self.shared.clone(), self.workers.clone()));
    }

    /// Capture owners await this before releasing a cancelled session. The
    /// nonblocking `cancel` entry point remains available to Drop and callbacks.
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

impl Drop for RealtimeSession {
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
    loop {
        let chunk =
            tokio::select! {biased;_=shared.close.cancelled()=>break,value=queue.recv()=>value};
        let Some(chunk) = chunk else {
            break;
        };
        let (frames, finishing) = match chunk {
            Audio::Frames(frames) => (frames, false),
            Audio::Finish => (vec![], true),
        };
        let commit = if finishing {
            let mut state = shared.policy.lock().unwrap();
            if state.cancelled {
                break;
            }
            if state.bytes_since_commit == 0 && state.commits_sent > 0 {
                state.final_target = Some(state.commits_sent);
                if state.commits_dispatched >= state.commits_sent {
                    state.final_ready = true;
                }
                drop(state);
                shared.changed.notify_waiters();
                break;
            }
            state.commits_sent += 1;
            state.final_target = Some(state.commits_sent);
            true
        } else {
            if shared.policy.lock().unwrap().cancelled {
                break;
            }
            false
        };
        let message = Message::Text(audio_event(&frames, commit).into());
        let sent = tokio::select! {biased;_=shared.close.cancelled()=>break,result=sender.send(message)=>result};
        if sent.is_err() {
            shared.fail("The ElevenLabs realtime connection closed while sending audio.");
            break;
        }
        if finishing {
            break;
        }
        let should_commit = {
            let mut state = shared.policy.lock().unwrap();
            state.bytes_sent += frames.len();
            if state.cancelled {
                break;
            }
            state.bytes_since_commit += frames.len();
            if state.bytes_since_commit >= MANUAL_COMMIT_INTERVAL_BYTES {
                state.bytes_since_commit = 0;
                state.commits_sent += 1;
                true
            } else {
                false
            }
        };
        if should_commit {
            let message = Message::Text(audio_event(&[], true).into());
            let sent = tokio::select! {biased;_=shared.close.cancelled()=>break,result=sender.send(message)=>result};
            if sent.is_err() {
                shared.fail("The ElevenLabs realtime connection closed while sending audio.");
                break;
            }
        }
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

pub fn audio_event(frames: &[u8], commit: bool) -> String {
    let mut payload =
        json!({"message_type":"input_audio_chunk","audio_base_64":STANDARD.encode(frames)});
    if commit {
        payload["commit"] = json!(true);
    }
    serde_json::to_string(&payload).unwrap()
}
pub fn decode_event(message: &[u8]) -> Result<Value> {
    let value: Value =
        serde_json::from_slice(message).map_err(|_| ProviderError::message(INVALID_EVENT))?;
    if !value.is_object() {
        return Err(ProviderError::message(INVALID_EVENT));
    }
    Ok(value)
}
pub fn event_error(event: &Value) -> Option<&'static str> {
    let kind = event.get("message_type").and_then(Value::as_str)?;
    if matches!(kind, "auth_error" | "scribe_auth_error") {
        Some("ElevenLabs rejected realtime transcription authentication.")
    } else if matches!(kind, "rate_limited" | "quota_exceeded") {
        Some("ElevenLabs realtime transcription is rate limited.")
    } else if kind.ends_with("_error") || event.get("error").is_some_and(Value::is_string) {
        Some("ElevenLabs realtime transcription failed.")
    } else {
        None
    }
}
fn join_segments<'a>(segments: impl IntoIterator<Item = &'a str>) -> String {
    segments
        .into_iter()
        .map(trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn realtime_uri(endpoint: &str, language: &str) -> Result<String> {
    let mut url = url::Url::parse(endpoint).map_err(|_| {
        ProviderError::message("The ElevenLabs realtime endpoint must be a ws:// or wss:// URL.")
    })?;
    let mut pairs = Vec::<(String, String)>::new();
    let mut positions = HashMap::new();
    for (key, value) in url.query_pairs() {
        if let Some(&index) = positions.get(key.as_ref()) {
            pairs[index] = (key.into_owned(), value.into_owned());
        } else {
            positions.insert(key.to_string(), pairs.len());
            pairs.push((key.into_owned(), value.into_owned()));
        }
    }
    for (key, value) in [
        ("model_id", SCRIBE_REALTIME_MODEL),
        ("audio_format", "pcm_16000"),
        ("commit_strategy", "manual"),
    ] {
        if let Some(&index) = positions.get(key) {
            pairs[index].1 = value.into();
        } else {
            positions.insert(key.into(), pairs.len());
            pairs.push((key.into(), value.into()));
        }
    }
    let (remove, key, value) = if language == "auto" {
        ("language_code", "include_language_detection", "true")
    } else {
        ("include_language_detection", "language_code", language)
    };
    pairs.retain(|(key, _)| key != remove);
    if let Some((_, existing)) = pairs.iter_mut().find(|(existing, _)| existing == key) {
        *existing = value.into();
    } else {
        pairs.push((key.into(), value.into()));
    }
    url.set_query(Some(
        &url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(
                pairs
                    .iter()
                    .map(|(key, value)| (key.as_str(), value.as_str())),
            )
            .finish(),
    ));
    Ok(url.into())
}
