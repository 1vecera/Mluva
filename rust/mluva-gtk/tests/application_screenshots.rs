//! Application-owned screenshots observed through unchanged release outputs and real I/O.
use adw::prelude::*;
use mluva_core::{
    config::{AppConfig, AppPaths},
    history::HistoryInput,
    screenshots::Screenshot,
};
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
    cell::RefCell,
    collections::BTreeMap,
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    rc::Rc,
    thread,
    time::{Duration, Instant},
};
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
        assert!(
            Instant::now() < end,
            "Screenshot application did not settle"
        );
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
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn unhex(value: &Value) -> Vec<u8> {
    value
        .as_str()
        .unwrap()
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn read(path: impl AsRef<Path>) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn write(path: &Path, value: &Value) {
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, serde_json::to_vec(value).unwrap()).unwrap();
    fs::rename(temporary, path).unwrap();
}
fn alive(pid: u32) -> bool {
    fs::read_to_string(format!("/proc/{pid}/stat"))
        .is_ok_and(|stat| stat.rsplit_once(')').unwrap().1.split_whitespace().next() != Some("Z"))
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
fn clipboard() -> Option<String> {
    let value = Rc::new(RefCell::new(None));
    let received = value.clone();
    gtk::gdk::Display::default()
        .unwrap()
        .clipboard()
        .read_text_async(None::<&gio::Cancellable>, move |result| {
            *received.borrow_mut() = Some(result.unwrap().map(String::from));
        });
    until(|| value.borrow().is_some());
    value.borrow_mut().take().unwrap()
}
fn editor_records(directory: &Path) -> Vec<(String, Value)> {
    let mut records = vec![];
    if let Ok(folders) = fs::read_dir(directory.join("editors")) {
        for folder in folders {
            let folder = folder.unwrap();
            for entry in fs::read_dir(folder.path()).unwrap() {
                let path = entry.unwrap().path();
                if path.extension().is_some_and(|ext| ext == "json")
                    && path.file_name().unwrap() != "command.json"
                {
                    records.push((
                        folder.file_name().to_string_lossy().into_owned(),
                        read(path),
                    ));
                }
            }
        }
    }
    records.sort_by(|a, b| a.0.cmp(&b.0));
    records
}
fn editor_command(directory: &Path, shot: &Screenshot, png: &Value) {
    let folder = directory.join("editors").join(&shot.identifier);
    write(
        &folder.join("command.json"),
        &json!({"op":"save","serial":1,"png":png}),
    );
    until(|| {
        editor_records(directory)
            .iter()
            .any(|(id, record)| id == &shot.identifier && record["serial"] == 1)
    });
    until(|| fs::read(&shot.path).unwrap() == unhex(png));
}
#[derive(Default)]
struct Observer {
    identities: BTreeMap<String, String>,
    images: BTreeMap<String, String>,
    capture: Option<String>,
    audio: Option<PathBuf>,
    audio_pid: Option<u32>,
}
impl Observer {
    fn identify(&mut self, id: &str) -> String {
        if self.capture.as_deref() == Some(id) {
            return "$CAPTURE".into();
        }
        let fresh = format!("$ENTRY{}", self.identities.len() + 1);
        self.identities.entry(id.into()).or_insert(fresh).clone()
    }
    fn image(&mut self, id: &str) -> String {
        let fresh = format!("$IMAGE{}", self.images.len() + 1);
        self.images.entry(id.into()).or_insert(fresh).clone()
    }
    fn snapshot(
        &mut self,
        owner: &ApplicationDesktop,
        services: &ApplicationServices,
        directory: &Path,
        peer: &http::Peer,
        durable: bool,
    ) -> Value {
        let entries = services.history.recent(100).unwrap();
        let history=entries.iter().rev().map(|e|json!({"id":self.identify(&e.identifier),"raw":e.raw_text,"output":e.delivered_text,"mode":e.mode,"outcome":e.delivery_outcome,"retained":e.retained_audio_path.is_some()})).collect::<Vec<_>>();
        let owners = entries
            .iter()
            .map(|entry| (entry.identifier.clone(), false))
            .chain(
                services
                    .screenshots
                    .pending_captures()
                    .unwrap()
                    .into_iter()
                    .map(|id| (id, true)),
            );
        let mut images = vec![];
        for (id, capture) in owners {
            for shot in services.screenshots.recent(&id, capture).unwrap() {
                if let Some(offset) = shot.captured_after_seconds {
                    assert!((0.0..8.0).contains(&offset));
                }
                images.push(json!({"id":self.image(&shot.identifier),"owner":self.identify(&id),"capture":capture,"offset":shot.captured_after_seconds.map(|_|"sampled"),"png":hex(&fs::read(shot.path).unwrap())}));
            }
        }
        images.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
        let mut editors: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        for (id, record) in editor_records(directory) {
            let count = editors.entry(self.image(&id)).or_default();
            count.0 += 1;
            count.1 += usize::from(alive(record["pid"].as_u64().unwrap() as u32));
        }
        let editors = editors
            .into_iter()
            .map(|(id, (launches, running))| json!({"id":id,"launches":launches,"running":running}))
            .collect::<Vec<_>>();
        let mut state = json!({"history":history,"images":images,"editors":editors,"requests":peer.observed.lock().unwrap().len(),"audio_alive":self.audio_pid.is_some_and(alive),"incognito":services.config().incognito_mode});
        if !durable {
            let shelf = &owner.capture.page.workspace.screenshot_shelf.widget;
            let all = widgets(shelf);
            let pictures = all
                .iter()
                .filter_map(|widget| widget.downcast_ref::<gtk::Picture>())
                .map(|picture| {
                    picture.paintable().map(|texture| {
                        hex(texture
                            .downcast::<gtk::gdk::Texture>()
                            .unwrap()
                            .save_to_png_bytes()
                            .as_ref())
                    })
                })
                .collect::<Vec<_>>();
            let labels = all
                .iter()
                .filter_map(|widget| {
                    widget
                        .downcast_ref::<gtk::Label>()
                        .map(|label| label.label().to_string())
                })
                .collect::<Vec<_>>();
            state["ui"] = capture_ui::observe(&owner.capture.page, self.audio.as_deref());
            state["shelf"] =
                json!({"visible":shelf.get_visible(),"labels":labels,"pictures":pictures});
            state["clipboard"] = json!(clipboard());
        }
        state
    }
}
fn normalize_wire(value: &mut Value) {
    match value {
        Value::Array(values) => values.iter_mut().for_each(normalize_wire),
        Value::Object(values) => values.values_mut().for_each(normalize_wire),
        Value::String(text) if text.contains("The attached screenshots are visual context") => {
            let (before, after) = text.rsplit_once('\n').unwrap();
            let mut metadata: Value = serde_json::from_str(after).unwrap();
            for image in metadata.as_array_mut().unwrap() {
                if let Some(offset) = image["captured_after_seconds"].as_f64() {
                    assert!((0.0..8.0).contains(&offset));
                    image["captured_after_seconds"] = json!("sampled");
                }
            }
            *text = format!("{before}\n{}", mluva_core::json::spaced(&metadata));
        }
        _ => {}
    }
}
fn isolated() -> PathBuf {
    let private =
        PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").expect("private desktop runner"));
    for key in ["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XAUTHORITY"] {
        assert!(PathBuf::from(std::env::var_os(key).unwrap()).starts_with(&private));
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
    let case = private.join("application-screenshot-case");
    assert_eq!(
        PathBuf::from(std::env::var_os("MLUVA_SCREENSHOT_FIXTURE_ROOT").unwrap()),
        case
    );
    assert_eq!(
        std::env::split_paths(&std::env::var_os("PATH").unwrap()).next(),
        Some(case.join("tools"))
    );
    assert!(std::env::var_os("MLUVA_DISABLE_GLOBAL_SHORTCUT").is_some());
    private
}

#[test]
#[ignore = "requires private GTK/session/network/devices and external PCM/picker/editor peers"]
fn released_application_screenshot_ownership_and_lifecycle() {
    let root = isolated();
    adw::init().unwrap();
    let settings = gtk::Settings::default().unwrap();
    settings.set_gtk_enable_animations(false);
    settings.set_gtk_cursor_blink(false);
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceLight);
    let resources = DocumentResources::from_directory(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources"),
    );
    let _theme = ThemeController::apply(
        root.join("state/omarchy/current/theme"),
        resources.font.parent().unwrap(),
    )
    .unwrap();
    let target = PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap()).join("debug");
    let fixture: Value = serde_json::from_str(include_str!(
        "fixtures/released-application-screenshots.json"
    ))
    .unwrap();
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
    let mut count = 0;
    for (index, row) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let params = &row["params"];
        let name = params["name"].as_str().unwrap();
        let directory = root.join("application-screenshot-case");
        if directory.exists() {
            fs::remove_dir_all(&directory).unwrap();
        }
        let tools = directory.join("tools");
        fs::create_dir_all(&tools).unwrap();
        for (name, binary) in [
            ("pw-record", target.join("audio-fixture-peer")),
            ("omarchy", target.join("screenshot-picker-fixture-peer")),
            (
                "tensaku-edit",
                target.join("examples/screenshot_editor_peer"),
            ),
        ] {
            symlink(binary, tools.join(name)).unwrap();
        }
        write(&tools.join("test-config.json"), &row["pcm"]);
        write(&directory.join("picker.json"), &row["selection"]);
        let mut responses = row["responses"].as_array().unwrap().clone();
        if params["edit_while_processing"] == true {
            responses[1]["wait_for_file"] = json!(directory.join("http-release"));
            responses[1]["incoming_file"] = json!(directory.join("http-incoming"));
        }
        let mut peer = http::Peer::new(&responses);
        let mut document = row["config"].clone();
        document["transcription_base_url"] = json!(format!("{}/v1", peer.address));
        document["litellm_base_url"] = json!(peer.address);
        let config: AppConfig = serde_json::from_value(document).unwrap();
        let paths = AppPaths {
            config: directory.join("config/mluva"),
            data: directory.join("data/mluva"),
            runtime: directory.join("runtime/mluva"),
        };
        config.save(&paths.config.join("config.json")).unwrap();
        let services = ApplicationServices::open(paths.clone()).unwrap();
        let application = adw::Application::builder()
            .application_id(format!("com.mluva.ScreenshotAcceptance{index}"))
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        application.register(None::<&gio::Cancellable>).unwrap();
        let owner = ApplicationDesktop::new(
            &application,
            services.clone(),
            DesktopRuntime::new().unwrap(),
            resources.clone(),
            NativeBinaries {
                asr_worker: target.join("mluva-asr-worker"),
                audio_cleanup: target.join("mluva-audio-cleanup"),
                screenshot_editor: directory.join("missing-installed-editor"),
            },
            ApplicationPlatform {
                compact_recording: Rc::new(|_| {}),
                close: Rc::new(|| {}),
            },
        )
        .unwrap();
        owner.shell.present();
        until(|| owner.capture.page.record_button.get_sensitive());
        owner
            .settings
            .capture
            .cleanup
            .set_active(params["enhance"] == true);
        let workspace = owner.capture.page.workspace.clone();
        let mut observer = Observer::default();
        let mut context = vec![];
        if params["saved"] == true || params["continuation"] == true {
            for text in if name == "saved-owner-and-editor" || params["stubborn"] == true {
                vec!["Earlier narration.", "Other conversation."]
            } else {
                vec!["Earlier narration."]
            } {
                let entry = services
                    .history
                    .add(HistoryInput {
                        language_code: "eng".into(),
                        delivery_outcome: "saved".into(),
                        ..HistoryInput::dictation(text, text)
                    })
                    .unwrap();
                observer.identify(&entry.identifier);
                context.push(entry);
            }
            if params["continuation"] == true {
                let shot = services
                    .screenshots
                    .add(
                        &context[0].identifier,
                        &unhex(&fixture["blue_png"]),
                        false,
                        None,
                    )
                    .unwrap();
                observer.image(&shot.identifier);
            }
            workspace.refresh_history().unwrap();
            workspace
                .show_conversation(Some(context[0].clone()), &[], false)
                .unwrap();
        }
        gtk::gdk::Display::default()
            .unwrap()
            .clipboard()
            .set_text("untouched screenshot clipboard");
        let mut stage_index = 0;
        let mut stage = |label: &str, observer: &mut Observer| {
            settle();
            let actual = json!({"stage":label,"state":observer.snapshot(&owner,&services,&directory,&peer,false)});
            write(&root.join(format!("{name}-{label}-actual.json")), &actual);
            assert_eq!(actual, row["stages"][stage_index], "{name}: {label}");
            stage_index += 1;
            count += 1;
        };
        stage("startup", &mut observer);
        if params["capture"] == true {
            if params["continuation"] == true {
                workspace.continue_button.emit_clicked();
            } else {
                application.activate_action("record", None);
            }
            until(|| {
                owner.capture.phase() == Some(CapturePhase::Recording)
                    && tools.join("raw.ready.json").exists()
            });
            observer.capture = owner.capture.session_identifier();
            observer.audio = fs::read_dir(paths.data.join("recordings"))
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .find(|path| path.extension().is_some_and(|ext| ext == "wav"));
            observer.audio_pid =
                Some(read(tools.join("raw.ready.json"))["pid"].as_u64().unwrap() as u32);
            stage("recording", &mut observer);
        }
        application.activate_action("screenshot", None);
        let shots = |observer: &Observer| {
            services
                .screenshots
                .recent(
                    observer
                        .capture
                        .as_deref()
                        .unwrap_or_else(|| &context[0].identifier),
                    observer.capture.is_some(),
                )
                .unwrap()
        };
        if params["blocked"] == true {
            settle();
            assert!(!directory.join("ready.json").exists());
            stage("capture-blocked", &mut observer);
        } else {
            until(|| directory.join("ready.json").exists());
            stage("picker-open", &mut observer);
            if name == "saved-owner-and-editor" || params["stubborn"] == true {
                workspace
                    .show_conversation(Some(context[1].clone()), &[], false)
                    .unwrap();
                stage("view-other-owner", &mut observer);
            }
            if params["close_processing"] == true {
                owner.capture.page.record_button.emit_clicked();
                until(|| {
                    !observer.audio_pid.is_some_and(alive)
                        && peer.observed.lock().unwrap().len() == 1
                });
                stage("processing-picker-open", &mut observer);
            }
            if params["delete_owner"] == true {
                services.history.delete(&context[0].identifier).unwrap();
            }
            if params["privacy"] == true {
                owner.settings.capture.incognito.set_active(true);
                until(
                    || !alive(read(directory.join("ready.json"))["pid"].as_u64().unwrap() as u32),
                );
                stage("privacy-cancelled", &mut observer);
            } else if params["pending"] != true {
                if params["wait_stop"] == true {
                    owner.capture.page.record_button.emit_clicked();
                    until(|| {
                        !observer.audio_pid.is_some_and(alive)
                            && peer.observed.lock().unwrap().len() == 1
                    });
                    assert!(!directory.join("http-incoming").exists());
                    stage("audio-stopped-picker-open", &mut observer);
                }
                fs::write(directory.join("release"), b"").unwrap();
                if params["invalid"] != true && params["delete_owner"] != true {
                    until(|| !editor_records(&directory).is_empty());
                    stage("attached", &mut observer);
                } else {
                    until(|| {
                        !alive(read(directory.join("ready.json"))["pid"].as_u64().unwrap() as u32)
                    });
                    settle();
                    stage("selection-discarded", &mut observer);
                }
                if name == "saved-owner-and-editor" || params["stubborn"] == true {
                    workspace
                        .show_conversation(Some(context[0].clone()), &[], false)
                        .unwrap();
                    stage("view-frozen-owner", &mut observer);
                    let shot = shots(&observer).remove(0);
                    let button = |tip: &str| {
                        widgets(&workspace.screenshot_shelf.widget)
                            .into_iter()
                            .find_map(|widget| {
                                widget
                                    .downcast::<gtk::Button>()
                                    .ok()
                                    .filter(|button| button.tooltip_text().as_deref() == Some(tip))
                            })
                            .unwrap()
                    };
                    button("Edit screenshot").emit_clicked();
                    stage("already-editing", &mut observer);
                    editor_command(&directory, &shot, &fixture["green_png"]);
                    settle();
                    stage("editor-saved", &mut observer);
                    button("Remove screenshot").emit_clicked();
                    until(|| !shot.path.exists());
                    stage("removed-after-editor-close", &mut observer);
                }
            }
        }
        let mut closed = false;
        if params["capture"] == true {
            if params["close"] == true {
                glib::MainContext::default()
                    .block_on(owner.shutdown())
                    .unwrap();
                closed = true;
            } else if params["cancel"] == true {
                application.activate_action("cancel", None);
                until(|| owner.capture.session_identifier().is_none());
                stage("cancelled", &mut observer);
            } else {
                if params["wait_stop"] != true {
                    owner.capture.page.record_button.emit_clicked();
                }
                if params["edit_while_processing"] == true {
                    until(|| directory.join("http-incoming").exists());
                    let shot = shots(&observer).remove(0);
                    editor_command(&directory, &shot, &fixture["green_png"]);
                    stage("file-edited-after-snapshot", &mut observer);
                    fs::write(directory.join("http-release"), b"").unwrap();
                }
                until(|| owner.capture.phase().is_none());
                stage("completed", &mut observer);
            }
        }
        assert_eq!(stage_index, row["stages"].as_array().unwrap().len());
        if !closed {
            glib::MainContext::default()
                .block_on(owner.shutdown())
                .unwrap();
        }
        until(|| !observer.audio_pid.is_some_and(alive));
        settle();
        let final_state = observer.snapshot(&owner, &services, &directory, &peer, true);
        write(
            &root.join(format!("{name}-final-actual.json")),
            &final_state,
        );
        assert_eq!(final_state, row["final"], "{name}: shutdown");
        if directory.join("ready.json").exists() {
            assert!(
                !alive(read(directory.join("ready.json"))["pid"].as_u64().unwrap() as u32),
                "{name}: picker survived shutdown"
            );
        }
        assert!(
            !fs::read_dir(&paths.runtime).unwrap().any(|entry| entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("screenshot-")),
            "{name}: picker directory survived shutdown"
        );
        // The released app leaves its external editors open on shutdown. End
        // only these synthetic child processes after observing that contract.
        for (_, record) in editor_records(&directory) {
            let pid = record["pid"].as_u64().unwrap() as i32;
            if alive(pid as u32) {
                unsafe {
                    libc::kill(pid, libc::SIGTERM);
                    libc::waitpid(pid, std::ptr::null_mut(), 0);
                }
            }
        }
        let mut requests = json!(peer.finish());
        normalize_wire(&mut requests);
        assert_eq!(
            requests, row["requests"],
            "{name}: HTTP image bytes and order"
        );
        eprintln!("{name}: {stage_index} states");
    }
    eprintln!("17 released screenshot workflows, {count} application states");
    metadata_failure_recovers_images(&root, &target, &resources, &fixture);
    cancelled_capture_rejects_late_selection(&root, &target, &resources, &fixture);
}

fn cancelled_capture_rejects_late_selection(
    root: &Path,
    target: &Path,
    resources: &DocumentResources,
    fixture: &Value,
) {
    let row = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["params"]["name"] == "cancel-pending-picker")
        .unwrap();
    let directory = root.join("application-screenshot-case");
    fs::remove_dir_all(&directory).unwrap();
    let tools = directory.join("tools");
    fs::create_dir_all(&tools).unwrap();
    for (name, binary) in [
        ("pw-record", target.join("audio-fixture-peer")),
        ("omarchy", target.join("screenshot-picker-fixture-peer")),
        (
            "tensaku-edit",
            target.join("examples/screenshot_editor_peer"),
        ),
    ] {
        symlink(binary, tools.join(name)).unwrap();
    }
    let mut pcm = row["pcm"].clone();
    pcm["finalize_delay_ms"] = json!(800);
    write(&tools.join("test-config.json"), &pcm);
    write(&directory.join("picker.json"), &row["selection"]);
    let mut peer = http::Peer::new(&[]);
    let mut document = row["config"].clone();
    document["transcription_base_url"] = json!(format!("{}/v1", peer.address));
    document["litellm_base_url"] = json!(peer.address);
    let config: AppConfig = serde_json::from_value(document).unwrap();
    let paths = AppPaths {
        config: directory.join("config/mluva"),
        data: directory.join("data/mluva"),
        runtime: directory.join("runtime/mluva"),
    };
    config.save(&paths.config.join("config.json")).unwrap();
    let services = ApplicationServices::open(paths).unwrap();
    let application = adw::Application::builder()
        .application_id("com.mluva.ScreenshotCancelRace")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    application.register(None::<&gio::Cancellable>).unwrap();
    let owner = ApplicationDesktop::new(
        &application,
        services.clone(),
        DesktopRuntime::new().unwrap(),
        resources.clone(),
        NativeBinaries {
            asr_worker: target.join("mluva-asr-worker"),
            audio_cleanup: target.join("mluva-audio-cleanup"),
            screenshot_editor: directory.join("missing-installed-editor"),
        },
        ApplicationPlatform {
            compact_recording: Rc::new(|_| {}),
            close: Rc::new(|| {}),
        },
    )
    .unwrap();
    owner.shell.present();
    until(|| owner.capture.page.record_button.get_sensitive());
    application.activate_action("record", None);
    until(|| {
        owner.capture.phase() == Some(CapturePhase::Recording)
            && tools.join("raw.ready.json").exists()
    });
    let capture = owner.capture.session_identifier().unwrap();
    application.activate_action("screenshot", None);
    until(|| directory.join("ready.json").exists());
    application.activate_action("cancel", None);
    assert_eq!(owner.capture.phase(), Some(CapturePhase::Cancelling));
    fs::write(directory.join("release"), b"").unwrap();
    until(|| !alive(read(directory.join("ready.json"))["pid"].as_u64().unwrap() as u32));
    settle();
    assert!(
        editor_records(&directory).is_empty(),
        "cancelled capture opened a late screenshot editor"
    );
    assert!(
        services
            .screenshots
            .recent(&capture, true)
            .unwrap()
            .is_empty()
    );
    assert_eq!(owner.capture.phase(), Some(CapturePhase::Cancelling));
    let first_picker = read(directory.join("ready.json"))["pid"].clone();
    application.activate_action("screenshot", None);
    settle();
    assert_eq!(
        read(directory.join("ready.json"))["pid"],
        first_picker,
        "new screenshot picker started during cancellation"
    );
    until(|| owner.capture.phase().is_none());
    glib::MainContext::default()
        .block_on(owner.shutdown())
        .unwrap();
    assert!(services.history.recent(100).unwrap().is_empty());
    assert!(services.screenshots.pending_captures().unwrap().is_empty());
    assert!(!alive(
        read(tools.join("raw.ready.json"))["pid"].as_u64().unwrap() as u32
    ));
    assert!(peer.finish().is_empty());
    eprintln!(
        "native delayed audio cancellation: late picker selection never attaches or opens an editor"
    );
}

fn metadata_failure_recovers_images(
    root: &Path,
    target: &Path,
    resources: &DocumentResources,
    fixture: &Value,
) {
    let row = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["params"]["name"] == "silent-with-image")
        .unwrap();
    let directory = root.join("application-screenshot-case");
    fs::remove_dir_all(&directory).unwrap();
    let tools = directory.join("tools");
    fs::create_dir_all(&tools).unwrap();
    for (name, binary) in [
        ("pw-record", target.join("audio-fixture-peer")),
        ("omarchy", target.join("screenshot-picker-fixture-peer")),
    ] {
        symlink(binary, tools.join(name)).unwrap();
    }
    let installed_editor = directory.join("mluva-screenshot-editor");
    symlink(
        target.join("examples/screenshot_editor_peer"),
        &installed_editor,
    )
    .unwrap();
    write(&tools.join("test-config.json"), &row["pcm"]);
    write(&directory.join("picker.json"), &row["selection"]);
    let mut peer = http::Peer::new(row["responses"].as_array().unwrap());
    let mut document = row["config"].clone();
    document["transcription_base_url"] = json!(format!("{}/v1", peer.address));
    document["litellm_base_url"] = json!(peer.address);
    let config: AppConfig = serde_json::from_value(document).unwrap();
    let paths = AppPaths {
        config: directory.join("config/mluva"),
        data: directory.join("data/mluva"),
        runtime: directory.join("runtime/mluva"),
    };
    config.save(&paths.config.join("config.json")).unwrap();
    let services = ApplicationServices::open(paths.clone()).unwrap();
    let application = adw::Application::builder()
        .application_id("com.mluva.ScreenshotStorageFailure")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    application.register(None::<&gio::Cancellable>).unwrap();
    let owner = ApplicationDesktop::new(
        &application,
        services.clone(),
        DesktopRuntime::new().unwrap(),
        resources.clone(),
        NativeBinaries {
            asr_worker: target.join("mluva-asr-worker"),
            audio_cleanup: target.join("mluva-audio-cleanup"),
            screenshot_editor: installed_editor,
        },
        ApplicationPlatform {
            compact_recording: Rc::new(|_| {}),
            close: Rc::new(|| {}),
        },
    )
    .unwrap();
    owner.shell.present();
    until(|| owner.capture.page.record_button.get_sensitive());
    application.activate_action("record", None);
    until(|| owner.capture.phase() == Some(CapturePhase::Recording));
    let capture = owner.capture.session_identifier().unwrap();
    application.activate_action("screenshot", None);
    until(|| directory.join("ready.json").exists());
    fs::write(directory.join("release"), b"").unwrap();
    until(|| !editor_records(&directory).is_empty());
    let shot = services
        .screenshots
        .recent(&capture, true)
        .unwrap()
        .remove(0);
    assert_eq!(fs::read(&shot.path).unwrap(), unhex(&fixture["red_png"]));
    // This is a real private SQLite write failure, not a mocked save callback.
    let database = rusqlite::Connection::open(&services.history.database.path).unwrap();
    database.execute_batch("CREATE TRIGGER reject_history BEFORE INSERT ON transcription_history BEGIN SELECT RAISE(ABORT, 'synthetic metadata write failure'); END;").unwrap();
    owner.capture.page.record_button.emit_clicked();
    until(|| owner.capture.phase().is_none());
    assert!(services.history.recent(100).unwrap().is_empty());
    assert_eq!(
        services.screenshots.pending_captures().unwrap(),
        std::slice::from_ref(&capture)
    );
    assert_eq!(
        services.screenshots.recent(&capture, true).unwrap(),
        std::slice::from_ref(&shot)
    );
    glib::MainContext::default()
        .block_on(owner.shutdown())
        .unwrap();
    assert_eq!(fs::read(&shot.path).unwrap(), unhex(&fixture["red_png"]));
    assert!(!alive(
        read(tools.join("raw.ready.json"))["pid"].as_u64().unwrap() as u32
    ));
    assert!(!alive(
        read(directory.join("ready.json"))["pid"].as_u64().unwrap() as u32
    ));
    let editor = editor_records(&directory).remove(0).1["pid"]
        .as_u64()
        .unwrap() as i32;
    assert!(alive(editor as u32));
    database
        .execute_batch("DROP TRIGGER reject_history;")
        .unwrap();
    drop(database);
    let reopened = ApplicationServices::open(paths).unwrap();
    let entries = reopened.history.recent(100).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].raw_text, "");
    assert_eq!(
        entries[0].delivered_text,
        "Screenshots from an interrupted narration."
    );
    assert_eq!(entries[0].delivery_outcome, "failed");
    assert!(reopened.screenshots.pending_captures().unwrap().is_empty());
    assert_eq!(
        reopened
            .screenshots
            .recent(&entries[0].identifier, false)
            .unwrap(),
        std::slice::from_ref(&shot)
    );
    assert_eq!(fs::read(&shot.path).unwrap(), unhex(&fixture["red_png"]));
    unsafe {
        libc::kill(editor, libc::SIGTERM);
        libc::waitpid(editor, std::ptr::null_mut(), 0);
    }
    let mut requests = json!(peer.finish());
    normalize_wire(&mut requests);
    assert_eq!(requests, row["requests"]);
    eprintln!(
        "native SQLite failure: selected pixels survive exit and recover on reopen; installed editor path exercised"
    );
}
