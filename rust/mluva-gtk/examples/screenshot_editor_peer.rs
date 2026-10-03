//! External editor/file-monitor boundary; no Mluva implementation is imported.
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};
static TERMINATED: AtomicBool = AtomicBool::new(false);
extern "C" fn terminate(_: i32) {
    TERMINATED.store(true, Ordering::Relaxed);
}

fn main() {
    let private = PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").unwrap());
    assert!(PathBuf::from(std::env::var_os("HOME").unwrap()).starts_with(&private));
    assert_ne!(
        fs::read_link("/proc/self/ns/net")
            .unwrap()
            .to_str()
            .unwrap(),
        std::env::var("MLUVA_HOST_NET_NS").unwrap()
    );
    for path in ["/dev/input", "/dev/uinput", "/dev/snd", "/dev/dri"] {
        assert!(!Path::new(path).exists());
    }
    let root = PathBuf::from(std::env::var_os("MLUVA_SCREENSHOT_FIXTURE_ROOT").unwrap());
    assert!(root.starts_with(&private));
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    assert_eq!(arguments.len(), 1);
    let path = PathBuf::from(&arguments[0]);
    assert!(path.starts_with(root.join("data")));
    let spec: Value = serde_json::from_slice(&fs::read(root.join("picker.json")).unwrap()).unwrap();
    unsafe {
        libc::signal(
            libc::SIGTERM,
            if spec["editor_ignore_term"] == true {
                libc::SIG_IGN
            } else {
                terminate as *const () as libc::sighandler_t
            },
        );
    }
    let id = path.file_stem().unwrap();
    let directory = root.join("editors").join(id);
    fs::create_dir_all(&directory).unwrap();
    let ready = directory.join(format!("{}.json", std::process::id()));
    let mut serial = 0;
    let observe = |serial| json!({"pid":std::process::id(),"serial":serial,"path":path,"png":hex(&fs::read(&path).unwrap_or_default())});
    save(&ready, &observe(serial));
    loop {
        if TERMINATED.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(
                spec["editor_term_delay_ms"].as_u64().unwrap_or(0),
            ));
            if let Some(data) = spec["editor_term_png"].as_str() {
                fs::write(&path, unhex(data)).unwrap();
            }
            break;
        }
        if let Ok(raw) = fs::read(directory.join("command.json"))
            && let Ok(command) = serde_json::from_slice::<Value>(&raw)
            && command["serial"]
                .as_u64()
                .is_some_and(|value| value > serial)
        {
            serial = command["serial"].as_u64().unwrap();
            if command["op"] == "save" {
                fs::write(&path, unhex(command["png"].as_str().unwrap())).unwrap();
                save(&ready, &observe(serial));
            } else if command["op"] == "quit" {
                save(&ready, &observe(serial));
                break;
            } else {
                panic!("unknown editor fixture command");
            }
        }
        thread::sleep(Duration::from_millis(5));
    }
}
fn save(path: &Path, value: &Value) {
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, serde_json::to_vec(value).unwrap()).unwrap();
    fs::rename(temporary, path).unwrap();
}
fn hex(data: &[u8]) -> String {
    data.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn unhex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
