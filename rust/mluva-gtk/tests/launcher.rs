//! Exercise actual native entry points through separate managed-launcher processes.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::{
        fs::{PermissionsExt, symlink},
        process::ExitStatusExt,
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[track_caller]
fn until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while !predicate() {
        assert!(Instant::now() < deadline, "native launcher did not settle");
        thread::sleep(Duration::from_millis(5));
    }
}
fn option<'a>(spec: &'a Value, name: &str, default: &'a str) -> &'a str {
    spec[name].as_str().unwrap_or(default)
}
fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::write(path, bytes).unwrap();
}
fn receipt(root: &Path) -> Value {
    json!(
        fs::read_to_string(root.join("receipts.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>()
    )
}
fn run(root: &Path, executable: &Path, spec: &Value, standalone: bool) -> Value {
    fs::create_dir(root).unwrap();
    fs::hard_link(executable, root.join("mluva")).unwrap();
    let home = root.join("home");
    fs::create_dir(&home).unwrap();
    let mut config = home.join(".config");
    let mut managed = config.join("daniel-ai-skills");
    let mut environment = BTreeMap::new();
    match option(spec, "layout", "home") {
        "xdg" | "override" => {
            config = root.join("xdg-config");
            environment.insert(
                "XDG_CONFIG_HOME".to_owned(),
                config.to_string_lossy().into_owned(),
            );
            managed = config.join("daniel-ai-skills");
        }
        "relative" => {
            managed = root.join("relative-managed");
            environment.insert("DAS_CONF_DIR".to_owned(), "relative-managed".to_owned());
        }
        "empty" => {
            environment.insert("XDG_CONFIG_HOME".to_owned(), String::new());
            environment.insert("DAS_CONF_DIR".to_owned(), String::new());
        }
        "home" => {}
        other => panic!("unknown layout {other}"),
    }
    if spec["layout"] == "override" {
        managed = root.join("managed override");
        environment.insert(
            "DAS_CONF_DIR".to_owned(),
            managed.to_string_lossy().into_owned(),
        );
    }
    let config_file = config.join("mluva/config.json");
    fs::create_dir_all(config_file.parent().unwrap()).unwrap();
    let content = if let Some(hex) = spec["config_hex"].as_str() {
        (0..hex.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap())
            .collect::<Vec<_>>()
    } else {
        option(spec, "config", "").as_bytes().to_vec()
    };
    match option(spec, "config_kind", "file") {
        "directory" => fs::create_dir(&config_file).unwrap(),
        "dangling" => symlink(root.join("missing"), &config_file).unwrap(),
        "symlink" => {
            let target = root.join("actual-config.json");
            write(&target, &content);
            symlink(target, &config_file).unwrap();
        }
        "file" => {
            if spec.get("config").is_some() || spec.get("config_hex").is_some() {
                write(&config_file, &content);
            }
        }
        other => panic!("unknown config kind {other}"),
    }
    fs::create_dir_all(managed.join("bin")).unwrap();
    fs::create_dir(managed.join("env")).unwrap();
    let marker = managed.join("snapshot.enabled");
    match option(spec, "snapshot", "file") {
        "directory" => fs::create_dir(marker).unwrap(),
        "missing" => {}
        "empty" => write(&marker, []),
        "file" => write(&marker, b"enabled"),
        other => panic!("unknown marker {other}"),
    }
    for (key, name) in [
        ("snapshot", "das-agent-snapshot"),
        ("scoped", "das-mcp-launch"),
        ("agent", "das-agent-launch"),
    ] {
        let path = managed.join("bin").join(name);
        match option(&spec["launchers"], key, "executable") {
            "executable" => {
                symlink(env!("CARGO_BIN_EXE_managed-launcher-fixture-peer"), path).unwrap()
            }
            "missing" => {}
            "directory" => fs::create_dir(path).unwrap(),
            "nonexec" => {
                write(&path, b"not executable");
                fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
            }
            "bad-interpreter" => {
                write(&path, b"#!/missing-private-interpreter\n");
                fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
            }
            other => panic!("unknown executable kind {other}"),
        }
    }
    for name in ["mluva", "agent"] {
        let path = managed.join("env").join(format!("{name}.env"));
        match option(&spec["profiles"], name, "file") {
            "directory" => fs::create_dir(path).unwrap(),
            "missing" => {}
            "empty" => write(&path, []),
            "file" => write(&path, b"synthetic reviewed reference\n"),
            other => panic!("unknown profile {other}"),
        }
    }
    write(
        &root.join("peer.json"),
        serde_json::to_vec(spec.get("peer").unwrap_or(&json!({}))).unwrap(),
    );
    let mut command = Command::new("bash");
    command
        .args([
            "-c",
            "export MLUVA_LAUNCH_PID=$$; exec \"$@\"",
            "private-launch",
        ])
        .arg(root.join("mluva"))
        .env_clear()
        .current_dir(root);
    if !standalone {
        if let Some(args) = spec["arguments"].as_array() {
            command.args(args.iter().map(|a| a.as_str().unwrap()));
        } else {
            command.args(["--help", "argument with spaces", "Ž🦀", ""]);
        }
    }
    for name in ["PATH", "LD_LIBRARY_PATH", "LC_ALL", "XDG_DATA_DIRS"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
        .env("HOME", home)
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("XDG_RUNTIME_DIR", root.join("runtime"))
        .env("MLUVA_LAUNCH_FIXTURE", root)
        .env("MLUVA_LAUNCH_CANARY", "present")
        .envs(environment);
    if let Some(environment) = spec["environment"].as_object() {
        for (name, value) in environment {
            command.env(name, value.as_str().unwrap());
        }
    }
    let out = root.join("stdout");
    let err = root.join("stderr");
    let mut process = Process(
        command
            .stdin(Stdio::null())
            .stdout(fs::File::create(&out).unwrap())
            .stderr(fs::File::create(&err).unwrap())
            .spawn()
            .unwrap(),
    );
    if spec["peer"]["wait"] == true {
        until(|| root.join("ready").exists());
        assert!(process.0.try_wait().unwrap().is_none());
        assert_eq!(
            unsafe { libc::kill(process.0.id() as i32, libc::SIGTERM) },
            0
        );
    }
    until(|| process.0.try_wait().unwrap().is_some());
    let status = process.0.wait().unwrap();
    let result = json!({"exit":status.code().unwrap_or_else(||-status.signal().unwrap()),"stdout":fs::read_to_string(out).unwrap(),"stderr":fs::read_to_string(err).unwrap(),"receipts":receipt(root)});
    if config_file.is_file() {
        assert_eq!(
            fs::read(config_file).unwrap(),
            content,
            "startup must not rewrite settings"
        );
    }
    for directory in ["data", "state", "runtime"] {
        assert!(
            !root.join(directory).exists(),
            "headless startup created {directory}"
        );
    }
    serde_json::from_str(
        &serde_json::to_string(&result)
            .unwrap()
            .replace(root.to_str().unwrap(), "$ROOT"),
    )
    .unwrap()
}

#[test]
#[ignore = "requires private HOME/XDG/network/PID/device boundaries and native narration binary"]
fn released_managed_launcher_routes_match_native_entrypoints() {
    let private = PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").unwrap())
        .canonicalize()
        .unwrap();
    assert!(
        PathBuf::from(std::env::var_os("HOME").unwrap())
            .canonicalize()
            .unwrap()
            .starts_with(&private)
    );
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
    let root = private.join("native-launcher");
    fs::create_dir(&root).unwrap();
    let app = root.join("application");
    fs::copy(env!("CARGO_BIN_EXE_mluva"), &app).unwrap();
    let narration = PathBuf::from(std::env::var_os("MLUVA_TEST_NATIVE_NARRATION").unwrap());
    let staged = root.join("narration");
    fs::copy(narration, &staged).unwrap();
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-launcher.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f"
    );
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        eprintln!("launcher: {}", case["input"]["name"]);
        let actual = run(
            &root.join(index.to_string()),
            if case["input"]["entry"] == "narration" {
                &staged
            } else {
                &app
            },
            &case["input"],
            false,
        );
        let mut expected = case["result"].clone();
        if case["input"]["entry"] == "narration" {
            for receipt in expected["receipts"].as_array_mut().unwrap() {
                assert_eq!(
                    receipt["arguments"].as_array_mut().unwrap().pop(),
                    Some(json!("--narrate"))
                );
            }
        }
        if matches!(
            case["input"]["name"].as_str(),
            Some("bad-interpreter" | "executable-directory")
        ) {
            expected["stderr"] = json!(
                "Mluva could not start its managed credential profile. Check the configured launcher and try again.\n"
            );
        }
        assert_eq!(actual, expected, "{}", case["input"]["name"]);
    }
    // The direct headless helper shares the same launch decision before creating
    // its executor. Its fixed --narrate dispatcher argument is unnecessary.
    for case in fixture["cases"].as_array().unwrap().iter().filter(|c| {
        c["input"]["name"]
            .as_str()
            .unwrap()
            .starts_with("narration-")
    }) {
        let actual = run(
            &root.join(case["input"]["name"].as_str().unwrap()),
            &staged,
            &case["input"],
            true,
        );
        let mut expected = case["result"].clone();
        for receipt in expected["receipts"].as_array_mut().unwrap() {
            assert_eq!(
                receipt["arguments"].as_array_mut().unwrap().pop(),
                Some(json!("--narrate"))
            );
        }
        assert_eq!(actual, expected);
    }
    println!(
        "Verified {} released launcher cases and all three standalone narration routes",
        fixture["cases"].as_array().unwrap().len()
    );
}
