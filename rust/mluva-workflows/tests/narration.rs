//! Compare the actual native CLI with independently observed v1.6.0 processes.
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    os::unix::{
        fs::{PermissionsExt, symlink},
        process::{CommandExt, ExitStatusExt},
    },
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
#[path = "support/http.rs"]
mod http;

fn private() -> PathBuf {
    let root =
        PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").expect("private runner required"));
    for name in ["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XAUTHORITY"] {
        assert!(PathBuf::from(std::env::var_os(name).unwrap()).starts_with(&root));
    }
    assert_ne!(
        fs::read_link("/proc/self/ns/net")
            .unwrap()
            .to_str()
            .unwrap(),
        std::env::var("MLUVA_HOST_NET_NS").unwrap()
    );
    for device in ["/dev/input", "/dev/uinput", "/dev/snd", "/dev/dri"] {
        assert!(!Path::new(device).exists());
    }
    assert!(std::env::var_os("WAYLAND_DISPLAY").is_none());
    root
}
fn read(path: impl AsRef<Path>) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn write(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}
#[track_caller]
fn until(mut predicate: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(8);
    while !predicate() {
        assert!(Instant::now() < end, "annotation process did not settle");
        thread::sleep(Duration::from_millis(3));
    }
}
fn staging() -> BTreeSet<PathBuf> {
    fs::read_dir("/dev/shm")
        .unwrap()
        .map(Result::unwrap)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("mluva-audio-")
        })
        .map(|entry| entry.path())
        .collect()
}
fn children(pid: u32) -> BTreeSet<u32> {
    fs::read_dir(format!("/proc/{pid}/task"))
        .unwrap()
        .map(Result::unwrap)
        .flat_map(|entry| {
            fs::read_to_string(entry.path().join("children"))
                .unwrap_or_default()
                .split_whitespace()
                .map(|pid| pid.parse().unwrap())
                .collect::<Vec<u32>>()
        })
        .collect()
}
fn alive(pid: u32) -> bool {
    fs::read_to_string(format!("/proc/{pid}/stat"))
        .is_ok_and(|stat| stat.rsplit_once(')').unwrap().1.split_whitespace().next() != Some("Z"))
}
fn clipboard(set: Option<&str>) -> String {
    let mut child = Command::new("xclip")
        .args([
            "-selection",
            "clipboard",
            if set.is_some() { "-in" } else { "-out" },
        ])
        .stdin(if set.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(if set.is_some() {
            Stdio::null()
        } else {
            Stdio::piped()
        })
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    if let Some(text) = set {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(text.as_bytes())
            .unwrap();
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}
fn output(bytes: &[u8]) -> Value {
    let mut hash = Command::new("sha256sum")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    hash.stdin.take().unwrap().write_all(bytes).unwrap();
    let hash = hash.wait_with_output().unwrap();
    assert!(hash.status.success());
    let text = std::str::from_utf8(bytes).unwrap();
    json!({"bytes":bytes.len(),"characters":text.chars().count(),"sha256":String::from_utf8(hash.stdout).unwrap().split_whitespace().next().unwrap(),"prefix":text.chars().take(80).collect::<String>(),"suffix":text.chars().skip(text.chars().count().saturating_sub(80)).collect::<String>()})
}
fn data_files(root: &Path) -> Vec<String> {
    fn walk(path: &Path, root: &Path, result: &mut Vec<String>) {
        if path.is_dir() {
            for entry in fs::read_dir(path).unwrap() {
                walk(&entry.unwrap().path(), root, result);
            }
        } else if path.is_file() {
            result.push(
                path.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    let mut result = vec![];
    walk(root, root, &mut result);
    result.sort();
    result
}

struct Helper {
    child: Child,
    input: Option<ChildStdin>,
    stdout: Option<JoinHandle<Vec<u8>>>,
    stderr: Option<JoinHandle<Vec<u8>>>,
}
impl Helper {
    fn launch(root: &Path, executable: &Path) -> Self {
        let mut child = Command::new(executable)
            .args(std::env::var_os("MLUVA_TEST_APP_NARRATION").map(|_| "--narrate"))
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("DBUS_SESSION_BUS_ADDRESS")
            .env_remove("AT_SPI_BUS_ADDRESS")
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    root.join("tools").display(),
                    std::env::var("PATH").unwrap()
                ),
            )
            .env("XDG_CONFIG_HOME", root.join("config"))
            .env("XDG_DATA_HOME", root.join("data"))
            .env("XDG_RUNTIME_DIR", root.join("runtime"))
            .env("TZ", "UTC")
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        fn drain(pipe: impl Read + Send + 'static) -> JoinHandle<Vec<u8>> {
            thread::spawn(move || {
                let mut bytes = vec![];
                pipe.take(600_000).read_to_end(&mut bytes).unwrap();
                bytes
            })
        }
        let stdout = Some(drain(child.stdout.take().unwrap()));
        let stderr = Some(drain(child.stderr.take().unwrap()));
        Self {
            input: child.stdin.take(),
            child,
            stdout,
            stderr,
        }
    }
    fn signal(&self, signal: i32, group: bool) {
        assert_eq!(
            unsafe {
                libc::kill(
                    if group {
                        -(self.child.id() as i32)
                    } else {
                        self.child.id() as i32
                    },
                    signal,
                )
            },
            0
        );
    }
    fn control(&mut self, bytes: &[u8], keep_open: bool) {
        if let Err(error) = self.input.as_mut().unwrap().write_all(bytes) {
            assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
        }
        if !keep_open {
            self.input.take();
        }
    }
    fn finish(&mut self) -> (i32, Vec<u8>, String) {
        until(|| self.child.try_wait().unwrap().is_some());
        self.input.take();
        let status = self.child.wait().unwrap();
        (
            status.code().unwrap_or_else(|| -status.signal().unwrap()),
            self.stdout.take().unwrap().join().unwrap(),
            String::from_utf8(self.stderr.take().unwrap().join().unwrap()).unwrap(),
        )
    }
}
impl Drop for Helper {
    fn drop(&mut self) {
        if self.child.try_wait().unwrap().is_none() {
            unsafe {
                libc::kill(-(self.child.id() as i32), libc::SIGKILL);
            }
            let _ = self.child.wait();
        }
    }
}

struct Case {
    directory: tempfile::TempDir,
    server: http::Peer,
    config: Value,
    before: BTreeSet<PathBuf>,
}
enum Fault {
    None,
    Finalizing,
    Uploading,
}
impl Case {
    fn new(row: &Value, fault: Fault) -> Self {
        let directory = tempfile::Builder::new()
            .prefix("native-annotation-")
            .tempdir_in(private())
            .unwrap();
        let root = directory.path();
        fs::create_dir(root.join("tools")).unwrap();
        fs::create_dir_all(root.join("config/mluva")).unwrap();
        symlink(
            env!("CARGO_BIN_EXE_meeting-audio-fixture-peer"),
            root.join("tools/pw-record"),
        )
        .unwrap();
        let mut pcm = row["pcm"].clone();
        if matches!(fault, Fault::Finalizing) {
            pcm["finalize_delay_ms"] = json!(500);
            pcm["signal_receipt"] = json!(root.join("microphone-signal"));
        }
        write(&root.join("tools/test-config.json"), &pcm);
        let mut responses = row["responses"].as_array().unwrap().clone();
        for response in &mut responses {
            if let Some(count) = response["payload"]["text"]["characters"].as_u64() {
                response["payload"]["text"] = json!(
                    response["payload"]["text"]["repeat"]
                        .as_str()
                        .unwrap()
                        .repeat(count as usize)
                );
            }
            if matches!(fault, Fault::Uploading) {
                response["wait_for_file"] = json!(root.join("provider-release"));
                response["incoming_file"] = json!(root.join("provider-incoming"));
                response["allow_disconnect"] = json!(true);
            }
        }
        let server = http::Peer::new(&responses);
        let mut config = row["config"].clone();
        config["transcription_base_url"] = json!(format!("{}/v1", server.address));
        write(&root.join("config/mluva/config.json"), &config);
        write(
            &root.join("config/mluva/personalization.json"),
            &row["personalization"],
        );
        clipboard(Some("untouched annotation clipboard"));
        Self {
            directory,
            server,
            config,
            before: staging(),
        }
    }
    fn root(&self) -> &Path {
        self.directory.path()
    }
    fn recording(&self, helper: &mut Helper) -> (Value, Value, BTreeSet<PathBuf>, BTreeSet<u32>) {
        let ready = self.root().join("tools/raw.ready.json");
        until(|| ready.exists() || helper.child.try_wait().unwrap().is_some());
        assert!(
            ready.exists(),
            "helper exited before recording: {:?}",
            helper.finish()
        );
        let staged: BTreeSet<_> = staging().difference(&self.before).cloned().collect();
        assert_eq!(staged.len(), 1);
        let path = staged.first().unwrap();
        let memory = json!({"directory_mode":path.metadata().unwrap().permissions().mode() & 0o777,"audio_mode":path.join("annotation.wav").metadata().unwrap().permissions().mode() & 0o777,"name":"annotation.wav"});
        (
            read(ready)["argv"].clone(),
            memory,
            staged,
            children(helper.child.id()),
        )
    }
}

#[test]
#[ignore = "requires private network/PID/devices, X11 clipboard and native process peers"]
fn released_annotation_cli_and_owned_cancellation() {
    // Exercise the same external protocol through either native entry point.
    let executable = std::env::var_os("MLUVA_TEST_APP_NARRATION")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_mluva-narrate")));
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-annotation-cli.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f"
    );
    for row in fixture["cases"].as_array().unwrap() {
        let params = &row["params"];
        eprintln!("annotation case: {}", params["name"]);
        let mut case = Case::new(row, Fault::None);
        let mut helper = Helper::launch(case.root(), &executable);
        let (microphone, memory, staged, owned) = if params["no_audio"] == true {
            (Value::Null, Value::Null, BTreeSet::new(), BTreeSet::new())
        } else {
            case.recording(&mut helper)
        };
        if params["privacy"] == true {
            case.config["incognito_mode"] = json!(true);
        }
        if params["mutate_route"] == true {
            case.config["language_code"] = json!("ces");
            case.config["transcription_base_url"] = json!("http://127.0.0.1:1");
            case.config["transcription_remote_model"] = json!("must-not-be-used");
        }
        write(&case.root().join("config/mluva/config.json"), &case.config);
        if params["damage_config"] == true {
            fs::write(case.root().join("config/mluva/config.json"), b"{incomplete").unwrap();
        }
        if params["mutate_personal"] == true {
            let mut changed = row["personalization"].clone();
            changed["dictionary"][0]["written"] = json!("Wrong late rule");
            changed["snippets"][0]["expansion"] = json!("Wrong late signature");
            write(
                &case.root().join("config/mluva/personalization.json"),
                &changed,
            );
        }
        if let Some(signal) = params["signal"].as_str() {
            helper.signal(
                match signal {
                    "SIGTERM" => libc::SIGTERM,
                    "SIGINT" => libc::SIGINT,
                    "SIGKILL" => libc::SIGKILL,
                    _ => panic!("unknown signal"),
                },
                params["kill_group"] == true,
            );
        } else {
            let bytes = params["control_hex"]
                .as_str()
                .map(|hex| {
                    (0..hex.len())
                        .step_by(2)
                        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                        .collect()
                })
                .unwrap_or_else(|| params["control"].as_str().unwrap_or("").as_bytes().to_vec());
            helper.control(&bytes, params["keep_open"] == true);
        }
        let (exit, stdout, stderr) = helper.finish();
        until(|| staged.iter().all(|path| !path.exists()) && owned.iter().all(|pid| !alive(*pid)));
        let actual = json!({"exit":exit,"stdout":output(&stdout),"stderr":stderr,"clipboard":clipboard(None),"requests":case.server.finish(),"memory":memory,"staging_gone":staged.iter().all(|path| !path.exists()),"children_gone":owned.iter().all(|pid| !alive(*pid)),"microphone":microphone,"data_files":data_files(&case.root().join("data"))});
        assert_eq!(actual, row["result"], "{}", params["name"]);
        assert_eq!(
            staging(),
            case.before,
            "no unobserved memory store may survive"
        );
    }
    eprintln!(
        "matched {} released annotation CLI cases",
        fixture["cases"].as_array().unwrap().len()
    );
    // An actual delayed microphone finalization holds Stop in the recorder
    // thread. A signal must drain it and suppress the later upload.
    let mut stop_row = fixture["cases"][0].clone();
    stop_row["responses"] = json!([]);
    let mut case = Case::new(&stop_row, Fault::Finalizing);
    let mut helper = Helper::launch(case.root(), &executable);
    let (_, _, staged, owned) = case.recording(&mut helper);
    helper.control(b"stop\n", false);
    until(|| case.root().join("microphone-signal").exists());
    assert_eq!(
        fs::read_to_string(case.root().join("microphone-signal")).unwrap(),
        libc::SIGINT.to_string()
    );
    assert!(helper.child.try_wait().unwrap().is_none());
    helper.signal(libc::SIGTERM, false);
    assert_eq!(
        helper.finish(),
        (
            1,
            vec![],
            format!(
                "{}\n",
                "Annotation could not finish. Check Mluva's microphone and speech provider, then try again."
            )
        )
    );
    until(|| staged.iter().all(|path| !path.exists()) && owned.iter().all(|pid| !alive(*pid)));
    assert!(case.server.finish().is_empty());
    assert!(data_files(&case.root().join("data")).is_empty());

    // Keep a real provider response pending after observing the uploaded WAV.
    // Cancellation must finish without waiting for the provider's response.
    let mut case = Case::new(&fixture["cases"][0], Fault::Uploading);
    let mut helper = Helper::launch(case.root(), &executable);
    let (_, _, staged, owned) = case.recording(&mut helper);
    helper.control(b"stop\n", false);
    until(|| case.root().join("provider-incoming").exists());
    helper.signal(libc::SIGTERM, false);
    let (exit, stdout, stderr) = helper.finish();
    assert_eq!(exit, 1);
    assert!(stdout.is_empty());
    assert_eq!(
        stderr,
        "Annotation could not finish. Check Mluva's microphone and speech provider, then try again.\n"
    );
    until(|| staged.iter().all(|path| !path.exists()) && owned.iter().all(|pid| !alive(*pid)));
    fs::write(case.root().join("provider-release"), b"release").unwrap();
    assert_eq!(case.server.finish().len(), 1);
    assert!(data_files(&case.root().join("data")).is_empty());
    assert_eq!(clipboard(None), "untouched annotation clipboard");

    // Exercise packaging failure before microphone launch, using the production
    // executable from a directory without its adjacent memory janitor.
    let mut case = Case::new(&stop_row, Fault::None);
    let copied = case.root().join("mluva-narrate");
    fs::copy(&executable, &copied).unwrap();
    let mut helper = Helper::launch(case.root(), &copied);
    let (exit, stdout, stderr) = helper.finish();
    assert_eq!(exit, 1);
    assert!(stdout.is_empty());
    assert_eq!(
        stderr,
        "Annotation could not finish. Check Mluva's microphone and speech provider, then try again.\n"
    );
    assert!(!case.root().join("tools/raw.ready.json").exists());
    assert_eq!(staging(), case.before);
    assert!(case.server.finish().is_empty());
    eprintln!("passed 3 native annotation ownership faults");
}
