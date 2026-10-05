//! Live revisions and final cancellation through independent managed GTK processes.
use super::*;
use std::collections::VecDeque;
use std::os::unix::process::ExitStatusExt;

struct Element {
    node: (String, String),
    name: String,
    role: String,
    sensitive: bool,
    text: String,
}
fn elements(accessibility: &Accessibility) -> Vec<Element> {
    let mut queue = VecDeque::from([(
        "org.a11y.atspi.Registry".into(),
        "/org/a11y/atspi/accessible/root".into(),
    )]);
    let mut result = vec![];
    let mut visited = 0;
    while let Some(node) = queue.pop_front() {
        visited += 1;
        assert!(visited < 3000);
        if let Some(children) =
            accessibility.call(&node, "org.a11y.atspi.Accessible", "GetChildren", None)
        {
            let children = children.child_value(0);
            for index in 0..children.n_children() {
                let child = children.child_value(index);
                queue.push_back((
                    child.child_value(0).str().unwrap().into(),
                    child.child_value(1).str().unwrap().into(),
                ));
            }
        }
        let Some(state) = accessibility
            .call(&node, "org.a11y.atspi.Accessible", "GetState", None)
            .and_then(|value| value.child_value(0).get::<Vec<u32>>())
        else {
            continue;
        };
        if state[0] & (1 << 25) == 0 {
            continue;
        }
        let name = accessibility
            .call(
                &node,
                "org.freedesktop.DBus.Properties",
                "Get",
                Some(&("org.a11y.atspi.Accessible", "Name").to_variant()),
            )
            .unwrap()
            .child_value(0)
            .as_variant()
            .unwrap()
            .str()
            .unwrap()
            .to_owned();
        let role = accessibility
            .call(&node, "org.a11y.atspi.Accessible", "GetRoleName", None)
            .unwrap()
            .child_value(0)
            .str()
            .unwrap()
            .to_owned();
        if !matches!(role.as_str(), "text box" | "button") {
            continue;
        }
        let text = if role == "text box" {
            accessibility
                .call(
                    &node,
                    "org.a11y.atspi.Text",
                    "GetText",
                    Some(&(0_i32, -1_i32).to_variant()),
                )
                .unwrap()
                .child_value(0)
                .str()
                .unwrap()
                .to_owned()
        } else {
            String::new()
        };
        result.push(Element {
            node,
            name,
            role,
            sensitive: state[0] & (1 << 24) != 0,
            text,
        });
    }
    result
}
fn records(root: &Path, name: &str) -> Vec<Value> {
    fs::read_to_string(root.join("codex-evidence").join(name))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn owned(root: &Path) -> Value {
    let peers = records(root, "process.jsonl");
    json!({"qwen":resources(root),"codex":{"processes":peers.len(),
        "alive":peers.iter().map(|row|Path::new(&format!("/proc/{}",row["pid"].as_u64().unwrap())).exists()).collect::<Vec<_>>(),
        "workspaces_exist":peers.iter().map(|row|Path::new(row["cwd"].as_str().unwrap()).exists()).collect::<Vec<_>>()}})
}
fn replies(root: &Path) -> Vec<Value> {
    table(root, "conversation_rewrites", "identifier")
}
fn clipboard() -> String {
    let output = Command::new("xclip")
        .args(["-selection", "clipboard", "-o"])
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}
fn set_clipboard(text: &str) {
    let mut child = Command::new("xclip")
        .args(["-selection", "clipboard"])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(text.as_bytes())
        .unwrap();
    assert!(child.wait().unwrap().success());
}
fn provider(root: &Path, fixture: &Value, edited: bool) {
    let speech = fixture["speech"].as_str().unwrap();
    let draft = fixture["draft"].clone();
    let final_text = format!(
        "{}{}",
        fixture["final"].as_str().unwrap(),
        if edited {
            fixture["edit"].as_str().unwrap()
        } else {
            ""
        }
    );
    write(
        &root.join("codex.json"),
        &json!({"scenario":"clean","evidence":root.join("codex-evidence"),
        "live_controls":{
            format!("preview|{speech}"):{"deltas":[draft]},
            format!("preview|{speech} {speech}"):{"deltas":[draft]},
            format!("final|{speech}"):{"deltas":[final_text],"gate":root.join("codex-evidence").join(fixture["final_gate"].as_str().unwrap_or("final.release"))}}}),
    );
}
fn turns(root: &Path) -> Vec<Value> {
    let mut model = Value::Null;
    let mut result = vec![];
    for row in records(root, "requests.jsonl") {
        let message = &row["message"];
        if message["method"] == "thread/start" {
            model = message["params"]["model"].clone()
        }
        if message["method"] == "turn/start" {
            result.push(json!({"input":message["params"]["input"],"model":model,"effort":message["params"]["effort"]}));
        }
    }
    result
}
fn stored(root: &Path) -> Value {
    let raw_history = history(root);
    let mut saved = replies(root);
    for reply in &mut saved {
        let index = raw_history
            .iter()
            .position(|entry| entry["identifier"] == reply["history_identifier"])
            .unwrap();
        chrono::DateTime::parse_from_rfc3339(reply["created_at"].as_str().unwrap()).unwrap();
        reply["history_identifier"] = json!(if index == 0 {
            "$ENTRY".to_owned()
        } else {
            format!("$ENTRY-{}", index + 1)
        });
        reply["created_at"] = json!("$CREATED_AT");
    }
    json!({"history":normalized_history(root),"replies":saved})
}
fn launch(binary: &Path, root: &Path, name: &str) -> Process {
    // A genuine X11 selection owner can inherit these descriptors past app exit.
    let log = fs::File::create(root.join(name)).unwrap();
    Process(
        application(binary, root)
            .env("HOME", root.join("home"))
            .env("XDG_CACHE_HOME", root.join("cache"))
            .env("TMPDIR", root.join("tmp"))
            .env_remove("LANG")
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", root.join("tools").display()),
            )
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap(),
    )
}
fn snapshot(
    root: &Path,
    bus: &Bus,
    events: &RefCell<Vec<Value>>,
    accessibility: &Accessibility,
    window: &str,
    stage: &str,
) -> Value {
    let mut state = checkpoint(root, bus, events, window, stage);
    let observed = elements(accessibility);
    let texts: Vec<_> = observed
        .iter()
        .filter(|element| element.role == "text box")
        .map(|element| json!({"name":element.name,"text":element.text}))
        .collect();
    let mut controls: Vec<_> = observed
        .iter()
        .filter(|element| {
            element.role == "button"
                && matches!(
                    element.name.as_str(),
                    "Dictate"
                        | "Stop"
                        | "Cancel"
                        | "Continue recording"
                        | "Save edits"
                        | "Copy original"
                        | "Copy"
                )
        })
        .map(|element| (element.name.clone(), element.sensitive))
        .collect();
    controls.sort();
    let raw_history = history(root);
    let review = events
        .borrow()
        .iter()
        .rev()
        .find(|event| event["name"] == "ShellStateChanged")
        .unwrap()["values"][0]
        .clone();
    state["review"] = json!({"phase":review["phase"],"message":review["message"],"preview":review["preview"],"show_copy":review["show_copy"]});
    state["resources"] = owned(root);
    state.as_object_mut().unwrap().shift_remove("history");
    state["store"] = stored(root);
    state["texts"] = json!(texts);
    state["controls"] = json!(controls);
    state["clipboard"] = json!(clipboard());
    let mut audio = vec![];
    for entry in &raw_history {
        let mut wave = mluva_audio::wav::WaveReader::open(Path::new(
            entry["retained_audio_path"].as_str().unwrap(),
        ))
        .unwrap();
        assert_eq!(
            (
                wave.metadata.channels,
                wave.metadata.sample_width,
                wave.metadata.sample_rate
            ),
            (1, 2, 16000)
        );
        let pcm = wave.read_frames(128001).unwrap();
        audio.push(json!({"bytes":pcm.len(),"sha256":hash(&pcm)}));
    }
    if let Some(first) = audio.first() {
        state["retained_pcm"] = first.clone();
    }
    if audio.len() > 1 {
        state["additional_pcm"] = json!(&audio[1..]);
    }
    if matches!(stage, "terminal" | "copied-final" | "fresh-terminal") {
        state["frame"] = frame(root, window, stage)
    }
    write(&root.join(format!("live-{stage}.json")), &state);
    state
}
fn record_draft(root: &Path, bus: &Bus, accessibility: &Accessibility, draft: &str) {
    bus.action("record");
    // Eight seconds of external pacing exceed the UI acknowledgement deadline.
    let end = Instant::now() + Duration::from_secs(15);
    while !root.join("tools/raw.ready.json").exists() {
        assert!(Instant::now() < end);
        settle();
    }
    until(|| {
        elements(accessibility)
            .iter()
            .any(|element| element.name == "Live draft" && element.text == draft)
    });
    for _ in 0..3 {
        settle();
    }
}
pub(super) fn exercise(
    binary: &Path,
    base: &Path,
    bus: &Bus,
    events: &RefCell<Vec<Value>>,
    capture_fixture: &Value,
    pcm: &[u8],
    binaries: &Path,
) {
    let mut fixture: Value = serde_json::from_str(include_str!(
        "../fixtures/released-bootstrap-managed-live.json"
    ))
    .unwrap();
    let restart: Value = serde_json::from_str(include_str!(
        "../fixtures/released-bootstrap-managed-live-restart.json"
    ))
    .unwrap();
    assert_eq!(
        hash(include_bytes!(
            "../fixtures/released-bootstrap-managed-live.json"
        )),
        restart["prefix"]["sha256"].as_str().unwrap()
    );
    let mut states = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == restart["prefix"]["case"])
        .unwrap()["result"]["states"]
        .as_array()
        .unwrap()
        .clone();
    states.extend(restart["result"]["states"].as_array().unwrap().clone());
    let mut case = json!({"name":restart["name"],"result":restart["result"]});
    case["result"]["states"] = json!(states);
    fixture["cases"].as_array_mut().unwrap().push(case);
    fixture["fresh"] = restart["fresh"].clone();
    let crashed: Value = serde_json::from_str(include_str!(
        "../fixtures/released-bootstrap-managed-live-crash.json"
    ))
    .unwrap();
    assert_eq!(crashed["parent"]["sha256"], restart["prefix"]["sha256"]);
    assert_eq!(crashed["speech"], fixture["speech"]);
    assert_eq!(crashed["draft"], fixture["draft"]);
    assert_eq!(crashed["fresh"], fixture["fresh"]);
    let mut result = crashed["result"].clone();
    // Keep the released oracle's surviving process visible. Only this explicit
    // safety repair changes its lifetime observation after SIGKILL/reopening.
    assert_eq!(
        result["crash"]["resources"]["codex"]["alive"],
        json!([false, false, true])
    );
    result["crash"]["resources"]["codex"]["alive"] =
        crashed["native_safety"]["codex_alive_after_crash"].clone();
    let reopened = result["states"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|state| state["stage"] == "reopened")
        .unwrap();
    assert_eq!(
        reopened["resources"]["codex"]["alive"],
        json!([false, false, true])
    );
    reopened["resources"]["codex"]["alive"] =
        crashed["native_safety"]["codex_alive_after_crash"].clone();
    fixture["cases"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":crashed["name"],"clipboard":crashed["clipboard"],"result":result}));
    let mut setup_fixture = capture_fixture.clone();
    for (key, value) in fixture["config_overrides"].as_object().unwrap() {
        setup_fixture["config"][key] = value.clone()
    }
    setup_fixture["runtime_spec"] = fixture["runtime_spec"].clone();
    let accessibility = Accessibility::open();
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let root = base.join(format!("managed-live-{name}"));
        setup(&root, &setup_fixture, pcm, binaries, 0);
        fs::create_dir(root.join("codex-evidence")).unwrap();
        let clip = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|directory| directory.join("xclip"))
            .find(|path| path.is_file())
            .unwrap();
        symlink(clip, root.join("tools/xclip")).unwrap();
        provider(&root, &fixture, false);
        fs::write(
            root.join("tools/codex"),
            format!(
                "#!/bin/sh\nexec '{}' serve '{}' \"$@\"\n",
                binaries.join("codex-fixture-peer").display(),
                root.join("codex.json").display()
            ),
        )
        .unwrap();
        fs::set_permissions(root.join("tools/codex"), fs::Permissions::from_mode(0o700)).unwrap();
        events.borrow_mut().clear();
        if name == "crash-reopen" {
            assert_eq!(
                unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) },
                0
            );
        }
        let mut process = launch(binary, &root, "application.log");
        until(|| bus.owner().is_some() && visible(process.0.id()));
        let mut window = place_window(process.0.id());
        set_clipboard(
            case["clipboard"]
                .as_str()
                .unwrap_or("untouched joined Live clipboard"),
        );
        let mut states = vec![];
        let mut crash = None;
        for expected in case["result"]["states"].as_array().unwrap() {
            let stage = expected["stage"].as_str().unwrap();
            match stage {
                "recording-draft" => {
                    record_draft(
                        &root,
                        bus,
                        &accessibility,
                        fixture["draft"].as_str().unwrap(),
                    );
                }
                "final-pending" => {
                    bus.action("record");
                    until(|| history(&root).len() == 1 && turns(&root).len() == 3);
                }
                "copy-refused" | "copied-final" => {
                    if stage == "copied-final" {
                        set_clipboard("positive Copy control")
                    }
                    let identifier = history(&root)[0]["identifier"].as_str().unwrap().to_owned();
                    bus.call(
                        &bus.owner().unwrap(),
                        OBJECT,
                        "org.gtk.Actions",
                        "Activate",
                        Some(
                            &(
                                "review",
                                vec![("copy", identifier.as_str(), "").to_variant()],
                                BTreeMap::<String, glib::Variant>::new(),
                            )
                                .to_variant(),
                        ),
                    )
                    .unwrap();
                    if stage == "copied-final" {
                        until(|| {
                            clipboard()
                                == format!(
                                    "{}{}",
                                    fixture["final"].as_str().unwrap(),
                                    fixture["edit"].as_str().unwrap()
                                )
                        })
                    }
                }
                "edited-during-final" => {
                    let node = elements(&accessibility)
                        .into_iter()
                        .find(|element| element.role == "text box" && element.name == "Live draft")
                        .unwrap()
                        .node;
                    let edited = format!(
                        "{}{}",
                        fixture["draft"].as_str().unwrap(),
                        fixture["edit"].as_str().unwrap()
                    );
                    assert_eq!(
                        accessibility
                            .call(
                                &node,
                                "org.a11y.atspi.EditableText",
                                "SetTextContents",
                                Some(&(edited.as_str(),).to_variant())
                            )
                            .unwrap()
                            .get::<(bool,)>(),
                        Some((true,))
                    );
                    provider(&root, &fixture, true);
                }
                "cancelled-final" => {
                    let node = elements(&accessibility)
                        .into_iter()
                        .find(|element| element.role == "button" && element.name == "Cancel")
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
                    until(|| owned(&root)["codex"]["alive"] == json!([false, false, false]));
                }
                "terminal" => {
                    fs::write(root.join("codex-evidence/final.release"), []).unwrap();
                    if name == "manual-edit" {
                        until(|| {
                            replies(&root).len() == 1
                                && clipboard()
                                    == format!(
                                        "{}{}",
                                        fixture["final"].as_str().unwrap(),
                                        fixture["edit"].as_str().unwrap()
                                    )
                        });
                    }
                    for _ in 0..3 {
                        settle()
                    }
                }
                "fresh-recording-draft" => {
                    fs::remove_file(root.join("codex-evidence/final.release")).unwrap();
                    let mut spec = fixture["fresh"]["runtime_spec"].clone();
                    spec["root"] = json!(&root);
                    let runtime = QWEN_RUNTIME.binary(&root.join("data/mluva"), "cpu");
                    write(&runtime.with_file_name("fixture.json"), &spec);
                    provider(&root, &fixture["fresh"], false);
                    reset_audio_receipts(&root);
                    record_draft(
                        &root,
                        bus,
                        &accessibility,
                        fixture["fresh"]["draft"].as_str().unwrap(),
                    );
                }
                "abandoned-final-released" => {
                    fs::write(root.join("codex-evidence/final.release"), []).unwrap();
                    for _ in 0..3 {
                        settle();
                    }
                }
                "fresh-final-pending" => {
                    bus.action("record");
                    until(|| history(&root).len() == 2 && turns(&root).len() == 6);
                }
                "fresh-terminal" => {
                    fs::write(root.join("codex-evidence/fresh.release"), []).unwrap();
                    until(|| {
                        replies(&root).len() == 1
                            && clipboard() == fixture["fresh"]["final"].as_str().unwrap()
                    });
                    for _ in 0..3 {
                        settle();
                    }
                }
                "reopened" => {
                    let pid = records(&root, "process.jsonl").last().unwrap()["pid"]
                        .as_i64()
                        .unwrap() as libc::pid_t;
                    process.0.kill().unwrap();
                    assert_eq!(process.0.wait().unwrap().signal(), Some(libc::SIGKILL));
                    until(|| bus.owner().is_none() && !visible(process.0.id()));
                    let mut status = 0;
                    until(|| unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) } == pid);
                    assert_eq!(libc::WTERMSIG(status), libc::SIGKILL);
                    let observed = json!({"resources":owned(&root),"store":stored(&root),"clipboard":clipboard(),
                        "name_released":true,"window_gone":true,"exit_code":-9});
                    write(&root.join("live-crash.json"), &observed);
                    assert_eq!(
                        observed, case["result"]["crash"],
                        "actual crash resources and durable state"
                    );
                    crash = Some(observed);
                    assert_eq!(fs::read(root.join("application.log")).unwrap(), b"");
                    events.borrow_mut().clear();
                    process = launch(binary, &root, "application-reopened.log");
                    until(|| bus.owner().is_some() && visible(process.0.id()));
                    window = place_window(process.0.id());
                    for _ in 0..3 {
                        settle();
                    }
                }
                "old-final-released" => {
                    fs::write(root.join("codex-evidence/final.release"), []).unwrap();
                    for _ in 0..3 {
                        settle();
                    }
                }
                other => panic!("unknown released Live action {other}"),
            }
            let actual = snapshot(&root, bus, events, &accessibility, &window, stage);
            states.push(actual.clone());
            assert_eq!(actual, *expected, "managed Live {name}: {stage}");
        }
        bus.action("quit");
        let exit = process.finish();
        until(|| bus.owner().is_none());
        assert_eq!(exit, 0);
        assert_eq!(fs::read(root.join("application.log")).unwrap(), b"");
        let (trace, health_statuses) = protocol(&root);
        let mut actual = json!({"states":states,"trace":trace,"health_statuses":health_statuses,"turns":turns(&root),
            "post_exit_resources":owned(&root),"exit_code":exit,"app_log_bytes":0});
        if let Some(crash) = crash {
            assert_eq!(
                fs::read(root.join("application-reopened.log")).unwrap(),
                b""
            );
            actual["crash"] = crash;
        }
        write(&root.join("managed-live-observed.json"), &actual);
        assert_eq!(
            actual, case["result"],
            "managed Live {name}: complete process/store/transport and revision ownership"
        );
        eprintln!(
            "matched managed Live {name}: {} states and actual edited/cancelled finalization",
            states.len()
        );
    }
}
