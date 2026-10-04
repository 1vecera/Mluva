//! Real native owner/widgets, external PCM processes, HTTP and local archives.
use adw::prelude::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use mluva_audio::catalog::PipeWireDeviceCatalog;
use mluva_core::{
    config::{AppConfig, AppPaths},
    meeting::MeetingRecord,
};
use mluva_gtk::{
    async_runtime::DesktopRuntime,
    capture_preferences::{CapturePreferences, PreferenceActivity},
    capture_view::CapturePage,
    document_layout::DocumentResources,
    meeting_controller::{MeetingController, MeetingControllerCallbacks},
    theme::ThemeController,
};
use mluva_providers::Secret;
use mluva_workflows::{
    meeting_services::MeetingServices, meeting_session::MeetingPhase, services::ApplicationServices,
};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
#[path = "../../mluva-workflows/tests/support/http.rs"]
mod http;

fn wait(predicate: impl Fn() -> bool) {
    let end = Instant::now() + Duration::from_secs(8);
    while !predicate() {
        assert!(Instant::now() < end, "Meeting did not settle");
        while glib::MainContext::default().pending() {
            glib::MainContext::default().iteration(false);
        }
        thread::sleep(Duration::from_millis(2));
    }
}
fn descendants(widget: &impl IsA<gtk::Widget>) -> Vec<gtk::Widget> {
    fn walk(widget: gtk::Widget, out: &mut Vec<gtk::Widget>) {
        out.push(widget.clone());
        let mut child = widget.first_child();
        while let Some(current) = child {
            walk(current.clone(), out);
            child = current.next_sibling();
        }
    }
    let mut out = vec![];
    walk(widget.as_ref().clone(), &mut out);
    out
}
#[path = "support/capture_graph.rs"]
mod capture_graph;
use capture_graph::graph;
fn normalize(value: Value, root: &Path) -> Value {
    match value {
        Value::String(value) => {
            let value = value.replace(root.to_str().unwrap(), "$ROOT");
            let expression =
                regex::Regex::new(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}")
                    .unwrap();
            json!(expression.replace_all(&value, "generated"))
        }
        Value::Array(values) => {
            Value::Array(values.into_iter().map(|v| normalize(v, root)).collect())
        }
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(k, v)| {
                    (
                        normalize(json!(k), root).as_str().unwrap().to_owned(),
                        normalize(v, root),
                    )
                })
                .collect(),
        ),
        other => other,
    }
}
fn observe(
    owner: &MeetingController,
    services: &ApplicationServices,
    page: &CapturePage,
    prefs: &CapturePreferences,
    root: &Path,
) -> Value {
    fn waves(directory: &Path, root: &Path, out: &mut serde_json::Map<String, Value>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                waves(&path, root, out);
            } else if path.extension().is_some_and(|e| e == "wav") {
                out.insert(
                    path.strip_prefix(root).unwrap().to_str().unwrap().into(),
                    json!(STANDARD.encode(fs::read(&path).unwrap())),
                );
            }
        }
    }
    let processing = owner.phase() == Some(MeetingPhase::Processing);
    let retrying = owner.retry_identifier().is_some();
    let asynchronous = processing || retrying;
    let record = &owner.page.record_button;
    let widgets = descendants(record);
    let mut records = services
        .meetings
        .lock()
        .unwrap()
        .meetings()
        .iter()
        .map(MeetingRecord::document)
        .collect::<Vec<_>>();
    for record in &mut records {
        record["timestamp"] = json!("generated");
    }
    let connection = rusqlite::Connection::open(&services.diagnostics.path).unwrap();
    let mut statement = connection
        .prepare("SELECT mode,stage,provider,outcome FROM diagnostic_events ORDER BY sequence")
        .unwrap();
    let events = statement
        .query_map([], |row| {
            Ok(vec![
                row.get::<_, String>(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
            ])
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let mut files = serde_json::Map::new();
    waves(root, root, &mut files);
    let buffer = page.output_view.buffer();
    normalize(
        json!({"status":owner.page.status.label().to_string(),"record":{"labels":widgets.iter().filter_map(|w|w.downcast_ref::<gtk::Label>()).map(|w|w.label().to_string()).collect::<Vec<_>>(),
        "sensitive":record.is_sensitive(),"suggested":record.has_css_class("suggested-action"),"destructive":record.has_css_class("destructive-action")},
        "privacy":[owner.page.privacy.title.label().to_string(),owner.page.privacy.subtitle.label().to_string()],
        "controls":{"mode":prefs.mode.is_sensitive(),"key":prefs.recording_key.is_sensitive(),"incognito":prefs.incognito.is_sensitive(),"language":prefs.language.is_sensitive(),"microphone":prefs.microphone.is_sensitive(),"system":prefs.system_audio.is_sensitive(),"refresh":prefs.refresh_audio.is_sensitive(),"audio_retention":prefs.audio_retention.is_sensitive(),"history_retention":prefs.history_retention.is_sensitive(),"spoken":prefs.spoken_commands.is_sensitive(),"remember":prefs.remember_application.is_sensitive()},
        "capture_sensitive":page.record_button.is_sensitive(),"output":buffer.text(&buffer.start_iter(),&buffer.end_iter(),true).to_string(),"records":records,
        "diagnostics":if asynchronous {json!("in-flight")}else{json!(events)},"wave_files":if asynchronous{json!("in-flight")}else{json!(files)},
        "recording":if asynchronous{json!("in-flight")}else{json!(owner.phase()==Some(MeetingPhase::Recording))},"processing":processing,"retrying":retrying}),
        root,
    )
}
fn compare(actual: Value, expected: &Value, label: &str, private: &Path) {
    if &actual != expected {
        fs::write(
            private.join("meeting-controller-actual.json"),
            serde_json::to_vec_pretty(&actual).unwrap(),
        )
        .unwrap();
        fs::write(
            private.join("meeting-controller-expected.json"),
            serde_json::to_vec_pretty(expected).unwrap(),
        )
        .unwrap();
        let changed = actual
            .as_object()
            .unwrap()
            .iter()
            .filter(|(k, v)| expected.get(*k) != Some(*v))
            .map(|(k, _)| k.as_str())
            .collect::<Vec<_>>();
        panic!("{label}: changed {changed:?}; synthetic observations are in private evidence");
    }
}
#[test]
#[ignore = "Requires the guarded private desktop/network/PID/device environment and built native audio peers"]
fn released_meeting_capture_retry_and_idle_transitions_match() {
    let private = PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").unwrap());
    for key in ["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XAUTHORITY"] {
        assert!(PathBuf::from(std::env::var_os(key).unwrap()).starts_with(&private));
    }
    for node in ["/dev/input", "/dev/uinput", "/dev/snd", "/dev/dri"] {
        assert!(!Path::new(node).exists());
    }
    assert_ne!(
        fs::read_link("/proc/self/ns/net")
            .unwrap()
            .to_str()
            .unwrap(),
        std::env::var("MLUVA_HOST_NET_NS").unwrap()
    );
    assert!(std::env::var_os("WAYLAND_DISPLAY").is_none());
    assert!(std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none());
    gtk::init().unwrap();
    adw::init().unwrap();
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceLight);
    let resources = DocumentResources::from_directory(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources"),
    );
    let _theme =
        ThemeController::apply(private.join("theme"), resources.font.parent().unwrap()).unwrap();
    let runtime = DesktopRuntime::new().unwrap();
    let target = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned();
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-meeting-controller.json")).unwrap();
    let mut states = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let directory = tempfile::tempdir_in(&private).unwrap();
        let root = directory.path();
        let paths = AppPaths {
            config: root.join("config"),
            data: root.join("data"),
            runtime: root.join("runtime"),
        };
        fs::create_dir_all(&paths.config).unwrap();
        AppConfig {
            language_code: "ces".into(),
            rewrite_provider: "none".into(),
            incognito_mode: case["incognito"] == true,
            ..AppConfig::default()
        }
        .save(&paths.config.join("config.json"))
        .unwrap();
        let services = ApplicationServices::open(paths).unwrap();
        let (page, prefs) = graph(services.clone(), resources.clone());
        let owner = MeetingController::new(
            services.clone(),
            runtime.clone(),
            page.clone(),
            prefs.clone(),
            PipeWireDeviceCatalog::default(),
            MeetingControllerCallbacks {
                activity: Rc::new(PreferenceActivity::default),
                idle: Rc::new(|| {}),
                copy: Rc::new(|_| {}),
            },
        )
        .unwrap();
        let audio_root = tempfile::tempdir_in(&private).unwrap();
        let config_path = audio_root.path().join("test-config.json");
        fs::write(
            &config_path,
            serde_json::to_vec(&case["audio"]["config"]).unwrap(),
        )
        .unwrap();
        fs::set_permissions(config_path, fs::Permissions::from_mode(0o600)).unwrap();
        let executable = audio_root.path().join("pw-record");
        symlink(target.join("audio-fixture-peer"), &executable).unwrap();
        let mut responses = case["responses"].as_array().unwrap().clone();
        for response in &mut responses {
            response["delay_ms"] = json!(300);
        }
        let mut peer = http::Peer::new(&responses);
        owner.set_services(Some(Arc::new(
            MeetingServices::new(
                Secret::new("synthetic-meeting-key"),
                format!("{}/speech-to-text", peer.address),
                executable,
                target.join("mluva-audio-cleanup"),
                services.meetings.clone(),
            )
            .with_diagnostics(services.diagnostics.clone()),
        )));
        let window = adw::Window::builder()
            .title("Mluva")
            .default_width(1060)
            .default_height(780)
            .content(&owner.page.widget)
            .build();
        window.present();
        let mut pids = vec![];
        for stage in case["stages"].as_array().unwrap() {
            let action = stage["action"].as_str().unwrap();
            match action {
                "initial" => {}
                "recording" => {
                    owner.page.record_button.emit_clicked();
                    wait(|| {
                        owner.phase() == Some(MeetingPhase::Recording)
                            && audio_root.path().join("system.ready.json").is_file()
                            && audio_root.path().join("microphone.ready.json").is_file()
                            && owner.page.record_button.is_sensitive()
                    });
                    for name in ["microphone", "system"] {
                        let ready: Value = serde_json::from_slice(
                            &fs::read(audio_root.path().join(format!("{name}.ready.json")))
                                .unwrap(),
                        )
                        .unwrap();
                        pids.push(ready["pid"].as_u64().unwrap());
                    }
                }
                "processing" => {
                    if case["index_failure"] == true {
                        let path = &services.meetings.lock().unwrap().path;
                        fs::create_dir(path).unwrap();
                        fs::write(path.join("preserved"), "blocked index").unwrap();
                    }
                    owner.page.record_button.emit_clicked();
                }
                "processing-guard" | "retry-start-guard" => owner.toggle(),
                "terminal" | "retry-terminal" => wait(|| !owner.busy()),
                "retrying" => {
                    let record = services.meetings.lock().unwrap().meetings()[0].clone();
                    owner.retry_record(&record);
                }
                "delete-guard" => {
                    let record = services.meetings.lock().unwrap().meetings()[0].clone();
                    assert!(!owner.delete(&record));
                }
                "cancelled" => {
                    assert!(owner.cancel());
                    wait(|| !owner.busy());
                }
                other => panic!("unknown Meeting action {other}"),
            }
            compare(
                observe(&owner, &services, &page, &prefs, root),
                &stage["observed"],
                &format!("{} {action}", case["name"]),
                &private,
            );
            states += 1;
        }
        let shutdown = owner.shutdown();
        let completed = Rc::new(std::cell::Cell::new(false));
        let flag = completed.clone();
        runtime.spawn(async move {
            shutdown.await.unwrap();
            flag.set(true);
        });
        wait(|| completed.get());
        for pid in pids {
            assert!(!Path::new(&format!("/proc/{pid}")).exists());
        }
        assert_eq!(peer.finish(), case["requests"].as_array().unwrap().clone());
        window.destroy();
        drop(owner);
    }
    println!(
        "Matched {} actual released Meeting workflows/{states} GTK states",
        fixture["cases"].as_array().unwrap().len()
    );
}
