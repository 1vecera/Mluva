//! Managed preview interruption through the actual independent GTK process.
use super::*;
use mluva_providers::local_assets::{MODEL_CATALOG, QWEN_RUNTIME};
use rusqlite::types::ValueRef;
#[path = "bootstrap_managed_live.rs"]
mod live;

fn write(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}
fn hash(bytes: &[u8]) -> String {
    glib::compute_checksum_for_data(glib::ChecksumType::Sha256, bytes)
        .unwrap()
        .into()
}
fn trace(root: &Path) -> Vec<Value> {
    fs::read_to_string(root.join("trace.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn resources(root: &Path) -> Value {
    let rows = trace(root);
    let starts: Vec<_> = rows.iter().filter(|row| row["kind"] == "start").collect();
    json!({"processes":starts.len(),
        "alive":starts.iter().map(|row|Path::new(&format!("/proc/{}",row["pid"].as_u64().unwrap())).exists()).collect::<Vec<_>>(),
        "keys_exist":starts.iter().map(|row|Path::new(row["key_path"].as_str().unwrap()).exists()).collect::<Vec<_>>()})
}
fn history(root: &Path) -> Vec<Value> {
    table(root, "transcription_history", "created_at")
}
fn table(root: &Path, name: &str, order: &str) -> Vec<Value> {
    let path = root.join("data/mluva/history.sqlite3");
    if !path.exists() {
        return vec![];
    }
    let store = rusqlite::Connection::open(path).unwrap();
    let mut query = store
        .prepare(&format!("SELECT * FROM {name} ORDER BY {order}"))
        .unwrap();
    let names: Vec<_> = query
        .column_names()
        .iter()
        .map(|name| name.to_string())
        .collect();
    query
        .query_map([], |row| {
            let mut value = serde_json::Map::new();
            for (index, name) in names.iter().enumerate() {
                let field = match row.get_ref(index)? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Integer(number) => json!(number),
                    ValueRef::Real(number) => json!(number),
                    ValueRef::Text(text) => json!(std::str::from_utf8(text).unwrap()),
                    ValueRef::Blob(_) => panic!("unexpected history blob"),
                };
                value.insert(name.clone(), field);
            }
            Ok(Value::Object(value))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
}
fn protocol(root: &Path) -> (Vec<Value>, Vec<u64>) {
    let mut rows = trace(root);
    let mut statuses = vec![];
    rows.retain_mut(|row| {
        if row["kind"] == "health" {
            assert_eq!(row["authenticated"], false);
            statuses.push(row["status"].as_u64().unwrap());
            false
        } else {
            if row["kind"] == "start" {
                row.as_object_mut().unwrap().shift_remove("pid");
                row.as_object_mut().unwrap().shift_remove("key_path");
            }
            true
        }
    });
    statuses.sort();
    statuses.dedup();
    assert!(!statuses.is_empty());
    (rows, statuses)
}
fn normalized_history(root: &Path) -> Value {
    let mut rows = history(root);
    let identifier =
        regex::Regex::new(r"^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
            .unwrap();
    let filename = regex::Regex::new(r"^\d{8}T\d{12}\.wav$").unwrap();
    for row in &mut rows {
        assert!(identifier.is_match(row["identifier"].as_str().unwrap()));
        chrono::DateTime::parse_from_rfc3339(row["created_at"].as_str().unwrap()).unwrap();
        let path = Path::new(row["retained_audio_path"].as_str().unwrap());
        assert!(path.starts_with(root.join("data/mluva/recordings")));
        assert!(filename.is_match(path.file_name().unwrap().to_str().unwrap()));
        assert!(row["recognition_ms"].as_u64().unwrap() <= 2000);
        row["identifier"] = json!("$ENTRY");
        row["created_at"] = json!("$CREATED_AT");
        row["recognition_ms"] = json!("$SAMPLED");
        row["retained_audio_path"] = json!("$ROOT/data/mluva/recordings/$AUDIO.wav");
    }
    json!(rows)
}
fn status(events: &RefCell<Vec<Value>>) -> Value {
    events
        .borrow()
        .iter()
        .rev()
        .find(|row| row["name"] == "StateChanged")
        .unwrap()["values"]
        .clone()
}
fn place_window(pid: u32) -> String {
    let output = Command::new("xdotool")
        .args(["search", "--onlyvisible", "--pid", &pid.to_string()])
        .output()
        .unwrap();
    let window = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .to_owned();
    assert!(
        Command::new("xdotool")
            .args([
                "windowsize",
                &window,
                "1100",
                "800",
                "windowmove",
                &window,
                "30",
                "30",
                "windowactivate",
                "--sync",
                &window,
            ])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("xdotool")
            .args(["mousemove", "1270", "890"])
            .status()
            .unwrap()
            .success()
    );
    settle();
    window
}
fn frame(root: &Path, window: &str, stage: &str) -> Value {
    let path = root.join(format!("managed-{stage}.png"));
    assert!(
        Command::new("/usr/bin/import")
            .args(["-window", window])
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let image = gtk::gdk_pixbuf::Pixbuf::from_file(path).unwrap();
    json!({"width":image.width(),"height":image.height(),"channels":image.n_channels(),
        "sha256":hash(image.read_pixel_bytes().as_ref())})
}
fn checkpoint(
    root: &Path,
    bus: &Bus,
    events: &RefCell<Vec<Value>>,
    window: &str,
    stage: &str,
) -> Value {
    bus.action("status");
    settle();
    let mut current = status(events);
    assert_eq!(current.as_array().unwrap().len(), 9);
    if current[1] == "recording" {
        assert!((7..=10).contains(&current[3].as_u64().unwrap()));
        assert!((0.0..=1.0).contains(&current[6].as_f64().unwrap()));
        current[3] = json!("$SAMPLED");
        current[6] = json!("$SAMPLED");
    }
    let mut state = json!({"stage":stage,"status":current,"resources":resources(root),"history":normalized_history(root)});
    if stage == "cancelled" {
        state["retained_audio"] = json!(data_files(&root.join("data/mluva/recordings")));
    }
    if matches!(stage, "cancelled" | "final") {
        state["frame"] = frame(root, window, stage);
    }
    if stage == "final" {
        let rows = history(root);
        assert_eq!(rows.len(), 1);
        let path = Path::new(rows[0]["retained_audio_path"].as_str().unwrap());
        let mut wave = mluva_audio::wav::WaveReader::open(path).unwrap();
        assert_eq!(
            (
                wave.metadata.channels,
                wave.metadata.sample_width,
                wave.metadata.sample_rate
            ),
            (1, 2, 16000)
        );
        let pcm = wave.read_frames(128001).unwrap();
        state["retained_pcm_sha256"] = json!(hash(&pcm));
        state["retained_pcm_bytes"] = json!(pcm.len());
    }
    state
}
fn setup(root: &Path, fixture: &Value, pcm: &[u8], binaries: &Path, close_delay_ms: u64) {
    for name in [
        "home",
        "config/mluva",
        "config/gtk-4.0",
        "data/mluva",
        "cache",
        "runtime",
        "state",
        "tmp",
        "tools",
    ] {
        fs::create_dir_all(root.join(name)).unwrap();
    }
    write(&root.join("config/mluva/config.json"), &fixture["config"]);
    fs::write(
        root.join("config/gtk-4.0/settings.ini"),
        "[Settings]\ngtk-cursor-blink=false\n",
    )
    .unwrap();
    for name in ["pw-record", "pw-dump"] {
        symlink(
            binaries.join("audio-fixture-peer"),
            root.join("tools").join(name),
        )
        .unwrap();
    }
    let encoded: String = pcm.iter().map(|byte| format!("{byte:02x}")).collect();
    write(
        &root.join("tools/test-config.json"),
        &json!({"pcm_hex":encoded,"fragments":[320],
        "bytes_per_second":32000,"wait":true,"dump_hex":"5b5d","signal_receipt":root.join("tools/signal.receipt"),
        "finalize_delay_ms":close_delay_ms}),
    );
    let data = root.join("data/mluva");
    let model = MODEL_CATALOG
        .iter()
        .find(|model| model.id == "qwen3-1.7b")
        .unwrap();
    let path = model.path(&data);
    fs::create_dir_all(&path).unwrap();
    for file in &model.files {
        fs::File::create(path.join(&file.name))
            .unwrap()
            .set_len(file.size)
            .unwrap();
    }
    fs::write(path.join(".ready"), &model.revision).unwrap();
    let runtime = QWEN_RUNTIME.binary(&data, "cpu");
    fs::create_dir_all(runtime.parent().unwrap()).unwrap();
    fs::copy(binaries.join("qwen-fixture-peer"), &runtime).unwrap();
    fs::set_permissions(&runtime, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(
        data.join("qwen-runtime/cpu/.ready"),
        &QWEN_RUNTIME.assets["cpu"].sha256,
    )
    .unwrap();
    let mut spec = fixture["runtime_spec"].clone();
    spec["root"] = json!(root);
    write(&runtime.with_file_name("fixture.json"), &spec);
}

pub fn exercise(binary: &Path, base: &Path, bus: &Bus, events: &RefCell<Vec<Value>>) {
    let fixture: Value = serde_json::from_str(include_str!(
        "../fixtures/released-bootstrap-managed-capture.json"
    ))
    .unwrap();
    let source =
        PathBuf::from(std::env::var_os("MLUVA_TEST_QWEN_PCM").expect("pinned public PCM required"));
    let full = fs::read(source).unwrap();
    assert_eq!(hash(&full), fixture["pcm"]["source_sha256"]);
    let pcm = &full[..fixture["pcm"]["first_bytes"].as_u64().unwrap() as usize];
    assert_eq!(hash(pcm), fixture["pcm"]["sha256"]);
    let binaries = Path::new(env!("CARGO_BIN_EXE_mluva")).parent().unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    for (case, close_delay_ms) in [(&cases[0], 0), (&cases[1], 0), (&cases[1], 500)] {
        let name = case["name"].as_str().unwrap();
        // Repeat Cancel with a delayed external microphone close. A queued
        // widget tick must preserve recording feedback while cleanup drains.
        let root = base.join(format!("managed-{name}-{close_delay_ms}"));
        setup(&root, &fixture, pcm, binaries, close_delay_ms);
        events.borrow_mut().clear();
        let mut process = Process(
            application(binary, &root)
                .env("HOME", root.join("home"))
                .env("XDG_CACHE_HOME", root.join("cache"))
                .env("TMPDIR", root.join("tmp"))
                .env_remove("LANG")
                .env(
                    "PATH",
                    format!("{}:/usr/bin:/bin", root.join("tools").display()),
                )
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        until(|| bus.owner().is_some() && visible(process.0.id()));
        let window = place_window(process.0.id());
        events.borrow_mut().clear();
        bus.action("record");
        until(|| trace(&root).iter().any(|row| row["kind"] == "request"));
        until(|| root.join("tools/raw.ready.json").exists());
        let mut states = vec![checkpoint(&root, bus, events, &window, "pending")];
        bus.action(if name == "cancel" { "cancel" } else { "record" });
        if name == "cancel" {
            until(|| {
                status(events)[1] == "hidden"
                    && resources(&root)["alive"] == json!([false])
                    && resources(&root)["keys_exist"] == json!([false])
            });
            settle();
            states.push(checkpoint(&root, bus, events, &window, "cancelled"));
            for file in ["raw.ready.json", "dump.ready.json", "signal.receipt"] {
                let path = root.join("tools").join(file);
                if path.exists() {
                    fs::remove_file(path).unwrap();
                }
            }
            bus.action("record");
            until(|| resources(&root)["processes"] == 2);
            until(|| root.join("tools/raw.ready.json").exists());
            states.push(checkpoint(&root, bus, events, &window, "fresh_recording"));
            bus.action("record");
        }
        until(|| {
            history(&root).len() == 1
                && status(events)[1] == "hidden"
                && resources(&root)["alive"] == json!([false, false])
        });
        for _ in 0..5 {
            settle();
        }
        states.push(checkpoint(&root, bus, events, &window, "final"));
        let phases: Vec<_> = events
            .borrow()
            .iter()
            .filter(|row| row["name"] == "StateChanged")
            .map(|row| row["values"][1].as_str().unwrap().to_owned())
            .collect();
        let mut phases = phases[phases
            .iter()
            .position(|phase| phase == "preparing")
            .unwrap()..]
            .to_vec();
        phases.dedup();
        bus.action("quit");
        let output = process.output();
        until(|| bus.owner().is_none());
        assert_eq!(output["stdout"], "");
        assert_eq!(output["stderr"], "");
        let (rows, statuses) = protocol(&root);
        let actual = json!({"states":states,"trace":rows,"phases":phases,"health_statuses":statuses,
            "health_requests_unauthenticated":true,"exit_code":output["exit"],"post_exit_resources":resources(&root),"app_log_bytes":0});
        write(&root.join("managed-observed.json"), &actual);
        assert_eq!(
            actual, case["result"],
            "managed Qwen {name}: public GTK process, active preview handoff/recovery and steady window pixels"
        );
        eprintln!(
            "matched managed Qwen {name} (close delay {close_delay_ms} ms): {} states and actual owned-runtime cleanup",
            states.len()
        );
    }
    live::exercise(binary, base, bus, events, &fixture, pcm, binaries);
}
