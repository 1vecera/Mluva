//! Local Herdr metadata and guarded prompt transport. No terminal reads, clipboard or focus.
use crate::{ProviderError, Result};
use mluva_core::agent_voice::{AgentStatus, Recipient, SendOutcome, Submission};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
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
    pub recipients: Vec<Recipient>,
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
    // SAFETY: geteuid has no arguments or memory preconditions.
    let uid = unsafe { libc::geteuid() };
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
            != unsafe { libc::geteuid() }
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
struct Agent {
    pane_id: String,
    name: Option<String>,
    agent: Option<String>,
    #[serde(default)]
    agent_status: AgentStatus,
    cwd: Option<String>,
    foreground_cwd: Option<String>,
}
fn label(value: &str, max: usize) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .take(max)
        .collect()
}
fn recipient(path: &Path, epoch: &str, guard: &str, value: Value) -> Option<Recipient> {
    let agent: Agent = serde_json::from_value(value).ok()?;
    let kind = agent.agent?;
    if !matches!(kind.as_str(), "claude" | "claude_code" | "codex") {
        return None;
    }
    let project = agent.foreground_cwd.or(agent.cwd).unwrap_or_default();
    Some(Recipient {
        socket: path.to_string_lossy().into_owned(),
        server_epoch: epoch.into(),
        pane_id: agent.pane_id.clone(),
        guard: guard.into(),
        name: label(
            agent
                .name
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(&agent.pane_id),
            80,
        ),
        kind,
        project: label(&project, 240),
        status: agent.agent_status,
    })
}

pub async fn discover(path: &Path) -> Result<Discovery> {
    let response = rpc(path, "agent.guarded_list", json!({})).await?;
    if let Some(result) = response.get("result") {
        let epoch = result
            .get("server_epoch")
            .and_then(Value::as_str)
            .filter(|e| !e.is_empty() && e.len() <= 128)
            .ok_or_else(|| ProviderError::message(INCOMPATIBLE))?;
        let agents = result
            .get("agents")
            .and_then(Value::as_array)
            .ok_or_else(|| ProviderError::message(INCOMPATIBLE))?;
        let recipients = agents
            .iter()
            .take(128)
            .filter_map(|entry| {
                let guard = entry.get("guard")?.as_str()?.to_owned();
                if guard.len() != 64 || !guard.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return None;
                }
                recipient(path, epoch, &guard, entry.get("agent")?.clone())
            })
            .collect();
        return Ok(Discovery {
            recipients,
            guarded: true,
        });
    }
    // Metadata-only compatibility view. There is intentionally no legacy send fallback.
    let response = rpc(path, "agent.list", json!({})).await?;
    let agents = response
        .pointer("/result/agents")
        .and_then(Value::as_array)
        .ok_or_else(|| ProviderError::message(INCOMPATIBLE))?;
    Ok(Discovery {
        recipients: agents
            .iter()
            .take(128)
            .filter_map(|agent| recipient(path, "", "", agent.clone()))
            .collect(),
        guarded: false,
    })
}

fn receipt(response: &Value, submission: &Submission) -> Option<SendOutcome> {
    let result = response.get("result")?;
    if result.get("server_epoch")?.as_str()? != submission.recipient.server_epoch
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
pub async fn submit(submission: &Submission, cancel: &CancellationToken) -> SendOutcome {
    if !submission.recipient.can_receive()
        || submission.text.trim().is_empty()
        || submission.text.len() > mluva_core::agent_voice::MAX_MESSAGE_BYTES
        || cancel.is_cancelled()
    {
        return SendOutcome::Rejected;
    }
    let recipient = &submission.recipient;
    let path = Path::new(&recipient.socket);
    let expires = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
        + 5_000;
    let params = json!({"target": recipient.pane_id, "server_epoch": recipient.server_epoch,
        "guard": recipient.guard, "operation_id": submission.id, "expires_at_unix_ms": expires, "text": submission.text});
    let first = tokio::select! { _ = cancel.cancelled() => return SendOutcome::Unconfirmed,
    result = rpc(path, "agent.prompt_guarded", params) => result };
    if let Ok(response) = first {
        if let Some(outcome) = receipt(&response, submission) {
            return outcome;
        }
        if response.pointer("/error/code").and_then(Value::as_str)
            == Some("guarded_prompt_rejected")
        {
            return SendOutcome::Rejected;
        }
    }
    let lookup = json!({"server_epoch": recipient.server_epoch, "operation_id": submission.id});
    let deadline = tokio::time::Instant::now() + Duration::from_secs(6);
    loop {
        let response = tokio::select! {
            _ = cancel.cancelled() => return SendOutcome::Unconfirmed,
            _ = tokio::time::sleep_until(deadline) => return SendOutcome::Unconfirmed,
            response = rpc(path, "agent.prompt_receipt", lookup.clone()) => response,
        };
        if let Ok(response) = response {
            if let Some(outcome) = receipt(&response, submission) {
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
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("herdr.sock");
        let listener = UnixListener::bind(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        (temp, path, listener)
    }
    async fn receive(listener: &UnixListener) -> (UnixStream, Value) {
        let (stream, _) = listener.accept().await.unwrap();
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
    fn submission(path: &Path) -> Submission {
        Submission {
            id: uuid::Uuid::new_v4().to_string(),
            text: "Synthetic Czech: zkouška příjemce.".into(),
            recipient: Recipient {
                socket: path.to_string_lossy().into(),
                server_epoch: "epoch".into(),
                pane_id: "w1:p2".into(),
                guard: "a".repeat(64),
                name: "selected".into(),
                kind: "codex".into(),
                project: "/synthetic".into(),
                status: AgentStatus::Working,
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
        assert_eq!(found.recipients.len(), 1);
        assert!(!found.recipients[0].can_receive());
        peer.await.unwrap();
        let sending = Submission {
            recipient: found.recipients[0].clone(),
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
