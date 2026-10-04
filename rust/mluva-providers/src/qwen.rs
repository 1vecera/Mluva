//! On-demand Qwen recognition through an owned, authenticated loopback process.
use crate::local_assets::{QWEN_RUNTIME, model};
use crate::{
    ProviderError, Result, Secret, TranscriptionResult, languages, models, multipart, transport,
};
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use mluva_audio::wav::{WaveFormatError, WaveReader, pcm16_wav};
use mluva_core::text::trim;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use tokio::process::{Child, Command};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

const CANCELLED: &str = "Local transcription cancelled.";
const FAILED: &str = "Qwen transcription failed. Try CPU or a smaller model.";
const START_FAILED: &str = "Qwen could not start. Try CPU or download the runtime again.";
const OUTPUT_LIMIT: &str = "Local transcription returned too much data.";
const MEMORY_LIMIT: &str = "Local model exceeded 5 GB RAM. Choose a smaller model.";
const LANGUAGE_NAMES: [&str; 30] = [
    "English",
    "Chinese",
    "Cantonese",
    "Arabic",
    "German",
    "French",
    "Spanish",
    "Portuguese",
    "Indonesian",
    "Italian",
    "Korean",
    "Russian",
    "Thai",
    "Vietnamese",
    "Japanese",
    "Turkish",
    "Hindi",
    "Malay",
    "Dutch",
    "Swedish",
    "Danish",
    "Finnish",
    "Polish",
    "Czech",
    "Filipino",
    "Persian",
    "Greek",
    "Hungarian",
    "Macedonian",
    "Romanian",
];

#[derive(Clone, Debug)]
pub struct QwenOptions {
    pub data_dir: PathBuf,
    pub model: String,
    pub device: String,
    pub keep_alive: bool,
}
impl QwenOptions {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            model: "qwen3-1.7b".into(),
            device: "cpu".into(),
            keep_alive: false,
        }
    }
}

struct Session {
    pid: u32,
    url: String,
    token: Mutex<Option<Secret>>,
    failure: Mutex<Option<String>>,
    stop: CancellationToken,
    done: AtomicBool,
    changed: Notify,
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
    fn failure(&self, otherwise: &str) -> ProviderError {
        ProviderError(
            self.failure
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| otherwise.into()),
        )
    }
}
struct StopOnDrop(Option<Arc<Session>>);
impl Drop for StopOnDrop {
    fn drop(&mut self) {
        if let Some(session) = &self.0 {
            session.stop.cancel();
        }
    }
}

pub struct QwenSpeechClient {
    options: QwenOptions,
    http: reqwest::Client,
    cancelled: CancellationToken,
    current: Mutex<Option<Arc<Session>>>,
    lifecycle: tokio::sync::Mutex<()>,
    transcription: tokio::sync::Mutex<()>,
}
impl QwenSpeechClient {
    pub fn new(options: QwenOptions) -> Result<Self> {
        let http = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(180))
            .read_timeout(Duration::from_secs(180))
            .build()
            .map_err(|_| ProviderError::message(FAILED))?;
        Ok(Self {
            options,
            http,
            cancelled: CancellationToken::new(),
            current: Mutex::new(None),
            lifecycle: tokio::sync::Mutex::new(()),
            transcription: tokio::sync::Mutex::new(()),
        })
    }
    /// Reap the owned runtime and remove its key before returning; a later job may restart it.
    pub async fn close(&self) {
        let _lifecycle = self.lifecycle.lock().await;
        let session = self.current.lock().unwrap().clone();
        if let Some(session) = session {
            session.shutdown().await;
            *self.current.lock().unwrap() = None;
        }
    }
    /// Invalidate output synchronously; await the returned future to finish process cleanup.
    pub fn cancel(&self) -> impl std::future::Future<Output = ()> + '_ {
        self.cancelled.cancel();
        self.close()
    }
    async fn connection(&self) -> Result<Arc<Session>> {
        let _lifecycle = self.lifecycle.lock().await;
        if let Some(session) = self.current.lock().unwrap().clone() {
            return Ok(session);
        }
        drop(_lifecycle);
        let spec = model(&self.options.model).map_err(|_| ProviderError::message(FAILED))?;
        if !spec.ready(&self.options.data_dir)
            || !QWEN_RUNTIME.ready(&self.options.data_dir, &self.options.device)
        {
            return Err(ProviderError::message(
                "Download Qwen and its runtime in Settings → Providers first.",
            ));
        }
        if self.cancelled.is_cancelled() {
            return Err(ProviderError::message(CANCELLED));
        }
        let cache = self.options.data_dir.join("qwen-cache");
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&cache)
            .map_err(|_| ProviderError::message(FAILED))?;
        let mut environment: BTreeMap<OsString, OsString> = ["PATH", "LANG", "HOME"]
            .into_iter()
            .filter_map(|key| std::env::var_os(key).map(|value| (key.into(), value)))
            .collect();
        environment.extend([
            ("HF_HUB_OFFLINE".into(), "1".into()),
            ("XDG_CACHE_HOME".into(), cache.clone().into_os_string()),
            ("__GL_SHADER_DISK_CACHE_PATH".into(), cache.into_os_string()),
            ("OMP_NUM_THREADS".into(), "4".into()),
        ]);
        let executable = QWEN_RUNTIME.binary(&self.options.data_dir, &self.options.device);
        let target = if self.options.device == "cuda" {
            self.gpu_target(&executable, &environment).await?
        } else {
            "none".into()
        };
        let _lifecycle = self.lifecycle.lock().await;
        if let Some(session) = self.current.lock().unwrap().clone() {
            return Ok(session);
        }
        if self.cancelled.is_cancelled() {
            return Err(ProviderError::message(CANCELLED));
        }
        let temporary = tempfile::Builder::new()
            .prefix("mluva-qwen-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .map_err(|_| ProviderError::message(FAILED))?;
        let mut entropy = [0; 32];
        getrandom::fill(&mut entropy).map_err(|_| ProviderError::message(FAILED))?;
        let token = URL_SAFE_NO_PAD.encode(entropy);
        let key_path = temporary.path().join("key");
        use std::io::Write;
        let mut key = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&key_path)
            .map_err(|_| ProviderError::message(FAILED))?;
        key.write_all(token.as_bytes())
            .map_err(|_| ProviderError::message(FAILED))?;
        drop(key);
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|listener| listener.local_addr())
            .map_err(|_| ProviderError::message(FAILED))?
            .port();
        let path = spec.path(&self.options.data_dir);
        let mut command = Command::new(&executable);
        command
            .arg("-m")
            .arg(path.join("Qwen3-ASR-1.7B-Q4_0.gguf"))
            .arg("--mmproj")
            .arg(path.join("mmproj-Qwen3-ASR-1.7B-Q8_0.gguf"))
            .args([
                "--host",
                "127.0.0.1",
                "--port",
                &port.to_string(),
                "--api-key-file",
            ])
            .arg(&key_path)
            .args([
                "--no-webui",
                "--log-disable",
                "-c",
                "2048",
                "-np",
                "1",
                "-t",
                "4",
                "-tb",
                "4",
                "--fit",
                "off",
                "--device",
                &target,
                "-ngl",
                if target == "none" { "0" } else { "99" },
            ]);
        if target == "none" {
            command.args(["--no-mmproj-offload", "--no-op-offload"]);
        }
        let process = command
            .env_clear()
            .envs(environment)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| ProviderError::message(FAILED))?;
        let session = Arc::new(Session {
            pid: process.id().unwrap(),
            url: format!("http://127.0.0.1:{port}"),
            token: Mutex::new(Some(Secret::new(token))),
            failure: Mutex::new(None),
            stop: self.cancelled.child_token(),
            done: AtomicBool::new(false),
            changed: Notify::new(),
        });
        *self.current.lock().unwrap() = Some(session.clone());
        tokio::spawn(lifecycle(process, temporary, session.clone()));
        // Startup health checks must not prevent close/cancel from taking ownership.
        drop(_lifecycle);
        let mut startup = StopOnDrop(Some(session.clone()));
        let ready = async {
            loop {
                if session.stop.is_cancelled() || session.done.load(Ordering::Acquire) {
                    return Err(session.failure(START_FAILED));
                }
                let response = self
                    .http
                    .get(format!("{}/health", session.url))
                    .timeout(Duration::from_millis(500))
                    .send();
                let result = tokio::select! {
                    biased;
                    _ = session.stop.cancelled() => return Err(session.failure(START_FAILED)),
                    response = response => response,
                };
                if result.is_ok_and(|response| response.status() == reqwest::StatusCode::OK) {
                    return Ok(());
                }
                tokio::select! {
                    biased;
                    _ = session.stop.cancelled() => return Err(session.failure(START_FAILED)),
                    _ = tokio::time::sleep(Duration::from_millis(100)) => {},
                }
            }
        };
        tokio::time::timeout(Duration::from_secs(90), ready)
            .await
            .map_err(|_| {
                ProviderError::message("Qwen startup timed out. Try a smaller model.")
            })??;
        startup.0 = None;
        Ok(session)
    }
    async fn gpu_target(
        &self,
        executable: &Path,
        environment: &BTreeMap<OsString, OsString>,
    ) -> Result<String> {
        let mut process = Command::new(executable)
            .arg("--list-devices")
            .env_clear()
            .envs(environment)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| ProviderError::message(FAILED))?;
        use tokio::io::AsyncReadExt;
        let mut stdout = process.stdout.take().unwrap();
        let mut stderr = process.stderr.take().unwrap();
        let mut out = vec![];
        let mut err = vec![];
        let status = {
            let read = async {
                let (status, _, _) = tokio::try_join!(
                    process.wait(),
                    stdout.read_to_end(&mut out),
                    stderr.read_to_end(&mut err)
                )?;
                std::io::Result::Ok(status)
            };
            tokio::select! {
                biased;
                _ = self.cancelled.cancelled() => Err(ProviderError::message(CANCELLED)),
                result = tokio::time::timeout(Duration::from_secs(15), read) => result
                    .map_err(|_| ProviderError::message(FAILED))
                    .and_then(|result| result.map_err(|_| ProviderError::message(FAILED))),
            }
        };
        let status = match status {
            Ok(status) => status,
            Err(error) => {
                let _ = process.start_kill();
                let _ = process.wait().await;
                return Err(error);
            }
        };
        if !status.success() {
            return Err(ProviderError::message(FAILED));
        }
        std::str::from_utf8(&err).map_err(|_| ProviderError::message(FAILED))?;
        let text = std::str::from_utf8(&out)
            .map_err(|_| ProviderError::message(FAILED))?
            .replace("\r\n", "\n")
            .replace('\r', "\n");
        for line in text.split('\n') {
            let line = line.trim_start_matches(mluva_core::text::whitespace);
            let Some((key, name)) = line.split_once(": ") else {
                continue;
            };
            if key.strip_prefix("Vulkan").is_some_and(|number| {
                !number.is_empty() && number.chars().all(mluva_core::text::decimal)
            }) && !name.is_empty()
                && name.contains("NVIDIA")
            {
                return Ok(key.into());
            }
        }
        Err(ProviderError::message(
            "No supported NVIDIA Vulkan device found. Choose CPU.",
        ))
    }
    pub async fn transcribe(
        &self,
        file_path: &Path,
        language_code: &str,
        mut on_partial: Option<&mut (dyn FnMut(String) + Send)>,
    ) -> Result<TranscriptionResult> {
        let language = languages::iso(language_code);
        if language != "auto"
            && !languages::LANGUAGES
                .iter()
                .take(30)
                .any(|entry| entry.iso == language)
        {
            return Err(ProviderError::message(
                "This language is not supported by Qwen. Choose another local model.",
            ));
        }
        let _job = self.transcription.lock().await;
        let mut owned = StopOnDrop(None);
        let mut retain_after_format_failure = false;
        let result = async {
            let session = self.connection().await?;
            owned.0 = Some(session.clone());
            let mut source = WaveReader::open(file_path).map_err(|error| {
                if error.kind() == std::io::ErrorKind::UnexpectedEof
                    || error
                        .get_ref()
                        .is_some_and(|inner| inner.is::<WaveFormatError>())
                {
                    retain_after_format_failure = true;
                    ProviderError(error.to_string())
                } else if error.kind() == std::io::ErrorKind::Other && error.to_string().is_empty()
                {
                    ProviderError::message("")
                } else {
                    session.failure(FAILED)
                }
            })?;
            if source.metadata.channels != 1
                || source.metadata.sample_width != 2
                || source.metadata.sample_rate != 16000
            {
                return Err(session.failure(FAILED));
            }
            let mut parts = vec![];
            loop {
                let frames = source
                    .read_frames(25 * 16000)
                    .map_err(|_| session.failure(FAILED))?;
                if frames.is_empty() {
                    break;
                }
                let audio = pcm16_wav(&frames).map_err(|_| session.failure(FAILED))?;
                parts.push(
                    self.recognize(
                        &session,
                        &audio,
                        language,
                        &parts.join(" "),
                        &mut on_partial,
                    )
                    .await?,
                );
            }
            Ok(TranscriptionResult {
                text: trim(&parts.join(" ")).into(),
                language_code: language_code.into(),
                language_probability: None,
                transcription_id: None,
                speaker_segments: vec![],
                audio_duration_seconds: None,
            })
        }
        .await;
        if (result.is_err() && !retain_after_format_failure) || !self.options.keep_alive {
            self.close().await;
        }
        owned.0 = None;
        result
    }
    async fn recognize(
        &self,
        session: &Session,
        audio: &[u8],
        language: &str,
        previous: &str,
        on_partial: &mut Option<&mut (dyn FnMut(String) + Send)>,
    ) -> Result<String> {
        let mut messages = vec![
            json!({"role":"user","content":[{"type":"input_audio","input_audio":{"data":STANDARD.encode(audio),"format":"wav"}}]}),
        ];
        let mut body = json!({"messages":messages,"temperature":0,"max_tokens":512,"stream":true,"cache_prompt":false});
        if language != "auto" {
            let index = languages::LANGUAGES
                .iter()
                .take(30)
                .position(|entry| entry.iso == language)
                .unwrap();
            messages.push(json!({"role":"assistant","content":format!("language {}<asr_text>", LANGUAGE_NAMES[index])}));
            body["messages"] = json!(messages);
            body["continue_final_message"] = json!(true);
            body["add_generation_prompt"] = json!(false);
        }
        let key = session
            .token
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| session.failure(FAILED))?;
        let bearer = Secret::new(format!("Bearer {}", key.value()));
        let request = self
            .http
            .post(format!("{}/v1/chat/completions", session.url))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(
                reqwest::header::AUTHORIZATION,
                transport::secret_header(&bearer)?,
            )
            .body(multipart::json_body(&body));
        let deadline = tokio::time::Instant::now() + Duration::from_secs(180);
        let read = async {
            let response = tokio::select! {
                biased;
                _ = session.stop.cancelled() => return Err(self.stopped(session)),
                response = request.send() => response.map_err(|_| session.failure(FAILED))?,
            };
            if !response.status().is_success() {
                return Err(session.failure(FAILED));
            }
            let mut lines = transport::Lines::new(response);
            let mut text = String::new();
            let mut transcript = String::new();
            let mut count = 0;
            while let Some(line) = lines
                .next(65_537, &session.stop)
                .await
                .map_err(|_| self.stopped(session))?
            {
                if self.cancelled.is_cancelled() {
                    return Err(ProviderError::message(CANCELLED));
                }
                if tokio::time::Instant::now() > deadline || line.len() > 65_536 {
                    return Err(ProviderError::message(
                        "Local transcription timed out or returned too much data.",
                    ));
                }
                let mut stripped = line.as_slice();
                while stripped
                    .first()
                    .is_some_and(|byte| matches!(*byte, 9..=13 | 32))
                {
                    stripped = &stripped[1..];
                }
                while stripped
                    .last()
                    .is_some_and(|byte| matches!(*byte, 9..=13 | 32))
                {
                    stripped = &stripped[..stripped.len() - 1];
                }
                if stripped == b"data: [DONE]" {
                    return Ok(trim(&transcript).into());
                }
                if !line.starts_with(b"data: ") {
                    continue;
                }
                let event: Value =
                    serde_json::from_slice(&line[6..]).map_err(|_| session.failure(FAILED))?;
                let event = event.as_object().ok_or_else(|| session.failure(FAILED))?;
                let Some(choices) = event
                    .get("choices")
                    .filter(|choices| models::truthy(choices))
                else {
                    continue;
                };
                let first = choices
                    .as_array()
                    .and_then(|choices| choices.first())
                    .and_then(Value::as_object)
                    .ok_or_else(|| session.failure(FAILED))?;
                if first
                    .get("finish_reason")
                    .is_some_and(|reason| reason == "length")
                {
                    return Err(ProviderError::message(
                        "Local transcript exceeded its output limit. Try a shorter recording.",
                    ));
                }
                let content = match first.get("delta") {
                    None => None,
                    Some(value) => value
                        .as_object()
                        .ok_or_else(|| session.failure(FAILED))?
                        .get("content"),
                };
                let Some(content) = content.filter(|content| !content.is_null()) else {
                    continue;
                };
                let content = content.as_str().ok_or_else(|| session.failure(FAILED))?;
                count += content.chars().count();
                text.push_str(content);
                if count > 100_000 {
                    return Err(ProviderError::message(OUTPUT_LIMIT));
                }
                transcript = text
                    .split_once("<asr_text>")
                    .map_or(
                        if language == "auto" {
                            ""
                        } else {
                            text.as_str()
                        },
                        |(_, value)| value,
                    )
                    .into();
                if !transcript.is_empty()
                    && let Some(callback) = on_partial.as_deref_mut()
                {
                    callback(trim(&format!("{previous} {transcript}")).into());
                }
            }
            Err(ProviderError::message(
                "Local transcription stream ended early. Please retry the recording.",
            ))
        };
        read.await
    }
    fn stopped(&self, session: &Session) -> ProviderError {
        if self.cancelled.is_cancelled() {
            ProviderError::message(CANCELLED)
        } else {
            session.failure(FAILED)
        }
    }
}
impl Drop for QwenSpeechClient {
    fn drop(&mut self) {
        self.cancelled.cancel();
        if let Some(session) = self.current.lock().unwrap().as_ref() {
            session.stop.cancel();
        }
    }
}

async fn lifecycle(mut process: Child, temporary: tempfile::TempDir, session: Arc<Session>) {
    let mut monitor = tokio::time::interval(Duration::from_millis(100));
    monitor.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    monitor.tick().await;
    loop {
        tokio::select! {
            biased;
            _ = session.stop.cancelled() => {
                if let Some(pid) = process.id() {
                    unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM); }
                }
                if tokio::time::timeout(Duration::from_secs(2), process.wait()).await.is_err() {
                    let _ = process.start_kill();
                    let _ = process.wait().await;
                }
                break;
            },
            _ = process.wait() => break,
            _ = monitor.tick() => {
                if let Ok(status) = fs::read_to_string(format!("/proc/{}/status", session.pid))
                    && status.lines().find_map(|line| {
                        line.strip_prefix("VmRSS:")
                            .and_then(|value| value.split_whitespace().next())
                            .and_then(|value| value.parse::<u64>().ok())
                    }).is_some_and(|rss| rss.saturating_mul(1024) > 5_000_000_000)
                {
                    *session.failure.lock().unwrap() = Some(MEMORY_LIMIT.into());
                    let _ = process.start_kill();
                    let _ = process.wait().await;
                    break;
                }
            }
        }
    }
    drop(temporary);
    *session.token.lock().unwrap() = None;
    session.done.store(true, Ordering::Release);
    session.changed.notify_waiters();
}
