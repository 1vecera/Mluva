//! Verify the actual runtime bundle, without copying a development checkout or
//! inheriting the user's installed app, providers, model caches or desktop.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env, fs,
    io::Read,
    os::unix::{
        fs::{PermissionsExt, symlink},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const PRODUCTION: &[&str] = &[
    "mluva",
    "mluva-shell",
    "mluva-narrate",
    "mluva-asr-worker",
    "mluva-audio-cleanup",
    "mluva-install-widget",
    "mluva-screenshot-editor",
    "mluva-uninstall",
];

fn copy_tree(source: &Path, target: &Path) {
    fs::create_dir_all(target).unwrap();
    for item in fs::read_dir(source).unwrap() {
        let item = item.unwrap();
        let to = target.join(item.file_name());
        if item.file_type().unwrap().is_dir() {
            copy_tree(&item.path(), &to);
        } else {
            fs::copy(item.path(), to).unwrap();
        }
    }
}

fn source(root: &Path, repository: &Path) -> PathBuf {
    let source = root.join("source");
    fs::create_dir_all(&source).unwrap();
    for file in [
        "LICENSE",
        "manifest.json",
        "linux/configure-input-helper.sh",
        "linux/configure-recording-overlay.sh",
    ] {
        let target = source.join(file);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(repository.join(file), target).unwrap();
    }
    for directory in [
        "rust/mluva-gtk/resources",
        "linux/quickshell",
        "linux/gnome-extension",
        "linux/resources",
    ] {
        copy_tree(&repository.join(directory), &source.join(directory));
    }
    // These inputs belong to the old source/development tree, never the runtime.
    for name in ["unrelated.py", "fixture-peer", "private-model-cache"] {
        fs::write(source.join(name), "do not distribute\n").unwrap();
    }
    source
}

fn command(root: &Path, executable: &Path) -> Command {
    let mut command = Command::new(executable);
    // A caller's private umask must not change published bundle file modes.
    unsafe {
        command.pre_exec(|| {
            libc::umask(0o077);
            Ok(())
        });
    }
    command
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", root.join("home"))
        .env("XDG_CONFIG_HOME", root.join("home/.config"))
        .env("XDG_DATA_HOME", root.join("home/.local/share"))
        .env("LC_ALL", "C.UTF-8")
        .env("TMPDIR", root.join("temp"))
        .stdin(Stdio::null());
    if let Some(value) = env::var_os("LD_LIBRARY_PATH") {
        command.env("LD_LIBRARY_PATH", value);
    }
    command
}

fn hash(path: &Path) -> String {
    let mut input = fs::File::open(path).unwrap();
    let mut hash = Sha256::new();
    let mut block = [0; 65536];
    loop {
        let size = input.read(&mut block).unwrap();
        if size == 0 {
            break;
        }
        hash.update(&block[..size]);
    }
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn inventory(root: &Path) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
    fn walk(
        root: &Path,
        path: &Path,
        files: &mut BTreeMap<String, String>,
        links: &mut BTreeMap<String, String>,
    ) {
        for item in fs::read_dir(path).unwrap() {
            let item = item.unwrap();
            let path = item.path();
            let name = path
                .strip_prefix(root)
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned();
            let kind = item.file_type().unwrap();
            if kind.is_dir() {
                assert_eq!(
                    fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                    0o755,
                    "{name}"
                );
                walk(root, &path, files, links);
            } else if kind.is_symlink() {
                links.insert(
                    name,
                    fs::read_link(path).unwrap().to_str().unwrap().to_owned(),
                );
            } else {
                assert!(kind.is_file());
                assert!(
                    !name.ends_with(".py")
                        && !name.ends_with(".pyc")
                        && !name.contains(".venv")
                        && !name.contains("fixture"),
                    "Unshipped implementation in {name}"
                );
                let mode = if name.starts_with("bin/") || name.ends_with(".sh") {
                    0o755
                } else {
                    0o644
                };
                assert_eq!(
                    fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                    mode,
                    "{name}"
                );
                files.insert(name, hash(&path));
            }
        }
    }
    let (mut files, mut links) = (BTreeMap::new(), BTreeMap::new());
    walk(root, root, &mut files, &mut links);
    (files, links)
}

fn no_stages(root: &Path) {
    assert!(
        !fs::read_dir(root).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".mluva-package-")),
        "Leaked package staging"
    );
}

#[test]
#[ignore = "requires built production ELF binaries and a guarded disposable Linux session"]
fn actual_native_bundle_is_complete_private_and_atomic() {
    let private =
        PathBuf::from(env::var_os("OFFSCREEN_SESSION_ROOT").expect("private session required"));
    assert_ne!(
        fs::read_link("/proc/self/ns/net")
            .unwrap()
            .to_str()
            .unwrap(),
        env::var("MLUVA_HOST_NET_NS").unwrap()
    );
    let root = private.join("native-package");
    for path in ["home", "temp"] {
        fs::create_dir_all(root.join(path)).unwrap();
    }
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let source = source(&root, repository);
    let binaries = Path::new(env!("CARGO_BIN_EXE_mluva-package"))
        .parent()
        .unwrap();
    let output = root.join("app");
    let package = |target: &Path| {
        let mut command = command(&root, Path::new(env!("CARGO_BIN_EXE_mluva-package")));
        command.arg(&source).arg(binaries).arg(target);
        command
    };
    let built = package(&output).output().unwrap();
    fs::write(root.join("package.stdout"), &built.stdout).unwrap();
    fs::write(root.join("package.stderr"), &built.stderr).unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let (mut files, links) = inventory(&output);
    files.remove(".mluva-native.json");
    let receipt: Value =
        serde_json::from_slice(&fs::read(output.join(".mluva-native.json")).unwrap()).unwrap();
    assert_eq!(receipt["schema"], 1);
    assert_eq!(receipt["application"], "com.mluva.Linux");
    assert_eq!(receipt["implementation"], "rust");
    assert_eq!(receipt["version"], "1.6.0");
    assert_eq!(receipt["sha256"], json!(files));
    assert_eq!(receipt["links"], json!(links));
    let names: Vec<_> = fs::read_dir(output.join("bin"))
        .unwrap()
        .map(|item| item.unwrap().file_name())
        .collect();
    assert_eq!(names.len(), PRODUCTION.len());
    for name in PRODUCTION {
        assert_eq!(
            hash(&output.join("bin").join(name)),
            hash(&binaries.join(name)),
            "{name} is not the selected built binary"
        );
    }
    for name in ["mluva-shell", "mluva-narrate", "mluva-screenshot-editor"] {
        assert_eq!(
            fs::read_link(output.join(name)).unwrap(),
            Path::new("bin").join(name)
        );
    }
    let widget: Value = serde_json::from_slice(
        &fs::read(output.join("quickshell/mluva.dictation/manifest.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(widget["version"], receipt["version"]);
    assert_eq!(widget["entryPoints"]["barWidget"], "Widget.qml");
    assert_eq!(
        hash(&output.join("quickshell/mluva.dictation/Widget.qml")),
        hash(&repository.join("linux/quickshell/mluva.dictation/Widget.qml"))
    );
    for name in [
        "com.mluva.Linux.svg",
        "mluva-symbolic.svg",
        "fonts/JetBrainsMono-Regular.ttf",
        "mermaid/preview.html",
        "mermaid/mermaid.min.js",
    ] {
        assert_eq!(
            hash(&output.join("resources").join(name)),
            hash(&repository.join("rust/mluva-gtk/resources").join(name)),
            "{name}"
        );
    }
    assert_eq!(fs::read_dir(root.join("home")).unwrap().count(), 0);
    let help = command(&root, &output.join("bin/mluva"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(
        help.status.success(),
        "{}",
        String::from_utf8_lossy(&help.stderr)
    );
    let released: Value = serde_json::from_str(include_str!(
        "../../mluva-gtk/tests/fixtures/released-bootstrap.json"
    ))
    .unwrap();
    assert_eq!(
        String::from_utf8(help.stdout).unwrap(),
        released["cli"][0]["stdout"]
    );
    assert_eq!(
        String::from_utf8(help.stderr).unwrap(),
        released["cli"][0]["stderr"]
    );
    assert_eq!(
        fs::read_dir(root.join("home")).unwrap().count(),
        0,
        "Package validation initialized user stores"
    );
    no_stages(&root);

    // Refusing an occupied or dangling destination must not change its content.
    let before = hash(&output.join(".mluva-native.json"));
    assert!(!package(&output).output().unwrap().status.success());
    assert_eq!(hash(&output.join(".mluva-native.json")), before);
    let dangling = root.join("dangling");
    symlink(root.join("missing"), &dangling).unwrap();
    assert!(!package(&dangling).output().unwrap().status.success());
    assert_eq!(fs::read_link(&dangling).unwrap(), root.join("missing"));

    let original = fs::read(source.join("manifest.json")).unwrap();
    let mut manifest: Value = serde_json::from_slice(&original).unwrap();
    manifest["version"] = json!("999.0.0");
    fs::write(
        source.join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let rejected = package(&root.join("mismatch")).output().unwrap();
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("same release version"));
    assert!(!root.join("mismatch").exists());
    fs::write(source.join("manifest.json"), &original).unwrap();

    let original_icon = source.join("rust/mluva-gtk/resources/mluva-symbolic.svg");
    let bytes = fs::read(&original_icon).unwrap();
    fs::remove_file(&original_icon).unwrap();
    symlink(
        repository.join("rust/mluva-gtk/resources/mluva-symbolic.svg"),
        &original_icon,
    )
    .unwrap();
    let rejected = package(&root.join("linked-assets")).output().unwrap();
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("regular package input"));
    assert!(!root.join("linked-assets").exists());
    no_stages(&root);
    fs::remove_file(&original_icon).unwrap();
    fs::write(&original_icon, bytes).unwrap();
    let forbidden = source.join("rust/mluva-gtk/resources/legacy.py");
    fs::write(&forbidden, "not an asset\n").unwrap();
    let rejected = package(&root.join("legacy-asset")).output().unwrap();
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("Unexpected runtime asset"));
    assert!(!root.join("legacy-asset").exists());
    no_stages(&root);
    fs::remove_file(forbidden).unwrap();

    let nested = source.join("rust/mluva-gtk/resources/nested-output");
    let rejected = package(&nested).output().unwrap();
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("outside its source asset"));
    assert!(!nested.exists());

    // A second owner wins the output name while the actual ELF files copy.
    let collision = root.join("collision");
    let log = fs::File::create(root.join("collision.log")).unwrap();
    let mut child = package(&collision)
        .stdout(log.try_clone().unwrap())
        .stderr(log)
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if fs::read_dir(&root).unwrap().any(|item| {
            item.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".mluva-package-")
        }) {
            break;
        }
        assert!(
            child.try_wait().unwrap().is_none(),
            "Package finished before the publication collision"
        );
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    fs::create_dir(&collision).unwrap();
    fs::write(collision.join("foreign"), "preserve\n").unwrap();
    assert!(!child.wait().unwrap().success());
    assert_eq!(
        fs::read_to_string(collision.join("foreign")).unwrap(),
        "preserve\n"
    );
    assert_eq!(fs::read_dir(&collision).unwrap().count(), 1);
    no_stages(&root);
    eprintln!("Actual native bundle verified at {}", output.display());
}
