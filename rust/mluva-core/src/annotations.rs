//! Screen annotation bundles owned by a recording, then by its conversation.
//!
//! A bundle directory's path never changes once created, because its summary has already been
//! delivered with absolute frame paths. Ownership moves in the database only; deletion, pruning
//! and merging follow the owning conversation exactly as screenshots do.
use std::fs::{self, DirBuilder};
use std::io::Read;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, params};

use crate::database::{Database, StoreResult, invalid, timestamp};
use crate::private_files::resolve_path;

/// The summary a conversation shows is bounded; bundles hold at most twelve short lines.
const MAX_SUMMARY_BYTES: u64 = 64 * 1024;

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

fn canonical_uuid(identifier: &str) -> bool {
    uuid::Uuid::parse_str(identifier).is_ok_and(|uuid| uuid.to_string() == identifier)
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
        if !canonical_uuid(identifier) || self.directory().is_symlink() {
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

    /// Reserve an empty owner-only bundle directory for a recording.
    pub fn create(&self, capture: &str) -> StoreResult<Annotation> {
        if capture.is_empty() {
            return Err(invalid("An annotation needs its recording."));
        }
        let identifier = uuid::Uuid::new_v4().to_string();
        let created_at = timestamp();
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(self.directory())?;
        fs::set_permissions(self.directory(), fs::Permissions::from_mode(0o700))?;
        let directory = self.path_for(&identifier)?;
        // Only clean up a directory this call created.
        DirBuilder::new().mode(0o700).create(&directory)?;
        let inserted = self.database.connect().and_then(|connection| {
            connection.execute(
                "INSERT INTO conversation_annotations (identifier, capture_identifier, created_at, keyframes) VALUES (?, ?, ?, 0)",
                params![identifier, capture, created_at],
            )?;
            Ok(())
        });
        if let Err(error) = inserted {
            let _ = fs::remove_dir_all(&directory);
            return Err(error);
        }
        Ok(Annotation {
            identifier,
            created_at,
            directory,
            keyframes: 0,
        })
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
