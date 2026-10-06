//! Independently frozen released document/review outcomes on real native transports.
use adw::prelude::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use mluva_core::{
    config::AppConfig,
    conversation::ConversationStore,
    history::{HistoryInput, HistoryStore},
    personalization::PersonalizationStore,
    prompts::PromptStore,
    screenshots::ScreenshotStore,
};
use mluva_gtk::{
    async_runtime::DesktopRuntime,
    capture_view::{CaptureCallbacks, CapturePage},
    conversation_view::{ConversationCallbacks, ConversationWorkspace},
    document_layout::DocumentResources,
    overlay_state::OverlayState,
    review_controller::{ReviewCallbacks, ReviewController},
    rewrite_settings::RewriteSettings,
    theme::ThemeController,
};
use serde_json::{Value, json};
#[path = "support/fixture_states.rs"]
mod fixture_states;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    rc::Rc,
    thread,
    time::{Duration, Instant},
};

fn drain() {
    while glib::MainContext::default().pending() {
        glib::MainContext::default().iteration(false);
    }
}
fn until(mut ready: impl FnMut() -> bool) {
    let limit = Instant::now() + Duration::from_secs(8);
    while !ready() {
        assert!(Instant::now() < limit, "Native review did not settle");
        drain();
        thread::sleep(Duration::from_millis(2));
    }
    drain();
}
fn settle() {
    let end = Instant::now() + Duration::from_millis(40);
    while Instant::now() < end {
        drain();
        thread::sleep(Duration::from_millis(2));
    }
}
fn records(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn wire(path: &Path) -> Vec<String> {
    records(&path.join("requests.jsonl"))
        .into_iter()
        .filter(|event| event["message"]["method"] == "turn/start")
        .map(|event| {
            event["message"]["params"]["input"][0]["text"]
                .as_str()
                .unwrap()
                .into()
        })
        .collect()
}
fn clipboard() -> String {
    let output = Command::new("xclip")
        .args(["-selection", "clipboard", "-o"])
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}
fn canary() {
    use std::io::Write;
    let mut process = Command::new("xclip")
        .args(["-selection", "clipboard"])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    process
        .stdin
        .take()
        .unwrap()
        .write_all(b"Review clipboard canary")
        .unwrap();
    assert!(process.wait().unwrap().success());
}
fn environment() -> PathBuf {
    let root = PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").unwrap())
        .canonicalize()
        .unwrap();
    for key in ["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XAUTHORITY"] {
        assert!(
            PathBuf::from(std::env::var_os(key).unwrap())
                .canonicalize()
                .unwrap()
                .starts_with(&root)
        );
    }
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
    assert!(std::env::var_os("WAYLAND_DISPLAY").is_none());
    assert!(std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none());
    let tools = root.join("review-codex-tools");
    assert_eq!(
        std::env::split_paths(&std::env::var_os("PATH").unwrap()).next(),
        Some(tools.clone())
    );
    let peer = PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap())
        .join("debug/codex-fixture-peer")
        .canonicalize()
        .unwrap();
    let quote = |path: &Path| format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"));
    fs::create_dir(&tools).unwrap();
    fs::set_permissions(&tools, fs::Permissions::from_mode(0o700)).unwrap();
    let script = tools.join("codex");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\nexec {} serve {} \"$@\"\n",
            quote(&peer),
            quote(&root.join("review-codex-fixture.json"))
        ),
    )
    .unwrap();
    fs::set_permissions(script, fs::Permissions::from_mode(0o700)).unwrap();
    root
}
fn page(
    history: HistoryStore,
    config: AppConfig,
    resources: &DocumentResources,
) -> Rc<CapturePage> {
    let workspace = ConversationWorkspace::new(
        ConversationStore::new(history),
        ConversationCallbacks {
            copy: Rc::new(|_| {}),
            rewrite: Rc::new(|_| {}),
            paste: Rc::new(|_| {}),
            open_archive: Rc::new(|| {}),
            save_prompt: Rc::new(|_| {}),
            cancel_rewrite: Rc::new(|| {}),
            rename: Rc::new(|_, _| true),
            delete: Rc::new(|_| true),
            merge: Rc::new(|_, _| true),
            continue_recording: Rc::new(|_| {}),
            capture_screenshot: Some(Rc::new(|| {})),
            edit_screenshot: Rc::new(|_| {}),
            remove_screenshot: Rc::new(|_| {}),
            edit_prompt: Rc::new(|_| {}),
        },
        resources.clone(),
    )
    .unwrap();
    CapturePage::new(
        workspace,
        RewriteSettings::new(config.clone(), Rc::new(|| {}), Rc::new(|_, _, _| {})),
        config,
        CaptureCallbacks {
            toggle_recording: Rc::new(|| {}),
            apply_live_settings: Rc::new(|_| false),
            toast: Rc::new(|_| {}),
            open_prompt: Rc::new(|_| {}),
            retry_initialization: Rc::new(|| {}),
            accept_command: Rc::new(|| {}),
            discard_command: Rc::new(|| {}),
            copy_scratchpad: Rc::new(|| {}),
            delete_scratchpad: Rc::new(|| {}),
            output_changed: Rc::new(|_| {}),
            announce: Rc::new(|_| {}),
        },
    )
    .unwrap()
}
fn documents(widget: &gtk::Widget) -> Vec<String> {
    let mut result = vec![];
    if let Some(view) = widget.downcast_ref::<gtk::TextView>() {
        let buffer = view.buffer();
        result.push(
            buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), true)
                .into(),
        );
    }
    let mut child = widget.first_child();
    while let Some(widget) = child {
        result.extend(documents(&widget));
        child = widget.next_sibling();
    }
    result
}
fn shell(state: &OverlayState, identifiers: &BTreeMap<String, String>) -> Value {
    let values = state
        .shell_values()
        .into_iter()
        .map(|(key, value)| {
            let mut parsed = match value.type_().as_str() {
                "s" => json!(value.get::<String>().unwrap()),
                "u" => json!(value.get::<u32>().unwrap()),
                "b" => json!(value.get::<bool>().unwrap()),
                "d" => json!(value.get::<f64>().unwrap()),
                "a(ss)" => json!(value.get::<Vec<(String, String)>>().unwrap()),
                other => panic!("Unexpected shell type {other}"),
            };
            if key == "identifier" {
                let id = parsed.as_str().unwrap();
                parsed = json!(identifiers.get(id).map_or(id, String::as_str));
            }
            (
                key,
                json!({"signature":value.type_().as_str(),"value":parsed}),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    Value::Object(values)
}
fn snapshot(
    owner: &ReviewController,
    last: &OverlayState,
    identifiers: &BTreeMap<String, String>,
) -> Value {
    let w = &owner.workspace;
    let notice = regex::Regex::new(r"first text [0-9.]+ s")
        .unwrap()
        .replace_all(&w.notice.label(), "first text measured s")
        .into_owned();
    json!({"notice":notice,"prompt":w.prompt_text(),"documents":documents(w.messages.upcast_ref()),"send_sensitive":w.send.get_sensitive(),"cancel_visible":w.cancel.get_visible(),"rewriting":owner.rewriting(),"entry":w.entry().map(|entry|json!({"raw":entry.raw_text,"output":entry.delivered_text})),"review":shell(last,identifiers)})
}
fn compare(root: &Path, name: &str, actual: &Value, expected: &Value) {
    if actual != expected {
        fs::write(
            root.join(format!("{name}.native.json")),
            serde_json::to_vec_pretty(actual).unwrap(),
        )
        .unwrap();
        fs::write(
            root.join(format!("{name}.released.json")),
            serde_json::to_vec_pretty(expected).unwrap(),
        )
        .unwrap();
        panic!(
            "Released review differs: {name}; observations saved under {}",
            root.display()
        );
    }
}

#[test]
#[ignore = "requires private Linux display/buses/network/device namespaces and native Codex peer"]
fn actual_document_rewrites_and_review_actions_match_release() {
    let root = environment();
    adw::init().unwrap();
    let fixture = fixture_states::load(
        include_str!("fixtures/released-review-controller.json"),
        &["cases"],
        "ui",
    );
    assert_eq!(
        fixture["reference_commit"],
        "5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f"
    );
    assert_eq!(
        fixture["gtk"],
        json!([
            gtk::major_version(),
            gtk::minor_version(),
            gtk::micro_version()
        ])
    );
    assert_eq!(fixture["pango"], gtk::pango::version_string().as_str());
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
    gtk::Settings::default()
        .unwrap()
        .set_gtk_cursor_blink(false);
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceLight);
    let resources = DocumentResources::from_directory(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources"),
    );
    let _theme = ThemeController::apply(
        root.join("state/omarchy/current/theme"),
        resources.font.parent().unwrap(),
    )
    .unwrap();
    let runtime = DesktopRuntime::new().unwrap();
    let mut observed = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let directory = tempfile::tempdir_in(&root).unwrap();
        let cfg: AppConfig = serde_json::from_value(case["config"].clone()).unwrap();
        let mut config = cfg.clone();
        let prompts = PromptStore::new(
            directory.path().join("prompts"),
            &cfg.live_rewrite_custom_instructions,
            &[],
        )
        .unwrap();
        let personal_path = directory.path().join("personalization.json");
        if let Some(body) = case["personal"].as_str() {
            fs::write(&personal_path, body).unwrap();
        }
        let mut personal = PersonalizationStore::new(personal_path);
        personal.prompt_store = Some(prompts.clone());
        let personal = Rc::new(RefCell::new(personal));
        let history = HistoryStore::new(directory.path().join("history.sqlite3"));
        history.initialize().unwrap();
        let page = page(history.clone(), cfg.clone(), &resources);
        let workspace = &page.workspace;
        workspace.set_private(cfg.incognito_mode);
        let window = adw::Window::builder()
            .title("Mluva")
            .default_width(1060)
            .default_height(780)
            .build();
        window.set_content(Some(&page.widget));
        window.present();
        settle();
        let text = case["text"].as_str().unwrap();
        let instruction = case["instruction"].as_str().unwrap();
        let final_text = case["final"].as_str().unwrap();
        let part = case["part"].as_str().unwrap();
        let entry = history
            .add(HistoryInput {
                delivery_outcome: "ready".into(),
                ..HistoryInput::dictation(text, text)
            })
            .unwrap();
        let mut identifiers = BTreeMap::from([(entry.identifier.clone(), "$NOTE".into())]);
        let image = case["png"].as_str().map(|png| {
            ScreenshotStore::new(&history.database.path)
                .add(
                    &entry.identifier,
                    &STANDARD.decode(png).unwrap(),
                    false,
                    Some(12.5),
                )
                .unwrap()
        });
        if case["earlier"] == true {
            workspace
                .store
                .append(
                    &entry.identifier,
                    "Earlier edit",
                    final_text,
                    "earlier-model",
                )
                .unwrap();
        }
        if case["edited_source"] == true {
            workspace
                .store
                .save_text(
                    &entry.identifier,
                    "A manually edited source has 78 files.",
                    None,
                )
                .unwrap();
        }
        workspace
            .show_conversation(
                Some(entry.clone()),
                &workspace.store.replies(&entry.identifier).unwrap(),
                false,
            )
            .unwrap();
        workspace.prompt.buffer().set_text(instruction);
        if case["override"] == true {
            let path = prompts.path("rewrite-polish").unwrap();
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, instruction).unwrap();
        }
        let projection = Rc::new(RefCell::new(OverlayState::default()));
        let published = projection.clone();
        let actions = Rc::new(RefCell::new(vec![]));
        let continued = actions.clone();
        let opens = Rc::new(Cell::new(0));
        let opened = opens.clone();
        let busy = case["capture_busy"] == true;
        let owner = ReviewController::new(
            workspace.clone(),
            runtime.clone(),
            cfg,
            prompts,
            personal,
            directory.path().into(),
            ReviewCallbacks {
                capture_busy: Rc::new(move || busy),
                publish: Rc::new(move |state| {
                    published.replace(state);
                }),
                open: Rc::new(move || opened.set(opened.get() + 1)),
                continue_recording: Rc::new(move |_| continued.borrow_mut().push("continue")),
            },
        );
        let evidence = directory.path().join("evidence");
        fs::create_dir(&evidence).unwrap();
        let gate = directory.path().join("start.release");
        let completion = directory.path().join("completion.release");
        let deltas = if case["failed"] == true {
            json!([42])
        } else {
            json!([part, &final_text[part.len()..]])
        };
        let mut spec = json!({"scenario":"clean","evidence":evidence,"document_controls":{instruction:{"gate":gate,"completion_gate":completion,"delta_delay_ms":70,"deltas":deltas}},"catalog":[{"id":"fixture-default","model":"fixture-model","displayName":"Fixture","isDefault":true,"supportedReasoningEfforts":[{"reasoningEffort":"low"}],"serviceTiers":[]}]});
        let model_gate = directory.path().join("model.release");
        if image.is_some() {
            spec["model_gate"] = json!(model_gate);
        }
        fs::write(
            root.join("review-codex-fixture.json"),
            serde_json::to_vec(&spec).unwrap(),
        )
        .unwrap();
        canary();
        owner.publish(&entry.identifier, "ready", "");
        let mut stages = vec![
            json!({"stage":"review-ready","ui":snapshot(&owner,&projection.borrow(),&identifiers)}),
        ];
        let action = case["action"].as_str();
        let no_request = action.is_some_and(|action| !matches!(action, "polish" | "style"));
        if no_request {
            let action = action.unwrap();
            owner.action(
                match action {
                    "wrong-copy" => "copy",
                    "missing-style" => "rewrite",
                    other => other,
                },
                if action == "wrong-copy" {
                    "22222222-2222-4222-8222-222222222222"
                } else {
                    &entry.identifier
                },
                if action == "missing-style" {
                    "missing"
                } else {
                    ""
                },
            );
        } else if let Some(action) = action {
            owner.action(
                "rewrite",
                &entry.identifier,
                if action == "polish" {
                    "polish"
                } else {
                    case["style_id"].as_str().unwrap()
                },
            );
        } else {
            owner.request(instruction);
        }
        if let Some(image) = image {
            until(|| {
                records(&evidence.join("requests.jsonl"))
                    .iter()
                    .any(|event| event["message"]["method"] == "model/list")
            });
            fs::remove_file(&image.path).unwrap();
            stages.push(json!({"stage":"image-removed-before-dispatch","ui":snapshot(&owner,&projection.borrow(),&identifiers)}));
            fs::write(model_gate, "").unwrap();
        }
        let excluded = [
            "disabled",
            "incognito",
            "capture_busy",
            "blank",
            "thinking",
            "fast",
        ]
        .iter()
        .any(|key| case[*key] == true);
        if !no_request && !excluded {
            until(|| wire(&evidence).len() == 1);
            stages.push(json!({"stage":"rewriting","ui":snapshot(&owner,&projection.borrow(),&identifiers)}));
            if case["duplicate"] == true {
                owner.begin(&entry.identifier, "A second instruction must not run.");
            }
            if case["other_review"] == true {
                let text = case["other"].as_str().unwrap();
                let other = history
                    .add(HistoryInput {
                        delivery_outcome: "ready".into(),
                        ..HistoryInput::dictation(text, text)
                    })
                    .unwrap();
                identifiers.insert(other.identifier.clone(), "$OTHER".into());
                owner.publish(&other.identifier, "ready", "");
                owner.begin(&other.identifier, "Another instruction must not run.");
            }
            if case["cancel"] == true {
                owner.action("cancel", &entry.identifier, "");
                stages.push(json!({"stage":"cancelled","ui":snapshot(&owner,&projection.borrow(),&identifiers)}));
                fs::write(&gate, "").unwrap();
                fs::write(&completion, "").unwrap();
            } else {
                fs::write(&gate, "").unwrap();
                if case["failed"] != true {
                    until(|| {
                        documents(workspace.messages.upcast_ref())
                            .last()
                            .is_some_and(|text| text == final_text)
                    });
                    stages.push(json!({"stage":"streamed","ui":snapshot(&owner,&projection.borrow(),&identifiers)}));
                } else {
                    until(|| !owner.rewriting());
                }
                if case["edit_prompt"] == true {
                    workspace
                        .prompt
                        .buffer()
                        .set_text("A later manual instruction must survive.");
                }
                if case["browse_other"] == true {
                    let text = case["other"].as_str().unwrap();
                    let other = history
                        .add(HistoryInput {
                            delivery_outcome: "ready".into(),
                            ..HistoryInput::dictation(text, text)
                        })
                        .unwrap();
                    workspace
                        .show_conversation(Some(other), &[], false)
                        .unwrap();
                }
                if case["delete"] == true {
                    history.delete(&entry.identifier).unwrap();
                }
                if case["privacy"] == true {
                    config.incognito_mode = true;
                    owner.set_config(config.clone());
                }
                if case["config_edit"] == true {
                    config.auto_copy_rewrite = false;
                    owner.set_config(config.clone());
                }
                if case["dismiss"] == true {
                    owner.action("dismiss", &entry.identifier, "");
                }
                stages.push(json!({"stage":"before-completion","ui":snapshot(&owner,&projection.borrow(),&identifiers)}));
                fs::write(&completion, "").unwrap();
            }
        }
        if !no_request {
            until(|| !owner.rewriting());
        }
        until(|| {
            records(&evidence.join("process.jsonl"))
                .iter()
                .all(|process| {
                    !Path::new("/proc")
                        .join(process["pid"].as_i64().unwrap().to_string())
                        .exists()
                })
        });
        settle();
        stages.push(
            json!({"stage":"terminal","ui":snapshot(&owner,&projection.borrow(),&identifiers)}),
        );
        compare(&root, name, &json!(stages), &case["stages"]);
        assert_eq!(
            json!(wire(&evidence)),
            case["prompts"],
            "{name}: prompt bytes"
        );
        let inputs = records(&evidence.join("requests.jsonl"))
            .into_iter()
            .filter(|event| event["message"]["method"] == "turn/start")
            .map(|event| event["message"]["params"]["input"].clone())
            .collect::<Vec<_>>();
        assert_eq!(
            json!(inputs),
            case["inputs"],
            "{name}: complete text/image request bytes"
        );
        let processes = records(&evidence.join("process.jsonl"));
        assert_eq!(
            processes.len() as u64,
            case["processes"].as_u64().unwrap(),
            "{name}: child count"
        );
        for process in processes {
            assert!(!Path::new(process["cwd"].as_str().unwrap()).exists());
        }
        let replies=workspace.store.replies(&entry.identifier).unwrap().iter().map(|reply|json!({"instruction":reply.instruction,"text":reply.text,"model":reply.model})).collect::<Vec<_>>();
        assert_eq!(json!(replies), case["replies"], "{name}: durable reply");
        assert_eq!(
            clipboard(),
            case["clipboard"].as_str().unwrap(),
            "{name}: clipboard"
        );
        assert_eq!(json!(*actions.borrow()), case["actions"]);
        assert_eq!(opens.get(), u32::from(action == Some("open")));
        if action == Some("open") {
            assert!(!workspace.composer.is_visible());
            assert!(
                gtk::prelude::GtkWindowExt::focus(&window)
                    .unwrap()
                    .is_mapped(),
                "Open must not focus the collapsed rewrite field"
            );
            for expanded in [true, false] {
                workspace.rewrite_toggle.set_active(expanded);
                owner.publish(&entry.identifier, "ready", "");
                owner.action("open", &entry.identifier, "");
                settle();
                assert_eq!(workspace.rewrite_toggle.is_active(), expanded);
                assert_eq!(workspace.composer.is_visible(), expanded);
                let focused = gtk::prelude::GtkWindowExt::focus(&window).unwrap();
                assert!(focused.is_mapped(), "Open keeps focus on a visible control");
                if expanded {
                    assert_eq!(focused, workspace.prompt);
                }
            }
        }
        observed += stages.len();
        if case["cancel"] == true {
            fs::remove_file(&gate).unwrap();
            fs::remove_file(&completion).unwrap();
            owner.begin(&entry.identifier, instruction);
            until(|| wire(&evidence).len() == 2);
            drop(owner);
            let deadline = Instant::now() + Duration::from_millis(600);
            while Instant::now() < deadline {
                drain();
                thread::sleep(Duration::from_millis(2));
            }
            assert!(history.find(&entry.identifier).is_ok());
            assert!(
                workspace
                    .store
                    .replies(&entry.identifier)
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(clipboard(), "Review clipboard canary");
            assert!(
                records(&evidence.join("process.jsonl"))
                    .iter()
                    .all(|process| !Path::new("/proc")
                        .join(process["pid"].as_i64().unwrap().to_string())
                        .exists()
                        && !Path::new(process["cwd"].as_str().unwrap()).exists())
            );
            assert_eq!(projection.borrow().phase, "hidden");
            println!(
                "NATIVE_CONTROLLER_EXIT no late reply/clipboard/projection; exact children reaped PASS"
            );
        } else {
            owner.shutdown();
        }
        window.close();
        settle();
        println!("NATIVE_REVIEW {name} {} PASS", stages.len());
    }
    println!(
        "NATIVE_COMPLETE {} document/review transactions, {observed} observations",
        fixture["cases"].as_array().unwrap().len()
    );
}
