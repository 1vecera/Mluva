//! Meeting through real root settings, capture children, fixed TLS API and archive.
use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::collections::BTreeSet;

fn read(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn archive(root: &Path) -> Value {
    let path = root.join("data/mluva/meetings/meetings.json");
    if path.exists() {
        read(&path)
    } else {
        Value::Null
    }
}
fn capture(root: &Path) -> Vec<Value> {
    ["microphone", "system"]
        .into_iter()
        .filter_map(|name| {
            let path = root.join(format!("tools/{name}.ready.json"));
            path.exists().then(|| {
                let mut row = read(&path);
                row["alive"] =
                    json!(Path::new(&format!("/proc/{}", row["pid"].as_u64().unwrap())).exists());
                row
            })
        })
        .collect()
}
fn memory() -> Vec<PathBuf> {
    let mut paths: Vec<_> = fs::read_dir("/dev/shm")
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    paths.sort();
    paths
}
fn audio(bytes: &[u8]) -> Value {
    assert_eq!(bytes.len(), 256044);
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(&bytes[8..16], b"WAVEfmt ");
    assert_eq!(
        u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize,
        bytes.len() - 8
    );
    assert_eq!(u32::from_le_bytes(bytes[16..20].try_into().unwrap()), 16);
    assert_eq!(u16::from_le_bytes(bytes[20..22].try_into().unwrap()), 1);
    assert_eq!(&bytes[36..40], b"data");
    assert_eq!(
        u32::from_le_bytes(bytes[40..44].try_into().unwrap()) as usize,
        bytes.len() - 44
    );
    json!({"bytes":bytes.len(),"sha256":hash(bytes),
        "parameters":[u16::from_le_bytes(bytes[22..24].try_into().unwrap()),u16::from_le_bytes(bytes[34..36].try_into().unwrap())/8,u32::from_le_bytes(bytes[24..28].try_into().unwrap())],
        "pcm_bytes":bytes.len()-44,"pcm_sha256":hash(&bytes[44..])})
}
fn files(root: &Path) -> Vec<Value> {
    let mut result = vec![];
    for directory in [root.join("data/mluva/meetings"), PathBuf::from("/dev/shm")] {
        for file in data_files(&directory)
            .into_iter()
            .filter(|file| file.ends_with(".wav"))
        {
            let path = directory.join(file);
            let mut observed = audio(&fs::read(&path).unwrap());
            observed["path"] = json!(path);
            result.push(observed);
        }
    }
    result
}
fn exports(root: &Path) -> Value {
    let directory = root.join("data/mluva/exports/meetings");
    let mut files = vec![];
    if directory.exists() {
        let mut paths: Vec<_> = fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        paths.sort();
        for path in paths {
            assert!(path.is_file());
            let bytes = fs::read(&path).unwrap();
            files.push(json!({"path":path,"mode":fs::metadata(&path).unwrap().permissions().mode()&0o777,
                "text":String::from_utf8(bytes.clone()).unwrap(),"bytes":bytes.len(),"sha256":hash(&bytes)}));
        }
    }
    json!({"directory_mode":directory.exists().then(||fs::metadata(&directory).unwrap().permissions().mode()&0o777),"files":files})
}
fn requests(root: &Path) -> Vec<Value> {
    let mut result = vec![];
    for index in 0..2 {
        let path = root.join(format!("proxy/body-{index}.json"));
        if !path.exists() {
            break;
        }
        let receipt = read(&path);
        assert_eq!(receipt["path"], "/v1/speech-to-text");
        let mut fields = receipt["fields"].clone();
        let bytes = STANDARD
            .decode(
                fields
                    .as_object_mut()
                    .unwrap()
                    .shift_remove("file")
                    .unwrap()
                    .as_str()
                    .unwrap(),
            )
            .unwrap();
        fs::write(root.join(format!("proxy/upload-{index}.wav")), &bytes).unwrap();
        result.push(json!({"host":"api.elevenlabs.io","uri":receipt["path"],"method":"POST","authenticated":true,"fields":fields,"audio":audio(&bytes)}));
    }
    result
}
fn normalize(root: &Path, captures: &[Value], archive: &Value, value: &mut Value) {
    if let Some(files) = value
        .get_mut("exports")
        .and_then(|value| value["files"].as_array_mut())
    {
        for file in files {
            file.as_object_mut().unwrap().shift_remove("bytes");
            file.as_object_mut().unwrap().shift_remove("sha256");
        }
    }
    let mut identities = BTreeSet::new();
    let mut volatile = BTreeSet::new();
    let mut replacements = vec![(root.to_str().unwrap().to_owned(), "$ROOT")];
    let uuid = regex::Regex::new(r"[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}").unwrap();
    for row in captures {
        let path = Path::new(
            row["argv"]
                .as_array()
                .unwrap()
                .last()
                .unwrap()
                .as_str()
                .unwrap(),
        );
        identities.extend(
            uuid.find_iter(path.to_str().unwrap())
                .map(|entry| entry.as_str().to_owned()),
        );
        if path.starts_with("/dev/shm") {
            let first = path
                .strip_prefix("/dev/shm")
                .unwrap()
                .components()
                .next()
                .unwrap();
            volatile.insert(
                Path::new("/dev/shm")
                    .join(first)
                    .to_str()
                    .unwrap()
                    .to_owned(),
            );
        }
    }
    if let Some(rows) = archive.as_array() {
        for row in rows {
            let identity = row["id"].as_str().unwrap();
            assert_eq!(uuid.find(identity).unwrap().as_str(), identity);
            identities.insert(identity.to_owned());
            let timestamp = row["timestamp"].as_str().unwrap();
            let parsed = chrono::DateTime::parse_from_rfc3339(timestamp).unwrap();
            assert_eq!(parsed.offset().local_minus_utc(), 0);
            replacements.push((timestamp.to_owned(), "$TIMESTAMP"));
            replacements.push((parsed.format("%a %-d %b · %H:%M").to_string(), "$DATE"));
            replacements.push((
                format!(
                    "mluva-meeting-{}-{}",
                    parsed.format("%Y-%m-%dT%H%M%S"),
                    &identity[..8]
                ),
                "mluva-meeting-$EXPORT_STAMP-$SHORT_MEETING",
            ));
        }
    }
    assert!(identities.len() <= 1 && volatile.len() <= 1);
    replacements.extend(identities.into_iter().map(|value| (value, "$MEETING")));
    replacements.extend(volatile.into_iter().map(|value| (value, "$MEMORY")));
    fn apply(value: &mut Value, replacements: &[(String, &str)]) {
        match value {
            Value::String(text) => {
                for (from, to) in replacements {
                    *text = text.replace(from, to);
                }
            }
            Value::Array(values) => {
                for value in values {
                    apply(value, replacements);
                }
            }
            Value::Object(values) => {
                for value in values.values_mut() {
                    apply(value, replacements);
                }
            }
            _ => {}
        }
    }
    apply(value, &replacements);
}
fn state(
    root: &Path,
    bus: &Bus,
    events: &RefCell<Vec<Value>>,
    accessibility: &Accessibility,
    stage: &str,
    retained: Option<&Value>,
    recording: Option<(Instant, Instant)>,
) -> Value {
    let sample_started = Instant::now();
    bus.action("status");
    settle();
    settle();
    let captures = capture(root);
    let archive = archive(root);
    let mut widgets: Vec<_> = live::inventory(accessibility).into_iter()
        .filter(|node| !node.name.is_empty() || !node.text.is_empty())
        .map(|node| json!({"name":node.name,"role":node.role,"sensitive":node.sensitive,"text":node.has_text.then_some(node.text)})).collect();
    let mut observed = json!({"stage":stage,"status":status(events),"archive":archive,"capture":captures,"audio":files(root),
        "history":{"history":history(root),"replies":table(root,"conversation_rewrites","identifier")},"clipboard":live::clipboard(),
        "memory_directories":memory(),"requests":requests(root),"widgets":widgets});
    if stage.starts_with("archive-") || stage.starts_with("retry-delete-") {
        observed["exports"] = exports(root);
        observed["config"] = read(&root.join("config/mluva/config.json"));
    }
    write(&root.join(format!("meeting-{stage}-raw.json")), &observed);
    if matches!(stage, "recording" | "dictation-guard") {
        let (requested, ready) = recording.expect("actual capture start interval");
        // The timer starts between the input and capture readiness. Allow its
        // one-second GTK tick, and include time spent reading the public tree.
        let minimum = sample_started
            .duration_since(ready)
            .as_secs()
            .saturating_sub(1);
        let maximum = requested.elapsed().as_secs();
        let clock = regex::Regex::new(r"^Meeting ([0-9]{2}):([0-5][0-9]) · ").unwrap();
        let mut seconds = vec![];
        for widget in observed["widgets"].as_array_mut().unwrap() {
            if widget["role"] != "status" {
                continue;
            }
            for field in ["name", "text"] {
                let Some(text) = widget[field].as_str() else {
                    continue;
                };
                let Some(value) = clock.captures(text) else {
                    continue;
                };
                let elapsed =
                    value[1].parse::<u64>().unwrap() * 60 + value[2].parse::<u64>().unwrap();
                assert!(
                    (minimum..=maximum).contains(&elapsed),
                    "{stage}: actual Meeting clock {elapsed} outside sampled {minimum}..={maximum}"
                );
                seconds.push(elapsed);
                widget[field] = json!(text.replacen(&value[0], "Meeting $ELAPSED · ", 1));
            }
        }
        assert!(!seconds.is_empty(), "actual recording timer observed");
        write(
            &root.join(format!("meeting-{stage}-timing.json")),
            &json!({"minimum_seconds":minimum,"maximum_seconds":maximum,"observed_seconds":seconds}),
        );
    }
    for row in observed["capture"].as_array_mut().unwrap() {
        row.as_object_mut().unwrap().shift_remove("pid");
    }
    let identity = if archive.as_array().is_some_and(Vec::is_empty) {
        retained.map_or_else(|| archive.clone(), |owner| json!([owner]))
    } else {
        archive.clone()
    };
    normalize(root, &captures, &identity, &mut observed);
    widgets = observed["widgets"].as_array().unwrap().clone();
    widgets.sort_by_cached_key(|value| serde_json::to_string(value).unwrap());
    observed["widgets"] = json!(widgets);
    observed
}
fn click(accessibility: &Accessibility, name: &str) {
    let node = live::elements(accessibility)
        .into_iter()
        .find(|node| node.role == "button" && node.name == name)
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
            .child_value(0)
            .get::<bool>(),
        Some(true)
    );
}
fn dialog_open(accessibility: &Accessibility) -> bool {
    accessibility.button("Delete permanently").is_some()
}
fn title(accessibility: &Accessibility, value: &str) {
    let nodes: BTreeSet<_> = live::inventory(accessibility)
        .into_iter()
        .filter(|node| node.role == "text box")
        .filter(|node| {
            accessibility
                .call(
                    &node.node,
                    "org.a11y.atspi.Accessible",
                    "GetInterfaces",
                    None,
                )
                .and_then(|value| value.child_value(0).get::<Vec<String>>())
                .is_some_and(|interfaces| {
                    interfaces
                        .iter()
                        .any(|name| name == "org.a11y.atspi.EditableText")
                })
        })
        .map(|node| node.node)
        .collect();
    assert_eq!(nodes.len(), 1, "one editable Meeting title");
    assert_eq!(
        accessibility
            .call(
                nodes.first().unwrap(),
                "org.a11y.atspi.EditableText",
                "SetTextContents",
                Some(&(value,).to_variant()),
            )
            .unwrap()
            .get::<(bool,)>(),
        Some((true,)),
    );
}

fn tls_peer(base: &Path, fixture: &Value, binaries: &Path) -> (PathBuf, Child) {
    let proxy = base.join("meeting-tls-peer");
    fs::create_dir(&proxy).unwrap();
    let certificate_log = fs::File::create(proxy.join("certificate.log")).unwrap();
    assert!(
        Command::new("openssl")
            .args([
                "req",
                "-x509",
                "-newkey",
                "rsa:2048",
                "-noenc",
                "-days",
                "1",
                "-subj",
                "/CN=api.elevenlabs.io",
                "-addext",
                "subjectAltName=DNS:api.elevenlabs.io",
                "-keyout"
            ])
            .arg(proxy.join("key.pem"))
            .arg("-out")
            .arg(proxy.join("cert.pem"))
            .stdout(certificate_log.try_clone().unwrap())
            .stderr(certificate_log)
            .status()
            .unwrap()
            .success()
    );
    let mut responses = vec![];
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let directory = base.join(format!("meeting-{name}/proxy"));
        for index in 0..case["requests"].as_array().unwrap().len() {
            let failure = index == 0 && name != "microphone-fallback";
            responses.push(json!({"route":"speech","status":if failure {503} else {200},"payload":if failure {json!({"detail":"synthetic Meeting unavailable"})} else {fixture["payload"].clone()},
                "expected_path":"/v1/speech-to-text","expected_headers":{"xi-api-key":"synthetic-meeting-key"},
                "wait_after_body":directory.join(format!("release-{index}")),"body_receipt":directory.join(format!("body-{index}.json")),
                "after_body_timeout_ms":60_000}));
        }
    }
    write(&proxy.join("spec.json"), &json!({"responses":responses}));
    let log = fs::File::create(proxy.join("peer.log")).unwrap();
    let peer = Command::new(binaries.join("examples/artifact_https_peer"))
        .arg(&proxy)
        .stdout(log.try_clone().unwrap())
        .stderr(log)
        .spawn()
        .unwrap();
    until(|| proxy.join("ready.json").exists());
    (proxy, peer)
}

pub(super) fn exercise(
    binary: &Path,
    base: &Path,
    bus: &Bus,
    events: &RefCell<Vec<Value>>,
    pcm: &[u8],
    binaries: &Path,
) {
    let fixture: Value =
        serde_json::from_str(include_str!("../fixtures/released-bootstrap-meeting.json")).unwrap();
    let count = fixture["input"]["pcm_bytes"].as_u64().unwrap() as usize;
    let header = fixture["input"]["header_hex"].as_str().unwrap();
    let header: Vec<_> = (0..header.len())
        .step_by(2)
        .map(|offset| u8::from_str_radix(&header[offset..offset + 2], 16).unwrap())
        .collect();
    let waves: Vec<_> = [&pcm[..count], &pcm[pcm.len() - count..]]
        .into_iter()
        .map(|pcm| [header.as_slice(), pcm].concat())
        .collect();
    assert_eq!(hash(&waves[0]), fixture["input"]["first_sha256"]);
    assert_eq!(hash(&waves[1]), fixture["input"]["last_sha256"]);
    let hex = |bytes: &[u8]| {
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };
    let accessibility = Accessibility::open();
    let (tls, mut peer) = tls_peer(base, &fixture, binaries);
    for case in fixture["cases"].as_array().unwrap() {
        assert!(memory().is_empty());
        let name = case["name"].as_str().unwrap();
        let root = base.join(format!("meeting-{name}"));
        for path in [
            "home",
            "config/mluva",
            "config/gtk-4.0",
            "data/mluva",
            "cache",
            "runtime",
            "state",
            "tmp",
            "tools",
            "proxy",
        ] {
            fs::create_dir_all(root.join(path)).unwrap();
        }
        let mut config = fixture["config"].clone();
        config["incognito_mode"] = json!(name == "incognito-failure");
        write(&root.join("config/mluva/config.json"), &config);
        fs::write(
            root.join("config/gtk-4.0/settings.ini"),
            "[Settings]\ngtk-cursor-blink=false\n",
        )
        .unwrap();
        let mut input = json!({"dump_hex":hex(&serde_json::to_vec(&fixture["input"]["catalog"]).unwrap()),
            "microphone":{"wav_hex":hex(&waves[0]),"wait":true},"system":{"wav_hex":hex(&waves[1]),"wait":true}});
        if name == "microphone-fallback" {
            input["system"] = json!({"wait":true,"exit":1});
        }
        write(&root.join("tools/test-config.json"), &input);
        for tool in ["pw-record", "pw-dump"] {
            symlink(
                binaries.join("audio-fixture-peer"),
                root.join("tools").join(tool),
            )
            .unwrap();
        }
        let clip = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|path| path.join("xclip"))
            .find(|path| path.is_file())
            .unwrap();
        symlink(clip, root.join("tools/xclip")).unwrap();
        let proxy = root.join("proxy");
        events.borrow_mut().clear();
        let log = fs::File::create(root.join("application.log")).unwrap();
        let mut process = Process(
            live::command(binary, &root)
                .env("ELEVENLABS_API_KEY", "synthetic-meeting-key")
                .env("OPENAI_API_KEY", "synthetic-openai-key")
                .env("SSL_CERT_FILE", tls.join("cert.pem"))
                .env("https_proxy", "http://127.0.0.1:48118")
                .env("HTTPS_PROXY", "http://127.0.0.1:48118")
                .env("no_proxy", "localhost,127.0.0.1")
                .env("NO_PROXY", "localhost,127.0.0.1")
                .stdout(log.try_clone().unwrap())
                .stderr(log)
                .spawn()
                .unwrap(),
        );
        until(|| bus.owner().is_some() && visible(process.0.id()));
        let window = place_window(process.0.id());
        live::set_clipboard("untouched Meeting clipboard");
        bus.action("meeting");
        until(|| accessibility.button("Start Meeting capture").is_some());
        let mut states = vec![];
        let mut retained = None;
        let mut recording = None;
        for index in case["states"].as_array().unwrap() {
            let mut expected = fixture["observations"][index.as_u64().unwrap() as usize].clone();
            let widgets = &fixture["widgets"][expected["widgets"].as_u64().unwrap() as usize];
            expected["widgets"] = json!(
                widgets
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|index| fixture["widget_items"][index.as_u64().unwrap() as usize].clone())
                    .collect::<Vec<_>>()
            );
            if let Some(index) = expected["exports"].as_u64() {
                expected["exports"] = fixture["exports"][index as usize].clone();
                expected["config"] = fixture["config"].clone();
            }
            let stage = expected["stage"].as_str().unwrap().to_owned();
            match stage.as_str() {
                "initial" => {}
                "recording" => {
                    let requested = Instant::now();
                    click(&accessibility, "Start Meeting capture");
                    until(|| capture(&root).len() == 2);
                    recording = Some((requested, Instant::now()));
                    let deadline = Instant::now() + Duration::from_secs(8);
                    while Instant::now() < deadline {
                        settle();
                    }
                }
                "dictation-guard" | "processing-dictation-guard" | "retry-dictation-guard" => {
                    bus.action("record")
                }
                "processing" => {
                    click(&accessibility, "Stop and transcribe Meeting");
                    until(|| proxy.join("body-0.json").exists());
                }
                "terminal" | "recovered" => {
                    fs::write(
                        proxy.join(if stage == "terminal" {
                            "release-0"
                        } else {
                            "release-1"
                        }),
                        b"release",
                    )
                    .unwrap();
                    until(|| accessibility.button("Start Meeting capture").is_some());
                }
                "expanded-failure" => {
                    assert!(
                        accessibility
                            .visible_content()
                            .1
                            .contains(&"Transcription failed — audio retained".into())
                    );
                    let geometry = Command::new("xdotool")
                        .args(["getwindowgeometry", "--shell", &window])
                        .output()
                        .unwrap();
                    assert!(geometry.status.success());
                    let text = String::from_utf8(geometry.stdout).unwrap();
                    let geometry: BTreeMap<_, _> = text
                        .lines()
                        .filter_map(|line| line.split_once('='))
                        .collect();
                    assert_eq!((geometry["WIDTH"], geometry["HEIGHT"]), ("1100", "800"));
                    let x = (geometry["X"].parse::<i32>().unwrap() + 850).to_string();
                    let y = (geometry["Y"].parse::<i32>().unwrap() + 548).to_string();
                    assert!(
                        Command::new("xdotool")
                            .args(["mousemove", &x, &y, "click", "1"])
                            .status()
                            .unwrap()
                            .success()
                    );
                    settle();
                    settle();
                    assert!(
                        Command::new("xdotool")
                            .args([
                                "mousemove",
                                "700",
                                "650",
                                "click",
                                "--repeat",
                                "10",
                                "--delay",
                                "40",
                                "5",
                                "mousemove",
                                "1270",
                                "890"
                            ])
                            .status()
                            .unwrap()
                            .success()
                    );
                }
                "retrying" => {
                    click(&accessibility, "Retry transcription");
                    until(|| proxy.join("body-1.json").exists());
                }
                "retry-delete-dialog"
                | "archive-delete-dialog"
                | "archive-delete-confirm-dialog" => {
                    click(&accessibility, "Delete");
                    until(|| dialog_open(&accessibility));
                }
                "retry-delete-guard" | "archive-deleted" => {
                    click(&accessibility, "Delete permanently");
                    until(|| !dialog_open(&accessibility));
                    if stage == "archive-deleted" {
                        until(|| archive(&root) == json!([]) && files(&root).is_empty());
                    }
                }
                "archive-delete-cancel" => {
                    click(&accessibility, "Cancel");
                    until(|| !dialog_open(&accessibility));
                }
                "archive-title-edit" => title(&accessibility, "  Release review — česky 📝  "),
                "archive-title-clear-edit" => title(&accessibility, " \u{2003} \t "),
                "archive-title-saved" | "archive-title-cleared" => {
                    click(&accessibility, "Save title");
                    let expected_title = if stage == "archive-title-saved" {
                        json!("Release review — česky 📝")
                    } else {
                        Value::Null
                    };
                    until(|| archive(&root)[0]["title"] == expected_title);
                }
                "archive-copy" => {
                    click(&accessibility, "Copy transcript");
                    until(|| live::clipboard() == fixture["payload"]["text"].as_str().unwrap());
                }
                "archive-notice-dismissed" => {
                    click(&accessibility, "Dismiss");
                    until(|| accessibility.button("Dismiss").is_none());
                }
                "archive-export-markdown" | "archive-reexport-markdown" => {
                    click(&accessibility, "Export Markdown");
                    until(|| {
                        exports(&root)["files"]
                            .as_array()
                            .is_some_and(|files| !files.is_empty())
                    });
                }
                "archive-export-json" | "archive-reexport-json" => {
                    click(&accessibility, "Export JSON");
                    until(|| {
                        exports(&root)["files"]
                            .as_array()
                            .is_some_and(|files| files.len() == 2)
                    });
                }
                other => panic!("unknown released Meeting stage {other}"),
            }
            let actual = state(
                &root,
                bus,
                events,
                &accessibility,
                &stage,
                retained.as_ref(),
                recording,
            );
            frame(&root, &window, &stage);
            if name == "retained-retry" && stage == "terminal" {
                retained = Some(archive(&root)[0].clone());
            }
            if let Some(original) = &retained {
                if stage == "archive-deleted" {
                    assert_eq!(archive(&root), json!([]));
                } else {
                    let current = archive(&root)[0].clone();
                    for field in ["id", "timestamp", "recordingFilename"] {
                        assert_eq!(
                            current[field], original[field],
                            "same original Meeting recovery owner"
                        );
                    }
                }
            }
            write(&root.join(format!("meeting-{stage}.json")), &actual);
            if name == "incognito-failure" && stage == "terminal" {
                assert_eq!(
                    case["native_improvements"],
                    json!([{"stage":"terminal","path":["memory_directories"],"source":["$MEMORY"],"native":[]}])
                );
                assert_eq!(expected["memory_directories"], json!(["$MEMORY"]));
                expected["memory_directories"] = json!([]);
            }
            assert_eq!(actual, expected, "assembled Meeting {name}: {stage}");
            states.push(actual);
        }
        let captures = capture(&root);
        bus.action("quit");
        let exit = process.finish();
        until(|| bus.owner().is_none());
        let mut observed = json!({"exit_code":exit,"application_name_absent":bus.owner().is_none(),"app_log_bytes":fs::metadata(root.join("application.log")).unwrap().len(),
            "capture":capture(&root),"audio":files(&root),"archive":archive(&root),"memory_directories":memory()});
        let mut expected = case["exit"].clone();
        if let Some(index) = expected["exports"].as_u64() {
            expected["exports"] = fixture["exports"][index as usize].clone();
            expected["config"] = fixture["config"].clone();
            observed["exports"] = exports(&root);
            observed["config"] = read(&root.join("config/mluva/config.json"));
        }
        write(&root.join("meeting-quit-raw.json"), &observed);
        for row in observed["capture"].as_array_mut().unwrap() {
            row.as_object_mut().unwrap().shift_remove("pid");
        }
        let current = archive(&root);
        let identity = if current.as_array().is_some_and(Vec::is_empty) {
            retained
                .as_ref()
                .map_or_else(|| current.clone(), |owner| json!([owner]))
        } else {
            current
        };
        normalize(&root, &captures, &identity, &mut observed);
        assert_eq!(
            observed, expected,
            "acknowledged Meeting Quit preserves archive/audio and reaps capture/RAM"
        );
        assert_eq!(json!(requests(&root)), case["requests"]);
        write(
            &root.join("meeting-observed.json"),
            &json!({"states":states,"exit":observed,"requests":requests(&root),"proxy_log_bytes":0}),
        );
        eprintln!(
            "matched assembled Meeting {name}: {} states, actual fixed TLS/audio/archive/guards and quit cleanup",
            states.len()
        );
    }
    fs::write(tls.join("stop"), b"stop").unwrap();
    until(|| peer.try_wait().unwrap().is_some());
    assert!(peer.wait().unwrap().success());
    assert_eq!(fs::metadata(tls.join("peer.log")).unwrap().len(), 0);
}
