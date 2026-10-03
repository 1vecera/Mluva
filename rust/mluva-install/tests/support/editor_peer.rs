//! Unshipped external editor endpoint. Never opens a display or an image.
use serde_json::json;
use std::{
    env, fs,
    io::Write,
    os::unix::{ffi::OsStrExt, fs::PermissionsExt},
    path::PathBuf,
};

fn main() {
    let root = PathBuf::from(env::var_os("MLUVA_EDITOR_PEER_DIR").expect("private peer directory"));
    let args: Vec<_> = env::args_os().skip(1).collect();
    let narrator = args
        .windows(2)
        .find(|pair| pair[0] == "--narration-command")
        .map(|pair| {
            fs::metadata(&pair[1]).is_ok_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            })
        });
    println!("editor stdout");
    eprintln!("editor stderr");
    std::io::stdout().flush().unwrap();
    let pid = std::process::id();
    fs::write(root.join("receipt.json"), serde_json::to_vec(&json!({
        "pid":pid,"same_pid":fs::read_to_string(root.join("initial-pid")).unwrap().trim() == pid.to_string(),
        "arguments":args.iter().map(|arg| arg.as_os_str().as_bytes().to_vec()).collect::<Vec<_>>(),
        "narration_executable":narrator
    })).unwrap()).unwrap();
    if env::var_os("MLUVA_EDITOR_PEER_WAIT").is_some() {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
    std::process::exit(
        env::var("MLUVA_EDITOR_PEER_EXIT")
            .unwrap_or_else(|_| "0".into())
            .parse()
            .unwrap(),
    );
}
