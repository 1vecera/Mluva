//! Memory-only Incognito staging with an independent native EOF cleanup process.

use crate::process;
use std::fs;
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Instant;

const MEMORY_ERROR: &str = "Incognito needs memory-backed /dev/shm; recording was not started.";
const START_ERROR: &str = "Incognito crash cleanup could not start.";
const STOPPED_ERROR: &str = "Incognito crash cleanup stopped; restart Mluva before recording.";

pub fn memory_backed(path: &Path) -> bool {
    let Ok(mountinfo) = fs::read_to_string("/proc/self/mountinfo") else {
        return false;
    };
    let mut selected = None;
    let mut length = 0;
    for line in mountinfo.lines() {
        let Some((left, right)) = line.split_once(" - ") else {
            continue;
        };
        let Some(mount) = left.split_whitespace().nth(4) else {
            continue;
        };
        let mount = mount.replace("\\040", " ").replace("\\134", "\\");
        if path.starts_with(&mount) && mount.len() > length {
            length = mount.len();
            selected = right.split_whitespace().next();
        }
    }
    matches!(selected, Some("tmpfs" | "ramfs"))
}

pub struct VolatileAudioStore {
    path: PathBuf,
    identity: (u64, u64),
    watchdog: Option<Child>,
}

impl VolatileAudioStore {
    pub fn open(cleanup_executable: &Path) -> io::Result<Self> {
        Self::open_in(Path::new("/dev/shm"), cleanup_executable)
    }

    /// An alternate root must still be a real kernel memory-backed filesystem.
    pub fn open_in(root: &Path, cleanup_executable: &Path) -> io::Result<Self> {
        let root = root.canonicalize()?;
        if !memory_backed(&root) {
            return Err(io::Error::other(MEMORY_ERROR));
        }
        let directory = tempfile::Builder::new()
            .prefix(&format!("mluva-audio-{}-", unsafe { libc::getuid() }))
            .tempdir_in(root)?;
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700))?;
        let metadata = directory.path().metadata()?;
        let mut store = Self {
            path: directory.keep(),
            identity: (metadata.dev(), metadata.ino()),
            watchdog: None,
        };
        let mut command = Command::new(cleanup_executable);
        command
            .arg(&store.path)
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let start = (|| {
            store.watchdog = Some(command.spawn()?);
            let mut stdout = store.watchdog.as_mut().unwrap().stdout.take().unwrap();
            process::nonblocking(stdout.as_raw_fd())?;
            let deadline = Instant::now() + process::FINALIZATION_TIMEOUT;
            let mut ready = [0; 6];
            let mut received = 0;
            while received < ready.len() {
                process::readable(stdout.as_raw_fd(), deadline)?;
                match stdout.read(&mut ready[received..]) {
                    Ok(0) => return Err(io::Error::other(START_ERROR)),
                    Ok(count) => received += count,
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
            if &ready != b"ready\n" {
                return Err(io::Error::other(START_ERROR));
            }
            Ok(())
        })();
        if start.is_err() {
            return Err(io::Error::other(START_ERROR));
        }
        Ok(store)
    }

    pub fn directory(&mut self) -> io::Result<&Path> {
        if !self
            .watchdog
            .as_mut()
            .is_some_and(|watchdog| matches!(watchdog.try_wait(), Ok(None)))
        {
            return Err(io::Error::other(STOPPED_ERROR));
        }
        Ok(&self.path)
    }

    pub fn close(&mut self) {
        if let Some(mut watchdog) = self.watchdog.take() {
            // EOF, rather than a signal, is the janitor's authority to clean up.
            watchdog.stdin.take();
            let _ = process::wait(&mut watchdog, process::FINALIZATION_TIMEOUT);
        }
        remove_owned_directory(&self.path, self.identity);
    }
}

impl Drop for VolatileAudioStore {
    fn drop(&mut self) {
        self.close();
    }
}

fn remove_owned_directory(path: &Path, identity: (u64, u64)) {
    if fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.is_dir()
            && (metadata.dev(), metadata.ino()) == identity
            && metadata.uid() == unsafe { libc::getuid() }
    }) {
        let _ = fs::remove_dir_all(path);
    }
}

/// Executed only by the dedicated helper binary, in a fresh session with no environment.
pub fn run_cleanup(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    let uid = unsafe { libc::getuid() };
    let prefix = format!("mluva-audio-{uid}-");
    if !metadata.is_dir()
        || metadata.uid() != uid
        || metadata.permissions().mode() & 0o777 != 0o700
        || !path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with(&prefix))
        || path.canonicalize()? != path
        || !memory_backed(path)
    {
        return Err(io::Error::other("Invalid volatile audio directory."));
    }
    let identity = (metadata.dev(), metadata.ino());
    let result = (|| {
        let mut stdout = io::stdout().lock();
        stdout.write_all(b"ready\n")?;
        stdout.flush()?;
        drop(stdout);
        let mut stdin = io::stdin().lock();
        let mut buffer = [0; 4_096];
        loop {
            match stdin.read(&mut buffer) {
                Ok(0) => return Ok(()),
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
    })();
    remove_owned_directory(path, identity);
    result
}
