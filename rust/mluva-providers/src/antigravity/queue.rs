//! Queue transport to a live Antigravity CLI session through its file-backed message store.
use super::titles::antigravity_data_dir;
use crate::{
    agent_platform,
    agent_targets::{Receipt, admissible},
};
use mluva_core::agent_voice::{Route, SendOutcome, Submission};
use serde_json::json;
use std::{
    os::unix::fs::MetadataExt,
    path::Path,
    time::Duration,
};
use tokio_util::sync::CancellationToken;

const TAKEN_WATCH: Duration = Duration::from_secs(20 * 60);
const TAKEN_POLL: Duration = Duration::from_millis(250);

/// Queue a message into Antigravity's per-conversation message store.
pub async fn submit(submission: &Submission, cancel: &CancellationToken) -> Receipt {
    let target = &submission.target;
    let Some(session) = target
        .antigravity
        .as_ref()
        .filter(|_| target.route() == Route::AntigravityQueue)
    else {
        return SendOutcome::Rejected.into();
    };

    if !admissible(submission, cancel) {
        return SendOutcome::Rejected.into();
    }

    // Immediately before writing, re-check that the exact process still holds the lock.
    let lock_path = Path::new(&session.lock);
    if !recheck_lock(lock_path, session.holder, &session.proc_start) {
        return SendOutcome::Rejected.into();
    }

    let Some(data_dir) = antigravity_data_dir() else {
        return SendOutcome::Rejected.into();
    };
    let brain_dir = data_dir.join("brain").join(&session.conversation_id);
    if !brain_dir.is_dir() {
        return SendOutcome::Rejected.into();
    }

    let messages_dir = brain_dir.join(".system_generated/messages");
    let undelivered_dir = messages_dir.join("undelivered");
    if std::fs::create_dir_all(&undelivered_dir).is_err() {
        return SendOutcome::Rejected.into();
    }

    let msg_id = uuid::Uuid::now_v7().to_string();
    let msg_file = messages_dir.join(format!("{msg_id}.json"));
    let undelivered_file = undelivered_dir.join(&msg_id);

    let now_str = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true);
    let payload = json!({
        "id": msg_id,
        "recipient": session.conversation_id,
        "sender": "mluva",
        "priority": "MESSAGE_PRIORITY_HIGH",
        "timestamp": now_str,
        "renderDetails": {
            "messageTitle": "Message from Mluva (Voice Dictation)"
        },
        "content": submission.text,
    });

    let bytes = match serde_json::to_vec(&payload) {
        Ok(b) => b,
        Err(_) => return SendOutcome::Rejected.into(),
    };

    // Atomic write to avoid partial JSON visible to a concurrent poll.
    let temp_file = messages_dir.join(format!(".tmp.{msg_id}"));
    if std::fs::write(&temp_file, bytes).is_err() {
        return SendOutcome::Rejected.into();
    }
    if std::fs::rename(&temp_file, &msg_file).is_err() {
        let _ = std::fs::remove_file(&temp_file);
        return SendOutcome::Rejected.into();
    }

    // Touch the undelivered notice. If this fails after the JSON was written, the delivery
    // is unconfirmed and never retried.
    if std::fs::write(&undelivered_file, b"").is_err() {
        return SendOutcome::Unconfirmed.into();
    }

    let cancel_token = cancel.clone();
    Receipt {
        outcome: SendOutcome::Accepted,
        taken: Some(Box::pin(async move {
            watch_taken(&undelivered_file, &cancel_token, TAKEN_WATCH).await
        })),
    }
}

/// Whether the lock file is still held by the target process with the same start token.
fn recheck_lock(lock: &Path, holder: u32, proc_start: &str) -> bool {
    let Ok(metadata) = std::fs::symlink_metadata(lock) else {
        return false;
    };
    let uid = agent_platform::effective_uid();
    if !metadata.file_type().is_file() || metadata.uid() != uid {
        return false;
    }
    if !proc_start.is_empty() && agent_platform::process_start(holder).as_deref() != Some(proc_start) {
        return false;
    }
    let holders = agent_platform::flock_holders();
    let mount = agent_platform::mount_device(lock.parent().unwrap_or(lock));
    agent_platform::flock_holder_on(&holders, mount, lock, &metadata) == Some(holder)
}

/// Watch for Antigravity CLI taking the message into a turn (which unlinks `undelivered/<id>`).
async fn watch_taken(undelivered: &Path, cancel: &CancellationToken, limit: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + limit;
    while tokio::time::Instant::now() < deadline {
        tokio::select! {
            _ = cancel.cancelled() => return false,
            _ = tokio::time::sleep(TAKEN_POLL) => {}
        }
        if !undelivered.exists() {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use mluva_core::agent_voice::{AgentKind, AgentStatus, AntigravitySession, Target};
    use std::os::unix::io::AsRawFd;

    const CONV_ID: &str = "0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b";

    #[tokio::test]
    async fn submits_message_and_confirms_when_taken() {
        let temp = tempfile::tempdir().unwrap();
        let conv_dir = temp.path().join("brain").join(CONV_ID);
        std::fs::create_dir_all(&conv_dir).unwrap();

        let lock_file = temp.path().join(format!("{CONV_ID}.lock"));
        let lock = std::fs::File::create(&lock_file).unwrap();
        assert_eq!(unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) }, 0);

        let target = Target {
            kind: AgentKind::AntigravityCli,
            session_id: CONV_ID.into(),
            status: AgentStatus::Idle,
            pid: std::process::id(),
            started: agent_platform::process_start(std::process::id()).unwrap_or_default(),
            antigravity: Some(AntigravitySession {
                conversation_id: CONV_ID.into(),
                lock: lock_file.to_string_lossy().into_owned(),
                holder: std::process::id(),
                proc_start: agent_platform::process_start(std::process::id()).unwrap_or_default(),
            }),
            ..Target::default()
        };

        let submission = Submission {
            id: "msg-1".into(),
            target,
            text: "test message".into(),
        };

        // Temporarily set ANTIGRAVITY_APP_DATA_DIR
        unsafe {
            std::env::set_var("ANTIGRAVITY_APP_DATA_DIR", temp.path());
        }

        let cancel = CancellationToken::new();
        let receipt = submit(&submission, &cancel).await;
        assert_eq!(receipt.outcome, SendOutcome::Accepted);
        assert!(receipt.taken.is_some());

        // Check file was created
        let messages_dir = conv_dir.join(".system_generated/messages");
        let entries: Vec<_> = std::fs::read_dir(&messages_dir)
            .unwrap()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
            .collect();
        assert_eq!(entries.len(), 1);

        let undelivered_dir = messages_dir.join("undelivered");
        let undelivered_entries: Vec<_> = std::fs::read_dir(&undelivered_dir)
            .unwrap()
            .flatten()
            .collect();
        assert_eq!(undelivered_entries.len(), 1);

        // Simulate agent taking message by unlinking undelivered file
        std::fs::remove_file(undelivered_entries[0].path()).unwrap();

        // Run taken watcher
        let taken = receipt.taken.unwrap().await;
        assert!(taken);

        unsafe {
            std::env::remove_var("ANTIGRAVITY_APP_DATA_DIR");
        }
    }
}
