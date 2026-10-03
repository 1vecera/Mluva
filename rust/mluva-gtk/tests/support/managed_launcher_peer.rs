//! Independent executable credential-launcher peer. Synthetic values only; never shipped.
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    os::unix::{fs::OpenOptionsExt, process::CommandExt},
    path::PathBuf,
    process::Command,
};

fn main() {
    let root = PathBuf::from(std::env::var_os("MLUVA_LAUNCH_FIXTURE").unwrap());
    let spec: Value = serde_json::from_slice(&fs::read(root.join("peer.json")).unwrap()).unwrap();
    let argv: Vec<_> = std::env::args().collect();
    let name = std::path::Path::new(&argv[0])
        .file_name()
        .unwrap()
        .to_str()
        .unwrap();
    let arguments = &argv[1..];
    let trace = json!({"launcher":name,"arguments":arguments,"same_pid":std::env::var("MLUVA_LAUNCH_PID").unwrap()==std::process::id().to_string(),
        "profile":std::env::var("MLUVA_SECRET_PROFILE").ok(),"inherited_canary":std::env::var_os("MLUVA_LAUNCH_CANARY").is_some()});
    let mut trace_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(root.join("receipts.jsonl"))
        .unwrap();
    writeln!(trace_file, "{trace}").unwrap();
    if let Some(code) = spec["exit"].as_i64() {
        eprintln!("Synthetic credential profile unavailable.");
        std::process::exit(code as i32);
    }
    if spec["wait"] == true {
        fs::write(root.join("ready"), b"ready").unwrap();
        loop {
            std::thread::park();
        }
    }
    let (offset, key) = match name {
        "das-agent-snapshot" => (4, arguments[2].as_str()),
        "das-mcp-launch" => (2, "ELEVENLABS_API_KEY"),
        "das-agent-launch" => (3, arguments[1].as_str()),
        other => panic!("unexpected private peer {other}"),
    };
    let mut command = Command::new(&arguments[offset]);
    command.args(&arguments[offset + 1..]);
    if spec["inject"] != false {
        command.env(key, "synthetic-launcher-credential");
    }
    let error = command.exec();
    panic!("private continuation failed: {error}");
}
