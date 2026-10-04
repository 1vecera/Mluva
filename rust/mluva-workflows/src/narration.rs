//! Fresh speech for one editor text box. Only an explicit Stop permits upload;
//! raw audio stays in memory, and this command never opens History or delivery.

use crate::{preparation::freeze_transcript_preparation, services::NativeBinaries};
use mluva_audio::{
    capture::{CaptureRecorder, CaptureStorage},
    recorder::PipeWireRecorder,
    volatile::VolatileAudioStore,
};
use mluva_core::{
    config::{AppConfig, AppPaths},
    personalization::PersonalizationStore,
    text::trim,
};
use mluva_providers::{credentials::CredentialStore, local_asr::OnnxOptions, speech::SpeechClient};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{self, Read, Write},
    os::fd::{AsRawFd, FromRawFd},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};
use tokio::{
    signal::unix::{Signal, SignalKind, signal},
    sync::oneshot,
};

const PRIVATE: &str = "Narrated screenshots are unavailable in Incognito.";
const FAILED: &str =
    "Annotation could not finish. Check Mluva's microphone and speech provider, then try again.";
const MAX_CHARACTERS: usize = 120_000;

enum Failure {
    Private,
    Failed,
}
fn failed<E>(_: E) -> Failure {
    Failure::Failed
}
type Result<T> = std::result::Result<T, Failure>;

/// The editor supplies stdin and owns the helper's process group. Signals unwind
/// through the same microphone/provider cleanup even while stdin remains open.
pub async fn run_cli() -> u8 {
    let result = async {
        let mut terminate = signal(SignalKind::terminate()).map_err(failed)?;
        let mut interrupt = signal(SignalKind::interrupt()).map_err(failed)?;
        narrate(&mut terminate, &mut interrupt).await
    }
    .await;
    match result {
        Ok(Some(text)) => {
            let mut stdout = io::stdout().lock();
            if stdout
                .write_all(text.as_bytes())
                .and_then(|()| stdout.flush())
                .is_ok()
            {
                0
            } else {
                let _ = writeln!(io::stderr(), "{FAILED}");
                1
            }
        }
        Ok(None) => 1,
        Err(error) => {
            let _ = writeln!(
                io::stderr(),
                "{}",
                match error {
                    Failure::Private => PRIVATE,
                    Failure::Failed => FAILED,
                }
            );
            1
        }
    }
}

async fn interrupted(terminate: &mut Signal, interrupt: &mut Signal) {
    tokio::select! { _ = terminate.recv() => {}, _ = interrupt.recv() => {} }
}

async fn narrate(terminate: &mut Signal, interrupt: &mut Signal) -> Result<Option<String>> {
    let environ: BTreeMap<_, _> = [
        "HOME",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_RUNTIME_DIR",
    ]
    .into_iter()
    .filter_map(|name| std::env::var(name).ok().map(|value| (name.into(), value)))
    .collect();
    let paths = AppPaths::from_environ(&environ).map_err(failed)?;
    let config_path = paths.config.join("config.json");
    let config = AppConfig::load(&config_path).map_err(failed)?;
    if config.incognito_mode {
        return Err(Failure::Private);
    }
    let binaries = NativeBinaries::beside_application().map_err(failed)?;
    let credentials = CredentialStore::new();
    let local = OnnxOptions::new(paths.data, &config.local_model, binaries.asr_worker);
    let speech = tokio::select! {
        biased;
        _ = interrupted(terminate, interrupt) => return Err(Failure::Failed),
        client = SpeechClient::new(&config, local, &credentials) => client.map_err(failed)?,
    };
    // Close the transport even if personalization, microphone discovery or the
    // memory janitor cannot be initialized. No application stores are opened.
    let result = async {
        let personal = PersonalizationStore::new(paths.config.join("personalization.json"));
        let preparation = freeze_transcript_preparation(&config, Some(&personal), "dictation", None).map_err(failed)?;
        let microphone = PipeWireRecorder::from_system(config.microphone_target.clone()).map_err(failed)?;
        let mut memory = VolatileAudioStore::open(&binaries.audio_cleanup).map_err(failed)?;
        let destination = memory.directory().map_err(failed)?.to_owned();
        let recorder = CaptureRecorder::new(microphone).map_err(failed)?;
        let result = tokio::select! {
            biased;
            _ = interrupted(terminate, interrupt) => Err(Failure::Failed),
            result = async {
                let audio = recorder.prepare_destination(CaptureStorage::Persistent(destination), "annotation.wav".into()).await.map_err(failed)?;
                recorder.start(audio, None).await.map_err(failed)?;
                let mut input = ControlInput::open().map_err(failed)?;
                if !input.stop().await.map_err(failed)? { return Ok(None); }
                let audio = recorder.stop().await.map_err(failed)?;
                if AppConfig::load(&config_path).map_err(failed)?.incognito_mode { return Ok(None); }
                let transcript = speech.transcribe(&audio, &config.language_code, &config.transcription_model).await.map_err(failed)?;
                let text = preparation.process(&transcript.text);
                if trim(&text).is_empty() || text.chars().count() > MAX_CHARACTERS { return Err(Failure::Failed); }
                Ok(Some(text))
            } => result,
        };
        // CaptureRecorder drains any queued start/stop before acknowledging
        // cancellation; cleanup never races a late microphone start.
        let cancelled = recorder.cancel().await;
        let closed = recorder.close().await;
        memory.close();
        cancelled.map_err(failed)?;
        closed.map_err(failed)?;
        result
    }.await;
    if result.is_err() {
        speech.cancel().await;
    }
    speech.close().await;
    result
}

/// Tokio's standard input cannot cancel a blocking read. This small owned reader
/// polls a duplicate descriptor instead, so editor cancellation also joins the
/// input thread. It never changes the caller's descriptor flags.
struct ControlInput {
    cancel: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    received: oneshot::Receiver<io::Result<bool>>,
}
impl ControlInput {
    fn open() -> io::Result<Self> {
        let fd = unsafe { libc::fcntl(libc::STDIN_FILENO, libc::F_DUPFD_CLOEXEC, 3) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // The successful duplication gives this File sole ownership of fd.
        let file = unsafe { File::from_raw_fd(fd) };
        let cancel = Arc::new(AtomicBool::new(false));
        let stopping = cancel.clone();
        let (send, received) = oneshot::channel();
        let thread = std::thread::Builder::new()
            .name("mluva-annotation-input".into())
            .spawn(move || {
                let _ = send.send(read_control(file, &stopping));
            })?;
        Ok(Self {
            cancel,
            thread: Some(thread),
            received,
        })
    }
    async fn stop(&mut self) -> io::Result<bool> {
        (&mut self.received).await.map_err(io::Error::other)?
    }
}
impl Drop for ControlInput {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn read_control(mut input: File, cancel: &AtomicBool) -> io::Result<bool> {
    let mut line = Vec::with_capacity(128);
    let mut pending = Vec::with_capacity(4);
    let mut characters = 0;
    while characters < 32 && !cancel.load(Ordering::Acquire) {
        let mut descriptor = libc::pollfd {
            fd: input.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        if unsafe { libc::poll(&mut descriptor, 1, 50) } < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        if descriptor.revents == 0 {
            continue;
        }
        let mut byte = [0];
        match input.read(&mut byte) {
            Ok(0) => return Ok(false),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
        line.push(byte[0]);
        if byte[0] == b'\n' {
            return Ok(line == b"stop\n");
        }
        pending.push(byte[0]);
        loop {
            match std::str::from_utf8(&pending) {
                Ok(text) => {
                    characters += text.chars().count();
                    pending.clear();
                    break;
                }
                Err(error) if error.error_len().is_none() => break,
                Err(error) => {
                    // Unix stdin uses surrogate escapes for invalid bytes. They
                    // count individually and cannot turn into the Stop command.
                    let consumed = error.valid_up_to() + error.error_len().unwrap();
                    characters += consumed;
                    pending.drain(..consumed);
                }
            }
        }
    }
    Ok(false)
}
