//! Offline ONNX requests through one lazily owned native worker per capture.
use crate::{
    ProviderError, Result, TranscriptionResult, languages, local_assets::model, multipart,
    onnx_runtime::ONNX_RUNTIME,
};
use serde_json::json;
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::{Notify, mpsc, oneshot},
};
use tokio_util::sync::CancellationToken;

const CANCELLED: &str = "Local transcription cancelled.";
const MEMORY_LIMIT: &str = "Local model exceeded 5 GB RAM. Choose a smaller model.";
const TIMEOUT: &str = "Local transcription timed out. Try a smaller model.";
const MAX_LINE_CHARACTERS: usize = 2_000_001;
const PARAKEET_LANGUAGES: [&str; 26] = [
    "auto", "bg", "hr", "cs", "da", "nl", "en", "et", "fi", "fr", "de", "el", "hu", "it", "lv",
    "lt", "mt", "pl", "pt", "ro", "sk", "sl", "es", "sv", "ru", "uk",
];

#[derive(Clone, Debug)]
pub struct OnnxOptions {
    pub data_dir: PathBuf,
    pub model: String,
    pub device: String,
    pub keep_alive: bool,
    pub worker_executable: PathBuf,
    /// The application may bundle its verified CPU library or use its native asset store.
    pub cpu_runtime: PathBuf,
}
impl OnnxOptions {
    pub fn new(
        data_dir: impl Into<PathBuf>,
        model: impl Into<String>,
        worker_executable: impl Into<PathBuf>,
    ) -> Self {
        let data_dir = data_dir.into();
        let cpu_runtime = ONNX_RUNTIME
            .library(&data_dir, "cpu")
            .expect("compiled CPU runtime library");
        Self {
            data_dir,
            worker_executable: worker_executable.into(),
            cpu_runtime,
            model: model.into(),
            device: "cpu".into(),
            keep_alive: false,
        }
    }
}
struct Request {
    bytes: Vec<u8>,
    reply: oneshot::Sender<Result<String>>,
}
struct Session {
    sender: mpsc::Sender<Request>,
    stop: CancellationToken,
    cancelled: CancellationToken,
    done: AtomicBool,
    abandoned: AtomicBool,
    changed: Notify,
    failure: Mutex<Option<ProviderError>>,
    otherwise: &'static str,
}
impl Session {
    async fn shutdown(&self) {
        self.stop.cancel();
        loop {
            let changed = self.changed.notified();
            if self.done.load(Ordering::Acquire) {
                return;
            }
            changed.await;
        }
    }
    fn error(&self) -> ProviderError {
        if self.cancelled.is_cancelled() {
            return ProviderError::message(CANCELLED);
        }
        self.failure
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| ProviderError::message(self.otherwise))
    }
}
struct StopOnDrop(Option<Arc<Session>>);
impl Drop for StopOnDrop {
    fn drop(&mut self) {
        if let Some(session) = &self.0 {
            session.abandoned.store(true, Ordering::Release);
            session.stop.cancel();
        }
    }
}

pub struct OnnxSpeechClient {
    options: OnnxOptions,
    cancelled: CancellationToken,
    current: Mutex<Option<Arc<Session>>>,
    lifecycle: tokio::sync::Mutex<()>,
    transcription: tokio::sync::Mutex<()>,
}
impl OnnxSpeechClient {
    pub fn new(options: OnnxOptions) -> Self {
        Self {
            options,
            cancelled: CancellationToken::new(),
            current: Mutex::new(None),
            lifecycle: tokio::sync::Mutex::new(()),
            transcription: tokio::sync::Mutex::new(()),
        }
    }
    fn failed(&self) -> &'static str {
        if self.options.device == "cuda" {
            "GPU transcription failed. Try CPU or reinstall GPU support."
        } else {
            "Local transcription failed. Try a smaller model or download it again."
        }
    }
    pub async fn close(&self) {
        let _lifecycle = self.lifecycle.lock().await;
        let session = self.current.lock().unwrap().clone();
        if let Some(session) = session {
            session.shutdown().await;
            *self.current.lock().unwrap() = None;
        }
    }
    /// Cancellation is immediate; the returned future waits for all worker resources to be released.
    pub fn cancel(&self) -> impl std::future::Future<Output = ()> + '_ {
        self.cancelled.cancel();
        self.close()
    }
    async fn connection(&self) -> Result<Arc<Session>> {
        let _lifecycle = self.lifecycle.lock().await;
        let previous = self.current.lock().unwrap().clone();
        if let Some(session) = previous {
            if !session.abandoned.load(Ordering::Acquire) {
                return Ok(session);
            }
            session.shutdown().await;
            *self.current.lock().unwrap() = None;
        }
        if self.cancelled.is_cancelled() {
            return Err(ProviderError::message(CANCELLED));
        }
        let spec = model(&self.options.model)?;
        let library = if self.options.device == "cuda" {
            ONNX_RUNTIME.library(&self.options.data_dir, "cuda")?
        } else {
            self.options.cpu_runtime.clone()
        };
        let mut command = Command::new(&self.options.worker_executable);
        command
            .arg(library)
            .arg(&self.options.model)
            .arg(spec.path(&self.options.data_dir))
            .arg(&self.options.device)
            .env_clear()
            .env("HF_HUB_OFFLINE", "1")
            .env("TRANSFORMERS_OFFLINE", "1")
            .env("OMP_NUM_THREADS", "1")
            .env("OPENBLAS_NUM_THREADS", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        for key in ["PATH", "LANG", "SYSTEMROOT"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        let temporary = if self.options.device == "cuda" {
            let temporary = tempfile::Builder::new()
                .prefix("mluva-onnx-")
                .permissions(fs::Permissions::from_mode(0o700))
                .tempdir()
                .map_err(|_| ProviderError::message(self.failed()))?;
            let cache = temporary.path().join("cuda");
            fs::DirBuilder::new()
                .mode(0o700)
                .create(&cache)
                .map_err(|_| ProviderError::message(self.failed()))?;
            command.env("CUDA_CACHE_PATH", cache).env(
                "LD_LIBRARY_PATH",
                ONNX_RUNTIME
                    .root(&self.options.data_dir, "cuda")
                    .join("lib"),
            );
            Some(temporary)
        } else {
            None
        };
        let mut child = command
            .spawn()
            .map_err(|_| ProviderError::message(self.failed()))?;
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        let (sender, receiver) = mpsc::channel(1);
        let session = Arc::new(Session {
            sender,
            stop: self.cancelled.child_token(),
            cancelled: self.cancelled.clone(),
            done: AtomicBool::new(false),
            abandoned: AtomicBool::new(false),
            changed: Notify::new(),
            failure: Mutex::new(None),
            otherwise: self.failed(),
        });
        *self.current.lock().unwrap() = Some(session.clone());
        tokio::spawn(lifecycle(
            child,
            input,
            output,
            receiver,
            temporary,
            session.clone(),
        ));
        Ok(session)
    }
    pub async fn transcribe(
        &self,
        file_path: &Path,
        language_code: &str,
    ) -> Result<TranscriptionResult> {
        let language = languages::iso(language_code);
        // Released preflight failures leave an existing retained worker alone.
        if self.options.model == "parakeet-v3" && !PARAKEET_LANGUAGES.contains(&language) {
            return Err(ProviderError::message(
                "Parakeet does not support this language. Choose a multilingual Whisper model.",
            ));
        }
        if self.options.device == "cuda" && !ONNX_RUNTIME.ready(&self.options.data_dir, "cuda") {
            return Err(ProviderError::message(
                "Download GPU support in Settings → Providers first, or choose CPU.",
            ));
        }
        if !model(&self.options.model)?.ready(&self.options.data_dir) {
            return Err(ProviderError::message(
                "Download your local model in Settings → Providers first.",
            ));
        }
        if self.cancelled.is_cancelled() {
            return Err(ProviderError::message(CANCELLED));
        }
        let _job = self.transcription.lock().await;
        let mut owned = StopOnDrop(None);
        let result = async {
            let session = self.connection().await?;
            owned.0 = Some(session.clone());
            let path = mluva_core::private_files::resolve_path(file_path)
                .map_err(|_| ProviderError::message(self.failed()))?;
            let path = path
                .to_str()
                .ok_or_else(|| ProviderError::message(self.failed()))?;
            let mut bytes = multipart::json_body(&json!({"path":path,"language":language}));
            bytes.push(b'\n');
            let (reply, receive) = oneshot::channel();
            session
                .sender
                .send(Request { bytes, reply })
                .await
                .map_err(|_| session.error())?;
            let text = receive.await.map_err(|_| session.error())??;
            if self.cancelled.is_cancelled() {
                return Err(ProviderError::message(CANCELLED));
            }
            Ok(TranscriptionResult {
                text,
                language_code: language_code.into(),
                language_probability: None,
                transcription_id: None,
                speaker_segments: vec![],
                audio_duration_seconds: None,
            })
        }
        .await;
        if result.is_err() || !self.options.keep_alive {
            self.close().await;
        }
        owned.0 = None;
        result
    }
}
impl Drop for OnnxSpeechClient {
    fn drop(&mut self) {
        self.cancelled.cancel();
        if let Some(session) = self.current.lock().unwrap().as_ref() {
            session.stop.cancel();
        }
    }
}

fn exceeds_memory(pid: u32) -> bool {
    fs::read_to_string(format!("/proc/{pid}/status")).is_ok_and(|status| {
        status
            .lines()
            .find_map(|line| {
                line.strip_prefix("VmRSS:")
                    .and_then(|value| value.split_whitespace().next())
                    .and_then(|value| value.parse::<u64>().ok())
            })
            .is_some_and(|rss| rss.saturating_mul(1024) > 5_000_000_000)
    })
}
/// Released text streams count decoded characters and normalize CR/LF, including at EOF.
async fn response_line(
    output: &mut BufReader<ChildStdout>,
    skip_lf: &mut bool,
    session: &Session,
) -> Result<Vec<u8>> {
    let mut line = Vec::new();
    let mut characters = 0;
    let mut remaining = 0;
    loop {
        let available = output.fill_buf().await.map_err(|_| session.error())?;
        if available.is_empty() {
            break;
        }
        let mut consumed = 0;
        let mut ended = false;
        for &byte in available {
            consumed += 1;
            if *skip_lf {
                *skip_lf = false;
                if byte == b'\n' {
                    continue;
                }
            }
            if remaining == 0 {
                match byte {
                    0..=127 => {
                        characters += 1;
                        if byte == b'\r' {
                            line.push(b'\n');
                            *skip_lf = true;
                            ended = true;
                        } else {
                            line.push(byte);
                            ended = byte == b'\n';
                        }
                    }
                    0xc2..=0xdf => {
                        remaining = 1;
                        line.push(byte);
                    }
                    0xe0..=0xef => {
                        remaining = 2;
                        line.push(byte);
                    }
                    0xf0..=0xf4 => {
                        remaining = 3;
                        line.push(byte);
                    }
                    _ => return Err(session.error()),
                }
            } else {
                if !(0x80..=0xbf).contains(&byte) {
                    return Err(session.error());
                }
                line.push(byte);
                remaining -= 1;
                if remaining == 0 {
                    characters += 1;
                }
            }
            if ended || characters == MAX_LINE_CHARACTERS {
                break;
            }
        }
        output.consume(consumed);
        if ended || characters == MAX_LINE_CHARACTERS {
            break;
        }
    }
    std::str::from_utf8(&line).map_err(|_| session.error())?;
    Ok(line)
}

/// The released JSON decoder accepts nonfinite values in unused metadata; text stays a string.
fn response_text(bytes: &[u8]) -> serde_json::Result<String> {
    let mut normalized = Vec::with_capacity(bytes.len());
    let mut quoted = false;
    let mut escaped = false;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if quoted {
            normalized.push(byte);
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
            index += 1;
            continue;
        }
        if byte == b'"' {
            quoted = true;
        }
        if let Some(token) = [b"-Infinity".as_slice(), b"Infinity", b"NaN"]
            .into_iter()
            .find(|token| bytes[index..].starts_with(token))
        {
            normalized.extend_from_slice(b"null");
            index += token.len();
        } else {
            normalized.push(byte);
            index += 1;
        }
    }
    struct Reply(String);
    impl<'de> serde::Deserialize<'de> for Reply {
        fn deserialize<D: serde::Deserializer<'de>>(
            decoder: D,
        ) -> std::result::Result<Self, D::Error> {
            struct Visitor;
            impl<'de> serde::de::Visitor<'de> for Visitor {
                type Value = Reply;
                fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                    formatter.write_str("a local speech reply containing string text")
                }
                fn visit_map<M: serde::de::MapAccess<'de>>(
                    self,
                    mut map: M,
                ) -> std::result::Result<Reply, M::Error> {
                    let mut text = None;
                    while let Some(key) = map.next_key::<String>()? {
                        if key == "text" {
                            text = map
                                .next_value::<serde_json::Value>()?
                                .as_str()
                                .map(str::to_owned);
                        } else {
                            map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                    text.map(Reply).ok_or_else(|| {
                        serde::de::Error::custom("local speech reply has no string text")
                    })
                }
            }
            decoder.deserialize_map(Visitor)
        }
    }
    serde_json::from_slice::<Reply>(&normalized).map(|reply| reply.0)
}
async fn request(
    input: &mut ChildStdin,
    output: &mut BufReader<ChildStdout>,
    skip_lf: &mut bool,
    bytes: &[u8],
    pid: u32,
    session: &Session,
) -> Result<String> {
    let exchange = async {
        input.write_all(bytes).await.map_err(|_| session.error())?;
        input.flush().await.map_err(|_| session.error())?;
        let line = response_line(output, skip_lf, session).await?;
        response_text(&line).map_err(|_| session.error())
    };
    tokio::pin!(exchange);
    let mut monitor = tokio::time::interval(Duration::from_millis(100));
    monitor.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let deadline = tokio::time::sleep(Duration::from_secs(180));
    tokio::pin!(deadline);
    loop {
        tokio::select! { biased;
            _=session.stop.cancelled()=>return Err(session.error()),
            _=monitor.tick()=>if exceeds_memory(pid) { return Err(ProviderError::message(MEMORY_LIMIT)); },
            _=&mut deadline=>return Err(ProviderError::message(TIMEOUT)),
            result=&mut exchange=>return result,
        }
    }
}
async fn lifecycle(
    mut child: Child,
    mut input: ChildStdin,
    mut output: BufReader<ChildStdout>,
    mut requests: mpsc::Receiver<Request>,
    temporary: Option<tempfile::TempDir>,
    session: Arc<Session>,
) {
    let pid = child.id().unwrap();
    let mut skip_lf = false;
    let mut monitor = tokio::time::interval(Duration::from_millis(100));
    monitor.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! { biased;
            _=session.stop.cancelled()=>break,
            _=child.wait()=>break,
            _=monitor.tick()=>if exceeds_memory(pid) { *session.failure.lock().unwrap()=Some(ProviderError::message(MEMORY_LIMIT)); break; },
            value=requests.recv()=>{
                let Some(value)=value else {break;};
                let result=request(&mut input,&mut output,&mut skip_lf,&value.bytes,pid,&session).await;
                if let Err(error)=&result { *session.failure.lock().unwrap()=Some(error.clone()); }
                let failed=result.is_err();
                let _=value.reply.send(result);
                if failed {break;}
            },
        }
    }
    let _ = child.start_kill();
    let _ = child.wait().await;
    drop(input);
    drop(output);
    drop(temporary);
    session.stop.cancel();
    session.done.store(true, Ordering::Release);
    session.changed.notify_waiters();
}
