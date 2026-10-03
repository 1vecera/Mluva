//! Public identity upgrades against separately captured, unmodified v1.6.0.
use super::*;
use rusqlite::{Connection, types::ValueRef};
use std::os::unix::fs::MetadataExt;

fn setup(root: &Path, name: &str) -> Value {
    assert!(
        Command::new("bash")
            .arg(repository().join("rust/mluva-install/tests/support/seed-legacy.sh"))
            .arg(root)
            .arg(name)
            .arg(repository().join("linux"))
            .status()
            .unwrap()
            .success()
    );
    let layout: Value =
        serde_json::from_slice(&fs::read(root.join("layout.json")).unwrap()).unwrap();
    if name == "legacy-wal" {
        assert!(
            Command::new(env!("CARGO_BIN_EXE_install-dependency-fixture-peer"))
                .arg("--leave-wal")
                .arg(data(root, &layout).join("voice-scribe"))
                .status()
                .unwrap()
                .success()
        );
    }
    layout
}

fn command(root: &Path, layout: &Value, name: &str, program: &Path) -> Command {
    let units = fs::metadata(root.join("units")).unwrap();
    let mut command = super::command(root, layout, name, program, true);
    command
        .env("DAS_CONF_DIR", layout["managed"].as_str().unwrap())
        .env(
            "MLUVA_MIGRATION_UNIT_ID",
            format!("{}:{}", units.dev(), units.ino()),
        )
        .env(
            "MLUVA_MIGRATION_BACKUP_PREFIX",
            data(root, layout).join("mluva-migration-backups"),
        );
    command
}

fn backup(root: &Path, layout: &Value) -> Option<PathBuf> {
    let paths: Vec<_> = fs::read_dir(data(root, layout).join("mluva-migration-backups"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.file_name().unwrap() != "upgrade-owned")
        .collect();
    assert!(paths.len() <= 1);
    paths.into_iter().next()
}

fn normalize(value: &str, root: &Path, backup: Option<&Path>) -> String {
    let value = if let Some(backup) = backup {
        value.replace(backup.to_str().unwrap(), "$BACKUP").replace(
            backup.strip_prefix(root).unwrap().to_str().unwrap(),
            "$BACKUP",
        )
    } else {
        value.to_owned()
    };
    normal(&value, root).replace(
        &format!("@{}.service", unsafe { libc::getuid() }),
        "@$UID.service",
    )
}

// Open a disposable copy so observation cannot checkpoint or otherwise alter
// the database and WAL bytes that the installer is responsible for preserving.
fn sql_view(path: &Path, root: &Path) -> Option<Value> {
    let temp = tempfile::tempdir_in(root.join("temp")).unwrap();
    let copy = temp.path().join("database");
    fs::copy(path, &copy).unwrap();
    for suffix in ["-wal", "-shm"] {
        let sidecar = PathBuf::from(format!("{}{suffix}", path.display()));
        if sidecar.is_file() {
            fs::copy(sidecar, format!("{}{suffix}", copy.display())).unwrap();
        }
    }
    fn query(db: &Connection, sql: &str) -> rusqlite::Result<Value> {
        let mut statement = db.prepare(sql)?;
        let count = statement.column_count();
        let columns: Vec<_> = statement
            .column_names()
            .iter()
            .map(|name| name.to_string())
            .collect();
        let rows = statement
            .query_map([], |row| {
                (0..count)
                    .map(|index| {
                        Ok(match row.get_ref(index)? {
                            ValueRef::Null => Value::Null,
                            ValueRef::Integer(value) => json!(value),
                            ValueRef::Real(value) => json!(value),
                            ValueRef::Text(value) => json!(std::str::from_utf8(value).unwrap()),
                            ValueRef::Blob(value) => json!(value),
                        })
                    })
                    .collect::<rusqlite::Result<Vec<Value>>>()
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(json!({"columns":columns,"rows":rows}))
    }
    let observe = || -> rusqlite::Result<Value> {
        let db = Connection::open(copy)?;
        let schema = query(&db, "SELECT name,sql FROM sqlite_master WHERE type IN ('table','index','trigger') ORDER BY name")?["rows"].clone();
        let mut tables = BTreeMap::new();
        for row in query(
            &db,
            "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name",
        )?["rows"]
            .as_array()
            .unwrap()
        {
            let name = row[0].as_str().unwrap();
            tables.insert(
                name.to_owned(),
                query(
                    &db,
                    &format!("SELECT * FROM \"{}\" ORDER BY 1", name.replace('"', "\"\"")),
                )?,
            );
        }
        let integrity = query(&db, "PRAGMA integrity_check")?["rows"].clone();
        Ok(json!({"schema":schema,"tables":tables,"integrity":integrity}))
    };
    observe().ok()
}

fn portable(root: &Path, backup: Option<&Path>) -> Value {
    fn visit(
        root: &Path,
        path: &Path,
        backup: Option<&Path>,
        output: &mut BTreeMap<String, Value>,
    ) {
        let Ok(meta) = path.symlink_metadata() else {
            return;
        };
        let mode = meta.permissions().mode() & 0o7777;
        let name = path.file_name().unwrap().to_str().unwrap();
        let value = if meta.is_dir()
            && name == "app"
            && (path.join("bin/mluva").is_file() || path.join("mluva_linux/app.py").is_file())
        {
            json!({"application":"prepared","mode":mode})
        } else if name == "mluva"
            && path.parent().unwrap().file_name().unwrap() == "bin"
            && (fs::read_link(path).is_ok_and(|link| link.ends_with("mluva/app/bin/mluva"))
                || (meta.is_file()
                    && fs::read_to_string(path)
                        .unwrap()
                        .contains("-m mluva_linux.app")))
        {
            json!({"command":"mluva"})
        } else if meta.is_symlink() {
            json!({"link":normalize(fs::read_link(path).unwrap().to_str().unwrap(), root, backup)})
        } else if meta.is_dir() {
            for entry in fs::read_dir(path).unwrap() {
                visit(root, &entry.unwrap().path(), backup, output);
            }
            json!({"directory":mode})
        } else if (name == "history.sqlite3" || path == root.join("foreign/history"))
            && let Some(sql) = sql_view(path, root)
        {
            json!({"sqlite":sql,"mode":mode})
        } else if ["history.sqlite3-wal", "history.sqlite3-shm"].contains(&name) {
            json!({"sqlite_sidecar":name.rsplit('-').next().unwrap(),"mode":mode})
        } else if backup == path.parent()
            && ["manifest.json", "desktop-settings.json"].contains(&name)
        {
            json!({"json":serde_json::from_slice::<Value>(&fs::read(path).unwrap()).unwrap(),"mode":mode})
        } else {
            let bytes = match String::from_utf8(fs::read(path).unwrap()) {
                Ok(value) => normalize(&value, root, backup).into_bytes(),
                Err(error) => error.into_bytes(),
            };
            json!({"sha256":digest(&bytes),"mode":mode})
        };
        output.insert(
            normalize(
                path.strip_prefix(root).unwrap().to_str().unwrap(),
                root,
                backup,
            ),
            serde_json::from_str(&normalize(&value.to_string(), root, backup)).unwrap(),
        );
    }
    let mut tree = BTreeMap::new();
    for name in [
        "home",
        "Žluťoučký home",
        "custom-data",
        "custom-config",
        "foreign",
        "units",
    ] {
        visit(root, &root.join(name), backup, &mut tree);
    }
    json!(tree)
}

fn restored_tree(
    root: &Path,
    layout: &Value,
    before: &BTreeMap<String, Value>,
) -> BTreeMap<String, Value> {
    let mut after = snapshot(root, layout, false);
    if let Some(backup) = backup(root, layout) {
        verify_backup(root, &backup, before, &after);
        let prefix = backup.strip_prefix(root).unwrap().to_str().unwrap();
        after.retain(|key, _| key != prefix && !key.starts_with(&format!("{prefix}/")));
        // A retained recovery backup intentionally makes its container private.
        let container = backup
            .parent()
            .unwrap()
            .strip_prefix(root)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(after[container], json!({"directory":0o700}));
        after.insert(container.to_owned(), before[container].clone());
    }
    after
}

fn equal_tree(observed: &Value, expected: &Value, context: &str) {
    let observed = observed.as_object().unwrap();
    let expected = expected.as_object().unwrap();
    assert_eq!(
        observed.keys().collect::<Vec<_>>(),
        expected.keys().collect::<Vec<_>>(),
        "{context}: tree paths"
    );
    for (key, value) in expected {
        assert_eq!(&observed[key], value, "{context}: {key}");
    }
}

fn verify_backup(
    root: &Path,
    backup: &Path,
    before: &BTreeMap<String, Value>,
    after: &BTreeMap<String, Value>,
) {
    let manifest: Vec<PathBuf> =
        serde_json::from_slice(&fs::read(backup.join("manifest.json")).unwrap()).unwrap();
    for (index, source) in manifest.iter().enumerate() {
        let source = source.strip_prefix(root).unwrap().to_str().unwrap();
        let target = backup
            .join(index.to_string())
            .strip_prefix(root)
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let subtree = |tree: &BTreeMap<String, Value>, prefix: &str| {
            tree.iter()
                .filter_map(|(key, value)| {
                    if key == prefix {
                        Some((String::new(), value.clone()))
                    } else {
                        key.strip_prefix(&format!("{prefix}/"))
                            .map(|rest| (rest.to_owned(), value.clone()))
                    }
                })
                .collect::<BTreeMap<_, _>>()
        };
        assert_eq!(
            subtree(before, source),
            subtree(after, &target),
            "backup {source}: exact original bytes, modes and links"
        );
    }
}

#[test]
#[ignore = "requires a guarded disposable Linux session"]
fn native_legacy_upgrade_matches_released_outcomes() {
    let run = private("legacy-comparison");
    let fixture: Value =
        serde_json::from_str(include_str!("../fixtures/released-legacy.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let root = run.join(name);
        let layout = setup(&root, name);
        let before = snapshot(&root, &layout, false);
        let bundle = fixture_bundle(&root);
        positive_trap(&root, &layout, name);
        let result = command(&root, &layout, name, &bundle.join("linux/install.sh"))
            .output()
            .unwrap();
        logs(&root, &result);
        let error = String::from_utf8_lossy(&result.stderr);
        assert!(
            !error.contains("synthetic-do-not-print"),
            "{name}: credential/parser data in diagnostic"
        );
        assert_eq!(
            result.status.code(),
            case["result"]["code"].as_i64().map(|value| value as i32),
            "{name}: {error}"
        );
        if result.status.success() {
            assert!(error.is_empty(), "{name}: {error}");
        }
        let backup = backup(&root, &layout);
        let commands = normalize(
            &fs::read_to_string(root.join("peer/commands")).unwrap_or_default(),
            &root,
            backup.as_deref(),
        )
        .replace("sudo mv --no-clobber --no-target-directory ", "sudo mv ");
        let observed = json!({
            "code":result.status.code(),
            "stdout":normalize(std::str::from_utf8(&result.stdout).unwrap(), &root, backup.as_deref()),
            "rolled_back":error.contains("Mluva upgrade rolled back;"),
            "commands":commands,
            "settings":serde_json::from_slice::<Value>(&fs::read(root.join("peer/settings.json")).unwrap()).unwrap(),
            "service":serde_json::from_slice::<Value>(&fs::read(root.join("peer/service.json")).unwrap()).unwrap(),
            "tree":portable(&root, backup.as_deref()),
        });
        fs::write(
            root.join("observation.json"),
            serde_json::to_vec_pretty(&observed).unwrap(),
        )
        .unwrap();
        // Small per-key failures keep the evidence readable even for a full
        // backup/database tree; the complete observation remains beside logs.
        for field in [
            "code",
            "stdout",
            "rolled_back",
            "commands",
            "settings",
            "service",
        ] {
            assert_eq!(observed[field], case["result"][field], "{name}: {field}");
        }
        equal_tree(&observed["tree"], &case["result"]["tree"], name);
        let after = restored_tree(&root, &layout, &before);
        if !result.status.success() {
            equal_tree(&json!(after), &json!(before), name);
        }
        eprintln!("Matched legacy migration {name}");
    }
}

#[test]
#[ignore = "requires a guarded disposable Linux session"]
fn native_legacy_service_interruption_and_concurrent_owner() {
    let run = private("legacy-interruption");
    for name in [
        "service-restore-failure",
        "service-signal",
        "service-move-signal",
        "service-move-foreign",
        "service-restore-foreign",
    ] {
        let root = run.join(name);
        let layout = setup(&root, name);
        let before = snapshot(&root, &layout, false);
        let settings = fs::read(root.join("peer/settings.json")).unwrap();
        let service = fs::read(root.join("peer/service.json")).unwrap();
        let bundle = fixture_bundle(&root);
        positive_trap(&root, &layout, name);
        let mut invocation = command(&root, &layout, name, &bundle.join("linux/install.sh"));
        let result = if name.ends_with("signal") {
            let mut child = invocation
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(25);
            while !root.join("peer/waiting").exists() {
                assert!(
                    child.try_wait().unwrap().is_none(),
                    "migration exited before the fault"
                );
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(10));
            }
            let pids: Vec<i32> = fs::read_dir("/proc")
                .unwrap()
                .filter_map(|entry| {
                    let entry = entry.ok()?;
                    let pid = entry.file_name().to_str()?.parse().ok()?;
                    let args = fs::read(entry.path().join("cmdline")).ok()?;
                    (args.split(|byte| *byte == 0).next()
                        == Some(
                            bundle
                                .join("bin/mluva-install")
                                .as_os_str()
                                .as_encoded_bytes(),
                        ))
                    .then_some(pid)
                })
                .collect();
            assert_eq!(pids.len(), 1);
            assert_eq!(unsafe { libc::kill(pids[0], libc::SIGTERM) }, 0);
            while child.try_wait().unwrap().is_none() {
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(10));
            }
            let result = child.wait_with_output().unwrap();
            assert_eq!(result.status.code(), Some(143));
            let worker = fs::read_to_string(root.join("peer/child")).unwrap();
            let status = PathBuf::from(format!("/proc/{}/stat", worker.trim()));
            assert!(
                !status.exists()
                    || fs::read_to_string(status)
                        .unwrap()
                        .split_whitespace()
                        .nth(2)
                        == Some("Z"),
                "owned descendant survived cancellation"
            );
            result
        } else {
            let result = invocation.output().unwrap();
            let restoring = name.starts_with("service-restore-");
            assert_eq!(result.status.code(), Some(if restoring { 23 } else { 1 }));
            result
        };
        logs(&root, &result);
        let backup = backup(&root, &layout).unwrap();
        assert!(!backup.join("complete").exists());
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(root.join("peer/settings.json")).unwrap())
                .unwrap(),
            serde_json::from_slice::<Value>(&settings).unwrap()
        );
        let expected_service = if name.starts_with("service-restore-") {
            json!({"old_enabled":false,"old_active":false,"new_enabled":true,"new_active":false})
        } else {
            serde_json::from_slice::<Value>(&service).unwrap()
        };
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(root.join("peer/service.json")).unwrap())
                .unwrap(),
            expected_service,
            "{name}: service state"
        );
        let after = restored_tree(&root, &layout, &before);
        let mut expected = before;
        if name.starts_with("service-restore-") {
            let unit = expected
                .remove("units/voice-scribe-input@.service")
                .unwrap();
            expected.insert("units/mluva-input@.service".into(), unit);
        }
        if name.ends_with("foreign") {
            expected.insert(
                "units/mluva-input@.service".into(),
                json!({"sha256":digest(b"concurrent service owner\n"),"mode":0o644}),
            );
        }
        equal_tree(&json!(after), &json!(expected), name);
        if !name.ends_with("signal") {
            assert!(String::from_utf8_lossy(&result.stderr).contains(
                if name.starts_with("service-restore-") {
                    "could not be fully restored"
                } else {
                    "another owner"
                }
            ));
        }
        eprintln!("Verified legacy {name}");
    }
}

#[test]
#[ignore = "requires an assembled actual native bundle and a guarded disposable Linux session"]
fn actual_native_bundle_migrates_legacy_state_and_starts() {
    let run = private("legacy-actual");
    let root = run.join("legacy-full");
    let layout = setup(&root, "legacy-full");
    let before = snapshot(&root, &layout, false);
    let source =
        PathBuf::from(env::var_os("MLUVA_TEST_NATIVE_BUNDLE").expect("actual bundle required"));
    positive_trap(&root, &layout, "legacy-full");
    let result = command(
        &root,
        &layout,
        "legacy-full",
        &source.join("linux/install.sh"),
    )
    .output()
    .unwrap();
    logs(&root, &result);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stderr.is_empty());
    let backup = backup(&root, &layout).unwrap();
    verify_backup(&root, &backup, &before, &snapshot(&root, &layout, false));
    let reference: Value =
        serde_json::from_str(include_str!("../fixtures/released-legacy.json")).unwrap();
    equal_tree(
        &portable(&root, Some(&backup)),
        &reference["cases"][0]["result"]["tree"],
        "actual bundle migration",
    );
    let app = data(&root, &layout).join("mluva/app");
    assert_eq!(
        fs::read(app.join(".mluva-native.json")).unwrap(),
        fs::read(source.join(".mluva-native.json")).unwrap()
    );
    assert!(!app.join("pyproject.toml").exists());
    assert!(!app.join(".venv").exists());
    let launcher = Path::new(layout["home"].as_str().unwrap()).join(".local/bin/mluva");
    let result = command(&root, &layout, "legacy-full", &launcher)
        .arg("--help")
        .output()
        .unwrap();
    logs(&root, &result);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stderr.is_empty());
    let reference: Value = serde_json::from_str(include_str!(
        "../../../mluva-gtk/tests/fixtures/released-bootstrap.json"
    ))
    .unwrap();
    assert_eq!(
        String::from_utf8(result.stdout).unwrap(),
        reference["cli"][0]["stdout"]
    );
    fs::write(
        run.join("installed-bundle"),
        app.as_os_str().as_encoded_bytes(),
    )
    .unwrap();
    eprintln!("Migrated complete native bundle at {}", app.display());
}
