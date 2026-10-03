//! Execute the actual launcher with an external native editor process. Stock
//! editor paths are overmounted in a nested private mount namespace.
use serde_json::{Value, json};
use std::{
    env, fs,
    os::unix::{
        ffi::OsStringExt,
        fs::{PermissionsExt, symlink},
    },
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

struct Process(Option<Child>);
impl Drop for Process {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn editor(path: &Path, kind: &str, root: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let peer = env!("CARGO_BIN_EXE_editor-launcher-fixture-peer");
    match kind {
        "missing" => {}
        "directory" => fs::create_dir(path).unwrap(),
        "dangling" => symlink(root.join("missing"), path).unwrap(),
        "bad-interpreter" => {
            fs::write(path, "#!/missing/editor-interpreter\n").unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        "link" => {
            let target = root.join("managed-target");
            fs::copy(peer, &target).unwrap();
            symlink(target, path).unwrap();
        }
        _ => {
            fs::copy(peer, path).unwrap();
            fs::set_permissions(
                path,
                fs::Permissions::from_mode(match kind {
                    "not-executable" => 0o644,
                    "group-only" => 0o010,
                    _ => 0o755,
                }),
            )
            .unwrap();
        }
    }
}

fn setup(root: &Path, spec: &Value) -> PathBuf {
    fs::create_dir_all(root.join("peer")).unwrap();
    let bin = root.join("home/.local/share/mluva/app/bin");
    fs::create_dir_all(&bin).unwrap();
    let binary = bin.join("mluva-screenshot-editor");
    fs::hard_link(env!("CARGO_BIN_EXE_mluva-screenshot-editor"), &binary).unwrap();
    editor(
        &root.join("home/.local/share/mluva/tensaku/tensaku"),
        spec["managed"].as_str().unwrap(),
        root,
    );
    editor(&root.join("stock"), spec["stock"].as_str().unwrap(), root);
    if spec["narrator"].as_bool().unwrap() {
        fs::copy(
            env!("CARGO_BIN_EXE_editor-launcher-fixture-peer"),
            bin.join("mluva-narrate"),
        )
        .unwrap();
    }
    binary
}

fn command(root: &Path, binary: &Path, spec: &Value) -> Command {
    let mut command = Command::new("/usr/bin/bwrap");
    command.args([
        "--die-with-parent",
        "--bind",
        "/",
        "/",
        "--tmpfs",
        "/usr/bin",
        "--ro-bind",
        "/usr/bin/bash",
        "/usr/bin/bash",
        "--ro-bind",
        "/usr/bin/env",
        "/usr/bin/env",
    ]);
    if spec["stock"] != "missing" {
        command
            .arg("--ro-bind")
            .arg(root.join("stock"))
            .arg("/usr/bin/tensaku-edit");
    }
    command
        .args([
            "--",
            "/usr/bin/bash",
            "-c",
            "printf '%s\\n' \"$$\" > \"$MLUVA_EDITOR_PEER_DIR/initial-pid\"; exec \"$@\"",
            "entry",
        ])
        .arg(binary)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", root.join("home"))
        .env("LC_ALL", "C.UTF-8")
        .env("MLUVA_EDITOR_PEER_DIR", root.join("peer"))
        .env("MLUVA_EDITOR_PEER_EXIT", spec["exit"].to_string())
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if spec["wait"] == true {
        command.env("MLUVA_EDITOR_PEER_WAIT", "1");
    }
    command
}

fn receipt(root: &Path) -> Option<Value> {
    fs::read(root.join("peer/receipt.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

fn execute(root: &Path, mut command: Command, waiting: bool) -> Output {
    let mut process = Process(Some(command.spawn().unwrap()));
    if waiting {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(value) = receipt(root) {
                assert_eq!(
                    unsafe { libc::kill(value["pid"].as_i64().unwrap() as i32, libc::SIGTERM) },
                    0
                );
                break;
            }
            assert!(process.0.as_mut().unwrap().try_wait().unwrap().is_none());
            assert!(
                Instant::now() < deadline,
                "Editor did not publish its invocation"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }
    process.0.take().unwrap().wait_with_output().unwrap()
}

fn observe(root: &Path, result: Output) -> Value {
    fs::write(root.join("stdout"), &result.stdout).unwrap();
    fs::write(root.join("stderr"), &result.stderr).unwrap();
    let mut receipt = receipt(root);
    if let Some(value) = &mut receipt {
        value.as_object_mut().unwrap().remove("pid");
        for arg in value["arguments"].as_array_mut().unwrap() {
            let bytes: Vec<u8> = serde_json::from_value(arg.clone()).unwrap();
            let text = String::from_utf8(bytes)
                .unwrap()
                .replace(
                    root.join("home/.local/share/mluva/app/bin/mluva-narrate")
                        .to_str()
                        .unwrap(),
                    "$NARRATOR",
                )
                .replace(root.to_str().unwrap(), "$CASE");
            *arg = json!(text.as_bytes());
        }
    }
    let error = if matches!(result.status.code(), Some(126 | 127)) && receipt.is_none() {
        assert_eq!(
            String::from_utf8(result.stderr).unwrap(),
            "Mluva screenshot editor could not start. Check its installed editor and try again.\n"
        );
        "exec-failed".to_owned()
    } else {
        String::from_utf8(result.stderr).unwrap()
    };
    json!({"code":result.status.code(),"stdout":String::from_utf8(result.stdout).unwrap(),"stderr":error,"receipt":receipt})
}

#[test]
#[ignore = "requires the guarded Linux runner, nested mounts and no host editor access"]
fn released_editor_launch_processes() {
    let private =
        PathBuf::from(env::var_os("OFFSCREEN_SESSION_ROOT").expect("private session required"));
    assert_ne!(
        fs::read_link("/proc/self/ns/net")
            .unwrap()
            .to_str()
            .unwrap(),
        env::var("MLUVA_HOST_NET_NS").unwrap()
    );
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-editor-launcher.json")).unwrap();
    let run = private.join("native-editor-launcher");
    fs::create_dir(&run).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let spec = &case["input"];
        let name = spec["name"].as_str().unwrap();
        let root = run.join(name);
        let binary = setup(&root, spec);
        let mut command = command(&root, &binary, spec);
        for arg in spec["args"].as_array().unwrap() {
            command.arg(
                arg.as_str()
                    .unwrap()
                    .replace("$CASE", root.to_str().unwrap()),
            );
        }
        let result = execute(&root, command, spec["wait"].as_bool().unwrap());
        let actual = observe(&root, result);
        fs::write(
            root.join("result.json"),
            serde_json::to_vec_pretty(&actual).unwrap(),
        )
        .unwrap();
        assert_eq!(actual, case["result"], "{name}");
        eprintln!("editor launcher {name}: matched");
    }
    let root = run.join("non-utf8-name");
    let spec = &fixture["cases"][0]["input"];
    let binary = setup(&root, spec);
    let mut command = command(&root, &binary, spec);
    let filename = b"image-\xff.png".to_vec();
    command.arg(std::ffi::OsString::from_vec(filename.clone()));
    let result = execute(&root, command, false);
    assert!(result.status.success());
    let actual = receipt(&root).unwrap();
    assert_eq!(actual["arguments"][1], json!(filename));
    assert_eq!(actual["arguments"][3], json!(filename));
    assert_eq!(actual["same_pid"], true);
    eprintln!(
        "{} released editor processes and one native byte-path case passed",
        fixture["cases"].as_array().unwrap().len()
    );
}
