//! Managed preview interruption through the actual independent GTK process.
use super::*;
use mluva_providers::local_assets::{MODEL_CATALOG, QWEN_RUNTIME};
use rusqlite::types::ValueRef;
#[path = "bootstrap_history_lifecycle.rs"]
mod history_lifecycle;
#[path = "bootstrap_managed_live.rs"]
mod live;
#[path = "bootstrap_meeting.rs"]
mod meeting;

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
    let audio_paths: std::collections::BTreeSet<_> = rows
        .iter()
        .map(|row| row["retained_audio_path"].as_str().unwrap())
        .collect();
    assert_eq!(
        audio_paths.len(),
        rows.len(),
        "distinct captured audio owners"
    );
    let identifier =
        regex::Regex::new(r"^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
            .unwrap();
    let filename = regex::Regex::new(r"^\d{8}T\d{12}\.wav$").unwrap();
    for (index, row) in rows.iter_mut().enumerate() {
        assert!(identifier.is_match(row["identifier"].as_str().unwrap()));
        chrono::DateTime::parse_from_rfc3339(row["created_at"].as_str().unwrap()).unwrap();
        let path = Path::new(row["retained_audio_path"].as_str().unwrap());
        assert!(path.starts_with(root.join("data/mluva/recordings")));
        assert!(filename.is_match(path.file_name().unwrap().to_str().unwrap()));
        let milliseconds = row["recognition_ms"].as_u64().unwrap();
        let sampled = if milliseconds > 2000 {
            assert_eq!(row["delivery_outcome"], "recognition-failed");
            assert!((177_000..=195_000).contains(&milliseconds));
            "$DEADLINE"
        } else {
            "$SAMPLED"
        };
        let suffix = if index == 0 {
            String::new()
        } else {
            format!("-{}", index + 1)
        };
        row["identifier"] = json!(format!("$ENTRY{suffix}"));
        row["created_at"] = json!("$CREATED_AT");
        row["recognition_ms"] = json!(sampled);
        row["retained_audio_path"] =
            json!(format!("$ROOT/data/mluva/recordings/$AUDIO{suffix}.wav"));
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
fn reset_audio_receipts(root: &Path) {
    for file in ["raw.ready.json", "dump.ready.json", "signal.receipt"] {
        let path = root.join("tools").join(file);
        if path.exists() {
            fs::remove_file(path).unwrap();
        }
    }
}

pub fn exercise(
    binary: &Path,
    base: &Path,
    bus: &Bus,
    events: &RefCell<Vec<Value>>,
    meeting_only: bool,
) {
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
    meeting::exercise(binary, base, bus, events, &full, binaries);
    if meeting_only {
        return;
    }
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
            reset_audio_receipts(&root);
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
    recovery(binary, base, bus, events, &fixture, &full, binaries);
    history_lifecycle::exercise(binary, base, bus, events, &fixture, &full, binaries);
    live::exercise(binary, base, bus, events, &fixture, pcm, binaries);
}

fn recovery(
    binary: &Path,
    base: &Path,
    bus: &Bus,
    events: &RefCell<Vec<Value>>,
    capture: &Value,
    full_pcm: &[u8],
    binaries: &Path,
) {
    let fixture: Value = serde_json::from_str(include_str!(
        "../fixtures/released-bootstrap-managed-recovery.json"
    ))
    .unwrap();
    assert_eq!(
        hash(include_bytes!(
            "../fixtures/released-bootstrap-managed-capture.json"
        )),
        fixture["base"]["sha256"]
    );
    let first = &full_pcm[..capture["pcm"]["first_bytes"].as_u64().unwrap() as usize];
    let fresh =
        &full_pcm[full_pcm.len() - fixture["fresh"]["last_bytes"].as_u64().unwrap() as usize..];
    assert_eq!(hash(fresh), fixture["fresh"]["sha256"]);
    let accessibility = Accessibility::open();
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let root = base.join(format!("managed-recovery-{name}"));
        let mut configuration = capture.clone();
        configuration["runtime_spec"] = case["runtime_spec"].clone();
        setup(&root, &configuration, first, binaries, 0);
        let clip = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|path| path.join("xclip"))
            .find(|path| path.is_file())
            .unwrap();
        symlink(clip, root.join("tools/xclip")).unwrap();
        events.borrow_mut().clear();
        let mut process = live::launch(binary, &root, "application.log");
        until(|| bus.owner().is_some() && visible(process.0.id()));
        let window = place_window(process.0.id());
        live::set_clipboard("untouched timeout recovery clipboard");
        let mut first_deadline = None;
        let mut fallback_deadline = None;
        let mut failed = None;
        let mut states = vec![];
        for index in case["states"].as_array().unwrap() {
            let mut expected = fixture["observations"][index.as_u64().unwrap() as usize].clone();
            expected["widgets"] =
                fixture["widgets"][expected["widgets"].as_u64().unwrap() as usize].clone();
            expected["store"] =
                fixture["stores"][expected["store"].as_u64().unwrap() as usize].clone();
            let stage = expected["stage"].as_str().unwrap();
            match stage {
                "recording" | "fresh-recording" => {
                    if stage == "fresh-recording" {
                        set_recording_audio(&root, fresh);
                    }
                    bus.action("record");
                    let end = Instant::now() + Duration::from_secs(15);
                    while !root.join("tools/raw.ready.json").exists() {
                        assert!(Instant::now() < end, "actual complete paced PCM");
                        settle();
                    }
                }
                "processing" => {
                    bus.action("record");
                    until(|| trace(&root).iter().any(|row| row["kind"] == "fragment"));
                    first_deadline = Some(Instant::now());
                }
                "fallback-processing" | "expired" => {
                    let start = if stage == "fallback-processing" {
                        first_deadline.unwrap()
                    } else {
                        fallback_deadline.unwrap_or(first_deadline.unwrap())
                    };
                    let end = start + Duration::from_secs(195);
                    while if stage == "fallback-processing" {
                        resources(&root)["processes"] != 3
                    } else {
                        status(events)[1] != "error"
                    } {
                        assert!(
                            Instant::now() < end,
                            "{name}: {stage}: actual 180-second recognition deadline"
                        );
                        settle();
                    }
                    if name == "incomplete-deadline" {
                        assert!((177.0..=195.0).contains(&start.elapsed().as_secs_f64()));
                    }
                    if stage == "fallback-processing" {
                        fallback_deadline = Some(Instant::now());
                    } else {
                        failed = Some(history(&root)[0].clone());
                    }
                }
                "failure-history" => {
                    let mut spec = fixture["fresh"]["runtime_spec"].clone();
                    spec["root"] = json!(&root);
                    let runtime = QWEN_RUNTIME.binary(&root.join("data/mluva"), "cpu");
                    write(&runtime.with_file_name("fixture.json"), &spec);
                    bus.action("history");
                    until(|| {
                        live::elements(&accessibility)
                            .iter()
                            .any(|node| node.role == "button" && node.name == "Retry transcription")
                    });
                }
                "retried-history" => {
                    click_retry(&accessibility);
                    until(|| {
                        history(&root)[0]["raw_text"] == "hello"
                            && resources(&root)["alive"] == json!([false, false, false, false])
                    });
                }
                "retried-document" => key("Escape"),
                "fresh-terminal" => {
                    bus.action("record");
                    until(|| {
                        history(&root).len() == 2
                            && resources(&root)["alive"]
                                == json!([false, false, false, false, false])
                    });
                    for _ in 0..5 {
                        settle();
                    }
                }
                other => panic!("unknown observed recovery action {other}"),
            }
            let actual = recovery_state(&root, bus, events, &accessibility, &window, stage);
            write(&root.join(format!("recovery-{stage}.json")), &actual);
            assert_eq!(actual, expected, "managed recovery {name}: {stage}");
            if matches!(
                stage,
                "retried-history" | "retried-document" | "fresh-recording" | "fresh-terminal"
            ) {
                let entry = history(&root)[0].clone();
                for field in ["identifier", "created_at", "retained_audio_path"] {
                    assert_eq!(
                        entry[field],
                        failed.as_ref().unwrap()[field],
                        "same recovered recording owner"
                    );
                }
            }
            states.push(actual);
        }
        let fragments: Vec<_> = trace(&root)
            .into_iter()
            .filter(|row| row["kind"] == "fragment")
            .collect();
        if name == "incomplete-deadline" {
            assert!(
                (350..=390).contains(&fragments.iter().filter(|row| row["bytes"] == 1).count())
            );
        }
        bus.action("quit");
        assert_eq!(process.finish(), 0);
        until(|| bus.owner().is_none());
        assert_eq!(fs::read(root.join("application.log")).unwrap(), b"");
        let (mut rows, health_statuses) = protocol(&root);
        rows.retain(|row| row["kind"] != "fragment");
        let actual = json!({"trace":rows,"health_statuses":health_statuses,"exit_code":0,"post_exit_resources":resources(&root),"app_log_bytes":0});
        assert_eq!(
            actual, fixture["protocol"],
            "full independent released transport/recovery contract"
        );
        write(
            &root.join("recovery-observed.json"),
            &json!({"states":states,"protocol":actual}),
        );
        eprintln!(
            "matched managed recovery {name}: {} states, actual expiry/error, retained-audio Retry and fresh capture",
            states.len()
        );
    }
}

fn recovery_state(
    root: &Path,
    bus: &Bus,
    events: &RefCell<Vec<Value>>,
    accessibility: &Accessibility,
    window: &str,
    stage: &str,
) -> Value {
    bus.action("status");
    for _ in 0..2 {
        settle();
    }
    let rows = history(root);
    let dates: Vec<_> = rows
        .iter()
        .map(|row| {
            chrono::DateTime::parse_from_rfc3339(row["created_at"].as_str().unwrap())
                .unwrap()
                .with_timezone(&chrono::Utc)
                .format("%a %-d %b · %H:%M")
                .to_string()
        })
        .collect();
    let normalize = |value: &str| {
        let mut value = value.to_owned();
        for (index, row) in rows.iter().enumerate() {
            let suffix = if index == 0 {
                String::new()
            } else {
                format!("-{}", index + 1)
            };
            value = value
                .replace(
                    row["identifier"].as_str().unwrap(),
                    &format!("$ENTRY{suffix}"),
                )
                .replace(
                    row["retained_audio_path"].as_str().unwrap(),
                    &format!("$ROOT/data/mluva/recordings/$AUDIO{suffix}.wav"),
                );
        }
        for date in &dates {
            value = value.replace(date, "$DATE");
        }
        if regex::Regex::new(r"^Recording [0-9]+:[0-9]+$")
            .unwrap()
            .is_match(&value)
        {
            assert!(
                [
                    "Recording 00:07",
                    "Recording 00:08",
                    "Recording 00:09",
                    "Recording 00:10"
                ]
                .contains(&value.as_str())
            );
            value = "Recording $SAMPLED".into();
        }
        value.replace(root.to_str().unwrap(), "$ROOT")
    };
    let (names, items) = accessibility.visible_content();
    let mut names: Vec<_> = names
        .iter()
        .map(|(role, name)| (role.clone(), normalize(name)))
        .collect();
    names.sort();
    let elements = live::elements(accessibility);
    let mut controls: Vec<_> = elements
        .iter()
        .filter(|node| node.role == "button")
        .map(|node| (normalize(&node.name), node.sensitive))
        .collect();
    controls.sort();
    let texts: Vec<_> = elements
        .iter()
        .filter(|node| node.role == "text box")
        .map(|node| json!({"name":normalize(&node.name),"text":normalize(&node.text)}))
        .collect();
    let mut current = status(events);
    let mut shell = events
        .borrow()
        .iter()
        .rev()
        .find(|row| row["name"] == "ShellStateChanged")
        .unwrap()["values"][0]
        .clone();
    if current[1] == "recording" {
        assert!((7..=10).contains(&current[3].as_u64().unwrap()));
        assert!((0.0..=1.0).contains(&current[6].as_f64().unwrap()));
        current[3] = json!("$SAMPLED");
        current[6] = json!("$SAMPLED");
        assert!((7..=10).contains(&shell["elapsed"].as_u64().unwrap()));
        assert!((0.0..=1.0).contains(&shell["level"].as_f64().unwrap()));
        shell["elapsed"] = json!("$SAMPLED");
        shell["level"] = json!("$SAMPLED");
    }
    if let Some(identifier) = shell.get("identifier") {
        shell["identifier"] = json!(normalize(identifier.as_str().unwrap()));
    }
    let audio = recovery_audio(root, &rows, current[1] == "recording");
    let mut state = json!({"stage":stage,"status":current,"shell":shell,"resources":resources(root),"store":{"history":normalized_history(root),"replies":table(root,"conversation_rewrites","identifier")},"clipboard":live::clipboard(),
        "widgets":{"names":names,"items":items.iter().map(|item|normalize(item)).collect::<Vec<_>>(),"controls":controls,"texts":texts},"retained_audio":audio});
    if stage == "expired" {
        state["frame"] = frame(root, window, stage);
    }
    state
}

fn recovery_audio(root: &Path, rows: &[Value], recording: bool) -> Vec<Value> {
    let mut audio = vec![];
    for file in data_files(&root.join("data/mluva/recordings")) {
        let path = root.join("data/mluva/recordings").join(file);
        if recording
            && !rows
                .iter()
                .any(|row| row["retained_audio_path"].as_str().unwrap() == path.to_str().unwrap())
        {
            continue;
        }
        let mut reader = mluva_audio::wav::WaveReader::open(&path).unwrap();
        assert_eq!(
            (
                reader.metadata.channels,
                reader.metadata.sample_width,
                reader.metadata.sample_rate
            ),
            (1, 2, 16000)
        );
        let pcm = reader.read_frames(128001).unwrap();
        assert_eq!(pcm.len(), 256000);
        let bytes = fs::read(&path).unwrap();
        assert_eq!(bytes.len(), 256044);
        audio.push(json!({"file_sha256":hash(&bytes),"bytes":bytes.len(),"parameters":[1,2,16000],"pcm_bytes":pcm.len(),"pcm_sha256":hash(&pcm)}));
    }
    audio
}

fn set_recording_audio(root: &Path, pcm: &[u8]) {
    reset_audio_receipts(root);
    let mut audio: Value =
        serde_json::from_slice(&fs::read(root.join("tools/test-config.json")).unwrap()).unwrap();
    audio["pcm_hex"] = json!(
        pcm.iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    write(&root.join("tools/test-config.json"), &audio);
}

fn click_retry(accessibility: &Accessibility) {
    let node = live::elements(accessibility)
        .into_iter()
        .find(|node| node.role == "button" && node.name == "Retry transcription")
        .unwrap()
        .node;
    assert_eq!(
        accessibility
            .call(
                &node,
                "org.a11y.atspi.Action",
                "DoAction",
                Some(&(0_i32,).to_variant())
            )
            .unwrap()
            .get::<(bool,)>(),
        Some((true,))
    );
}
