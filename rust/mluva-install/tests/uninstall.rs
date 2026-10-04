//! Actual removal processes with independently frozen released outcomes and
//! private system-unit/interpreter mounts. No privileged command reaches a host.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env, fs,
    os::unix::fs::{MetadataExt, PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
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

fn private() -> PathBuf {
    let root =
        PathBuf::from(env::var_os("OFFSCREEN_SESSION_ROOT").expect("private session required"));
    assert_ne!(
        fs::read_link("/proc/self/ns/net")
            .unwrap()
            .to_str()
            .unwrap(),
        env::var("MLUVA_HOST_NET_NS").unwrap()
    );
    root
}
fn repository() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}
fn normal(value: &str, root: &Path) -> String {
    value.replace(root.to_str().unwrap(), "$CASE").replace(
        env!("CARGO_BIN_EXE_editor-launcher-fixture-peer"),
        "$PROCESS",
    )
}
fn copy_tree(source: &Path, target: &Path) {
    fs::create_dir_all(target).unwrap();
    for item in fs::read_dir(source).unwrap() {
        let item = item.unwrap();
        let to = target.join(item.file_name());
        if item.file_type().unwrap().is_symlink() {
            symlink(fs::read_link(item.path()).unwrap(), to).unwrap();
        } else if item.file_type().unwrap().is_dir() {
            copy_tree(&item.path(), &to);
        } else {
            fs::copy(item.path(), to).unwrap();
        }
    }
}
fn setup(root: &Path, name: &str) -> (Value, PathBuf) {
    let assets = repository().join("linux");
    assert!(
        Command::new("bash")
            .arg(repository().join("rust/mluva-install/tests/support/seed-uninstall.sh"))
            .arg(root)
            .arg(name)
            .arg(&assets)
            .status()
            .unwrap()
            .success()
    );
    let layout = serde_json::from_slice(&fs::read(root.join("layout.json")).unwrap()).unwrap();
    let source = root.join("source");
    fs::create_dir_all(source.join("bin")).unwrap();
    copy_tree(
        &assets.join("gnome-extension"),
        &source.join("gnome-extension"),
    );
    fs::create_dir(source.join("resources")).unwrap();
    fs::copy(
        assets.join("resources/mluva-input@.service"),
        source.join("resources/mluva-input@.service"),
    )
    .unwrap();
    for name in [
        "configure-input-helper.sh",
        "configure-recording-overlay.sh",
    ] {
        fs::copy(assets.join(name), source.join(name)).unwrap();
    }
    let program = if let Some(reference) = env::var_os("MLUVA_TEST_UNINSTALL") {
        let program = source.join("uninstall.sh");
        fs::copy(reference, &program).unwrap();
        program
    } else {
        let program = source.join("bin/mluva-uninstall");
        fs::hard_link(env!("CARGO_BIN_EXE_mluva-uninstall"), &program).unwrap();
        program
    };
    fs::create_dir(root.join("fake-bin")).unwrap();
    for name in [
        "id",
        "sudo",
        "gnome-extensions",
        "update-desktop-database",
        "python-trap",
    ] {
        let path = root.join("fake-bin").join(name);
        fs::write(&path, include_bytes!("support/uninstall-peer.sh")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    if matches!(name, "database-group-only" | "database-unavailable") {
        fs::set_permissions(
            root.join("fake-bin/update-desktop-database"),
            fs::Permissions::from_mode(0o010),
        )
        .unwrap();
    }
    if name == "database-nonexecutable" {
        fs::set_permissions(
            root.join("fake-bin/update-desktop-database"),
            fs::Permissions::from_mode(0o644),
        )
        .unwrap();
    }
    (layout, program)
}
fn command(root: &Path, name: &str, layout: &Value) -> Command {
    let units = fs::metadata(root.join("units")).unwrap();
    let mut cmd = Command::new("/usr/bin/bwrap");
    cmd.args([
        "--die-with-parent",
        "--bind",
        "/",
        "/",
        "--dev",
        "/dev",
        "--bind",
    ])
    .arg(root.join("units"))
    .args([
        "/etc/systemd/system",
        "--tmpfs",
        "/run/systemd",
        "--tmpfs",
        "/run/dbus",
        "--ro-bind",
    ])
    .arg(root.join("fake-bin/python-trap"))
    .arg("/usr/bin/python3");
    if matches!(name, "database-unavailable" | "database-nonexecutable") {
        cmd.arg("--ro-bind")
            .arg(root.join("fake-bin/update-desktop-database"))
            .arg("/usr/bin/update-desktop-database");
    }
    cmd.args(["--", "/usr/bin/bash", "-c",
            "[[ \"$(stat -Lc %d:%i /etc/systemd/system)\" == \"$MLUVA_INSTALL_UNIT_ID\" ]] || exit 98; exec \"$@\"", "entry"])
        .env_clear().env("PATH", format!("{}:/usr/bin:/bin", root.join("fake-bin").display()))
        .env("HOME", layout["live"].as_str().unwrap()).env("MLUVA_INSTALL_HOME", layout["install"].as_str().unwrap())
        .env("XDG_DATA_HOME", layout["data"].as_str().unwrap()).env("XDG_CONFIG_HOME", layout["config"].as_str().unwrap())
        .env("LC_ALL", "C.UTF-8").env("MLUVA_UNINSTALL_PEER", root.join("peer")).env("MLUVA_UNINSTALL_CASE", name)
        .env("MLUVA_INSTALL_UNIT_ID", format!("{}:{}", units.dev(), units.ino())).stdin(Stdio::null());
    cmd
}
fn snapshot(root: &Path) -> BTreeMap<String, Value> {
    fn visit(root: &Path, path: &Path, output: &mut BTreeMap<String, Value>) {
        let Ok(metadata) = path.symlink_metadata() else {
            return;
        };
        let name = path
            .strip_prefix(root)
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let mode = metadata.permissions().mode() & 0o7777;
        let value = if metadata.is_symlink() {
            json!({"link": normal(fs::read_link(path).unwrap().to_str().unwrap(), root)})
        } else if metadata.is_dir() {
            for item in fs::read_dir(path).unwrap() {
                visit(root, &item.unwrap().path(), output);
            }
            json!({"directory": mode})
        } else {
            let bytes = fs::read(path).unwrap();
            let bytes = match String::from_utf8(bytes) {
                Ok(value) => normal(&value, root).into_bytes(),
                Err(error) => error.into_bytes(),
            };
            let digest: String = Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            json!({"sha256": digest, "mode": mode})
        };
        output.insert(name, value);
    }
    let mut output = BTreeMap::new();
    for name in [
        "home",
        "Žluťoučký home",
        "custom-data",
        "custom-config",
        "foreign",
        "units",
    ] {
        visit(root, &root.join(name), &mut output);
    }
    output
}
fn observe(root: &Path, before: &BTreeMap<String, Value>, result: Output) -> Value {
    fs::write(root.join("stdout"), &result.stdout).unwrap();
    fs::write(root.join("stderr"), &result.stderr).unwrap();
    let after = snapshot(root);
    let error = String::from_utf8(result.stderr).unwrap();
    let error = match result.status.code() {
        Some(126) => {
            assert_eq!(error, "Mluva could not execute update-desktop-database.\n");
            "optional-command-exec-failed".to_owned()
        }
        Some(143) => {
            assert_eq!(error, "Mluva desktop helper stopped after signal 15.\n");
            "optional-command-signal".to_owned()
        }
        _ => normal(&error, root),
    };
    json!({"code":result.status.code(), "stdout":normal(&String::from_utf8(result.stdout).unwrap(),root),
        "stderr":error,
        "commands":normal(&fs::read_to_string(root.join("peer/commands")).unwrap_or_default(),root),
        "removed":before.keys().filter(|name| !after.contains_key(*name)).collect::<Vec<_>>(),
        "changed":after.iter().filter(|(name,value)| before.get(*name) != Some(*value)).collect::<BTreeMap<_,_>>()})
}
fn running_peer(root: &Path, program: &Path, legacy: bool) -> Process {
    fs::create_dir_all(program.parent().unwrap()).unwrap();
    if legacy {
        symlink(env!("CARGO_BIN_EXE_editor-launcher-fixture-peer"), program).unwrap();
    } else {
        fs::hard_link(env!("CARGO_BIN_EXE_editor-launcher-fixture-peer"), program).unwrap();
    }
    fs::write(root.join("peer/initial-pid"), "0\n").unwrap();
    let mut child = Process(
        Command::new(program)
            .args(["-m", "mluva_linux.app"])
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("MLUVA_EDITOR_PEER_DIR", root.join("peer"))
            .env("MLUVA_EDITOR_PEER_WAIT", "1")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while !root.join("peer/receipt.json").exists() {
        assert!(child.0.try_wait().unwrap().is_none());
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    child
}
fn prove_python_trap(root: &Path, name: &str, layout: &Value) {
    assert_eq!(
        command(root, name, layout)
            .arg("/usr/bin/python3")
            .output()
            .unwrap()
            .status
            .code(),
        Some(99)
    );
    fs::remove_file(root.join("peer/python-used")).unwrap();
    fs::remove_file(root.join("peer/commands")).unwrap();
}

#[test]
#[ignore = "requires guarded private Linux mounts, processes and service peers"]
fn released_uninstall_processes_preserve_owned_and_foreign_state() {
    let run = private().join("native-uninstall");
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-uninstall.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let root = run.join(name);
        let (layout, program) = setup(&root, name);
        let _child = (name == "running").then(|| {
            running_peer(
                &root,
                &Path::new(layout["home"].as_str().unwrap())
                    .join(".local/share/mluva/app/.venv/bin/python"),
                true,
            )
        });
        prove_python_trap(&root, name, &layout);
        let before = snapshot(&root);
        let result = command(&root, name, &layout).arg(program).output().unwrap();
        let actual = observe(&root, &before, result);
        fs::write(
            root.join("result.json"),
            serde_json::to_vec_pretty(&actual).unwrap(),
        )
        .unwrap();
        assert!(!root.join("peer/python-used").exists());
        assert_eq!(actual, case["result"], "{name}");
        eprintln!("uninstall {name}: matched");
    }
}

#[test]
#[ignore = "requires guarded private Linux mounts, processes and service peers"]
fn native_uninstall_rejects_escaped_staging_and_active_executables() {
    let run = private().join("native-uninstall-guards");
    for name in [
        "alias",
        "escape",
        "marker-link",
        "native-marker-invalid",
        "native-marker-foreign",
        "native-process",
    ] {
        let root = run.join(name);
        let (mut layout, program) = setup(&root, "owned");
        let app = root.join("home/.local/share/mluva/app");
        let mut child = None;
        match name {
            "alias" => {
                symlink(root.join("home"), root.join("home-alias")).unwrap();
                layout["live"] = layout["home"].clone();
                layout["install"] = json!(root.join("home-alias"));
            }
            "escape" => {
                fs::rename(root.join("home/.local/share"), root.join("foreign/data")).unwrap();
                symlink(root.join("foreign/data"), root.join("home/.local/share")).unwrap();
            }
            "marker-link" => {
                fs::rename(app.join("pyproject.toml"), root.join("foreign/marker")).unwrap();
                symlink(root.join("foreign/marker"), app.join("pyproject.toml")).unwrap();
            }
            "native-marker-invalid" | "native-marker-foreign" => {
                fs::remove_file(app.join("pyproject.toml")).unwrap();
                fs::write(
                    app.join(".mluva-native.json"),
                    if name == "native-marker-invalid" {
                        "{synthetic-do-not-print"
                    } else {
                        "{\"schema\":1,\"application\":\"other\",\"implementation\":\"rust\"}"
                    },
                )
                .unwrap();
            }
            _ => {
                child = Some(running_peer(&root, &app.join("bin/mluva"), false));
            }
        }
        prove_python_trap(&root, name, &layout);
        let before = snapshot(&root);
        let result = command(&root, name, &layout).arg(program).output().unwrap();
        let actual = observe(&root, &before, result);
        fs::write(
            root.join("result.json"),
            serde_json::to_vec_pretty(&actual).unwrap(),
        )
        .unwrap();
        assert_eq!(snapshot(&root), before, "{name}: guarded state changed");
        assert_eq!(actual["code"], 1, "{name}");
        assert_eq!(
            actual["commands"], "",
            "{name}: external effects before guard"
        );
        assert!(!root.join("peer/python-used").exists());
        assert!(
            !actual["stderr"]
                .as_str()
                .unwrap()
                .contains("synthetic-do-not-print")
        );
        drop(child);
        eprintln!("uninstall guard {name}: preserved");
    }
}

#[test]
#[ignore = "requires a freshly assembled native bundle and the guarded Linux runner"]
fn packaged_public_uninstaller_removes_itself_and_preserves_user_state() {
    let root = private().join("packaged-uninstall");
    let (layout, _) = setup(&root, "owned");
    let home = Path::new(layout["home"].as_str().unwrap());
    let app = home.join(".local/share/mluva/app");
    let profile = home.join(".config/daniel-ai-skills/env/mluva.env");
    fs::create_dir_all(profile.parent().unwrap()).unwrap();
    fs::write(
        &profile,
        "ELEVENLABS_API_KEY=op://synthetic/reference/credential\n",
    )
    .unwrap();
    fs::set_permissions(&profile, fs::Permissions::from_mode(0o600)).unwrap();
    let mut retained = snapshot(&root);
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-uninstall.json")).unwrap();
    let owned = &fixture["cases"][0]["result"];
    for path in owned["removed"].as_array().unwrap() {
        retained.remove(path.as_str().unwrap());
    }
    // The independent reference owns the integrations; native payload bytes are
    // validated by package.rs and must disappear as one complete directory.
    fs::remove_dir_all(&app).unwrap();
    let bundle = PathBuf::from(
        env::var_os("MLUVA_TEST_NATIVE_BUNDLE").expect("assembled native bundle required"),
    );
    copy_tree(&bundle, &app);
    assert!(app.join("bin/mluva-uninstall").is_file());
    assert!(!app.join("pyproject.toml").exists());
    let launcher = home.join(".local/bin/mluva");
    fs::remove_file(&launcher).unwrap();
    symlink(app.join("bin/mluva"), &launcher).unwrap();
    let public = home.join(".local/bin/mluva-uninstall");
    assert_eq!(
        public.canonicalize().unwrap(),
        app.join("bin/mluva-uninstall")
    );
    prove_python_trap(&root, "owned", &layout);
    let result = command(&root, "owned", &layout)
        .arg(&public)
        .output()
        .unwrap();
    fs::write(root.join("stdout"), &result.stdout).unwrap();
    fs::write(root.join("stderr"), &result.stderr).unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        normal(&String::from_utf8(result.stdout).unwrap(), &root),
        owned["stdout"]
    );
    assert_eq!(String::from_utf8(result.stderr).unwrap(), owned["stderr"]);
    assert!(!app.exists() && !public.is_symlink());
    assert_eq!(snapshot(&root), retained);
    assert!(!root.join("peer/python-used").exists());
    eprintln!(
        "Native packaged public uninstaller removed its complete payload and retained all user bytes"
    );
}
