//! Compare relocated, independent application processes with the frozen release
//! through public D-Bus, accessibility and private X11 interfaces.
use glib::variant::ToVariant;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    os::unix::{
        fs::{PermissionsExt, symlink},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    rc::Rc,
    thread,
    time::{Duration, Instant},
};
#[path = "support/accessibility.rs"]
#[allow(dead_code)]
mod accessibility;
#[path = "../../mluva-workflows/tests/support/http.rs"]
mod http;
#[path = "support/bootstrap_managed_capture.rs"]
mod managed_capture;
#[path = "support/bootstrap_rewrite.rs"]
mod rewrite;
use accessibility::Accessibility;

const NAME: &str = "com.mluva.Linux";
const OBJECT: &str = "/com/mluva/Linux";

fn hash(bytes: &[u8]) -> String {
    glib::compute_checksum_for_data(glib::ChecksumType::Sha256, bytes)
        .unwrap()
        .into()
}

fn settle() {
    let end = Instant::now() + Duration::from_millis(250);
    while Instant::now() < end {
        while glib::MainContext::default().pending() {
            glib::MainContext::default().iteration(false);
        }
        thread::sleep(Duration::from_millis(5));
    }
}
#[track_caller]
fn until(mut ready: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(8);
    while !ready() {
        assert!(Instant::now() < end, "application did not settle");
        while glib::MainContext::default().pending() {
            glib::MainContext::default().iteration(false);
        }
        thread::sleep(Duration::from_millis(5));
    }
}
fn visible(pid: u32) -> bool {
    Command::new("xdotool")
        .args(["search", "--onlyvisible", "--pid", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .success()
}
fn key(chord: &str) {
    assert!(
        Command::new("xdotool")
            .args(["key", "--clearmodifiers", chord])
            .status()
            .unwrap()
            .success()
    );
}
fn variant(value: &glib::Variant) -> Value {
    match value.type_().as_str() {
        "s" | "o" | "g" => json!(value.str().unwrap()),
        "b" => json!(value.get::<bool>().unwrap()),
        "u" => json!(value.get::<u32>().unwrap()),
        "d" => json!(value.get::<f64>().unwrap()),
        "v" => variant(&value.as_variant().unwrap()),
        kind if kind.starts_with("a{") => Value::Object(
            (0..value.n_children())
                .map(|index| {
                    let entry = value.child_value(index);
                    (
                        entry.child_value(0).str().unwrap().to_owned(),
                        variant(&entry.child_value(1)),
                    )
                })
                .collect(),
        ),
        kind if kind.starts_with(['a', '(']) => Value::Array(
            (0..value.n_children())
                .map(|index| variant(&value.child_value(index)))
                .collect(),
        ),
        other => panic!("unexpected public protocol type {other}"),
    }
}
struct Bus(gio::DBusConnection);
impl Bus {
    fn call(
        &self,
        destination: &str,
        object: &str,
        interface: &str,
        method: &str,
        arguments: Option<&glib::Variant>,
    ) -> Result<glib::Variant, glib::Error> {
        self.0.call_sync(
            Some(destination),
            object,
            interface,
            method,
            arguments,
            None,
            gio::DBusCallFlags::NO_AUTO_START,
            1500,
            gio::Cancellable::NONE,
        )
    }
    fn owner(&self) -> Option<String> {
        self.call(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "GetNameOwner",
            Some(&(NAME,).to_variant()),
        )
        .ok()
        .map(|reply| reply.child_value(0).str().unwrap().to_owned())
    }
    fn action(&self, name: &str) {
        self.call(
            &self.owner().unwrap(),
            OBJECT,
            "org.gtk.Actions",
            "Activate",
            Some(
                &(
                    name,
                    Vec::<glib::Variant>::new(),
                    BTreeMap::<String, glib::Variant>::new(),
                )
                    .to_variant(),
            ),
        )
        .unwrap();
    }
}
struct Process(Child);
impl Process {
    fn finish(&mut self) -> i32 {
        until(|| self.0.try_wait().unwrap().is_some());
        self.0.wait().unwrap().code().unwrap()
    }
    fn output(&mut self) -> Value {
        let exit = self.finish();
        let mut stdout = String::new();
        let mut stderr = String::new();
        self.0
            .stdout
            .take()
            .unwrap()
            .read_to_string(&mut stdout)
            .unwrap();
        self.0
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut stderr)
            .unwrap();
        json!({"exit":exit,"stdout":stdout,"stderr":stderr})
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        if self.0.try_wait().unwrap().is_none() {
            // Test failure must not leave a private application or its children
            // running, including when the normal quit contract is broken.
            unsafe { libc::kill(-(self.0.id() as i32), libc::SIGKILL) };
            let _ = self.0.wait();
        }
    }
}
fn copy_tree(source: &Path, target: &Path) {
    fs::create_dir_all(target).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let destination = target.join(entry.file_name());
        if entry.file_type().unwrap().is_symlink() {
            std::os::unix::fs::symlink(fs::read_link(entry.path()).unwrap(), destination).unwrap();
        } else if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &destination);
        } else {
            fs::copy(entry.path(), destination).unwrap();
        }
    }
}
fn data_files(root: &Path) -> Vec<String> {
    fn walk(path: &Path, root: &Path, result: &mut Vec<String>) {
        if path.is_dir() {
            for entry in fs::read_dir(path).unwrap() {
                walk(&entry.unwrap().path(), root, result);
            }
        } else if path.is_file() {
            result.push(path.strip_prefix(root).unwrap().to_str().unwrap().into());
        }
    }
    let mut files = vec![];
    walk(root, root, &mut files);
    files.sort();
    files
}
fn application(binary: &Path, root: &Path) -> Command {
    let mut command = Command::new(binary);
    command
        .current_dir(root)
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_RUNTIME_DIR", root.join("runtime"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("TZ", "UTC")
        .env("GSETTINGS_BACKEND", "memory")
        .env("MLUVA_DISABLE_GLOBAL_SHORTCUT", "1")
        .env_remove("PYTHONPATH")
        .env_remove("VIRTUAL_ENV")
        .process_group(0)
        .stdin(Stdio::null());
    command
}

fn alive(pid: u32) -> bool {
    fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|stat| {
            stat.rsplit_once(')')
                .map(|(_, tail)| tail.trim_start().starts_with('Z'))
        })
        == Some(false)
}
fn children(pid: u32) -> Vec<u32> {
    fs::read_dir(format!("/proc/{pid}/task"))
        .unwrap()
        .flat_map(|task| {
            fs::read_to_string(task.unwrap().path().join("children"))
                .unwrap()
                .split_whitespace()
                .map(|pid| pid.parse().unwrap())
                .collect::<Vec<_>>()
        })
        .collect()
}
fn cold_recording(
    binary: &Path,
    root: &Path,
    bus: &Bus,
    accessibility: &Accessibility,
    events: &RefCell<Vec<Value>>,
) {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-bootstrap-record.json")).unwrap();
    let directory = root.join("cold-record");
    fs::create_dir_all(directory.join("config/mluva")).unwrap();
    fs::create_dir(directory.join("tools")).unwrap();
    let binaries = Path::new(env!("CARGO_BIN_EXE_mluva")).parent().unwrap();
    symlink(
        binaries.join("audio-fixture-peer"),
        directory.join("tools/pw-record"),
    )
    .unwrap();
    fs::write(
        directory.join("tools/test-config.json"),
        serde_json::to_vec(&fixture["pcm"]).unwrap(),
    )
    .unwrap();
    let mut server = http::Peer::new(&[]);
    let mut config = fixture["config"].clone();
    config["transcription_base_url"] = json!(format!("{}/v1", server.address));
    fs::write(
        directory.join("config/mluva/config.json"),
        serde_json::to_vec(&config).unwrap(),
    )
    .unwrap();
    let mut clip = Command::new("xclip")
        .args(["-selection", "clipboard", "-in"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    clip.stdin
        .take()
        .unwrap()
        .write_all(b"untouched startup clipboard")
        .unwrap();
    assert!(clip.wait().unwrap().success());
    events.borrow_mut().clear();
    let mut process = Process(
        rewrite::source_application(binary, &directory)
            .arg("--gapplication-service")
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    directory.join("tools").display(),
                    std::env::var("PATH").unwrap()
                ),
            )
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    until(|| bus.owner().is_some());
    assert!(!visible(process.0.id()));
    bus.action("record");
    let ready = directory.join("tools/raw.ready.json");
    until(|| ready.exists());
    settle();
    settle();
    let (mut names, items) = accessibility.visible_content();
    let elapsed = regex::Regex::new(r"^Recording [0-9]+:[0-9]+$").unwrap();
    for (_, name) in &mut names {
        if elapsed.is_match(name) {
            *name = "Recording $SAMPLED".into();
        }
    }
    let observed = events.borrow().clone();
    let mut phases: Vec<_> = observed
        .iter()
        .filter(|event| event["name"] == "StateChanged")
        .map(|event| event["values"][1].clone())
        .collect();
    phases.dedup();
    let mut signals = observed[observed.len() - 2..].to_vec();
    assert_eq!(signals[0]["values"][1], "recording");
    assert_eq!(signals[0]["values"][2], "Recording");
    signals[0]["values"][3] = json!("$SAMPLED");
    signals[0]["values"][6] = json!("$SAMPLED");
    signals[1]["values"][0]["elapsed"] = json!("$SAMPLED");
    signals[1]["values"][0]["level"] = json!("$SAMPLED");
    let state =
        json!({"visible":visible(process.0.id()),"names":names,"items":items,"signals":signals});
    let microphone: Value = serde_json::from_slice(&fs::read(ready).unwrap()).unwrap();
    let owned = children(process.0.id());
    assert!(owned.contains(&(microphone["pid"].as_u64().unwrap() as u32)));
    bus.action("quit");
    let output = process.output();
    assert_eq!(output["stdout"], "");
    assert_eq!(output["stderr"], "");
    until(|| bus.owner().is_none() && owned.iter().all(|pid| !alive(*pid)));
    let rows: i64 = rusqlite::Connection::open(directory.join("data/mluva/history.sqlite3"))
        .unwrap()
        .query_row("select count(*) from transcription_history", [], |row| {
            row.get(0)
        })
        .unwrap();
    let clip = Command::new("xclip")
        .args(["-selection", "clipboard", "-out"])
        .output()
        .unwrap();
    assert!(clip.status.success());
    let result = json!({"state":state,"phases":phases,"microphone":microphone["argv"],"exit":output["exit"],"children_gone":owned.iter().all(|pid| !alive(*pid)),"audio_files":data_files(&directory.join("data/mluva/recordings")),"history_rows":rows,"clipboard":String::from_utf8(clip.stdout).unwrap(),"requests":server.finish()});
    fs::write(
        directory.join("observed.json"),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
    assert_eq!(result, rewrite::source_snapshot(binary, &fixture["result"]));
    eprintln!("matched first cold Record through actual microphone and quit cleanup");
}

fn startup_faults(
    binary: &Path,
    root: &Path,
    bus: &Bus,
    accessibility: &Accessibility,
    events: &RefCell<Vec<Value>>,
) {
    for command in ["cancel", "record"] {
        let directory = root.join(format!("{command}-pending-startup"));
        fs::create_dir_all(directory.join("config/mluva")).unwrap();
        fs::create_dir(directory.join("tools")).unwrap();
        let binaries = Path::new(env!("CARGO_BIN_EXE_mluva")).parent().unwrap();
        symlink(
            binaries.join("credential-fixture-peer"),
            directory.join("tools/secret-tool"),
        )
        .unwrap();
        symlink(
            binaries.join("audio-fixture-peer"),
            directory.join("tools/pw-record"),
        )
        .unwrap();
        fs::write(
            directory.join("keyring.json"),
            br#"{"lookup":{"sleep_ms":750,"stdout":"synthetic-startup-key\n"}}"#,
        )
        .unwrap();
        fs::write(
            directory.join("tools/test-config.json"),
            br#"{"wait":true,"pcm_hex":"0000"}"#,
        )
        .unwrap();
        fs::write(directory.join("config/mluva/config.json"), br#"{"transcription_provider":"elevenlabs","rewrite_provider":"none","welcome_completed":true,"automatic_titles":false}"#).unwrap();
        events.borrow_mut().clear();
        let mut process = Process(
            rewrite::source_application(binary, &directory)
                .arg("--gapplication-service")
                .env(
                    "PATH",
                    format!(
                        "{}:{}",
                        directory.join("tools").display(),
                        std::env::var("PATH").unwrap()
                    ),
                )
                .env("CREDENTIAL_FIXTURE_ROOT", &directory)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        until(|| bus.owner().is_some());
        bus.action("record");
        until(|| directory.join("active.pid").exists());
        let keyring = fs::read_to_string(directory.join("active.pid"))
            .unwrap()
            .parse()
            .unwrap();
        assert!(alive(keyring));
        bus.action(command);
        // Wait for public capture readiness, not just for the credential child to
        // exit, so a lost cancellation cannot start the microphone afterwards.
        until(|| {
            accessibility
                .button("Dictate")
                .and_then(|node| {
                    accessibility.call(&node, "org.a11y.atspi.Accessible", "GetState", None)
                })
                .and_then(|reply| reply.child_value(0).get::<Vec<u32>>())
                .is_some_and(|state| state[0] & (1 << 24) != 0)
        });
        settle();
        assert!(!directory.join("tools/raw.ready.json").exists());
        assert!(!alive(keyring));
        assert!(
            events
                .borrow()
                .iter()
                .filter(|event| event["name"] == "StateChanged")
                .all(|event| event["values"][1] == "hidden")
        );
        bus.action("quit");
        assert_eq!(process.output(), json!({"exit":0,"stdout":"","stderr":""}));
        until(|| bus.owner().is_none());
    }

    let bare = root.join("missing-resources");
    fs::create_dir_all(bare.join("bin")).unwrap();
    fs::copy(binary, bare.join("bin/mluva")).unwrap();
    let mut process = Process(
        application(&bare.join("bin/mluva"), &bare)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    assert_eq!(
        process.output(),
        json!({"exit":1,"stdout":"","stderr":"Mluva could not start. Check its installed resources and local data.\n"})
    );
    until(|| bus.owner().is_none());
    assert!(data_files(&bare.join("data")).is_empty());
    eprintln!("passed native pending-startup cancel/repeat-toggle and missing-resource cleanup");
}

fn private_session() -> PathBuf {
    let private = PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").unwrap());
    for variable in ["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XAUTHORITY"] {
        assert!(PathBuf::from(std::env::var_os(variable).unwrap()).starts_with(&private));
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
    assert_eq!(std::env::var("GDK_BACKEND").unwrap(), "x11");
    assert!(std::env::var_os("WAYLAND_DISPLAY").is_none());
    private
}

#[test]
#[ignore = "builds and runs actual source Make commands in the guarded disposable desktop"]
fn source_make_build_and_run_without_python() {
    let private = private_session();
    let root = private.join("source-make");
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    for directory in ["bin", "config/mluva", "home"] {
        fs::create_dir_all(root.join(directory)).unwrap();
    }
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/released-bootstrap.json")).unwrap();
    let case = reference["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "normal-hide-reopen")
        .unwrap();
    fs::write(
        root.join("config/mluva/config.json"),
        serde_json::to_vec(&case["config"]).unwrap(),
    )
    .unwrap();
    fs::write(
        root.join("home/personal-note"),
        b"keep unrelated user bytes\n",
    )
    .unwrap();
    for name in ["python3", "uv"] {
        let path = root.join("bin").join(name);
        fs::write(&path, "#!/usr/bin/bash\nprintf '%s\\n' \"${0##*/}\" >> \"$MLUVA_SOURCE_INTERPRETER_TRAP\"\nexit 99\n").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let invoke = || {
        let mut command = application(Path::new("/usr/bin/bwrap"), &root);
        command
            .args([
                "--die-with-parent",
                "--bind",
                "/",
                "/",
                "--dev",
                "/dev",
                "--ro-bind",
            ])
            .arg(root.join("bin/python3"))
            .args(["/usr/bin/python3", "--"])
            .env("HOME", root.join("home"))
            .env(
                "PATH",
                format!(
                    "{}:{}:/usr/bin:/bin",
                    root.join("bin").display(),
                    Path::new(env!("CARGO")).parent().unwrap().display()
                ),
            )
            .env("CARGO_NET_OFFLINE", "true")
            .env("CARGO_BUILD_JOBS", "2")
            .env(
                "MLUVA_SOURCE_INTERPRETER_TRAP",
                root.join("interpreter-used"),
            );
        command
    };
    for program in ["/usr/bin/python3", "uv"] {
        let output = invoke().arg(program).output().unwrap();
        assert_eq!(output.status.code(), Some(99));
        assert!(root.join("interpreter-used").exists());
        fs::remove_file(root.join("interpreter-used")).unwrap();
    }
    let make = |target: &str| {
        let mut command = invoke();
        command
            .args(["/usr/bin/make", "--no-print-directory", "-s", "-C"])
            .arg(repository)
            .arg(target);
        command
    };
    let setup = make("linux-setup").output().unwrap();
    fs::write(root.join("setup.stdout"), &setup.stdout).unwrap();
    fs::write(root.join("setup.stderr"), &setup.stderr).unwrap();
    assert!(
        setup.status.success(),
        "native Make setup failed: {:?}: {}",
        setup.status.code(),
        String::from_utf8_lossy(&setup.stderr)
    );
    assert!(!root.join("interpreter-used").exists());
    assert_eq!(data_files(&root.join("home")), ["personal-note"]);
    let bus = Bus(gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).unwrap());
    assert!(bus.owner().is_none());
    let accessibility = Accessibility::open();
    let log = fs::File::create(root.join("run.log")).unwrap();
    let mut process = Process(
        make("run")
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap(),
    );
    until(|| bus.owner().is_some() || process.0.try_wait().unwrap().is_some());
    assert!(
        process.0.try_wait().unwrap().is_none(),
        "source Make run exited before registration"
    );
    let owner = bus.owner().unwrap();
    let pid = bus
        .call(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "GetConnectionUnixProcessID",
            Some(&(owner.as_str(),).to_variant()),
        )
        .unwrap()
        .get::<(u32,)>()
        .unwrap()
        .0;
    let executable = fs::read_link(format!("/proc/{pid}/exe")).unwrap();
    let bundle = executable.parent().unwrap().parent().unwrap().to_owned();
    let receipt: Value =
        serde_json::from_slice(&fs::read(bundle.join(".mluva-native.json")).unwrap()).unwrap();
    assert_eq!(receipt["implementation"], "rust");
    until(|| visible(pid));
    let mut content = accessibility.visible_content();
    until(|| {
        content = accessibility.visible_content();
        !content.0.is_empty()
    });
    let expected =
        &reference["snapshots"][case["states"][0]["snapshot"].as_u64().unwrap() as usize];
    if rewrite::disclosure(&executable) {
        rewrite::activate(&accessibility);
        content = accessibility.visible_content();
    }
    assert_eq!(
        json!(content.0),
        rewrite::source_snapshot(&executable, expected)["names"]
    );
    assert_eq!(json!(content.1), expected["items"]);
    let actions = bus
        .call(&owner, OBJECT, "org.gtk.Actions", "DescribeAll", None)
        .unwrap();
    assert_eq!(variant(&actions.child_value(0)), case["actions"]);
    let secondary = make("run").output().unwrap();
    fs::write(root.join("second.stdout"), &secondary.stdout).unwrap();
    fs::write(root.join("second.stderr"), &secondary.stderr).unwrap();
    assert!(secondary.status.success());
    assert_eq!(bus.owner().as_deref(), Some(owner.as_str()));
    bus.action("quit");
    assert_eq!(process.finish(), 0);
    until(|| bus.owner().is_none() && !alive(pid));
    assert!(
        !bundle.exists(),
        "source runtime survived its owned process"
    );
    assert_eq!(
        fs::read(root.join("home/personal-note")).unwrap(),
        b"keep unrelated user bytes\n"
    );
    assert_eq!(data_files(&root.join("home")), ["personal-note"]);
    assert!(!root.join("interpreter-used").exists());
    eprintln!(
        "Actual Make setup/run/forwarding/quit preserved released UI/actions and cleaned its runtime without Python or uv"
    );
}

#[test]
#[ignore = "requires private X11/session/accessibility, native peers and pinned MLUVA_TEST_QWEN_PCM"]
fn released_process_actions_residency_and_headless_dispatch() {
    let private = private_session();
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-bootstrap.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f"
    );
    let root = private.join("native-bootstrap");
    let bundle = root.join("relocated");
    fs::create_dir_all(bundle.join("bin")).unwrap();
    let binary = bundle.join("bin/mluva");
    if let Some(packaged) = std::env::var_os("MLUVA_TEST_NATIVE_BUNDLE") {
        copy_tree(Path::new(&packaged), &bundle);
    } else {
        let executable = Path::new(env!("CARGO_BIN_EXE_mluva"));
        fs::copy(executable, &binary).unwrap();
        fs::copy(
            executable.with_file_name("mluva-audio-cleanup"),
            bundle.join("bin/mluva-audio-cleanup"),
        )
        .unwrap();
        copy_tree(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("resources"),
            &bundle.join("resources"),
        );
    }
    let bus = Bus(gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).unwrap());
    assert!(bus.owner().is_none());
    let accessibility = Accessibility::open();
    let events = Rc::new(RefCell::new(Vec::<Value>::new()));
    let observed = events.clone();
    let subscription = bus.0.subscribe_to_signal(
        None,
        Some("com.mluva.Linux.RecordingStatus"),
        None,
        Some("/com/mluva/Linux/RecordingStatus"),
        None,
        gio::DBusSignalFlags::NONE,
        move |signal| {
            observed
                .borrow_mut()
                .push(json!({"name":signal.signal_name,"values":variant(signal.parameters)}));
        },
    );
    bus.0.flush_sync(gio::Cancellable::NONE).unwrap();
    let meeting_only = std::env::var_os("MLUVA_TEST_MEETING_ONLY").is_some();
    let live_only = std::env::var_os("MLUVA_TEST_LIVE_ONLY").is_some();
    assert!(
        !(meeting_only && live_only),
        "choose one focused application flow"
    );
    if meeting_only || live_only {
        managed_capture::exercise(&binary, &root, &bus, &events, meeting_only);
        return;
    }
    rewrite::exercise(
        &binary,
        &root,
        &bus,
        &accessibility,
        &fixture["cases"][0]["config"],
    );
    for case in fixture["cases"].as_array().unwrap() {
        events.borrow_mut().clear();
        let name = case["name"].as_str().unwrap();
        let directory = root.join(name);
        fs::create_dir_all(directory.join("config/mluva")).unwrap();
        fs::write(
            directory.join("config/mluva/config.json"),
            serde_json::to_vec(&case["config"]).unwrap(),
        )
        .unwrap();
        let log = fs::File::create(directory.join("application.log")).unwrap();
        let flags: Vec<_> = case["flags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let mut process = Process(
            rewrite::source_application(&binary, &directory)
                .args(&flags)
                .stdout(log.try_clone().unwrap())
                .stderr(log)
                .spawn()
                .unwrap(),
        );
        until(|| bus.owner().is_some() || process.0.try_wait().unwrap().is_some());
        assert!(
            process.0.try_wait().unwrap().is_none(),
            "{name} exited at startup"
        );
        let actions = bus
            .call(
                &bus.owner().unwrap(),
                OBJECT,
                "org.gtk.Actions",
                "DescribeAll",
                None,
            )
            .unwrap();
        assert_eq!(variant(&actions.child_value(0)), case["actions"], "{name}");
        if flags.is_empty() {
            until(|| visible(process.0.id()));
        }
        let mut states = vec![];
        for state in case["states"].as_array().unwrap() {
            let stage = state["stage"].as_str().unwrap();
            match stage {
                "started" => {}
                "hidden" => {
                    key("Escape");
                    settle();
                    key("alt+F4");
                    until(|| !visible(process.0.id()));
                }
                "second-process" => {
                    let first = bus.owner();
                    let secondary = directory.join("second");
                    fs::create_dir(&secondary).unwrap();
                    let output = application(&binary, &secondary)
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped())
                        .spawn()
                        .unwrap();
                    let mut output = Process(output);
                    let mut observed = output.output();
                    observed["same_owner"] = json!(bus.owner() == first);
                    observed["files"] = json!(data_files(&secondary));
                    assert_eq!(observed, case["secondary"], "{name} forwarding");
                    until(|| visible(process.0.id()));
                }
                command => {
                    bus.action(command.strip_prefix("hidden-").unwrap_or(command));
                }
            }
            settle();
            let visible = visible(process.0.id());
            let (names, items) = if visible {
                // X11 mapping and AT-SPI registration are asynchronous. A
                // mapped window alone does not make its accessibility tree
                // observable; wait for content, then compare the full snapshot.
                let mut content = accessibility.visible_content();
                if content.0.is_empty() {
                    eprintln!("{name}: {stage}: awaiting visible accessibility content");
                    until(|| {
                        content = accessibility.visible_content();
                        !content.0.is_empty()
                    });
                }
                content
            } else {
                (vec![], vec![])
            };
            let actual = json!({"visible":visible,"names":names,"items":items,"signals":events.borrow().clone()});
            states.push(json!({"stage":stage,"state":actual}));
            fs::write(
                directory.join("observed.json"),
                serde_json::to_vec_pretty(&states).unwrap(),
            )
            .unwrap();
            let expected = &fixture["snapshots"][state["snapshot"].as_u64().unwrap() as usize];
            assert_eq!(
                actual,
                rewrite::source_snapshot(&binary, expected),
                "{name}: {stage}"
            );
        }
        bus.action("quit");
        assert_eq!(json!(process.finish()), case["exit"], "{name} shutdown");
        until(|| bus.owner().is_none());
        assert!(!visible(process.0.id()));
        // The released cold-service quit accesses an uninitialized scratchpad
        // and logs its traceback. Keep the same clean exit without that bug.
        assert_eq!(
            fs::read_to_string(directory.join("application.log")).unwrap(),
            "",
            "{name}"
        );
        settle();
        eprintln!("matched {name}");
    }
    cold_recording(&binary, &root, &bus, &accessibility, &events);
    managed_capture::exercise(&binary, &root, &bus, &events, false);
    startup_faults(&binary, &root, &bus, &accessibility, &events);
    drop(subscription);
    for (index, case) in fixture["cli"].as_array().unwrap().iter().enumerate() {
        let directory = root.join(format!("cli-{index}"));
        fs::create_dir_all(directory.join("config/mluva")).unwrap();
        fs::write(
            directory.join("config/mluva/config.json"),
            br#"{"incognito_mode":true}"#,
        )
        .unwrap();
        let flags: Vec<_> = case["flags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let mut command = application(&binary, &directory);
        for variable in [
            "DISPLAY",
            "WAYLAND_DISPLAY",
            "DBUS_SESSION_BUS_ADDRESS",
            "AT_SPI_BUS_ADDRESS",
        ] {
            command.env_remove(variable);
        }
        let mut process = Process(
            command
                .args(&flags)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let mut output = process.output();
        output["flags"] = json!(flags);
        assert_eq!(output, *case);
    }
    eprintln!(
        "matched 38 public states, 13 actions, forwarding without secondary stores and 5 headless CLI contracts; evidence {}",
        root.display()
    );
}
