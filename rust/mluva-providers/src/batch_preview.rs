//! Sequential batch previews; only full-audio recognition may become final text.

use crate::{
    ProviderError, Result,
    realtime::{RealtimePreview, RealtimeSessionResult},
    speech::SpeechClient,
};
use futures_util::future::BoxFuture;
use mluva_core::{private_files::atomic_write_private, text};
use std::{
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{sync::Notify, task::JoinHandle};
use tokio_util::sync::CancellationToken;

pub const BYTES_PER_SECOND: usize = 32_000;
pub const MAX_PREVIEW_BYTES: usize = 30 * 60 * BYTES_PER_SECOND;
pub type PreviewSpeechFactory =
    Arc<dyn Fn() -> BoxFuture<'static, Result<Arc<SpeechClient>>> + Send + Sync>;

#[derive(Clone)]
pub struct BatchPreviewClient {
    pub factory: PreviewSpeechFactory,
    pub directory: PathBuf,
    pub chunk_seconds: u32,
    pub preview_enabled: bool,
}

impl BatchPreviewClient {
    /// Starting a session creates no provider, model, file or microphone.
    pub fn start(&self, language: &str) -> BatchPreviewSession {
        let shared = Arc::new(Shared {
            factory: self.factory.clone(),
            directory: self.directory.clone(),
            language: language.into(),
            chunk_bytes: self.chunk_seconds as usize * BYTES_PER_SECOND,
            state: Mutex::new(State {
                audio: vec![],
                offset: 0,
                text: String::new(),
                healthy: true,
                finishing: false,
                enabled: self.preview_enabled,
                active: None,
            }),
            ready: Notify::new(),
            cancelled: CancellationToken::new(),
            stop_preview: CancellationToken::new(),
            worker_done: CancellationToken::new(),
            closed: CancellationToken::new(),
            closing: AtomicBool::new(false),
            requests: AtomicUsize::new(0),
            requests_done: Notify::new(),
            worker: Mutex::new(None),
            runtime: tokio::runtime::Handle::current(),
        });
        let done = WorkerDone(shared.worker_done.clone());
        *shared.worker.lock().unwrap() = Some(shared.runtime.spawn(preview(shared.clone(), done)));
        BatchPreviewSession { shared }
    }
}

struct State {
    audio: Vec<u8>,
    offset: usize,
    text: String,
    healthy: bool,
    finishing: bool,
    enabled: bool,
    active: Option<Arc<SpeechClient>>,
}
struct Shared {
    factory: PreviewSpeechFactory,
    directory: PathBuf,
    language: String,
    chunk_bytes: usize,
    state: Mutex<State>,
    ready: Notify,
    cancelled: CancellationToken,
    stop_preview: CancellationToken,
    worker_done: CancellationToken,
    closed: CancellationToken,
    closing: AtomicBool,
    requests: AtomicUsize,
    requests_done: Notify,
    worker: Mutex<Option<JoinHandle<()>>>,
    runtime: tokio::runtime::Handle,
}

pub struct BatchPreviewSession {
    shared: Arc<Shared>,
}

impl BatchPreviewSession {
    pub fn bytes_sent(&self) -> usize {
        self.shared.state.lock().unwrap().offset
    }
    pub fn is_healthy(&self) -> bool {
        self.shared.state.lock().unwrap().healthy && !self.shared.cancelled.is_cancelled()
    }
    pub fn snapshot(&self) -> RealtimePreview {
        RealtimePreview {
            committed_text: self.shared.state.lock().unwrap().text.clone(),
            volatile_text: String::new(),
        }
    }
    pub fn submit_audio(&self, frames: &[u8]) {
        let mut state = self.shared.state.lock().unwrap();
        if !state.healthy || state.finishing || self.shared.cancelled.is_cancelled() {
            return;
        }
        if frames.len() > MAX_PREVIEW_BYTES - state.audio.len() {
            state.healthy = false;
            state.audio = vec![];
            self.shared.ready.notify_one();
            return;
        }
        state.audio.extend_from_slice(frames);
        if state.enabled
            && state.audio.len().saturating_sub(state.offset) >= self.shared.chunk_bytes
        {
            self.shared.ready.notify_one();
        }
    }
    pub fn set_preview_enabled(&self, enabled: bool) {
        let mut state = self.shared.state.lock().unwrap();
        state.enabled = enabled;
        if enabled && state.audio.len().saturating_sub(state.offset) >= self.shared.chunk_bytes {
            self.shared.ready.notify_one();
        }
    }
    pub async fn finish(&self) -> Result<RealtimeSessionResult> {
        let started = Instant::now();
        let frames = {
            let mut state = self.shared.state.lock().unwrap();
            if state.finishing {
                return Err(ProviderError::message(
                    "Preview unavailable; use the finalized recording.",
                ));
            }
            state.finishing = true;
            std::mem::take(&mut state.audio)
        };
        let mut guard = Finishing {
            shared: self.shared.clone(),
            complete: false,
        };
        self.shared.stop_preview.cancel();
        self.shared.ready.notify_one();
        let active = self.shared.state.lock().unwrap().active.clone();
        if let Some(active) = active {
            active.cancel().await;
        }
        self.shared.reap_worker().await;
        self.shared.wait_requests().await;
        let result = if !self.is_healthy() || frames.is_empty() {
            Err(ProviderError::message(
                "Preview unavailable; use the finalized recording.",
            ))
        } else {
            transcribe(self.shared.clone(), frames, false).await
        };
        guard.complete = true;
        result.map(|transcription| RealtimeSessionResult {
            transcription,
            finalization_seconds: started.elapsed().as_secs_f64(),
        })
    }
    /// Invalidate audio/text immediately; process cleanup stays off the caller.
    pub fn cancel(&self) {
        self.shared.cancel();
    }
    pub async fn cancel_and_wait(&self) {
        self.cancel();
        self.shared.closed.cancelled().await;
    }
}
impl Drop for BatchPreviewSession {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl Shared {
    fn cancel(self: &Arc<Self>) {
        self.cancelled.cancel();
        self.stop_preview.cancel();
        {
            let mut state = self.state.lock().unwrap();
            state.audio = vec![];
            state.text.clear();
        }
        self.ready.notify_one();
        if !self.closing.swap(true, Ordering::AcqRel) {
            let shared = self.clone();
            self.runtime.spawn(async move {
                let active = shared.state.lock().unwrap().active.clone();
                if let Some(active) = active {
                    active.cancel().await;
                }
                shared.reap_worker().await;
                // A final request may be active after the preview worker ends.
                let active = shared.state.lock().unwrap().active.clone();
                if let Some(active) = active {
                    active.cancel().await;
                }
                shared.wait_requests().await;
                shared.closed.cancel();
            });
        }
    }
    async fn reap_worker(&self) {
        let worker = self.worker.lock().unwrap().take();
        if let Some(mut worker) = worker {
            if tokio::time::timeout(Duration::from_secs(2), &mut worker)
                .await
                .is_err()
            {
                worker.abort();
                let _ = worker.await;
            }
        } else {
            self.worker_done.cancelled().await;
        }
    }

    async fn wait_requests(&self) {
        loop {
            let notification = self.requests_done.notified();
            tokio::pin!(notification);
            notification.as_mut().enable();
            if self.requests.load(Ordering::Acquire) == 0 {
                return;
            }
            notification.await;
        }
    }
}

struct Finishing {
    shared: Arc<Shared>,
    complete: bool,
}
impl Drop for Finishing {
    fn drop(&mut self) {
        if !self.complete {
            self.shared.cancel();
        }
    }
}
struct WorkerDone(CancellationToken);
impl Drop for WorkerDone {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

async fn preview(shared: Arc<Shared>, _finished: WorkerDone) {
    loop {
        tokio::select! { biased;
            _ = shared.stop_preview.cancelled() => return,
            _ = shared.ready.notified() => {},
        }
        let frames = {
            let mut state = shared.state.lock().unwrap();
            if !state.healthy || state.finishing || shared.cancelled.is_cancelled() {
                return;
            }
            if !state.enabled || state.audio.len() - state.offset < shared.chunk_bytes {
                continue;
            }
            let frames = state.audio[state.offset..].to_vec();
            state.offset += frames.len();
            frames
        };
        let result = transcribe(shared.clone(), frames, true).await;
        let mut state = shared.state.lock().unwrap();
        if shared.cancelled.is_cancelled() || state.finishing {
            return;
        }
        match result {
            Ok(result) => {
                state.text = text::trim(&format!("{} {}", state.text, result.text)).into()
            }
            Err(_) => {
                state.healthy = false;
                return;
            }
        }
        if state.enabled && state.audio.len() - state.offset >= shared.chunk_bytes {
            shared.ready.notify_one();
        }
    }
}

async fn transcribe(
    shared: Arc<Shared>,
    frames: Vec<u8>,
    preview: bool,
) -> Result<crate::TranscriptionResult> {
    // A dropped caller cannot abandon blocking staging or provider cleanup.
    // Both preview and final requests remain owned until their private files
    // and client are released; cancellation acknowledgements await that owner.
    let (reply, response) = tokio::sync::oneshot::channel();
    {
        // Cancellation takes this same lock before starting its cleanup owner.
        // Register before it can acknowledge zero requests, or refuse new work.
        let _state = shared.state.lock().unwrap();
        if shared.cancelled.is_cancelled() || (preview && shared.stop_preview.is_cancelled()) {
            return Err(ProviderError::message("Transcription cancelled."));
        }
        shared.requests.fetch_add(1, Ordering::AcqRel);
    }
    shared.runtime.spawn({
        let shared = shared.clone();
        async move {
            let _done = RequestDone(shared.clone());
            let result = transcribe_owned(shared, frames, preview).await;
            let _ = reply.send(result);
        }
    });
    response
        .await
        .map_err(|_| ProviderError::message("Transcription cancelled."))?
}

struct RequestDone(Arc<Shared>);
impl Drop for RequestDone {
    fn drop(&mut self) {
        self.0.requests.fetch_sub(1, Ordering::AcqRel);
        self.0.requests_done.notify_waiters();
    }
}

async fn transcribe_owned(
    shared: Arc<Shared>,
    frames: Vec<u8>,
    preview: bool,
) -> Result<crate::TranscriptionResult> {
    let directory = shared.directory.clone();
    let (temporary, path) = tokio::task::spawn_blocking(move || -> std::io::Result<_> {
        std::fs::create_dir_all(&directory)?;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
        let temporary = tempfile::Builder::new()
            .prefix("speech-")
            .tempdir_in(directory)?;
        let path = temporary.path().join("audio.wav");
        atomic_write_private(&path, &mluva_audio::wav::pcm16_wav(&frames)?)?;
        Ok((temporary, path))
    })
    .await
    .map_err(|_| ProviderError::message("Speech preview storage failed."))?
    .map_err(|_| ProviderError::message("Speech preview storage failed."))?;
    let stop = if preview {
        &shared.stop_preview
    } else {
        &shared.cancelled
    };
    let client = tokio::select! { biased;
        _ = stop.cancelled() => return Err(ProviderError::message("Transcription cancelled.")),
        client = (shared.factory)() => client?,
    };
    shared.state.lock().unwrap().active = Some(client.clone());
    let result = tokio::select! { biased;
        _ = stop.cancelled() => Err(ProviderError::message("Transcription cancelled.")),
        result = client.transcribe(&path, &shared.language, "") => result,
    };
    client.close().await;
    shared.state.lock().unwrap().active = None;
    drop(temporary);
    if shared.cancelled.is_cancelled() {
        return Err(ProviderError::message("Transcription cancelled."));
    }
    result
}
