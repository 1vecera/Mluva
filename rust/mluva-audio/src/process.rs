use crate::{AudioCaptureError, Result};
use std::env;
use std::ffi::{CString, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub(crate) const FINALIZATION_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) fn find_executable(name: &str) -> Option<PathBuf> {
    let search_path = env::var_os("PATH").unwrap_or_else(|| {
        let length = unsafe { libc::confstr(libc::_CS_PATH, std::ptr::null_mut(), 0) };
        if length == 0 {
            return OsString::from("/bin:/usr/bin");
        }
        let mut value = vec![0; length];
        unsafe { libc::confstr(libc::_CS_PATH, value.as_mut_ptr().cast(), length) };
        value.truncate(length - 1);
        OsString::from_vec(value)
    });
    env::split_paths(&search_path)
        .map(|directory| directory.join(name))
        .find(|path| {
            fs::metadata(path).is_ok_and(|metadata| {
                metadata.is_file()
                    && CString::new(path.as_os_str().as_bytes())
                        .is_ok_and(|path| unsafe { libc::access(path.as_ptr(), libc::X_OK) } == 0)
            })
        })
}

pub(crate) fn record_command(executable: &Path, target: Option<&str>) -> Command {
    let mut command = Command::new(executable);
    command.args(["--rate", "16000", "--channels", "1", "--format", "s16"]);
    if let Some(target) = target {
        command.args(["--target", target]);
    }
    // Only async-signal-safe operations run between fork and exec.
    unsafe {
        command.pre_exec(|| {
            libc::umask(0o077);
            Ok(())
        });
    }
    command
}

pub(crate) fn private_parent(path: &Path) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
}

pub(crate) fn create_private_file(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
}

pub(crate) fn remove_file(path: &Path) {
    let _ = fs::remove_file(path);
}

pub(crate) fn signal(child: &mut Child, signal: i32) -> io::Result<()> {
    if child.try_wait()?.is_none() {
        // A live Child retains this PID until wait reaps it; ESRCH is a normal race.
        let result = unsafe { libc::kill(child.id() as libc::pid_t, signal) };
        if result != 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ESRCH) {
                return Err(error);
            }
        }
    }
    Ok(())
}

pub(crate) fn wait(child: &mut Child, timeout: Duration) -> io::Result<ExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            if let Err(error) = child.kill() {
                if let Some(status) = child.try_wait()? {
                    return Ok(status);
                }
                return Err(error);
            }
            return child.wait();
        }
        thread::sleep(Duration::from_millis(5));
    }
}

pub(crate) fn finalize(child: &mut Child) -> io::Result<ExitStatus> {
    signal(child, libc::SIGINT)?;
    wait(child, FINALIZATION_TIMEOUT)
}

pub(crate) fn terminate(child: &mut Child) {
    let _ = signal(child, libc::SIGTERM);
    let _ = wait(child, FINALIZATION_TIMEOUT);
}

pub(crate) fn accepted(status: ExitStatus) -> bool {
    matches!(status.code(), Some(0 | 1)) || status.signal() == Some(libc::SIGINT)
}

pub(crate) fn recorder_executable() -> Result<PathBuf> {
    find_executable("pw-record").ok_or(AudioCaptureError::Message(
        "pw-record is required. Install PipeWire tools for your distribution.",
    ))
}

pub(crate) fn nonblocking(fd: RawFd) -> io::Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags == -1 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub(crate) fn readable(fd: RawFd, deadline: Instant) -> io::Result<()> {
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Subprocess response timed out.",
            ));
        }
        let mut descriptor = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let result = unsafe {
            libc::poll(
                &mut descriptor,
                1,
                remaining.as_millis().min(i32::MAX as u128) as i32,
            )
        };
        if result > 0 {
            return Ok(());
        }
        if result == 0 {
            continue;
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

/// Drain without retaining oversized output or allowing a pipe to outlive its deadline.
pub(crate) fn bounded_stdout(
    mut command: Command,
    limit: usize,
    timeout: Duration,
) -> io::Result<(ExitStatus, Vec<u8>)> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let result = (|| {
        let mut stdout = child.stdout.take().expect("requested a stdout pipe");
        nonblocking(stdout.as_raw_fd())?;
        let deadline = Instant::now() + timeout;
        let mut output = Vec::new();
        let mut buffer = [0; 16_384];
        loop {
            readable(stdout.as_raw_fd(), deadline)?;
            match stdout.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    let retain = count.min((limit + 1).saturating_sub(output.len()));
                    output.extend_from_slice(&buffer[..retain]);
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                    ) =>
                {
                    continue;
                }
                Err(error) => return Err(error),
            }
        }
        Ok((
            wait(
                &mut child,
                deadline.saturating_duration_since(Instant::now()),
            )?,
            output,
        ))
    })();
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}
