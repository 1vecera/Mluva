//! Observe the actual headless executable through independent session-bus peers.
//! The fixture comes from unchanged v1.6.0 processes, not this implementation.
use glib::variant::ToVariant;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader},
    os::unix::{fs::symlink, process::ExitStatusExt},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    rc::Rc,
    thread,
    time::{Duration, Instant},
};

const NAME: &str = "com.mluva.Linux";
const ACTION: &str = "/com/mluva/Linux";
const OBJECT: &str = "/com/mluva/Linux/RecordingStatus";
const INTERFACE: &str = "com.mluva.Linux.RecordingStatus";
const XML: &str = "<node><interface name='org.gtk.Actions'><method name='Activate'><arg type='s' direction='in'/><arg type='av' direction='in'/><arg type='a{sv}' direction='in'/></method></interface></node>";

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn pump(duration: Duration) {
    let end = Instant::now() + duration;
    loop {
        while glib::MainContext::default().pending() {
            glib::MainContext::default().iteration(false);
        }
        if Instant::now() >= end {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
}
fn until(mut condition: impl FnMut() -> bool) {
    let limit = Instant::now() + Duration::from_secs(6);
    while !condition() {
        assert!(
            Instant::now() < limit,
            "private shell observation timed out"
        );
        pump(Duration::from_millis(3));
    }
}
fn exit(status: ExitStatus) -> i32 {
    status.code().unwrap_or_else(|| -status.signal().unwrap())
}

struct Peer {
    connection: gio::DBusConnection,
    registration: Option<gio::RegistrationId>,
    receipts: Rc<RefCell<Vec<Value>>>,
}
impl Peer {
    fn new(address: &str, replay: Option<glib::Variant>, mode: &str) -> Self {
        let connection = gio::DBusConnection::for_address_sync(
            address,
            gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
                | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
            None,
            gio::Cancellable::NONE,
        )
        .unwrap();
        let receipts = Rc::new(RefCell::new(vec![]));
        let observed = receipts.clone();
        let mode = mode.to_owned();
        let pending = RefCell::new(Vec::new());
        let node = gio::DBusNodeInfo::for_xml(XML).unwrap();
        let registration = connection.register_object(ACTION, &node.interfaces()[0])
            .method_call(move |connection, _, _, _, method, parameters, invocation| {
                assert_eq!(method, "Activate");
                let (action, arguments, platform) = parameters.get::<(String, Vec<glib::Variant>, BTreeMap<String,glib::Variant>)>().unwrap();
                let arguments = arguments.iter().map(|a| json!(a.get::<(String,String,String)>().unwrap())).collect::<Vec<_>>();
                assert!(platform.is_empty());
                let message = invocation.message();
                observed.borrow_mut().push(json!({"action":action,"arguments":arguments,"platform":{},
                    "unique_destination":message.destination()==connection.unique_name(),
                    "no_auto_start":message.flags().contains(gio::DBusMessageFlags::NO_AUTO_START)}));
                match mode.as_str() {
                    "error" => invocation.return_dbus_error("com.mluva.SyntheticFailure", "Synthetic private failure"),
                    "timeout" => pending.borrow_mut().push(invocation),
                    "ok" => {
                        if let Some(replay) = &replay { send(&connection, replay, None, OBJECT, INTERFACE); }
                        invocation.return_value(Some(&().to_variant()));
                    }
                    other => panic!("unknown peer mode {other}"),
                }
            }).build().unwrap();
        Self {
            connection,
            registration: Some(registration),
            receipts,
        }
    }
    fn name(&self, method: &str, arguments: &glib::Variant) -> glib::Variant {
        self.connection
            .call_sync(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
                method,
                Some(arguments),
                None,
                gio::DBusCallFlags::NO_AUTO_START,
                1500,
                gio::Cancellable::NONE,
            )
            .unwrap()
    }
    fn acquire(&self) {
        assert_eq!(
            self.name("RequestName", &(NAME, 7_u32).to_variant())
                .child_get::<u32>(0),
            1
        );
    }
    fn release(&self) {
        self.name("ReleaseName", &(NAME,).to_variant());
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        self.connection
            .unregister_object(self.registration.take().unwrap())
            .unwrap();
        self.connection.close_sync(gio::Cancellable::NONE).unwrap();
    }
}
fn send(
    connection: &gio::DBusConnection,
    value: &glib::Variant,
    signal: Option<&str>,
    path: &str,
    interface: &str,
) {
    connection
        .emit_signal(
            None,
            path,
            interface,
            signal.unwrap_or(if value.type_().as_str() == "(a{sv})" {
                "ShellStateChanged"
            } else {
                "StateChanged"
            }),
            Some(value),
        )
        .unwrap();
    connection.flush_sync(gio::Cancellable::NONE).unwrap();
}
fn command(root: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(root.join("bin/mluva-shell"));
    command
        .args(args)
        .current_dir(root)
        .env("HOME", root.join("home"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("AT_SPI_BUS_ADDRESS")
        .env_remove("PYTHONPATH")
        .env_remove("VIRTUAL_ENV");
    command
}
fn arguments(case: &Value) -> Vec<&str> {
    case["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect()
}
fn run(root: &Path, args: &[&str]) -> (Value, Duration) {
    let stdout = root.join("action.out");
    let stderr = root.join("action.err");
    let start = Instant::now();
    let mut process = Process(
        command(root, args)
            .stdout(fs::File::create(&stdout).unwrap())
            .stderr(fs::File::create(&stderr).unwrap())
            .spawn()
            .unwrap(),
    );
    let mut status = None;
    until(|| {
        status = process.0.try_wait().unwrap();
        status.is_some()
    });
    (
        json!({"exit":exit(status.unwrap()),"stdout":fs::read_to_string(stdout).unwrap(),"stderr":fs::read_to_string(stderr).unwrap()}),
        start.elapsed(),
    )
}
fn lines(path: &Path) -> Vec<Value> {
    let text = fs::read_to_string(path).unwrap();
    text.rfind('\n').map_or_else(Vec::new, |end| {
        text[..=end]
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    })
}
fn observe_step(out: &Path, cursor: &mut usize, step: &Value, action: impl FnOnce()) {
    // The first line can arrive before the observer is scheduled. Keep one
    // stream cursor; never discard startup or late messages between steps.
    let before = *cursor;
    let expected = step["states"].as_array().unwrap();
    action();
    if expected.is_empty() {
        pump(Duration::from_millis(100));
    } else {
        until(|| lines(out).len() >= before + expected.len());
        pump(Duration::from_millis(15));
    }
    assert_eq!(&lines(out)[before..], expected, "{}", step["name"]);
    *cursor += expected.len();
}

#[test]
#[ignore = "requires private network/PID/session-bus and HOME/XDG boundaries"]
fn released_shell_commands_and_owner_lifecycle_match_native_process() {
    let root = PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").unwrap())
        .canonicalize()
        .unwrap();
    for key in ["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME"] {
        assert!(
            PathBuf::from(std::env::var_os(key).unwrap())
                .canonicalize()
                .unwrap()
                .starts_with(&root)
        );
    }
    assert_ne!(
        fs::read_link("/proc/self/ns/net")
            .unwrap()
            .to_str()
            .unwrap(),
        std::env::var("MLUVA_HOST_NET_NS").unwrap()
    );
    for path in ["/dev/input", "/dev/uinput", "/dev/snd", "/dev/dri"] {
        assert!(!Path::new(path).exists());
    }
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    for (name, executable, source) in [
        (
            "native-shell",
            Path::new(env!("CARGO_BIN_EXE_mluva-shell")).to_owned(),
            false,
        ),
        ("source-shell", repository.join("linux/mluva-shell"), true),
    ] {
        compare_process(root.join(name), &executable, source);
    }
}

fn compare_process(root: PathBuf, executable: &Path, source: bool) {
    fs::create_dir(&root).unwrap();
    for directory in ["bin", "home", "config", "data", "state"] {
        fs::create_dir(root.join(directory)).unwrap();
    }
    if source {
        symlink(executable, root.join("bin/mluva-shell")).unwrap();
        // Prepare the source build before measuring bounded D-Bus calls. The
        // command itself is still exercised for every CLI, action and watcher.
        let prepared = command(&root, &["--help"]).output().unwrap();
        fs::write(root.join("build.stdout"), &prepared.stdout).unwrap();
        fs::write(root.join("build.stderr"), &prepared.stderr).unwrap();
        assert!(
            prepared.status.success(),
            "source shell build failed: {:?}: {}",
            prepared.status.code(),
            String::from_utf8_lossy(&prepared.stderr)
        );
    } else {
        fs::copy(executable, root.join("bin/mluva-shell")).unwrap();
    }
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-shell.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f"
    );
    let address = std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap();
    for case in fixture["cli"].as_array().unwrap() {
        let (result, _) = run(&root, &arguments(case));
        assert_eq!(
            result,
            json!({"exit":case["exit"],"stdout":case["stdout"],"stderr":case["stderr"]}),
            "{}",
            case["args"]
        );
    }
    for mode in ["ok", "error", "timeout"] {
        let peer = Peer::new(&address, None, mode);
        peer.acquire();
        for case in fixture["actions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["mode"] == mode)
        {
            let before = peer.receipts.borrow().len();
            let (result, elapsed) = run(&root, &arguments(case));
            assert_eq!(result, case["result"], "{}", case["args"]);
            assert_eq!(
                &peer.receipts.borrow()[before..],
                std::slice::from_ref(&case["receipt"])
            );
            if mode == "timeout" {
                assert!(elapsed >= Duration::from_millis(1400) && elapsed < Duration::from_secs(5));
            }
        }
        peer.release();
    }
    for case in fixture["watches"].as_array().unwrap() {
        let overlay = case["overlay"].as_bool().unwrap();
        let out = root.join(format!("watch-{overlay}.out"));
        let err = root.join(format!("watch-{overlay}.err"));
        let args = if overlay {
            vec!["watch", "--overlay"]
        } else {
            vec!["watch"]
        };
        let mut process = Process(
            command(&root, &args)
                .stdout(fs::File::create(&out).unwrap())
                .stderr(fs::File::create(&err).unwrap())
                .spawn()
                .unwrap(),
        );
        let payloads = fixture["payloads"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["overlay"] == overlay)
            .collect::<Vec<_>>();
        let replay = glib::Variant::parse(None, case["replay"].as_str().unwrap()).unwrap();
        let a = Peer::new(&address, Some(replay.clone()), "ok");
        let b = Peer::new(&address, Some(replay.clone()), "ok");
        let c = Peer::new(&address, None, "error");
        // Deliberately observe late: startup output must not be lost even when
        // the child writes before the first step begins.
        until(|| !lines(&out).is_empty());
        assert_eq!(
            fs::read_link(format!("/proc/{}/exe", process.0.id()))
                .unwrap()
                .file_name()
                .unwrap(),
            "mluva-shell",
            "the command must replace its launcher with the native bridge"
        );
        let mut cursor = 0;
        for step in case["steps"].as_array().unwrap() {
            observe_step(&out, &mut cursor, step, || {
                match step["name"].as_str().unwrap() {
                    "initial-stopped" => {}
                    "first-owner" => a.acquire(),
                    "replacement-owner" => b.acquire(),
                    "wrong-path" => {
                        send(&a.connection, &replay, None, "/com/mluva/Wrong", INTERFACE)
                    }
                    "wrong-interface" => {
                        send(&a.connection, &replay, None, OBJECT, "com.mluva.Wrong")
                    }
                    "wrong-signal" => send(
                        &a.connection,
                        &replay,
                        Some(if overlay {
                            "StateChanged"
                        } else {
                            "ShellStateChanged"
                        }),
                        OBJECT,
                        INTERFACE,
                    ),
                    "stale-sender" => send(&a.connection, &replay, None, OBJECT, INTERFACE),
                    "owner-exit" => {
                        a.release();
                        b.release();
                    }
                    "failed-replay" => c.acquire(),
                    "failed-owner-exit" => c.release(),
                    name => {
                        let payload = payloads.iter().find(|p| p["name"] == name).unwrap();
                        let value =
                            glib::Variant::parse(None, payload["variant"].as_str().unwrap())
                                .unwrap();
                        send(
                            &a.connection,
                            &value,
                            Some(if overlay {
                                "ShellStateChanged"
                            } else {
                                "StateChanged"
                            }),
                            OBJECT,
                            INTERFACE,
                        );
                    }
                }
            });
            assert!(process.0.try_wait().unwrap().is_none());
        }
        let receipts = [&a, &b, &c]
            .iter()
            .flat_map(|p| p.receipts.borrow().clone())
            .collect::<Vec<_>>();
        assert_eq!(json!(receipts), case["receipts"]);
        process.0.kill().unwrap();
        process.0.wait().unwrap();
        assert_eq!(fs::read_to_string(err).unwrap(), "");
        assert!(fs::read_to_string(out).unwrap().is_ascii());
    }
    for case in fixture["disconnects"].as_array().unwrap() {
        let mode = case["mode"].as_str().unwrap();
        let mut daemon = Process(
            Command::new("dbus-daemon")
                .args(["--session", "--nofork", "--print-address=1"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let mut address = String::new();
        BufReader::new(daemon.0.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        assert!(!address.trim().is_empty());
        let err = root.join(format!("{mode}.err"));
        let mut process = Process(
            command(&root, &["watch"])
                .env("DBUS_SESSION_BUS_ADDRESS", address.trim())
                .stdout(Stdio::piped())
                .stderr(fs::File::create(&err).unwrap())
                .spawn()
                .unwrap(),
        );
        let mut stdout = BufReader::new(process.0.stdout.take().unwrap());
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&line).unwrap(),
            json!({"phase":"stopped","elapsed":0})
        );
        let peer = if mode == "broken-pipe" {
            drop(stdout);
            let peer = Peer::new(address.trim(), None, "ok");
            peer.acquire();
            Some(peer)
        } else {
            daemon.0.kill().unwrap();
            daemon.0.wait().unwrap();
            None
        };
        let mut status = None;
        until(|| {
            status = process.0.try_wait().unwrap();
            status.is_some()
        });
        // Deliberate repair: a closed widget pipe exits cleanly instead of Python's
        // finalizer traceback/120. Bus disappearance keeps Gio's released SIGTERM.
        assert_eq!(
            exit(status.unwrap()),
            if mode == "broken-pipe" {
                0
            } else {
                case["exit"].as_i64().unwrap() as i32
            }
        );
        assert_eq!(fs::read_to_string(err).unwrap(), "");
        if let Some(peer) = peer {
            peer.release();
            drop(peer);
        }
    }
    // A real activatable service makes the negative claim falsifiable. The final
    // explicit StartServiceByName is a positive control for this private trap.
    let activation = root.join("activation");
    fs::create_dir(&activation).unwrap();
    let services = activation.join("services");
    fs::create_dir(&services).unwrap();
    let marker = activation.join("started");
    fs::write(
        services.join(format!("{NAME}.service")),
        format!(
            "[D-BUS Service]\nName={NAME}\nExec=/usr/bin/touch \"{}\"\n",
            marker.display()
        ),
    )
    .unwrap();
    let config = activation.join("bus.conf");
    fs::write(&config,format!(
        "<busconfig><type>session</type><listen>unix:abstract=mluva-shell-native-activation-{}</listen><servicedir>{}</servicedir><policy context='default'><allow send_destination='*'/><allow receive_sender='*'/><allow own='*'/></policy></busconfig>",
        std::process::id(),services.display()
    )).unwrap();
    let mut daemon = Process(
        Command::new("dbus-daemon")
            .args(["--nofork", "--print-address=1"])
            .arg(format!("--config-file={}", config.display()))
            .stdout(Stdio::piped())
            .stderr(fs::File::create(activation.join("bus.err")).unwrap())
            .spawn()
            .unwrap(),
    );
    let mut address = String::new();
    BufReader::new(daemon.0.stdout.take().unwrap())
        .read_line(&mut address)
        .unwrap();
    assert!(!address.trim().is_empty());
    let out = activation.join("watch.out");
    let err = activation.join("watch.err");
    let mut watch = Process(
        command(&root, &["watch"])
            .env("DBUS_SESSION_BUS_ADDRESS", address.trim())
            .stdout(fs::File::create(&out).unwrap())
            .stderr(fs::File::create(&err).unwrap())
            .spawn()
            .unwrap(),
    );
    until(|| !lines(&out).is_empty());
    assert_eq!(lines(&out), vec![json!({"phase":"stopped","elapsed":0})]);
    pump(Duration::from_millis(100));
    let watch_started = marker.exists();
    let action = command(&root, &["record"])
        .env("DBUS_SESSION_BUS_ADDRESS", address.trim())
        .output()
        .unwrap();
    assert_eq!(action.status.code(), Some(1));
    assert!(action.stdout.is_empty());
    assert_eq!(
        String::from_utf8(action.stderr).unwrap(),
        "Mluva unavailable. Start the configured Mluva application first.\n"
    );
    let action_started = marker.exists();
    watch.0.kill().unwrap();
    watch.0.wait().unwrap();
    assert_eq!(fs::read_to_string(err).unwrap(), "");
    let control = Peer::new(address.trim(), None, "ok");
    // The trap exits without taking the name, so the method itself returns an
    // activation error; the independent marker proves the process ran.
    assert!(
        control
            .connection
            .call_sync(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
                "StartServiceByName",
                Some(&(NAME, 0_u32).to_variant()),
                None,
                gio::DBusCallFlags::NO_AUTO_START,
                1500,
                gio::Cancellable::NONE
            )
            .is_err()
    );
    until(|| marker.exists());
    assert_eq!(
        json!({"watch_started":watch_started,"action_started":action_started,"positive_control_started":marker.exists()}),
        fixture["activation"]
    );
    drop(control);
    for directory in ["home", "config", "data", "state"] {
        assert_eq!(fs::read_dir(root.join(directory)).unwrap().count(), 0);
    }
    println!(
        "Verified {} CLI cases, {} action receipts, {} watcher steps and {} disconnects through {}",
        fixture["cli"].as_array().unwrap().len(),
        fixture["actions"].as_array().unwrap().len(),
        fixture["watches"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| w["steps"].as_array().unwrap().len())
            .sum::<usize>(),
        fixture["disconnects"].as_array().unwrap().len(),
        executable.display()
    );
}
