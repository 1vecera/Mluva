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
//! and must still name the chosen process, session and socket; the socket path must be private
//! to this account, and the connected peer must be that process. The frame carries the session
//! ID, which Claude Code checks as well, so a `/clear` in between drops it. The socket answers
//! nothing, so acceptance is read from the session's own transcript: the queue's record of
//! exactly this text, or a record carrying this message's ID. Without one the delivery is
//! unconfirmed and is never written again. Only lines that can be such a record are parsed, they
//! are compared in memory and nothing from them is kept or logged.
use crate::claude_sessions::{self, MAX_SOCKET_PATH};
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
/// this long refused, held or dropped it, or has not read it yet.
const RECEIPT_WINDOW: Duration = Duration::from_millis(2_500);
const RECEIPT_POLL: Duration = Duration::from_millis(50);
/// Transcript bytes read per poll; a busy session's tool output is read in later polls.
const MAX_RECEIPT_READ: u64 = 1_048_576;
/// A receipt line holds at most the message and its metadata; anything longer is content.
const MAX_RECEIPT_LINE: usize = 4 * MAX_MESSAGE_BYTES + 16_384;

/// Queue a message once into the session's own inbox, then read its receipt.
pub async fn submit(submission: &Submission, cancel: &CancellationToken) -> SendOutcome {
    submit_within(submission, cancel, RECEIPT_WINDOW).await
}

async fn submit_within(
    submission: &Submission,
    cancel: &CancellationToken,
    window: Duration,
) -> SendOutcome {
    let target = &submission.target;
    let Some(inbox) = target
        .inbox
        .as_ref()
        .filter(|_| target.route() == Route::ClaudeInbox)
    else {
        return SendOutcome::Rejected;
    };
    if !target.can_receive()
        || submission.text.trim().is_empty()
        || submission.text.len() > MAX_MESSAGE_BYTES
        || cancel.is_cancelled()
    {
        return SendOutcome::Rejected;
    }
    // Everything is checked again now; anything that changed refuses rather than retargets.
    let Some((config, cwd)) = recheck(inbox, target) else {
        return SendOutcome::Rejected;
    };
    let mut transcript = Transcript::new(config, cwd, target.session_id.clone());
    let Some(stream) = connect(inbox, target.pid).await else {
        return SendOutcome::Rejected;
    };
    match write_frame(stream, &frame(submission)).await {
        Written::Nothing => return SendOutcome::Rejected,
        Written::Partly => return SendOutcome::Unconfirmed,
        Written::Whole => {}
    }
    transcript.receipt(submission, cancel, window).await
}

/// The session's registry entry, read again: still this process, this session and this inbox,
/// at a socket only this account can reach. Returns the configuration directory and the
/// session's working directory, which locate its transcript.
fn recheck(inbox: &ClaudeInbox, target: &Target) -> Option<(PathBuf, String)> {
    let entry = Path::new(&inbox.entry);
    let sessions = entry.parent()?;
    if entry.file_name()?.to_str()? != format!("{}.json", target.pid)
        || !claude_sessions::private_directory(sessions)
    {
        return None;
    }
    let started = claude_sessions::process_start(target.pid)?;
    let namespace = claude_sessions::own_namespace();
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
    (same && trusted_socket(Path::new(&inbox.socket)))
        .then(|| (sessions.parent().map(Path::to_path_buf), session.cwd))
        .and_then(|(config, cwd)| Some((config?, cwd)))
}

/// A socket owned by this account, not reached through a symlink, in a directory only this
/// account can enter, at a path short enough to be bound whole.
fn trusted_socket(path: &Path) -> bool {
    let uid = claude_sessions::effective_uid();
    let directory = path
        .parent()
        .and_then(|parent| fs::symlink_metadata(parent).ok());
    path.is_absolute()
        && path.as_os_str().len() <= MAX_SOCKET_PATH
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
    let peer = stream.peer_cred().ok()?;
    let pid = i32::try_from(pid).ok()?;
    (peer.uid() == claude_sessions::effective_uid() && peer.pid() == Some(pid)).then_some(stream)
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

/// The new end of one session's transcript, read for this message's receipt.
struct Transcript {
    config: PathBuf,
    cwd: String,
    session_id: String,
    path: Option<PathBuf>,
    /// Bytes before the write are history, never a receipt.
    offset: u64,
    partial: Vec<u8>,
    /// Inside a line too long to be a receipt.
    skipping: bool,
}

impl Transcript {
    fn new(config: PathBuf, cwd: String, session_id: String) -> Self {
        let path = claude_sessions::transcript_path(&config, &cwd, &session_id);
        let offset = path
            .as_deref()
            .and_then(claude_sessions::open_private)
            .map_or(0, |(_, metadata)| metadata.len());
        Self {
            config,
            cwd,
            session_id,
            path,
            offset,
            partial: vec![],
            skipping: false,
        }
    }

    /// Accepted once a receipt appears; unconfirmed when none does within the window or the
    /// wait is cancelled. Nothing is ever written again.
    async fn receipt(
        &mut self,
        submission: &Submission,
        cancel: &CancellationToken,
        window: Duration,
    ) -> SendOutcome {
        let deadline = tokio::time::Instant::now() + window;
        loop {
            if self.read(submission) {
                return SendOutcome::Accepted;
            }
            if tokio::time::Instant::now() >= deadline {
                return SendOutcome::Unconfirmed;
            }
            tokio::select! {
                _ = cancel.cancelled() => return SendOutcome::Unconfirmed,
                _ = tokio::time::sleep(RECEIPT_POLL) => {}
            }
        }
    }

    /// Read what was appended since the last read; true when it holds this message's receipt.
    fn read(&mut self, submission: &Submission) -> bool {
        if self.path.is_none() {
            // A session's first message creates its transcript.
            self.path = claude_sessions::transcript_path(&self.config, &self.cwd, &self.session_id);
        }
        let Some((mut file, metadata)) =
            self.path.as_deref().and_then(claude_sessions::open_private)
        else {
            return false;
        };
        let length = metadata.len();
        if length < self.offset {
            // Rewritten: start again from its beginning.
            (self.offset, self.skipping) = (0, false);
            self.partial.clear();
        }
        if length == self.offset || file.seek(SeekFrom::Start(self.offset)).is_err() {
            return false;
        }
        let mut bytes = Vec::new();
        if (&mut file)
            .take((length - self.offset).min(MAX_RECEIPT_READ))
            .read_to_end(&mut bytes)
            .is_err()
        {
            return false;
        }
        self.offset += bytes.len() as u64;
        for byte in bytes {
            if byte != b'\n' {
                if !self.skipping {
                    self.partial.push(byte);
                    self.skipping = self.partial.len() > MAX_RECEIPT_LINE;
                }
                continue;
            }
            let line = std::mem::take(&mut self.partial);
            let whole = !std::mem::replace(&mut self.skipping, false);
            if whole && receipt_line(&line, submission) {
                return true;
            }
        }
        if self.skipping {
            self.partial.clear();
        }
        false
    }
}

/// Whether one transcript line records this message: the queue admitting exactly this text for
/// this session, or the turn, queue or attachment record carrying this message's ID.
fn receipt_line(line: &[u8], submission: &Submission) -> bool {
    let has = |mark: &[u8]| line.windows(mark.len()).any(|w| w == mark);
    if !line.starts_with(b"{") || !(has(b"\"queue-operation\"") || has(submission.id.as_bytes())) {
        return false;
    }
    let Ok(record) = serde_json::from_slice::<Value>(line) else {
        return false;
    };
    let field = |key: &str| record.get(key).and_then(Value::as_str);
    let ours = Some(submission.id.as_str());
    match field("type") {
        Some("queue-operation") => {
            let admitted = field("operation") == Some("enqueue")
                && field("content").is_some_and(|content| content.trim() == submission.text.trim())
                && field("sessionId").is_none_or(|id| id == submission.target.session_id);
            admitted || field("commandUuid") == ours
        }
        Some("user") => field("uuid") == ours,
        Some("attachment") => {
            record
                .pointer("/attachment/source_uuid")
                .and_then(Value::as_str)
                == ours
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude_sessions::{ClaudeRegistry, merge};
    use mluva_core::agent_voice::{AgentStatus, Delivery};
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
                "startedAt": 1_793_000_000_000_u64, "procStart": claude_sessions::process_start(pid).unwrap(),
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
        fn append(&self, line: Value) {
            let mut file = fs::OpenOptions::new()
                .append(true)
                .open(&self.transcript)
                .unwrap();
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
            (
                mluva_core::agent_voice::AgentKind::ClaudeDesktop,
                Route::ClaudeInbox,
                "app"
            )
        );
        // A waiting session is shown blocked: a socket message never answers its prompt.
        fixture.entry(json!({"status": "waiting", "waitingFor": "permission prompt"}));
        let waiting = fixture.target();
        assert!(!waiting.can_receive() && !waiting.draft_only());
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
            let mut file = fs::OpenOptions::new()
                .append(true)
                .open(&transcript)
                .unwrap();
            for line in [
                json!({"type": "queue-operation", "operation": "dequeue", "sessionId": SESSION}),
                json!({"type": "user", "uuid": id, "isMeta": true, "origin": {"kind": "peer"}}),
            ] {
                std::io::Write::write_all(&mut file, format!("{line}\n").as_bytes()).unwrap();
            }
            no_more_connections(&listener).await;
        });
        assert_eq!(
            submit(&sending, &CancellationToken::new()).await,
            SendOutcome::Accepted
        );
        peer.await.unwrap();
    }

    #[tokio::test]
    async fn a_working_session_admitting_exactly_this_text_to_its_queue_is_accepted() {
        let fixture = Fixture::new();
        let listener = fixture.listen();
        let target = Target {
            status: AgentStatus::Working,
            ..fixture.target()
        };
        let sending = submission(target, "Run the tests again");
        // An older admission of the same text, before this write, is history.
        fixture.append(json!({"type": "queue-operation", "operation": "enqueue", "sessionId": SESSION, "content": "Run the tests again"}));
        let transcript = fixture.transcript.clone();
        let peer = tokio::spawn(async move {
            frame_from(&listener).await;
            let mut file = fs::OpenOptions::new()
                .append(true)
                .open(&transcript)
                .unwrap();
            // Another session's admission and other text are not this message's receipt.
            for line in [
                json!({"type": "queue-operation", "operation": "enqueue", "sessionId": "other-session", "content": "Run the tests again"}),
                json!({"type": "queue-operation", "operation": "enqueue", "sessionId": SESSION, "content": "Different text"}),
                json!({"type": "queue-operation", "operation": "enqueue", "sessionId": SESSION, "content": "Run the tests again"}),
            ] {
                std::io::Write::write_all(&mut file, format!("{line}\n").as_bytes()).unwrap();
            }
        });
        assert_eq!(
            submit(&sending, &CancellationToken::new()).await,
            SendOutcome::Accepted
        );
        peer.await.unwrap();
    }

    #[tokio::test]
    async fn no_receipt_leaves_delivery_unconfirmed_and_never_writes_again() {
        let fixture = Fixture::new();
        let listener = fixture.listen();
        let sending = submission(fixture.target(), "Held or refused message");
        let transcript = fixture.transcript.clone();
        let peer = tokio::spawn(async move {
            frame_from(&listener).await;
            // A session that holds messages from other sessions shows only a notice.
            let mut file = fs::OpenOptions::new()
                .append(true)
                .open(&transcript)
                .unwrap();
            let held = json!({"type": "system", "subtype": "informational", "level": "warning",
                "content": "Held peer message — from an unidentified session; preview: «Held or refused message» — not delivered to Claude (1 held)."});
            std::io::Write::write_all(&mut file, format!("{held}\n").as_bytes()).unwrap();
            no_more_connections(&listener).await;
        });
        assert_eq!(
            submit_within(
                &sending,
                &CancellationToken::new(),
                Duration::from_millis(400)
            )
            .await,
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
                submit(&sending, &CancellationToken::new()).await,
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
            submit(
                &submission(chosen.clone(), "Never delivered"),
                &CancellationToken::new()
            )
            .await,
            SendOutcome::Rejected
        );
        // A blocked or missing target is refused before anything is read.
        fixture.entry(json!({}));
        for target in [
            Target {
                status: AgentStatus::Blocked,
                ..chosen.clone()
            },
            Target {
                missing: true,
                ..chosen.clone()
            },
        ] {
            assert_eq!(
                submit(
                    &submission(target, "Never delivered"),
                    &CancellationToken::new()
                )
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
            submit(
                &submission(target.clone(), "No listener"),
                &CancellationToken::new()
            )
            .await,
            SendOutcome::Rejected
        );
        let listener = fixture.listen();
        // A socket directory others can enter, or a symlink in the socket's place.
        let socks = fixture.socket.parent().unwrap();
        fs::set_permissions(socks, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            submit(
                &submission(target.clone(), "Shared directory"),
                &CancellationToken::new()
            )
            .await,
            SendOutcome::Rejected
        );
        fs::set_permissions(socks, fs::Permissions::from_mode(0o700)).unwrap();
        let real = socks.join("real.sock");
        fs::rename(&fixture.socket, &real).unwrap();
        std::os::unix::fs::symlink(&real, &fixture.socket).unwrap();
        assert_eq!(
            submit(
                &submission(target, "Symlinked socket"),
                &CancellationToken::new()
            )
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
        let parent = claude_sessions::parent(std::process::id()).unwrap();
        let entry = fixture.config.join("sessions");
        fs::remove_file(entry.join(format!("{}.json", std::process::id()))).unwrap();
        let record = json!({"pid": parent, "sessionId": SESSION, "cwd": CWD, "startedAt": 1,
            "procStart": claude_sessions::process_start(parent).unwrap(), "peerProtocol": 1,
            "kind": "interactive", "messagingSocketPath": fixture.socket.to_string_lossy(),
            "status": "idle"});
        let path = entry.join(format!("{parent}.json"));
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
            submit(
                &submission(target, "Wrong process"),
                &CancellationToken::new()
            )
            .await,
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
            submit(&submission(fixture.target(), "Before"), &cancelled).await,
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
            submit(&submission(fixture.target(), "During"), &cancel).await,
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

    #[test]
    fn receipts_match_this_message_only_and_skip_content_lines() {
        let target = Target {
            session_id: SESSION.into(),
            ..Target::default()
        };
        let sending = submission(target, "Run the tests again");
        let id = sending.id.as_str();
        let line = |value: Value| value.to_string().into_bytes();
        for (record, receipt) in [
            (json!({"type": "user", "uuid": id}), true),
            (
                json!({"type": "queue-operation", "operation": "remove", "reason": "absorbed_mid_turn", "commandUuid": id}),
                true,
            ),
            (
                json!({"type": "attachment", "attachment": {"type": "queued_command", "source_uuid": id}}),
                true,
            ),
            (
                json!({"type": "queue-operation", "operation": "enqueue", "content": "Run the tests again\n"}),
                true,
            ),
            (
                json!({"type": "queue-operation", "operation": "dequeue", "sessionId": SESSION}),
                false,
            ),
            (
                json!({"type": "user", "uuid": "someone-else", "message": {"content": id}}),
                false,
            ),
            (
                json!({"type": "assistant", "message": {"content": format!("I saw {id}")}}),
                false,
            ),
        ] {
            assert_eq!(
                receipt_line(&line(record.clone()), &sending),
                receipt,
                "{record}"
            );
        }
        assert!(!receipt_line(
            b"not json with \"queue-operation\"",
            &sending
        ));
    }
}
