use crate::support::{Peer, mode, reference};
use mluva_audio::volatile::{VolatileAudioStore, memory_backed};
use mluva_audio::wav::WaveReader;
use serde_json::{Value, json};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

const CLEANUP: &str = env!("CARGO_BIN_EXE_mluva-audio-cleanup");

struct Owner {
    child: Child,
    messages: Receiver<Value>,
    directory: Option<PathBuf>,
}

impl Owner {
    fn start(peer: &Peer, meeting: bool) -> Self {
        Self::start_mode(peer, if meeting { "meeting" } else { "dictation" })
    }

    fn start_mode(peer: &Peer, mode: &str) -> Self {
        let mut command = Command::new(&peer.executable);
        command
            .args(["owner", peer.executable.to_str().unwrap(), CLEANUP, mode])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = command.spawn().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, messages) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else {
                    break;
                };
                let Ok(value) = serde_json::from_str(&line) else {
                    break;
                };
                if sender.send(value).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            messages,
            directory: None,
        }
    }

    fn message(&mut self) -> Value {
        let value = self
            .messages
            .recv_timeout(Duration::from_secs(8))
            .expect("Native owner did not respond");
        if let Some(directory) = value["directory"].as_str() {
            self.directory = Some(directory.into());
        }
        value
    }

    fn crash(&mut self) {
        let pid = self.child.id() as i32;
        // Never signal any inherited/host process group.
        assert_eq!(unsafe { libc::getsid(pid) }, pid);
        assert_eq!(unsafe { libc::getpgid(pid) }, pid);
        assert_eq!(unsafe { libc::killpg(pid, libc::SIGKILL) }, 0);
        self.child.wait().unwrap();
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let pid = self.child.id() as i32;
            if unsafe { libc::getpgid(pid) } == pid {
                unsafe { libc::killpg(pid, libc::SIGKILL) };
            }
            let _ = self.child.wait();
        }
        if let Some(directory) = &self.directory {
            let deadline = Instant::now() + Duration::from_secs(5);
            while directory.exists() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }
            if directory.exists() {
                let _ = fs::remove_dir_all(directory);
            }
        }
    }
}

fn owner_peer() -> Peer {
    let reference = reference();
    let mut config = reference["meetings"][0]["config"].clone();
    config["pcm_hex"] = reference["recordings"][0]["config"]["pcm_hex"].clone();
    config["repeat"] = json!(true);
    config["exit"] = json!(1);
    Peer::new(&config)
}

#[test]
fn sigkill_removes_real_dictation_and_meeting_audio_without_app_restart() {
    let released: Value =
        serde_json::from_str(include_str!("../fixtures/released-audio-privacy.json")).unwrap();
    assert_eq!(released["reference"], reference()["reference"]);
    for meeting in [false, true] {
        let peer = owner_peer();
        let mut owner = Owner::start(&peer, meeting);
        let report = owner.message();
        let directory = PathBuf::from(report["directory"].as_str().unwrap());
        let watchdog = report["watchdog"].as_u64().unwrap() as i32;
        let directory_mode = mode(&directory);
        let memory = memory_backed(&directory);
        let session = unsafe { libc::getsid(watchdog) } == watchdog;
        let environment_empty = fs::read(format!("/proc/{watchdog}/environ"))
            .unwrap()
            .is_empty();
        let mut file_modes: Vec<_> = report["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|file| file["mode"].as_u64().unwrap())
            .collect();
        file_modes.sort_unstable();
        assert_ne!(watchdog, owner.child.id() as i32);
        assert_eq!(
            fs::read_to_string(format!("/proc/{watchdog}/cmdline"))
                .unwrap()
                .split('\0')
                .filter(|argument| !argument.is_empty())
                .collect::<Vec<_>>(),
            vec![CLEANUP, directory.to_str().unwrap()]
        );
        owner.crash();
        let deadline = Instant::now() + Duration::from_secs(5);
        while directory.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        let durable_audio_absent =
            !peer
                .path()
                .read_dir()
                .unwrap()
                .filter_map(Result::ok)
                .any(|entry| {
                    entry
                        .path()
                        .extension()
                        .is_some_and(|extension| extension == "wav")
                });
        let actual = json!({"meeting":meeting,"file_count":file_modes.len(),"file_modes":file_modes,"directory_mode":directory_mode,"memory_backed":memory,"watchdog_has_own_session":session,"watchdog_environment_empty":environment_empty,"removed_after_sigkill":!directory.exists(),"durable_audio_absent":durable_audio_absent});
        assert_eq!(actual, released["observations"][usize::from(meeting)]);
    }
}

#[test]
fn watchdog_death_refuses_another_capture_and_parent_still_erases_audio() {
    let peer = owner_peer();
    let mut owner = Owner::start(&peer, false);
    let report = owner.message();
    let watchdog = report["watchdog"].as_u64().unwrap() as i32;
    assert_eq!(unsafe { libc::kill(watchdog, libc::SIGKILL) }, 0);
    let deadline = Instant::now() + Duration::from_secs(2);
    while fs::read_to_string(format!("/proc/{watchdog}/status"))
        .is_ok_and(|status| !status.lines().any(|line| line.starts_with("State:\tZ")))
    {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    owner
        .child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"check\n")
        .unwrap();
    assert_eq!(
        owner.message()["error"],
        "Incognito crash cleanup stopped; restart Mluva before recording."
    );
    owner.child.stdin.take();
    owner.child.wait().unwrap();
    assert!(!Path::new(report["directory"].as_str().unwrap()).exists());
}

#[test]
fn durable_capture_keeps_released_recovery_metadata_after_sigkill() {
    let reference = reference();
    let case = &reference["live_recording"];
    let peer = Peer::new(&case["config"]);
    let mut owner = Owner::start_mode(&peer, "durable");
    let report = owner.message();
    assert!(report["watchdog"].is_null());
    let path = Path::new(report["directory"].as_str().unwrap()).join("capture.wav");
    owner.crash();
    assert!(path.exists());
    assert_eq!(mode(&path), 0o600);
    let reading = WaveReader::open(&path).unwrap();
    let actual = json!({"compatible":reading.compatible(),"frame_count":reading.metadata.frame_count,"header_hex":crate::support::hex(&fs::read(&path).unwrap()[..44])});
    assert_eq!(actual, case["live_metadata"]);
    // Durable fixtures belong to the test root; no janitor should erase them.
    owner.directory = None;
}

#[test]
fn volatile_staging_refuses_disk_and_removes_every_directory_on_close() {
    let disk = tempfile::tempdir().unwrap();
    assert!(!memory_backed(disk.path()));
    let error = VolatileAudioStore::open_in(disk.path(), Path::new(CLEANUP))
        .err()
        .unwrap();
    assert_eq!(
        error.to_string(),
        "Incognito needs memory-backed /dev/shm; recording was not started."
    );
    assert_eq!(fs::read_dir(disk.path()).unwrap().count(), 0);
    let memory = tempfile::tempdir_in("/dev/shm").unwrap();
    let mut store = VolatileAudioStore::open_in(memory.path(), Path::new(CLEANUP)).unwrap();
    let directory = store.directory().unwrap().to_path_buf();
    fs::write(directory.join("fixture.wav"), b"synthetic audio").unwrap();
    store.close();
    store.close();
    assert!(!directory.exists());
    assert_eq!(
        store.directory().unwrap_err().to_string(),
        "Incognito crash cleanup stopped; restart Mluva before recording."
    );
}

#[test]
fn malformed_or_incomplete_watchdog_readiness_fails_closed_and_reaps_child() {
    for (protocol, hang) in [("wrong\n", false), ("r", true)] {
        let memory = tempfile::tempdir_in("/dev/shm").unwrap();
        let peer = Peer::new(&json!({"ready_protocol":protocol,"hang":hang}));
        let started = Instant::now();
        let error = VolatileAudioStore::open_in(memory.path(), &peer.executable)
            .err()
            .unwrap();
        assert_eq!(
            error.to_string(),
            "Incognito crash cleanup could not start."
        );
        assert!(started.elapsed() < Duration::from_secs(15));
        assert_eq!(fs::read_dir(memory.path()).unwrap().count(), 0);
        let pid = peer.ready("protocol")["pid"].as_u64().unwrap() as i32;
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
    }
}

#[test]
fn native_janitor_refuses_unowned_scope_and_never_follows_nested_links() {
    let disk = tempfile::tempdir().unwrap();
    let sentinel = disk.path().join("retain.wav");
    fs::write(&sentinel, b"retain existing recording").unwrap();
    let refused = Command::new(CLEANUP)
        .arg(disk.path())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(refused.stdout.is_empty());
    assert_eq!(fs::read(&sentinel).unwrap(), b"retain existing recording");
    let memory = tempfile::tempdir_in("/dev/shm").unwrap();
    let directory = memory
        .path()
        .join(format!("mluva-audio-{}-fixture", unsafe { libc::getuid() }));
    fs::create_dir(&directory).unwrap();
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
    symlink(disk.path(), directory.join("external")).unwrap();
    let mut child = Command::new(CLEANUP)
        .arg(&directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut ready = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut ready)
        .unwrap();
    assert_eq!(ready, "ready\n");
    child.stdin.take();
    assert!(child.wait().unwrap().success());
    assert!(!directory.exists());
    assert_eq!(fs::read(&sentinel).unwrap(), b"retain existing recording");
}
