//! Independent release comparison through the assembled application, real actions and I/O.
use adw::prelude::*;
use mluva_core::config::{AppConfig, AppPaths};
use mluva_gtk::{
    application::{ApplicationDesktop, ApplicationPlatform},
    async_runtime::DesktopRuntime,
    document_layout::DocumentResources,
    theme::ThemeController,
};
use mluva_workflows::{
    capture::CapturePhase,
    services::{ApplicationServices, NativeBinaries},
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::Command,
    rc::Rc,
    thread,
    time::{Duration, Instant},
};
#[path = "support/accessibility.rs"]
#[allow(dead_code)]
mod accessibility;
#[path = "support/application_commands.rs"]
mod application_commands;
#[path = "support/application_compact.rs"]
mod application_compact;
#[path = "support/application_continuation.rs"]
mod application_continuation;
#[path = "support/application_images.rs"]
mod application_images;
#[path = "support/application_live_editor.rs"]
mod application_live_editor;
#[path = "support/application_live_workspace.rs"]
mod application_live_workspace;
#[path = "support/application_management.rs"]
mod application_management;
#[path = "support/application_onboarding.rs"]
mod application_onboarding;
#[path = "support/application_prompts.rs"]
mod application_prompts;
#[path = "support/application_providers.rs"]
mod application_providers;
#[path = "support/capture_ui.rs"]
#[allow(dead_code)]
mod capture_ui;
#[path = "../../mluva-workflows/tests/support/http.rs"]
mod http;
#[path = "support/screenshot_wire.rs"]
mod screenshot_wire;
fn drain() {
    while glib::MainContext::default().pending() {
        glib::MainContext::default().iteration(false);
    }
}
#[track_caller]
fn until(mut predicate: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(8);
    while !predicate() {
        assert!(Instant::now() < end, "Application did not settle");
        drain();
        thread::sleep(Duration::from_millis(2));
    }
    drain();
}
fn settle() {
    settle_for(Duration::from_millis(40));
}
fn settle_for(duration: Duration) {
    let end = Instant::now() + duration;
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
fn clipboard() -> Option<String> {
    let value = Rc::new(RefCell::new(None));
    let copied = value.clone();
    gtk::gdk::Display::default()
        .unwrap()
        .clipboard()
        .read_text_async(None::<&gio::Cancellable>, move |result| {
            *copied.borrow_mut() = Some(result.unwrap().map(String::from));
        });
    until(|| value.borrow().is_some());
    value.borrow_mut().take().unwrap()
}
fn widgets(widget: &impl IsA<gtk::Widget>) -> Vec<gtk::Widget> {
    let mut result = vec![widget.as_ref().clone()];
    let mut child = widget.as_ref().first_child();
    while let Some(current) = child {
        result.extend(widgets(&current));
        child = current.next_sibling();
    }
    result
}
fn window_id() -> String {
    let result = Command::new("xdotool")
        .args(["search", "--onlyvisible", "--name", "^Mluva$"])
        .output()
        .unwrap();
    assert!(result.status.success());
    let windows = String::from_utf8(result.stdout).unwrap();
    let windows = windows.lines().collect::<Vec<_>>();
    assert_eq!(windows.len(), 1);
    windows[0].into()
}
fn capture_window(path: &Path) {
    assert!(
        Command::new("import")
            .args(["-window", &window_id()])
            .arg(path)
            .status()
            .unwrap()
            .success()
    );
}
fn release_application(owner: Rc<ApplicationDesktop>, application: adw::Application) {
    // WebKit tears down asynchronously while the GTK owner loop still runs.
    let released = Rc::downgrade(&owner);
    drop(owner);
    drop(application);
    until(|| released.upgrade().is_none());
    settle_for(Duration::from_millis(500));
}
fn shutdown(
    owner: &Rc<ApplicationDesktop>,
    platform_closed: &Cell<bool>,
    evidence: &Path,
    pids: &[u64],
) {
    let drained = Rc::new(Cell::new(false));
    let done = drained.clone();
    let shutdown = owner.shutdown();
    glib::MainContext::default().spawn_local(async move {
        shutdown.await.unwrap();
        done.set(true);
    });
    until(|| drained.get());
    settle();
    assert!(platform_closed.get());
    for pid in pids {
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
    }
    for child in records(&evidence.join("process.jsonl")) {
        assert!(!Path::new(&format!("/proc/{}", child["pid"])).exists());
        assert!(!Path::new(child["cwd"].as_str().unwrap()).exists());
    }
}
type Signals = Rc<RefCell<Vec<(String, glib::Variant)>>>;
struct Observer {
    identities: BTreeMap<String, String>,
    events: Signals,
}
impl Observer {
    fn snapshot(
        &mut self,
        owner: &ApplicationDesktop,
        services: &ApplicationServices,
        audio: Option<&Path>,
    ) -> Value {
        let phase = match owner.capture.phase() {
            Some(CapturePhase::Preparing) => Some("preparing"),
            Some(CapturePhase::Recording) => Some("recording"),
            Some(CapturePhase::Processing) => Some("processing"),
            _ => None,
        };
        let prefs = &owner.settings.capture;
        let history=services.history.recent(100).unwrap().into_iter().rev().map(|entry|{
        let fresh=format!("$ENTRY{}",self.identities.len()+1);let id=self.identities.entry(entry.identifier).or_insert(fresh);
        json!({"id":id,"raw":entry.raw_text,"output":entry.delivered_text,"title":entry.title,"mode":entry.mode,"outcome":entry.delivery_outcome,"retained":entry.retained_audio_path.is_some()})
    }).collect::<Vec<_>>();
        let config = services.config();
        let mut state = json!({"ui":capture_ui::observe(&owner.capture.page,audio),"page":owner.shell.stack.visible_child_name().map(String::from),"mode":prefs.mode.selected(),"controls":[prefs.mode.get_sensitive(),prefs.language.get_sensitive(),prefs.audio_retention.get_sensitive(),prefs.incognito.get_sensitive()],"pending":{"command":owner.pending.has_command(),"notes":services.scratchpad.borrow().draft.is_some()},"history":history,"config":{"mode":config.default_mode,"live":config.live_rewrite_enabled,"incognito":config.incognito_mode}});
        state["config"]["provider"] = json!(config.transcription_provider);
        state["clipboard"] = json!(clipboard());
        let before = self.events.borrow().len();
        owner
            .shell
            .window
            .application()
            .unwrap()
            .activate_action("status", None);
        until(|| self.events.borrow().len() >= before + 2);
        let selected = phase.map(|phase| {
            let find =
                || {
                    self.events.borrow().iter().enumerate().rev().find_map(
                        |(index, (name, value))| {
                            (name == "StateChanged"
                                && value.child_value(1).str() == Some(phase)
                                && index + 1 < self.events.borrow().len())
                            .then_some(index)
                        },
                    )
                };
            until(|| find().is_some());
            find().unwrap()
        });
        let events = self.events.borrow();
        let latest = |name: &str| match selected {
            Some(index) => {
                let index = index + usize::from(name == "ShellStateChanged");
                assert_eq!(events[index].0, name);
                &events[index].1
            }
            None => &events.iter().rev().find(|event| event.0 == name).unwrap().1,
        };
        let mut signal = json!(
            latest("StateChanged")
                .get::<(
                    bool,
                    String,
                    String,
                    u32,
                    String,
                    String,
                    f64,
                    String,
                    String
                )>()
                .unwrap()
        );
        let mut shell = Value::Object(
            latest("ShellStateChanged")
                .child_value(0)
                .get::<BTreeMap<String, glib::Variant>>()
                .unwrap()
                .into_iter()
                .map(|(key, value)| {
                    let value = match value.type_().as_str() {
                        "s" => json!(value.get::<String>().unwrap()),
                        "b" => json!(value.get::<bool>().unwrap()),
                        "u" => json!(value.get::<u32>().unwrap()),
                        "d" => json!(value.get::<f64>().unwrap()),
                        "a(ss)" => json!(value.get::<Vec<(String, String)>>().unwrap()),
                        other => panic!("Unknown widget type {other}"),
                    };
                    (key, value)
                })
                .collect(),
        );
        signal[3] = json!("sampled");
        signal[6] = json!("sampled");
        shell["elapsed"] = json!("sampled");
        if shell.get("level").is_some() {
            shell["level"] = json!("sampled");
        }
        if let Some(id) = shell["identifier"].as_str() {
            let fresh = format!("$ENTRY{}", self.identities.len() + 1);
            shell["identifier"] = json!(self.identities.entry(id.to_owned()).or_insert(fresh));
        }
        state["overlay"] = json!({"signal":signal,"shell":shell});
        state
    }
}
fn isolated(tools_name: &str) -> PathBuf {
    let root =
        PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").expect("private desktop runner"));
    for key in ["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XAUTHORITY"] {
        assert!(PathBuf::from(std::env::var_os(key).unwrap()).starts_with(&root));
    }
    assert_ne!(
        fs::read_link("/proc/self/ns/net")
            .unwrap()
            .to_str()
            .unwrap(),
        std::env::var("MLUVA_HOST_NET_NS").unwrap()
    );
    for device in ["/dev/input", "/dev/uinput", "/dev/snd", "/dev/dri"] {
        assert!(!Path::new(device).exists());
    }
    assert_eq!(
        std::env::split_paths(&std::env::var_os("PATH").unwrap()).next(),
        Some(root.join(tools_name))
    );
    root
}
#[test]
#[ignore = "requires isolated GTK/session/network/device runner and native PCM/Codex peers"]
fn released_assembled_application_and_shutdown() {
    let reopening = std::env::var("MLUVA_PROMPT_REOPEN").as_deref() == Ok("1");
    let tools_name = if reopening {
        "application-reopened-tools"
    } else {
        "application-tools"
    };
    let root = isolated(tools_name);
    assert!(
        std::env::var_os("MLUVA_DISABLE_GLOBAL_SHORTCUT").is_some(),
        "match the released observer's disabled portal environment"
    );
    let tools = root.join(tools_name);
    fs::create_dir(&tools).unwrap();
    let target = PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap()).join("debug");
    symlink(
        target.join("audio-fixture-peer").canonicalize().unwrap(),
        tools.join("pw-record"),
    )
    .unwrap();
    let quote = |path: &Path| format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"));
    fs::write(
        tools.join("codex"),
        format!(
            "#!/bin/sh\nexec {} serve {} \"$@\"\n",
            quote(&target.join("codex-fixture-peer")),
            quote(&root.join("application-codex.json"))
        ),
    )
    .unwrap();
    fs::set_permissions(tools.join("codex"), fs::Permissions::from_mode(0o700)).unwrap();
    if std::env::var_os("MLUVA_PROVIDER_CASE").is_some()
        || std::env::var_os("MLUVA_ONBOARDING_CASE").is_some()
    {
        application_providers::prepare_credentials(&root, &tools, &target);
    }
    gtk::init().unwrap();
    adw::init().unwrap();
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
    gtk::Settings::default()
        .unwrap()
        .set_gtk_cursor_blink(false);
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceLight);
    let bus = gio::DBusConnection::for_address_sync(
        &std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap(),
        gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
            | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
        None,
        gio::Cancellable::NONE,
    )
    .unwrap();
    let events = Rc::new(RefCell::new(vec![]));
    let received = events.clone();
    let _subscription = bus.subscribe_to_signal(
        None,
        Some(mluva_gtk::overlay_state::INTERFACE),
        None,
        Some(mluva_gtk::overlay_state::OBJECT_PATH),
        None,
        gio::DBusSignalFlags::NONE,
        move |signal| {
            received
                .borrow_mut()
                .push((signal.signal_name.to_owned(), signal.parameters.clone()))
        },
    );
    bus.flush_sync(gio::Cancellable::NONE).unwrap();
    let resources = DocumentResources::from_directory(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources"),
    );
    let _theme = ThemeController::apply(
        root.join("state/omarchy/current/theme"),
        resources.font.parent().unwrap(),
    )
    .unwrap();
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-application.json")).unwrap();
    assert_eq!(
        fixture["reference"],
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
    let commands: Value =
        serde_json::from_str(include_str!("fixtures/released-application-commands.json")).unwrap();
    let continuation: Value = serde_json::from_str(include_str!(
        "fixtures/released-application-continuation.json"
    ))
    .unwrap();
    let live_workspace: Value = serde_json::from_str(include_str!(
        "fixtures/released-application-live-workspace.json"
    ))
    .unwrap();
    let compact: Value =
        serde_json::from_str(include_str!("fixtures/released-application-compact.json")).unwrap();
    let live_editor: Value = serde_json::from_str(include_str!(
        "fixtures/released-application-live-editor.json"
    ))
    .unwrap();
    let providers: Value =
        serde_json::from_str(include_str!("fixtures/released-application-providers.json")).unwrap();
    let management: Value = serde_json::from_str(include_str!(
        "fixtures/released-application-management.json"
    ))
    .unwrap();
    let prompts: Value =
        serde_json::from_str(include_str!("fixtures/released-application-prompts.json")).unwrap();
    let images: Value =
        serde_json::from_str(include_str!("fixtures/released-application-images.json")).unwrap();
    let onboarding: Value = serde_json::from_str(include_str!(
        "fixtures/released-application-onboarding.json"
    ))
    .unwrap();
    for additional in [
        &commands,
        &continuation,
        &live_workspace,
        &compact,
        &live_editor,
        &providers,
        &management,
        &prompts,
        &images,
        &onboarding,
    ] {
        for key in ["reference", "gtk", "pango"] {
            assert_eq!(additional[key], fixture[key]);
        }
    }
    let mut cases = if let Ok(name) = std::env::var("MLUVA_ONBOARDING_CASE") {
        let selected = onboarding["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|case| case["name"] == name)
            .collect::<Vec<_>>();
        assert_eq!(selected.len(), 1, "select one released onboarding scenario");
        selected
    } else if let Ok(name) = std::env::var("MLUVA_IMAGE_CASE") {
        let selected = images["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|case| case["name"] == name)
            .collect::<Vec<_>>();
        assert_eq!(
            selected.len(),
            1,
            "select one released image workspace scenario"
        );
        selected
    } else if let Ok(name) = std::env::var("MLUVA_PROMPT_CASE") {
        let selected = prompts["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|case| case["name"] == name)
            .collect::<Vec<_>>();
        assert_eq!(selected.len(), 1, "select one released prompt scenario");
        selected
    } else if let Ok(name) = std::env::var("MLUVA_MANAGEMENT_CASE") {
        let selected = management["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|case| case["name"] == name)
            .collect::<Vec<_>>();
        assert_eq!(selected.len(), 1, "select one released management scenario");
        selected
    } else if let Ok(name) = std::env::var("MLUVA_PROVIDER_CASE") {
        let selected = providers["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|case| case["name"] == name)
            .collect::<Vec<_>>();
        assert_eq!(selected.len(), 1, "select one released provider scenario");
        selected
    } else if let Ok(name) = std::env::var("MLUVA_COMPACT_CASE") {
        let selected = compact["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|case| case["name"] == name)
            .collect::<Vec<_>>();
        assert_eq!(selected.len(), 1, "select one released compact scenario");
        selected
    } else {
        fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .chain([&commands, &continuation, &live_workspace])
            .chain(live_editor["cases"].as_array().unwrap())
            .collect()
    };
    if let Ok(name) = std::env::var("MLUVA_APPLICATION_CASE") {
        cases.retain(|case| case["name"] == name);
        assert_eq!(cases.len(), 1, "select one released application scenario");
    }
    let workflows = cases.len();
    let mut count = 0;
    for (index, row) in cases.into_iter().enumerate() {
        let row = if reopening { &row["reopen"] } else { row };
        events.borrow_mut().clear();
        let name = row["name"].as_str().unwrap();
        let params = &row["params"];
        let directory = tempfile::tempdir_in(&root).unwrap();
        let evidence = directory.path().join("evidence");
        fs::create_dir(&evidence).unwrap();
        let _ = fs::remove_file(tools.join("raw.ready.json"));
        let mut pcm = row["pcm"].clone();
        if let Some(repetitions) = row["pcm_repetitions"].as_u64() {
            pcm["pcm_hex"] = json!(
                pcm["pcm_hex"]
                    .as_str()
                    .unwrap()
                    .repeat(repetitions as usize)
            );
        }
        fs::write(
            tools.join("test-config.json"),
            serde_json::to_vec(&pcm).unwrap(),
        )
        .unwrap();
        let mut codex = json!({"scenario":"clean","evidence":evidence,"title_controls":{"Narration keeps 12 files.":{"deltas":["Generated conversation"]}}});
        if let Some(catalog) = row.get("catalog") {
            codex["catalog"] = catalog.clone();
        }
        if params["management"] == true {
            codex["document_controls"] = json!({"Unsent follow-up":{"gate":evidence.join("rewrite.release"),"deltas":["This must not become a saved reply."]}});
        }
        if params["prompts"] == true {
            codex["live_controls"] = json!({format!("preview|{}",row["source"].as_str().unwrap()):{"deltas":[row["draft"]]}});
            codex["document_controls"] =
                json!({"External style instructions":{"deltas":["A concise style response."]}});
        }
        if params["images"] == true {
            codex["deltas"] = json!(["More words."]);
            codex["document_controls"] = json!({"Use these screenshots to explain the visible controls.":{"deltas":["The image says Narration selected area 71."]}});
        }
        fs::write(
            root.join("application-codex.json"),
            serde_json::to_vec(&codex).unwrap(),
        )
        .unwrap();
        let mut responses = row["responses"].as_array().unwrap().clone();
        for response in &mut responses {
            response["delay_ms"] = json!(350);
            if params["compact"] == true && name == "processing" {
                response["wait_for_file"] = json!(evidence.join("provider.release"));
                response["incoming_file"] = json!(evidence.join("speech.arrived"));
            }
            if response["held"] == true {
                response["wait_for_file"] = json!(evidence.join("catalog.release"));
                response["incoming_file"] = json!(evidence.join("catalog.arrived"));
                response["completed_file"] = json!(evidence.join("catalog.sent"));
            }
        }
        // A fixed private endpoint keeps expanded URL fields pixel-comparable.
        let mut peer = if params["providers"] == true {
            http::Peer::bind("127.0.0.1:48117", &responses)
        } else {
            http::Peer::new(&responses)
        };
        let mut document = row["config"].clone();
        if params["providers"] != true && params["onboarding"] != true {
            document["transcription_base_url"] = json!(format!("{}/v1", peer.address));
        }
        let config: AppConfig = serde_json::from_value(document).unwrap();
        let paths = if params["prompts"] == true {
            application_prompts::paths(&root, reopening)
        } else if params["onboarding"] == true {
            application_onboarding::paths(&root)
        } else {
            AppPaths {
                config: directory.path().join("config/mluva"),
                data: directory.path().join("data/mluva"),
                runtime: directory.path().join("runtime/mluva"),
            }
        };
        if !reopening {
            config.save(&paths.config.join("config.json")).unwrap();
            if params["prompts"] == true && name == "bad-config" {
                fs::write(paths.config.join("config.json"), "{malformed configuration").unwrap();
            }
        }
        if params["prompts"] == true || params["images"] == true || params["onboarding"] == true {
            adw::StyleManager::default().set_color_scheme(if params["theme"] == "dark" {
                adw::ColorScheme::ForceDark
            } else {
                adw::ColorScheme::ForceLight
            });
        }
        if params["providers"] == true {
            application_providers::prepare_model(&paths.data);
        }
        let mut artifacts = (params["onboarding"] == true)
            .then(|| application_onboarding::Artifacts::prepare(&root, &tools, &target, row));
        let services = ApplicationServices::open(paths.clone()).unwrap();
        let screenshot_editor = if params["images"] == true {
            application_images::prepare(&tools, directory.path(), &target, row)
        } else {
            target.join("mluva-screenshot-editor")
        };
        let application = adw::Application::builder()
            .application_id(if params["images"] == true {
                "com.mluva.Linux".into()
            } else {
                format!("com.mluva.Acceptance{index}")
            })
            .flags(if params["images"] == true {
                gio::ApplicationFlags::FLAGS_NONE
            } else {
                gio::ApplicationFlags::NON_UNIQUE
            })
            .build();
        application.register(None::<&gio::Cancellable>).unwrap();
        let runtime = DesktopRuntime::new().unwrap();
        let platform_closed = Rc::new(Cell::new(false));
        let closing = platform_closed.clone();
        // Compositor presentation/bootstrap remain distribution boundaries.
        let owner = ApplicationDesktop::new(
            &application,
            services.clone(),
            runtime,
            resources.clone(),
            NativeBinaries {
                asr_worker: target.join("mluva-asr-worker"),
                audio_cleanup: target.join("mluva-audio-cleanup"),
                screenshot_editor,
            },
            ApplicationPlatform {
                compact_recording: Rc::new(|_| {}),
                close: Rc::new(move || closing.set(true)),
            },
        )
        .unwrap();
        if params["commands"] == true || params["continuation"] == true {
            // These immutable layout references predate the remembered disclosure.
            // Compare their expanded controls; bootstrap owns the new default.
            owner.capture.page.workspace.rewrite_toggle.set_active(true);
        }
        owner.shell.present();
        until(|| {
            owner.capture.page.record_button.get_sensitive()
                || owner.capture.page.view_state().initialization_failed
        });
        settle();
        if params["onboarding"] == true {
            let pids =
                application_onboarding::exercise(&owner, &services, row, &tools, &root, &evidence);
            shutdown(&owner, &platform_closed, &evidence, &pids);
            assert!(
                peer.finish().is_empty(),
                "local onboarding made an HTTP provider request"
            );
            assert_eq!(application_continuation::turns(&evidence), json!([]));
            artifacts.as_mut().unwrap().finish(&row["transfers"]);
            count += row["stages"].as_array().unwrap().len();
            release_application(owner, application);
            continue;
        }
        if params["images"] == true {
            let mut flow = application_images::Flow::new(
                &owner, &services, row, &tools, &root, &evidence, &peer,
            );
            flow.exercise();
            shutdown(&owner, &platform_closed, &evidence, &flow.audio_pids());
            flow.closed();
            drop(flow);
            assert_eq!(
                json!(peer.finish()),
                row["requests"],
                "image workspace HTTP"
            );
            let mut turns = application_continuation::turns(&evidence);
            screenshot_wire::normalize(&mut turns);
            assert_eq!(
                turns, row["turns"],
                "image workspace Codex image bytes and order"
            );
            count += row["stages"].as_array().unwrap().len();
            release_application(owner, application);
            continue;
        }
        if params["prompts"] == true {
            let pids = application_prompts::exercise(
                &owner, &services, row, &tools, &root, &evidence, reopening,
            );
            shutdown(&owner, &platform_closed, &evidence, &pids);
            assert_eq!(
                application_continuation::requests(peer.finish()),
                row["requests"],
                "prompt HTTP"
            );
            assert_eq!(
                application_continuation::turns(&evidence),
                row["turns"],
                "prompt Codex"
            );
            count += row["stages"].as_array().unwrap().len();
            release_application(owner, application);
            continue;
        }
        if params["management"] == true {
            let pids = application_management::exercise(
                &owner,
                &services,
                &application,
                row,
                &tools,
                &root,
                &evidence,
            );
            shutdown(&owner, &platform_closed, &evidence, &pids);
            assert_eq!(json!(peer.finish()), row["requests"], "management HTTP");
            let methods = records(&evidence.join("requests.jsonl"))
                .into_iter()
                .filter_map(|r| r["message"]["method"].as_str().map(String::from))
                .collect::<Vec<_>>();
            assert_eq!(json!(methods), row["methods"], "management Codex methods");
            assert_eq!(
                application_continuation::turns(&evidence),
                row["turns"],
                "management Codex input"
            );
            count += row["stages"].as_array().unwrap().len();
            release_application(owner, application);
            continue;
        }
        if params["providers"] == true {
            let pids = application_providers::exercise(
                &owner, &services, row, &tools, &root, &evidence, &peer,
            );
            shutdown(&owner, &platform_closed, &evidence, &pids);
            assert_eq!(json!(peer.finish()), row["requests"], "provider HTTP");
            let methods = records(&evidence.join("requests.jsonl"))
                .into_iter()
                .filter_map(|r| r["message"]["method"].as_str().map(String::from))
                .collect::<Vec<_>>();
            assert_eq!(json!(methods), row["methods"], "provider Codex");
            count += row["stages"].as_array().unwrap().len();
            release_application(owner, application);
            continue;
        }
        if params["live_editor"] == true {
            let pids = application_live_editor::exercise(
                &owner, &services, row, &tools, &root, &evidence, &events,
            );
            shutdown(&owner, &platform_closed, &evidence, &pids);
            assert_eq!(
                application_continuation::requests(peer.finish()),
                row["requests"],
                "Live editor HTTP"
            );
            assert_eq!(
                application_continuation::turns(&evidence),
                row["turns"],
                "Live editor Codex"
            );
            count += row["stages"].as_array().unwrap().len();
            release_application(owner, application);
            continue;
        }
        if params["compact"] == true {
            let pids =
                application_compact::exercise(&owner, &services, row, &tools, &root, &evidence);
            shutdown(&owner, &platform_closed, &evidence, &pids);
            assert_eq!(
                application_continuation::requests(peer.finish()),
                row["requests"],
                "compact HTTP"
            );
            assert_eq!(
                application_continuation::turns(&evidence),
                row["turns"],
                "compact Codex"
            );
            count += 2;
            release_application(owner, application);
            continue;
        }
        if params["commands"] == true {
            let pid =
                application_commands::exercise(&owner, &services, &application, row, &tools, &root);
            shutdown(&owner, &platform_closed, &evidence, &[pid]);
            assert_eq!(json!(peer.finish()), row["requests"], "command HTTP");
            let turns = records(&evidence.join("requests.jsonl"))
                .into_iter()
                .filter(|record| record["message"]["method"] == "turn/start")
                .map(|record| record["message"].clone())
                .collect::<Vec<_>>();
            assert_eq!(
                json!(turns),
                row["turns"],
                "stale commands must not dispatch a provider turn"
            );
            count += row["stages"].as_array().unwrap().len();
            continue;
        }
        if params["continuation"] == true {
            let pids = application_continuation::exercise(
                &owner, &services, row, &tools, &root, &evidence, &events,
            );
            shutdown(&owner, &platform_closed, &evidence, &pids);
            assert_eq!(
                application_continuation::requests(peer.finish()),
                row["requests"],
                "continuation HTTP"
            );
            assert_eq!(
                application_continuation::turns(&evidence),
                row["turns"],
                "continuation Codex"
            );
            count += row["stages"].as_array().unwrap().len();
            continue;
        }
        if params["live_workspace"] == true {
            let pids = application_live_workspace::exercise(
                &owner, &services, row, &tools, &root, &evidence,
            );
            shutdown(&owner, &platform_closed, &evidence, &pids);
            assert_eq!(
                application_continuation::requests(peer.finish()),
                row["requests"],
                "Live workspace HTTP"
            );
            assert_eq!(
                application_continuation::turns(&evidence),
                row["turns"],
                "Live workspace Codex"
            );
            count += row["stages"].as_array().unwrap().len();
            release_application(owner, application);
            continue;
        }
        gtk::gdk::Display::default()
            .unwrap()
            .clipboard()
            .set_text("untouched application clipboard");
        let mut observer = Observer {
            identities: BTreeMap::new(),
            events: events.clone(),
        };
        let mut stage_index = 0;
        let mut stage = |stage: &str, audio: Option<&Path>| {
            let actual = json!({"stage":stage,"state":observer.snapshot(&owner,&services,audio)});
            let expected = &row["stages"][stage_index];
            fs::write(
                root.join(format!("{name}-{stage}-actual.json")),
                serde_json::to_vec_pretty(&actual).unwrap(),
            )
            .unwrap();
            assert_eq!(&actual, expected, "{name}: {stage}");
            stage_index += 1;
            count += 1;
        };
        stage("startup", None);
        if params["repair"] == true {
            application.activate_action("settings", None);
            owner.settings.providers.speech.provider_row.set_selected(2);
            owner.settings.providers.apply_button.emit_clicked();
            until(|| owner.capture.page.record_button.get_sensitive());
            application.activate_action("latest", None);
            settle();
            stage("repaired", None);
        }
        application.activate_action("record", None);
        let audio = paths.data.join("recordings").join(format!(
            "{}.wav",
            owner.capture.session_identifier().unwrap()
        ));
        stage("preparing", Some(&audio));
        until(|| {
            owner.capture.phase() == Some(CapturePhase::Recording)
                && tools.join("raw.ready.json").exists()
        });
        settle();
        stage("recording", Some(&audio));
        let ready: Value =
            serde_json::from_slice(&fs::read(tools.join("raw.ready.json")).unwrap()).unwrap();
        let pid = ready["pid"].as_u64().unwrap();
        if params["close"] != true {
            owner.capture.page.record_button.emit_clicked();
            stage("processing", Some(&audio));
            until(|| owner.capture.phase().is_none());
            if params["title"] == true {
                eprintln!(
                    "{name}: awaiting generated title after capture; saved title = {:?}",
                    services.history.recent(1).unwrap()[0].title
                );
                until(|| {
                    services
                        .history
                        .recent(1)
                        .unwrap()
                        .first()
                        .is_some_and(|entry| {
                            entry.title.as_deref() == Some("Generated conversation")
                        })
                });
            }
            settle();
            stage("completed", Some(&audio));
            if params["repair"] == true {
                owner.capture.page.setup_button.emit_clicked();
                until(|| !owner.capture.page.view_state().initialization_failed);
                settle();
                stage("acknowledged", Some(&audio));
            }
            if let Some(mode) = params["mode"].as_str() {
                application.activate_action("record", None);
                stage("blocked-next-recording", Some(&audio));
                if mode == "scratchpad" {
                    owner
                        .capture
                        .page
                        .output_view
                        .buffer()
                        .set_text("Edited notes keep 34 files.");
                    stage("edited-notes", Some(&audio));
                    owner.capture.page.copy_scratchpad.emit_clicked();
                } else {
                    owner.capture.page.accept_command.emit_clicked();
                }
                settle();
                stage("accepted", Some(&audio));
            } else {
                for action in [
                    "history",
                    "settings",
                    "meeting",
                    "personalization",
                    "latest",
                ] {
                    application.activate_action(action, None);
                    settle();
                    stage(action, Some(&audio));
                }
            }
        }
        assert_eq!(stage_index, row["stages"].as_array().unwrap().len());
        if params["broken_save"] == true {
            fs::remove_file(paths.config.join("config.json")).unwrap();
            fs::create_dir(paths.config.join("config.json")).unwrap();
        }
        shutdown(&owner, &platform_closed, &evidence, &[pid]);
        assert_eq!(
            json!({"live":services.config().live_rewrite_enabled,"audio_child_alive":false,"history_count":services.history.recent(100).unwrap().len(),"audio_exists":audio.exists()}),
            row["closed"],
            "{name}: closed"
        );
        assert_eq!(json!(peer.finish()), row["requests"], "{name}: HTTP");
        let turns=records(&evidence.join("requests.jsonl")).into_iter().filter(|record|record["message"]["method"]=="turn/start").map(|record|json!({"prompt":record["message"]["params"]["input"][0]["text"],"model":record["message"]["params"]["model"]})).collect::<Vec<_>>();
        assert_eq!(json!(turns), row["turns"], "{name}: Codex");
        println!("{name}: {stage_index} states PASS");
    }
    println!(
        "Assembled application: {workflows} workflows, {count} GTK/store states, acknowledged owner shutdown"
    );
}
