//! A JSON-RPC client for a Codex app server: WebSocket text frames over its Unix socket, one
//! message per frame, without the `jsonrpc` member.
//!
//! Only the methods in [`ALLOWED_METHODS`] ever leave this client: anything else is refused
//! before it reaches the socket, so Mluva cannot start, resume, steer or interrupt a turn,
//! subscribe to a thread or change Codex's configuration. Notifications and requests from the
//! server are read past and never answered, so a pending approval can never be granted from
//! here. Nothing read is logged.
use crate::agent_platform;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{path::Path, time::Duration};
use tokio::net::UnixStream;
use tokio_tungstenite::{
    WebSocketStream,
    tungstenite::{Message, protocol::WebSocketConfig},
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const CALL_TIMEOUT: Duration = Duration::from_secs(5);
/// Thread metadata and short turn pages are small; a larger message is not read.
const MAX_MESSAGE: usize = 8 * 1_048_576;

/// Every method Mluva's Codex client may send: the handshake, reading which threads a server
/// has loaded and what a thread is, and listing, adding and withdrawing its own queued message
/// and finding the turn that took it.
pub const ALLOWED_METHODS: &[&str] = &[
    "initialize",
    "initialized",
    "thread/loaded/list",
    "thread/read",
    "thread/queue/list",
    "thread/queue/add",
    "thread/queue/delete",
    "thread/turns/list",
];

/// Which process may answer on a server's socket.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Peer {
    /// Exactly this process, such as the daemon its PID file names.
    Process(u32),
    /// This process or one it started, such as Mluva's helper behind an `npm` launcher.
    Within(u32),
}

impl Peer {
    fn accepts(self, pid: u32) -> bool {
        match self {
            Self::Process(expected) => pid == expected,
            Self::Within(root) => {
                let mut current = pid;
                for _ in 0..16 {
                    if current == root {
                        return true;
                    }
                    match agent_platform::parent(current) {
                        Some(next) if next > 1 && next != current => current = next,
                        _ => return false,
                    }
                }
                false
            }
        }
    }
}

/// Why a call did not return a result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RpcError {
    /// The method is not one Mluva sends; nothing was written.
    Refused,
    /// The server answered with an error.
    Server { code: i64, message: String },
    /// No answer arrived: the connection failed, closed or timed out. The request may or may
    /// not have been carried out.
    Transport,
}

impl RpcError {
    /// The server does not offer the method, or offers it only to experimental clients.
    pub fn unsupported(&self) -> bool {
        matches!(self, Self::Server { code, message } if *code == -32601 || message.contains("experimental"))
    }
}

/// An initialized connection to one app server.
pub struct AppServer {
    socket: WebSocketStream<UnixStream>,
    next: u64,
}

impl AppServer {
    /// Connect to the server at `path`, check that the process answering is one `peer` accepts
    /// and belongs to this account, and initialize with the experimental API.
    pub async fn connect(path: &Path, peer: Peer) -> Result<Self, RpcError> {
        let stream = tokio::time::timeout(CONNECT_TIMEOUT, UnixStream::connect(path))
            .await
            .map_err(|_| RpcError::Transport)?
            .map_err(|_| RpcError::Transport)?;
        let (uid, pid) = agent_platform::peer_process(&stream).ok_or(RpcError::Transport)?;
        if uid != agent_platform::effective_uid() || !peer.accepts(pid) {
            return Err(RpcError::Transport);
        }
        let config = WebSocketConfig::default()
            .max_message_size(Some(MAX_MESSAGE))
            .max_frame_size(Some(MAX_MESSAGE));
        let (socket, _) = tokio::time::timeout(
            CONNECT_TIMEOUT,
            tokio_tungstenite::client_async_with_config("ws://localhost/rpc", stream, Some(config)),
        )
        .await
        .map_err(|_| RpcError::Transport)?
        .map_err(|_| RpcError::Transport)?;
        let mut server = Self { socket, next: 1 };
        server
            .call(
                "initialize",
                json!({"clientInfo": {"name": "mluva", "version": env!("CARGO_PKG_VERSION")},
                    "capabilities": {"experimentalApi": true}}),
            )
            .await?;
        server.notify("initialized").await?;
        Ok(server)
    }

    async fn notify(&mut self, method: &str) -> Result<(), RpcError> {
        if !ALLOWED_METHODS.contains(&method) {
            return Err(RpcError::Refused);
        }
        self.socket
            .send(Message::Text(json!({"method": method}).to_string().into()))
            .await
            .map_err(|_| RpcError::Transport)
    }

    /// Send one request and wait for its own response.
    pub async fn call(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
        if !ALLOWED_METHODS.contains(&method) {
            return Err(RpcError::Refused);
        }
        let id = self.next;
        self.next += 1;
        let request = json!({"id": id, "method": method, "params": params}).to_string();
        tokio::time::timeout(CALL_TIMEOUT, async {
            self.socket
                .send(Message::Text(request.into()))
                .await
                .map_err(|_| RpcError::Transport)?;
            while let Some(frame) = self.socket.next().await {
                let text = match frame.map_err(|_| RpcError::Transport)? {
                    Message::Text(text) => text,
                    Message::Close(_) => return Err(RpcError::Transport),
                    _ => continue,
                };
                let Ok(message) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                // A server request has a method, and its ID may equal ours: never ours, never
                // answered.
                if message.get("method").is_some() || message.get("id") != Some(&json!(id)) {
                    continue;
                }
                if let Some(error) = message.get("error") {
                    return Err(RpcError::Server {
                        code: error.get("code").and_then(Value::as_i64).unwrap_or(0),
                        message: error
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .chars()
                            .take(512)
                            .collect(),
                    });
                }
                return Ok(message.get("result").cloned().unwrap_or(Value::Null));
            }
            Err(RpcError::Transport)
        })
        .await
        .map_err(|_| RpcError::Transport)?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codex::test_peer::{Peer as TestPeer, Scenario, THREAD};

    #[tokio::test]
    async fn only_allowed_methods_ever_reach_the_socket() {
        let peer = TestPeer::start(Scenario::Idle);
        let mut server = AppServer::connect(&peer.socket, Peer::Process(std::process::id()))
            .await
            .unwrap();
        for forbidden in [
            "thread/start",
            "thread/resume",
            "turn/start",
            "turn/steer",
            "turn/interrupt",
            "thread/queue/start",
            "config/write",
            "skills/config/write",
            "thread/subscribe",
        ] {
            assert_eq!(
                server.call(forbidden, json!({"threadId": THREAD})).await,
                Err(RpcError::Refused),
                "{forbidden}"
            );
        }
        assert!(server.call("thread/loaded/list", json!({})).await.is_ok());
        let sent = peer.methods();
        assert_eq!(sent, ["initialize", "initialized", "thread/loaded/list"]);
    }

    #[tokio::test]
    async fn a_socket_answered_by_an_unexpected_process_is_never_initialized() {
        let peer = TestPeer::start(Scenario::Idle);
        let own = std::process::id();
        // This process descends from its parent: `Within` the parent accepts it, a different
        // exact process does not.
        let parent = agent_platform::parent(own).unwrap();
        assert!(
            AppServer::connect(&peer.socket, Peer::Within(parent))
                .await
                .is_ok()
        );
        assert_eq!(
            AppServer::connect(&peer.socket, Peer::Process(parent))
                .await
                .err(),
            Some(RpcError::Transport)
        );
        assert_eq!(
            peer.methods().iter().filter(|m| *m == "initialize").count(),
            1
        );
    }
}
