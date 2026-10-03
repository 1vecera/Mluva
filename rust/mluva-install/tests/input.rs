//! Compare service ownership through the real public helper, with the system
//! unit directory privately mounted and privileged service operations receipted.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    os::unix::fs::{MetadataExt, PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

fn command(root: &Path) -> Command {
    let units = fs::metadata(root.join("units")).unwrap();
    let mut command = Command::new("/usr/bin/bwrap");
    command
        .args(["--die-with-parent", "--bind", "/", "/", "--dev", "/dev", "--bind"])
        .arg(root.join("units"))
        .args([
            "/etc/systemd/system",
            "--tmpfs",
            "/run/systemd",
            "--tmpfs",
            "/run/dbus",
            "--ro-bind",
        ])
        .arg(root.join("bin/python-trap"))
        .args([
            "/usr/bin/python3",
            "--",
            "/usr/bin/bash",
            "-c",
            "[[ \"$(stat -Lc %d:%i /etc/systemd/system)\" == \"$MLUVA_INPUT_UNIT_DIR\" ]] || exit 98; exec \"$@\"",
            "entry",
        ])
        .env_clear()
        .env("PATH", format!("{}:/usr/bin:/bin", root.join("bin").display()))
        .env("HOME", root.join("home"))
        .env("LC_ALL", "C.UTF-8")
        .env("MLUVA_INPUT_PEER_DIR", root.join("peer"))
        .env("MLUVA_INPUT_UNIT_DIR", format!("{}:{}", units.dev(), units.ino()))
        .current_dir(root)
        .stdin(Stdio::null());
    command
}

fn state(path: &Path, root: &Path) -> Value {
    let Ok(metadata) = path.symlink_metadata() else {
        return Value::Null;
    };
    if metadata.is_symlink() {
        json!({"link": fs::read_link(path).unwrap().to_str().unwrap().replace(root.to_str().unwrap(), "$CASE")})
    } else if metadata.is_dir() {
        json!({"directory": true})
    } else {
        let hash: String = Sha256::digest(fs::read(path).unwrap())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        json!({"sha256": hash})
    }
}

#[test]
#[ignore = "requires the guarded Linux runner and private system-unit/interpreter mounts"]
fn released_input_ownership_without_python() {
    let private =
        PathBuf::from(env::var_os("OFFSCREEN_SESSION_ROOT").expect("private session required"));
    assert_ne!(
        fs::read_link("/proc/self/ns/net")
            .unwrap()
            .to_str()
            .unwrap(),
        env::var("MLUVA_HOST_NET_NS").unwrap()
    );
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let helper = env::var_os("MLUVA_TEST_INPUT_HELPER")
        .map(PathBuf::from)
        .unwrap_or_else(|| repository.join("linux/configure-input-helper.sh"));
    let template = fs::read(repository.join("linux/resources/mluva-input@.service")).unwrap();
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-input-ownership.json")).unwrap();
    let run = private.join("native-input-ownership");
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let root = run.join(name);
        for directory in ["app/resources", "units", "bin", "peer", "home"] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        let script = root.join("app/configure-input-helper.sh");
        fs::copy(&helper, &script).unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let source = root.join("app/resources/mluva-input@.service");
        fs::write(&source, &template).unwrap();
        let public = root.join("bin/mluva-input-helper");
        symlink(&script, &public).unwrap();
        for name in ["id", "sudo", "python-trap"] {
            let path = root.join("bin").join(name);
            fs::write(&path, include_bytes!("support/input-peer.sh")).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let unit = root.join("units/mluva-input@.service");
        match name {
            "same" | "changed" | "newline" | "nul" => {
                let mut contents = template.clone();
                contents.extend_from_slice(match name {
                    "changed" => b"changed\n",
                    "newline" => b"\n",
                    "nul" => b"\0",
                    _ => b"",
                });
                fs::write(&unit, contents).unwrap();
            }
            "link" => symlink(&source, &unit).unwrap(),
            "dangling" => symlink(root.join("missing"), &unit).unwrap(),
            "directory" => fs::create_dir(&unit).unwrap(),
            "absent" => {}
            _ => panic!("Unknown reference case {name}"),
        }
        // Independently prove this exact mount intercepts absolute Python calls.
        let trap = command(&root).arg("/usr/bin/python3").output().unwrap();
        assert_eq!(trap.status.code(), Some(99));
        let python_used = root.join("peer/python-used");
        assert!(python_used.exists());
        fs::remove_file(&python_used).unwrap();
        let result = command(&root).arg(public).arg("remove").output().unwrap();
        fs::write(root.join("stdout"), &result.stdout).unwrap();
        fs::write(root.join("stderr"), &result.stderr).unwrap();
        let actual = json!({
            "name": name,
            "code": result.status.code(),
            "stdout": String::from_utf8(result.stdout).unwrap(),
            "stderr": String::from_utf8(result.stderr).unwrap(),
            "commands": fs::read_to_string(root.join("peer/commands")).unwrap_or_default(),
            "unit": state(&unit, &root),
        });
        fs::write(
            root.join("result.json"),
            serde_json::to_vec_pretty(&actual).unwrap(),
        )
        .unwrap();
        assert!(
            !python_used.exists(),
            "{name}: public input helper invoked Python"
        );
        assert_eq!(&actual, case, "{name}");
        eprintln!("input ownership {name}: matched without Python");
    }
}
