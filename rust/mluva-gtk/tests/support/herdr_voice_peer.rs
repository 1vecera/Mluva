//! Synthetic local protocol peer, never a personal Herdr session or a real agent.
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::{fs::PermissionsExt, net::UnixListener},
    path::PathBuf,
};
fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 4);
    let socket = PathBuf::from(&args[1]);
    let state = PathBuf::from(&args[2]);
    let audit = PathBuf::from(&args[3]);
    let listener = UnixListener::bind(socket.as_path()).unwrap();
    fs::set_permissions(socket, fs::Permissions::from_mode(0o600)).unwrap();
    let mut receipts = HashMap::new();
    println!("herdr-voice-peer-ready");
    for stream in listener.incoming() {
        let mut stream = stream.unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let mut line = String::new();
        BufReader::new(&mut stream).read_line(&mut line).unwrap();
        let request: Value = serde_json::from_str(&line).unwrap();
        fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&audit)
            .unwrap()
            .write_all(line.as_bytes())
            .unwrap();
        let state: Value = serde_json::from_slice(&fs::read(&state).unwrap()).unwrap();
        let params = &request["params"];
        let result = match request["method"].as_str().unwrap() {
            "agent.guarded_list" => {
                json!({"server_epoch": "synthetic-server", "agents": state["agents"]})
            }
            // Every agent, guarded or not: sessions Herdr did not launch carry no guard.
            "agent.list" => {
                let mut agents: Vec<Value> = state["agents"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|entry| entry["agent"].clone())
                    .collect();
                agents.extend(state["unmanaged"].as_array().cloned().unwrap_or_default());
                json!({"agents": agents})
            }
            "agent.prompt_guarded" => {
                let valid = state["agents"].as_array().unwrap().iter().any(|agent| {
                    agent["agent"]["pane_id"] == params["target"]
                        && agent["guard"] == params["guard"]
                        && agent["agent"]["agent_status"] != "blocked"
                });
                let status = if !valid {
                    "rejected"
                } else if state["mode"] == "uncertain" {
                    "uncertain"
                } else {
                    "accepted"
                };
                receipts
                    .entry(params["operation_id"].as_str().unwrap().to_owned())
                    .or_insert(status.to_owned());
                if state["mode"] == "drop-ack" {
                    continue;
                }
                json!({"server_epoch": "synthetic-server", "operation_id": params["operation_id"], "state": status})
            }
            "agent.prompt_receipt" => {
                json!({"server_epoch": "synthetic-server", "operation_id": params["operation_id"], "state": receipts.get(params["operation_id"].as_str().unwrap()).unwrap()})
            }
            _ => panic!(
                "Unexpected transport: no terminal scraping, focus or raw key API is permitted"
            ),
        };
        let response = json!({"id": request["id"], "result": result});
        let _ = writeln!(stream, "{response}");
    }
}
