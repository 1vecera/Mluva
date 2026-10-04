//! Rewrite only managed audio-path fields. Preserve the original files for undo,
//! including a closed history database's committed WAL frames.
use super::{Result, Snapshot, Transaction, present, replace, require_regular, text};
use rusqlite::Connection;
use std::{
    fs,
    path::{Path, PathBuf},
};

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

pub(super) fn rebase(tx: &mut Transaction, old: &Path, new: &Path) -> Result<()> {
    let old_prefix = format!("{}/", old.display());
    let new_prefix = format!("{}/", new.display());
    let database = new.join("history.sqlite3");
    require_regular(&database)?;
    if present(&database) {
        let expected = Snapshot::read(&database)?;
        let staged = tx.prepare(&database)?;
        fs::copy(&database, &staged)?;
        for suffix in ["-wal", "-shm"] {
            let path = sidecar(&database, suffix);
            require_regular(&path)?;
            if present(&path) {
                fs::copy(&path, sidecar(&staged, suffix))?;
            }
        }
        let result = (|| -> rusqlite::Result<()> {
            let mut connection = Connection::open(&staged)?;
            let columns = connection
                .prepare("PRAGMA table_info(transcription_history)")?
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            if columns.iter().any(|name| name == "retained_audio_path") {
                let transaction = connection.transaction()?;
                // SQLite substr counts Unicode characters, not UTF-8 bytes.
                let length = old_prefix.chars().count() as i64;
                transaction.execute("UPDATE transcription_history SET retained_audio_path = ?1 || substr(retained_audio_path, ?2) WHERE substr(retained_audio_path, 1, ?3) = ?4",rusqlite::params![new_prefix,length+1,length,old_prefix])?;
                transaction.commit()?;
            }
            connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;
            connection.close().map_err(|(_, error)| error)?;
            Ok(())
        })();
        result.map_err(
            |_| "Mluva could not migrate the history database. Existing state was preserved.",
        )?;
        for suffix in ["-wal", "-shm"] {
            tx.retire(&sidecar(&database, suffix))?;
        }
        tx.publish(&database, &expected)?;
    }
    let draft = new.join("scratchpad-draft.json");
    require_regular(&draft)?;
    if present(&draft) {
        let mut value: serde_json::Value = serde_json::from_str(&text(&draft)?).map_err(
            |_| "Mluva could not migrate the saved draft. Existing state was preserved.",
        )?;
        let object = value
            .as_object_mut()
            .ok_or("The saved draft is not an object; it was left untouched.")?;
        if let Some(audio) = object.get("audio_path").and_then(|value| value.as_str())
            && let Some(suffix) = audio.strip_prefix(&old_prefix)
        {
            object.insert(
                "audio_path".into(),
                serde_json::Value::String(format!("{new_prefix}{suffix}")),
            );
            replace(
                tx,
                &draft,
                (serde_json::to_string_pretty(&value)? + "\n").as_bytes(),
                None,
            )?;
        }
    }
    Ok(())
}
