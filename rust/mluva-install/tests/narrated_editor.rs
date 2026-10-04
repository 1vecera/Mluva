//! Exercise the editor's existing Bash/Rust installation contract without a
//! Python test runner. Package acceptance also uses a real prebuilt editor.
use std::{
    env, fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

const METADATA: &[&str] = &["LICENSE", "NOTICE", "upstream-commit", "narration.patch"];

fn repository() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}

fn executable(path: &Path, content: &[u8]) {
    fs::write(path, content).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn command(root: &Path, program: &Path) -> Command {
    let mut command = Command::new("/usr/bin/bash");
    command
        .arg(program)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", root.join("other-home"))
        .env("MLUVA_INSTALL_HOME", root.join("home"))
        .env("XDG_DATA_HOME", root.join("other-data"))
        .env("MLUVA_EDITOR_HELP_WITNESS", root.join("editor-invoked"))
        .stdin(Stdio::null());
    command
}

fn input(root: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let app = root.join("home/.local/share/mluva/app");
    let bin = root.join("home/.local/bin");
    let prebuilt = root.join("prebuilt editor Ω");
    for directory in [&app, &bin, &prebuilt] {
        fs::create_dir_all(directory).unwrap();
    }
    executable(&app.join("mluva-screenshot-editor"), b"#!/bin/sh\nexit 0\n");
    executable(&bin.join("mluva-narrate"), b"#!/bin/sh\nexit 0\n");
    for name in METADATA {
        fs::copy(
            repository().join("linux/integrations/tensaku").join(name),
            prebuilt.join(name),
        )
        .unwrap();
    }
    executable(&prebuilt.join("tensaku"), b"#!/bin/sh\nprintf invoked > \"$MLUVA_EDITOR_HELP_WITNESS\"\nprintf '%s\\n' --narration-command\n");
    (app, bin, prebuilt)
}

fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert_eq!(
        output.stdout,
        b"Narration enabled in the default Tensaku screenshot editor.\n"
    );
}

#[test]
fn prebuilt_activation_preserves_metadata_and_unrelated_editors() {
    let script = env::var_os("MLUVA_TEST_NARRATED_INSTALLER")
        .map(PathBuf::from)
        .unwrap_or_else(|| repository().join("linux/install-narrated-editor.sh"));
    for (name, status, message) in [
        ("fresh", 0, ""),
        ("owned", 0, ""),
        (
            "patch",
            5,
            "Editor source metadata does not match this Mluva release: narration.patch",
        ),
        (
            "metadata-link",
            5,
            "Editor source metadata does not match this Mluva release: LICENSE",
        ),
        (
            "missing-narrator",
            3,
            "Install or upgrade Mluva before activating the narrated editor.",
        ),
        ("foreign-editor", 4, "Preserved an unrelated user editor:"),
        ("nonexecutable", 5, "Missing prebuilt Tensaku executable."),
        ("no-narration", 1, ""),
    ] {
        let root = tempfile::Builder::new()
            .prefix("narrated-install-")
            .tempdir()
            .unwrap();
        let root = root.path();
        let (app, bin, prebuilt) = input(root);
        let destination = root.join("home/.local/share/mluva/tensaku");
        match name {
            "owned" => {
                symlink(
                    app.join("mluva-screenshot-editor"),
                    bin.join("tensaku-edit"),
                )
                .unwrap();
                fs::create_dir(&destination).unwrap();
                fs::write(destination.join("personal-note"), b"preserve this note\n").unwrap();
            }
            "patch" => fs::write(prebuilt.join("narration.patch"), b"another release\n").unwrap(),
            "metadata-link" => {
                fs::rename(prebuilt.join("LICENSE"), root.join("outside-license")).unwrap();
                symlink(root.join("outside-license"), prebuilt.join("LICENSE")).unwrap();
            }
            "missing-narrator" => fs::remove_file(bin.join("mluva-narrate")).unwrap(),
            "foreign-editor" => {
                fs::write(bin.join("tensaku-edit"), b"user-owned editor\n").unwrap()
            }
            "nonexecutable" => {
                fs::set_permissions(prebuilt.join("tensaku"), fs::Permissions::from_mode(0o644))
                    .unwrap()
            }
            "no-narration" => executable(
                &prebuilt.join("tensaku"),
                b"#!/bin/sh\nprintf invoked > \"$MLUVA_EDITOR_HELP_WITNESS\"\n",
            ),
            _ => {}
        }
        let result = command(root, &script)
            .arg("--prebuilt-dir")
            .arg(&prebuilt)
            .output()
            .unwrap();
        assert_eq!(
            result.status.code(),
            Some(status),
            "{name}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(
            String::from_utf8_lossy(&result.stderr).contains(message),
            "{name}"
        );
        if status == 0 {
            success(&result);
            for name in METADATA.iter().copied().chain(["tensaku"]) {
                assert_eq!(
                    fs::read(destination.join(name)).unwrap(),
                    fs::read(prebuilt.join(name)).unwrap(),
                    "{name}"
                );
                let mode = if name == "tensaku" { 0o755 } else { 0o644 };
                assert_eq!(
                    fs::metadata(destination.join(name))
                        .unwrap()
                        .permissions()
                        .mode()
                        & 0o777,
                    mode
                );
            }
            assert_eq!(
                fs::read_link(bin.join("tensaku-edit")).unwrap(),
                app.join("mluva-screenshot-editor")
            );
            assert!(root.join("editor-invoked").is_file());
            if name == "owned" {
                assert_eq!(
                    fs::read(destination.join("personal-note")).unwrap(),
                    b"preserve this note\n"
                );
            }
        } else {
            assert!(!destination.exists());
            assert_eq!(
                root.join("editor-invoked").exists(),
                name == "no-narration",
                "Only an admitted executable may be queried for narration support: {name}"
            );
            if name == "foreign-editor" {
                assert_eq!(
                    fs::read(bin.join("tensaku-edit")).unwrap(),
                    b"user-owned editor\n"
                );
            } else {
                assert!(!bin.join("tensaku-edit").exists());
            }
        }
        assert!(!root.join("other-data").exists());
        eprintln!("Verified narrated editor {name}");
    }
}

#[test]
#[ignore = "requires an actual native package, prebuilt Tensaku and the guarded disposable session"]
fn native_package_can_install_and_upgrade_the_narrated_editor() {
    let session = PathBuf::from(env::var_os("OFFSCREEN_SESSION_ROOT").unwrap());
    assert_ne!(
        fs::read_link("/proc/self/ns/net")
            .unwrap()
            .to_string_lossy(),
        env::var("MLUVA_HOST_NET_NS").unwrap()
    );
    let bundle = PathBuf::from(env::var_os("MLUVA_TEST_NATIVE_BUNDLE").unwrap());
    let prebuilt = PathBuf::from(env::var_os("MLUVA_TEST_TENSAKU_BUNDLE").unwrap());
    let root = session.join("narrated-editor-package");
    for name in ["home", "other-home", "temp", "tools"] {
        fs::create_dir_all(root.join(name)).unwrap();
    }
    let trap = root.join("python-trap");
    executable(
        &trap,
        b"#!/bin/sh\ntouch \"$MLUVA_PYTHON_WITNESS\"\nexit 99\n",
    );
    for name in ["pw-record", "pw-dump", "wl-copy"] {
        symlink("/usr/bin/false", root.join("tools").join(name)).unwrap();
    }
    let invocation = || {
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
            .arg(&trap)
            .args(["/usr/bin/python3", "--"])
            .env_clear()
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", root.join("tools").display()),
            )
            .env("HOME", root.join("other-home"))
            .env("MLUVA_INSTALL_HOME", root.join("home"))
            .env("TMPDIR", root.join("temp"))
            .env("MLUVA_PYTHON_WITNESS", root.join("python-used"))
            .env("XDG_CURRENT_DESKTOP", "offscreen")
            .stdin(Stdio::null());
        if let Some(value) = env::var_os("LD_LIBRARY_PATH") {
            command.env("LD_LIBRARY_PATH", value);
        }
        command
    };
    assert_eq!(
        invocation()
            .arg("/usr/bin/python3")
            .status()
            .unwrap()
            .code(),
        Some(99)
    );
    fs::remove_file(root.join("python-used")).unwrap();
    let installed = invocation()
        .arg("/usr/bin/bash")
        .arg(bundle.join("linux/install.sh"))
        .output()
        .unwrap();
    fs::write(root.join("app.stdout"), &installed.stdout).unwrap();
    fs::write(root.join("app.stderr"), &installed.stderr).unwrap();
    assert!(
        installed.status.success(),
        "{}",
        String::from_utf8_lossy(&installed.stderr)
    );
    let app = root.join("home/.local/share/mluva/app");
    let destination = root.join("home/.local/share/mluva/tensaku");
    for source in [&bundle, &app] {
        let result = invocation()
            .arg("/usr/bin/bash")
            .arg(source.join("linux/install-narrated-editor.sh"))
            .arg("--prebuilt-dir")
            .arg(&prebuilt)
            .output()
            .unwrap();
        fs::write(root.join("editor.stdout"), &result.stdout).unwrap();
        fs::write(root.join("editor.stderr"), &result.stderr).unwrap();
        success(&result);
        for name in METADATA.iter().copied().chain(["tensaku"]) {
            assert_eq!(
                fs::read(destination.join(name)).unwrap(),
                fs::read(prebuilt.join(name)).unwrap(),
                "{name}"
            );
        }
        if source == &bundle {
            fs::write(destination.join("personal-note"), b"preserve on upgrade\n").unwrap();
        }
    }
    assert_eq!(
        fs::read(destination.join("personal-note")).unwrap(),
        b"preserve on upgrade\n"
    );
    assert_eq!(
        fs::read_link(root.join("home/.local/bin/tensaku-edit")).unwrap(),
        app.join("mluva-screenshot-editor")
    );
    let help = invocation()
        .arg(destination.join("tensaku"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(
        help.status.success(),
        "{}",
        String::from_utf8_lossy(&help.stderr)
    );
    assert!(String::from_utf8_lossy(&help.stdout).contains("--narration-command"));
    let build = invocation()
        .arg("/usr/bin/bash")
        .arg(app.join("linux/build-narrated-editor.sh"))
        .arg("relative-output")
        .output()
        .unwrap();
    assert_eq!(build.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&build.stderr).starts_with("Usage: build-narrated-editor.sh"));
    assert!(!root.join("python-used").exists());
    eprintln!("Actual package activated the prebuilt editor and preserved its upgrade state");
}
