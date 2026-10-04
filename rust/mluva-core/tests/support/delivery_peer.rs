//! Native synthetic desktop helpers and public delivery driver; never installed.
mod terminal_peer;
use mluva_core::delivery::{
    DeliveryError, DeliveryOptions, TargetResult, deliver_text, keyboard_paste_available,
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    ffi::OsString,
    fs,
    io::{Read, Write},
    os::unix::{
        fs::{OpenOptionsExt, PermissionsExt},
        net::{UnixDatagram, UnixListener},
    },
    path::Path,
    time::{Duration, Instant},
};

fn trace(root: &Path, mut value: Value) {
    let mut now = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    assert_eq!(
        unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut now) },
        0
    );
    value["at_ns"] = json!(now.tv_sec as u64 * 1_000_000_000 + now.tv_nsec as u64);
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(root.join("trace.jsonl"))
        .unwrap();
    let mut line = serde_json::to_vec(&value).unwrap();
    line.push(b'\n');
    file.write_all(&line).unwrap();
}

fn run_helper(root: &Path, name: &str) -> ! {
    let spec: Value =
        serde_json::from_slice(&fs::read(root.join("fixture.json")).unwrap()).unwrap();
    let options = &spec["helper_options"][name];
    let mut input = vec![];
    if matches!(name, "wl-copy" | "xclip") && options["close_stdin"] != true {
        std::io::stdin().read_to_end(&mut input).unwrap();
    } else if options["close_stdin"] == true {
        unsafe { libc::close(libc::STDIN_FILENO) };
    }
    let event = json!({"event":"helper","helper":name,
        "args":std::env::args().skip(1).collect::<Vec<_>>(),
        "input_bytes":input.len(),"pid":std::process::id()});
    // Store actual stdin separately: the parent compares it with its synthetic source bytes.
    if matches!(name, "wl-copy" | "xclip") {
        fs::write(root.join("clipboard.bin"), input).unwrap();
    }
    trace(root, event);
    let sleep = options["sleep_ms"].as_u64().unwrap_or(0);
    std::thread::sleep(Duration::from_millis(sleep));
    if let Some(signal) = options["signal"].as_i64() {
        unsafe { libc::raise(signal as libc::c_int) };
        panic!("fixture helper signal did not stop the process");
    }
    std::process::exit(options["exit"].as_i64().unwrap_or(0) as i32)
}

fn callback(
    root: &Path,
    kind: &str,
    value: &Value,
    text: Option<&str>,
) -> TargetResult<Option<bool>> {
    let helpers_finished = helpers_reaped(root);
    let clipboard_present = root.join("clipboard.bin").exists();
    trace(
        root,
        json!({"event":kind,"value":value,"clipboard_present":clipboard_present,
        "helpers_reaped":helpers_finished,"text":text}),
    );
    if value == "error" {
        return Err(std::io::Error::other("synthetic captured-target failure").into());
    }
    Ok(value.as_bool())
}

fn helpers_reaped(root: &Path) -> bool {
    let path = root.join("trace.jsonl");
    if !path.exists() {
        return true;
    }
    let trace = fs::read_to_string(path).unwrap();
    trace.lines().all(|line| {
        let line: Value = serde_json::from_str(line).unwrap();
        line["pid"]
            .as_u64()
            .is_none_or(|pid| !Path::new("/proc").join(pid.to_string()).exists())
    })
}

fn text(spec: &Value) -> String {
    spec["text"]
        .as_str()
        .unwrap_or("Příliš žluťoučký 😊\nSecond line\r\n")
        .repeat(spec["text_repeat"].as_u64().unwrap_or(1) as usize)
}

fn driver(root: &Path, spec: Value) {
    let socket_path = spec["socket_path"].as_str().map(Path::new);
    let mut datagram = None;
    let mut stream = None;
    if let Some(path) = socket_path {
        match spec["socket_kind"].as_str().unwrap_or("live") {
            "live" | "stale" | "symlink" => {
                let bound = if spec["socket_kind"] == "symlink" {
                    root.join("real-socket")
                } else {
                    path.to_path_buf()
                };
                let listener = UnixDatagram::bind(&bound).unwrap();
                if spec["socket_kind"] == "symlink" {
                    std::os::unix::fs::symlink(bound, path).unwrap();
                }
                if spec["socket_kind"] != "stale" {
                    listener.set_nonblocking(true).unwrap();
                    datagram = Some(listener);
                }
            }
            "stream" => stream = Some(UnixListener::bind(path).unwrap()),
            "file" => fs::write(path, "not a daemon socket").unwrap(),
            "directory" => fs::create_dir(path).unwrap(),
            "missing" => {}
            _ => panic!("unknown fixture socket kind"),
        }
        if path.exists() {
            fs::set_permissions(
                path,
                fs::Permissions::from_mode(spec["socket_mode"].as_u64().unwrap_or(0o600) as u32),
            )
            .unwrap();
        }
    }
    let source = text(&spec);
    let mut direct = |text: &str| callback(root, "direct", &spec["direct"], Some(text));
    let authorize = || {
        let value = callback(root, "authorize", &spec["authorize"], None)?;
        Ok(value.unwrap())
    };
    let mut count = 0;
    let mut confirm = || {
        let values = spec["confirm"].as_array().unwrap();
        let index = count.min(values.len() - 1);
        count += 1;
        callback(root, "confirm", &values[index], None)
    };
    let available = match spec.get("availability_environment") {
        Some(map) => {
            let environment: HashMap<String, String> = serde_json::from_value(map.clone()).unwrap();
            let environment = environment
                .into_iter()
                .map(|(key, value)| (OsString::from(key), OsString::from(value)))
                .collect();
            keyboard_paste_available(Some(&environment), spec["application"].as_str())
        }
        None => keyboard_paste_available(None, spec["application"].as_str()),
    };
    // The fixture deliberately lets a previously discovered helper disappear before dispatch.
    let mut guarded_authorize = || {
        let value = authorize();
        if let Some(name) = spec["remove_at_authorization"].as_str() {
            fs::remove_file(root.join("bin").join(name)).unwrap();
        }
        value
    };
    let timeout = match spec["timeout"].as_str() {
        Some("nan") => f64::NAN,
        Some("infinity") => f64::INFINITY,
        Some("negative_infinity") => f64::NEG_INFINITY,
        _ => spec["timeout"].as_f64().unwrap_or(0.75),
    };
    let before = Instant::now();
    let result = deliver_text(
        &source,
        spec["auto_paste"] != false,
        DeliveryOptions {
            confirm_paste: spec.get("confirm").map(|_| &mut confirm as _),
            insert_directly: spec.get("direct").map(|_| &mut direct as _),
            authorize_keyboard_paste: spec.get("authorize").map(|_| &mut guarded_authorize as _),
            confirmation_timeout_seconds: timeout,
            application_identifier: spec["application"].as_str(),
        },
    );
    let result = match result {
        Ok(receipt) => json!({"ok":{"copied":receipt.copied,"pasted":receipt.pasted,
            "guidance":receipt.guidance,"paste_dispatched":receipt.paste_dispatched,
            "paste_confirmed":receipt.paste_confirmed,"history_outcome":receipt.history_outcome()}}),
        Err(DeliveryError::Clipboard(_)) => json!({"error_kind":"clipboard"}),
        Err(error) => json!({"error":error.to_string()}),
    };
    let mut events = vec![];
    let mut clipboard_at = None;
    let mut settled_before_keyboard = None;
    if let Ok(lines) = fs::read_to_string(root.join("trace.jsonl")) {
        events = lines
            .lines()
            .map(|line| {
                let mut event: Value = serde_json::from_str(line).unwrap();
                if matches!(event["helper"].as_str(), Some("wl-copy" | "xclip")) {
                    clipboard_at = event["at_ns"].as_u64();
                } else if matches!(
                    event["helper"].as_str(),
                    Some("wtype" | "xdotool" | "ydotool")
                ) {
                    settled_before_keyboard = Some(
                        event["at_ns"].as_u64().unwrap() - clipboard_at.unwrap() >= 110_000_000,
                    );
                }
                event.as_object_mut().unwrap().remove("pid");
                event.as_object_mut().unwrap().remove("at_ns");
                event
            })
            .collect();
    }
    let clipboard = fs::read(root.join("clipboard.bin")).ok();
    let copied_exactly = clipboard.as_deref() == Some(source.as_bytes());
    let no_socket_input = datagram.as_ref().is_none_or(|socket| {
        let mut buffer = [0; 64];
        matches!(socket.recv(&mut buffer), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    });
    drop((datagram, stream));
    println!(
        "{}",
        json!({"available":available,"result":result,"events":events,
        "clipboard_exact":copied_exactly,"helpers_reaped":helpers_reaped(root),
        "settled_before_keyboard":settled_before_keyboard,
        "no_socket_input":no_socket_input,
        "elapsed_ms":before.elapsed().as_secs_f64()*1000.0})
    );
}

fn main() {
    let invocation = std::env::args_os().next().unwrap();
    let inherited_root = std::env::var_os("DELIVERY_FIXTURE_ROOT");
    let root = inherited_root
        .as_deref()
        .map(Path::new)
        .unwrap_or_else(|| Path::new(&invocation).parent().unwrap().parent().unwrap());
    if std::env::args().nth(1).as_deref() == Some("--hold-terminal") {
        terminal_peer::hold(root);
    }
    let name = Path::new(&invocation)
        .file_name()
        .unwrap()
        .to_str()
        .unwrap();
    if matches!(name, "wl-copy" | "xclip" | "wtype" | "xdotool" | "ydotool") {
        run_helper(root, name);
    }
    if name == "hyprctl" {
        terminal_peer::hyprctl(root);
    }
    let mut input = vec![];
    std::io::stdin().read_to_end(&mut input).unwrap();
    let spec: Value = serde_json::from_slice(&input).unwrap();
    if spec["mode"] == "terminal" {
        terminal_peer::driver(root, spec);
    } else {
        driver(root, spec);
    }
}
