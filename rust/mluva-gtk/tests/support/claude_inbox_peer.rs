//! A synthetic Claude Code session with an inbox, for the private application test.
//!
//! Usage: claude-inbox-fixture-peer CONFIG SOCKET_DIR SESSION_ID CWD TITLE WIRE [MODE_FILE]
//!
//! When MODE_FILE says `working`, the session admits a message at once but takes it into its
//! turn only four seconds later, as a busy session does at its next tool boundary.
//!
//! It registers itself in CONFIG's session registry as Claude Code 2.1.310 does, under its own
//! PID and start time, binds its inbox at SOCKET_DIR/<pid>.sock and writes a transcript with a
//! generated title. Each frame it receives is appended to WIRE, and the transcript records the
//! queue admitting it and the turn that took it, as an idle session does. Only synthetic text
//! ever reaches it.
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    os::unix::{fs::PermissionsExt, net::UnixListener},
    path::Path,
};

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

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (args, mode) = match args.as_slice() {
        [rest @ .., mode] if rest.len() == 6 => (rest.to_vec(), Some(mode.clone())),
        _ => (args.clone(), None),
    };
    let [config, sockets, session, cwd, title, wire] = args.as_slice() else {
        eprintln!("usage: claude-inbox-fixture-peer CONFIG SOCKET_DIR SESSION_ID CWD TITLE WIRE");
        std::process::exit(2);
    };
    let (config, sockets) = (Path::new(config), Path::new(sockets));
    let pid = std::process::id();
    let stat = fs::read_to_string("/proc/self/stat").unwrap();
    let start = stat[stat.rfind(')').unwrap() + 1..]
        .split_whitespace()
        .nth(19)
        .unwrap()
        .to_owned();
    private_directory(sockets);
    let socket = sockets.join(format!("{pid}.sock"));
    let listener = UnixListener::bind(&socket).unwrap();
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o600)).unwrap();
    // Claude Code names a project's transcript folder after its path.
    let slug: String = cwd
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let projects = config.join("projects").join(slug);
    private_directory(&projects);
    let transcript = projects.join(format!("{session}.jsonl"));
    append(
        &transcript,
        &json!({"type": "ai-title", "aiTitle": title, "sessionId": session}),
    );
    private_directory(&config.join("sessions"));
    let entry = config.join(format!("sessions/{pid}.json"));
    fs::write(
        &entry,
        json!({"pid": pid, "sessionId": session, "cwd": cwd, "startedAt": 2, "procStart": start,
            "version": "2.1.310", "peerProtocol": 1, "kind": "interactive", "entrypoint": "cli",
            "messagingSocketPath": socket.to_string_lossy(), "name": "inbox-3b",
            "nameSource": "derived", "status": "idle"})
        .to_string(),
    )
    .unwrap();
    fs::set_permissions(&entry, fs::Permissions::from_mode(0o600)).unwrap();
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let mut bytes = vec![];
        if stream.read_to_end(&mut bytes).is_err() || bytes.is_empty() {
            continue;
        }
        let Ok(frame) = serde_json::from_slice::<Value>(&bytes) else {
            continue;
        };
        append(Path::new(wire), &frame);
        // Claude Code drops a frame meant for another session.
        if frame["session_id"] != session.as_str() {
            continue;
        }
        let content = frame["message"]["content"].clone();
        append(
            &transcript,
            &json!({"type": "queue-operation", "operation": "enqueue", "sessionId": session, "content": content}),
        );
        let taken = json!({"type": "user", "uuid": frame["uuid"], "isMeta": true, "origin": {"kind": "peer"}, "sessionId": session});
        let working = mode
            .as_deref()
            .and_then(|mode| fs::read_to_string(mode).ok())
            .is_some_and(|mode| mode.trim() == "working");
        if working {
            let transcript = transcript.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(4));
                append(&transcript, &taken);
            });
        } else {
            append(&transcript, &taken);
        }
    }
}
