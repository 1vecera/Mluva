//! A synthetic Codex daemon with one live thread, for private application tests.
//!
//! Usage: codex-app-server-fixture-peer CODEX_HOME RUNTIME THREAD_ID TITLE CWD MODE_FILE WIRE
//!
//! It lays out CODEX_HOME as Codex 0.160.1 does for a running daemon: `daemon.pid` naming this
//! process and this boot, a control-socket link into `RUNTIME/codex-daemon-<uid>/`, the thread's
//! name in `session_index.jsonl`, and the thread's writer lock, held by this process. It answers
//! the app-server JSON-RPC Mluva uses over WebSocket. MODE_FILE says how the thread treats a
//! queued message: `idle` takes it as a turn, anything else leaves it queued. Every message
//! received is appended to WIRE. Only synthetic text ever reaches it.
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    os::unix::{fs::PermissionsExt, io::AsRawFd},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::net::UnixListener;
use tokio_tungstenite::tungstenite::Message;

#[derive(Default)]
struct Thread {
    queue: Vec<Value>,
    turns: Vec<Value>,
}

fn private_directory(path: &Path) {
    fs::create_dir_all(path).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

fn append(path: &Path, line: &Value) {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
    file.write_all(format!("{line}\n").as_bytes()).unwrap();
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [home, runtime, thread, title, cwd, mode, wire] = args.as_slice() else {
        eprintln!(
            "usage: codex-app-server-fixture-peer CODEX_HOME RUNTIME THREAD TITLE CWD MODE WIRE"
        );
        std::process::exit(2);
    };
    let (home, runtime) = (PathBuf::from(home), PathBuf::from(runtime));
    let pid = std::process::id();
    // SAFETY: geteuid has no arguments or memory preconditions.
    let uid = unsafe { libc::geteuid() };
    let stat = fs::read_to_string("/proc/self/stat").unwrap();
    let ticks: u64 = stat[stat.rfind(')').unwrap() + 1..]
        .split_whitespace()
        .nth(19)
        .unwrap()
        .parse()
        .unwrap();
    let boot = fs::read_to_string("/proc/sys/kernel/random/boot_id").unwrap();
    private_directory(&home);
    // The thread's writer lock, held for as long as this process lives.
    let locks = home.join("thread-writer-locks");
    private_directory(&locks);
    let lock = fs::File::create(locks.join(format!("{thread}.lock"))).unwrap();
    // SAFETY: a valid open descriptor and a lock operation.
    assert_eq!(unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) }, 0);
    append(
        &home.join("session_index.jsonl"),
        &json!({"id": thread, "thread_name": title, "updated_at": "2026-10-26T11:27:54Z"}),
    );
    let sockets = runtime.join(format!("codex-daemon-{uid}"));
    private_directory(&sockets);
    let socket = sockets.join("0123456789abcdef");
    let listener = UnixListener::bind(&socket).unwrap();
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o600)).unwrap();
    let control = home.join("app-server-control");
    private_directory(&control);
    let _ = fs::remove_file(control.join("app-server-control.sock"));
    std::os::unix::fs::symlink(&socket, control.join("app-server-control.sock")).unwrap();
    let daemon = home.join("app-server-daemon");
    private_directory(&daemon);
    let pid_file = daemon.join("daemon.pid");
    fs::write(
        &pid_file,
        json!({"pid": pid, "processStartTime": "synthetic",
            "processIdentity": {"bootId": boot.trim(), "startTicks": ticks}})
        .to_string(),
    )
    .unwrap();
    fs::set_permissions(&pid_file, fs::Permissions::from_mode(0o600)).unwrap();
    let state = Arc::new(Mutex::new(Thread::default()));
    let (thread, title, cwd) = (thread.clone(), title.clone(), cwd.clone());
    let (mode, wire) = (PathBuf::from(mode), PathBuf::from(wire));
    while let Ok((stream, _)) = listener.accept().await {
        let (state, thread, title, cwd) =
            (state.clone(), thread.clone(), title.clone(), cwd.clone());
        let (mode, wire) = (mode.clone(), wire.clone());
        tokio::spawn(async move {
            let Ok(mut socket) = tokio_tungstenite::accept_async(stream).await else {
                return;
            };
            while let Some(Ok(Message::Text(text))) = socket.next().await {
                let Ok(message) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                append(&wire, &message);
                let (Some(method), Some(id)) = (
                    message.get("method").and_then(Value::as_str),
                    message.get("id").cloned(),
                ) else {
                    continue;
                };
                let params = &message["params"];
                let idle = fs::read_to_string(&mode).is_ok_and(|mode| mode.trim() == "idle");
                let result = {
                    let mut locked = state.lock().unwrap();
                    match method {
                        "initialize" => {
                            json!({"userAgent": "fixture/0.160.1", "codexHome": "/synthetic",
                            "platformFamily": "unix", "platformOs": "linux"})
                        }
                        "thread/read" => {
                            json!({"thread": {"id": thread, "sessionId": thread, "name": title,
                            "ephemeral": false, "threadSource": "user", "parentThreadId": null, "cwd": cwd,
                            "source": "cli", "status": if idle { json!({"type": "idle"}) }
                                else { json!({"type": "active", "activeFlags": []}) }}})
                        }
                        "thread/queue/list" => json!({"data": locked.queue, "nextCursor": null}),
                        "thread/queue/add" => {
                            let item = json!({"id": format!("item-{}", locked.queue.len() + locked.turns.len() + 1),
                                "input": params["input"], "clientUserMessageId": params["clientUserMessageId"]});
                            locked.queue.push(item.clone());
                            if idle {
                                let state = state.clone();
                                tokio::spawn(async move {
                                    tokio::time::sleep(Duration::from_millis(200)).await;
                                    let mut thread = state.lock().unwrap();
                                    if !thread.queue.is_empty() {
                                        let item = thread.queue.remove(0);
                                        thread.turns.push(json!({"id": "turn", "status": "inProgress",
                                            "items": [{"type": "userMessage", "id": "message",
                                                "clientId": item["clientUserMessageId"], "content": item["input"]}]}));
                                    }
                                });
                            }
                            json!({"queuedSubmission": item})
                        }
                        "thread/queue/delete" => {
                            let before = locked.queue.len();
                            locked
                                .queue
                                .retain(|item| item["id"] != params["queuedSubmissionId"]);
                            json!({"deleted": locked.queue.len() < before})
                        }
                        "thread/turns/list" => {
                            let mut turns = locked.turns.clone();
                            turns.reverse();
                            json!({"data": turns, "nextCursor": null})
                        }
                        _ => {
                            drop(locked);
                            let error = json!({"id": id, "error": {"code": -32600, "message": "not served by the fixture"}});
                            let _ = socket.send(Message::Text(error.to_string().into())).await;
                            continue;
                        }
                    }
                };
                let response = json!({"id": id, "result": result});
                if socket
                    .send(Message::Text(response.to_string().into()))
                    .await
                    .is_err()
                {
                    return;
                }
            }
        });
    }
    drop(lock);
}
