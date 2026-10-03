//! Cancellation belongs to the installer and its children, never the desktop.
use crate::install_paths::Result;
use std::{
    fmt,
    os::unix::process::{CommandExt, ExitStatusExt},
    process::Command,
    sync::atomic::{AtomicBool, AtomicI32, Ordering},
    thread,
    time::{Duration, Instant},
};

static SIGNAL: AtomicI32 = AtomicI32::new(0);
static RESTORING: AtomicBool = AtomicBool::new(false);
extern "C" fn interrupted(signal: libc::c_int) {
    if RESTORING.load(Ordering::Relaxed) {
        return;
    }
    let _ = SIGNAL.compare_exchange(0, signal, Ordering::Relaxed, Ordering::Relaxed);
}

pub struct Rollback(i32);
impl Rollback {
    pub fn begin() -> Self {
        RESTORING.store(true, Ordering::Relaxed);
        Self(SIGNAL.swap(0, Ordering::Relaxed))
    }
}
impl Drop for Rollback {
    fn drop(&mut self) {
        SIGNAL.store(self.0, Ordering::Relaxed);
        RESTORING.store(false, Ordering::Relaxed);
    }
}

pub fn output(command: &mut Command) -> Result<(u8, String)> {
    use std::io::{Read, Seek};
    let mut output = tempfile::tempfile()?;
    let result = run(command
        .stdin(std::process::Stdio::null())
        .stdout(output.try_clone()?)
        .stderr(std::process::Stdio::null()));
    checkpoint()?;
    let status = match result {
        Ok(()) => 0,
        Err(error) => match error.downcast_ref::<Failed>() {
            Some(failure) => failure.0,
            None => return Err(error),
        },
    };
    output.rewind()?;
    let mut text = String::new();
    output
        .take(1024 * 1024)
        .read_to_string(&mut text)
        .map_err(|_| "A desktop command returned invalid text.")?;
    Ok((status, text))
}

pub fn watch_signals() -> Result<()> {
    for signal in [libc::SIGHUP, libc::SIGINT, libc::SIGTERM] {
        let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
        action.sa_sigaction = interrupted as *const () as usize;
        unsafe { libc::sigemptyset(&mut action.sa_mask) };
        if unsafe { libc::sigaction(signal, &action, std::ptr::null_mut()) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    Ok(())
}

#[derive(Debug)]
pub struct Failed(pub u8);
impl fmt::Display for Failed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Installation stopped with status {}.", self.0)
    }
}
impl std::error::Error for Failed {}

pub fn checkpoint() -> Result<()> {
    let signal = SIGNAL.load(Ordering::Relaxed);
    if signal != 0 {
        return Err(Failed((128 + signal) as u8).into());
    }
    Ok(())
}

pub fn run(command: &mut Command) -> Result<()> {
    execute(command, true)
}

/// sudo's authentication prompt must keep the caller's terminal foreground
/// group. Cancellation signals only that owned child, never the parent group.
pub fn authenticate(command: &mut Command) -> Result<()> {
    execute(command, false)
}

fn execute(command: &mut Command, group: bool) -> Result<()> {
    checkpoint()?;
    if group {
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(|error| {
        Failed(if error.kind() == std::io::ErrorKind::NotFound {
            127
        } else {
            126
        })
    })?;
    loop {
        if let Err(error) = checkpoint() {
            let target = if group {
                -(child.id() as i32)
            } else {
                child.id() as i32
            };
            unsafe { libc::kill(target, libc::SIGTERM) };
            let deadline = Instant::now() + Duration::from_secs(2);
            while child.try_wait()?.is_none() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }
            unsafe { libc::kill(target, libc::SIGKILL) };
            child.wait()?;
            return Err(error);
        }
        if let Some(status) = child.try_wait()? {
            return if status.success() {
                Ok(())
            } else {
                Err(
                    Failed(status.code().unwrap_or(128 + status.signal().unwrap_or(0)) as u8)
                        .into(),
                )
            };
        }
        thread::sleep(Duration::from_millis(10));
    }
}
