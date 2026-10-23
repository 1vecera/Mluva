//! Screen annotation bundles owned by a recording, then by its conversation.
//!
//! A bundle directory's path never changes once created, because its summary has already been
//! delivered with absolute frame paths. Ownership moves in the database only; deletion, pruning
//! and merging follow the owning conversation exactly as screenshots do.
//!
//! New bundles are `annotations/<8 hex>/` holding `01.png`, `02.png`… and `sheet.png`, so each
//! delivered path stays short. Bundles from earlier releases keep their UUID directory with
//! `frames/frame-NN.png` and `contact-sheet.png`, and are read, recovered and removed alike.
use std::fs::{self, DirBuilder};
use std::io::Read;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, params};

use crate::database::{Database, StoreError, StoreResult, invalid, timestamp};
use crate::private_files::resolve_path;

/// The summary a conversation shows is bounded; bundles hold at most twelve short lines.
const MAX_SUMMARY_BYTES: u64 = 64 * 1024;
/// A new bundle's contact sheet, beside its keyframes.
pub const CONTACT_SHEET: &str = "sheet.png";
/// Fresh identifiers tried before creating a bundle fails. Each is 32 random bits, so a
/// second attempt is already rare.
const IDENTIFIER_ATTEMPTS: usize = 8;

/// A new bundle's keyframe file: its two-digit index, directly in the bundle.
pub fn keyframe_file(index: usize) -> String {
    format!("{index:02}.png")
}

/// Whether `relative` names a keyframe in either layout: `NN.png`, or `frames/frame-NN.png`
/// in a bundle from an earlier release.
fn is_keyframe_file(relative: &str) -> bool {
    let numbered = |name: &str, prefix: &str| {
        name.strip_prefix(prefix)
            .and_then(|name| name.strip_suffix(".png"))
            .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
    };
    match relative.split_once('/') {
        None => numbered(relative, ""),
        Some(("frames", name)) => numbered(name, "frame-"),
        Some(_) => false,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Annotation {
    pub identifier: String,
    pub created_at: String,
    pub directory: PathBuf,
    pub keyframes: i64,
}

#[derive(Clone, Debug)]
pub struct AnnotationStore {
    pub database: Database,
}

/// A bundle identifier: eight lowercase hex digits, or the canonical UUID of a bundle from an
/// earlier release. Anything else, such as a separator or `..`, is refused.
fn valid_identifier(identifier: &str) -> bool {
    let short = identifier.len() == 8
        && identifier
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    short || uuid::Uuid::parse_str(identifier).is_ok_and(|uuid| uuid.to_string() == identifier)
}

/// Eight lowercase hex digits: the first 32 bits of a v4 UUID, which are all random.
fn new_identifier() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..8].to_owned()
}

/// Whether an insert failed only because the identifier already has a row.
fn duplicate_identifier(error: &StoreError) -> bool {
    matches!(
        error,
        StoreError::Database(rusqlite::Error::SqliteFailure(failure, _))
            if failure.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY
    )
}
fn owner_column(capture: bool) -> &'static str {
    if capture {
        "capture_identifier"
    } else {
        "history_identifier"
    }
}

impl AnnotationStore {
    pub fn new(database: impl AsRef<Path>) -> Self {
        Self {
            database: Database::new(database),
        }
    }

    pub fn directory(&self) -> PathBuf {
        self.database
            .path
            .parent()
            .unwrap_or(Path::new("."))
            .join("annotations")
    }

    /// The bundle directory, refusing links and anything outside the managed directory.
    pub fn path_for(&self, identifier: &str) -> StoreResult<PathBuf> {
        if !valid_identifier(identifier) || self.directory().is_symlink() {
            return Err(invalid("Invalid annotation identifier."));
        }
        let candidate = self.directory().join(identifier);
        if candidate.is_symlink()
            || resolve_path(&candidate)?.parent()
                != Some(resolve_path(&self.directory())?.as_path())
        {
            return Err(invalid("Annotation is outside its managed directory."));
        }
        Ok(candidate)
    }

    /// Reserve an empty owner-only bundle directory for a recording under a short identifier.
    pub fn create(&self, capture: &str) -> StoreResult<Annotation> {
        self.create_from(capture, std::iter::repeat_with(new_identifier))
    }

    /// Take the first of `identifiers` whose directory and row are both new. An identifier in
    /// use is skipped, never reused: its directory is not entered and its row not replaced.
    fn create_from(
        &self,
        capture: &str,
        identifiers: impl IntoIterator<Item = String>,
    ) -> StoreResult<Annotation> {
        if capture.is_empty() {
            return Err(invalid("An annotation needs its recording."));
        }
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(self.directory())?;
        fs::set_permissions(self.directory(), fs::Permissions::from_mode(0o700))?;
        for identifier in identifiers.into_iter().take(IDENTIFIER_ATTEMPTS) {
            let directory = self.path_for(&identifier)?;
            // Creating the directory is the reservation: it fails when the name exists.
            match DirBuilder::new().mode(0o700).create(&directory) {
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                result => result?,
            }
            let created_at = timestamp();
            let inserted = self.database.connect().and_then(|connection| {
                connection.execute(
                    "INSERT INTO conversation_annotations (identifier, capture_identifier, created_at, keyframes) VALUES (?, ?, ?, 0)",
                    params![identifier, capture, created_at],
                )?;
                Ok(())
            });
            match inserted {
                Ok(()) => {
                    return Ok(Annotation {
                        identifier,
                        created_at,
                        directory,
                        keyframes: 0,
                    });
                }
                // Only the empty directory this call created is removed.
                Err(error) => {
                    let _ = fs::remove_dir(&directory);
                    if !duplicate_identifier(&error) {
                        return Err(error);
                    }
                }
            }
        }
        Err(invalid(
            "Could not reserve a new screen annotation directory.",
        ))
    }

    pub fn set_keyframes(&self, identifier: &str, keyframes: usize) -> StoreResult<()> {
        self.database.connect()?.execute(
            "UPDATE conversation_annotations SET keyframes = ? WHERE identifier = ?",
            params![keyframes as i64, identifier],
        )?;
        Ok(())
    }

    pub fn recent(&self, owner: &str, capture: bool) -> StoreResult<Vec<Annotation>> {
        let connection = self.database.connect()?;
        let mut statement = connection.prepare(&format!(
            "SELECT identifier, created_at, keyframes FROM conversation_annotations WHERE {} = ? ORDER BY created_at, identifier",
            owner_column(capture)
        ))?;
        let rows = statement
            .query_map([owner], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|(identifier, created_at, keyframes)| {
                Ok(Annotation {
                    directory: self.path_for(&identifier)?,
                    identifier,
                    created_at,
                    keyframes,
                })
            })
            .collect()
    }

    /// The bundle's `summary.md`, when it was written.
    pub fn summary(&self, identifier: &str) -> StoreResult<Option<String>> {
        let path = self.path_for(identifier)?.join("summary.md");
        let file = match fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        if !file.metadata()?.is_file() {
            return Err(invalid("The annotation summary is not a regular file."));
        }
        let mut text = String::new();
        file.take(MAX_SUMMARY_BYTES).read_to_string(&mut text)?;
        Ok(Some(text))
    }

    /// Whether a bundle was written to the end: its summary exists, which is written last, and
    /// every keyframe its data lists is a regular file inside the bundle.
    pub fn is_complete(&self, identifier: &str) -> bool {
        let Ok(directory) = self.path_for(identifier) else {
            return false;
        };
        let regular = |path: &Path| {
            fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_file())
        };
        if !regular(&directory.join("summary.md")) {
            return false;
        }
        let Ok(data) = fs::read(directory.join("annotation.json")) else {
            return false;
        };
        let Ok(data) = serde_json::from_slice::<serde_json::Value>(&data) else {
            return false;
        };
        let Some(keyframes) = data["keyframes"].as_array().filter(|k| !k.is_empty()) else {
            return false;
        };
        keyframes.iter().all(|keyframe| {
            keyframe["file"]
                .as_str()
                .is_some_and(|file| is_keyframe_file(file) && regular(&directory.join(file)))
        })
    }

    /// The bundle's keyframe files relative to it, in order, from either layout.
    pub fn keyframe_files(&self, identifier: &str) -> StoreResult<Vec<String>> {
        let directory = self.path_for(identifier)?;
        let mut files = Vec::new();
        for prefix in ["", "frames/"] {
            let listed = match fs::read_dir(directory.join(prefix)) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            for entry in listed {
                let entry = entry?;
                let Ok(name) = entry.file_name().into_string() else {
                    continue;
                };
                let relative = format!("{prefix}{name}");
                if is_keyframe_file(&relative) && entry.file_type()?.is_file() {
                    files.push(relative);
                }
            }
        }
        files.sort();
        Ok(files)
    }

    pub fn bind_capture(&self, capture: &str, conversation: &str) -> StoreResult<()> {
        let mut connection = self.database.connect()?;
        let transaction = connection.transaction()?;
        if transaction
            .query_row(
                "SELECT 1 FROM transcription_history WHERE identifier = ?",
                [conversation],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .is_none()
        {
            return Err(invalid("This conversation no longer exists."));
        }
        transaction.execute(
            "UPDATE conversation_annotations SET history_identifier = ?, capture_identifier = NULL WHERE capture_identifier = ?",
            params![conversation, capture],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn delete(&self, identifier: &str) -> StoreResult<()> {
        let directory = self.path_for(identifier)?;
        match fs::remove_dir_all(&directory) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            result => result?,
        }
        self.database.connect()?.execute(
            "DELETE FROM conversation_annotations WHERE identifier = ?",
            [identifier],
        )?;
        Ok(())
    }

    pub fn delete_owner(&self, owner: &str, capture: bool) -> StoreResult<()> {
        let connection = self.database.connect()?;
        let mut statement = connection.prepare(&format!(
            "SELECT identifier FROM conversation_annotations WHERE {} = ?",
            owner_column(capture)
        ))?;
        let identifiers = statement
            .query_map([owner], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for identifier in identifiers {
            self.delete(&identifier)?;
        }
        Ok(())
    }

    pub fn pending_captures(&self) -> StoreResult<Vec<String>> {
        let connection = self.database.connect()?;
        let mut statement = connection.prepare("SELECT DISTINCT capture_identifier FROM conversation_annotations WHERE capture_identifier IS NOT NULL ORDER BY capture_identifier")?;
        Ok(statement
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    }
}

/// The experimental opt-in, kept beside `config.json` so older releases still read their
/// settings unchanged.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AnnotationSettings {
    pub enabled: bool,
}

impl AnnotationSettings {
    pub const FILE: &'static str = "screen-annotations.json";

    pub fn load(path: &Path) -> Result<Self, &'static str> {
        let read = fs::File::open(path).and_then(|file| {
            let mut bytes = Vec::new();
            file.take(4_097).read_to_end(&mut bytes)?;
            Ok(bytes)
        });
        match read {
            Ok(bytes) if bytes.len() <= 4_096 => serde_json::from_slice(&bytes).map_err(|_| {
                "Screen annotation settings could not be read; the original file was preserved."
            }),
            Ok(_) => Err("Screen annotation settings are too large; the original file was preserved."),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(_) => Err("Screen annotation settings could not be read; the original file was preserved."),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), &'static str> {
        let mut bytes = serde_json::to_vec_pretty(self)
            .map_err(|_| "Screen annotation settings could not be saved.")?;
        bytes.push(b'\n');
        crate::private_files::atomic_write_private(path, &bytes)
            .map_err(|_| "Screen annotation settings could not be saved.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::HistoryStore;

    fn store() -> (tempfile::TempDir, AnnotationStore) {
        let root = tempfile::tempdir().unwrap();
        let history = HistoryStore::new(root.path().join("history.sqlite3"));
        history.initialize().unwrap();
        (root, AnnotationStore::new(&history.database.path))
    }

    fn ids(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn an_identifier_in_use_is_skipped_and_never_reused() {
        let (_root, store) = store();
        let kept = store.create_from("first", ids(&["0000000a"])).unwrap();
        fs::write(kept.directory.join("01.png"), b"first").unwrap();
        // A directory without a row, and a row whose directory is gone.
        let orphan = store.directory().join("0000000b");
        DirBuilder::new().mode(0o700).create(&orphan).unwrap();
        fs::write(orphan.join("01.png"), b"orphan").unwrap();
        let rowless = store.create_from("second", ids(&["0000000c"])).unwrap();
        fs::remove_dir(&rowless.directory).unwrap();

        let fresh = store
            .create_from(
                "third",
                ids(&["0000000a", "0000000b", "0000000c", "0000000d"]),
            )
            .unwrap();
        assert_eq!(fresh.identifier, "0000000d");
        assert_eq!(fresh.directory, store.directory().join("0000000d"));
        assert_eq!(fs::read(kept.directory.join("01.png")).unwrap(), b"first");
        assert_eq!(fs::read(orphan.join("01.png")).unwrap(), b"orphan");
        assert!(!rowless.directory.exists());
        let owners = |capture| {
            store
                .recent(capture, true)
                .unwrap()
                .into_iter()
                .map(|annotation| annotation.identifier)
                .collect::<Vec<_>>()
        };
        assert_eq!(owners("first"), ["0000000a"]);
        assert_eq!(owners("second"), ["0000000c"]);
        assert_eq!(owners("third"), ["0000000d"]);
    }

    #[test]
    fn creation_fails_closed_once_its_attempts_are_used() {
        let (_root, store) = store();
        store.create_from("first", ids(&["0000000a"])).unwrap();
        let taken = std::iter::repeat_with(|| "0000000a".to_owned());
        assert!(store.create_from("second", taken).is_err());
        assert!(store.create_from("second", ids(&[])).is_err());
        assert_eq!(store.pending_captures().unwrap(), ["first"]);
        assert_eq!(fs::read_dir(store.directory()).unwrap().count(), 1);
    }

    #[test]
    fn new_identifiers_are_eight_lowercase_hex_digits() {
        for _ in 0..64 {
            let identifier = new_identifier();
            assert!(valid_identifier(&identifier) && identifier.len() == 8);
        }
    }
}
