//! A synthetic external audio endpoint. It never loads an audio library or opens a device.
use serde_json::{Value, json};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI32, Ordering};
use std::thread;
use std::time::{Duration, Instant};

static SIGNAL: AtomicI32 = AtomicI32::new(0);
extern "C" fn received(signal: i32) {
    SIGNAL.store(signal, Ordering::Relaxed);
}

fn bytes(value: &Value, key: &str) -> Vec<u8> {
    let encoded = value[key].as_str().unwrap_or("");
    assert!(encoded.len().is_multiple_of(2));
    (0..encoded.len())
        .step_by(2)
        .map(|offset| u8::from_str_radix(&encoded[offset..offset + 2], 16).unwrap())
        .collect()
}

fn private_write(path: &Path, value: &[u8]) {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .unwrap()
        .write_all(value)
        .unwrap();
}

fn ready(directory: &Path, name: &str, arguments: &[String]) {
    private_write(
        &directory.join(format!("{name}.ready.json")),
        &serde_json::to_vec(&json!({"argv":arguments,"pid":std::process::id()})).unwrap(),
    );
}

fn pause_until_signal(config: &Value) {
    if config["wait"].as_bool() == Some(true) {
        while SIGNAL.load(Ordering::Relaxed) == 0 {
            thread::sleep(Duration::from_millis(5));
        }
    }
}

fn owner(arguments: &[String], config: &Value) {
    use mluva_audio::meeting::PipeWireMeetingRecorder;
    use mluva_audio::recorder::PipeWireRecorder;
    use mluva_audio::volatile::VolatileAudioStore;
    let mut store = if arguments[3] == "durable" {
        None
    } else {
        Some(VolatileAudioStore::open(Path::new(&arguments[2])).unwrap())
    };
    let directory = match &mut store {
        Some(store) => store.directory().unwrap().to_path_buf(),
        None => Path::new(&arguments[1]).parent().unwrap().join("durable"),
    };
    let watchdog: Option<u32> = store.as_ref().map(|_| {
        let children =
            fs::read_to_string(format!("/proc/self/task/{}/children", std::process::id())).unwrap();
        children.split_whitespace().next().unwrap().parse().unwrap()
    });
    let mut microphone = PipeWireRecorder::new(&arguments[1], None);
    let mut meeting = PipeWireMeetingRecorder::new(&arguments[1], None, None);
    let path = directory.join("capture.wav");
    if arguments[3] == "meeting" {
        meeting.start(&path).unwrap();
    } else {
        microphone.start(&path, None).unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while fs::read_dir(&directory)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.metadata().is_ok_and(|metadata| metadata.len() > 44))
        .count()
        < if arguments[3] == "meeting" { 2 } else { 1 }
    {
        assert!(Instant::now() < deadline, "No synthetic audio arrived");
        thread::sleep(Duration::from_millis(5));
    }
    let modes: Vec<_> = fs::read_dir(&directory).unwrap().filter_map(Result::ok).map(|entry| {
        use std::os::unix::fs::PermissionsExt;
        json!({"name":entry.file_name(),"mode":entry.metadata().unwrap().permissions().mode() & 0o777})
    }).collect();
    println!(
        "{}",
        json!({"directory":directory,"watchdog":watchdog,"files":modes})
    );
    io::stdout().flush().unwrap();
    let mut command = String::new();
    io::stdin().read_line(&mut command).unwrap();
    if command.trim() == "check" {
        println!(
            "{}",
            json!({"error":store.as_mut().unwrap().directory().err().map(|error|error.to_string())})
        );
        io::stdout().flush().unwrap();
    }
    // Keep the configured owner alive for a process-group crash, or drop normally on EOF.
    if config["owner_hang"].as_bool() == Some(true) {
        loop {
            thread::sleep(Duration::from_secs(1));
        }
    }
}

fn main() {
    let mut args = std::env::args();
    let executable = PathBuf::from(args.next().unwrap());
    let arguments: Vec<_> = args.collect();
    let directory = executable.parent().unwrap();
    let config: Value =
        serde_json::from_slice(&fs::read(directory.join("test-config.json")).unwrap()).unwrap();
    if arguments
        .first()
        .is_some_and(|argument| argument == "factory-capture")
    {
        let result = (|| {
            let catalog = mluva_audio::catalog::PipeWireDeviceCatalog::from_system(None)
                .map_err(|error| error.to_string())?;
            let mut recorder = mluva_audio::recorder::PipeWireRecorder::from_system(None)
                .map_err(|error| error.to_string())?;
            recorder
                .start(Path::new(&arguments[1]), None)
                .map_err(|error| error.to_string())?;
            let deadline = Instant::now() + Duration::from_secs(5);
            while !directory.join("raw.ready.json").exists() {
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(5));
            }
            recorder.stop().map_err(|error| error.to_string())?;
            Ok::<_, String>(catalog)
        })();
        let result = match result {
            Ok(catalog) => json!({"ok":catalog}),
            Err(error) => json!({"error":error}),
        };
        println!("{result}");
        return;
    }
    if arguments
        .first()
        .is_some_and(|argument| argument == "owner")
    {
        owner(&arguments, &config);
        return;
    }
    if config["ready_protocol"].is_string() {
        ready(directory, "protocol", &arguments);
        print!("{}", config["ready_protocol"].as_str().unwrap());
        io::stdout().flush().unwrap();
        if config["hang"].as_bool() == Some(true) {
            loop {
                thread::sleep(Duration::from_secs(1));
            }
        }
        return;
    }
    let default_interrupt = config["default_interrupt"].as_bool() == Some(true);
    unsafe {
        if !default_interrupt {
            libc::signal(libc::SIGINT, received as *const () as libc::sighandler_t);
        }
        if config["ignore_interrupt"].as_bool() == Some(true) {
            libc::signal(libc::SIGINT, libc::SIG_IGN);
        }
        libc::signal(libc::SIGTERM, received as *const () as libc::sighandler_t);
    }
    if arguments == ["--no-colors"] {
        let payload = bytes(&config, "dump_hex");
        io::stdout().write_all(&payload).unwrap();
        let count = config["extra_dump_bytes"].as_u64().unwrap_or(0);
        for _ in 0..count / 1_024 {
            io::stdout().write_all(&[b' '; 1_024]).unwrap();
        }
        io::stdout()
            .write_all(&vec![b' '; (count % 1_024) as usize])
            .unwrap();
        io::stdout().flush().unwrap();
        ready(directory, "dump", &arguments);
        pause_until_signal(&config);
        std::process::exit(
            config["dump_exit"]
                .as_i64()
                .or_else(|| config["exit"].as_i64())
                .unwrap_or(0) as i32,
        );
    }
    assert!(arguments.starts_with(&[
        "--rate".into(),
        "16000".into(),
        "--channels".into(),
        "1".into(),
        "--format".into(),
        "s16".into()
    ]));
    if arguments.ends_with(&["--raw".into(), "-".into()]) {
        let payload = bytes(&config, "pcm_hex");
        let sizes: Vec<_> = config["fragments"]
            .as_array()
            .map(|sizes| {
                sizes
                    .iter()
                    .map(|size| size.as_u64().unwrap() as usize)
                    .collect()
            })
            .unwrap_or_else(|| vec![payload.len().max(1)]);
        let mut first = true;
        loop {
            let mut offset = 0;
            let mut index = 0;
            while offset < payload.len() {
                let size = sizes[index % sizes.len()].min(payload.len() - offset);
                assert!(size != 0);
                io::stdout()
                    .write_all(&payload[offset..offset + size])
                    .unwrap();
                io::stdout().flush().unwrap();
                offset += size;
                index += 1;
            }
            if first {
                ready(directory, "raw", &arguments);
                first = false;
            }
            if config["repeat"].as_bool() != Some(true) || SIGNAL.load(Ordering::Relaxed) != 0 {
                break;
            }
            thread::sleep(Duration::from_millis(
                config["interval_ms"].as_u64().unwrap_or(20),
            ));
        }
        io::stderr()
            .write_all(&bytes(&config, "stderr_hex"))
            .unwrap();
        pause_until_signal(&config);
        std::process::exit(config["exit"].as_i64().unwrap_or(0) as i32);
    }
    let captures_system = arguments
        .windows(2)
        .any(|pair| pair[0] == "--properties" && pair[1].contains("stream.capture.sink"));
    let name = if captures_system {
        "system"
    } else {
        "microphone"
    };
    let stream = &config[name];
    if stream["wav_hex"].is_string() {
        private_write(
            Path::new(arguments.last().unwrap()),
            &bytes(stream, "wav_hex"),
        );
    }
    ready(directory, name, &arguments);
    pause_until_signal(stream);
    std::process::exit(stream["exit"].as_i64().unwrap_or(0) as i32);
}
