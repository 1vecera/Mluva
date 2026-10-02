//! Shared actual archive/file observer, never installed with the application.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use mluva_core::{database::StoreError, meeting::MeetingStore};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, os::unix::fs::PermissionsExt, path::Path};

pub fn fixture() -> Value {
    serde_json::from_str(include_str!("../fixtures/released-meeting.json")).unwrap()
}
pub fn observe(store: &MeetingStore, root: &Path) -> Value {
    fn walk(
        directory: &Path,
        root: &Path,
        files: &mut BTreeMap<String, Value>,
        modes: &mut BTreeMap<String, u32>,
    ) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .to_string();
            let metadata = fs::symlink_metadata(&path).unwrap();
            if metadata.file_type().is_symlink() {
                files.insert(relative, json!({"link":fs::read_link(&path).unwrap().to_string_lossy().replace(root.to_str().unwrap(), "$ROOT")}));
            } else if metadata.is_file() {
                files.insert(
                    relative.clone(),
                    json!({"bytes":STANDARD.encode(fs::read(&path).unwrap())}),
                );
                modes.insert(relative, metadata.permissions().mode() & 0o777);
            } else if metadata.is_dir() {
                modes.insert(relative, metadata.permissions().mode() & 0o777);
                walk(&path, root, files, modes);
            }
        }
    }
    let mut files = BTreeMap::new();
    let mut modes = BTreeMap::new();
    walk(root, root, &mut files, &mut modes);
    json!({"meetings":store.meetings().iter().map(|meeting|meeting.document()).collect::<Vec<_>>(),
        "persistence_error":store.persistence_error.is_some(),"files":files,"modes":modes})
}
pub fn setup(root: &Path, initial: &Value) {
    if let Some(files) = initial["files"].as_object() {
        for (relative, contents) in files {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents.as_str().unwrap()).unwrap();
        }
    }
    let archive = root.join("archive/meetings.json");
    if let Some(value) = initial.get("records") {
        fs::create_dir_all(archive.parent().unwrap()).unwrap();
        fs::write(&archive, mluva_core::json::spaced(value)).unwrap();
    }
    if let Some(value) = initial["malformed"].as_str() {
        fs::create_dir_all(archive.parent().unwrap()).unwrap();
        fs::write(&archive, value).unwrap();
    }
}
pub fn kind(error: &StoreError) -> &'static str {
    match error {
        StoreError::NotFound => "not-found",
        StoreError::Io(_) => "io",
        StoreError::Invalid(message) if message.starts_with("Meeting changes are disabled") => {
            "blocked"
        }
        _ => "invalid",
    }
}
pub fn write_failure(path: &Path) {
    let bytes = if path.is_file() {
        fs::read(path).unwrap()
    } else {
        b"[]".to_vec()
    };
    if path.exists() {
        fs::remove_file(path).unwrap();
    }
    fs::create_dir_all(path).unwrap();
    fs::write(path.join("preserved"), bytes).unwrap();
}
pub fn assert_value(actual: &Value, expected: &Value, label: &str) {
    if actual != expected {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp");
        fs::write(
            directory.join("meeting-actual.json"),
            serde_json::to_vec_pretty(actual).unwrap(),
        )
        .unwrap();
        fs::write(
            directory.join("meeting-expected.json"),
            serde_json::to_vec_pretty(expected).unwrap(),
        )
        .unwrap();
        panic!("{label}: Meeting observation mismatch; complete synthetic results saved in tmp");
    }
}
