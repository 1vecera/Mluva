//! Independently frozen Meeting records, literal insights and owner-only archive transactions.

use mluva_core::{
    database::{StoreError, StoreResult},
    meeting::{MeetingRecord, MeetingStore, extract_meeting_insights},
};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
};
#[path = "support/meeting.rs"]
mod support;

#[test]
fn released_meeting_records_insights_and_archive_transactions_match() {
    let reference = support::fixture();
    assert_eq!(
        reference["reference"],
        "5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f"
    );
    for row in reference["models"].as_array().unwrap() {
        let model = MeetingRecord::decode(&row["payload"]).unwrap();
        assert_eq!(model.document(), row["expected"], "{}", row["name"]);
        assert_eq!(
            model.markdown(),
            row["markdown"].as_str().unwrap(),
            "{}",
            row["name"]
        );
    }
    for row in reference["invalid"].as_array().unwrap() {
        assert_eq!(
            MeetingRecord::decode(&row["payload"])
                .unwrap_err()
                .to_string(),
            row["error"].as_str().unwrap(),
            "{}",
            row["name"]
        );
    }
    for row in reference["insights"].as_array().unwrap() {
        assert_eq!(
            serde_json::to_value(extract_meeting_insights(row["text"].as_str().unwrap())).unwrap(),
            row["expected"],
            "{}",
            row["text"]
        );
    }
    for row in reference["loads"].as_array().unwrap() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("meetings.json");
        let raw = row["raw"].as_str().unwrap().as_bytes();
        fs::write(&path, raw).unwrap();
        if let Some(size) = row["sparse_size"].as_u64() {
            fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .unwrap()
                .set_len(size)
                .unwrap();
        }
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        let mut store = MeetingStore::new(path.clone(), None);
        let outcome = if store.clear().is_ok() {
            "accepted"
        } else {
            "blocked"
        };
        let size = fs::metadata(&path).unwrap().len();
        let preserved = row["sparse_size"].as_u64().map_or_else(
            || fs::read(&path).unwrap() == raw,
            |expected| size == expected,
        );
        support::assert_value(
            &json!({"error":store.persistence_error.is_some(),"clear":outcome,
            "meetings":store.meetings().iter().map(MeetingRecord::document).collect::<Vec<_>>(),"size":size,
            "mode":fs::metadata(&path).unwrap().permissions().mode() & 0o777,"preserved":preserved}),
            &row["expected"],
            row["name"].as_str().unwrap(),
        );
    }
    let mut states = 0;
    for row in reference["stores"].as_array().unwrap() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        support::setup(root, &row["initial"]);
        let path = root.join("archive/meetings.json");
        let mut store = MeetingStore::new(path.clone(), None);
        for stage in row["stages"].as_array().unwrap() {
            let action = &stage["action"];
            let operation = action["op"].as_str().unwrap();
            let result: StoreResult<Value> = (|| {
                let identifier = action["id"].as_str().unwrap_or("");
                match operation {
                    "observe" => Ok(Value::Null),
                    "save" => Ok(store
                        .save(MeetingRecord::decode(&action["record"])?)?
                        .document()),
                    "rename" => Ok(store
                        .rename(identifier, action["title"].as_str().map(str::to_owned))?
                        .document()),
                    "delete" => {
                        store.delete(identifier)?;
                        Ok(Value::Null)
                    }
                    "clear" => {
                        store.clear()?;
                        Ok(Value::Null)
                    }
                    "recent" => {
                        let limit = action["limit"].as_u64().ok_or_else(|| {
                            StoreError::Invalid(
                                "Meeting limit must be a non-negative integer.".into(),
                            )
                        })?;
                        Ok(json!(
                            store
                                .recent(limit as usize)
                                .iter()
                                .map(MeetingRecord::document)
                                .collect::<Vec<_>>()
                        ))
                    }
                    "find" => Ok(store.find(identifier)?.document()),
                    "archive" => Ok(json!(
                        store
                            .archive_recording(
                                &root.join(action["source"].as_str().unwrap()),
                                identifier
                            )?
                            .strip_prefix(root)
                            .unwrap()
                            .to_string_lossy()
                    )),
                    "path" => Ok(json!(
                        store
                            .recording_path(&MeetingRecord::decode(&action["record"])?)
                            .map(|path| path
                                .strip_prefix(root)
                                .unwrap()
                                .to_string_lossy()
                                .to_string())
                    )),
                    "export" => Ok(json!(
                        store
                            .export(
                                store.find(identifier)?,
                                &root.join("exports"),
                                action["format"].as_str().unwrap()
                            )?
                            .strip_prefix(root)
                            .unwrap()
                            .to_string_lossy()
                    )),
                    "reopen" => {
                        store = MeetingStore::new(path.clone(), None);
                        Ok(Value::Null)
                    }
                    "write_failure" => {
                        support::write_failure(&path);
                        Ok(Value::Null)
                    }
                    "restore" => {
                        let bytes = fs::read(path.join("preserved"))?;
                        fs::remove_file(path.join("preserved"))?;
                        fs::remove_dir(&path)?;
                        fs::write(&path, bytes)?;
                        Ok(Value::Null)
                    }
                    "link" => {
                        let path = root.join(action["path"].as_str().unwrap());
                        fs::create_dir_all(path.parent().unwrap())?;
                        symlink(root.join(action["target"].as_str().unwrap()), path)?;
                        Ok(Value::Null)
                    }
                    _ => panic!("unexpected archive action {operation}"),
                }
            })();
            let result = result.unwrap_or_else(|error| json!({"error":support::kind(&error)}));
            let label = format!("{} {action}", row["name"]);
            support::assert_value(&result, &stage["result"], &label);
            support::assert_value(&support::observe(&store, root), &stage["observed"], &label);
            states += 1;
        }
    }
    println!(
        "Matched {} records, {} rejected rows, {} literal insight texts and {states} actual archive states",
        reference["models"].as_array().unwrap().len(),
        reference["invalid"].as_array().unwrap().len(),
        reference["insights"].as_array().unwrap().len()
    );
}
