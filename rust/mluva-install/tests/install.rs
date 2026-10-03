//! Public installer transactions against frozen release outcomes. Dependency
//! faults use an ELF peer; the final test installs the complete real bundle.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env, fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

fn repository() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}
fn private(name: &str) -> PathBuf {
    let root =
        PathBuf::from(env::var_os("OFFSCREEN_SESSION_ROOT").expect("private session required"));
    assert_ne!(
        fs::read_link("/proc/self/ns/net")
            .unwrap()
            .to_str()
            .unwrap(),
        env::var("MLUVA_HOST_NET_NS").unwrap()
    );
    let path = root.join(name);
    fs::create_dir(&path).unwrap();
    path
}
fn normal(value: &str, root: &Path) -> String {
    value.replace(root.to_str().unwrap(), "$CASE").replace(
        env!("CARGO_BIN_EXE_install-dependency-fixture-peer"),
        "$DEPENDENCY",
    )
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn setup(root: &Path, name: &str) -> Value {
    assert!(
        Command::new("bash")
            .arg(repository().join("rust/mluva-install/tests/support/seed-install.sh"))
            .arg(root)
            .arg(name)
            .arg(repository().join("linux"))
            .status()
            .unwrap()
            .success()
    );
    serde_json::from_slice(&fs::read(root.join("layout.json")).unwrap()).unwrap()
}
fn data(root: &Path, layout: &Value) -> PathBuf {
    if layout["data"] == root.join("custom-data").to_str().unwrap() {
        root.join("custom-data")
    } else {
        Path::new(layout["home"].as_str().unwrap()).join(".local/share")
    }
}

fn snapshot(root: &Path, layout: &Value, portable: bool) -> BTreeMap<String, Value> {
    fn visit(
        root: &Path,
        path: &Path,
        app: &Path,
        launcher: &Path,
        portable: bool,
        output: &mut BTreeMap<String, Value>,
    ) {
        let Ok(meta) = path.symlink_metadata() else {
            return;
        };
        let name = path
            .strip_prefix(root)
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let mode = meta.permissions().mode() & 0o7777;
        let prepared = path == app
            && (app.join("bin/mluva").is_file() || app.join("mluva_linux/app.py").is_file());
        let value = if portable && prepared {
            json!({"application":"prepared","mode":mode})
        } else if portable
            && path == launcher
            && (fs::read_link(path).is_ok_and(|target| target == app.join("bin/mluva"))
                || (meta.is_file()
                    && fs::read_to_string(path)
                        .unwrap()
                        .contains("-m mluva_linux.app")))
        {
            json!({"command":"mluva"})
        } else if meta.is_symlink() {
            json!({"link":normal(fs::read_link(path).unwrap().to_str().unwrap(),root)})
        } else if meta.is_dir() {
            for entry in fs::read_dir(path).unwrap() {
                visit(
                    root,
                    &entry.unwrap().path(),
                    app,
                    launcher,
                    portable,
                    output,
                );
            }
            json!({"directory":mode})
        } else {
            let bytes = fs::read(path).unwrap();
            let bytes = match String::from_utf8(bytes) {
                Ok(value) => normal(&value, root).into_bytes(),
                Err(error) => error.into_bytes(),
            };
            json!({"sha256":digest(&bytes),"mode":mode})
        };
        output.insert(name, value);
    }
    let mut output = BTreeMap::new();
    let app = data(root, layout).join("mluva/app");
    let launcher = Path::new(layout["home"].as_str().unwrap()).join(".local/bin/mluva");
    for name in [
        "home",
        "Žluťoučký home",
        "custom-data",
        "custom-config",
        "foreign",
        "units",
    ] {
        visit(
            root,
            &root.join(name),
            &app,
            &launcher,
            portable,
            &mut output,
        );
    }
    output
}

fn fixture_bundle(root: &Path) -> PathBuf {
    let bundle = root.join("source");
    for name in ["bin", "resources"] {
        fs::create_dir_all(bundle.join(name)).unwrap();
    }
    for name in [
        "mluva",
        "mluva-shell",
        "mluva-narrate",
        "mluva-asr-worker",
        "mluva-audio-cleanup",
        "mluva-install-widget",
        "mluva-screenshot-editor",
        "mluva-uninstall",
        "mluva-install",
    ] {
        let source = if name == "mluva-install" {
            env!("CARGO_BIN_EXE_mluva-install")
        } else {
            env!("CARGO_BIN_EXE_install-dependency-fixture-peer")
        };
        fs::hard_link(source, bundle.join("bin").join(name)).unwrap();
    }
    for name in [
        "com.mluva.Linux.desktop.in",
        "com.mluva.Linux.svg",
        "mluva-input@.service",
    ] {
        fs::copy(
            repository().join("linux/resources").join(name),
            bundle.join("resources").join(name),
        )
        .unwrap();
    }
    for name in [
        "configure-input-helper.sh",
        "configure-recording-overlay.sh",
    ] {
        fs::copy(repository().join("linux").join(name), bundle.join(name)).unwrap();
    }
    let links = json!({"mluva-shell":"bin/mluva-shell","mluva-narrate":"bin/mluva-narrate","mluva-screenshot-editor":"bin/mluva-screenshot-editor","uninstall.sh":"bin/mluva-uninstall","install.sh":"bin/mluva-install"});
    for (name, target) in links.as_object().unwrap() {
        symlink(target.as_str().unwrap(), bundle.join(name)).unwrap();
    }
    let mut files = BTreeMap::new();
    for directory in [
        &bundle,
        bundle.join("bin").as_path(),
        bundle.join("resources").as_path(),
    ] {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_file() {
                files.insert(
                    entry
                        .path()
                        .strip_prefix(&bundle)
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .to_owned(),
                    digest(&fs::read(entry.path()).unwrap()),
                );
            }
        }
    }
    fs::write(bundle.join(".mluva-native.json"),serde_json::to_vec(&json!({"schema":1,"application":"com.mluva.Linux","implementation":"rust","version":"1.6.0","sha256":files,"links":links})).unwrap()).unwrap();
    bundle
}

fn command(root: &Path, layout: &Value, name: &str, program: &Path, native: bool) -> Command {
    let mut command = Command::new("/usr/bin/bwrap");
    command
        .args([
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
        ]);
    if native {
        command
            .arg("--ro-bind")
            .arg(root.join("fake-bin/python-trap"))
            .arg("/usr/bin/python3");
    }
    command
        .arg("--")
        .arg(program)
        .env_clear()
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", root.join("fake-bin").display()),
        )
        .env("HOME", layout["live"].as_str().unwrap())
        .env("MLUVA_INSTALL_HOME", layout["install"].as_str().unwrap())
        .env("XDG_DATA_HOME", layout["data"].as_str().unwrap())
        .env("XDG_CONFIG_HOME", layout["config"].as_str().unwrap())
        .env("LC_ALL", "C.UTF-8")
        .env("TMPDIR", root.join("temp"))
        .env("MLUVA_INSTALL_PEER", root.join("peer"))
        .env("MLUVA_INSTALL_CASE", name)
        .env("MLUVA_INSTALL_TARGET", layout["home"].as_str().unwrap())
        .env(
            "MLUVA_INSTALL_DEPENDENCY_PEER",
            env!("CARGO_BIN_EXE_install-dependency-fixture-peer"),
        )
        .env(
            "XDG_CURRENT_DESKTOP",
            if name.contains("gnome") {
                "GNOME"
            } else {
                "offscreen"
            },
        )
        .stdin(Stdio::null());
    if let Some(value) = env::var_os("LD_LIBRARY_PATH") {
        command.env("LD_LIBRARY_PATH", value);
    }
    command
}
fn positive_trap(root: &Path, layout: &Value, name: &str) {
    assert_eq!(
        command(root, layout, name, Path::new("/usr/bin/python3"), true)
            .output()
            .unwrap()
            .status
            .code(),
        Some(99)
    );
    assert!(root.join("peer/python-used").exists());
    fs::remove_file(root.join("peer/python-used")).unwrap();
}
fn logs(root: &Path, result: &Output) {
    fs::write(root.join("stdout"), &result.stdout).unwrap();
    fs::write(root.join("stderr"), &result.stderr).unwrap();
    assert!(!root.join("peer/python-used").exists());
    assert!(!String::from_utf8_lossy(&result.stderr).contains("synthetic-value-do-not-echo"));
}

#[test]
#[ignore = "requires a guarded disposable Linux session"]
fn modern_install_matches_released_public_outcomes() {
    let run = private("install-comparison");
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-install.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let root = run.join(name);
        let layout = setup(&root, name);
        let before = snapshot(&root, &layout, false);
        let bundle = fixture_bundle(&root);
        positive_trap(&root, &layout, name);
        let result = command(&root, &layout, name, &bundle.join("install.sh"), true)
            .output()
            .unwrap();
        logs(&root, &result);
        assert_eq!(
            result.status.code(),
            case["result"]["code"].as_i64().map(|value| value as i32),
            "{name}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let error = String::from_utf8_lossy(&result.stderr);
        let category = if result.status.success() {
            assert!(error.is_empty(), "{name}: {error}");
            ""
        } else if name == "dependency-failure" {
            assert!(error.contains("73"));
            "dependency-failed"
        } else if error.contains("unexpected application") {
            "unrecognized application"
        } else {
            [
                "unrecognized application",
                "unexpected application",
                "unrelated command",
                "unsafe XDG root",
                "absolute non-root",
            ]
            .into_iter()
            .find(|needle| error.contains(needle))
            .unwrap_or_else(|| panic!("{name}: {error}"))
        };
        assert_eq!(category, case["result"]["error"], "{name}");
        assert_eq!(
            normal(std::str::from_utf8(&result.stdout).unwrap(), &root),
            case["result"]["stdout"],
            "{name}"
        );
        assert_eq!(
            normal(
                &fs::read_to_string(root.join("peer/commands")).unwrap_or_default(),
                &root
            ),
            case["result"]["commands"],
            "{name}"
        );
        if name == "dependency-failure" {
            // The released app-only rollback leaves its parent's mode changed.
            assert_eq!(
                snapshot(&root, &layout, false),
                before,
                "complete dependency-failure rollback"
            );
        } else {
            assert_eq!(
                json!(snapshot(&root, &layout, true)),
                case["result"]["tree"],
                "{name}"
            );
        }
        eprintln!("Matched install case {name}");
    }
}

#[test]
#[ignore = "requires a guarded disposable Linux session"]
fn modern_install_rolls_back_all_public_paths() {
    let run = private("install-rollback");
    for name in [
        "database-failure",
        "live-database-failure",
        "live-profile-invalid",
        "live-profile-duplicate",
        "native-legacy",
        "desktop-foreign",
        "desktop-link",
        "icon-foreign",
        "icon-link",
    ] {
        let root = run.join(name);
        let layout = setup(&root, name);
        let before = snapshot(&root, &layout, false);
        let bundle = fixture_bundle(&root);
        let reference = env::var_os("MLUVA_TEST_INSTALL_REFERENCE");
        let program = reference
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(|| bundle.join("install.sh"));
        if reference.is_none() {
            positive_trap(&root, &layout, name);
        }
        let result = command(&root, &layout, name, &program, reference.is_none())
            .output()
            .unwrap();
        logs(&root, &result);
        assert!(!result.status.success(), "{name}");
        if name.ends_with("database-failure") {
            assert_eq!(result.status.code(), Some(25));
            assert!(root.join("peer/dependency-check").exists());
        }
        assert_eq!(
            snapshot(&root, &layout, false),
            before,
            "{name}: the previous installation must be byte/mode/link identical"
        );
        eprintln!("Complete rollback/preservation for {name}");
    }
}

#[test]
#[ignore = "requires a guarded disposable Linux session"]
fn native_install_cancellation_and_concurrent_owner_are_preserved() {
    let run = private("install-interruption");
    for name in ["live-signal", "live-foreign-launcher"] {
        let root = run.join(name);
        let layout = setup(&root, name);
        let before = snapshot(&root, &layout, false);
        let bundle = fixture_bundle(&root);
        positive_trap(&root, &layout, name);
        let result = if name == "live-signal" {
            // bwrap's default process arrangement preserves the invoked PID;
            // record the installer's actual PID from /proc rather than guessing.
            let log = fs::File::create(root.join("interrupt.log")).unwrap();
            let mut child = command(&root, &layout, name, &bundle.join("install.sh"), true)
                .stdout(log.try_clone().unwrap())
                .stderr(log)
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(15);
            while !root.join("peer/waiting").exists() {
                assert!(child.try_wait().unwrap().is_none());
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(10));
            }
            let executable = bundle.join("bin/mluva-install");
            let pid = fs::read_dir("/proc")
                .unwrap()
                .filter_map(|entry| {
                    let entry = entry.ok()?;
                    let pid = entry.file_name().to_str()?.parse::<i32>().ok()?;
                    let args = fs::read(entry.path().join("cmdline")).ok()?;
                    if args.split(|byte| *byte == 0).next()
                        == Some(bundle.join("install.sh").as_os_str().as_encoded_bytes())
                        || fs::read_link(entry.path().join("exe"))
                            .is_ok_and(|path| path == executable)
                    {
                        Some(pid)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            assert_eq!(pid.len(), 1);
            assert_eq!(unsafe { libc::kill(pid[0], libc::SIGTERM) }, 0);
            while child.try_wait().unwrap().is_none() {
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(10));
            }
            let status = child.wait().unwrap();
            assert_eq!(status.code(), Some(143));
            let worker: i32 = fs::read_to_string(root.join("peer/child"))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            let alive = PathBuf::from(format!("/proc/{worker}/stat"));
            assert!(
                !alive.exists()
                    || fs::read_to_string(alive).unwrap().split_whitespace().nth(2) == Some("Z"),
                "installer descendant survived cancellation"
            );
            assert_eq!(
                snapshot(&root, &layout, false),
                before,
                "interruption rollback"
            );
            None
        } else {
            Some(
                command(&root, &layout, name, &bundle.join("install.sh"), true)
                    .output()
                    .unwrap(),
            )
        };
        if let Some(result) = result {
            logs(&root, &result);
            assert_eq!(result.status.code(), Some(25));
            let launcher = Path::new(layout["home"].as_str().unwrap()).join(".local/bin/mluva");
            assert_eq!(fs::read_to_string(&launcher).unwrap(), "concurrent owner\n");
            let backups = fs::read_dir(launcher.parent().unwrap())
                .unwrap()
                .filter_map(|entry| {
                    let path = entry.unwrap().path();
                    path.file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with(".mluva-install-")
                        .then_some(path)
                })
                .collect::<Vec<_>>();
            assert_eq!(backups.len(), 1);
            assert_eq!(
                fs::metadata(&backups[0]).unwrap().permissions().mode() & 0o777,
                0o700
            );
            let previous = backups[0].join("previous");
            assert!(
                fs::read_to_string(previous)
                    .unwrap()
                    .contains("-m mluva_linux.app")
            );
            fs::remove_file(&launcher).unwrap();
            fs::rename(backups[0].join("previous"), &launcher).unwrap();
            fs::remove_dir(&backups[0]).unwrap();
            assert_eq!(
                snapshot(&root, &layout, false),
                before,
                "all other paths restored"
            );
            assert!(String::from_utf8_lossy(&result.stderr).contains("Recovery files remain"));
        }
        assert!(!root.join("peer/python-used").exists());
        eprintln!("Verified native {name}");
    }
}

#[test]
#[ignore = "requires an assembled actual native bundle and a guarded disposable Linux session"]
fn actual_bundle_installs_upgrades_and_starts_from_its_public_launcher() {
    let run = private("install-actual");
    let root = run.join("owned");
    let layout = setup(&root, "owned");
    let source =
        PathBuf::from(env::var_os("MLUVA_TEST_NATIVE_BUNDLE").expect("actual bundle required"));
    positive_trap(&root, &layout, "owned");
    // Exercise the real desktop cache tool on the disposable applications dir.
    fs::remove_file(root.join("fake-bin/update-desktop-database")).unwrap();
    let result = command(&root, &layout, "owned", &source.join("install.sh"), true)
        .output()
        .unwrap();
    logs(&root, &result);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let app = data(&root, &layout).join("mluva/app");
    let bin = Path::new(layout["home"].as_str().unwrap()).join(".local/bin");
    assert_eq!(
        fs::read(app.join(".mluva-native.json")).unwrap(),
        fs::read(source.join(".mluva-native.json")).unwrap()
    );
    assert!(!app.join("pyproject.toml").exists());
    assert!(!app.join(".venv").exists());
    assert_eq!(
        fs::read_link(bin.join("mluva")).unwrap(),
        app.join("bin/mluva")
    );
    let help = command(&root, &layout, "owned", &bin.join("mluva"), true)
        .arg("--help")
        .output()
        .unwrap();
    logs(&root, &help);
    let released: Value = serde_json::from_str(include_str!(
        "../../mluva-gtk/tests/fixtures/released-bootstrap.json"
    ))
    .unwrap();
    assert!(help.status.success());
    assert_eq!(
        String::from_utf8(help.stdout).unwrap(),
        released["cli"][0]["stdout"]
    );
    assert!(help.stderr.is_empty());
    // Reinstall from the installed copy itself; moving the old app must neither
    // invalidate its executable nor recursively consume the staging directory.
    let result = command(&root, &layout, "owned", &app.join("install.sh"), true)
        .output()
        .unwrap();
    logs(&root, &result);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        fs::read(app.join(".mluva-native.json")).unwrap(),
        fs::read(source.join(".mluva-native.json")).unwrap()
    );
    fs::write(
        run.join("installed-bundle"),
        app.as_os_str().as_encoded_bytes(),
    )
    .unwrap();
    eprintln!("Complete installed native bundle at {}", app.display());
}

#[test]
#[ignore = "requires a guarded disposable Linux session"]
fn native_installer_rejects_damaged_bundles_and_running_apps_before_publication() {
    let run = private("install-admission");
    for name in [
        "payload",
        "link",
        "receipt",
        "version",
        "architecture",
        "running",
        "live-legacy-custom",
    ] {
        let root = run.join(name);
        let layout = setup(&root, name);
        let bundle = fixture_bundle(&root);
        let inventory = bundle.join(".mluva-native.json");
        let mut child = None;
        match name {
            "payload" => fs::write(
                bundle.join("resources/com.mluva.Linux.svg"),
                "changed package\n",
            )
            .unwrap(),
            "link" => {
                fs::remove_file(bundle.join("mluva-shell")).unwrap();
                symlink(root.join("foreign/sentinel"), bundle.join("mluva-shell")).unwrap();
            }
            "receipt" => fs::write(&inventory, "{synthetic-value-do-not-echo").unwrap(),
            "version" => {
                let mut value: Value =
                    serde_json::from_slice(&fs::read(&inventory).unwrap()).unwrap();
                value["version"] = json!("999.0.0");
                fs::write(&inventory, serde_json::to_vec(&value).unwrap()).unwrap();
            }
            "architecture" => {
                let binary = bundle.join("bin/mluva");
                let mut bytes = fs::read(&binary).unwrap();
                bytes[18..20].copy_from_slice(&0_u16.to_le_bytes());
                fs::remove_file(&binary).unwrap(); // Never mutate a hard-linked build input.
                fs::write(&binary, &bytes).unwrap();
                fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
                let mut value: Value =
                    serde_json::from_slice(&fs::read(&inventory).unwrap()).unwrap();
                value["sha256"]["bin/mluva"] = json!(digest(&bytes));
                fs::write(&inventory, serde_json::to_vec(&value).unwrap()).unwrap();
            }
            "running" => {
                let executable = data(&root, &layout).join("mluva/app/resident");
                fs::copy("/usr/bin/sleep", &executable).unwrap();
                child = Some(
                    Command::new(executable)
                        .arg("30")
                        .stdin(Stdio::null())
                        .spawn()
                        .unwrap(),
                );
            }
            "live-legacy-custom" => {
                let profile = root.join("home/.config/other-managed/env/voice-scribe.env");
                fs::create_dir_all(profile.parent().unwrap()).unwrap();
                fs::write(
                    profile,
                    "ELEVENLABS_API_KEY=op://synthetic/legacy/credential\n",
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        let before = snapshot(&root, &layout, false);
        positive_trap(&root, &layout, name);
        let mut candidate = command(&root, &layout, name, &bundle.join("install.sh"), true);
        if name == "live-legacy-custom" {
            candidate.env("DAS_CONF_DIR", root.join("home/.config/other-managed"));
        }
        let result = candidate.output().unwrap();
        if let Some(mut child) = child {
            child.kill().unwrap();
            child.wait().unwrap();
        }
        logs(&root, &result);
        assert!(!result.status.success(), "{name}");
        let error = String::from_utf8_lossy(&result.stderr);
        let expected = match name {
            "running" => "Mluva is running",
            "live-legacy-custom" => "cannot yet migrate VoiceScribe",
            "version" => "does not match",
            "receipt" => "inventory is invalid",
            "architecture" => "architecture",
            _ => "differs from its release inventory",
        };
        assert!(error.contains(expected), "{name}: {error}");
        assert!(
            !root.join("peer/dependency-check").exists(),
            "executed an inadmissible bundle"
        );
        assert_eq!(snapshot(&root, &layout, false), before, "{name}");
        eprintln!("Refused {name} without mutation or dependency execution");
    }
}
