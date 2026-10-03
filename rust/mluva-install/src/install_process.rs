//! Cancellation belongs to the installer and its children, never the desktop.
use crate::install_paths::Result;
use std::{
    fmt,
    os::unix::process::{CommandExt, ExitStatusExt},
    process::Command,
    sync::atomic::{AtomicI32, Ordering},
    thread,
    time::{Duration, Instant},
};

static SIGNAL: AtomicI32 = AtomicI32::new(0);
extern "C" fn interrupted(signal: libc::c_int) {
    let _ = SIGNAL.compare_exchange(0, signal, Ordering::Relaxed, Ordering::Relaxed);
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
    checkpoint()?;
    let mut child = command.process_group(0).spawn().map_err(|error| {
        Failed(if error.kind() == std::io::ErrorKind::NotFound {
            127
        } else {
            126
        })
    })?;
    loop {
        if let Err(error) = checkpoint() {
            unsafe { libc::kill(-(child.id() as i32), libc::SIGTERM) };
            let deadline = Instant::now() + Duration::from_secs(2);
            while child.try_wait()?.is_none() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }
            unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
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
