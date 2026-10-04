//! One owned Omarchy region selection; selected pixels never enter the clipboard.
use mluva_core::{
    private_files::resolve_path,
    screenshots::{MAX_IMAGE_BYTES, validate_png},
};
use std::{
    ffi::OsString,
    fs::{self, DirBuilder, OpenOptions},
    future::Future,
    io::{self, Read},
    os::unix::{
        ffi::OsStringExt,
        fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    process::Stdio,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tokio::{io::AsyncReadExt, process::Command};
use tokio_util::sync::CancellationToken;

const SELECTION_TIMEOUT: Duration = Duration::from_secs(180);
const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);
const MAX_PATH_BYTES: usize = 4096;

#[derive(Debug, thiserror::Error)]
pub enum ScreenshotCaptureError {
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    Invalid(String),
    #[error("Screenshot selection timed out. Press F10 to try again.")]
    Timeout,
}
pub type ScreenshotCaptureResult = Result<Option<Vec<u8>>, ScreenshotCaptureError>;

/// Keep this owner until selection has been attached or discarded. `run` is a
/// single-use, non-borrowing future; dropping either it or this owner cancels
/// the picker. Await its result before releasing the runtime during shutdown.
pub struct ScreenshotCapture {
    directory: PathBuf,
    cancelled: CancellationToken,
    started: AtomicBool,
}
impl ScreenshotCapture {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
            cancelled: CancellationToken::new(),
            started: AtomicBool::new(false),
        }
    }
    pub fn cancel(&self) {
        self.cancelled.cancel();
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.is_cancelled()
    }
    pub fn run(&self) -> impl Future<Output = ScreenshotCaptureResult> + Send + use<> {
        let directory = self.directory.clone();
        let cancelled = self.cancelled.clone();
        let first = !self.started.swap(true, Ordering::AcqRel);
        async move {
            if !first {
                return Err(ScreenshotCaptureError::Invalid(
                    "This screenshot picker has already started.".into(),
                ));
            }
            let guard = cancelled.clone().drop_guard();
            // Own cleanup independently of a caller that drops its waiter.
            let result = tokio::spawn(run_owned(directory, cancelled))
                .await
                .map_err(|_| {
                    ScreenshotCaptureError::Invalid(
                        "Screenshot selection stopped unexpectedly.".into(),
                    )
                });
            guard.disarm();
            result?
        }
    }
}
impl Drop for ScreenshotCapture {
    fn drop(&mut self) {
        self.cancel();
    }
}

struct PickerProcess(tokio::process::Child);
impl PickerProcess {
    fn signal_group(&self, signal: i32) {
        if let Some(pid) = self.0.id() {
            // The unreaped child retains this process-group ID. It was created
            // by setsid below; never signal a group obtained from picker output.
            unsafe {
                libc::kill(-(pid as libc::pid_t), signal);
            }
        }
    }
    async fn close(&mut self) {
        self.signal_group(libc::SIGTERM);
        let deadline = tokio::time::Instant::now() + SHUTDOWN_GRACE;
        // Retain the leader until its whole group has been signalled. Reaping
        // a promptly exiting leader first would abandon a stubborn descendant
        // and allow the numeric group ID to be reused before escalation.
        while !self.exited_without_reaping() && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        self.signal_group(libc::SIGKILL);
        let _ = self.0.wait().await;
    }
    fn exited_without_reaping(&self) -> bool {
        let Some(pid) = self.0.id() else {
            return true;
        };
        let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
        // WNOHANG never blocks; WNOWAIT preserves the child's ownership until
        // Tokio performs the only reap after process-group cleanup.
        unsafe {
            libc::waitid(
                libc::P_PID,
                pid,
                info.as_mut_ptr(),
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            ) == 0
                && info.assume_init().si_pid() != 0
        }
    }
}
impl Drop for PickerProcess {
    fn drop(&mut self) {
        self.signal_group(libc::SIGKILL);
    }
}

async fn run_owned(directory: PathBuf, cancelled: CancellationToken) -> ScreenshotCaptureResult {
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&directory)?;
    let temporary = tempfile::Builder::new()
        .prefix("screenshot-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir_in(directory)?;
    if cancelled.is_cancelled() {
        return Ok(None);
    }
    let mut command = Command::new("omarchy");
    command
        .args(["screenshot", "region", "save"])
        .env("OMARCHY_SCREENSHOT_DIR", temporary.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    // This is the released start_new_session contract, restricted to an
    // async-signal-safe call between fork and exec.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut process = PickerProcess(command.spawn()?);
    let mut stdout = process.0.stdout.take().expect("requested stdout pipe");
    let completion = async {
        let mut output = Vec::new();
        let mut has_text = false;
        let mut buffer = [0; 8192];
        loop {
            let count = stdout.read(&mut buffer).await?;
            if count == 0 {
                break;
            }
            has_text |= buffer[..count].iter().any(|byte| !whitespace(*byte));
            let retain = count.min((MAX_PATH_BYTES + 1).saturating_sub(output.len()));
            output.extend_from_slice(&buffer[..retain]);
        }
        // Read before wait so an inherited pipe cannot outlive an already
        // reaped group leader and make subsequent group cancellation unsafe.
        let status = process.0.wait().await?;
        Ok::<_, io::Error>((status, output, has_text))
    };
    let result = tokio::select! { biased;
        _ = cancelled.cancelled() => None,
        result = tokio::time::timeout(SELECTION_TIMEOUT, completion) => Some(result),
    };
    let (status, output, has_text) = match result {
        None => {
            process.close().await;
            return Ok(None);
        }
        Some(Err(_)) => {
            cancelled.cancel();
            process.close().await;
            return Err(ScreenshotCaptureError::Timeout);
        }
        Some(Ok(Err(error))) => {
            process.close().await;
            return Err(error.into());
        }
        Some(Ok(Ok(value))) => value,
    };
    if cancelled.is_cancelled() || !status.success() || !has_text {
        return Ok(None);
    }
    if output.len() > MAX_PATH_BYTES {
        return Err(ScreenshotCaptureError::Invalid(
            "Screenshot picker returned an invalid file.".into(),
        ));
    }
    let start = output.iter().position(|byte| !whitespace(*byte)).unwrap();
    let end = output.iter().rposition(|byte| !whitespace(*byte)).unwrap() + 1;
    let path = PathBuf::from(OsString::from_vec(output[start..end].to_vec()));
    if path.is_symlink()
        || resolve_path(&path)?.parent() != Some(resolve_path(temporary.path())?.as_path())
    {
        return Err(ScreenshotCaptureError::Invalid(
            "Screenshot picker returned a file outside its capture directory.".into(),
        ));
    }
    let data = read_image(&path)?;
    validate_png(&data).map_err(|error| ScreenshotCaptureError::Invalid(error.to_string()))?;
    Ok(Some(data))
}
fn whitespace(byte: u8) -> bool {
    matches!(byte, 9..=13 | 32)
}

fn read_image(path: &Path) -> io::Result<Vec<u8>> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(io::Error::from_raw_os_error(if metadata.is_dir() {
            libc::EISDIR
        } else {
            libc::EINVAL
        }));
    }
    let mut data = Vec::new();
    file.take((MAX_IMAGE_BYTES + 1) as u64)
        .read_to_end(&mut data)?;
    Ok(data)
}
