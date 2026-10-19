//! Text-only account documents and durable revision checkpoints. No audio or provider configuration crosses this boundary.
use crate::{
    conversation::ConversationStore,
    database::{StoreResult, invalid},
    history::HistoryEntry,
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountRewrite {
    pub id: String,
    pub instruction: String,
    pub text: String,
    pub model: String,
    pub created_at: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSegment {
    pub id: String,
    pub raw_text: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountNote {
    pub id: String,
    pub title: String,
    pub raw_text: String,
    pub text: String,
    pub delivered_text: String,
    pub mode: String,
    #[serde(default)]
    pub rewrites: Vec<AccountRewrite>,
    #[serde(default)]
    pub segments: Vec<AccountSegment>,
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
    #[serde(default)]
    pub device_id: String,
}
impl AccountNote {
    pub fn snapshot(store: &ConversationStore, entry: &HistoryEntry) -> StoreResult<Self> {
        Ok(Self {
            id: entry.identifier.clone(),
            title: entry.title.clone().unwrap_or_default(),
            raw_text: entry.raw_text.clone(),
            text: store.source_text(entry, false)?,
            delivered_text: entry.delivered_text.clone(),
            mode: entry.mode.clone(),
            created_at: chrono::DateTime::parse_from_rfc3339(&entry.created_at)
                .map_or(0, |time| time.timestamp_millis()),
            rewrites: store
                .replies(&entry.identifier)?
                .into_iter()
                .map(|r| AccountRewrite {
                    id: r.created_at.clone(),
                    instruction: r.instruction,
                    text: r.text,
                    model: r.model,
                    created_at: r.created_at,
                })
                .collect(),
            segments: store
                .history
                .continuations(&entry.identifier)?
                .into_iter()
                .map(|s| AccountSegment {
                    id: s.identifier,
                    raw_text: s.raw_text,
                })
                .collect(),
            ..Self::default()
        })
    }
    pub fn same_content(&self, other: &Self) -> bool {
        self.id == other.id
            && self.title == other.title
            && self.raw_text == other.raw_text
            && self.text == other.text
            && self.delivered_text == other.delivered_text
            && self.mode == other.mode
            && self.rewrites == other.rewrites
            // Local segment timestamps can differ from an imported note's timestamps.
            // Conversation order lives in working text; original segments are identified by ID.
            && self.segments.iter().map(|s| (&s.id, &s.raw_text)).collect::<std::collections::BTreeMap<_,_>>()
                == other.segments.iter().map(|s| (&s.id, &s.raw_text)).collect::<std::collections::BTreeMap<_,_>>()
    }
    fn validate(&self) -> StoreResult<()> {
        if uuid::Uuid::parse_str(&self.id).is_err()
            || !["dictation", "command", "scratchpad", "meeting"].contains(&self.mode.as_str())
            || self.raw_text.len() > 90000
            || self.text.len() > 90000
            || self.delivered_text.len() > 90000
            || self.rewrites.len() > 100
            || self.segments.len() > 100
            || self.segments.iter().any(|s| {
                s.id == self.id || uuid::Uuid::parse_str(&s.id).is_err() || s.raw_text.len() > 90000
            })
            || self
                .segments
                .iter()
                .map(|s| &s.id)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.segments.len()
            || serde_json::to_vec(self)?.len() > 320000
        {
            return Err(invalid("Invalid or oversized account note."));
        }
        Ok(())
    }
}
#[derive(Clone)]
pub struct AccountLedger {
    pub store: ConversationStore,
    pub owner: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingEdit {
    pub note: AccountNote,
    pub operation_id: String,
    pub base_revision: u64,
    #[serde(default)]
    pub merged_sources: std::collections::BTreeMap<String, u64>,
}
impl AccountLedger {
    pub fn new(store: ConversationStore, owner: String) -> StoreResult<Self> {
        store.history.database.connect()?.execute_batch("CREATE TABLE IF NOT EXISTS account_notes(owner TEXT NOT NULL, id TEXT NOT NULL, baseline TEXT, pending TEXT, imported INTEGER NOT NULL DEFAULT 0, deleted INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(owner,id)); CREATE TABLE IF NOT EXISTS account_preferences(owner TEXT PRIMARY KEY, preferences TEXT NOT NULL);")?;
        Ok(Self { store, owner })
    }
    pub fn preferences(&self) -> StoreResult<serde_json::Value> {
        let value: Option<String> = self
            .store
            .history
            .database
            .connect()?
            .query_row(
                "SELECT preferences FROM account_preferences WHERE owner=?",
                [&self.owner],
                |r| r.get(0),
            )
            .optional()?;
        Ok(value
            .map(|v| serde_json::from_str(&v))
            .transpose()?
            .unwrap_or_default())
    }
    pub fn save_preferences(&self, value: &serde_json::Value) -> StoreResult<()> {
        self.store.history.database.connect()?.execute("INSERT INTO account_preferences VALUES(?,?) ON CONFLICT(owner) DO UPDATE SET preferences=excluded.preferences", params![self.owner, serde_json::to_string(value)?])?;
        Ok(())
    }
    pub fn ids(&self) -> StoreResult<Vec<String>> {
        let db = self.store.history.database.connect()?;
        let mut q = db.prepare("SELECT id FROM account_notes WHERE owner=? AND deleted=0")?;
        Ok(q.query_map([&self.owner], |r| r.get(0))?
            .collect::<Result<_, _>>()?)
    }
    pub fn baseline(&self, id: &str) -> StoreResult<Option<AccountNote>> {
        let value: Option<Option<String>> = self
            .store
            .history
            .database
            .connect()?
            .query_row(
                "SELECT baseline FROM account_notes WHERE owner=? AND id=? AND deleted=0",
                params![self.owner, id],
                |r| r.get(0),
            )
            .optional()?;
        value
            .flatten()
            .map(|s| serde_json::from_str(&s).map_err(Into::into))
            .transpose()
    }
    pub fn track(&self, id: &str) -> StoreResult<()> {
        self.store.history.database.connect()?.execute(
            "INSERT OR IGNORE INTO account_notes(owner,id) VALUES(?,?)",
            params![self.owner, id],
        )?;
        Ok(())
    }
    /// Freeze a retry body until acknowledged; edits made during an upload form the next revision.
    pub fn pending(&self, id: &str) -> StoreResult<Option<PendingEdit>> {
        let db = self.store.history.database.connect()?;
        let pending: Option<String> = db.query_row(
            "SELECT pending FROM account_notes WHERE owner=? AND id=?",
            params![self.owner, id],
            |r| r.get(0),
        )?;
        if let Some(pending) = pending {
            return Ok(Some(serde_json::from_str(&pending)?));
        }
        let entry = match self.store.history.find(id) {
            Ok(e) => e,
            Err(crate::database::StoreError::NotFound) => return Ok(None),
            Err(e) => return Err(e),
        };
        let note = AccountNote::snapshot(&self.store, &entry)?;
        let baseline = self.baseline(id)?;
        if baseline.as_ref().is_some_and(|b| b.same_content(&note)) {
            return Ok(None);
        }
        let edit = PendingEdit {
            merged_sources: note
                .segments
                .iter()
                .filter_map(|s| {
                    self.baseline(&s.id)
                        .ok()
                        .flatten()
                        .map(|base| (s.id.clone(), base.revision))
                })
                .collect(),
            note,
            operation_id: uuid::Uuid::new_v4().to_string(),
            base_revision: baseline.map_or(0, |b| b.revision),
        };
        db.execute(
            "UPDATE account_notes SET pending=? WHERE owner=? AND id=? AND deleted=0",
            params![serde_json::to_string(&edit)?, self.owner, id],
        )?;
        Ok(Some(edit))
    }
    pub fn acknowledge(&self, note: &AccountNote) -> StoreResult<()> {
        self.store.history.database.connect()?.execute("UPDATE account_notes SET baseline=?, pending=NULL WHERE owner=? AND id=? AND deleted=0", params![serde_json::to_string(note)?,self.owner,note.id])?;
        Ok(())
    }
    pub fn discard_rejected_edit(&self, id: &str) -> StoreResult<()> {
        self.store.history.database.connect()?.execute(
            "UPDATE account_notes SET pending=NULL WHERE owner=? AND id=?",
            params![self.owner, id],
        )?;
        Ok(())
    }
    /// Refuse to overwrite local changes; the caller presents both versions for an explicit resolution.
    pub fn receive(&self, note: &AccountNote) -> StoreResult<bool> {
        note.validate()?;
        let db = self.store.history.database.connect()?;
        let deleted: Option<bool> = db
            .query_row(
                "SELECT deleted FROM account_notes WHERE owner=? AND id=?",
                params![self.owner, note.id],
                |r| r.get(0),
            )
            .optional()?;
        if deleted == Some(true) {
            return Ok(false);
        }
        if let Some(base) = self.baseline(&note.id)? {
            if note.revision <= base.revision {
                return Ok(false);
            }
            if let Ok(entry) = self.store.history.find(&note.id) {
                let local = AccountNote::snapshot(&self.store, &entry)?;
                if !base.same_content(&local) {
                    return Err(invalid(
                        "This note also changed here. Review both versions in Account settings.",
                    ));
                }
            } else {
                return Ok(false);
            }
        } else if self.store.history.find(&note.id).is_ok() {
            return Err(invalid(
                "This local note has not finished uploading. Retry sync.",
            ));
        }
        self.apply(note)?;
        Ok(true)
    }
    pub fn apply(&self, note: &AccountNote) -> StoreResult<()> {
        note.validate()?;
        let mut db = self.store.history.database.connect()?;
        let tx = db.transaction()?;
        if tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM recording_continuations WHERE segment_identifier=?)",
            [&note.id],
            |r| r.get::<_, bool>(0),
        )? {
            return Err(invalid(
                "An original segment cannot become another account conversation.",
            ));
        }
        let old: Option<String> = tx
            .query_row(
                "SELECT raw_text FROM transcription_history WHERE identifier=?",
                [&note.id],
                |r| r.get(0),
            )
            .optional()?;
        if old.as_ref().is_some_and(|r| r != &note.raw_text) {
            return Err(invalid("Original recognition cannot be replaced."));
        }
        let stamp = chrono::DateTime::from_timestamp_millis(note.created_at)
            .unwrap_or_default()
            .to_rfc3339();
        tx.execute("INSERT INTO transcription_history(identifier,created_at,raw_text,delivered_text,mode,language_code,delivery_outcome,title) VALUES(?,?,?,?,?,'','account-sync',?) ON CONFLICT(identifier) DO UPDATE SET delivered_text=excluded.delivered_text,title=excluded.title",params![note.id,stamp,note.raw_text,note.delivered_text,note.mode,note.title])?;
        tx.execute("INSERT INTO conversation_sources VALUES(?,?) ON CONFLICT(history_identifier) DO UPDATE SET text=excluded.text",params![note.id,note.text])?;
        tx.execute(
            "DELETE FROM conversation_rewrites WHERE history_identifier=?",
            [&note.id],
        )?;
        for r in &note.rewrites {
            tx.execute("INSERT INTO conversation_rewrites(history_identifier,instruction,text,model,created_at) VALUES(?,?,?,?,?)",params![note.id,r.instruction,r.text,r.model,r.created_at])?;
        }
        for s in &note.segments {
            let existing: Option<(String, Option<String>)> = tx.query_row("SELECT raw_text,(SELECT history_identifier FROM recording_continuations WHERE segment_identifier=h.identifier) FROM transcription_history h WHERE identifier=?", [&s.id], |r| Ok((r.get(0)?,r.get(1)?))).optional()?;
            if let Some((raw, parent)) = existing {
                if raw != s.raw_text {
                    return Err(invalid("Original recording segments cannot be replaced."));
                }
                if parent.as_deref() != Some(&note.id) {
                    let tracked: bool = tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM account_notes WHERE owner=? AND id=?)",
                        params![self.owner, parent.as_deref().unwrap_or(&s.id)],
                        |r| r.get(0),
                    )?;
                    if !tracked {
                        return Err(invalid(
                            "A segment belongs to a private local conversation.",
                        ));
                    }
                    if let Some(base) = self.baseline(&s.id)? {
                        let entry = self.store.history.find(&s.id)?;
                        if !base.same_content(&AccountNote::snapshot(&self.store, &entry)?) {
                            return Err(invalid(
                                "The merged source also changed locally. Review both versions.",
                            ));
                        }
                    }
                }
            }
            tx.execute("INSERT OR IGNORE INTO transcription_history(identifier,created_at,raw_text,delivered_text,mode,language_code,delivery_outcome) VALUES(?,?,?,?,'dictation','','account-sync')",params![s.id,stamp,s.raw_text,s.raw_text])?;
            tx.execute(
                "INSERT INTO recording_continuations VALUES(?,?) ON CONFLICT(segment_identifier) DO UPDATE SET history_identifier=excluded.history_identifier",
                params![s.id, note.id],
            )?;
        }
        tx.execute("INSERT INTO account_notes(owner,id,baseline,imported) VALUES(?,?,?,?) ON CONFLICT(owner,id) DO UPDATE SET baseline=excluded.baseline,pending=NULL",params![self.owner,note.id,serde_json::to_string(note)?,old.is_none()])?;
        tx.commit()?;
        self.store.export_markdown(&note.id)?;
        for segment in &note.segments {
            self.store.remove_markdown(&segment.id)?;
        }
        Ok(())
    }
    pub fn delete(&self, id: &str) -> StoreResult<()> {
        let tracked: bool = self.store.history.database.connect()?.query_row(
            "SELECT EXISTS(SELECT 1 FROM account_notes WHERE owner=? AND id=?)",
            params![self.owner, id],
            |r| r.get(0),
        )?;
        if tracked
            && self
                .store
                .history
                .recording_conversation(id)
                .is_ok_and(|e| e.identifier == id)
        {
            self.store.history.delete(id)?;
        }
        self.store.history.database.connect()?.execute("INSERT INTO account_notes(owner,id,deleted) VALUES(?,?,1) ON CONFLICT(owner,id) DO UPDATE SET baseline=NULL,pending=NULL,deleted=1",params![self.owner,id])?;
        Ok(())
    }
    /// Remove downloaded cache on sign-out; original local recordings and their retained audio survive.
    pub fn sign_out(&self) -> StoreResult<()> {
        let db = self.store.history.database.connect()?;
        // Original local segments survive even if another device merged them into a downloaded note.
        db.execute("DELETE FROM recording_continuations WHERE segment_identifier IN (SELECT id FROM account_notes WHERE owner=? AND imported=0)", [&self.owner])?;
        let mut q = db.prepare("SELECT id FROM account_notes WHERE owner=? AND imported=1")?;
        let ids: Vec<String> = q
            .query_map([&self.owner], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        for id in ids {
            if self.store.history.find(&id).is_ok() {
                self.store.history.delete(&id)?;
            }
        }
        db.execute(
            "DELETE FROM account_notes WHERE owner=? AND imported=1",
            [&self.owner],
        )?;
        db.execute(
            "UPDATE account_notes SET pending=NULL WHERE owner=?",
            [&self.owner],
        )?;
        Ok(())
    }
}
