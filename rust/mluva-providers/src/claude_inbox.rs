//! Claude Code's per-session inbox: a Queue route to any running Claude Code session, whatever
//! terminal, multiplexer or application hosts it.
//!
//! Claude Code binds one private Unix socket per session for cross-session messaging and names
//! it in the session's registry entry. Writing one `user` frame puts the text into that session's
//! own prompt queue: the agent takes it between tool calls or as a new turn, and its composer, a
//! permission prompt or a running tool are never touched. The agent sees it as a message from
//! another Claude session, not as typed by the person. The socket and its delivery rules are
//! documented; the frame and the registry field are not, so the route is feature-detected
//! (`peerProtocol` 1) and a session without it stays draft only.
//!
//! Immediately before writing, the registry entry is read again with the registry's safety bar
//! and must still name the chosen process, session and socket; the socket must sit in one of
//! Claude Code's inbox directories, private to this account, and the connected peer must be that
//! process. The frame carries the session ID, which Claude Code checks as well, so a `/clear` in
//! between drops it. The socket answers nothing, so what happened is read from the session's own
//! transcript: the queue admitting exactly this text (queued), a turn or queue record carrying
//! this message's ID (taken into a turn), or a notice that the session holds it for approval.
//! Without any of them the delivery is unconfirmed and is never written again. Only lines that
//! can be such a record are parsed, they are compared in memory and nothing from them is kept or
//! logged.
use crate::{
    agent_platform,
    agent_targets::{Receipt, admissible},
    claude_sessions::{self, MAX_SOCKET_PATH},
};
use mluva_core::agent_voice::{
    ClaudeInbox, MAX_MESSAGE_BYTES, Route, SendOutcome, Submission, Target,
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    os::unix::fs::{FileTypeExt, MetadataExt},
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{
    io::{AsyncWrite, AsyncWriteExt},
    net::UnixStream,
};
use tokio_util::sync::CancellationToken;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// Claude Code records an accepted message within about 50 ms; a session that has not after
/// this long refused or dropped it, or has not read it yet.
const RECEIPT_WINDOW: Duration = Duration::from_millis(2_500);
const RECEIPT_POLL: Duration = Duration::from_millis(50);
/// A queued message is taken at the agent's next tool boundary or after its turn; past this,
/// History keeps saying queued.
const TAKEN_WATCH: Duration = Duration::from_secs(20 * 60);
const TAKEN_POLL: Duration = Duration::from_millis(500);
/// Transcript bytes read per poll; a busy session's tool output is read in later polls.
const MAX_RECEIPT_READ: u64 = 1_048_576;
/// A receipt line holds at most the message and its metadata; anything longer is content.
const MAX_RECEIPT_LINE: usize = 4 * MAX_MESSAGE_BYTES + 16_384;

/// Where and how long the transport looks; tests narrow both.
struct Limits {
    /// Claude Code's inbox directories; a socket anywhere else is not trusted.
    directories: Vec<PathBuf>,
    receipt: Duration,
    taken: Duration,
}

/// Queue a message once into the session's own inbox, then read its receipt.
pub async fn submit(submission: &Submission, cancel: &CancellationToken) -> Receipt {
    let limits = Limits {
        directories: agent_platform::inbox_directories(),
        receipt: RECEIPT_WINDOW,
        taken: TAKEN_WATCH,
    };
    submit_within(submission, cancel, &limits).await
}

async fn submit_within(
    submission: &Submission,
    cancel: &CancellationToken,
    limits: &Limits,
) -> Receipt {
    let target = &submission.target;
    let Some(inbox) = target
        .inbox
        .as_ref()
        .filter(|_| target.route() == Route::ClaudeInbox)
    else {
        return SendOutcome::Rejected.into();
    };
    if !admissible(submission, cancel) {
        return SendOutcome::Rejected.into();
    }
    // Everything is checked again now; anything that changed refuses rather than retargets.
    let Some((config, cwd)) = recheck(inbox, target, &limits.directories) else {
        return SendOutcome::Rejected.into();
    };
    let mut transcript = Transcript::new(config, cwd, submission);
    let Some(stream) = connect(inbox, target.pid).await else {
        return SendOutcome::Rejected.into();
    };
    match write_frame(stream, &frame(submission)).await {
        Written::Nothing => return SendOutcome::Rejected.into(),
        Written::Partly => return SendOutcome::Unconfirmed.into(),
        Written::Whole => {}
    }
    match transcript.receipt(cancel, limits.receipt).await {
        Some(Evidence::Taken) => SendOutcome::Taken.into(),
        Some(Evidence::Held) => SendOutcome::Held.into(),
        Some(Evidence::Queued) => {
            let (cancel, limit) = (cancel.clone(), limits.taken);
            Receipt {
                outcome: SendOutcome::Accepted,
                taken: Some(Box::pin(
                    async move { transcript.taken(&cancel, limit).await },
                )),
            }
        }
        None => SendOutcome::Unconfirmed.into(),
    }
}

/// The session's registry entry, read again: still this process, this session and this inbox,
/// at a socket only this account can reach. Returns the configuration directory and the
/// session's working directory, which locate its transcript.
fn recheck(
    inbox: &ClaudeInbox,
    target: &Target,
    directories: &[PathBuf],
) -> Option<(PathBuf, String)> {
    let entry = Path::new(&inbox.entry);
    let sessions = entry.parent()?;
    if entry.file_name()?.to_str()? != format!("{}.json", target.pid)
        || !claude_sessions::private_directory(sessions)
    {
        return None;
    }
    let started = agent_platform::process_start(target.pid)?;
    let namespace = agent_platform::own_namespace();
    let Some(Ok((start, session))) =
        claude_sessions::read_entry(entry, target.pid, namespace.as_deref())
    else {
        return None;
    };
    let same = start == started
        && start == inbox.proc_start
        && session.session_id == target.session_id
        && session
            .inbox
            .as_ref()
            .is_some_and(|fresh| fresh.socket == inbox.socket);
    let config = sessions.parent()?.to_path_buf();
    (same && trusted_socket(Path::new(&inbox.socket), directories)).then_some((config, session.cwd))
}

/// A socket owned by this account, not reached through a symlink, in one of Claude Code's inbox
/// directories that only this account can enter, at a path short enough to be bound whole.
fn trusted_socket(path: &Path, directories: &[PathBuf]) -> bool {
    let uid = agent_platform::effective_uid();
    let Some(parent) = path.parent() else {
        return false;
    };
    let directory = fs::symlink_metadata(parent).ok();
    path.is_absolute()
        && path.as_os_str().len() <= MAX_SOCKET_PATH
        && directories.iter().any(|known| known == parent)
        && directory
            .is_some_and(|d| d.file_type().is_dir() && d.uid() == uid && d.mode() & 0o077 == 0)
        && fs::symlink_metadata(path).is_ok_and(|s| s.file_type().is_socket() && s.uid() == uid)
}

/// Connect, and keep the connection only if the process answering is the session's own.
async fn connect(inbox: &ClaudeInbox, pid: u32) -> Option<UnixStream> {
    let stream = tokio::time::timeout(CONNECT_TIMEOUT, UnixStream::connect(&inbox.socket))
        .await
        .ok()?
        .ok()?;
    let (uid, peer) = agent_platform::peer_process(&stream)?;
    (uid == agent_platform::effective_uid() && peer == pid).then_some(stream)
}

/// One line: the text as a user message, the session it is meant for and this message's ID.
/// Nothing else: no priority, sender or attachments.
fn frame(submission: &Submission) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(&json!({
        "type": "user",
        "message": {"role": "user", "content": submission.text},
        "session_id": submission.target.session_id,
        "uuid": submission.id,
    }))
    .unwrap_or_default();
    bytes.push(b'\n');
    bytes
}

#[derive(Debug, PartialEq, Eq)]
enum Written {
    /// Nothing reached the socket.
    Nothing,
    /// Some of the frame may have reached the session.
    Partly,
    Whole,
}

/// Write the frame once and half-close. A failure after the first byte is never retried.
async fn write_frame<W: AsyncWrite + Unpin>(mut writer: W, frame: &[u8]) -> Written {
    let mut sent = 0;
    let _ = tokio::time::timeout(CONNECT_TIMEOUT, async {
        while sent < frame.len() {
            match writer.write(&frame[sent..]).await {
                Ok(0) | Err(_) => return,
                Ok(written) => sent += written,
            }
        }
        let _ = writer.shutdown().await;
    })
    .await;
    match sent {
        0 => Written::Nothing,
        sent if sent < frame.len() => Written::Partly,
        _ => Written::Whole,
    }
}

/// What the session's transcript shows of this message, weakest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Evidence {
    /// The queue admitted it.
    Queued,
    /// The session holds it until the person approves it there.
    Held,
    /// The agent took it into a turn.
    Taken,
}

/// The new end of one session's transcript, read for this message's receipt.
#[derive(Default)]
struct Transcript {
    config: PathBuf,
    cwd: String,
    session_id: String,
    id: String,
    text: String,
    path: Option<PathBuf>,
    /// Bytes before the write are history, never a receipt.
    offset: u64,
    partial: Vec<u8>,
    /// Inside a line too long to be a receipt.
    skipping: bool,
}

impl Transcript {
    fn new(config: PathBuf, cwd: String, submission: &Submission) -> Self {
        let session_id = submission.target.session_id.clone();
        let path = claude_sessions::transcript_path(&config, &cwd, &session_id);
        let offset = path
            .as_deref()
            .and_then(claude_sessions::open_private)
            .map_or(0, |(_, metadata)| metadata.len());
        Self {
            config,
            cwd,
            session_id,
            id: submission.id.clone(),
            text: submission.text.clone(),
            path,
            offset,
            partial: vec![],
            skipping: false,
        }
    }

    /// The first evidence within the window, if any; a cancelled wait has none.
    async fn receipt(&mut self, cancel: &CancellationToken, window: Duration) -> Option<Evidence> {
        let deadline = tokio::time::Instant::now() + window;
        loop {
            if let Some(evidence) = self.read_blocking().await {
                return Some(evidence);
            }
            if tokio::time::Instant::now() >= deadline {
                return None;
            }
            tokio::select! {
                _ = cancel.cancelled() => return None,
                _ = tokio::time::sleep(RECEIPT_POLL) => {}
            }
        }
    }

    /// After a queued receipt: true once the agent takes the message into a turn.
    async fn taken(&mut self, cancel: &CancellationToken, limit: Duration) -> bool {
        let deadline = tokio::time::Instant::now() + limit;
        while tokio::time::Instant::now() < deadline {
            if self.read_blocking().await == Some(Evidence::Taken) {
                return true;
            }
            tokio::select! {
                _ = cancel.cancelled() => return false,
                _ = tokio::time::sleep(TAKEN_POLL) => {}
            }
        }
        false
    }

    /// [`Self::read`] on a blocking thread, away from whatever thread awaits the delivery.
    async fn read_blocking(&mut self) -> Option<Evidence> {
        let mut moved = std::mem::take(self);
        let (moved, evidence) = tokio::task::spawn_blocking(move || {
            let evidence = moved.read();
            (moved, evidence)
        })
        .await
        .ok()?;
        *self = moved;
        evidence
    }

    /// Read what was appended since the last read; the strongest evidence in it.
    fn read(&mut self) -> Option<Evidence> {
        if self.path.is_none() {
            // A session's first message creates its transcript.
            self.path = claude_sessions::transcript_path(&self.config, &self.cwd, &self.session_id);
        }
        let (mut file, metadata) = self
            .path
            .as_deref()
            .and_then(claude_sessions::open_private)?;
        let length = metadata.len();
        if length < self.offset {
            // Rewritten: start again from its beginning.
            (self.offset, self.skipping) = (0, false);
            self.partial.clear();
        }
        if length == self.offset || file.seek(SeekFrom::Start(self.offset)).is_err() {
            return None;
        }
        let mut bytes = Vec::new();
        (&mut file)
            .take((length - self.offset).min(MAX_RECEIPT_READ))
            .read_to_end(&mut bytes)
            .ok()?;
        self.offset += bytes.len() as u64;
        let mut best = None;
        for byte in bytes {
            if byte != b'\n' {
                if !self.skipping {
                    self.partial.push(byte);
                    self.skipping = self.partial.len() > MAX_RECEIPT_LINE;
                }
                continue;
            }
            let line = std::mem::take(&mut self.partial);
            if !std::mem::replace(&mut self.skipping, false) {
                best = best.max(evidence(&line, &self.id, &self.text, &self.session_id));
            }
        }
        if self.skipping {
            self.partial.clear();
        }
        best
    }
}

/// What one transcript line says of this message: the queue admitting exactly this text for
/// this session, the turn, queue or attachment record carrying this message's ID, or the notice
/// that holds a message whose preview starts this text.
fn evidence(line: &[u8], id: &str, text: &str, session_id: &str) -> Option<Evidence> {
    let has = |mark: &[u8]| line.windows(mark.len()).any(|w| w == mark);
    if !line.starts_with(b"{")
        || !(has(b"\"queue-operation\"")
            || has(id.as_bytes())
            || has("Held peer message".as_bytes()))
    {
        return None;
    }
    let record = serde_json::from_slice::<Value>(line).ok()?;
    let field = |key: &str| record.get(key).and_then(Value::as_str);
    let ours = Some(id);
    match field("type")? {
        "queue-operation" if field("commandUuid") == ours => Some(Evidence::Taken),
        "queue-operation" => (field("operation") == Some("enqueue")
            && field("content").is_some_and(|content| content.trim() == text.trim())
            && field("sessionId").is_none_or(|sid| sid == session_id))
        .then_some(Evidence::Queued),
        "user" => (field("uuid") == ours).then_some(Evidence::Taken),
        "attachment" => (record
            .pointer("/attachment/source_uuid")
            .and_then(Value::as_str)
            == ours)
            .then_some(Evidence::Taken),
        "system" => held_preview(field("content")?)
            .filter(|preview| !preview.is_empty() && text.trim_start().starts_with(preview))
            .map(|_| Evidence::Held),
        _ => None,
    }
}

/// The preview of the text a held-message notice quotes, without a trailing ellipsis.
fn held_preview(notice: &str) -> Option<&str> {
    let rest = notice.strip_prefix("Held peer message")?;
    let start = rest.find('«')? + '«'.len_utf8();
    let end = start + rest[start..].find('»')?;
    Some(rest[start..end].trim_end_matches('…').trim())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude_sessions::{ClaudeRegistry, merge};
    use mluva_core::agent_voice::{AgentKind, AgentStatus, Delivery};
    use std::os::unix::fs::PermissionsExt;
    use tokio::{io::AsyncReadExt, net::UnixListener};

    const SESSION: &str = "0f0f0f0f-1111-4222-8333-444444444444";
    const CWD: &str = "/synthetic/inbox";

    /// A Claude Code configuration with one live session, this test process, whose registry
    /// entry names an inbox socket a test peer listens on.
    struct Fixture {
        _temp: tempfile::TempDir,
        config: PathBuf,
        socket: PathBuf,
        transcript: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let temp = tempfile::tempdir().unwrap();
            let config = temp.path().join("claude");
            let socks = temp.path().join("cc-socks");
            for directory in [
                config.join("sessions"),
                config.join("projects"),
                socks.clone(),
            ] {
                fs::create_dir_all(&directory).unwrap();
                fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
            }
            let socket = socks.join(format!("{}.sock", std::process::id()));
            let projects = config.join("projects").join(claude_sessions::slug(CWD));
            fs::create_dir_all(&projects).unwrap();
            fs::set_permissions(&projects, fs::Permissions::from_mode(0o700)).unwrap();
            let fixture = Self {
                transcript: projects.join(format!("{SESSION}.jsonl")),
                _temp: temp,
                config,
                socket,
            };
            fixture.entry(json!({}));
            fs::write(
                &fixture.transcript,
                "{\"type\":\"ai-title\",\"aiTitle\":\"Inbox\"}\n",
            )
            .unwrap();
            fixture
        }
        /// The registry entry, shaped like Claude Code 2.1.310's, with `extra` merged in.
        fn entry(&self, extra: Value) {
            let pid = std::process::id();
            let mut entry = json!({"pid": pid, "sessionId": SESSION, "cwd": CWD,
                "startedAt": 1_793_000_000_000_u64, "procStart": agent_platform::process_start(pid).unwrap(),
                "version": "2.1.310", "peerProtocol": 1, "kind": "interactive", "entrypoint": "cli",
                "messagingSocketPath": self.socket.to_string_lossy(), "name": "inbox-fixture",
                "nameSource": "user", "status": "idle"});
            entry
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            let path = self.config.join("sessions").join(format!("{pid}.json"));
            fs::write(&path, entry.to_string()).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        /// The session as discovery offers it.
        fn target(&self) -> Target {
            let sessions = ClaudeRegistry::new(&self.config, 0).sessions();
            merge(vec![], &sessions).remove(0)
        }
        fn listen(&self) -> UnixListener {
            let listener = UnixListener::bind(&self.socket).unwrap();
            fs::set_permissions(&self.socket, fs::Permissions::from_mode(0o600)).unwrap();
            listener
        }
        /// This fixture's socket directory stands in for Claude Code's.
        fn limits(&self, receipt_ms: u64) -> Limits {
            Limits {
                directories: vec![self.socket.parent().unwrap().to_path_buf()],
                receipt: Duration::from_millis(receipt_ms),
                taken: Duration::from_secs(5),
            }
        }
        async fn send(&self, submission: &Submission, cancel: &CancellationToken) -> Receipt {
            submit_within(submission, cancel, &self.limits(2_500)).await
        }
        async fn outcome(&self, submission: &Submission) -> SendOutcome {
            self.send(submission, &CancellationToken::new())
                .await
                .outcome
        }
    }

    fn append(path: &Path, lines: &[Value]) {
        let mut file = fs::OpenOptions::new().append(true).open(path).unwrap();
        for line in lines {
            std::io::Write::write_all(&mut file, format!("{line}\n").as_bytes()).unwrap();
        }
    }

    fn submission(target: Target, text: &str) -> Submission {
        Submission {
            id: uuid::Uuid::new_v4().to_string(),
            target,
            text: text.into(),
        }
    }

    /// Accept one connection and return the frame it carried.
    async fn frame_from(listener: &UnixListener) -> Value {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = vec![];
        stream.read_to_end(&mut bytes).await.unwrap();
        assert_eq!(bytes.last(), Some(&b'\n'), "one whole line");
        serde_json::from_slice(&bytes).unwrap()
    }

    /// No second connection: nothing is ever written again.
    async fn no_more_connections(listener: &UnixListener) {
        assert!(
            tokio::time::timeout(Duration::from_millis(300), listener.accept())
                .await
                .is_err()
        );
    }

    #[test]
    fn discovery_offers_queue_only_for_a_session_whose_entry_names_an_inbox() {
        let fixture = Fixture::new();
        let target = fixture.target();
        assert_eq!(
            (target.route(), target.delivery(), target.can_receive()),
            (Route::ClaudeInbox, Delivery::Queue, true)
        );
        let inbox = target.inbox.as_ref().unwrap();
        assert_eq!(inbox.socket, fixture.socket.to_string_lossy());
        // Another protocol, or none, leaves the session draft only.
        fixture.entry(json!({"peerProtocol": 2}));
        assert!(fixture.target().draft_only());
        fixture.entry(json!({"messagingSocketPath": null}));
        let none = fixture.target();
        assert_eq!(none.draft_hint(), "Draft only: this session has no inbox");
        fixture.entry(json!({"messagingSocketPath": "relative/inbox.sock"}));
        assert!(fixture.target().draft_only());
        // A Code session of the Claude desktop app has the same inbox.
        fixture.entry(json!({"entrypoint": "claude-desktop"}));
        let desktop = fixture.target();
        assert_eq!(
            (desktop.kind, desktop.route(), desktop.place().as_str()),
            (AgentKind::ClaudeDesktop, Route::ClaudeInbox, "app")
        );
        // A session waiting on a prompt can be queued to: the message waits behind it.
        fixture.entry(json!({"status": "waiting", "waitingFor": "permission prompt"}));
        let waiting = fixture.target();
        assert_eq!(waiting.status, AgentStatus::Blocked);
        assert!(waiting.can_receive());
        assert_eq!(
            waiting.delivery_hint(),
            Some("Waits behind its prompt, as a message from another Claude session")
        );
    }

    #[tokio::test]
    async fn an_idle_session_takes_the_frame_once_and_its_turn_record_is_the_receipt() {
        let fixture = Fixture::new();
        let listener = fixture.listen();
        let sending = submission(fixture.target(), "Synthetic Czech: zkus testy znovu.");
        let id = sending.id.clone();
        let transcript = fixture.transcript.clone();
        let peer = tokio::spawn(async move {
            let frame = frame_from(&listener).await;
            let keys: Vec<_> = frame.as_object().unwrap().keys().cloned().collect();
            assert_eq!(keys, ["type", "message", "session_id", "uuid"]);
            assert_eq!(frame["type"], "user");
            assert_eq!(
                frame["message"],
                json!({"role": "user", "content": "Synthetic Czech: zkus testy znovu."})
            );
            assert_eq!(frame["session_id"], SESSION);
            assert_eq!(frame["uuid"], id.as_str());
            append(
                &transcript,
                &[
                    json!({"type": "queue-operation", "operation": "enqueue", "sessionId": SESSION, "content": "Synthetic Czech: zkus testy znovu."}),
                    json!({"type": "queue-operation", "operation": "dequeue", "sessionId": SESSION}),
                    json!({"type": "user", "uuid": id, "isMeta": true, "origin": {"kind": "peer"}}),
                ],
            );
            no_more_connections(&listener).await;
        });
        let receipt = fixture.send(&sending, &CancellationToken::new()).await;
        assert_eq!(receipt.outcome, SendOutcome::Taken);
        assert!(receipt.taken.is_none());
        peer.await.unwrap();
    }

    #[tokio::test]
    async fn a_working_session_queues_it_and_is_seen_taking_it_at_its_next_tool_boundary() {
        let fixture = Fixture::new();
        let listener = fixture.listen();
        let target = Target {
            status: AgentStatus::Working,
            ..fixture.target()
        };
        let sending = submission(target, "Run the tests again");
        let id = sending.id.clone();
        // An older admission of the same text, before this write, is history.
        append(
            &fixture.transcript,
            &[
                json!({"type": "queue-operation", "operation": "enqueue", "sessionId": SESSION, "content": "Run the tests again"}),
            ],
        );
        let transcript = fixture.transcript.clone();
        let (admitted, absorb) = tokio::sync::oneshot::channel::<()>();
        let peer = tokio::spawn(async move {
            frame_from(&listener).await;
            // Another session's admission and other text are not this message's receipt.
            append(
                &transcript,
                &[
                    json!({"type": "queue-operation", "operation": "enqueue", "sessionId": "other-session", "content": "Run the tests again"}),
                    json!({"type": "queue-operation", "operation": "enqueue", "sessionId": SESSION, "content": "Different text"}),
                    json!({"type": "queue-operation", "operation": "enqueue", "sessionId": SESSION, "content": "Run the tests again"}),
                ],
            );
            absorb.await.unwrap();
            append(
                &transcript,
                &[
                    json!({"type": "assistant", "message": {"content": "tool output"}}),
                    json!({"type": "queue-operation", "operation": "remove", "reason": "absorbed_mid_turn", "commandUuid": id}),
                ],
            );
        });
        let receipt = fixture.send(&sending, &CancellationToken::new()).await;
        assert_eq!(receipt.outcome, SendOutcome::Accepted);
        let taken = receipt
            .taken
            .expect("a queued message is watched until it is taken");
        admitted.send(()).unwrap();
        assert!(taken.await);
        peer.await.unwrap();
    }

    #[tokio::test]
    async fn a_held_message_is_held_and_no_receipt_is_unconfirmed_never_resent() {
        let fixture = Fixture::new();
        let listener = fixture.listen();
        let transcript = fixture.transcript.clone();
        let peer = tokio::spawn(async move {
            frame_from(&listener).await;
            // A session that holds messages from other sessions shows only a notice.
            append(
                &transcript,
                &[
                    json!({"type": "system", "subtype": "informational", "level": "warning",
                    "content": "Held peer message — from an unidentified session [verified pid 4100]; preview: «Please review the release notes for…» — not delivered to Claude (1 held)."}),
                ],
            );
            frame_from(&listener).await;
            no_more_connections(&listener).await;
        });
        let held = submission(
            fixture.target(),
            "Please review the release notes for the next version today",
        );
        assert_eq!(fixture.outcome(&held).await, SendOutcome::Held);
        // A refused or dropped message leaves no record at all.
        let silent = submission(fixture.target(), "Refused message");
        assert_eq!(
            submit_within(&silent, &CancellationToken::new(), &fixture.limits(400))
                .await
                .outcome,
            SendOutcome::Unconfirmed
        );
        peer.await.unwrap();
    }

    #[tokio::test]
    async fn a_replaced_session_stale_process_or_moved_inbox_is_refused_before_connecting() {
        let fixture = Fixture::new();
        let listener = fixture.listen();
        let chosen = fixture.target();
        let refused = [
            // `/clear` started a new session in the same process.
            json!({"sessionId": "1e1e1e1e-1111-4222-8333-555555555555"}),
            // The PID now belongs to another process.
            json!({"procStart": "1"}),
            // The entry names a different socket.
            json!({"messagingSocketPath": "/run/user/fixture/cc-socks/other.sock"}),
            // The inbox is gone: messaging was turned off or the session restarted without it.
            json!({"messagingSocketPath": null}),
        ];
        for change in refused {
            fixture.entry(change.clone());
            let sending = submission(chosen.clone(), "Never delivered");
            assert_eq!(
                fixture.outcome(&sending).await,
                SendOutcome::Rejected,
                "{change}"
            );
        }
        // The process exited and its entry went with it.
        fs::remove_file(
            fixture
                .config
                .join("sessions")
                .join(format!("{}.json", std::process::id())),
        )
        .unwrap();
        assert_eq!(
            fixture
                .outcome(&submission(chosen.clone(), "Never delivered"))
                .await,
            SendOutcome::Rejected
        );
        // A missing target, or one whose state no transport accepts, is refused before anything
        // is read.
        fixture.entry(json!({}));
        for target in [
            Target {
                missing: true,
                ..chosen.clone()
            },
            Target {
                status: AgentStatus::Unknown,
                ..chosen.clone()
            },
        ] {
            assert_eq!(
                fixture
                    .outcome(&submission(target, "Never delivered"))
                    .await,
                SendOutcome::Rejected
            );
        }
        no_more_connections(&listener).await;
    }

    #[tokio::test]
    async fn an_untrusted_socket_or_a_missing_listener_is_never_written_to() {
        let fixture = Fixture::new();
        let target = fixture.target();
        // No listener: the transport is missing, so nothing is sent.
        assert_eq!(
            fixture
                .outcome(&submission(target.clone(), "No listener"))
                .await,
            SendOutcome::Rejected
        );
        let listener = fixture.listen();
        // A socket outside Claude Code's inbox directories.
        let elsewhere = Limits {
            directories: vec![PathBuf::from("/run/user/fixture/cc-socks")],
            ..fixture.limits(400)
        };
        assert_eq!(
            submit_within(
                &submission(target.clone(), "Outside the inbox directories"),
                &CancellationToken::new(),
                &elsewhere
            )
            .await
            .outcome,
            SendOutcome::Rejected
        );
        // A socket directory others can enter, or a symlink in the socket's place.
        let socks = fixture.socket.parent().unwrap();
        fs::set_permissions(socks, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            fixture
                .outcome(&submission(target.clone(), "Shared directory"))
                .await,
            SendOutcome::Rejected
        );
        fs::set_permissions(socks, fs::Permissions::from_mode(0o700)).unwrap();
        let real = socks.join("real.sock");
        fs::rename(&fixture.socket, &real).unwrap();
        std::os::unix::fs::symlink(&real, &fixture.socket).unwrap();
        assert_eq!(
            fixture
                .outcome(&submission(target, "Symlinked socket"))
                .await,
            SendOutcome::Rejected
        );
        no_more_connections(&listener).await;
    }

    #[tokio::test]
    async fn a_socket_answered_by_another_process_gets_nothing() {
        // The registry names this test's parent process, but the socket is this process's.
        let fixture = Fixture::new();
        let listener = fixture.listen();
        let parent = agent_platform::parent(std::process::id()).unwrap();
        let sessions = fixture.config.join("sessions");
        fs::remove_file(sessions.join(format!("{}.json", std::process::id()))).unwrap();
        let record = json!({"pid": parent, "sessionId": SESSION, "cwd": CWD, "startedAt": 1,
            "procStart": agent_platform::process_start(parent).unwrap(), "peerProtocol": 1,
            "kind": "interactive", "messagingSocketPath": fixture.socket.to_string_lossy(),
            "status": "idle"});
        let path = sessions.join(format!("{parent}.json"));
        fs::write(&path, record.to_string()).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let target = fixture.target();
        assert_eq!(target.pid, parent);
        let peer = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = vec![];
            stream.read_to_end(&mut bytes).await.unwrap();
            assert!(bytes.is_empty(), "nothing was written to the wrong process");
        });
        assert_eq!(
            fixture.outcome(&submission(target, "Wrong process")).await,
            SendOutcome::Rejected
        );
        peer.await.unwrap();
    }

    #[tokio::test]
    async fn a_cancelled_wait_is_unconfirmed_and_a_cancelled_start_sends_nothing() {
        let fixture = Fixture::new();
        let listener = fixture.listen();
        let cancelled = CancellationToken::new();
        cancelled.cancel();
        assert_eq!(
            fixture
                .send(&submission(fixture.target(), "Before"), &cancelled)
                .await
                .outcome,
            SendOutcome::Rejected
        );
        let cancel = CancellationToken::new();
        let peer = {
            let cancel = cancel.clone();
            tokio::spawn(async move {
                frame_from(&listener).await;
                cancel.cancel();
            })
        };
        assert_eq!(
            fixture
                .send(&submission(fixture.target(), "During"), &cancel)
                .await
                .outcome,
            SendOutcome::Unconfirmed
        );
        peer.await.unwrap();
    }

    /// Accepts `limit` bytes, then fails, as a connection the session closed mid-write.
    struct Failing {
        limit: usize,
        taken: usize,
    }
    impl AsyncWrite for Failing {
        fn poll_write(
            mut self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
            bytes: &[u8],
        ) -> std::task::Poll<std::io::Result<usize>> {
            let room = self.limit - self.taken;
            if room == 0 {
                return std::task::Poll::Ready(Err(std::io::ErrorKind::BrokenPipe.into()));
            }
            let taken = room.min(bytes.len()).min(7);
            self.taken += taken;
            std::task::Poll::Ready(Ok(taken))
        }
        fn poll_flush(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Ready(Ok(()))
        }
        fn poll_shutdown(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn a_write_that_fails_after_the_first_byte_is_partial_never_nothing() {
        let frame = b"{\"type\":\"user\"}\n";
        assert_eq!(
            write_frame(Failing { limit: 0, taken: 0 }, frame).await,
            Written::Nothing
        );
        assert_eq!(
            write_frame(Failing { limit: 9, taken: 0 }, frame).await,
            Written::Partly
        );
        assert_eq!(
            write_frame(
                Failing {
                    limit: frame.len(),
                    taken: 0
                },
                frame
            )
            .await,
            Written::Whole
        );
    }

    /// Mluva's own transport, as shipped, against one real Claude Code session started only for
    /// this check and named by `MLUVA_REAL_CLAUDE_SESSION`; no other session is touched.
    /// `MLUVA_REAL_CLAUDE_EXPECT` is the outcome to expect: `Taken`, `Accepted` or `Rejected`.
    #[tokio::test]
    #[ignore = "delivers one message to a disposable real Claude Code session"]
    async fn a_disposable_real_session_takes_one_message() {
        let session = std::env::var("MLUVA_REAL_CLAUDE_SESSION").expect("session ID");
        let expect = std::env::var("MLUVA_REAL_CLAUDE_EXPECT").unwrap_or_else(|_| "Taken".into());
        let chosen = std::env::var("MLUVA_REAL_CLAUDE_CHOSEN").ok();
        let sessions = ClaudeRegistry::from_environment().unwrap().sessions();
        let live = merge(vec![], &sessions);
        // A choice made earlier may name a session that has since been cleared or resumed.
        let id = chosen.as_deref().unwrap_or(&session);
        let target = live
            .iter()
            .find(|t| t.session_id == session)
            .cloned()
            .map(|mut t| {
                t.session_id = id.into();
                t
            })
            .expect("the disposable session is live");
        assert_eq!(target.route(), Route::ClaudeInbox);
        let text = "Reply with the single word MLUVAQUEUED";
        let receipt = submit(&submission(target, text), &CancellationToken::new()).await;
        println!("outcome: {:?}", receipt.outcome);
        assert_eq!(format!("{:?}", receipt.outcome), expect);
        if let Some(taken) = receipt.taken {
            let seen = tokio::time::timeout(Duration::from_secs(120), taken).await;
            println!("taken later: {seen:?}");
            assert_eq!(seen, Ok(true));
        }
    }

    #[test]
    fn evidence_matches_this_message_only_and_skips_content_lines() {
        let id = "9a9a9a9a-0000-4000-8000-000000000001";
        let text = "Run the tests again";
        let line = |value: Value| value.to_string().into_bytes();
        for (record, expected) in [
            (json!({"type": "user", "uuid": id}), Some(Evidence::Taken)),
            (
                json!({"type": "queue-operation", "operation": "remove", "reason": "absorbed_mid_turn", "commandUuid": id}),
                Some(Evidence::Taken),
            ),
            (
                json!({"type": "attachment", "attachment": {"type": "queued_command", "source_uuid": id}}),
                Some(Evidence::Taken),
            ),
            (
                json!({"type": "queue-operation", "operation": "enqueue", "content": "Run the tests again\n"}),
                Some(Evidence::Queued),
            ),
            (
                json!({"type": "system", "subtype": "informational", "content": "Held peer message — preview: «Run the tests…» — not delivered"}),
                Some(Evidence::Held),
            ),
            (
                json!({"type": "system", "subtype": "informational", "content": "Held peer message — preview: «Something else» — not delivered"}),
                None,
            ),
            (
                json!({"type": "queue-operation", "operation": "enqueue", "sessionId": "other", "content": text}),
                None,
            ),
            (
                json!({"type": "queue-operation", "operation": "dequeue", "sessionId": SESSION}),
                None,
            ),
            (
                json!({"type": "user", "uuid": "someone-else", "message": {"content": id}}),
                None,
            ),
            (
                json!({"type": "assistant", "message": {"content": format!("I saw {id}")}}),
                None,
            ),
        ] {
            assert_eq!(
                evidence(&line(record.clone()), id, text, SESSION),
                expected,
                "{record}"
            );
        }
        assert_eq!(
            evidence(b"not json with \"queue-operation\"", id, text, SESSION),
            None
        );
    }
}
