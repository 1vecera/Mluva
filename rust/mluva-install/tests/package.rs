//! Verify the actual runtime bundle, without copying a development checkout or
//! inheriting the user's installed app, providers, model caches or desktop.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
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
    "mluva-install",
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
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        "linux/configure-input-helper.sh",
        "linux/configure-recording-overlay.sh",
        "linux/install-narrated-editor.sh",
        "linux/build-narrated-editor.sh",
    ] {
        let target = source.join(file);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(repository.join(file), target).unwrap();
    }
    for directory in [
        "rust",
        "linux/quickshell",
        "linux/gnome-extension",
        "linux/resources",
        "linux/integrations/tensaku",
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
        .env(
            "PATH",
            format!(
                "{}:/usr/bin:/bin",
                Path::new(env!("CARGO")).parent().unwrap().display()
            ),
        )
        .env("HOME", root.join("home"))
        .env("XDG_CONFIG_HOME", root.join("home/.config"))
        .env("XDG_DATA_HOME", root.join("home/.local/share"))
        .env("LC_ALL", "C.UTF-8")
        .env("TMPDIR", root.join("temp"))
        .stdin(Stdio::null());
    if let Some(value) = env::var_os("LD_LIBRARY_PATH") {
        command.env("LD_LIBRARY_PATH", value);
    }
    command.env("CARGO", env!("CARGO"));
    for name in ["CARGO_HOME", "RUSTUP_HOME"] {
        if let Some(value) = env::var_os(name) {
            command.env(name, value);
        }
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
    assert_eq!(
        fs::read(output.join("third-party-licenses/onnx-asr/LICENSE"))
            .expect("The native ASR data and derived implementation need their upstream notice"),
        fs::read(repository.join("rust/mluva-asr/resources/onnx-asr-LICENSE")).unwrap()
    );
    let dependency_inventory: Value =
        serde_json::from_slice(&fs::read(output.join("RUST-DEPENDENCIES.json")).unwrap()).unwrap();
    assert_eq!(dependency_inventory["schema"], 1);
    // The statically linked standard library is outside Cargo.lock. Compare its
    // complete upstream notice payload to the actual pinned compiler installation.
    let rustc = Path::new(env!("CARGO")).with_file_name("rustc");
    let sysroot = command(&root, &rustc)
        .args(["--print", "sysroot"])
        .output()
        .unwrap();
    assert!(sysroot.status.success());
    let compiler_docs =
        Path::new(String::from_utf8(sysroot.stdout).unwrap().trim()).join("share/doc/rust");
    let mut compiler_notices = vec!["COPYRIGHT-library.html".to_owned()];
    compiler_notices.extend(
        fs::read_dir(compiler_docs.join("licenses"))
            .unwrap()
            .map(|entry| format!("licenses/{}", entry.unwrap().file_name().to_str().unwrap())),
    );
    compiler_notices.sort();
    for name in &compiler_notices {
        assert_eq!(
            fs::read(
                output
                    .join("third-party-licenses/rust-standard-library")
                    .join(name)
            )
            .expect("The linked Rust standard library needs its upstream notices"),
            fs::read(compiler_docs.join(name)).unwrap()
        );
    }
    let compiler = &dependency_inventory["toolchain"];
    assert_eq!(compiler["notices"], json!(compiler_notices));
    assert_eq!(
        compiler["notice_directory"],
        "third-party-licenses/rust-standard-library"
    );
    let version = command(&root, &rustc).arg("--version").output().unwrap();
    assert!(version.status.success());
    assert_eq!(
        compiler["rustc"].as_str().unwrap(),
        String::from_utf8(version.stdout).unwrap().trim()
    );
    let dependencies = dependency_inventory["packages"].as_array().unwrap();
    let identities: BTreeSet<_> = dependencies
        .iter()
        .map(|item| {
            (
                item["name"].as_str().unwrap().to_owned(),
                item["version"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(identities.len(), dependencies.len());
    // Cargo metadata also resolves optional edges that are not active in this
    // build (for example ort's ndarray feature). Keep their notices, but require
    // coverage of every dependency in Cargo's independently selected build tree.
    let tree = command(&root, Path::new(env!("CARGO")))
        .current_dir(&source)
        .args([
            "tree",
            "--locked",
            "--offline",
            "--workspace",
            "--target",
            dependency_inventory["target"].as_str().unwrap(),
            "--edges",
            "normal,build",
            "--prefix",
            "none",
            "--format",
            "{p}",
        ])
        .output()
        .unwrap();
    assert!(
        tree.status.success(),
        "{}",
        String::from_utf8_lossy(&tree.stderr)
    );
    let expected: BTreeSet<_> = String::from_utf8(tree.stdout)
        .unwrap()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let name = fields.next().unwrap();
            let version = fields.next().unwrap().strip_prefix('v').unwrap();
            if fields
                .next()
                .is_some_and(|location| location.starts_with("(/"))
            {
                None
            } else {
                Some((name.to_owned(), version.to_owned()))
            }
        })
        .collect();
    assert!(
        expected.is_subset(&identities),
        "Missing built dependency notices: {:?}",
        expected.difference(&identities).collect::<Vec<_>>()
    );
    let lock: toml::Value =
        toml::from_str(&fs::read_to_string(source.join("Cargo.lock")).unwrap()).unwrap();
    for dependency in dependencies {
        let locked = lock["package"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| {
                item["name"].as_str() == dependency["name"].as_str()
                    && item["version"].as_str() == dependency["version"].as_str()
            })
            .unwrap();
        assert_eq!(dependency["checksum"].as_str(), locked["checksum"].as_str());
        assert!(
            dependency["license"]
                .as_str()
                .is_some_and(|license| !license.is_empty())
        );
        let notices = dependency["notices"].as_array().unwrap();
        assert!(!notices.is_empty());
        for notice in notices {
            assert!(
                !fs::read(
                    output
                        .join(dependency["notice_directory"].as_str().unwrap())
                        .join(notice.as_str().unwrap())
                )
                .unwrap()
                .is_empty()
            );
        }
    }
    // ring's upstream LICENSE explicitly refers to both root notices and these
    // nested once_cell licenses. A root-only collector would lose them.
    let ring = dependencies
        .iter()
        .find(|item| item["name"] == "ring")
        .unwrap();
    let ring_identity = format!("ring-{}", ring["version"].as_str().unwrap());
    let cache = PathBuf::from(env::var_os("CARGO_HOME").unwrap()).join("registry/cache");
    let ring_archives: Vec<_> = fs::read_dir(cache)
        .unwrap()
        .map(|directory| {
            directory
                .unwrap()
                .path()
                .join(format!("{ring_identity}.crate"))
        })
        .filter(|path| path.is_file())
        .collect();
    let ring_archive = ring_archives
        .iter()
        .find(|path| hash(path) == ring["checksum"].as_str().unwrap())
        .unwrap();
    for notice in [
        "LICENSE",
        "LICENSE-BoringSSL",
        "LICENSE-other-bits",
        "src/polyfill/once_cell/LICENSE-APACHE",
        "src/polyfill/once_cell/LICENSE-MIT",
    ] {
        assert!(ring["notices"].as_array().unwrap().contains(&json!(notice)));
        let upstream = Command::new("/usr/bin/tar")
            .arg("-xzOf")
            .arg(ring_archive)
            .arg(format!("{ring_identity}/{notice}"))
            .output()
            .unwrap();
        assert!(
            upstream.status.success(),
            "{}",
            String::from_utf8_lossy(&upstream.stderr)
        );
        assert_eq!(
            fs::read(
                output
                    .join(ring["notice_directory"].as_str().unwrap())
                    .join(notice)
            )
            .unwrap(),
            upstream.stdout
        );
    }
    assert!(output.join("THIRD_PARTY_NOTICES.md").is_file());
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

    // A bad cached archive must fail before publication. Overlay all matching
    // cache paths only in this child namespace; the actual cache is unchanged.
    let damaged = root.join("damaged.crate");
    fs::write(&damaged, b"corrupt dependency archive\n").unwrap();
    let rejected_notice = root.join("rejected-notice");
    let mut blocked = command(&root, Path::new("/usr/bin/bwrap"));
    blocked.args(["--die-with-parent", "--bind", "/", "/", "--dev", "/dev"]);
    for path in &ring_archives {
        blocked.arg("--ro-bind").arg(&damaged).arg(path);
    }
    let blocked = blocked
        .arg("--")
        .arg(env!("CARGO_BIN_EXE_mluva-package"))
        .arg(&source)
        .arg(binaries)
        .arg(&rejected_notice)
        .output()
        .unwrap();
    assert!(!blocked.status.success());
    assert!(
        String::from_utf8_lossy(&blocked.stderr).contains(&format!(
            "No checksum-verified cached archive for {ring_identity}"
        )),
        "{}",
        String::from_utf8_lossy(&blocked.stderr)
    );
    assert!(!rejected_notice.exists());
    assert_eq!(hash(ring_archive), ring["checksum"].as_str().unwrap());
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
    // Cargo's cache can be relative to the caller, while the source checkout is
    // elsewhere. Both metadata and notice collection must use that same cache.
    symlink(env::var_os("CARGO_HOME").unwrap(), root.join("cargo-cache")).unwrap();
    let archive = root.join("runtime.tar.gz");
    let archived = command(&root, Path::new(env!("CARGO_BIN_EXE_mluva-package")))
        .current_dir(&root)
        .env("CARGO_HOME", "cargo-cache")
        .arg("--archive")
        .arg(&source)
        .arg(binaries)
        .arg(&archive)
        .output()
        .unwrap();
    assert!(
        archived.status.success(),
        "{}",
        String::from_utf8_lossy(&archived.stderr)
    );
    assert_eq!(
        fs::metadata(&archive).unwrap().permissions().mode() & 0o777,
        0o644
    );
    let extracted = root.join("extracted");
    fs::create_dir(&extracted).unwrap();
    let unpacked = Command::new("/usr/bin/tar")
        .args(["-xzf"])
        .arg(&archive)
        .arg("-C")
        .arg(&extracted)
        .output()
        .unwrap();
    assert!(
        unpacked.status.success(),
        "{}",
        String::from_utf8_lossy(&unpacked.stderr)
    );
    assert_eq!(fs::read_dir(&extracted).unwrap().count(), 1);
    let extracted = extracted.join("mluva-1.6.0");
    let (mut archived_files, archived_links) = inventory(&extracted);
    archived_files.remove(".mluva-native.json");
    assert_eq!(archived_files, files);
    assert_eq!(archived_links, links);
    assert_eq!(
        hash(&extracted.join(".mluva-native.json")),
        hash(&output.join(".mluva-native.json"))
    );
    let repeated = root.join("repeated.tar.gz");
    let result = command(&root, Path::new(env!("CARGO_BIN_EXE_mluva-package")))
        .arg("--archive")
        .arg(&source)
        .arg(binaries)
        .arg(&repeated)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        hash(&archive),
        hash(&repeated),
        "Archive depends on staging time, owner or directory iteration order"
    );
    let occupied = command(&root, Path::new(env!("CARGO_BIN_EXE_mluva-package")))
        .arg("--archive")
        .arg(&source)
        .arg(binaries)
        .arg(&archive)
        .output()
        .unwrap();
    assert!(!occupied.status.success());
    assert_eq!(hash(&archive), hash(&repeated));
    no_stages(&root);
    eprintln!("Actual native bundle verified at {}", output.display());
}
