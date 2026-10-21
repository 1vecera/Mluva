//! Claude Code's own running-session registry, read without starting Claude Code.
//!
//! Claude Code registers each interactive process as `<config>/sessions/<pid>.json`. Only
//! those files are opened: the `.key` files beside them are secrets, recognised by name and
//! never opened or inspected. A session's title comes from a bounded, incremental tail of its
//! transcript; only title records are parsed, only the title is kept, and nothing is logged.
//!
//! Claude Code documents `claude agents --json` as its interface; the registry files it
//! reads are not a documented format. Mluva reads them tolerantly so a 2-second poll needs no
//! process start: unknown fields and values are ignored, and a field of an unexpected type is
//! treated as absent.
use crate::herdr::label;
use mluva_core::agent_voice::{AgentStatus, Recipient};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{Read, Seek, SeekFrom},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

/// Bounds against a crowded or hostile directory; a person runs far fewer sessions.
const MAX_SESSIONS: usize = 128;
const MAX_DIRECTORY_ENTRIES: usize = 4_096;
const MAX_ENTRY_BYTES: u64 = 64 * 1_024;
const MAX_TITLE_CHARS: usize = 240;
/// Claude Code appends its title records again as a session goes on, so the latest one sits
/// near the end of even a very large transcript. The first read starts this far from the end.
const FIRST_TAIL_BYTES: u64 = 1_048_576;
/// Later reads take only what was appended, up to this much per poll.
const MAX_APPEND_BYTES: u64 = 1_048_576;
/// Claude Code shortens longer project directory names and appends a hash.
const MAX_SLUG_CHARS: usize = 200;
/// Title and agent-setting records are short; any longer line is conversation content.
const MAX_RECORD_BYTES: usize = 4_096;

/// One live interactive Claude Code session from its own registry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaudeSession {
    pub pid: u32,
    pub session_id: String,
    pub cwd: String,
    /// Milliseconds since the epoch, as Claude Code records it.
    pub started_at: u64,
    pub status: AgentStatus,
    /// The person's own name for the session (`-n`, `/rename`).
    pub user_name: Option<String>,
    /// Claude Code's generated handle, such as `<project>-3f`: a last resort.
    pub derived_name: Option<String>,
    /// The `--agent` type, which Claude Code also puts in the terminal title.
    pub agent_type: Option<String>,
    /// The latest `custom-title` record in the transcript.
    pub custom_title: Option<String>,
    /// The latest `ai-title` record: Claude Code's generated session title.
    pub ai_title: Option<String>,
}

impl ClaudeSession {
    /// The session's own title: the person's name for it, then its custom, then its
    /// generated title.
    pub fn own_title(&self) -> Option<&str> {
        self.user_name
            .as_deref()
            .or(self.custom_title.as_deref())
            .or(self.ai_title.as_deref())
    }
}

/// Incremental title reader for one transcript.
#[derive(Default)]
struct Titles {
    path: Option<PathBuf>,
    inode: u64,
    offset: u64,
    custom: Option<String>,
    ai: Option<String>,
    /// The latest `agent-setting` record: the `--agent` type, kept across a resume.
    agent: Option<String>,
}

/// Reads the registry on each call; remembers only transcript positions and titles.
pub struct ClaudeRegistry {
    config: PathBuf,
    /// This process: it and its descendants are never offered as recipients.
    exclude: u32,
    titles: HashMap<String, Titles>,
    /// The last whole entry per process, kept while that exact process lives, so a registry
    /// file caught mid-rewrite does not make a session flicker out of the wheel.
    entries: HashMap<u32, (String, ClaudeSession)>,
}

impl ClaudeRegistry {
    pub fn new(config: impl Into<PathBuf>, exclude: u32) -> Self {
        Self {
            config: config.into(),
            exclude,
            titles: HashMap::new(),
            entries: HashMap::new(),
        }
    }

    /// `$CLAUDE_CONFIG_DIR`, else `~/.claude`, excluding this process tree. A clauth
    /// profile tree is never read; its sessions register in the shared directory anyway.
    pub fn from_environment() -> Option<Self> {
        let home = std::env::var_os("HOME")
            .filter(|home| !home.is_empty())
            .map(PathBuf::from);
        let profiles = home.as_ref().map(|home| home.join(".clauth/profiles"));
        let config = std::env::var_os("CLAUDE_CONFIG_DIR")
            .filter(|dir| !dir.is_empty())
            .map(PathBuf::from)
            .filter(|dir| {
                dir.is_absolute()
                    && profiles
                        .as_ref()
                        .is_none_or(|profiles| !dir.starts_with(profiles))
            })
            .or_else(|| home.map(|home| home.join(".claude")))?;
        Some(Self::new(config, std::process::id()))
    }

    /// Live interactive sessions in the order they started, with their own titles.
    /// Blocking and bounded: a directory listing, one small file per session, a `/proc`
    /// read per session and only newly appended transcript bytes.
    pub fn sessions(&mut self) -> Vec<ClaudeSession> {
        let mut sessions = self.live();
        sessions.sort_by_key(|s| (s.started_at, s.pid));
        let live: HashSet<&str> = sessions.iter().map(|s| s.session_id.as_str()).collect();
        self.titles.retain(|id, _| live.contains(id.as_str()));
        for session in &mut sessions {
            let titles = self.titles.entry(session.session_id.clone()).or_default();
            titles.refresh(&self.config, &session.cwd, &session.session_id);
            session.custom_title = titles.custom.clone();
            session.ai_title = titles.ai.clone();
            if session.agent_type.is_none() {
                session.agent_type = titles.agent.clone();
            }
        }
        sessions
    }

    fn live(&mut self) -> Vec<ClaudeSession> {
        let directory = self.config.join("sessions");
        if !private_directory(&directory) {
            self.entries.clear();
            return vec![];
        }
        let Ok(listing) = fs::read_dir(&directory) else {
            return vec![];
        };
        let namespace = fs::read_link("/proc/self/ns/pid")
            .ok()
            .map(|link| link.to_string_lossy().into_owned());
        let mut seen = HashMap::new();
        for item in listing.take(MAX_DIRECTORY_ENTRIES).flatten() {
            if seen.len() >= MAX_SESSIONS {
                break;
            }
            // Only `<pid>.json` names; `.key` files and everything else are skipped by name.
            let name = item.file_name();
            let Some(pid) = name
                .to_str()
                .and_then(|name| name.strip_suffix(".json"))
                .filter(|pid| !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit()))
                .and_then(|pid| pid.parse::<u32>().ok())
                .filter(|pid| *pid > 1)
            else {
                continue;
            };
            let Some(started) = process_start(pid) else {
                continue;
            };
            let parsed = read_entry(&item.path(), pid, namespace.as_deref());
            let session = match parsed {
                Some(Ok((start, session))) if start == started => Some(session),
                Some(Ok(_)) => None,
                // Unreadable this instant: keep this exact process's last whole entry.
                Some(Err(())) => self
                    .entries
                    .get(&pid)
                    .filter(|(start, _)| *start == started)
                    .map(|(_, session)| session.clone()),
                None => None,
            };
            if let Some(session) = session.filter(|_| !self.within_own_tree(pid)) {
                seen.insert(pid, (started, session));
            }
        }
        self.entries = seen;
        self.entries.values().map(|(_, s)| s.clone()).collect()
    }

    fn within_own_tree(&self, mut pid: u32) -> bool {
        for _ in 0..64 {
            if pid == self.exclude {
                return true;
            }
            match parent(pid) {
                Some(next) if next > 1 && next != pid => pid = next,
                _ => return false,
            }
        }
        false
    }
}

/// A directory owned by this account, not a symlink and not writable by others.
fn private_directory(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| {
        m.file_type().is_dir() && m.uid() == effective_uid() && m.mode() & 0o022 == 0
    })
}

fn effective_uid() -> u32 {
    // SAFETY: geteuid has no arguments or memory preconditions.
    unsafe { libc::geteuid() }
}

/// Open a regular file owned by this account without following a final symlink and
/// without blocking on a FIFO.
fn open_private(path: &Path) -> Option<(fs::File, fs::Metadata)> {
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .ok()?;
    let metadata = file.metadata().ok()?;
    (metadata.file_type().is_file()
        && metadata.uid() == effective_uid()
        && metadata.mode() & 0o022 == 0)
        .then_some((file, metadata))
}

/// `None` rejects the entry; `Err` means it could not be read whole just now.
fn read_entry(
    path: &Path,
    pid: u32,
    namespace: Option<&str>,
) -> Option<Result<(String, ClaudeSession), ()>> {
    // A symlink, a FIFO or another owner's file is never a registry entry.
    let (file, metadata) = open_private(path)?;
    if metadata.len() > MAX_ENTRY_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    if file.take(MAX_ENTRY_BYTES).read_to_end(&mut bytes).is_err() {
        return Some(Err(()));
    }
    let Ok(Value::Object(entry)) = serde_json::from_slice::<Value>(&bytes) else {
        return Some(Err(()));
    };
    let text = |key: &str| entry.get(key).and_then(Value::as_str);
    let start = match entry.get("procStart")? {
        Value::String(start) => start.clone(),
        Value::Number(start) => start.to_string(),
        _ => return None,
    };
    let domain_matches = match (text("pidDomain"), namespace) {
        (None, _) => true,
        (Some(domain), Some(ours)) => domain.find("pid:[").is_some_and(|at| &domain[at..] == ours),
        (Some(_), None) => false,
    };
    let session_id = text("sessionId").filter(|id| valid_session_id(id))?;
    if entry.get("pid").and_then(Value::as_u64) != Some(u64::from(pid))
        || text("kind") != Some("interactive")
        || entry.get("spare").and_then(Value::as_bool) == Some(true)
        || !domain_matches
    {
        return None;
    }
    let clean = |key: &str| {
        text(key)
            .map(|value| label(value.trim(), MAX_TITLE_CHARS))
            .filter(|value| !value.is_empty())
    };
    let name = clean("name");
    let user_named = text("nameSource") == Some("user");
    Some(Ok((
        start,
        ClaudeSession {
            pid,
            session_id: session_id.into(),
            cwd: text("cwd").map(|cwd| label(cwd, 4_096)).unwrap_or_default(),
            started_at: entry.get("startedAt").and_then(Value::as_u64).unwrap_or(0),
            status: status(text("status"), text("waitingFor")),
            user_name: name.clone().filter(|_| user_named),
            derived_name: name.filter(|_| !user_named),
            agent_type: clean("agent"),
            custom_title: None,
            ai_title: None,
        },
    )))
}

/// Claude Code reports `busy`, `shell` (its turn is over but a background command still
/// runs, which Claude Code itself shows as working), `idle` and `waiting`. `waitingFor`
/// accompanies only `waiting`: a permission prompt, a question or an open dialog.
fn status(status: Option<&str>, waiting_for: Option<&str>) -> AgentStatus {
    if status == Some("waiting") || waiting_for.is_some_and(|w| !w.trim().is_empty()) {
        return AgentStatus::Blocked;
    }
    match status {
        Some("busy" | "shell") => AgentStatus::Working,
        Some("idle") => AgentStatus::Idle,
        _ => AgentStatus::Unknown,
    }
}

/// Session IDs become a file name, so only plain identifier characters are accepted.
fn valid_session_id(id: &str) -> bool {
    (1..=128).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

/// The fields of `/proc/<pid>/stat` after the parenthesised command name, from `state`.
fn stat_fields(pid: u32) -> Option<Vec<String>> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = stat.get(stat.rfind(')')? + 1..)?;
    Some(rest.split_whitespace().map(str::to_owned).collect())
}

/// Field 22, the start time, which `procStart` records to detect PID reuse.
fn process_start(pid: u32) -> Option<String> {
    stat_fields(pid)?.into_iter().nth(19)
}

fn parent(pid: u32) -> Option<u32> {
    stat_fields(pid)?.get(1)?.parse().ok()
}

/// Claude Code names a project's transcript directory after its path, with every UTF-16
/// unit other than an ASCII letter or digit replaced by `-`.
fn slug(cwd: &str) -> String {
    cwd.chars()
        .flat_map(|c| {
            let unit = if c.is_ascii_alphanumeric() { c } else { '-' };
            std::iter::repeat_n(
                unit,
                if c.is_ascii_alphanumeric() {
                    1
                } else {
                    c.len_utf16()
                },
            )
        })
        .collect()
}

fn transcript_path(config: &Path, cwd: &str, session_id: &str) -> Option<PathBuf> {
    let projects = config.join("projects");
    let file = format!("{session_id}.jsonl");
    let slug = slug(cwd);
    if slug.len() <= MAX_SLUG_CHARS {
        let directory = projects.join(&slug);
        return private_directory(&directory).then(|| directory.join(file));
    }
    // A shortened name ends with a hash Mluva does not recompute; match the prefix instead.
    let prefix = format!("{}-", &slug[..MAX_SLUG_CHARS]);
    fs::read_dir(&projects)
        .ok()?
        .take(MAX_DIRECTORY_ENTRIES)
        .flatten()
        .filter(|item| {
            item.file_name()
                .to_str()
                .is_some_and(|n| n.starts_with(&prefix))
        })
        .map(|item| item.path())
        .find(|directory| private_directory(directory) && directory.join(&file).exists())
        .map(|directory| directory.join(file))
}

impl Titles {
    fn refresh(&mut self, config: &Path, cwd: &str, session_id: &str) {
        if self.path.is_none() {
            self.path = transcript_path(config, cwd, session_id);
        }
        let Some((mut file, metadata)) = self.path.as_deref().and_then(open_private) else {
            // Not written yet, or replaced by something Mluva will not read.
            return;
        };
        let length = metadata.len();
        if metadata.ino() != self.inode || length < self.offset {
            *self = Self {
                path: self.path.take(),
                inode: metadata.ino(),
                ..Self::default()
            };
        }
        if length == self.offset {
            return;
        }
        let limit = if self.offset == 0 {
            FIRST_TAIL_BYTES
        } else {
            MAX_APPEND_BYTES
        };
        let start = self.offset.max(length.saturating_sub(limit));
        let mut bytes = Vec::new();
        if file.seek(SeekFrom::Start(start)).is_err()
            || (&mut file)
                .take(length - start)
                .read_to_end(&mut bytes)
                .is_err()
        {
            return;
        }
        // A read that skipped ahead starts inside a line; only whole lines are trusted.
        let whole = if start > self.offset {
            bytes
                .iter()
                .position(|b| *b == b'\n')
                .map_or(bytes.len(), |at| at + 1)
        } else {
            0
        };
        let Some(end) = bytes.iter().rposition(|b| *b == b'\n').map(|at| at + 1) else {
            // A line still being written is read again next time, unless it alone fills
            // the whole read: such a line is content, never a title record.
            if bytes.len() as u64 >= limit {
                self.offset = length;
            }
            return;
        };
        for line in bytes[whole.min(end)..end].split(|b| *b == b'\n') {
            self.title_line(line);
        }
        self.offset = start + end as u64;
    }

    fn title_line(&mut self, line: &[u8]) {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Record {
            r#type: String,
            custom_title: Option<Value>,
            ai_title: Option<Value>,
            agent_setting: Option<Value>,
        }
        // Cheap prefilter: long or ordinary conversation lines are never parsed.
        let marked = |mark: &[u8]| line.windows(mark.len()).any(|w| w == mark);
        if line.len() > MAX_RECORD_BYTES
            || !line.starts_with(b"{")
            || !(marked(b"-title\"") || marked(b"\"agent-setting\""))
        {
            return;
        }
        let Ok(record) = serde_json::from_slice::<Record>(line) else {
            return;
        };
        let title = |value: Option<Value>| {
            value
                .as_ref()
                .and_then(Value::as_str)
                .map(|t| label(t.trim(), MAX_TITLE_CHARS))
                .filter(|t| !t.is_empty())
        };
        match record.r#type.as_str() {
            "custom-title" => self.custom = title(record.custom_title),
            "ai-title" => self.ai = title(record.ai_title),
            "agent-setting" => self.agent = title(record.agent_setting),
            _ => {}
        }
    }
}

fn is_claude(kind: &str) -> bool {
    matches!(kind, "claude" | "claude_code")
}

/// One wheel: Herdr's agents first, in Herdr's pane order, then Claude Code sessions Herdr
/// does not show, in the order they started. A Claude agent in Herdr is matched to its
/// registry session by session ID and named from it; Herdr keeps its transport and status.
pub fn merge(herdr: Vec<Recipient>, sessions: &[ClaudeSession]) -> Vec<Recipient> {
    let by_id: HashMap<&str, &ClaudeSession> = sessions
        .iter()
        .map(|s| (s.session_id.as_str(), s))
        .collect();
    let mut joined = HashSet::new();
    let mut merged: Vec<Recipient> = herdr
        .into_iter()
        .map(|mut recipient| {
            if is_claude(&recipient.kind) {
                let session = by_id.get(recipient.session_id.as_str()).copied();
                if let Some(session) = session {
                    joined.insert(session.session_id.as_str());
                    recipient.pid = session.pid;
                    if recipient.project.is_empty() {
                        recipient.project.clone_from(&session.cwd);
                    }
                }
                name_in_herdr(&mut recipient, session);
            }
            recipient
        })
        .collect();
    let mut native: Vec<&ClaudeSession> = sessions
        .iter()
        .filter(|s| !joined.contains(s.session_id.as_str()))
        .collect();
    native.sort_by_key(|s| (s.started_at, s.pid));
    merged.extend(native.into_iter().map(|session| {
        Recipient {
            name: session
                .derived_name
                .clone()
                .unwrap_or_else(|| format!("claude-{}", session.pid)),
            title: session.own_title().unwrap_or_default().into(),
            kind: "claude".into(),
            project: session.cwd.clone(),
            status: session.status,
            session_id: session.session_id.clone(),
            pid: session.pid,
            ..Recipient::default()
        }
    }));
    merged
}

/// Claude names in Herdr: the session's own title, then Herdr's label, then the terminal
/// title unless it is only the `--agent` type, then Claude Code's generated handle.
/// On entry `title` holds Herdr's terminal title, already without the bare product name.
fn name_in_herdr(recipient: &mut Recipient, session: Option<&ClaudeSession>) {
    let labelled = !recipient.name.trim().is_empty() && recipient.name != recipient.pane_id;
    let agent_type = session.and_then(|s| s.agent_type.as_deref());
    let terminal = Some(recipient.title.trim())
        .filter(|t| !t.is_empty() && agent_type.is_none_or(|a| !t.eq_ignore_ascii_case(a)));
    recipient.title = match session.and_then(ClaudeSession::own_title) {
        Some(own) => own.into(),
        None if labelled => String::new(),
        None => terminal.unwrap_or_default().into(),
    };
    if !labelled
        && recipient.title.is_empty()
        && let Some(derived) = session.and_then(|s| s.derived_name.clone())
    {
        recipient.name = derived;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{os::unix::fs::PermissionsExt, sync::mpsc, time::Duration};

    struct Fixture {
        _temp: tempfile::TempDir,
        config: PathBuf,
        namespace: String,
    }
    impl Fixture {
        fn new() -> Self {
            let temp = tempfile::tempdir().unwrap();
            let config = temp.path().join("claude");
            for directory in [config.join("sessions"), config.join("projects")] {
                fs::create_dir_all(&directory).unwrap();
                fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
            }
            let namespace = fs::read_link("/proc/self/ns/pid")
                .unwrap()
                .to_string_lossy()
                .into_owned();
            Self {
                _temp: temp,
                config,
                namespace,
            }
        }
        /// An entry shaped like Claude Code 2.1's registry file.
        fn entry(&self, pid: u32, session: &str, extra: Value) -> Value {
            let mut entry = json!({"pid": pid, "sessionId": session, "cwd": "/synthetic/notes",
                "startedAt": 1_000 + u64::from(pid % 1_000), "kind": "interactive", "entrypoint": "cli",
                "name": format!("notes-{:02x}", pid % 256), "nameSource": "derived", "status": "idle",
                "version": "2.1.306", "pidDomain": format!("linux:0123456789abcdef0123456789abcdef:{}", self.namespace),
                "procStart": process_start(pid).unwrap_or_else(|| "1".into())});
            entry
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            entry
        }
        fn write(&self, name: &str, value: &Value) {
            let path = self.config.join("sessions").join(name);
            fs::write(&path, value.to_string()).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        }
        fn transcript(&self, session: &str, lines: &[Value]) -> PathBuf {
            let directory = self.config.join("projects").join(slug("/synthetic/notes"));
            fs::create_dir_all(&directory).unwrap();
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
            let path = directory.join(format!("{session}.jsonl"));
            let mut body = String::new();
            for line in lines {
                body.push_str(&format!("{line}\n"));
            }
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .unwrap();
            let mut existing = fs::read_to_string(&path).unwrap();
            existing.push_str(&body);
            fs::write(&path, existing).unwrap();
            path
        }
    }

    /// A process that has exited, so its PID is free.
    fn exited_pid() -> u32 {
        let mut child = std::process::Command::new("true").spawn().unwrap();
        let pid = child.id();
        child.wait().unwrap();
        pid
    }

    #[test]
    fn only_live_interactive_entries_are_admitted_and_key_files_are_never_opened() {
        let fixture = Fixture::new();
        let own = std::process::id();
        let parent = parent(own).unwrap();
        fixture.write(
            &format!("{own}.json"),
            &fixture.entry(own, "live-session", json!({})),
        );
        let dead = exited_pid();
        fixture.write(
            &format!("{dead}.json"),
            &fixture.entry(dead, "dead-session", json!({"procStart": "1"})),
        );
        // PID reuse: the PID is alive, but it is not the process that registered.
        fixture.write(
            &format!("{parent}.json"),
            &fixture.entry(parent, "reused-pid", json!({"procStart": "1"})),
        );
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let other = child.id();
        fixture.write(
            &format!("{other}.json"),
            &fixture.entry(other, "background", json!({"kind": "bg"})),
        );
        // The key file is a FIFO with a waiting writer: any open of it releases the writer.
        let key = fixture
            .config
            .join("sessions")
            .join(format!("{own}.{}.key", "c".repeat(64)));
        mkfifo(&key);
        let (opened, waiting) = mpsc::channel();
        let writer = {
            let key = key.clone();
            std::thread::spawn(move || {
                let _file = fs::OpenOptions::new().write(true).open(&key).unwrap();
                opened.send(()).unwrap();
            })
        };
        let mut registry = ClaudeRegistry::new(&fixture.config, 0);
        let found = registry.sessions();
        assert_eq!(
            found
                .iter()
                .map(|s| s.session_id.as_str())
                .collect::<Vec<_>>(),
            ["live-session"]
        );
        let derived = format!("notes-{:02x}", own % 256);
        assert_eq!(
            (
                found[0].status,
                found[0].derived_name.as_deref(),
                found[0].user_name.as_deref()
            ),
            (AgentStatus::Idle, Some(derived.as_str()), None)
        );
        assert!(
            waiting.recv_timeout(Duration::from_millis(200)).is_err(),
            "a .key file was opened"
        );
        // A pre-started spare and another PID namespace never qualify either.
        let rewrite = |extra: Value| {
            fixture.write(
                &format!("{other}.json"),
                &fixture.entry(other, "second", extra),
            )
        };
        rewrite(json!({"spare": true}));
        assert_eq!(registry.sessions().len(), 1);
        rewrite(json!({"pidDomain": "linux:0123456789abcdef0123456789abcdef:pid:[1]"}));
        assert_eq!(registry.sessions().len(), 1);
        // Unknown fields and values from a newer Claude Code are ignored, not fatal.
        rewrite(
            json!({"status": "compacting", "futureField": {"nested": [1]}, "agent": 7, "startedAt": 5_000}),
        );
        let found = registry.sessions();
        assert_eq!(
            (found.len(), found[1].status, found[1].agent_type.as_deref()),
            (2, AgentStatus::Unknown, None)
        );
        rewrite(
            json!({"status": "waiting", "nameSource": "user", "name": "Second", "startedAt": 5_000}),
        );
        let found = registry.sessions();
        assert_eq!(
            (found.len(), found[1].user_name.as_deref(), found[1].status),
            (2, Some("Second"), AgentStatus::Blocked)
        );
        // A file caught mid-rewrite keeps that exact process; its exit removes it.
        fs::write(
            fixture
                .config
                .join("sessions")
                .join(format!("{other}.json")),
            b"{\"pid\":",
        )
        .unwrap();
        assert_eq!(registry.sessions().len(), 2);
        child.kill().unwrap();
        child.wait().unwrap();
        assert_eq!(registry.sessions().len(), 1);
        drop(fs::File::open(&key).unwrap());
        writer.join().unwrap();
    }

    fn mkfifo(path: &Path) {
        let path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        // SAFETY: a valid NUL-terminated path and a mode; no memory is retained.
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    }

    #[test]
    fn symlinked_or_shared_registries_and_this_process_tree_are_not_read() {
        let fixture = Fixture::new();
        let own = std::process::id();
        let entry = fixture.entry(own, "live-session", json!({}));
        let elsewhere = fixture.config.join("elsewhere.json");
        fs::write(&elsewhere, entry.to_string()).unwrap();
        std::os::unix::fs::symlink(
            &elsewhere,
            fixture.config.join("sessions").join(format!("{own}.json")),
        )
        .unwrap();
        assert!(
            ClaudeRegistry::new(&fixture.config, 0)
                .sessions()
                .is_empty()
        );
        fs::remove_file(fixture.config.join("sessions").join(format!("{own}.json"))).unwrap();
        fixture.write(&format!("{own}.json"), &entry);
        assert_eq!(ClaudeRegistry::new(&fixture.config, 0).sessions().len(), 1);
        // This process, or one it descends from, is Mluva's own tree.
        assert!(
            ClaudeRegistry::new(&fixture.config, own)
                .sessions()
                .is_empty()
        );
        assert!(
            ClaudeRegistry::new(&fixture.config, parent(own).unwrap())
                .sessions()
                .is_empty()
        );
        let sessions = fixture.config.join("sessions");
        fs::set_permissions(&sessions, fs::Permissions::from_mode(0o777)).unwrap();
        assert!(
            ClaudeRegistry::new(&fixture.config, 0)
                .sessions()
                .is_empty()
        );
        fs::set_permissions(&sessions, fs::Permissions::from_mode(0o700)).unwrap();
        let moved = fixture.config.join("real-sessions");
        fs::rename(&sessions, &moved).unwrap();
        std::os::unix::fs::symlink(&moved, &sessions).unwrap();
        assert!(
            ClaudeRegistry::new(&fixture.config, 0)
                .sessions()
                .is_empty()
        );
    }

    #[test]
    fn titles_come_from_the_latest_records_read_incrementally() {
        let fixture = Fixture::new();
        let own = std::process::id();
        fixture.write(
            &format!("{own}.json"),
            &fixture.entry(own, "live-session", json!({"agent": "project-lead"})),
        );
        let mut registry = ClaudeRegistry::new(&fixture.config, 0);
        // No transcript yet: only the derived handle.
        let session = &registry.sessions()[0];
        assert_eq!(
            (session.own_title(), session.agent_type.as_deref()),
            (None, Some("project-lead"))
        );
        let message = |text: &str| json!({"type": "user", "message": {"content": text}, "sessionId": "live-session"});
        let path = fixture.transcript(
            "live-session",
            &[
                // A title-shaped object inside a message, such as tool output, is not a record.
                json!({"type": "user", "message": {"type": "ai-title", "aiTitle": "Injected"}}),
                json!({"type": "ai-title", "aiTitle": "First generated title", "sessionId": "live-session"}),
                json!({"type": "ai-title", "aiTitle": "Fix the release notes", "sessionId": "live-session"}),
            ],
        );
        let session = &registry.sessions()[0];
        assert_eq!(session.ai_title.as_deref(), Some("Fix the release notes"));
        assert_eq!(session.own_title(), Some("Fix the release notes"));
        // A rename outranks the generated title; later generated titles do not undo it.
        fixture.transcript(
            "live-session",
            &[
                json!({"type": "custom-title", "customTitle": "Release notes", "sessionId": "live-session"}),
                json!({"type": "ai-title", "aiTitle": "Later generated title", "sessionId": "live-session"}),
            ],
        );
        let session = &registry.sessions()[0];
        assert_eq!(
            (session.custom_title.as_deref(), session.ai_title.as_deref()),
            (Some("Release notes"), Some("Later generated title"))
        );
        assert_eq!(session.own_title(), Some("Release notes"));
        // A line still being written is not parsed until it is whole.
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        std::io::Write::write_all(&mut file, br#"{"type":"custom-title","customTitle":"Half"#)
            .unwrap();
        assert_eq!(registry.sessions()[0].own_title(), Some("Release notes"));
        std::io::Write::write_all(&mut file, b" written\",\"sessionId\":\"live-session\"}\n")
            .unwrap();
        assert_eq!(registry.sessions()[0].own_title(), Some("Half written"));
        // A large transcript is read from its tail; content far from the end is skipped.
        let filler = message(&"x".repeat(4_000));
        let mut lines = vec![
            json!({"type": "custom-title", "customTitle": "Too far back", "sessionId": "live-session"}),
        ];
        lines.extend(std::iter::repeat_n(
            filler,
            (FIRST_TAIL_BYTES as usize / 4_000) + 64,
        ));
        lines.push(
            json!({"type": "ai-title", "aiTitle": "Near the end", "sessionId": "live-session"}),
        );
        fs::remove_file(&path).unwrap();
        fixture.transcript("live-session", &lines);
        let mut fresh = ClaudeRegistry::new(&fixture.config, 0);
        let session = &fresh.sessions()[0];
        assert_eq!(
            (session.custom_title.as_deref(), session.ai_title.as_deref()),
            (None, Some("Near the end"))
        );
        // A resumed session's registry entry has no agent type; its transcript keeps it.
        fixture.transcript(
            "live-session",
            &[json!({"type": "agent-setting", "agentSetting": "release-lead", "sessionId": "live-session"})],
        );
        fixture.write(
            &format!("{own}.json"),
            &fixture.entry(own, "live-session", json!({})),
        );
        assert_eq!(
            fresh.sessions()[0].agent_type.as_deref(),
            Some("release-lead")
        );
        // Paths name transcript directories as Claude Code's JavaScript does, per UTF-16 unit.
        assert_eq!(slug("/notes/Přehled 🙂"), "-notes-P-ehled---");
        // The person's own registry name outranks every transcript title.
        fixture.write(
            &format!("{own}.json"),
            &fixture.entry(
                own,
                "live-session",
                json!({"name": "My review", "nameSource": "user"}),
            ),
        );
        assert_eq!(fresh.sessions()[0].own_title(), Some("My review"));
    }

    fn claude(pane: &str, name: &str, terminal: &str, session: &str, guard: &str) -> Recipient {
        Recipient {
            socket: "/run/user/fixture/herdr.sock".into(),
            server_epoch: "epoch".into(),
            pane_id: pane.into(),
            guard: guard.into(),
            name: if name.is_empty() {
                pane.into()
            } else {
                name.into()
            },
            title: terminal.into(),
            kind: "claude".into(),
            project: "/synthetic/project".into(),
            status: AgentStatus::Idle,
            session_id: session.into(),
            ..Recipient::default()
        }
    }

    fn native(pid: u32, id: &str, started_at: u64) -> ClaudeSession {
        ClaudeSession {
            pid,
            session_id: id.into(),
            cwd: "/synthetic/native".into(),
            started_at,
            status: AgentStatus::Working,
            user_name: None,
            derived_name: Some(format!("native-{pid}")),
            agent_type: None,
            custom_title: None,
            ai_title: None,
        }
    }

    #[test]
    fn herdr_order_comes_first_enriched_by_session_and_unmatched_sessions_follow() {
        let codex = Recipient {
            kind: "codex".into(),
            title: "Thread name".into(),
            session_id: "shared-id".into(),
            ..claude("w1:p1", "dispatch", "", "", &"a".repeat(64))
        };
        let herdr = vec![
            codex,
            claude("w1:p2", "rel-lead", "project-lead", "lead", &"b".repeat(64)),
            claude("w1:p3", "", "Claude Code session title", "typed", ""),
            claude("w1:p4", "", "project-lead", "agent-only", ""),
            claude("w1:p5", "", "", "unregistered", ""),
        ];
        let lead = ClaudeSession {
            user_name: Some("Lead for the release".into()),
            agent_type: Some("project-lead".into()),
            ..native(41, "lead", 5)
        };
        let typed = ClaudeSession {
            ai_title: Some("Investigate clipboard owner".into()),
            custom_title: Some("Clipboard owner".into()),
            ..native(42, "typed", 4)
        };
        let agent_only = ClaudeSession {
            agent_type: Some("project-lead".into()),
            ..native(43, "agent-only", 3)
        };
        let sessions = [
            native(52, "late-native", 9),
            lead,
            typed,
            agent_only,
            native(51, "early-native", 1),
            // A Codex session ID that happens to match never names a Codex pane.
            ClaudeSession {
                user_name: Some("Unrelated".into()),
                ..native(53, "shared-id", 7)
            },
        ];
        let merged = merge(herdr, &sessions);
        let shown: Vec<_> = merged
            .iter()
            .map(|r| {
                (
                    r.pane_id.as_str(),
                    r.display_name(),
                    r.has_transport(),
                    r.pid,
                )
            })
            .collect();
        assert_eq!(
            shown,
            [
                ("w1:p1", "Thread name", true, 0),
                // The person's own name beats both Herdr's label and the agent-type title.
                ("w1:p2", "Lead for the release", true, 41),
                // A rename beats the generated title.
                ("w1:p3", "Clipboard owner", false, 42),
                // The terminal shows only the agent type: Claude Code's handle instead.
                ("w1:p4", "native-43", false, 43),
                // No registry entry: Herdr's own view stays, by pane.
                ("w1:p5", "w1:p5", false, 0),
                // Sessions Herdr does not show follow in the order they started.
                ("", "native-51", false, 51),
                ("", "Unrelated", false, 53),
                ("", "native-52", false, 52),
            ]
        );
        // Herdr keeps its status; registry sessions report their own.
        assert_eq!(merged[2].status, AgentStatus::Idle);
        assert_eq!(merged[5].status, AgentStatus::Working);
        assert!(merged[5].socket.is_empty() && merged[5].draft_only());
        // Without Herdr, the registry alone is the wheel.
        assert_eq!(merge(vec![], &sessions).len(), sessions.len());
        // Herdr's label outranks a plain terminal title when the session has no own title.
        let labelled = merge(
            vec![claude(
                "w2:p1",
                "reviewer",
                "Some terminal title",
                "unnamed",
                "",
            )],
            &[native(60, "unnamed", 1)],
        );
        assert_eq!(labelled[0].display_name(), "reviewer");
    }
}
