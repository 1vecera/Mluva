//! Combined setup's public shell entry point, frozen released control flow and
//! real terminal confirmation. External peers never install system packages.
use serde_json::{Value, json};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::{
    collections::BTreeSet,
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

fn repository() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}

fn private(name: &str) -> PathBuf {
    assert_ne!(
        fs::read_link("/proc/self/ns/net")
            .unwrap()
            .to_str()
            .unwrap(),
        env::var("MLUVA_HOST_NET_NS").unwrap()
    );
    let path =
        PathBuf::from(env::var_os("OFFSCREEN_SESSION_ROOT").expect("private session required"))
            .join(name);
    fs::create_dir(&path).unwrap();
    path
}

fn command(root: &Path) -> Command {
    let mut command = Command::new("/usr/bin/bwrap");
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
        .args([
            "/usr/bin/python3",
            "--tmpfs",
            "/etc/systemd/system",
            "--tmpfs",
            "/run/systemd",
            "--tmpfs",
            "/run/dbus",
            "--",
        ])
        .env_clear()
        .env("PATH", root.join("bin"))
        .env("HOME", root.join("home"))
        .env("USER", "test-user")
        .env("MLUVA_SETUP_LOG", root.join("commands.log"))
        .env("MLUVA_SETUP_PROGRAM", root.join("source/install.sh"))
        .env("MLUVA_SETUP_NATIVE", "1")
        .env("SHELL", "/usr/bin/bash")
        .env("LC_ALL", "C.UTF-8");
    if let Some(value) = env::var_os("LD_LIBRARY_PATH") {
        command.env("LD_LIBRARY_PATH", value);
    }
    command
}

#[test]
#[ignore = "requires an assembled actual native bundle and a guarded disposable Linux session"]
fn actual_combined_setup_installs_app_widget_and_preserves_failure_boundaries() {
    let run = private("setup-actual");
    let source =
        PathBuf::from(env::var_os("MLUVA_TEST_NATIVE_BUNDLE").expect("assembled bundle required"));
    for name in [
        "app-only",
        "combined",
        "widget-failure",
        "foreign-widget",
        "package-failure",
    ] {
        let root = run.join(name);
        assert!(
            Command::new("/usr/bin/bash")
                .arg(repository().join("rust/mluva-install/tests/support/seed-setup.sh"))
                .arg(&root)
                .arg(name)
                .arg(source.join("install.sh"))
                .status()
                .unwrap()
                .success()
        );
        fs::create_dir(root.join("widget-peer")).unwrap();
        fs::create_dir(root.join("temp")).unwrap();
        fs::write(
            root.join("home/personal-note"),
            b"preserve unrelated user data\n",
        )
        .unwrap();
        fs::copy(
            repository().join("rust/mluva-install/tests/support/setup-omarchy-peer.sh"),
            root.join("bin/omarchy"),
        )
        .unwrap();
        fs::set_permissions(root.join("bin/omarchy"), fs::Permissions::from_mode(0o755)).unwrap();
        fs::copy(
            repository().join("rust/mluva-install/tests/support/omarchy-peer.sh"),
            root.join("bin/omarchy-shell"),
        )
        .unwrap();
        fs::set_permissions(
            root.join("bin/omarchy-shell"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        // Real dependencies are loaded by public help; capture/input are never
        // started. These names satisfy the installer's external-command probe.
        for name in ["pw-record", "pw-dump", "wl-copy"] {
            symlink("/usr/bin/false", root.join("bin").join(name)).unwrap();
        }
        let app = root.join("home/.local/share/mluva/app");
        let widget = root.join("home/.config/omarchy/plugins/mluva.dictation");
        if name == "foreign-widget" {
            fs::create_dir_all(&widget).unwrap();
            fs::write(widget.join("foreign.qml"), b"another owner\n").unwrap();
        }
        let invocation = || {
            let mut command = command(&root);
            command
                .env(
                    "PATH",
                    format!("{}:/usr/bin:/bin", root.join("bin").display()),
                )
                .env("XDG_CURRENT_DESKTOP", "offscreen")
                .env("TMPDIR", root.join("temp"))
                .env("MLUVA_ACTUAL_SETUP_CASE", name)
                .env(
                    "MLUVA_WIDGET_PEER",
                    repository().join("rust/mluva-install/tests/support/omarchy-peer.sh"),
                )
                .env("MLUVA_WIDGET_PEER_DIR", root.join("widget-peer"));
            if name == "widget-failure" {
                command.env("MLUVA_WIDGET_FAILURE", "shell-rescanPlugins-1");
            }
            command
        };
        let trap = invocation().arg("/usr/bin/python3").output().unwrap();
        assert_eq!(trap.status.code(), Some(99));
        fs::remove_file(root.join("commands.log.python-used")).unwrap();
        let mut install = invocation();
        install
            .arg("/usr/bin/bash")
            .arg(source.join("install.sh"))
            .arg("--yes");
        if name == "app-only" {
            install.arg("--app-only");
        }
        let result = install.output().unwrap();
        fs::write(root.join("stdout"), &result.stdout).unwrap();
        fs::write(root.join("stderr"), &result.stderr).unwrap();
        assert!(!root.join("commands.log.python-used").exists());
        assert_eq!(
            fs::read(root.join("home/personal-note")).unwrap(),
            b"preserve unrelated user data\n"
        );
        let requests = fs::read_to_string(root.join("commands.log")).unwrap();
        if name == "foreign-widget" {
            assert!(!result.status.success());
            assert!(!requests.contains("packages"));
            assert!(!app.exists());
            assert_eq!(
                fs::read(widget.join("foreign.qml")).unwrap(),
                b"another owner\n"
            );
            assert_eq!(fs::read_dir(&widget).unwrap().count(), 1);
        } else if name == "package-failure" {
            assert_eq!(result.status.code(), Some(7));
            assert!(!app.exists());
            assert!(!widget.exists());
        } else {
            assert!(requests.contains("packages"));
            assert_eq!(
                fs::read(app.join(".mluva-native.json")).unwrap(),
                fs::read(source.join(".mluva-native.json")).unwrap()
            );
            let launcher = root.join("home/.local/bin/mluva");
            let help = invocation().arg(&launcher).arg("--help").output().unwrap();
            assert!(
                help.status.success(),
                "{name}: {}",
                String::from_utf8_lossy(&help.stderr)
            );
            assert!(help.stderr.is_empty());
            let reference: Value = serde_json::from_str(include_str!(
                "../../mluva-gtk/tests/fixtures/released-bootstrap.json"
            ))
            .unwrap();
            assert_eq!(
                String::from_utf8(help.stdout).unwrap(),
                reference["cli"][0]["stdout"]
            );
            if name == "widget-failure" {
                assert_eq!(result.status.code(), Some(1));
                assert!(
                    String::from_utf8_lossy(&result.stderr)
                        .contains("native app is installed, but widget setup failed")
                );
                assert!(!widget.exists());
            } else {
                assert!(
                    result.status.success(),
                    "{name}: {}",
                    String::from_utf8_lossy(&result.stderr)
                );
                assert!(result.stderr.is_empty());
                if name == "combined" {
                    let manifest: Value =
                        serde_json::from_slice(&fs::read(widget.join("manifest.json")).unwrap())
                            .unwrap();
                    assert_eq!(manifest["version"], "1.6.0");
                    assert_eq!(
                        fs::read(
                            widget.join(manifest["entryPoints"]["barWidget"].as_str().unwrap())
                        )
                        .unwrap(),
                        fs::read(source.join("quickshell/mluva.dictation/Widget.qml")).unwrap()
                    );
                    assert!(widget.join(".mluva-bundle.json").is_file());
                    let calls = fs::read_to_string(root.join("widget-peer/calls.tsv")).unwrap();
                    assert!(calls.contains("plugin\tenable\tmluva.dictation\t"));
                    fs::write(
                        run.join("installed-bundle"),
                        app.as_os_str().as_encoded_bytes(),
                    )
                    .unwrap();
                } else {
                    assert!(!widget.exists());
                    // The documented Bash removal entry point must work even
                    // while deleting the bundle that contains its executable.
                    let removed = invocation()
                        .arg("/usr/bin/bash")
                        .arg(app.join("linux/uninstall.sh"))
                        .output()
                        .unwrap();
                    assert!(
                        removed.status.success(),
                        "{}",
                        String::from_utf8_lossy(&removed.stderr)
                    );
                    assert!(!app.exists());
                    assert!(root.join("home/personal-note").is_file());
                }
            }
        }
        assert!(!root.join("commands.log.python-used").exists());
        eprintln!("Actual combined setup {name} verified");
    }
}

fn normal(value: &[u8], root: &Path) -> String {
    String::from_utf8(value.to_vec())
        .unwrap()
        .replace(root.to_str().unwrap(), "$CASE")
        .replace('\r', "")
}

// The release's runtime requirements survive except Python/GI provisioning.
// OpenSSL and SQLite are direct dependencies observed in the production ELF;
// names are checked against the official Arch/Fedora package catalogs.
fn compare_package_request(observed: &str, expected: &str) -> String {
    let mut result = observed.to_owned();
    for prefix in ["omarchy pkg add ", "sudo dnf install -y "] {
        let Some(old) = expected.lines().find_map(|line| line.strip_prefix(prefix)) else {
            continue;
        };
        let native = observed
            .lines()
            .find_map(|line| line.strip_prefix(prefix))
            .expect("missing package request");
        let mut packages: BTreeSet<&str> = old
            .split_whitespace()
            .filter(|name| {
                ![
                    "uv",
                    "python",
                    "python-gobject",
                    "python-cairo",
                    "python3-gobject",
                    "gobject-introspection",
                ]
                .contains(name)
            })
            .collect();
        packages.extend(if prefix.starts_with("omarchy") {
            ["openssl", "sqlite"]
        } else {
            ["openssl-libs", "sqlite-libs"]
        });
        assert_eq!(
            native.split_whitespace().collect::<BTreeSet<_>>(),
            packages,
            "reviewed native runtime dependencies"
        );
        result = result.replace(&format!("{prefix}{native}"), &format!("{prefix}{old}"));
    }
    result
}

#[test]
#[ignore = "requires a guarded disposable Linux session with private PTYs"]
fn native_setup_matches_released_confirmation_ordering_and_failures() {
    let run = private("setup-comparison");
    let script = env::var_os("MLUVA_TEST_SETUP_SCRIPT")
        .map(PathBuf::from)
        .unwrap_or_else(|| repository().join("rust/mluva-install/resources/setup.sh"));
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/released-setup.json")).unwrap();
    for case in reference["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let root = run.join(name);
        assert!(
            Command::new("/usr/bin/bash")
                .arg(repository().join("rust/mluva-install/tests/support/seed-setup.sh"))
                .arg(&root)
                .arg(name)
                .arg(&script)
                .status()
                .unwrap()
                .success()
        );
        let trap = command(&root).arg("/usr/bin/python3").output().unwrap();
        assert_eq!(trap.status.code(), Some(99));
        assert!(root.join("commands.log.python-used").is_file());
        fs::remove_file(root.join("commands.log.python-used")).unwrap();
        let mut invoke = command(&root);
        for (key, value) in case["environment"].as_object().unwrap() {
            invoke.env(
                key,
                value
                    .as_str()
                    .unwrap()
                    .replace("$CASE", root.to_str().unwrap()),
            );
        }
        if case["tty"].is_null() {
            invoke
                .arg("/usr/bin/bash")
                .arg(root.join("source/install.sh"));
            invoke.args(
                case["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|value| value.as_str().unwrap()),
            );
        } else {
            invoke.args([
                "/usr/bin/script",
                "--quiet",
                "--return",
                "--echo",
                "never",
                "--command",
                "exec /usr/bin/bash \"$MLUVA_SETUP_PROGRAM\"",
                "/dev/null",
            ]);
        }
        let mut child = invoke
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        if let Some(value) = case["tty"].as_str() {
            child
                .stdin
                .take()
                .unwrap()
                .write_all(value.as_bytes())
                .unwrap();
        } else {
            drop(child.stdin.take());
        }
        let result = child.wait_with_output().unwrap();
        fs::write(root.join("stdout"), &result.stdout).unwrap();
        fs::write(root.join("stderr"), &result.stderr).unwrap();
        assert!(!root.join("commands.log.python-used").exists());
        assert_eq!(
            result.status.code(),
            case["result"]["code"].as_i64().map(|value| value as i32),
            "{name}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let requests = normal(
            &fs::read(root.join("commands.log")).unwrap_or_default(),
            &root,
        );
        let observed = json!({"code":result.status.code(),"stdout":normal(&result.stdout,&root),"stderr":normal(&result.stderr,&root),"commands":compare_package_request(&requests,case["result"]["commands"].as_str().unwrap())});
        assert_eq!(observed, case["result"], "{name}");
        assert_eq!(
            fs::read_dir(root.join("home")).unwrap().count(),
            0,
            "setup endpoints must not mutate user state"
        );
        eprintln!("Matched combined setup {name}");
    }
}
