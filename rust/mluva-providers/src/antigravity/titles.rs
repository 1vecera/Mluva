//! Antigravity conversation titles by conversation ID from `conversation_summaries.db`.
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

/// Default location of Antigravity CLI data directory: `$ANTIGRAVITY_APP_DATA_DIR`, defaulting to
/// `~/.gemini/antigravity-cli`.
pub fn antigravity_data_dir() -> Option<PathBuf> {
    std::env::var_os("ANTIGRAVITY_APP_DATA_DIR")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|home| !home.is_empty())
                .map(|home| PathBuf::from(home).join(".gemini/antigravity-cli"))
        })
}

/// Path to Antigravity's conversation summaries database.
pub fn antigravity_summaries_db_path() -> Option<PathBuf> {
    antigravity_data_dir().map(|dir| dir.join("conversation_summaries.db"))
}

/// Read all conversation titles from `conversation_summaries.db`.
pub fn read_conversation_titles(db_path: &Path) -> HashMap<String, String> {
    let mut titles = HashMap::new();
    let Ok(conn) = rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) else {
        return titles;
    };
    let Ok(mut stmt) =
        conn.prepare("SELECT conversation_id, title FROM conversation_summaries WHERE title != ''")
    else {
        return titles;
    };
    let Ok(rows) = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    }) else {
        return titles;
    };
    for row in rows.flatten() {
        let (id, title) = row;
        let trimmed = title.trim();
        if !trimmed.is_empty() {
            titles.insert(id, trimmed.to_owned());
        }
    }
    titles
}

/// Antigravity conversation titles cache, read on demand.
#[derive(Debug, Default)]
pub struct AntigravityTitles {
    path: Option<PathBuf>,
    titles: HashMap<String, String>,
}

impl AntigravityTitles {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self {
            path,
            titles: HashMap::new(),
        }
    }

    pub fn refresh(&mut self) -> &HashMap<String, String> {
        let path = self.path.clone().or_else(antigravity_summaries_db_path);
        if let Some(path) = path.as_deref().filter(|p| p.exists()) {
            self.titles = read_conversation_titles(path);
        } else {
            self.titles.clear();
        }
        &self.titles
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_titles_from_sqlite_database() {
        let temp = tempfile::tempdir().unwrap();
        let db = temp.path().join("conversation_summaries.db");
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute(
            "CREATE TABLE conversation_summaries (conversation_id TEXT PRIMARY KEY, title TEXT)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO conversation_summaries (conversation_id, title) VALUES (?1, ?2)",
            ["conv-1", "Build Pipeline"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO conversation_summaries (conversation_id, title) VALUES (?1, ?2)",
            ["conv-2", "   "],
        )
        .unwrap();

        let mut titles = AntigravityTitles::new(Some(db));
        let map = titles.refresh();
        assert_eq!(map.get("conv-1").map(String::as_str), Some("Build Pipeline"));
        assert_eq!(map.get("conv-2"), None);
    }
}
