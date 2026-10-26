//! Local Herdr metadata and guarded prompt transport. No terminal reads, clipboard or focus.
use crate::{ProviderError, Result};
use mluva_core::agent_voice::{
    AgentKind, AgentStatus, HerdrPane, Route, SendOutcome, Submission, Target,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::{Read, Seek, SeekFrom},
    os::unix::fs::{FileTypeExt, MetadataExt},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};
use tokio_util::sync::CancellationToken;

const MAX_RESPONSE: u64 = 1_048_576;
const UNAVAILABLE: &str = "Herdr is unavailable. Your message stays in Mluva.";
const INCOMPATIBLE: &str =
    "This Herdr version needs the guarded prompt integration. Automatic sending is unavailable.";

#[derive(Clone, Debug)]
pub struct Discovery {
    pub targets: Vec<Target>,
    pub guarded: bool,
}

/// Only local, explicitly selected or conventionally located sockets are candidates.
pub fn socket_candidates(config_home: &Path, explicit: Option<&str>) -> Vec<PathBuf> {
    if let Some(path) = explicit {
        return vec![PathBuf::from(path)];
    }
    let root = config_home.join("herdr");
    let mut paths = vec![root.join("herdr.sock")];
    if let Ok(entries) = std::fs::read_dir(root.join("sessions")) {
        let mut sessions: Vec<_> = entries
            .flatten()
            .take(64)
            .map(|entry| entry.path().join("herdr.sock"))
            .collect();
        sessions.sort();
        paths.extend(sessions);
    }
    paths
        .into_iter()
        .filter(|path| secure_socket(path).is_ok())
        .collect()
}

fn secure_socket(path: &Path) -> Result<()> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|_| ProviderError::message(UNAVAILABLE))?;
    let uid = crate::agent_platform::effective_uid();
    if !path.is_absolute()
        || !metadata.file_type().is_socket()
        || metadata.uid() != uid
        || metadata.mode() & 0o022 != 0
    {
        return Err(ProviderError::message(
            "Choose a local Herdr socket owned by your account and not writable by others.",
        ));
    }
    Ok(())
}

async fn rpc(path: &Path, method: &str, params: Value) -> Result<Value> {
    secure_socket(path)?;
    tokio::time::timeout(Duration::from_secs(2), async {
        let stream = UnixStream::connect(path)
            .await
            .map_err(|_| ProviderError::message(UNAVAILABLE))?;
        // Check the connected peer as well as the pathname, closing the replacement race.
        if stream
            .peer_cred()
            .map_err(|_| ProviderError::message(UNAVAILABLE))?
            .uid()
            != crate::agent_platform::effective_uid()
        {
            return Err(ProviderError::message(
                "The Herdr connection belongs to another account.",
            ));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let mut bytes = serde_json::to_vec(&json!({"id": id, "method": method, "params": params}))
            .map_err(|_| ProviderError::message("The Herdr request could not be prepared."))?;
        bytes.push(b'\n');
        let mut stream = stream;
        stream
            .write_all(&bytes)
            .await
            .map_err(|_| ProviderError::message(UNAVAILABLE))?;
        let mut response = Vec::new();
        BufReader::new(stream.take(MAX_RESPONSE + 1))
            .read_until(b'\n', &mut response)
            .await
            .map_err(|_| ProviderError::message(UNAVAILABLE))?;
        if response.len() > MAX_RESPONSE as usize || response.last() != Some(&b'\n') {
            return Err(ProviderError::message(
                "Herdr returned an incomplete response.",
            ));
        }
        let value: Value = serde_json::from_slice(&response)
            .map_err(|_| ProviderError::message("Herdr returned an invalid response."))?;
        if value.get("id").and_then(Value::as_str) != Some(&id) {
            return Err(ProviderError::message(
                "Herdr returned a response for a different request.",
            ));
        }
        Ok(value)
    })
    .await
    .map_err(|_| ProviderError::message(UNAVAILABLE))?
}

#[derive(Deserialize)]
struct AgentSession {
    agent: String,
    kind: String,
    value: String,
}
#[derive(Deserialize)]
struct Agent {
    pane_id: String,
    name: Option<String>,
    agent: Option<String>,
    #[serde(default)]
    agent_status: AgentStatus,
    cwd: Option<String>,
    foreground_cwd: Option<String>,
    terminal_title_stripped: Option<String>,
    agent_session: Option<AgentSession>,
}
pub(crate) fn label(value: &str, max: usize) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .take(max)
        .collect()
}

/// Titles are shown whole; this bound only rejects runaway metadata.
const MAX_TITLE_CHARS: usize = 240;
/// Codex appends one line per naming event, so the newest names sit at the end.
const MAX_CODEX_INDEX_BYTES: u64 = 4 * 1_048_576;

/// Codex thread names by session ID from Codex's own `session_index.jsonl`; the latest line wins.
pub fn codex_thread_names(index: &Path) -> HashMap<String, String> {
    let mut names = CodexNames::new(index.to_path_buf());
    names.refresh();
    names.names
}

/// Codex thread names, read incrementally: the newest 4 MiB of the index on first sight, then
/// only what Codex appended since, so a large index is not read again on every pass.
#[derive(Debug, Default)]
pub struct CodexNames {
    path: PathBuf,
    inode: u64,
    offset: u64,
    /// The start of a line Codex is still writing.
    partial: Vec<u8>,
    names: HashMap<String, String>,
}

impl CodexNames {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            ..Self::default()
        }
    }

    /// The names after reading what was appended; the latest line for an ID wins.
    pub fn refresh(&mut self) -> &HashMap<String, String> {
        #[derive(Deserialize)]
        struct Entry {
            id: String,
            thread_name: Option<String>,
        }
        let Ok(mut file) = std::fs::File::open(&self.path) else {
            self.names.clear();
            return &self.names;
        };
        let Ok(metadata) = file.metadata() else {
            return &self.names;
        };
        let length = metadata.len();
        let mut skip_partial = false;
        if metadata.ino() != self.inode || length < self.offset {
            // A new or rewritten index: start again from its newest whole lines.
            self.inode = metadata.ino();
            self.offset = length.saturating_sub(MAX_CODEX_INDEX_BYTES);
            self.names.clear();
            self.partial.clear();
            skip_partial = self.offset > 0;
        }
        if length == self.offset || file.seek(SeekFrom::Start(self.offset)).is_err() {
            return &self.names;
        }
        let mut bytes = Vec::new();
        if (&mut file)
            .take((length - self.offset).min(MAX_CODEX_INDEX_BYTES))
            .read_to_end(&mut bytes)
            .is_err()
        {
            return &self.names;
        }
        self.offset += bytes.len() as u64;
        // A read that starts inside a line trusts only the whole lines after it.
        let start = if skip_partial {
            bytes
                .iter()
                .position(|b| *b == b'\n')
                .map_or(bytes.len(), |at| at + 1)
        } else {
            0
        };
        let mut pending = std::mem::take(&mut self.partial);
        pending.extend_from_slice(&bytes[start..]);
        let complete = pending
            .iter()
            .rposition(|b| *b == b'\n')
            .map_or(0, |at| at + 1);
        self.partial = pending.split_off(complete);
        for entry in pending
            .split(|b| *b == b'\n')
            .filter_map(|line| serde_json::from_slice::<Entry>(line).ok())
        {
            match entry.thread_name.filter(|name| !name.trim().is_empty()) {
                Some(name) => self.names.insert(entry.id, name),
                None => self.names.remove(&entry.id),
            };
        }
        &self.names
    }
}

/// `$CODEX_HOME/session_index.jsonl`, defaulting to `~/.codex`.
pub fn codex_index_path() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".codex")))
        .map(|home| home.join("session_index.jsonl"))
}

/// The agent's own name for this session, or empty when it has none yet.
fn session_title(
    agent: &Agent,
    kind: AgentKind,
    codex_threads: &HashMap<String, String>,
) -> String {
    let title = if kind == AgentKind::CodexCli {
        // Codex composes its terminal title from configurable items (thread, project,
        // activity), so the title comes from its thread index for this exact session.
        agent
            .agent_session
            .as_ref()
            .filter(|s| s.agent == "codex" && s.kind == "id")
            .and_then(|s| codex_threads.get(&s.value))
            .map(String::as_str)
    } else {
        // Claude Code titles the terminal with the session's custom or generated title;
        // Herdr strips its activity glyph. The product name means no title yet.
        agent
            .terminal_title_stripped
            .as_deref()
            .filter(|title| !title.trim().eq_ignore_ascii_case("claude code"))
    };
    title.map_or_else(String::new, |title| label(title.trim(), MAX_TITLE_CHARS))
}

fn target(
    path: &Path,
    epoch: &str,
    guard: &str,
    value: Value,
    codex_threads: &HashMap<String, String>,
) -> Option<Target> {
    let agent: Agent = serde_json::from_value(value).ok()?;
    let kind = AgentKind::from_herdr(agent.agent.as_deref()?)?;
    let title = session_title(&agent, kind, codex_threads);
    let session_id = agent
        .agent_session
        .as_ref()
        .filter(|s| s.kind == "id" && s.value.len() <= 128)
        .map(|s| label(&s.value, 128))
        .unwrap_or_default();
    let project = agent.foreground_cwd.or(agent.cwd).unwrap_or_default();
    Some(Target {
        herdr: Some(HerdrPane {
            socket: path.to_string_lossy().into_owned(),
            server_epoch: epoch.into(),
            pane_id: agent.pane_id.clone(),
            guard: guard.into(),
        }),
        name: label(
            agent
                .name
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(&agent.pane_id),
            80,
        ),
        title,
        kind,
        project: label(&project, 240),
        status: agent.agent_status,
        session_id,
        ..Target::default()
    })
}

pub async fn discover(path: &Path) -> Result<Discovery> {
    discover_with(path, codex_index_path().as_deref()).await
}

async fn listed_agents(path: &Path) -> Result<Vec<Value>> {
    let response = rpc(path, "agent.list", json!({})).await?;
    response
        .pointer("/result/agents")
        .and_then(Value::as_array)
        .map(|agents| agents.iter().take(128).cloned().collect())
        .ok_or_else(|| ProviderError::message(INCOMPATIBLE))
}

/// Discover with Codex thread names already read.
pub async fn discover_named(
    path: &Path,
    codex_threads: &HashMap<String, String>,
) -> Result<Discovery> {
    discover_listed(path, codex_threads).await
}

/// Discover with an explicit Codex thread index; tests pass a fixture.
///
/// Herdr guards only the sessions it can vouch for, such as those it launched itself, so
/// the guarded list alone hides sessions a person started by typing `claude`. Every agent
/// Herdr lists is shown, in Herdr's pane order; only guarded ones carry a guard and can be
/// sent to. The rest stay visible without a transport.
pub async fn discover_with(path: &Path, codex_index: Option<&Path>) -> Result<Discovery> {
    let codex_threads = codex_index.map(codex_thread_names).unwrap_or_default();
    discover_listed(path, &codex_threads).await
}

async fn discover_listed(
    path: &Path,
    codex_threads: &HashMap<String, String>,
) -> Result<Discovery> {
    let response = rpc(path, "agent.guarded_list", json!({})).await?;
    let Some(result) = response.get("result") else {
        // Metadata-only compatibility view. There is intentionally no legacy send fallback.
        let agents = listed_agents(path).await?;
        return Ok(Discovery {
            targets: agents
                .into_iter()
                .filter_map(|agent| target(path, "", "", agent, codex_threads))
                .collect(),
            guarded: false,
        });
    };
    let epoch = result
        .get("server_epoch")
        .and_then(Value::as_str)
        .filter(|e| !e.is_empty() && e.len() <= 128)
        .ok_or_else(|| ProviderError::message(INCOMPATIBLE))?;
    let mut guarded: Vec<(String, String, Value)> = result
        .get("agents")
        .and_then(Value::as_array)
        .ok_or_else(|| ProviderError::message(INCOMPATIBLE))?
        .iter()
        .take(128)
        .filter_map(|entry| {
            let guard = entry.get("guard")?.as_str()?.to_owned();
            if guard.len() != 64 || !guard.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            let agent = entry.get("agent")?.clone();
            Some((agent.get("pane_id")?.as_str()?.to_owned(), guard, agent))
        })
        .collect();
    // Without the full list, the guarded agents are still correct on their own.
    let listed = listed_agents(path).await.unwrap_or_default();
    let mut targets = Vec::with_capacity(listed.len().max(guarded.len()));
    for agent in listed {
        let pane = agent.get("pane_id").and_then(Value::as_str);
        let found = guarded.iter().position(|(p, ..)| Some(p.as_str()) == pane);
        targets.extend(match found {
            // The guard belongs to the guarded snapshot, so its metadata is used with it.
            Some(index) => {
                let (_, guard, agent) = guarded.remove(index);
                target(path, epoch, &guard, agent, codex_threads)
            }
            None => target(path, epoch, "", agent, codex_threads),
        });
    }
    // An agent guarded between the two reads keeps its guard, after the others.
    targets.extend(
        guarded
            .into_iter()
            .filter_map(|(_, guard, agent)| target(path, epoch, &guard, agent, codex_threads)),
    );
    Ok(Discovery {
        targets,
        guarded: true,
    })
}

fn receipt(response: &Value, epoch: &str, submission: &Submission) -> Option<SendOutcome> {
    let result = response.get("result")?;
    if result.get("server_epoch")?.as_str()? != epoch
        || result.get("operation_id")?.as_str()? != submission.id
    {
        return None;
    }
    match result.get("state")?.as_str()? {
        "accepted" => Some(SendOutcome::Accepted),
        "rejected" => Some(SendOutcome::Rejected),
        "uncertain" => Some(SendOutcome::Unconfirmed),
        _ => None,
    }
}

/// Send once, then reconcile only by reading its receipt. Cancellation never triggers replay.
///
/// Herdr re-checks the guard against the exact agent process and pane before it writes, so a
/// replaced or restarted session is refused there, not here.
pub async fn submit(submission: &Submission, cancel: &CancellationToken) -> SendOutcome {
    let target = &submission.target;
    let Some(pane) = target
        .herdr
        .as_ref()
        .filter(|_| target.route() == Route::Herdr)
    else {
        return SendOutcome::Rejected;
    };
    if !crate::agent_targets::admissible(submission, cancel) {
        return SendOutcome::Rejected;
    }
    let path = Path::new(&pane.socket);
    let expires = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
        + 5_000;
    let params = json!({"target": pane.pane_id, "server_epoch": pane.server_epoch,
        "guard": pane.guard, "operation_id": submission.id, "expires_at_unix_ms": expires, "text": submission.text});
    let first = tokio::select! { _ = cancel.cancelled() => return SendOutcome::Unconfirmed,
    result = rpc(path, "agent.prompt_guarded", params) => result };
    if let Ok(response) = first {
        if let Some(outcome) = receipt(&response, &pane.server_epoch, submission) {
            return outcome;
        }
        if response.pointer("/error/code").and_then(Value::as_str)
            == Some("guarded_prompt_rejected")
        {
            return SendOutcome::Rejected;
        }
    }
    let lookup = json!({"server_epoch": pane.server_epoch, "operation_id": submission.id});
    let deadline = tokio::time::Instant::now() + Duration::from_secs(6);
    loop {
        let response = tokio::select! {
            _ = cancel.cancelled() => return SendOutcome::Unconfirmed,
            _ = tokio::time::sleep_until(deadline) => return SendOutcome::Unconfirmed,
            response = rpc(path, "agent.prompt_receipt", lookup.clone()) => response,
        };
        if let Ok(response) = response {
            if let Some(outcome) = receipt(&response, &pane.server_epoch, submission) {
                return outcome;
            }
            if response.get("error").is_some() {
                return SendOutcome::Unconfirmed;
            }
        }
        tokio::select! { _ = cancel.cancelled() => return SendOutcome::Unconfirmed,
        _ = tokio::time::sleep(Duration::from_millis(150)) => {} }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tokio::net::UnixListener;
    fn peer() -> (tempfile::TempDir, PathBuf, UnixListener) {
        let temp = crate::test_sockets::socket_dir();
        let path = temp.path().join("herdr.sock");
        let listener = UnixListener::bind(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        (temp, path, listener)
    }
    async fn receive(listener: &UnixListener) -> (UnixStream, Value) {
        let stream = crate::test_sockets::accept_within(listener, "Mluva never asked Herdr").await;
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        (reader.into_inner(), serde_json::from_str(&line).unwrap())
    }
    async fn reply(stream: &mut UnixStream, request: &Value, result: Value) {
        stream
            .write_all(format!("{}\n", json!({"id": request["id"], "result": result})).as_bytes())
            .await
            .unwrap();
    }
    fn pane(target: &Target) -> &str {
        &target.herdr.as_ref().unwrap().pane_id
    }
    fn submission(path: &Path) -> Submission {
        Submission {
            id: uuid::Uuid::new_v4().to_string(),
            text: "Synthetic Czech: zkouška příjemce.".into(),
            target: Target {
                herdr: Some(HerdrPane {
                    socket: path.to_string_lossy().into(),
                    server_epoch: "epoch".into(),
                    pane_id: "w1:p2".into(),
                    guard: "a".repeat(64),
                }),
                name: "selected".into(),
                title: String::new(),
                kind: AgentKind::CodexCli,
                project: "/synthetic".into(),
                status: AgentStatus::Working,
                ..Target::default()
            },
        }
    }
    #[tokio::test]
    async fn lost_acknowledgement_reads_receipt_without_resending_or_reading_terminal() {
        let (_temp, path, listener) = peer();
        let sending = submission(&path);
        let operation = sending.id.clone();
        let peer = tokio::spawn(async move {
            let (stream, request) = receive(&listener).await;
            assert_eq!(request["method"], "agent.prompt_guarded");
            let params = request["params"].as_object().unwrap();
            assert_eq!(params.len(), 6);
            assert_eq!(params["target"], "w1:p2");
            assert_eq!(params["operation_id"], operation);
            assert_eq!(params["guard"], "a".repeat(64));
            drop(stream); // Server accepted, but the first acknowledgement was lost.
            let (mut stream, request) = receive(&listener).await;
            assert_eq!(request["method"], "agent.prompt_receipt");
            assert_eq!(request["params"]["operation_id"], operation);
            reply(
                &mut stream,
                &request,
                json!({"server_epoch": "epoch", "operation_id": operation, "state": "accepted"}),
            )
            .await;
            assert!(
                tokio::time::timeout(Duration::from_millis(200), listener.accept())
                    .await
                    .is_err()
            );
        });
        assert_eq!(
            submit(&sending, &CancellationToken::new()).await,
            SendOutcome::Accepted
        );
        peer.await.unwrap();
    }
    #[tokio::test]
    async fn names_come_from_each_agents_own_session_title_in_herdr_order() {
        let (temp, path, listener) = peer();
        let index = temp.path().join("session_index.jsonl");
        let long = "Investigate why the clipboard owner changes after suspend and resume, then \
                    write the regression test before the next release";
        std::fs::write(
            &index,
            [
                json!({"id": "codex-a", "thread_name": "First name, later renamed"}),
                json!({"id": "codex-b", "thread_name": long}),
                json!({"id": "codex-a", "thread_name": "Lockdown check"}),
                json!({"id": "codex-c", "thread_name": "Přehled zkoušek 修复 🙂"}),
            ]
            .iter()
            .map(|line| format!("{line}\n"))
            .collect::<String>(),
        )
        .unwrap();
        let agent = |pane: &str, kind: &str, name: &str, title: Value, session: Value| {
            json!({"guard": "e".repeat(64), "agent": {"pane_id": pane, "agent": kind, "name": name,
                "agent_status": "idle", "terminal_title_stripped": title,
                "agent_session": session}})
        };
        let codex = |id: &str| json!({"source": "herdr:codex", "agent": "codex", "kind": "id", "value": id});
        let claude = |id: &str| json!({"source": "herdr:claude", "agent": "claude", "kind": "id", "value": id});
        let agents = json!([
            agent(
                "w1:p1",
                "codex",
                "dispatch-01",
                json!("Lockdown check | project"),
                codex("codex-a")
            ),
            agent(
                "w1:p2",
                "claude",
                "notes-lead",
                json!("Meeting notes"),
                claude("claude-a")
            ),
            agent(
                "w1:p3",
                "codex",
                "cos",
                json!("Fallback | project"),
                codex("codex-b")
            ),
            agent(
                "w2:p1",
                "claude",
                "reviewer",
                json!("Claude Code"),
                claude("claude-b")
            ),
            agent(
                "w2:p2",
                "codex",
                "unnamed-codex",
                json!("project"),
                codex("codex-new")
            ),
            agent("w2:p3", "codex", "", Value::Null, codex("codex-c")),
            agent("w2:p4", "claude", "", Value::Null, Value::Null),
        ]);
        let peer = tokio::spawn(async move {
            let (mut stream, request) = receive(&listener).await;
            assert_eq!(request["method"], "agent.guarded_list");
            reply(
                &mut stream,
                &request,
                json!({"server_epoch": "epoch", "agents": agents}),
            )
            .await;
        });
        let found = discover_with(&path, Some(&index)).await.unwrap();
        peer.await.unwrap();
        let shown: Vec<_> = found
            .targets
            .iter()
            .map(|t| (pane(t), t.display_name(), t.title.as_str()))
            .collect();
        assert_eq!(
            shown,
            [
                // Codex: the thread index for this session, never its terminal-title layout.
                ("w1:p1", "Lockdown check", "Lockdown check"),
                // Claude Code: its own session title, not Herdr's dispatch label.
                ("w1:p2", "Meeting notes", "Meeting notes"),
                // Long titles stay whole.
                ("w1:p3", long, long),
                // No session title yet: Herdr's label is a genuine fallback, not a parse.
                ("w2:p1", "reviewer", ""),
                ("w2:p2", "unnamed-codex", ""),
                (
                    "w2:p3",
                    "Přehled zkoušek 修复 🙂",
                    "Přehled zkoušek 修复 🙂"
                ),
                ("w2:p4", "w2:p4", ""),
            ]
        );
        // Identity is unchanged by naming; the guard and pane remain the selection key.
        assert!(
            found
                .targets
                .iter()
                .all(|t| t.herdr.as_ref().unwrap().guard == "e".repeat(64))
        );
        assert!(codex_thread_names(&temp.path().join("missing.jsonl")).is_empty());
    }
    /// Herdr guards only sessions it launched; one typed into a pane is in `agent.list` but
    /// not `agent.guarded_list`. It must still be offered, visible and never sendable.
    #[tokio::test]
    async fn an_unmanaged_claude_session_herdr_lists_but_does_not_guard_is_still_discovered() {
        let (_temp, path, listener) = peer();
        let session = |id: &str| json!({"source": "herdr:claude", "agent": "claude", "kind": "id", "value": id});
        let managed = json!({"pane_id": "w1:p1", "agent": "claude", "name": "lead",
            "agent_status": "idle", "agent_session": session("managed-session")});
        let typed = json!({"pane_id": "w1:p2", "agent": "claude", "agent_status": "idle",
            "terminal_title_stripped": "Typed session", "agent_session": session("typed-session")});
        let shell = json!({"pane_id": "w1:p3", "name": "shell"});
        let codex = json!({"pane_id": "w2:p1", "agent": "codex", "name": "builder", "agent_status": "idle"});
        let (guarded, listed) = (
            json!([{"guard": "f".repeat(64), "agent": managed}, {"guard": "e".repeat(64), "agent": codex}]),
            json!([managed, typed, shell, codex]),
        );
        let peer = tokio::spawn(async move {
            let (mut stream, request) = receive(&listener).await;
            assert_eq!(request["method"], "agent.guarded_list");
            reply(
                &mut stream,
                &request,
                json!({"server_epoch": "epoch", "agents": guarded}),
            )
            .await;
            let (mut stream, request) = receive(&listener).await;
            assert_eq!(request["method"], "agent.list");
            reply(&mut stream, &request, json!({"agents": listed})).await;
            // Discovery reads only; it never prompts, focuses or reads a terminal.
            assert!(
                tokio::time::timeout(Duration::from_millis(200), listener.accept())
                    .await
                    .is_err()
            );
        });
        let found = discover_with(&path, None).await.unwrap();
        let shown: Vec<_> = found
            .targets
            .iter()
            .map(|t| (pane(t), t.display_name(), t.can_receive()))
            .collect();
        assert_eq!(
            shown,
            [
                ("w1:p1", "lead", true),
                ("w1:p2", "Typed session", false),
                ("w2:p1", "builder", true),
            ],
            "every listed agent appears in Herdr's order; only guarded ones can receive"
        );
        peer.await.unwrap();
        assert!(found.guarded);
        let typed = found.targets[1].clone();
        assert!(typed.herdr.as_ref().unwrap().guard.is_empty());
        assert_eq!(typed.herdr.as_ref().unwrap().server_epoch, "epoch");
        assert!(typed.draft_only() && typed.route() == Route::DraftOnly);
        // Without a guard there is no send path: nothing even connects.
        let sending = Submission {
            target: typed,
            ..submission(&path)
        };
        assert_eq!(
            submit(&sending, &CancellationToken::new()).await,
            SendOutcome::Rejected
        );
    }
    #[tokio::test]
    async fn native_session_ids_come_from_herdr_and_a_failed_full_list_keeps_guarded_agents() {
        let (_temp, path, listener) = peer();
        let peer = tokio::spawn(async move {
            let (mut stream, request) = receive(&listener).await;
            reply(&mut stream, &request, json!({"server_epoch": "epoch", "agents": [{"guard": "f".repeat(64),
                "agent": {"pane_id": "w1:p1", "agent": "claude", "agent_status": "idle", "agent_session":
                {"source": "herdr:claude", "agent": "claude", "kind": "id", "value": "managed-session"}}}]})).await;
            let (mut stream, request) = receive(&listener).await;
            assert_eq!(request["method"], "agent.list");
            stream
                .write_all(
                    format!(
                        "{}\n",
                        json!({"id": request["id"], "error": {"code": "internal"}})
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        });
        let found = discover_with(&path, None).await.unwrap();
        peer.await.unwrap();
        assert_eq!(found.targets.len(), 1);
        assert_eq!(found.targets[0].session_id, "managed-session");
        assert!(found.targets[0].can_receive());
    }
    #[test]
    fn a_large_codex_index_is_read_from_its_newest_whole_lines() {
        let temp = tempfile::tempdir().unwrap();
        let index = temp.path().join("session_index.jsonl");
        let filler = format!("{}\n", json!({"id": "old", "thread_name": "x".repeat(200)}));
        let mut body = filler.repeat((MAX_CODEX_INDEX_BYTES as usize / filler.len()) + 8);
        body.push_str(&format!(
            "{}\n",
            json!({"id": "new", "thread_name": "Newest thread"})
        ));
        std::fs::write(&index, body).unwrap();
        let names = codex_thread_names(&index);
        assert_eq!(names.get("new").map(String::as_str), Some("Newest thread"));
        assert!(names.get("old").is_some_and(|name| name.len() == 200));
    }
    #[tokio::test]
    async fn old_server_has_metadata_only_and_never_receives_an_unguarded_prompt() {
        let (_temp, path, listener) = peer();
        let peer = tokio::spawn(async move {
            let (mut stream, request) = receive(&listener).await;
            assert_eq!(request["method"], "agent.guarded_list");
            stream
                .write_all(
                    format!(
                        "{}\n",
                        json!({"id": request["id"], "error": {"code": "unknown_method"}})
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            drop(stream);
            let (mut stream, request) = receive(&listener).await;
            assert_eq!(request["method"], "agent.list");
            reply(&mut stream, &request, json!({"agents": [{"pane_id": "w1:p2", "name": "duplicate name", "agent": "codex", "agent_status": "idle"}]})).await;
        });
        let found = discover(&path).await.unwrap();
        assert!(!found.guarded);
        assert_eq!(found.targets.len(), 1);
        assert!(!found.targets[0].can_receive());
        peer.await.unwrap();
        let sending = Submission {
            target: found.targets[0].clone(),
            ..submission(&path)
        };
        assert_eq!(
            submit(&sending, &CancellationToken::new()).await,
            SendOutcome::Rejected
        );
    }
    #[tokio::test]
    async fn restarted_server_and_conflicting_receipt_leave_delivery_unconfirmed() {
        let (_temp, path, listener) = peer();
        let sending = submission(&path);
        let peer = tokio::spawn(async move {
            let (stream, _) = receive(&listener).await;
            drop(stream);
            let (mut stream, request) = receive(&listener).await;
            reply(&mut stream, &request, json!({"server_epoch": "different", "operation_id": "different", "state": "accepted"})).await;
            drop(stream);
            let (mut stream, request) = receive(&listener).await;
            stream
                .write_all(
                    format!(
                        "{}\n",
                        json!({"id": request["id"], "error": {"code": "receipt_unknown"}})
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        });
        assert_eq!(
            submit(&sending, &CancellationToken::new()).await,
            SendOutcome::Unconfirmed
        );
        peer.await.unwrap();
    }
    #[test]
    fn rejects_world_writable_and_symlink_socket_endpoints() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _entered = runtime.enter();
        let (temp, path, _listener) = peer();
        assert!(secure_socket(&path).is_ok());
        let alias = temp.path().join("alias.sock");
        std::os::unix::fs::symlink(&path, &alias).unwrap();
        assert!(secure_socket(&alias).is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666)).unwrap();
        assert!(secure_socket(&path).is_err());
    }
}
