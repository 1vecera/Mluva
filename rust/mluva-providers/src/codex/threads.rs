//! Live Codex threads, whatever runs them: a TUI in any terminal, tmux or Herdr pane, the
//! shared daemon, or Codex Desktop's own server.
//!
//! A thread is live only while some process holds the exclusive lock on
//! `$CODEX_HOME/thread-writer-locks/<thread>.lock`; lock files outlive their owners, so a file
//! alone means nothing. The holder comes from the kernel's lock table, never from taking the
//! lock or resuming the thread. Each live thread is read once from an app server to leave out
//! Codex's ephemeral helpers and sub-agents; its name comes from Codex's session index, and its
//! state from the server that owns it, when Mluva is connected to that one.
use super::server::{Owner, Servers};
use crate::{agent_platform, claude_sessions, herdr};
use mluva_core::agent_voice::{AgentKind, AgentStatus, CodexThread, Target};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    time::SystemTime,
};

/// Bounds against a crowded or hostile directory; a person keeps far fewer threads open.
const MAX_LOCK_ENTRIES: usize = 4_096;
const MAX_LIVE: usize = 64;
/// Thread reads per pass, so one pass never waits on many.
const MAX_READS: usize = 16;

/// A thread whose writer lock some process holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveThread {
    pub id: String,
    pub lock: PathBuf,
    pub holder: u32,
    /// When the lock file was last written, which orders threads by when they were opened.
    pub since: SystemTime,
}

/// Threads whose lock is held now, oldest first.
pub fn live_threads(home: &Path, holders: &HashMap<(u64, u64), u32>) -> Vec<LiveThread> {
    let directory = home.join("thread-writer-locks");
    if !claude_sessions::private_directory(&directory) {
        return vec![];
    }
    let Ok(listing) = std::fs::read_dir(&directory) else {
        return vec![];
    };
    let uid = agent_platform::effective_uid();
    let mut live: Vec<LiveThread> = listing
        .take(MAX_LOCK_ENTRIES)
        .flatten()
        .filter_map(|item| {
            let name = item.file_name();
            let id = name.to_str()?.strip_suffix(".lock")?;
            if !thread_id(id) {
                return None;
            }
            // Not followed: a symlink or another owner's file is never a thread's lock.
            let metadata = std::fs::symlink_metadata(item.path()).ok()?;
            if !metadata.file_type().is_file() || metadata.uid() != uid {
                return None;
            }
            let holder = agent_platform::flock_holder(holders, &item.path(), &metadata)?;
            Some(LiveThread {
                id: id.to_owned(),
                lock: item.path(),
                holder,
                since: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            })
        })
        .collect();
    live.sort_by(|a, b| a.since.cmp(&b.since).then_with(|| a.id.cmp(&b.id)));
    live.truncate(MAX_LIVE);
    live
}

/// Codex thread IDs are UUIDs; anything else in the directory, such as its coordination lock,
/// is not a thread.
fn thread_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(at, b)| match at {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_hexdigit(),
        })
}

/// What a server says about a thread that never changes.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Meta {
    name: Option<String>,
    cwd: String,
}

/// Live-thread discovery, remembering which threads were read and kept.
#[derive(Default)]
pub struct CodexThreads {
    /// `None` for a thread left out: ephemeral, not started by a person, or a sub-agent.
    known: HashMap<String, Option<Meta>>,
}

impl CodexThreads {
    /// Every live thread a person started, as a target Codex's queue reaches.
    pub async fn discover(&mut self, servers: &Servers) -> Vec<Target> {
        let live = live_threads(servers.home(), &agent_platform::flock_holders());
        self.known
            .retain(|id, _| live.iter().any(|thread| &thread.id == id));
        if live.is_empty() {
            servers.tidy().await;
            return vec![];
        }
        let unread: Vec<String> = live
            .iter()
            .filter(|thread| !self.known.contains_key(&thread.id))
            .take(MAX_READS)
            .map(|thread| thread.id.clone())
            .collect();
        let mut states = HashMap::new();
        // The daemon may own threads and so know their state; Mluva's helper owns none, so it
        // is asked only about threads not read yet.
        let connection = match servers.connect_daemon().await {
            Some(daemon) => Some((daemon, Owner::Daemon)),
            None if !unread.is_empty() => servers.connect().await,
            None => None,
        };
        if let Some((mut server, owner)) = connection {
            let ask: Vec<&String> = match owner {
                Owner::Daemon => live
                    .iter()
                    .map(|thread| &thread.id)
                    .filter(|id| {
                        unread.contains(id) || self.known.get(*id).is_some_and(Option::is_some)
                    })
                    .collect(),
                Owner::Helper => unread.iter().collect(),
            };
            for id in ask {
                let Ok(read) = server
                    .call(
                        "thread/read",
                        json!({"threadId": id, "includeTurns": false}),
                    )
                    .await
                else {
                    continue;
                };
                let thread = read.get("thread").unwrap_or(&Value::Null);
                if unread.contains(id) {
                    self.known.insert(id.clone(), classify(thread));
                }
                if let Some(status) = status(thread) {
                    states.insert(id.clone(), status);
                }
            }
            drop(server);
            servers.tidy().await;
        }
        let names = herdr::codex_thread_names(&servers.home().join("session_index.jsonl"));
        live.into_iter()
            .filter_map(|thread| {
                let meta = self.known.get(&thread.id)?.clone()?;
                let name = names.get(&thread.id).cloned().or(meta.name);
                Some(target(
                    &thread,
                    name,
                    meta.cwd,
                    states.get(&thread.id).copied(),
                ))
            })
            .collect()
    }
}

/// Kept unless ephemeral, started by something other than a person, or a sub-agent.
fn classify(thread: &Value) -> Option<Meta> {
    let text = |key: &str| thread.get(key).and_then(Value::as_str);
    if thread.get("ephemeral").and_then(Value::as_bool) != Some(false)
        || text("threadSource").is_some_and(|source| source != "user")
        || thread
            .get("parentThreadId")
            .is_some_and(|parent| !parent.is_null())
    {
        return None;
    }
    let clean = |value: Option<&str>| {
        value
            .map(|value| herdr::label(value.trim(), 240))
            .filter(|value| !value.is_empty())
    };
    Some(Meta {
        name: clean(text("name")),
        cwd: clean(text("cwd")).unwrap_or_default(),
    })
}

/// A thread's state from the server that has it loaded; `None` when this server does not.
fn status(thread: &Value) -> Option<AgentStatus> {
    let status = thread.get("status")?;
    match status.get("type")?.as_str()? {
        "idle" => Some(AgentStatus::Idle),
        "active" => {
            let waiting = status
                .get("activeFlags")
                .and_then(Value::as_array)
                .is_some_and(|flags| {
                    flags.iter().any(|flag| {
                        matches!(
                            flag.as_str(),
                            Some("waitingOnApproval" | "waitingOnUserInput")
                        )
                    })
                });
            Some(if waiting {
                AgentStatus::Blocked
            } else {
                AgentStatus::Working
            })
        }
        _ => None,
    }
}

fn target(
    thread: &LiveThread,
    name: Option<String>,
    cwd: String,
    status: Option<AgentStatus>,
) -> Target {
    let kind = if agent_platform::bundled_app_server(thread.holder) {
        AgentKind::CodexDesktop
    } else {
        AgentKind::CodexCli
    };
    Target {
        kind,
        name: format!("codex-{}", &thread.id[..8]),
        title: name.unwrap_or_default(),
        project: cwd,
        status: status.unwrap_or(AgentStatus::Unknown),
        session_id: thread.id.clone(),
        pid: thread.holder,
        codex: Some(CodexThread {
            lock: thread.lock.to_string_lossy().into_owned(),
            holder: thread.holder,
        }),
        ..Target::default()
    }
}

/// Join live threads to the targets already found. A Codex pane in Herdr reports the thread's
/// ID as its session; it keeps its place and Herdr's state when the server knows none, and gains
/// the queue route. Threads nobody else shows follow, in the order they were opened.
pub fn merge(mut targets: Vec<Target>, threads: Vec<Target>) -> Vec<Target> {
    for thread in threads {
        let pane = targets.iter_mut().find(|target| {
            target.kind.is_codex()
                && !target.session_id.is_empty()
                && target.session_id == thread.session_id
        });
        match pane {
            Some(pane) => {
                pane.pid = thread.pid;
                pane.kind = thread.kind;
                pane.codex = thread.codex;
                if thread.status != AgentStatus::Unknown {
                    pane.status = thread.status;
                }
                if pane.title.is_empty() {
                    pane.title = thread.title;
                }
                if pane.project.is_empty() {
                    pane.project = thread.project;
                }
            }
            None => targets.push(thread),
        }
    }
    targets
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::{fs::PermissionsExt, io::AsRawFd};

    const A: &str = "01900000-0000-7000-8000-00000000a001";
    const B: &str = "01900000-0000-7000-8000-00000000a002";
    const C: &str = "01900000-0000-7000-8000-00000000a003";

    fn home() -> tempfile::TempDir {
        let temp = tempfile::tempdir().unwrap();
        let locks = temp.path().join("thread-writer-locks");
        std::fs::create_dir(&locks).unwrap();
        std::fs::set_permissions(&locks, std::fs::Permissions::from_mode(0o700)).unwrap();
        temp
    }

    /// Create a thread's lock file and, if `held`, hold it as a Codex process would.
    fn lock(home: &Path, id: &str, held: bool) -> std::fs::File {
        let file =
            std::fs::File::create(home.join("thread-writer-locks").join(format!("{id}.lock")))
                .unwrap();
        if held {
            // SAFETY: a valid open descriptor and a lock operation.
            assert_eq!(unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) }, 0);
        }
        file
    }

    #[test]
    fn only_held_locks_on_thread_ids_are_live() {
        let temp = home();
        let _held = lock(temp.path(), A, true);
        let _left_behind = lock(temp.path(), B, false);
        let _coordination =
            std::fs::File::create(temp.path().join("thread-writer-locks/.coordination.lock"))
                .unwrap();
        // A symlink to a held lock is not the thread's own lock file.
        std::os::unix::fs::symlink(
            temp.path().join(format!("thread-writer-locks/{A}.lock")),
            temp.path().join(format!("thread-writer-locks/{C}.lock")),
        )
        .unwrap();
        let live = live_threads(temp.path(), &agent_platform::flock_holders());
        assert_eq!(live.len(), 1);
        assert_eq!(
            (live[0].id.as_str(), live[0].holder),
            (A, std::process::id())
        );
        // A lock directory others can write to is not read at all.
        let locks = temp.path().join("thread-writer-locks");
        std::fs::set_permissions(&locks, std::fs::Permissions::from_mode(0o777)).unwrap();
        assert!(live_threads(temp.path(), &agent_platform::flock_holders()).is_empty());
        assert!(!thread_id("not-a-thread") && thread_id(A));
    }

    #[test]
    fn helpers_sub_agents_and_threads_not_started_by_a_person_are_left_out() {
        let user = json!({"id": A, "name": "Fix the build", "ephemeral": false, "threadSource": "user",
            "parentThreadId": null, "cwd": "/synthetic/project", "status": {"type": "idle"}});
        assert_eq!(
            classify(&user),
            Some(Meta {
                name: Some("Fix the build".into()),
                cwd: "/synthetic/project".into()
            })
        );
        for left_out in [
            json!({"id": B, "ephemeral": true, "threadSource": "thread_title", "name": null}),
            json!({"id": B, "ephemeral": false, "threadSource": "review"}),
            json!({"id": B, "ephemeral": false, "threadSource": "user", "parentThreadId": A}),
            json!({"id": B}),
        ] {
            assert_eq!(classify(&left_out), None, "{left_out}");
        }
    }

    #[test]
    fn state_comes_only_from_a_server_that_has_the_thread_loaded() {
        for (value, expected) in [
            (json!({"type": "idle"}), Some(AgentStatus::Idle)),
            (
                json!({"type": "active", "activeFlags": []}),
                Some(AgentStatus::Working),
            ),
            (
                json!({"type": "active", "activeFlags": ["waitingOnApproval"]}),
                Some(AgentStatus::Blocked),
            ),
            (
                json!({"type": "active", "activeFlags": ["waitingOnUserInput"]}),
                Some(AgentStatus::Blocked),
            ),
            (json!({"type": "notLoaded"}), None),
            (json!({"type": "systemError"}), None),
        ] {
            assert_eq!(status(&json!({"status": value})), expected);
        }
    }

    #[test]
    fn a_herdr_codex_pane_gains_the_queue_route_and_other_threads_follow() {
        let pane = Target {
            herdr: Some(mluva_core::agent_voice::HerdrPane {
                socket: "/run/user/fixture/herdr.sock".into(),
                server_epoch: "epoch".into(),
                pane_id: "w1:p3".into(),
                guard: String::new(),
            }),
            kind: AgentKind::CodexCli,
            name: "builder".into(),
            title: "Fix the build".into(),
            status: AgentStatus::Working,
            session_id: A.into(),
            ..Target::default()
        };
        let live = |id: &str, holder: u32, status| {
            let thread = LiveThread {
                id: id.into(),
                lock: format!("/synthetic/codex/thread-writer-locks/{id}.lock").into(),
                holder,
                since: SystemTime::UNIX_EPOCH,
            };
            target(&thread, Some("Other".into()), "/synthetic".into(), status)
        };
        let merged = merge(
            vec![pane.clone()],
            vec![
                live(A, 4_200, None),
                live(B, 4_300, Some(AgentStatus::Idle)),
            ],
        );
        assert_eq!(merged.len(), 2);
        assert_eq!(
            merged[0].route(),
            mluva_core::agent_voice::Route::CodexQueue
        );
        assert_eq!(
            (merged[0].pid, merged[0].status, merged[0].title.as_str()),
            (4_200, AgentStatus::Working, "Fix the build")
        );
        assert!(merged[0].herdr.is_some(), "the pane keeps its place");
        assert_eq!(
            (merged[1].session_id.as_str(), merged[1].name.as_str()),
            (B, "codex-01900000")
        );
        assert_eq!(merged[1].status, AgentStatus::Idle);
        // The same thread later held by another process is a different choice.
        let reopened = merge(vec![pane], vec![live(A, 4_999, None)]);
        assert!(!reopened[0].same_instance(&merged[0]));
    }
}

#[cfg(test)]
mod discovery_tests {
    use super::*;
    use crate::codex::test_peer::{Peer, Scenario, THREAD};
    use std::os::unix::{fs::PermissionsExt, io::AsRawFd};

    #[tokio::test]
    async fn live_threads_a_person_started_are_offered_by_name_with_the_owners_state() {
        let temp = tempfile::tempdir().unwrap();
        let locks = temp.path().join("thread-writer-locks");
        std::fs::create_dir(&locks).unwrap();
        std::fs::set_permissions(&locks, std::fs::Permissions::from_mode(0o700)).unwrap();
        let helper = "01900000-0000-7000-8000-00000000a0e1";
        let held: Vec<std::fs::File> = [THREAD, helper]
            .iter()
            .map(|id| {
                let file = std::fs::File::create(locks.join(format!("{id}.lock"))).unwrap();
                // SAFETY: a valid open descriptor and a lock operation.
                assert_eq!(unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) }, 0);
                file
            })
            .collect();
        std::fs::write(
            temp.path().join("session_index.jsonl"),
            format!(
                "{}\n",
                json!({"id": THREAD, "thread_name": "Přehled zkoušek 🙂"})
            ),
        )
        .unwrap();
        let peer = Peer::start(Scenario::Idle);
        let servers = Servers::fixed(temp.path().into(), peer.socket.clone());
        let mut threads = CodexThreads::default();
        let found = threads.discover(&servers).await;
        assert_eq!(found.len(), 1, "the title helper is left out");
        let target = &found[0];
        assert_eq!(target.session_id, THREAD);
        assert_eq!(target.display_name(), "Přehled zkoušek 🙂");
        assert_eq!(
            (target.status, target.kind),
            (AgentStatus::Idle, AgentKind::CodexCli)
        );
        assert_eq!(target.route(), mluva_core::agent_voice::Route::CodexQueue);
        assert_eq!(target.project, "/synthetic/project");
        // Each thread is read once; a later pass asks only for state.
        let reads = || {
            peer.methods()
                .iter()
                .filter(|m| *m == "thread/read")
                .count()
        };
        assert_eq!(reads(), 2);
        threads.discover(&servers).await;
        assert_eq!(
            reads(),
            3,
            "the daemon is asked only for the kept thread's state"
        );
        // A closed thread leaves the wheel.
        drop(held);
        assert!(threads.discover(&servers).await.is_empty());
        assert_eq!(peer.answers_to_server(), 0);
    }
}
