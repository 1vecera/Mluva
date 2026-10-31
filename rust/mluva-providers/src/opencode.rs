//! OpenCode session titles from `opencode.db`.
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

/// Default location of OpenCode data directory: `$XDG_DATA_HOME/opencode`, defaulting to
/// `~/.local/share/opencode`.
pub fn opencode_data_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .filter(|dir| !dir.is_empty())
        .map(|dir| PathBuf::from(dir).join("opencode"))
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|home| !home.is_empty())
                .map(|home| PathBuf::from(home).join(".local/share/opencode"))
        })
}

/// Path to OpenCode's database.
pub fn opencode_db_path() -> Option<PathBuf> {
    opencode_data_dir().map(|dir| dir.join("opencode.db"))
}

/// Read all session titles from `opencode.db`.
pub fn read_session_titles(db_path: &Path) -> HashMap<String, String> {
    let mut titles = HashMap::new();
    let Ok(conn) = rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) else {
        return titles;
    };
    let Ok(mut stmt) = conn.prepare("SELECT id, title FROM session WHERE title != ''") else {
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

/// OpenCode session titles cache, read on demand.
#[derive(Debug, Default)]
pub struct OpenCodeTitles {
    path: Option<PathBuf>,
    titles: HashMap<String, String>,
}

impl OpenCodeTitles {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self {
            path,
            titles: HashMap::new(),
        }
    }

    pub fn refresh(&mut self) -> &HashMap<String, String> {
        let path = self.path.clone().or_else(opencode_db_path);
        if let Some(path) = path.as_deref().filter(|p| p.exists()) {
            self.titles = read_session_titles(path);
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
        let db = temp.path().join("opencode.db");
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute(
            "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT NOT NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session (id, title) VALUES (?1, ?2)",
            ["ses_1", "Feature Work"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session (id, title) VALUES (?1, ?2)",
            ["ses_2", "   "],
        )
        .unwrap();

        let mut titles = OpenCodeTitles::new(Some(db));
        let map = titles.refresh();
        assert_eq!(map.get("ses_1").map(String::as_str), Some("Feature Work"));
        assert_eq!(map.get("ses_2"), None);
    }
}
