//! Actual application settings/actions, portal protocol and separate text-target receipts.
use adw::prelude::*;
use glib::variant::ToVariant;
use mluva_core::config::{AppConfig, AppPaths};
use mluva_gtk::{
    application::{ApplicationDesktop, ApplicationPlatform},
    async_runtime::DesktopRuntime,
    document_layout::DocumentResources,
    text_target::FocusedTextTargetTracker,
    theme::ThemeController,
};
use mluva_workflows::{
    capture::CapturePhase,
    services::{ApplicationServices, NativeBinaries},
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    fs,
    io::{BufRead, BufReader},
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    rc::Rc,
    thread,
    time::{Duration, Instant},
};
#[path = "support/capture_ui.rs"]
#[allow(dead_code)]
mod capture_ui;
#[path = "../../mluva-workflows/tests/support/http.rs"]
mod http;
#[path = "support/text_transport.rs"]
#[allow(dead_code)]
mod text_transport;

#[track_caller]
fn until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while !predicate() {
        assert!(
            Instant::now() < deadline,
            "Application/portal did not settle"
        );
        while glib::MainContext::default().pending() {
            glib::MainContext::default().iteration(false);
        }
        thread::sleep(Duration::from_millis(2));
    }
}
fn settle() {
    text_transport::settle(Duration::from_millis(150));
}
struct Portal {
    child: Child,
    bus: gio::DBusConnection,
}
impl Portal {
    fn new() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_global-shortcut-fixture-peer"))
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut ready = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut ready)
            .unwrap();
        assert_eq!(ready.trim(), "portal-ready");
        let bus = gio::DBusConnection::for_address_sync(
            &std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap(),
            gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
                | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
            None,
            None::<&gio::Cancellable>,
        )
        .unwrap();
        Self { child, bus }
    }
    fn control(&self, method: &str, value: Option<&str>) -> Value {
        let parameters = value.map(|value| (value,).to_variant());
        let reply = self
            .bus
            .call_sync(
                Some("org.freedesktop.portal.Desktop"),
                "/com/mluva/TestPortal",
                "com.mluva.TestPortal",
                method,
                parameters.as_ref(),
                None,
                gio::DBusCallFlags::NONE,
                2000,
                None::<&gio::Cancellable>,
            )
            .unwrap();
        if method == "Snapshot" {
            serde_json::from_str(&reply.get::<(String,)>().unwrap().0).unwrap()
        } else {
            Value::Null
        }
    }
    fn drive(&self, action: Value) {
        self.control("Drive", Some(&action.to_string()));
    }
    fn snapshot(&self) -> Value {
        self.control("Snapshot", None)
    }
}
impl Drop for Portal {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
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
fn snapshot(
    owner: &ApplicationDesktop,
    services: &ApplicationServices,
    peer: &text_transport::Peer,
    audio: Option<&Path>,
    browser: bool,
) -> Value {
    let mut target = peer.observed();
    if browser {
        target = target["observed"].clone();
    } else {
        target.as_object_mut().unwrap().remove("serial");
        target.as_object_mut().unwrap().remove("pid");
    }
    let prefs = &owner.settings.capture;
    json!({"ui":capture_ui::observe(&owner.capture.page,audio),"hint":owner.capture.page.action_hint.label().as_str(),"approval":prefs.shortcut_status.subtitle().map(String::from),"latest":prefs.latest_shortcut_status.subtitle().map(String::from),"key":prefs.recording_key.selected(),"key_sensitive":prefs.recording_key.get_sensitive(),"saved_key":AppConfig::load(&services.paths.config.join("config.json")).unwrap().global_recording_key,"provider":services.config().transcription_provider,"page":owner.shell.stack.visible_child_name().map(String::from),"visible":owner.shell.window.get_visible(),"history":services.history.recent(100).unwrap().into_iter().rev().map(|e|json!({"raw":e.raw_text,"output":e.delivered_text,"outcome":e.delivery_outcome})).collect::<Vec<_>>(),"clipboard":clipboard(),"target":target})
}

fn platform() -> ApplicationPlatform {
    ApplicationPlatform {
        compact_recording: Rc::new(|_| {}),
        close: Rc::new(|| {}),
    }
}

fn key_changed_while_readiness_is_pending(
    root: &Path,
    tools: &Path,
    target: &Path,
    resources: &DocumentResources,
    portal: &Portal,
) {
    portal.control("Reset", Some("normal"));
    let directory = tempfile::tempdir_in(root).unwrap();
    fs::write(
        directory.path().join("keyring.json"),
        serde_json::to_vec(&json!({"lookup":{"sleep_ms":1000,"stdout":"synthetic-startup-key\n"}}))
            .unwrap(),
    )
    .unwrap();
    let quote = |path: &Path| format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"));
    let helper = tools.join("secret-tool");
    fs::write(
        &helper,
        format!(
            "#!/bin/sh\nexec env CREDENTIAL_FIXTURE_ROOT={} {} \"$@\"\n",
            quote(directory.path()),
            quote(&target.join("credential-fixture-peer"))
        ),
    )
    .unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
    let paths = AppPaths {
        config: directory.path().join("config"),
        data: directory.path().join("data"),
        runtime: directory.path().join("runtime"),
    };
    AppConfig {
        transcription_provider: "elevenlabs".into(),
        rewrite_provider: "none".into(),
        welcome_completed: true,
        automatic_titles: false,
        ..AppConfig::default()
    }
    .save(&paths.config.join("config.json"))
    .unwrap();
    let services = ApplicationServices::open(paths).unwrap();
    let application = adw::Application::builder()
        .application_id("com.mluva.ShortcutPendingReadiness")
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
            screenshot_editor: target.join("mluva-screenshot-editor"),
        },
        platform(),
    )
    .unwrap();
    owner.shell.present();
    until(|| directory.path().join("active.pid").exists());
    let pid = fs::read_to_string(directory.path().join("active.pid")).unwrap();
    assert!(Path::new(&format!("/proc/{pid}")).exists());
    assert!(!owner.capture.page.record_button.get_sensitive());
    assert!(portal.snapshot()["trace"].as_array().unwrap().is_empty());
    owner.settings.capture.recording_key.set_selected(10);
    until(|| owner.capture.page.record_button.get_sensitive());
    fs::write(root.join("pending-readiness-before-approval.json"), serde_json::to_vec_pretty(&json!({"capture_ready":owner.capture.page.record_button.get_sensitive(),"key":services.config().global_recording_key,"approval":owner.settings.capture.shortcut_status.subtitle().map(String::from),"portal":portal.snapshot()})).unwrap()).unwrap();
    until(|| {
        owner
            .settings
            .capture
            .shortcut_status
            .subtitle()
            .unwrap()
            .contains("press F11")
    });
    assert_eq!(services.config().global_recording_key, "F11");
    assert_eq!(owner.capture.page.status.label(), "Ready");
    let observed = portal.snapshot();
    let bindings = observed["trace"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["method"] == "BindShortcuts")
        .collect::<Vec<_>>();
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0]["shortcuts"][0]["id"], "toggle-recording-f11");
    let drained = Rc::new(Cell::new(false));
    let done = drained.clone();
    let shutdown = owner.shutdown();
    glib::MainContext::default().spawn_local(async move {
        shutdown.await.unwrap();
        done.set(true);
    });
    until(|| drained.get());
    settle();
    assert_eq!(portal.snapshot()["disconnected"], json!([1]));
    assert!(!Path::new(&format!("/proc/{pid}")).exists());
    assert!(services.history.recent(1).unwrap().is_empty());
    fs::remove_file(helper).unwrap();
    println!(
        "Native pending-readiness key change: latest preference approved once, stale readiness discarded, shutdown acknowledged"
    );
}
#[test]
#[ignore = "requires private display/buses/network/devices and native portal/audio/text peers"]
fn released_application_portal_actions_settings_and_target_delivery() {
    let root =
        PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").expect("guarded runner required"));
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
    for path in ["/dev/input", "/dev/uinput", "/dev/snd", "/dev/dri"] {
        assert!(!Path::new(path).exists());
    }
    assert!(std::env::var_os("MLUVA_DISABLE_GLOBAL_SHORTCUT").is_none());
    let browser = std::env::var_os("MLUVA_APPLICATION_BROWSER").is_some();
    let browser_fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-application-browser.json")).unwrap();
    if browser {
        for name in [
            "ATSPI_DISABLE_P2P",
            "ATSPI_IN_TESTS",
            "ATSPI_NO_CACHE",
            "PYATSPI_NOCACHE",
        ] {
            assert!(
                std::env::var_os(name).is_none(),
                "{name} changes default transport/cache"
            );
        }
        assert!(text_transport::audit_rows().unwrap().is_empty());
        assert!(!Path::new("/run/dbus/system_bus_socket").exists());
    }
    let selected = std::env::var("MLUVA_APPLICATION_SHORTCUT_CASE").ok();
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-application-shortcuts.json")).unwrap();
    assert_eq!(browser_fixture["reference"], fixture["reference"]);
    if selected.is_none() {
        let mut cases = fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                !browser
                    || browser_fixture["cases"]
                        .get(row["params"]["name"].as_str().unwrap())
                        .is_some()
            })
            .map(|(index, _)| index.to_string())
            .collect::<Vec<_>>();
        if !browser {
            cases.push("readiness".into());
        }
        for case in cases {
            // libatspi owns a process-global cache even after every tracker is
            // closed. Each cold application scenario needs a fresh client.
            let log_path = root.join(format!("shortcut-client-{case}.log"));
            let log = fs::File::create(&log_path).unwrap();
            // A clipboard owner can outlive the test client. Its inherited log
            // descriptor must not keep a stdout pipe's EOF waiter alive.
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "released_application_portal_actions_settings_and_target_delivery",
                    "--ignored",
                    "--test-threads=1",
                    "--nocapture",
                ])
                .env("MLUVA_APPLICATION_SHORTCUT_CASE", &case)
                .stdout(log.try_clone().unwrap())
                .stderr(log);
            if browser {
                command.env(
                    "MLUVA_TEXT_READ_AUDIT",
                    root.join(format!("text-read-audit-{case}.jsonl")),
                );
            }
            let status = command.status().unwrap();
            let output = fs::read_to_string(log_path).unwrap();
            assert!(status.success(), "case {case}: {output}");
            assert!(
                !["WARNING", "CRITICAL"]
                    .iter()
                    .any(|word| output.contains(word)),
                "case {case}: {output}"
            );
            print!("{output}");
        }
        return;
    }
    assert!(
        selected.as_deref() == Some("readiness")
            || selected
                .as_ref()
                .and_then(|case| case.parse::<usize>().ok())
                .is_some_and(|index| index < fixture["cases"].as_array().unwrap().len())
    );
    let tools = root.join("application-shortcut-tools");
    assert_eq!(
        std::env::split_paths(&std::env::var_os("PATH").unwrap()).next(),
        Some(tools.clone())
    );
    fs::create_dir_all(&tools).unwrap();
    let target = PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap()).join("debug");
    if !tools.join("pw-record").exists() {
        symlink(target.join("audio-fixture-peer"), tools.join("pw-record")).unwrap();
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
    // Export this process's real GTK accessibility cache before libatspi scans
    // the desktop. Keep it alive across the tested application owner's exit.
    let accessibility_window = gtk::Window::builder()
        .title("Private accessibility lifetime")
        .default_width(1)
        .default_height(1)
        .decorated(false)
        .build();
    accessibility_window.present();
    settle();
    let resources = DocumentResources::from_directory(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources"),
    );
    let _theme = ThemeController::apply(
        root.join("state/omarchy/current/theme"),
        resources.font.parent().unwrap(),
    )
    .unwrap();
    let portal = Portal::new();
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
        if selected.as_deref() != Some(index.to_string().as_str()) {
            continue;
        }
        let params = &row["params"];
        let name = params["name"].as_str().unwrap();
        let browser_case = browser.then(|| &browser_fixture["cases"][name]);
        assert!(!browser || !browser_case.unwrap().is_null());
        portal.control("Reset", Some(params["portal"].as_str().unwrap_or("normal")));
        let directory = tempfile::tempdir_in(&root).unwrap();
        let _ = fs::remove_file(tools.join("raw.ready.json"));
        fs::write(
            tools.join("test-config.json"),
            serde_json::to_vec(&row["pcm"]).unwrap(),
        )
        .unwrap();
        let mut responses = row["responses"].as_array().unwrap().clone();
        for response in &mut responses {
            response["delay_ms"] = json!(350);
        }
        let mut http = http::Peer::new(&responses);
        let mut cfg = row["config"].clone();
        cfg["transcription_base_url"] = json!(format!("{}/v1", http.address));
        let config: AppConfig = serde_json::from_value(cfg).unwrap();
        let paths = AppPaths {
            config: directory.path().join("config/mluva"),
            data: directory.path().join("data/mluva"),
            runtime: directory.path().join("runtime/mluva"),
        };
        config.save(&paths.config.join("config.json")).unwrap();
        let services = ApplicationServices::open(paths).unwrap();
        let mut peer = text_transport::Peer::new(
            directory.path().join("peer"),
            if browser {
                "firefox_text_peer"
            } else {
                "text_target_peer"
            },
        );
        let browser_pid = browser.then(|| {
            let metadata = &peer.observed()["metadata"];
            assert_eq!(metadata["version"], browser_fixture["browser"]["version"]);
            assert_eq!(metadata["build_id"], browser_fixture["browser"]["build_id"]);
            let pid = metadata["pid"].as_u64().unwrap();
            assert_eq!(
                fs::read_link(format!("/proc/{pid}/exe")).unwrap(),
                PathBuf::from(metadata["identity"].as_str().unwrap())
            );
            pid
        });
        let application = adw::Application::builder()
            .application_id(format!("com.mluva.ShortcutAcceptance{index}"))
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        application.register(None::<&gio::Cancellable>).unwrap();
        let runtime = DesktopRuntime::new().unwrap();
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
            platform(),
        )
        .unwrap();
        owner.shell.present();
        if params["repair"] == true {
            until(|| owner.capture.page.view_state().initialization_failed);
        } else if params["portal"] == "pending-bind" {
            until(|| {
                portal.snapshot()["trace"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v["method"] == "BindShortcuts")
            });
        } else if params["portal"] == "deny-bind" {
            until(|| {
                owner
                    .settings
                    .capture
                    .shortcut_status
                    .subtitle()
                    .unwrap()
                    .contains("not currently approved")
            });
        } else {
            until(|| {
                owner
                    .settings
                    .capture
                    .latest_shortcut_status
                    .subtitle()
                    .unwrap()
                    .contains("opens the latest conversation")
            });
        }
        settle();
        gtk::gdk::Display::default()
            .unwrap()
            .clipboard()
            .set_text("untouched shortcut clipboard");
        let mut stage_index = 0;
        let mut stage = |label: &str, audio: Option<&Path>, peer: &text_transport::Peer| {
            let actual =
                json!({"stage":label,"state":snapshot(&owner,&services,peer,audio,browser)});
            let mut expected = row["stages"][stage_index].clone();
            if let Some(browser_case) = browser_case {
                let stage = &browser_case["stages"][stage_index];
                assert_eq!(stage["stage"], label);
                expected["state"]["target"] =
                    browser_fixture["targets"][stage["target"].as_u64().unwrap() as usize].clone();
                for field in ["status", "tooltip"] {
                    if let Some(value) = stage.get(field) {
                        expected["state"]["ui"][field] = value.clone();
                    }
                }
            }
            fs::write(
                root.join(format!("{name}-{label}-actual.json")),
                serde_json::to_vec_pretty(&actual).unwrap(),
            )
            .unwrap();
            assert_eq!(actual, expected, "{name}: {label}");
            stage_index += 1;
            count += 1;
        };
        stage("startup", None, &peer);
        if params["repair"] == true {
            application.activate_action("settings", None);
            let providers = &owner.settings.providers;
            providers.speech.provider_row.set_selected(2);
            providers.apply_button.emit_clicked();
            until(|| owner.capture.page.record_button.get_sensitive());
            settle();
            stage("repaired-without-shortcuts", None, &peer);
            assert!(portal.snapshot()["trace"].as_array().unwrap().is_empty());
            owner.capture.page.setup_button.emit_clicked();
            until(|| {
                owner
                    .settings
                    .capture
                    .latest_shortcut_status
                    .subtitle()
                    .unwrap()
                    .contains("opens the latest conversation")
            });
            settle();
            stage("acknowledged-and-approved", None, &peer);
        }
        if params["providers"] == true {
            application.activate_action("settings", None);
            let providers = &owner.settings.providers;
            providers.speech.provider_row.set_selected(0);
            providers.apply_button.emit_clicked();
            until(|| !owner.capture.page.record_button.get_sensitive());
            settle();
            stage("missing-credentials-retains-shortcut", None, &peer);
            providers.speech.provider_row.set_selected(2);
            providers.apply_button.emit_clicked();
            until(|| owner.capture.page.record_button.get_sensitive());
            settle();
            stage("provider-restored", None, &peer);
        }
        if params["settings"] == true {
            owner.settings.capture.recording_key.set_selected(10);
            stage("requesting-F11", None, &peer);
            until(|| {
                owner
                    .settings
                    .capture
                    .shortcut_status
                    .subtitle()
                    .unwrap()
                    .contains("press F11")
            });
            settle();
            stage("approved-F11", None, &peer);
            portal.drive(json!({"op":"Activated","id":"toggle-recording-f9","session":1}));
            settle();
            stage("old-session-ignored", None, &peer);
            portal.drive(json!({"op":"changed","shortcuts":[{"id":"toggle-recording-f11","trigger":"F12"},{"id":"open-rewrite","trigger":"CTRL+R"}]}));
            until(|| {
                owner
                    .settings
                    .capture
                    .shortcut_status
                    .subtitle()
                    .unwrap()
                    .contains("desktop assigned F12")
            });
            settle();
            stage("desktop-changed-keys", None, &peer);
            owner.settings.capture.auto_paste.set_active(false);
            settle();
            stage("automatic-paste-off", None, &peer);
            owner.shell.window.close();
            settle();
            stage("hidden-resident", None, &peer);
            portal.drive(json!({"op":"Activated","id":"open-rewrite"}));
            until(|| owner.shell.window.get_visible());
            settle();
            stage("latest-reopens", None, &peer);
        }
        let mut audio = None;
        let mut pid = None;
        if let Some(capture) = params["capture"].as_str() {
            peer.request(json!({"operation":"setup","kind":"entry","text":"Before 🐎 after","start":7,"end":if browser {9} else {8},"editable":true}));
            stage("focused-target", None, &peer);
            if capture == "manual" {
                application.activate_action("record", None);
            } else {
                portal.drive(json!({"op":"Activated","id":"toggle-recording-f9"}));
            }
            until(|| {
                owner.capture.phase() == Some(CapturePhase::Recording)
                    && tools.join("raw.ready.json").exists()
            });
            let ready: Value =
                serde_json::from_slice(&fs::read(tools.join("raw.ready.json")).unwrap()).unwrap();
            pid = Some(ready["pid"].as_u64().unwrap());
            audio = Some(services.paths.data.join("recordings").join(format!(
                "{}.wav",
                owner.capture.session_identifier().unwrap()
            )));
            settle();
            stage("recording", audio.as_deref(), &peer);
            if capture == "global" {
                portal.drive(json!({"op":"Activated","id":"toggle-recording-f9"}));
                settle();
                stage("held-key-does-not-stop", audio.as_deref(), &peer);
                owner.settings.capture.recording_key.set_selected(10);
                stage("active-key-change-rejected", audio.as_deref(), &peer);
            }
            if capture == "cancel" {
                portal.drive(json!({"op":"Activated","id":"cancel-capture"}));
            } else if params["portal"] == "deny-bind" {
                owner.capture.page.record_button.emit_clicked();
            } else {
                portal.drive(json!({"op":"Deactivated","id":"toggle-recording-f9"}));
                portal.drive(json!({"op":"Activated","id":"toggle-recording-f9"}));
            }
            until(|| owner.capture.phase().is_none());
            settle();
            stage("completed", audio.as_deref(), &peer);
            if capture == "global" {
                if !browser {
                    assert_eq!(
                        peer.observed()["entry"]["text"],
                        "Before Portal dictation keeps 12 files. after"
                    );
                }
                application.activate_action("history", None);
                settle();
                portal.drive(json!({"op":"Activated","id":"open-rewrite"}));
                until(|| owner.shell.stack.visible_child_name().as_deref() == Some("capture"));
                settle();
                stage("latest-conversation", audio.as_deref(), &peer);
            } else if !browser {
                assert_eq!(peer.observed()["entry"]["text"], "Before 🐎 after");
            }
        }
        if browser {
            assert_eq!(
                stage_index,
                browser_case.unwrap()["stages"].as_array().unwrap().len()
            );
            assert!(
                text_transport::audit_rows().unwrap().is_empty(),
                "automatic recording must not read target text"
            );
            let mut probe = FocusedTextTargetTracker::new().unwrap();
            peer.request(json!({"operation":"setup","kind":"textview","text":"Audit: 🐎 guard","start":7,"end":9}));
            let selected = probe
                .capture_text_target(2000)
                .unwrap()
                .expect("explicit read control");
            assert_eq!(
                json!({"text":selected.selected_text(),"start":selected.selection().unwrap().0,"end":selected.selection().unwrap().1,"caret":selected.caret_offset()}),
                browser_fixture["control"]
            );
            probe.close();
            let rows = text_transport::audit_rows().unwrap();
            assert_eq!(
                rows.len(),
                1,
                "explicit GetText control must observe one real request"
            );
            assert_eq!(rows[0]["pid"], std::process::id());
            assert_eq!(rows[0]["valid"], true);
            assert_eq!(rows[0]["start"], 7);
            assert_eq!(rows[0]["end"], 9);
            assert!(rows[0]["peer"].is_boolean());
        }
        assert_eq!(stage_index, row["stages"].as_array().unwrap().len());
        let drained = Rc::new(Cell::new(false));
        let done = drained.clone();
        let shutdown = owner.shutdown();
        glib::MainContext::default().spawn_local(async move {
            shutdown.await.unwrap();
            done.set(true);
        });
        until(|| drained.get());
        settle();
        if let Some(pid) = pid {
            assert!(!Path::new(&format!("/proc/{pid}")).exists());
        }
        if let Some(audio) = audio {
            assert!(!audio.exists());
        }
        if browser {
            assert_eq!(
                portal.snapshot()["disconnected"],
                json!([2]),
                "Mluva disconnects before its browser"
            );
            peer.request(json!({"operation":"quit"}));
            peer.await_successful_exit();
            assert!(!Path::new(&format!("/proc/{}", browser_pid.unwrap())).exists());
            until(|| portal.snapshot()["disconnected"] == json!([2, 1]));
            assert_eq!(
                portal.snapshot(),
                browser_fixture["portals"]
                    [browser_case.unwrap()["portal"].as_u64().unwrap() as usize],
                "{name}: all portal owners close"
            );
            assert!(!text_transport::bus_name_has_owner("org.a11y.Bus"));
            assert_eq!(
                text_transport::audit_rows().unwrap().len(),
                1,
                "include all lifecycle reads"
            );
        } else {
            assert_eq!(portal.snapshot(), row["portal"], "{name}: portal cleanup");
        }
        assert_eq!(
            json!(http.finish()),
            row["requests"],
            "{name}: provider requests"
        );
        peer.stop();
        println!("{name}: {stage_index} application states PASS");
    }
    if selected.as_deref() == Some("readiness") {
        key_changed_while_readiness_is_pending(&root, &tools, &target, &resources, &portal);
    } else {
        println!("Compared {count} GTK/target/store states in a fresh application process");
    }
    accessibility_window.destroy();
}
