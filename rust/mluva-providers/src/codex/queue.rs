//! Send to a live Codex thread if it is idle, through Codex's own message queue.
//!
//! The message is queued for the thread's UUID with a fresh client message ID, through any app
//! server on the same `CODEX_HOME`; the process that owns the thread takes it from the shared
//! store. The server that has the thread loaded takes it at once; a thread open in another
//! process, such as a `--no-daemon` TUI or Codex Desktop, takes it at that process's next check
//! of the store, so Mluva waits one full check period for it. If it leaves the queue within that
//! window and the thread's own history shows a user message with that client ID, it was sent to
//! exactly that thread. If the thread does not take it, because it is busy, waiting for the
//! person or not open, the message is withdrawn and the draft kept. If the withdrawal finds it
//! already gone, its turn is looked for once more; without one the delivery is unconfirmed.
//! Nothing is ever queued twice or left behind.
//!
//! Only `thread/loaded/list`, `thread/queue/list`, `thread/queue/add`, `thread/queue/delete`
//! and `thread/turns/list` are called. Mluva never starts, resumes, steers or interrupts a turn,
//! never starts the queue and never answers the server's own requests.
use super::{
    rpc::{AppServer, RpcError},
    server::Servers,
};
use crate::{agent_platform, agent_targets::Receipt};
use mluva_core::agent_voice::{MAX_MESSAGE_BYTES, Route, SendOutcome, Submission};
use serde_json::{Value, json};
use std::{
    os::unix::fs::MetadataExt,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio_util::sync::CancellationToken;

/// How often an app server checks Codex's shared queue store for items another process added:
/// every 10 s in Codex 0.160.1 (`watch_external_messages`, `codex-rs/ext/queue/src/service.rs`
/// at tag `rust-v0.160.1`). An item added through the server that has the thread loaded is
/// dispatched at once instead.
const EXTERNAL_QUEUE_CHECK_SECS: u64 = 10;
/// The thread is loaded in the server Mluva talks to: it takes an idle thread's message at once,
/// so one it has not taken within this window is busy.
const SAME_SERVER_WINDOW: Duration = Duration::from_secs(3);
/// The thread is open in another process: one full check period of its server, plus room for
/// its dispatch and Mluva's own polling.
const CROSS_PROCESS_WINDOW: Duration = Duration::from_secs(EXTERNAL_QUEUE_CHECK_SECS + 2);
const POLL: Duration = Duration::from_millis(250);
/// A dispatched message's turn is recorded at once; a few reads allow for a slow write.
const CONFIRM_READS: usize = 6;

struct Limits {
    /// How long to wait when the connected server has the thread loaded.
    same_server: Duration,
    /// How long to wait when another process has it open.
    cross_process: Duration,
    poll: Duration,
}

/// Send once if the thread is idle; otherwise keep the draft.
pub async fn submit(submission: &Submission, cancel: &CancellationToken) -> Receipt {
    let Some(servers) = Servers::system() else {
        return SendOutcome::Rejected.into();
    };
    let limits = Limits {
        same_server: SAME_SERVER_WINDOW,
        cross_process: CROSS_PROCESS_WINDOW,
        poll: POLL,
    };
    submit_via(submission, cancel, servers, &limits)
        .await
        .into()
}

async fn submit_via(
    submission: &Submission,
    cancel: &CancellationToken,
    servers: &Servers,
    limits: &Limits,
) -> SendOutcome {
    let target = &submission.target;
    let Some(thread) = target
        .codex
        .as_ref()
        .filter(|_| target.route() == Route::CodexQueue)
    else {
        return SendOutcome::Rejected;
    };
    if !target.can_receive()
        || submission.text.trim().is_empty()
        || submission.text.len() > MAX_MESSAGE_BYTES
        || cancel.is_cancelled()
        || !held_by(Path::new(&thread.lock), thread.holder)
    {
        return SendOutcome::Rejected;
    }
    let Some((mut server, _)) = servers.connect().await else {
        return SendOutcome::Rejected;
    };
    let id = target.session_id.as_str();
    // The queue methods exist only for experimental clients of new enough servers.
    if server
        .call("thread/queue/list", json!({"threadId": id, "limit": 1}))
        .await
        .is_err()
    {
        return SendOutcome::Rejected;
    }
    // Where the thread is loaded decides how soon an idle thread takes the message.
    let window = if loaded_here(&mut server, id).await {
        limits.same_server
    } else {
        limits.cross_process
    };
    let client = uuid_v7();
    let added = server
        .call(
            "thread/queue/add",
            json!({"threadId": id, "clientUserMessageId": client,
                "input": [{"type": "text", "text": submission.text, "text_elements": []}]}),
        )
        .await;
    let mut sending = Sending {
        servers,
        server,
        thread: id,
        client: &client,
        item: None,
    };
    match added {
        Ok(result) => {
            sending.item = result
                .pointer("/queuedSubmission/id")
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
        // Refused, such as an unknown thread or a full queue: nothing was stored.
        Err(RpcError::Server { .. }) => return SendOutcome::Rejected,
        Err(RpcError::Transport) => {}
    }
    if sending.item.is_none() {
        // The answer was lost: find the message by its client ID before deciding anything.
        return match sending.find().await {
            Some(Found::Queued(item)) => {
                sending.item = Some(item);
                sending.until_taken(cancel, window, limits.poll).await
            }
            Some(Found::Turn) => SendOutcome::Accepted,
            Some(Found::Nowhere) | None => SendOutcome::Unconfirmed,
        };
    }
    sending.until_taken(cancel, window, limits.poll).await
}

/// Whether the connected server has the thread loaded. When that cannot be learned, the longer
/// wait is assumed, so an idle thread elsewhere is never given up on too early.
async fn loaded_here(server: &mut AppServer, thread: &str) -> bool {
    server
        .call("thread/loaded/list", json!({}))
        .await
        .ok()
        .and_then(|loaded| loaded.get("data").and_then(Value::as_array).cloned())
        .is_some_and(|loaded| loaded.iter().any(|id| id.as_str() == Some(thread)))
}

/// One queued message and the connection that watches it.
struct Sending<'a> {
    servers: &'a Servers,
    server: AppServer,
    thread: &'a str,
    client: &'a str,
    item: Option<String>,
}

enum Found {
    Queued(String),
    Turn,
    Nowhere,
}

impl Sending<'_> {
    /// Wait for the thread to take the message; withdraw it if it does not.
    async fn until_taken(
        &mut self,
        cancel: &CancellationToken,
        window: Duration,
        poll: Duration,
    ) -> SendOutcome {
        let deadline = tokio::time::Instant::now() + window;
        while tokio::time::Instant::now() < deadline {
            tokio::select! {
                _ = cancel.cancelled() => return self.withdraw().await,
                _ = tokio::time::sleep(poll) => {}
            }
            match self.find().await {
                Some(Found::Queued(_)) => {}
                Some(Found::Turn) => return SendOutcome::Accepted,
                Some(Found::Nowhere) => return self.confirm().await,
                None => return self.withdraw().await,
            }
        }
        self.withdraw().await
    }

    /// Where the message is now: still queued, in a turn, or neither; `None` when the server
    /// cannot be asked.
    async fn find(&mut self) -> Option<Found> {
        let queue = self
            .ask(
                "thread/queue/list",
                json!({"threadId": self.thread, "limit": 100}),
            )
            .await?;
        let queued = queue
            .get("data")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|item| {
                item.get("clientUserMessageId").and_then(Value::as_str) == Some(self.client)
                    || item.get("id").and_then(Value::as_str) == self.item.as_deref()
            })
            .and_then(|item| item.get("id").and_then(Value::as_str));
        if let Some(item) = queued {
            return Some(Found::Queued(item.to_owned()));
        }
        Some(if self.in_a_turn().await? {
            Found::Turn
        } else {
            Found::Nowhere
        })
    }

    /// Whether the thread's latest turns hold a user message with this client ID.
    async fn in_a_turn(&mut self) -> Option<bool> {
        let turns = self
            .ask(
                "thread/turns/list",
                json!({"threadId": self.thread, "limit": 10}),
            )
            .await?;
        Some(
            turns
                .get("data")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|turn| turn.get("items").and_then(Value::as_array))
                .flatten()
                .any(|item| {
                    item.get("type").and_then(Value::as_str) == Some("userMessage")
                        && item.get("clientId").and_then(Value::as_str) == Some(self.client)
                }),
        )
    }

    /// Gone from the queue: sent if its turn appears, else unconfirmed.
    async fn confirm(&mut self) -> SendOutcome {
        for _ in 0..CONFIRM_READS {
            if self.in_a_turn().await == Some(true) {
                return SendOutcome::Accepted;
            }
            tokio::time::sleep(POLL).await;
        }
        SendOutcome::Unconfirmed
    }

    /// Take the message back. Withdrawn means the draft is kept; already gone means it was
    /// taken meanwhile, so its turn decides.
    async fn withdraw(&mut self) -> SendOutcome {
        let Some(item) = self.item.clone() else {
            return SendOutcome::Unconfirmed;
        };
        let deleted = self
            .ask(
                "thread/queue/delete",
                json!({"threadId": self.thread, "queuedSubmissionId": item}),
            )
            .await;
        match deleted.and_then(|result| result.get("deleted").and_then(Value::as_bool)) {
            Some(true) => SendOutcome::Busy,
            Some(false) => self.confirm().await,
            // It may still be queued and fire later; that is never claimed as kept.
            None => SendOutcome::Unconfirmed,
        }
    }

    /// One call; a broken connection is replaced once.
    async fn ask(&mut self, method: &str, params: Value) -> Option<Value> {
        match self.server.call(method, params.clone()).await {
            Ok(result) => Some(result),
            Err(RpcError::Server { .. }) => None,
            Err(RpcError::Transport) => {
                self.server = self.servers.connect().await?.0;
                self.server.call(method, params).await.ok()
            }
        }
    }
}

/// Whether the thread's lock is still held by the same process.
fn held_by(lock: &Path, holder: u32) -> bool {
    std::fs::symlink_metadata(lock).is_ok_and(|metadata| {
        metadata.file_type().is_file()
            && metadata.uid() == agent_platform::effective_uid()
            && agent_platform::flock_holder(&agent_platform::flock_holders(), lock, &metadata)
                == Some(holder)
    })
}

/// A fresh time-ordered UUID (version 7) for the message's client ID.
fn uuid_v7() -> String {
    let milliseconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let mut bytes = *uuid::Uuid::new_v4().as_bytes();
    bytes[..6].copy_from_slice(&milliseconds.to_be_bytes()[2..]);
    bytes[6] = (bytes[6] & 0x0f) | 0x70;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    uuid::Uuid::from_bytes(bytes).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codex::test_peer::{Peer, Scenario, THREAD};
    use mluva_core::agent_voice::{AgentKind, AgentStatus, CodexThread, Target};
    use std::os::unix::{fs::PermissionsExt, io::AsRawFd};

    /// A Codex home whose thread lock this test holds, as a Codex process holds it.
    struct Live {
        temp: tempfile::TempDir,
        _lock: std::fs::File,
    }

    fn live() -> Live {
        let temp = tempfile::tempdir().unwrap();
        let locks = temp.path().join("thread-writer-locks");
        std::fs::create_dir(&locks).unwrap();
        std::fs::set_permissions(&locks, std::fs::Permissions::from_mode(0o700)).unwrap();
        let lock = std::fs::File::create(locks.join(format!("{THREAD}.lock"))).unwrap();
        // SAFETY: a valid open descriptor and a lock operation.
        assert_eq!(unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) }, 0);
        Live { temp, _lock: lock }
    }

    fn submission(home: &Path, holder: u32) -> Submission {
        Submission {
            id: uuid::Uuid::new_v4().to_string(),
            text: "Synthetic Czech: spusť testy znovu.".into(),
            target: Target {
                kind: AgentKind::CodexCli,
                name: "codex-01900000".into(),
                status: AgentStatus::Unknown,
                session_id: THREAD.into(),
                pid: holder,
                codex: Some(CodexThread {
                    lock: home
                        .join(format!("thread-writer-locks/{THREAD}.lock"))
                        .to_string_lossy()
                        .into_owned(),
                    holder,
                }),
                ..Target::default()
            },
        }
    }

    /// Codex's windows at a tenth of their length, matching the peer's scaled check period.
    const QUICK: Limits = Limits {
        same_server: Duration::from_millis(300),
        cross_process: Duration::from_millis(1_200),
        poll: Duration::from_millis(50),
    };

    async fn send(scenario: Scenario) -> (SendOutcome, Peer) {
        let home = live();
        let peer = Peer::start(scenario);
        let servers = Servers::fixed(home.temp.path().into(), peer.socket.clone());
        let sending = submission(home.temp.path(), std::process::id());
        let outcome = submit_via(&sending, &CancellationToken::new(), &servers, &QUICK).await;
        (outcome, peer)
    }

    /// No method that starts, steers, resumes or interrupts work, and no answer to the server.
    fn only_queue_methods(peer: &Peer) {
        for method in peer.methods() {
            assert!(
                [
                    "initialize",
                    "initialized",
                    "thread/loaded/list",
                    "thread/queue/list",
                    "thread/queue/add",
                    "thread/queue/delete",
                    "thread/turns/list",
                ]
                .contains(&method.as_str()),
                "{method}"
            );
        }
        assert_eq!(peer.answers_to_server(), 0);
    }

    fn adds(peer: &Peer) -> usize {
        peer.methods()
            .iter()
            .filter(|m| *m == "thread/queue/add")
            .count()
    }

    #[tokio::test]
    async fn an_idle_thread_takes_it_and_its_own_turn_proves_it_was_sent() {
        let (outcome, peer) = send(Scenario::Idle).await;
        assert_eq!(outcome, SendOutcome::Accepted);
        assert_eq!(adds(&peer), 1);
        only_queue_methods(&peer);
        let state = peer.state.lock().unwrap();
        let add = state
            .received
            .iter()
            .find(|m| m["method"] == "thread/queue/add")
            .unwrap();
        assert_eq!(add["params"]["threadId"], THREAD);
        assert_eq!(
            add["params"]["input"],
            json!([{"type": "text", "text": "Synthetic Czech: spusť testy znovu.", "text_elements": []}])
        );
        let client = add["params"]["clientUserMessageId"].as_str().unwrap();
        assert_eq!(uuid::Uuid::parse_str(client).unwrap().get_version_num(), 7);
        let init = &state.received[0];
        assert_eq!(init["method"], "initialize");
        assert_eq!(init["params"]["capabilities"]["experimentalApi"], true);
        assert!(state.queue.is_empty());
    }

    #[tokio::test]
    async fn a_busy_or_closed_thread_keeps_the_draft_and_nothing_stays_queued() {
        let (outcome, peer) = send(Scenario::Busy).await;
        assert_eq!(outcome, SendOutcome::Busy);
        assert_eq!(adds(&peer), 1);
        assert!(peer.methods().contains(&"thread/queue/delete".into()));
        assert!(
            peer.state.lock().unwrap().queue.is_empty(),
            "never left behind"
        );
        only_queue_methods(&peer);
    }

    #[tokio::test]
    async fn a_message_taken_as_it_is_withdrawn_is_confirmed_by_its_turn() {
        let (outcome, peer) = send(Scenario::TakenDuringDelete).await;
        assert_eq!(outcome, SendOutcome::Accepted);
        assert_eq!(adds(&peer), 1);
        only_queue_methods(&peer);
    }

    #[tokio::test]
    async fn a_server_without_the_queue_or_an_unknown_thread_stores_nothing() {
        let (outcome, peer) = send(Scenario::Unsupported).await;
        assert_eq!(outcome, SendOutcome::Rejected);
        assert_eq!(adds(&peer), 0);
        let (outcome, peer) = send(Scenario::UnknownThread).await;
        assert_eq!(outcome, SendOutcome::Rejected);
        assert!(peer.state.lock().unwrap().queue.is_empty());
        only_queue_methods(&peer);
    }

    #[tokio::test]
    async fn a_lost_answer_is_settled_by_the_client_id_and_never_queued_again() {
        let (outcome, peer) = send(Scenario::AddAnswerLost).await;
        assert_eq!(outcome, SendOutcome::Accepted);
        assert_eq!(adds(&peer), 1, "the add is never repeated");
        assert!(peer.state.lock().unwrap().connections >= 2);
        only_queue_methods(&peer);
    }

    #[tokio::test]
    async fn a_thread_no_longer_open_or_held_by_another_process_is_refused_before_connecting() {
        let peer = Peer::start(Scenario::Idle);
        // The lock file outlived its owner: present, but nobody holds it.
        let stale = tempfile::tempdir().unwrap();
        let locks = stale.path().join("thread-writer-locks");
        std::fs::create_dir(&locks).unwrap();
        std::fs::File::create(locks.join(format!("{THREAD}.lock"))).unwrap();
        let servers = Servers::fixed(stale.path().into(), peer.socket.clone());
        let sending = submission(stale.path(), std::process::id());
        assert_eq!(
            submit_via(&sending, &CancellationToken::new(), &servers, &QUICK).await,
            SendOutcome::Rejected
        );
        // Held, but by another process than the one chosen.
        let home = live();
        let servers = Servers::fixed(home.temp.path().into(), peer.socket.clone());
        let sending = submission(home.temp.path(), std::process::id() + 1);
        assert_eq!(
            submit_via(&sending, &CancellationToken::new(), &servers, &QUICK).await,
            SendOutcome::Rejected
        );
        assert_eq!(peer.state.lock().unwrap().connections, 0);
    }

    async fn timed(scenario: Scenario, limits: &Limits) -> (SendOutcome, Duration, Peer) {
        let home = live();
        let peer = Peer::start(scenario);
        let servers = Servers::fixed(home.temp.path().into(), peer.socket.clone());
        let sending = submission(home.temp.path(), std::process::id());
        let started = tokio::time::Instant::now();
        let outcome = submit_via(&sending, &CancellationToken::new(), &servers, limits).await;
        (outcome, started.elapsed(), peer)
    }

    /// A thread open in another process takes the message only at that process's next check of
    /// Codex's queue store. The window covers a full check period, so an idle thread is sent to;
    /// the earlier same-server window alone withdrew it as busy.
    #[tokio::test]
    async fn a_thread_open_in_another_process_gets_a_full_check_period_to_take_it() {
        let (outcome, elapsed, peer) = timed(Scenario::ExternalIdle, &QUICK).await;
        assert_eq!(outcome, SendOutcome::Accepted);
        assert!(
            elapsed > QUICK.same_server,
            "taken only at the next check: {elapsed:?}"
        );
        assert_eq!(adds(&peer), 1);
        assert!(peer.methods().contains(&"thread/loaded/list".into()));
        only_queue_methods(&peer);
        let short_only = Limits {
            cross_process: QUICK.same_server,
            ..QUICK
        };
        let (outcome, _, peer) = timed(Scenario::ExternalIdle, &short_only).await;
        assert_eq!(
            outcome,
            SendOutcome::Busy,
            "the old 3 s window gives up too early"
        );
        assert!(peer.state.lock().unwrap().queue.is_empty());
    }

    /// Where the thread is loaded decides the wait: a loaded busy thread is given up on after the
    /// short window, one in another process only after a full check period.
    #[tokio::test]
    async fn the_short_window_stays_for_threads_the_connected_server_has_loaded() {
        let (outcome, elapsed, _) = timed(Scenario::Idle, &QUICK).await;
        assert_eq!(outcome, SendOutcome::Accepted);
        assert!(
            elapsed < QUICK.same_server,
            "same server takes it at once: {elapsed:?}"
        );
        let (outcome, elapsed, _) = timed(Scenario::Busy, &QUICK).await;
        assert_eq!(outcome, SendOutcome::Busy);
        assert!(elapsed < QUICK.cross_process, "{elapsed:?}");
        let (outcome, elapsed, peer) = timed(Scenario::ExternalBusy, &QUICK).await;
        assert_eq!(outcome, SendOutcome::Busy);
        assert!(elapsed >= QUICK.cross_process, "{elapsed:?}");
        assert!(
            peer.state.lock().unwrap().queue.is_empty(),
            "never left behind"
        );
    }

    /// Mluva's own discovery and Send-if-idle transport, with production limits, against a real
    /// disposable Codex TUI in the private `CODEX_HOME` named by `MLUVA_REAL_CODEX_HOME`, through
    /// Mluva's own app server. Prints each pickup time.
    #[tokio::test]
    #[ignore = "sends to a disposable real Codex TUI in a private CODEX_HOME"]
    async fn a_disposable_real_codex_thread_takes_each_message() {
        let home = std::path::PathBuf::from(std::env::var("MLUVA_REAL_CODEX_HOME").unwrap());
        let runtime = std::path::PathBuf::from(std::env::var("MLUVA_REAL_CODEX_RUNTIME").unwrap());
        let rounds: usize = std::env::var("MLUVA_REAL_CODEX_ROUNDS")
            .ok()
            .and_then(|n| n.parse().ok())
            .unwrap_or(3);
        let servers = Servers::new(home, Some(runtime), agent_platform::codex_executable());
        let mut threads = crate::codex::threads::CodexThreads::default();
        let target = threads
            .discover(&servers)
            .await
            .into_iter()
            .next()
            .expect("the disposable thread is live");
        let limits = Limits {
            same_server: SAME_SERVER_WINDOW,
            cross_process: CROSS_PROCESS_WINDOW,
            poll: POLL,
        };
        for round in 0..rounds {
            let sending = Submission {
                id: uuid::Uuid::new_v4().to_string(),
                target: target.clone(),
                text: format!("MLUVA-PROBE-{round} reply with the word OK"),
            };
            let started = tokio::time::Instant::now();
            let outcome = submit_via(&sending, &CancellationToken::new(), &servers, &limits).await;
            println!(
                "round {round}: {outcome:?} after {:.2} s",
                started.elapsed().as_secs_f64()
            );
            assert_eq!(outcome, SendOutcome::Accepted);
            // Let the loopback turn finish before the next message.
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        servers.shutdown().await;
    }

    #[test]
    fn the_cross_process_window_covers_codexs_check_period() {
        assert_eq!(EXTERNAL_QUEUE_CHECK_SECS, 10);
        assert!(CROSS_PROCESS_WINDOW > Duration::from_secs(EXTERNAL_QUEUE_CHECK_SECS));
        assert!(SAME_SERVER_WINDOW < Duration::from_secs(EXTERNAL_QUEUE_CHECK_SECS));
    }

    #[tokio::test]
    async fn closing_mid_wait_withdraws_the_message() {
        let home = live();
        let peer = Peer::start(Scenario::Busy);
        let servers = Servers::fixed(home.temp.path().into(), peer.socket.clone());
        let sending = submission(home.temp.path(), std::process::id());
        let cancel = CancellationToken::new();
        let closing = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            closing.cancel();
        });
        let slow = Limits {
            same_server: Duration::from_secs(30),
            cross_process: Duration::from_secs(30),
            poll: Duration::from_millis(50),
        };
        assert_eq!(
            submit_via(&sending, &cancel, &servers, &slow).await,
            SendOutcome::Busy
        );
        assert!(peer.state.lock().unwrap().queue.is_empty());
    }
}
