//! Actual installer processes and files compared with frozen v1.6.0 results.
//! Only external shell discovery is synthetic; Git and schema validation run.
use regex::Regex;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env, fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::{Command, Output},
};

const ID: &str = "mluva.dictation";
const SOURCE: &str = "linux/quickshell/mluva.dictation";

fn sha256(bytes: impl AsRef<[u8]>) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn copy_tree(source: &Path, target: &Path) {
    fs::create_dir_all(target).unwrap();
    for item in fs::read_dir(source).unwrap() {
        let item = item.unwrap();
        let destination = target.join(item.file_name());
        if item.file_type().unwrap().is_dir() {
            copy_tree(&item.path(), &destination);
        } else {
            fs::copy(item.path(), destination).unwrap();
        }
    }
    fs::set_permissions(target, fs::metadata(source).unwrap().permissions()).unwrap();
}

fn normalize(root: &Path, value: &str) -> String {
    let mut result = value.replace(root.to_str().unwrap(), "$CASE");
    for (pattern, replacement) in [
        (r#"\$CASE/temp/mluva-widget-[^/\s'"\],]+"#, "$$SOURCE"),
        (
            r#"\$CASE/home/\.config/omarchy/\.mluva-stage-[^/\s'"\],]+"#,
            "$$STAGE",
        ),
        (
            r#"plugin-backups/mluva\.dictation-[^/\s'"\],]+"#,
            "plugin-backups/$$BACKUP",
        ),
    ] {
        result = Regex::new(pattern)
            .unwrap()
            .replace_all(&result, replacement)
            .into_owned();
    }
    result
}

fn snapshot(root: &Path, tree: &Path) -> Value {
    fn visit(root: &Path, tree: &Path, path: &Path, result: &mut BTreeMap<String, Value>) {
        for item in fs::read_dir(path).unwrap() {
            let item = item.unwrap();
            if item.file_name() == ".git" {
                continue;
            }
            let path = item.path();
            let key = normalize(root, path.strip_prefix(tree).unwrap().to_str().unwrap());
            let kind = item.file_type().unwrap();
            let mode = format!(
                "{:o}",
                fs::symlink_metadata(&path).unwrap().permissions().mode() & 0o777
            );
            if kind.is_symlink() {
                result.insert(key, json!({"link": normalize(root, fs::read_link(path).unwrap().to_str().unwrap())}));
            } else if kind.is_dir() {
                result.insert(key, json!({"mode":mode}));
                visit(root, tree, &path, result);
            } else {
                result.insert(
                    key,
                    json!({"mode":mode, "sha256":sha256(fs::read(path).unwrap())}),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    if tree.exists() {
        visit(root, tree, tree, &mut result);
    }
    json!(result)
}

fn command(root: &Path, executable: impl AsRef<std::ffi::OsStr>, spec: &Value) -> Command {
    let mut command = Command::new(executable);
    command
        .env_clear()
        .current_dir(root)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", root.join("bin").display()),
        )
        .env("HOME", root.join("home"))
        .env("TMPDIR", root.join("temp"))
        .env("LC_ALL", "C.UTF-8")
        .env("MLUVA_WIDGET_PEER_DIR", root.join("peer"))
        .env(
            "MLUVA_WIDGET_FAILURE",
            spec["failure"].as_str().unwrap_or(""),
        )
        .env(
            "MLUVA_WIDGET_DELAY",
            spec["delay"].as_u64().unwrap_or(0).to_string(),
        )
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1");
    for role in ["AUTHOR", "COMMITTER"] {
        command
            .env(format!("GIT_{role}_NAME"), "Fixture")
            .env(format!("GIT_{role}_EMAIL"), "fixture@example.invalid")
            .env(format!("GIT_{role}_DATE"), "2026-01-01T00:00:00+0000");
    }
    command
}

fn git(root: &Path, args: &[&str]) {
    let result = command(root, "git", &Value::Null)
        .args(args)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "Git setup: {}",
        String::from_utf8_lossy(&result.stderr)
    );
}

fn load(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::write(path, bytes).unwrap();
}

fn spaced_json(value: &Value) -> String {
    // Test-input serialization matches the observer's ordinary json.dumps().
    // Expected installed manifests/receipts come only from the frozen fixture.
    let input = serde_json::to_string(value).unwrap();
    let mut result = String::new();
    let (mut inside, mut escaped) = (false, false);
    for character in input.chars() {
        result.push(character);
        if character == '"' && !escaped {
            inside = !inside;
        }
        if !inside && matches!(character, ',' | ':') {
            result.push(' ');
        }
        escaped = character == '\\' && !escaped;
    }
    result + "\n"
}

fn mutate(root: &Path, name: &str) {
    let source = root.join("repo").join(SOURCE);
    let target = root.join("home/.config/omarchy/plugins").join(ID);
    let manifest = root.join("repo/manifest.json");
    let receipt = target.join(".mluva-bundle.json");
    match name {
        "" | "missing-parents" => {}
        "edit" | "missing" | "link" => {
            let entry = target.join(
                load(&target.join("manifest.json"))["entryPoints"]["barWidget"]
                    .as_str()
                    .unwrap(),
            );
            if name == "edit" {
                write(&entry, "// customization\n");
            } else {
                fs::remove_file(&entry).unwrap();
                if name == "link" {
                    symlink(source.join("Widget.qml"), entry).unwrap();
                }
            }
        }
        "source-edit" => write(&source.join("Widget.qml"), "// upgraded widget\n"),
        "unicode" => {
            write(&source.join("Žluťoučký 🦀.qml"), "// unicode\n");
            let mut value = load(&manifest);
            value["name"] = json!("Mluva žluťoučký 🦀");
            write(&manifest, serde_json::to_vec(&value).unwrap());
        }
        "extra" => write(&target.join("custom.qml"), "// customization\n"),
        "rename" => fs::rename(&target, target.with_file_name("renamed-mluva")).unwrap(),
        "receipt-link" => {
            fs::rename(&receipt, root.join("external-receipt")).unwrap();
            symlink(root.join("external-receipt"), receipt).unwrap();
        }
        "no-receipt" => fs::remove_file(receipt).unwrap(),
        "bad-receipt" => write(&receipt, "{private-invalid-json"),
        "foreign-receipt" => {
            let mut value = load(&receipt);
            value["repository"] = json!("https://example.invalid");
            write(&receipt, spaced_json(&value));
        }
        "duplicate" | "hidden" | "unrelated" => {
            let next = target.with_file_name(match name {
                "duplicate" => "old-mluva",
                "hidden" => ".ignored",
                _ => "other.plugin",
            });
            copy_tree(&target, &next);
            if name == "unrelated" {
                write(&next.join("manifest.json"), "{\"id\":\"other.plugin\"}\n");
            }
        }
        "plugin-link" | "dangling-plugin" => {
            fs::rename(&target, root.join("external-plugin")).unwrap();
            symlink(
                root.join(if name == "plugin-link" {
                    "external-plugin"
                } else {
                    "nonexistent"
                }),
                target,
            )
            .unwrap();
        }
        "unmanaged" => {
            fs::remove_dir_all(&target).unwrap();
            fs::create_dir(&target).unwrap();
            write(
                &target.join("manifest.json"),
                "{\"id\":\"mluva.dictation\"}\n",
            );
        }
        "source-link" => {
            fs::rename(&source, root.join("external-source")).unwrap();
            symlink(root.join("external-source"), source).unwrap();
        }
        "manifest-link" => {
            fs::rename(&manifest, root.join("external-manifest")).unwrap();
            symlink(root.join("external-manifest"), manifest).unwrap();
        }
        "nested-link" => symlink(root.join("nonexistent"), source.join("fonts/link")).unwrap(),
        "bad-manifest" => write(&manifest, "{private-invalid-json"),
        "missing-entry" => fs::remove_file(source.join("Widget.qml")).unwrap(),
        "wrong-id" | "outside-entry" => {
            let mut value = load(&manifest);
            if name == "wrong-id" {
                value["id"] = json!("other.plugin");
            } else {
                value["entryPoints"]["barWidget"] = json!("outside/Widget.qml");
            }
            write(&manifest, serde_json::to_vec(&value).unwrap());
        }
        _ => panic!("Unknown fixture mutation {name}"),
    }
}

fn executable(root: &Path) -> PathBuf {
    root.join("repo/bin/mluva-install-widget")
}

fn legacy(root: &Path, kind: &str) {
    let upstream = root.join("upstream/widget");
    fs::create_dir_all(upstream.parent().unwrap()).unwrap();
    let result = command(root, executable(root), &Value::Null)
        .arg("--stage")
        .arg(&upstream)
        .output()
        .unwrap();
    assert!(result.status.success());
    let up = upstream.to_str().unwrap();
    git(root, &["init", "--initial-branch=main", up]);
    git(root, &["-C", up, "add", "."]);
    git(root, &["-C", up, "commit", "-m", "Published widget"]);
    let target = root.join("home/.config/omarchy/plugins/legacy-mluva");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    let target = target.to_str().unwrap();
    git(root, &["clone", up, target]);
    let origin = match kind {
        "https" => "https://github.com/1vecera/omarchy-mluva",
        "ssh" => "git@github.com:1vecera/omarchy-mluva.git",
        _ => "https://github.com/1vecera/omarchy-mluva.git",
    };
    git(root, &["-C", target, "remote", "set-url", "origin", origin]);
    match kind {
        "dirty" => write(&Path::new(target).join("Widget.qml"), "// local change\n"),
        "untracked" => write(&Path::new(target).join("custom.qml"), "// local change\n"),
        "local-commit" => git(
            root,
            &["-C", target, "commit", "--allow-empty", "-m", "Local"],
        ),
        "foreign" => git(
            root,
            &[
                "-C",
                target,
                "remote",
                "set-url",
                "origin",
                "https://example.invalid",
            ],
        ),
        "missing-origin" => git(root, &["-C", target, "remote", "remove", "origin"]),
        _ => {}
    }
}

fn error_kind(stderr: &str) -> String {
    if stderr.is_empty() {
        return String::new();
    }
    let body = stderr
        .lines()
        .find_map(|line| line.strip_prefix("Mluva widget setup: "))
        .unwrap_or(stderr.trim());
    if body == "A widget setup command failed."
        || body == "The widget's local Git ownership check failed."
    {
        "command-failed".into()
    } else if body.contains("JSON document is invalid") || body.contains("invalid plugin list") {
        "json-invalid".into()
    } else if body.contains("os error 17") {
        "exists".into()
    } else if body.contains("outside its bundle") {
        "outside-entry".into()
    } else {
        body.into()
    }
}

fn observe(root: &Path, output: &Output) -> Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("private-invalid-json"),
        "Parser leaked a private document"
    );
    json!({
        "code":output.status.code(), "stdout":normalize(root, &stdout),
        "error":normalize(root, &error_kind(&stderr)),
        "calls":normalize(root, &fs::read_to_string(root.join("peer/calls.tsv")).unwrap_or_default()),
        "home":snapshot(root, &root.join("home")), "output":snapshot(root, &root.join("output"))
    })
}

fn setup(root: &Path, repository: &Path) {
    for path in ["bin", "temp", "peer", "home", "repo/bin"] {
        fs::create_dir_all(root.join(path)).unwrap();
    }
    fs::hard_link(env!("CARGO_BIN_EXE_mluva-install-widget"), executable(root)).unwrap();
    copy_tree(&repository.join(SOURCE), &root.join("repo").join(SOURCE));
    // This comparison uses the immutable release's input version. Current
    // release versions are exercised through actual package/setup tests.
    let mut manifest = load(&repository.join("manifest.json"));
    manifest["version"] = json!("1.6.0");
    write(
        &root.join("repo/manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    );
    for name in ["omarchy", "omarchy-shell"] {
        let path = root.join("bin").join(name);
        write(&path, include_bytes!("support/omarchy-peer.sh"));
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

fn native_ownership_faults(run: &Path, repository: &Path) {
    for fault in ["promotion-conflict", "replaced-target"] {
        let root = run.join(fault);
        setup(&root, repository);
        let installed = command(&root, executable(&root), &Value::Null)
            .output()
            .unwrap();
        assert!(installed.status.success());
        let target = root.join("home/.config/omarchy/plugins/mluva.dictation");
        let original = snapshot(&root, &target);
        if fault == "promotion-conflict" {
            mutate(&root, "rename");
        }
        mutate(&root, "source-edit");
        for file in fs::read_dir(root.join("peer")).unwrap() {
            fs::remove_file(file.unwrap().path()).unwrap();
        }
        let result = command(&root, executable(&root), &json!({"failure":fault}))
            .output()
            .unwrap();
        write(&root.join("fault.stderr"), &result.stderr);
        assert_eq!(result.status.code(), Some(1));
        assert_eq!(
            fs::read_to_string(target.join("foreign.qml")).unwrap(),
            "unrelated widget\n",
            "{fault}: removed a replacement owned by someone else"
        );
        if fault == "promotion-conflict" {
            assert_eq!(
                snapshot(&root, &target.with_file_name("renamed-mluva")),
                original
            );
        } else {
            assert!(String::from_utf8_lossy(&result.stderr).contains("backup was preserved"));
            let backup = fs::read_dir(root.join("home/.config/omarchy/plugin-backups"))
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path()
                .join(ID);
            assert_eq!(snapshot(&root, &backup), original);
        }
        assert_eq!(fs::read_dir(root.join("temp")).unwrap().count(), 0);
        eprintln!("native widget {fault}: unrelated target and previous installation preserved");
    }
}

#[test]
#[ignore = "requires a guarded disposable Linux session and the installed Omarchy schema validator"]
fn released_widget_installation_transactions() {
    let session =
        PathBuf::from(env::var_os("OFFSCREEN_SESSION_ROOT").expect("private session required"));
    assert_ne!(
        fs::read_link("/proc/self/ns/net")
            .unwrap()
            .to_str()
            .unwrap(),
        env::var("MLUVA_HOST_NET_NS").unwrap()
    );
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-widget.json")).unwrap();
    let validator = fs::read("/usr/share/omarchy/bin/omarchy-plugin-validate").unwrap();
    assert_eq!(sha256(validator), fixture["validator_sha256"]);
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let run = session.join("native-widget");
    fs::create_dir(&run).unwrap();
    let mut states = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let spec = &case["input"];
        let name = spec["name"].as_str().unwrap();
        let root = run.join(name);
        setup(&root, repository);
        if let Some(kind) = spec["initial"].as_str() {
            legacy(&root, kind);
        }
        let legacy_target = root.join("home/.config/omarchy/plugins/legacy-mluva");
        let mut git_before = snapshot(&root, &legacy_target.join(".git"));
        git_before.as_object_mut().unwrap().remove("index"); // Git may refresh its stat cache.
        for (index, action) in spec["steps"].as_array().unwrap().iter().enumerate() {
            for file in fs::read_dir(root.join("peer")).unwrap() {
                fs::remove_file(file.unwrap().path()).unwrap();
            }
            let mutation = action["mutation"].as_str().unwrap_or("");
            mutate(&root, mutation);
            let mut cli = command(&root, executable(&root), action);
            match action["mode"].as_str().unwrap() {
                "check" => {
                    cli.arg("--check");
                }
                "stage" => {
                    cli.arg("--stage")
                        .arg(root.join(if mutation == "missing-parents" {
                            "output/deep/widget"
                        } else {
                            "output"
                        }));
                }
                "install" => {}
                "raw" => {
                    for arg in action["args"].as_array().unwrap() {
                        cli.arg(
                            arg.as_str()
                                .unwrap()
                                .replace("$CASE", root.to_str().unwrap()),
                        );
                    }
                }
                _ => unreachable!(),
            }
            let output = cli.output().unwrap();
            write(&root.join(format!("{index}.stdout")), &output.stdout);
            write(&root.join(format!("{index}.stderr")), &output.stderr);
            let actual = observe(&root, &output);
            write(
                &root.join(format!("{index}.json")),
                serde_json::to_vec_pretty(&actual).unwrap(),
            );
            assert_eq!(actual, case["results"][index], "{name} step {index}");
            assert_eq!(
                fs::read_dir(root.join("temp")).unwrap().count(),
                0,
                "{name} leaked temporary staging"
            );
            states += 1;
        }
        if spec["initial"].is_string() {
            let retained = if legacy_target.exists() {
                legacy_target
            } else {
                fs::read_dir(root.join("home/.config/omarchy/plugin-backups"))
                    .unwrap()
                    .next()
                    .unwrap()
                    .unwrap()
                    .path()
                    .join("legacy-mluva")
            };
            let mut git_after = snapshot(&root, &retained.join(".git"));
            git_after.as_object_mut().unwrap().remove("index");
            assert_eq!(
                git_before, git_after,
                "{name}: local Git history was not preserved"
            );
        }
        eprintln!("widget {name}: matched");
    }
    eprintln!(
        "{} released widget transactions / {states} states matched",
        fixture["cases"].as_array().unwrap().len()
    );
    native_ownership_faults(&run, repository);
}
