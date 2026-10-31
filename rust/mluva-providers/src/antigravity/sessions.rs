//! Live Antigravity CLI sessions, whatever runs them: a TUI in any terminal, tmux or Herdr pane.
//!
//! An Antigravity session is live while an active `agy` process holds the exclusive lock on
//! `<dataDir>/presence/<conversation_id>.lock`. The holder comes from the kernel's lock table.
//! The session's title, project directory and status are read from `conversation_summaries.db`.
use super::titles::antigravity_data_dir;
use crate::agent_platform;
use mluva_core::agent_voice::{AgentKind, AgentStatus, AntigravitySession, Host, Target};
use std::{
    collections::HashMap,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    time::SystemTime,
};

/// Bounds against an unexpectedly crowded directory.
const MAX_LOCK_ENTRIES: usize = 4_096;
const MAX_LIVE: usize = 64;

/// A live Antigravity conversation whose lock some process holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveAntigravitySession {
    pub conversation_id: String,
    pub lock: PathBuf,
    pub holder: u32,
    pub proc_start: String,
    pub since: SystemTime,
}

/// Conversations whose presence lock is held now, oldest first.
pub fn live_sessions(
    presence_dir: &Path,
    holders: &agent_platform::FlockHolders,
) -> Vec<LiveAntigravitySession> {
    if !presence_dir.is_dir() {
        return vec![];
    }
    let Ok(listing) = std::fs::read_dir(presence_dir) else {
        return vec![];
    };
    let uid = agent_platform::effective_uid();
    let mount = agent_platform::mount_device(presence_dir);
    let mut live: Vec<LiveAntigravitySession> = listing
        .take(MAX_LOCK_ENTRIES)
        .flatten()
        .filter_map(|item| {
            let name = item.file_name();
            let id = name.to_str()?.strip_suffix(".lock")?;
            if !conversation_id(id) {
                return None;
            }
            let metadata = std::fs::symlink_metadata(item.path()).ok()?;
            if !metadata.file_type().is_file() || metadata.uid() != uid {
                return None;
            }
            let holder = agent_platform::flock_holder_on(holders, mount, &item.path(), &metadata)?;
            let proc_start = agent_platform::process_start(holder).unwrap_or_default();
            Some(LiveAntigravitySession {
                conversation_id: id.to_owned(),
                lock: item.path(),
                holder,
                proc_start,
                since: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            })
        })
        .collect();
    live.sort_by(|a, b| a.since.cmp(&b.since).then_with(|| a.conversation_id.cmp(&b.conversation_id)));
    live
}

/// Antigravity conversation IDs are UUIDs (36 characters with hyphens).
fn conversation_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(at, b)| match at {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_hexdigit(),
        })
}

#[derive(Clone, Debug, Default)]
struct SummaryMeta {
    title: String,
    status: AgentStatus,
    project: String,
}

fn read_summary_meta(db_path: &Path, ids: &[String]) -> HashMap<String, SummaryMeta> {
    let mut meta = HashMap::new();
    if ids.is_empty() {
        return meta;
    }
    let Ok(conn) = rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) else {
        return meta;
    };
    for id in ids {
        let mut stmt = match conn.prepare(
            "SELECT title, status, workspace_uris FROM conversation_summaries WHERE conversation_id = ?1",
        ) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if let Ok(mut rows) = stmt.query([id])
            && let Ok(Some(row)) = rows.next()
        {
            let title = row.get::<_, String>(0).unwrap_or_default();
            let status_str = row.get::<_, String>(1).unwrap_or_default();
            let workspace_uris = row.get::<_, String>(2).unwrap_or_default();

            let status = if status_str.contains("RUNNING") {
                AgentStatus::Working
            } else if status_str.contains("IDLE") {
                AgentStatus::Idle
            } else if status_str.contains("BLOCKED") {
                AgentStatus::Blocked
            } else {
                AgentStatus::Idle
            };

            let project = parse_first_workspace_path(&workspace_uris);
            meta.insert(
                id.clone(),
                SummaryMeta {
                    title: title.trim().to_owned(),
                    status,
                    project,
                },
            );
        }
    }
    meta
}

fn parse_first_workspace_path(json_uris: &str) -> String {
    let Ok(parsed) = serde_json::from_str::<Vec<String>>(json_uris) else {
        return String::new();
    };
    let Some(first) = parsed.into_iter().next() else {
        return String::new();
    };
    if let Some(path) = first.strip_prefix("file://") {
        urlencoding_decode(path)
    } else {
        first
    }
}

fn urlencoding_decode(input: &str) -> String {
    let mut bytes = Vec::with_capacity(input.len());
    let mut chars = input.bytes();
    while let Some(b) = chars.next() {
        if b == b'%'
            && let (Some(h1), Some(h2)) = (chars.next(), chars.next())
            && let Ok(val) = u8::from_str_radix(std::str::from_utf8(&[h1, h2]).unwrap_or(""), 16)
        {
            bytes.push(val);
            continue;
        }
        bytes.push(b);
    }
    String::from_utf8(bytes).unwrap_or_else(|_| input.to_owned())
}

/// Standalone Antigravity CLI session discovery.
#[derive(Default)]
pub struct AntigravitySessions {
    custom_presence_dir: Option<PathBuf>,
    custom_db_path: Option<PathBuf>,
}

impl AntigravitySessions {
    pub fn new(presence: Option<PathBuf>, db: Option<PathBuf>) -> Self {
        Self {
            custom_presence_dir: presence,
            custom_db_path: db,
        }
    }

    pub async fn discover(&mut self) -> Vec<Target> {
        let presence_dir = self
            .custom_presence_dir
            .clone()
            .or_else(|| antigravity_data_dir().map(|d| d.join("presence")));
        let db_path = self
            .custom_db_path
            .clone()
            .or_else(|| antigravity_data_dir().map(|d| d.join("conversation_summaries.db")));

        let Some(presence_dir) = presence_dir else {
            return vec![];
        };

        let live = tokio::task::spawn_blocking(move || {
            live_sessions(&presence_dir, &agent_platform::flock_holders())
        })
        .await
        .unwrap_or_default();

        if live.is_empty() {
            return vec![];
        }

        let ids: Vec<String> = live.iter().take(MAX_LIVE).map(|s| s.conversation_id.clone()).collect();
        let meta = if let Some(db_path) = db_path.filter(|p| p.exists()) {
            tokio::task::spawn_blocking(move || read_summary_meta(&db_path, &ids))
                .await
                .unwrap_or_default()
        } else {
            HashMap::new()
        };

        live.into_iter()
            .take(MAX_LIVE)
            .map(|session| {
                let m = meta.get(&session.conversation_id);
                let title = m.map(|m| m.title.clone()).unwrap_or_default();
                let project = m.map(|m| m.project.clone()).unwrap_or_default();
                let status = m.map(|m| m.status).unwrap_or(AgentStatus::Idle);

                Target {
                    kind: AgentKind::AntigravityCli,
                    name: "agy".into(),
                    title,
                    project,
                    status,
                    session_id: session.conversation_id.clone(),
                    pid: session.holder,
                    started: session.proc_start.clone(),
                    antigravity: Some(AntigravitySession {
                        conversation_id: session.conversation_id,
                        lock: session.lock.to_string_lossy().into_owned(),
                        holder: session.holder,
                        proc_start: session.proc_start,
                    }),
                    host: Host::Terminal,
                    ..Target::default()
                }
            })
            .collect()
    }
}

/// Merge Antigravity CLI sessions into discovered targets, enriching Herdr panes with
/// lock/process identity and Antigravity queue capabilities.
pub fn merge(mut targets: Vec<Target>, sessions: Vec<Target>) -> Vec<Target> {
    for session in sessions {
        let pane = targets.iter_mut().find(|target| {
            target.kind.is_antigravity()
                && !target.session_id.is_empty()
                && target.session_id == session.session_id
        });
        match pane {
            Some(pane) => {
                pane.pid = session.pid;
                pane.started = session.started;
                pane.antigravity = session.antigravity;
                if pane.title.is_empty() {
                    pane.title = session.title;
                }
                if pane.project.is_empty() {
                    pane.project = session.project;
                }
                if pane.status == AgentStatus::Unknown && session.status != AgentStatus::Unknown {
                    pane.status = session.status;
                }
            }
            None => targets.push(session),
        }
    }
    targets
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::{fs::PermissionsExt, io::AsRawFd};

    const CONV_A: &str = "0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b";
    const CONV_B: &str = "0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5c";

    fn temp_presence() -> tempfile::TempDir {
        let temp = tempfile::tempdir().unwrap();
        let presence = temp.path().join("presence");
        std::fs::create_dir(&presence).unwrap();
        std::fs::set_permissions(&presence, std::fs::Permissions::from_mode(0o700)).unwrap();
        temp
    }

    fn lock(dir: &Path, id: &str, held: bool) -> std::fs::File {
        let file = std::fs::File::create(dir.join(format!("{id}.lock"))).unwrap();
        if held {
            assert_eq!(unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) }, 0);
        }
        file
    }

    #[test]
    fn only_held_locks_on_conversation_uuids_are_live() {
        let temp = temp_presence();
        let presence = temp.path().join("presence");

        let _held_a = lock(&presence, CONV_A, true);
        let _unheld_b = lock(&presence, CONV_B, false);
        let _not_uuid = lock(&presence, "not-a-uuid", true);

        let holders = agent_platform::flock_holders();
        let live = live_sessions(&presence, &holders);
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].conversation_id, CONV_A);
        assert_eq!(live[0].holder, std::process::id());
    }

    #[test]
    fn merge_enriches_herdr_panes_and_appends_standalone() {
        let pane = Target {
            kind: AgentKind::AntigravityCli,
            name: "agy".into(),
            session_id: CONV_A.into(),
            ..Target::default()
        };
        let standalone = Target {
            kind: AgentKind::AntigravityCli,
            name: "agy".into(),
            title: "Task runner".into(),
            session_id: CONV_B.into(),
            pid: 1234,
            antigravity: Some(AntigravitySession {
                conversation_id: CONV_B.into(),
                lock: "/tmp/b.lock".into(),
                holder: 1234,
                proc_start: "100".into(),
            }),
            ..Target::default()
        };
        let session_a = Target {
            kind: AgentKind::AntigravityCli,
            name: "agy".into(),
            title: "Merged title".into(),
            session_id: CONV_A.into(),
            pid: 5678,
            antigravity: Some(AntigravitySession {
                conversation_id: CONV_A.into(),
                lock: "/tmp/a.lock".into(),
                holder: 5678,
                proc_start: "200".into(),
            }),
            ..Target::default()
        };

        let merged = merge(vec![pane], vec![session_a, standalone]);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].session_id, CONV_A);
        assert_eq!(merged[0].title, "Merged title");
        assert_eq!(merged[0].pid, 5678);
        assert!(merged[0].antigravity.is_some());

        assert_eq!(merged[1].session_id, CONV_B);
        assert_eq!(merged[1].title, "Task runner");
        assert_eq!(merged[1].pid, 1234);
    }

    #[tokio::test]
    async fn live_system_discovery_finds_antigravity_sessions_with_titles() {
        let mut sessions = AntigravitySessions::default();
        let targets = sessions.discover().await;
        for target in &targets {
            assert_eq!(target.kind, AgentKind::AntigravityCli);
            assert_eq!(target.route(), mluva_core::agent_voice::Route::AntigravityQueue);
            assert!(target.antigravity.is_some());
            assert!(!target.session_id.is_empty());
            println!(
                "Live Antigravity target: display_name='{}', title='{}', session_id={}, project='{}', status={:?}",
                target.display_name(),
                target.title,
                target.session_id,
                target.project,
                target.status
            );
        }
    }
}
