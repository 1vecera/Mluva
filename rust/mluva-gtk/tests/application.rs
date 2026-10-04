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
    rc::Rc,
    thread,
    time::{Duration, Instant},
};
#[path = "support/application_commands.rs"]
mod application_commands;
#[path = "support/capture_ui.rs"]
#[allow(dead_code)]
mod capture_ui;
#[path = "../../mluva-workflows/tests/support/http.rs"]
mod http;
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
fn shutdown(owner: &Rc<ApplicationDesktop>) {
    let drained = Rc::new(Cell::new(false));
    let done = drained.clone();
    let shutdown = owner.shutdown();
    glib::MainContext::default().spawn_local(async move {
        shutdown.await.unwrap();
        done.set(true);
    });
    until(|| drained.get());
    settle();
}
struct Observer {
    identities: BTreeMap<String, String>,
    events: Rc<RefCell<Vec<(String, glib::Variant)>>>,
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
fn isolated() -> PathBuf {
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
        Some(root.join("application-tools"))
    );
    root
}
#[test]
#[ignore = "requires isolated GTK/session/network/device runner and native PCM/Codex peers"]
fn released_assembled_application_and_shutdown() {
    let root = isolated();
    assert!(
        std::env::var_os("MLUVA_DISABLE_GLOBAL_SHORTCUT").is_some(),
        "match the released observer's disabled portal environment"
    );
    let tools = root.join("application-tools");
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
    assert_eq!(commands["reference"], fixture["reference"]);
    assert_eq!(commands["gtk"], fixture["gtk"]);
    assert_eq!(commands["pango"], fixture["pango"]);
    let cases = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .chain(std::iter::once(&commands));
    let mut count = 0;
    for (index, row) in cases.enumerate() {
        events.borrow_mut().clear();
        let name = row["name"].as_str().unwrap();
        let params = &row["params"];
        let directory = tempfile::tempdir_in(&root).unwrap();
        let evidence = directory.path().join("evidence");
        fs::create_dir(&evidence).unwrap();
        let _ = fs::remove_file(tools.join("raw.ready.json"));
        fs::write(
            tools.join("test-config.json"),
            serde_json::to_vec(&row["pcm"]).unwrap(),
        )
        .unwrap();
        fs::write(root.join("application-codex.json"),serde_json::to_vec(&json!({"scenario":"clean","evidence":evidence,"title_controls":{"Narration keeps 12 files.":{"deltas":["Generated conversation"]}}})).unwrap()).unwrap();
        let mut responses = row["responses"].as_array().unwrap().clone();
        for response in &mut responses {
            response["delay_ms"] = json!(350);
        }
        let mut peer = http::Peer::new(&responses);
        let mut document = row["config"].clone();
        document["transcription_base_url"] = json!(format!("{}/v1", peer.address));
        let config: AppConfig = serde_json::from_value(document).unwrap();
        let paths = AppPaths {
            config: directory.path().join("config/mluva"),
            data: directory.path().join("data/mluva"),
            runtime: directory.path().join("runtime/mluva"),
        };
        config.save(&paths.config.join("config.json")).unwrap();
        let services = ApplicationServices::open(paths.clone()).unwrap();
        let application = adw::Application::builder()
            .application_id(format!("com.mluva.Acceptance{index}"))
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        application.register(None::<&gio::Cancellable>).unwrap();
        let runtime = DesktopRuntime::new().unwrap();
        let platform_closed = Rc::new(Cell::new(false));
        let closing = platform_closed.clone();
        // Compositor presentation/bootstrap remain distribution boundaries.
        // Screenshot workflows have their own assembled-application comparison.
        let owner = ApplicationDesktop::new(
            &application,
            services.clone(),
            runtime,
            resources.clone(),
            NativeBinaries {
                asr_worker: target.join("mluva-asr-worker"),
                audio_cleanup: target.join("mluva-audio-cleanup"),
                screenshot_editor: target.join("mluva-screenshot-editor"),
            },
            ApplicationPlatform {
                compact_recording: Rc::new(|_| {}),
                close: Rc::new(move || closing.set(true)),
            },
        )
        .unwrap();
        owner.shell.present();
        until(|| {
            owner.capture.page.record_button.get_sensitive()
                || owner.capture.page.view_state().initialization_failed
        });
        settle();
        if params["commands"] == true {
            let pid =
                application_commands::exercise(&owner, &services, &application, row, &tools, &root);
            shutdown(&owner);
            assert!(platform_closed.get());
            assert!(!Path::new(&format!("/proc/{pid}")).exists());
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
            for child in records(&evidence.join("process.jsonl")) {
                assert!(!Path::new(&format!("/proc/{}", child["pid"])).exists());
                assert!(!Path::new(child["cwd"].as_str().unwrap()).exists());
            }
            count += row["stages"].as_array().unwrap().len();
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
        shutdown(&owner);
        assert!(platform_closed.get());
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
        for child in records(&evidence.join("process.jsonl")) {
            assert!(!Path::new(&format!("/proc/{}", child["pid"])).exists());
            assert!(!Path::new(child["cwd"].as_str().unwrap()).exists());
        }
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
        "Assembled application: {} workflows, {count} GTK/store states, acknowledged owner shutdown",
        fixture["cases"].as_array().unwrap().len() + 1
    );
}
