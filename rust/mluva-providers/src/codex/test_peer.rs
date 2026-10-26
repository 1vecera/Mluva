//! A local Codex app server for tests: WebSocket JSON-RPC on a Unix socket, answering with the
//! shapes captured from Codex 0.160.1. It keeps one thread's queue and turns, dispatches queued
//! messages as the scenario says, and records every message the client sends.
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::net::UnixListener;
use tokio_tungstenite::tungstenite::Message;

pub const THREAD: &str = "01900000-0000-7000-8000-00000000a001";

/// How the thread treats a queued message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scenario {
    /// Idle: the message starts a turn shortly after it is queued.
    Idle,
    /// Busy or not open: the message stays queued.
    Busy,
    /// The message starts a turn just as it is withdrawn.
    TakenDuringDelete,
    /// The server lacks the experimental queue methods.
    Unsupported,
    /// The thread is unknown to the store.
    UnknownThread,
    /// The add is stored and later dispatched, but its answer never arrives.
    AddAnswerLost,
}

#[derive(Default)]
pub struct State {
    pub queue: Vec<Value>,
    pub turns: Vec<Value>,
    /// Every message the client sent, in order.
    pub received: Vec<Value>,
    pub connections: usize,
}

pub struct Peer {
    pub socket: PathBuf,
    pub state: Arc<Mutex<State>>,
    _temp: tempfile::TempDir,
}

impl Peer {
    pub fn start(scenario: Scenario) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let socket = temp.path().join("app-server.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let state = Arc::new(Mutex::new(State::default()));
        let shared = state.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                shared.lock().unwrap().connections += 1;
                let state = shared.clone();
                tokio::spawn(async move {
                    let Ok(mut socket) = tokio_tungstenite::accept_async(stream).await else {
                        return;
                    };
                    while let Some(Ok(Message::Text(text))) = socket.next().await {
                        let message: Value = serde_json::from_str(&text).unwrap();
                        state.lock().unwrap().received.push(message.clone());
                        let Some(method) = message.get("method").and_then(Value::as_str) else {
                            continue;
                        };
                        let Some(id) = message.get("id").cloned() else {
                            continue;
                        };
                        let params = message["params"].clone();
                        if method == "thread/queue/add" {
                            // Notifications and a server request come first; neither is the
                            // answer, and the request must never be answered.
                            for unsolicited in [
                                json!({"method": "thread/queue/changed", "params": {"threadId": THREAD}}),
                                json!({"id": 0, "method": "item/commandExecution/requestApproval",
                                    "params": {"threadId": THREAD, "command": "synthetic"}}),
                                json!({"method": "account/rateLimits/updated", "params": {"plan": "synthetic"}}),
                            ] {
                                socket
                                    .send(Message::Text(unsolicited.to_string().into()))
                                    .await
                                    .unwrap();
                            }
                        }
                        let reply = answer(scenario, method, &params, &state);
                        let Some(reply) = reply else {
                            // The answer is lost with the connection.
                            return;
                        };
                        let mut response = json!({"id": id});
                        match reply {
                            Ok(result) => response["result"] = result,
                            Err((code, text)) => {
                                response["error"] = json!({"code": code, "message": text})
                            }
                        }
                        socket
                            .send(Message::Text(response.to_string().into()))
                            .await
                            .unwrap();
                    }
                });
            }
        });
        Self {
            socket,
            state,
            _temp: temp,
        }
    }

    /// The methods the client called, in order.
    pub fn methods(&self) -> Vec<String> {
        self.state
            .lock()
            .unwrap()
            .received
            .iter()
            .filter_map(|m| m.get("method").and_then(Value::as_str).map(str::to_owned))
            .collect()
    }

    /// Messages the client sent that answer a server request: there must be none.
    pub fn answers_to_server(&self) -> usize {
        self.state
            .lock()
            .unwrap()
            .received
            .iter()
            .filter(|m| m.get("method").is_none())
            .count()
    }
}

type Reply = Option<Result<Value, (i64, &'static str)>>;

fn answer(scenario: Scenario, method: &str, params: &Value, state: &Arc<Mutex<State>>) -> Reply {
    let mut locked = state.lock().unwrap();
    Some(match method {
        "initialize" => Ok(
            json!({"userAgent": "mluva/0.160.1 (synthetic)", "codexHome": "/synthetic/codex",
            "platformFamily": "unix", "platformOs": "linux"}),
        ),
        "thread/queue/list" if scenario == Scenario::Unsupported => Err((
            -32601,
            "thread/queue/list requires experimentalApi capability",
        )),
        "thread/queue/list" => Ok(json!({"data": locked.queue, "nextCursor": null})),
        "thread/queue/add" if scenario == Scenario::UnknownThread => Err((
            -32603,
            "failed to read thread: invalid thread-store request: no rollout found for thread id",
        )),
        "thread/queue/add" => {
            let item = json!({"id": format!("item-{}", locked.queue.len() + locked.turns.len() + 1),
                "input": params["input"], "clientUserMessageId": params["clientUserMessageId"]});
            locked.queue.push(item.clone());
            if matches!(scenario, Scenario::Idle | Scenario::AddAnswerLost) {
                let state = state.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    dispatch(&mut state.lock().unwrap());
                });
            }
            if scenario == Scenario::AddAnswerLost {
                return None;
            }
            Ok(json!({"queuedSubmission": item}))
        }
        "thread/queue/delete" => {
            if scenario == Scenario::TakenDuringDelete {
                dispatch(&mut locked);
            }
            let before = locked.queue.len();
            let id = params["queuedSubmissionId"].clone();
            locked.queue.retain(|item| item["id"] != id);
            Ok(json!({"deleted": locked.queue.len() < before}))
        }
        "thread/turns/list" => {
            let mut turns = locked.turns.clone();
            turns.reverse();
            Ok(json!({"data": turns, "nextCursor": null, "backwardsCursor": null}))
        }
        "thread/read" => {
            let id = params["threadId"].as_str().unwrap_or_default();
            Ok(json!({"thread": if id == THREAD {
                json!({"id": id, "sessionId": id, "name": "Fix the build", "ephemeral": false,
                    "threadSource": "user", "parentThreadId": null, "cwd": "/synthetic/project",
                    "source": "cli", "status": {"type": "idle"}})
            } else {
                json!({"id": id, "ephemeral": true, "threadSource": "thread_title", "name": null,
                    "status": {"type": "idle"}})
            }}))
        }
        _ => Err((-32600, "not served by this test peer")),
    })
}

/// The thread takes its oldest queued message as a new turn.
fn dispatch(state: &mut State) {
    if state.queue.is_empty() {
        return;
    }
    let item = state.queue.remove(0);
    state.turns.push(json!({"id": format!("turn-{}", state.turns.len() + 1), "status": "inProgress",
        "items": [
            {"type": "userMessage", "id": "typed-earlier", "clientId": "00000000-0000-4000-8000-0000000000aa", "content": []},
            {"type": "userMessage", "id": "item", "clientId": item["clientUserMessageId"], "content": item["input"]},
        ]}));
}
