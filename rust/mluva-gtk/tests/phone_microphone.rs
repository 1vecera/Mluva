//! Real desktop, private D-Bus widget signals and independent phone IPC process.
use adw::prelude::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use mluva_core::{
    config::{AppConfig, AppPaths},
    phone::{PhoneReply, PhoneRequest},
};
use mluva_gtk::{
    application::{ApplicationDesktop, ApplicationPlatform},
    async_runtime::DesktopRuntime,
    document_layout::DocumentResources,
};
use mluva_workflows::{
    capture::CapturePhase,
    services::{ApplicationServices, NativeBinaries},
};
use serde_json::json;
use std::{
    cell::RefCell,
    io::Write,
    os::unix::fs::{PermissionsExt, symlink},
    path::PathBuf,
    process::{Command, Stdio},
    rc::Rc,
    time::{Duration, Instant},
};
#[path = "../../mluva-workflows/tests/support/http.rs"]
mod http;
#[track_caller]
fn until(mut condition: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(30);
    while !condition() {
        assert!(Instant::now() < end, "Phone desktop did not settle");
        while glib::MainContext::default().pending() {
            glib::MainContext::default().iteration(false);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    while glib::MainContext::default().pending() {
        glib::MainContext::default().iteration(false);
    }
}
fn call(target: &std::path::Path, request: PhoneRequest) -> PhoneReply {
    let mut client = Command::new(target.join("phone-fixture-peer"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    client
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&request).unwrap())
        .unwrap();
    until(|| client.try_wait().unwrap().is_some());
    let result = client.wait_with_output().unwrap();
    assert!(result.status.success());
    serde_json::from_slice(&result.stdout).unwrap()
}
#[test]
#[ignore = "requires the private desktop/device/network runner"]
fn phone_starts_native_widget_audio_stop_history_and_copy_without_pc_microphone() {
    let root =
        PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").expect("Use isolated runner"));
    assert!(std::env::var("DISPLAY").unwrap().starts_with(':'));
    assert!(PathBuf::from(std::env::var_os("XDG_CONFIG_HOME").unwrap()).starts_with(&root));
    let target = PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap()).join("debug");
    let tools = root.join("application-tools");
    std::fs::create_dir(&tools).unwrap();
    // Readiness can resolve PipeWire tools; a phone capture must never execute them.
    let sentinel = root.join("pc-microphone-opened");
    let script = format!("#!/bin/sh\ntouch '{}'\nexit 99\n", sentinel.display());
    std::fs::write(tools.join("pw-record"), script).unwrap();
    std::fs::set_permissions(
        tools.join("pw-record"),
        std::fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    symlink("/bin/false", tools.join("wpctl")).unwrap();
    gtk::init().unwrap();
    adw::init().unwrap();
    let bus = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).unwrap();
    let states = Rc::new(RefCell::new(Vec::new()));
    let received = states.clone();
    let _watch = bus.subscribe_to_signal(
        None,
        Some(mluva_gtk::overlay_state::INTERFACE),
        Some("ShellStateChanged"),
        Some(mluva_gtk::overlay_state::OBJECT_PATH),
        None,
        gio::DBusSignalFlags::NONE,
        move |signal| {
            received.borrow_mut().push(
                signal
                    .parameters
                    .child_value(0)
                    .get::<std::collections::BTreeMap<String, glib::Variant>>()
                    .unwrap(),
            );
        },
    );
    let mut peer = http::Peer::new(&[
        json!({"route":"speech","status":200,"payload":{"text":"Synthetic phone microphone."}}),
        json!({"route":"speech","status":200,"payload":{"text":"Stopped on PC."}}),
        json!({"route":"speech","status":200,"payload":{"text":"Synthetic private phone."}}),
    ]);
    let paths = AppPaths {
        config: root.join("phone/config"),
        data: root.join("phone/data"),
        runtime: root.join("phone/runtime"),
    };
    let config = AppConfig {
        transcription_provider: "litellm".into(),
        transcription_base_url: format!("{}/v1", peer.address),
        rewrite_provider: "none".into(),
        automatic_titles: false,
        welcome_completed: true,
        ..Default::default()
    };
    config.save(&paths.config.join("config.json")).unwrap();
    let services = ApplicationServices::open(paths).unwrap();
    let application = adw::Application::builder()
        .application_id("com.mluva.PhoneAcceptance")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    application.register(gio::Cancellable::NONE).unwrap();
    let owner = ApplicationDesktop::new(
        &application,
        services.clone(),
        DesktopRuntime::new().unwrap(),
        DocumentResources::from_directory(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources"),
        ),
        NativeBinaries {
            asr_worker: target.join("mluva-asr-worker"),
            audio_cleanup: target.join("mluva-audio-cleanup"),
            screenshot_editor: target.join("mluva-screenshot-editor"),
        },
        ApplicationPlatform {
            compact_recording: Rc::new(|_| {}),
            close: Rc::new(|| {}),
        },
    )
    .unwrap();
    until(|| {
        owner.capture.page.record_button.get_sensitive()
            || owner.capture.page.view_state().initialization_failed
    });
    assert!(
        !owner.capture.page.view_state().initialization_failed,
        "{}",
        owner.capture.page.status.label()
    );
    println!("phone stage: native readiness");
    assert!(
        !owner.shell.window.is_visible(),
        "phone start must not present the desktop window"
    );
    let id = "050405bf-f712-4f08-9b65-a33cc58a4a4d".to_string();
    assert_eq!(
        call(
            &target,
            PhoneRequest::Start {
                identifier: id.clone()
            }
        )
        .phase,
        "preparing"
    );
    println!(
        "phone stage: started phase {:?}, status {}",
        owner.capture.phase(),
        owner.capture.page.status.label()
    );
    until(|| {
        owner.capture.phase() == Some(CapturePhase::Recording) || owner.capture.phase().is_none()
    });
    assert_eq!(
        owner.capture.phase(),
        Some(CapturePhase::Recording),
        "{}",
        owner.capture.page.status.label()
    );
    println!("phone stage: recording");
    let pcm = STANDARD.encode(16384_i16.to_le_bytes().repeat(8000));
    assert_eq!(
        call(
            &target,
            PhoneRequest::Audio {
                identifier: id.clone(),
                sequence: 1,
                pcm: pcm.clone()
            }
        )
        .phase,
        "error"
    );
    assert_eq!(
        call(
            &target,
            PhoneRequest::Audio {
                identifier: id.clone(),
                sequence: 0,
                pcm: pcm.clone()
            }
        )
        .sequence,
        1
    );
    assert_eq!(
        call(
            &target,
            PhoneRequest::Audio {
                identifier: id.clone(),
                sequence: 0,
                pcm: pcm.clone()
            }
        )
        .sequence,
        1,
        "duplicate acknowledgement cannot append twice"
    );
    assert_eq!(
        call(
            &target,
            PhoneRequest::Start {
                identifier: "a854b08a-bc40-492f-a453-58369ecef8d6".into()
            }
        )
        .phase,
        "error"
    );
    until(|| {
        states.borrow().iter().any(|s| {
            s.get("phase").and_then(|v| v.str()) == Some("recording")
                && s.get("level").and_then(|v| v.get::<f64>()) == Some(0.5)
        })
    });
    assert!(!sentinel.exists());
    assert!(
        peer.observed.lock().unwrap().is_empty(),
        "no final batch recognition before Stop"
    );
    call(
        &target,
        PhoneRequest::Stop {
            identifier: id.clone(),
            sequence: 1,
        },
    );
    until(|| owner.capture.phase().is_none());
    let reply = call(
        &target,
        PhoneRequest::Status {
            identifier: id.clone(),
        },
    );
    assert_eq!(
        reply.phase,
        "completed",
        "{} {:?} {:?}",
        owner.capture.page.setup_body.label(),
        owner.capture.page.setup_body.tooltip_text(),
        peer.observed.lock().unwrap().len()
    );
    assert_eq!(reply.text, "Synthetic phone microphone.");
    assert!(reply.copied);
    println!("phone stage: phone Stop completed");
    let history = services.history.recent(10).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(
        history[0].application_identifier.as_deref(),
        Some(format!("mluva-web:{id}").as_str())
    );
    let id2 = "a854b08a-bc40-492f-a453-58369ecef8d6".to_string();
    call(
        &target,
        PhoneRequest::Start {
            identifier: id2.clone(),
        },
    );
    until(|| owner.capture.phase() == Some(CapturePhase::Recording));
    call(
        &target,
        PhoneRequest::Audio {
            identifier: id2.clone(),
            sequence: 0,
            pcm: pcm.clone(),
        },
    );
    owner.capture.stop(); // the widget uses the same Stop owner
    until(|| owner.capture.phase().is_none());
    assert_eq!(
        call(&target, PhoneRequest::Status { identifier: id2 }).text,
        "Stopped on PC."
    );
    println!("phone stage: PC Stop completed");
    let id3 = "0c032e34-3396-4f3c-b2ae-1a060985bb39".to_string();
    call(
        &target,
        PhoneRequest::Start {
            identifier: id3.clone(),
        },
    );
    until(|| owner.capture.phase() == Some(CapturePhase::Recording));
    call(
        &target,
        PhoneRequest::Audio {
            identifier: id3.clone(),
            sequence: 0,
            pcm,
        },
    );
    // No heartbeat/audio after a lost phone connection: cancel without delivering partial text.
    until(|| owner.capture.phase().is_none());
    assert_eq!(
        call(&target, PhoneRequest::Status { identifier: id3 }).phase,
        "cancelled"
    );
    assert_eq!(services.history.recent(10).unwrap().len(), 2);
    assert!(!sentinel.exists());
    owner.settings.capture.incognito.set_active(true);
    assert!(services.config().incognito_mode);
    until(|| owner.capture.page.record_button.get_sensitive());
    let private_id = "5847e509-f653-4ac2-be9b-57a20b8a9118".to_string();
    call(
        &target,
        PhoneRequest::Start {
            identifier: private_id.clone(),
        },
    );
    until(|| owner.capture.phase() == Some(CapturePhase::Recording));
    assert!(
        call(
            &target,
            PhoneRequest::Status {
                identifier: private_id.clone()
            }
        )
        .incognito
    );
    call(
        &target,
        PhoneRequest::Audio {
            identifier: private_id.clone(),
            sequence: 0,
            pcm: STANDARD.encode(16384_i16.to_le_bytes().repeat(8000)),
        },
    );
    call(
        &target,
        PhoneRequest::Stop {
            identifier: private_id.clone(),
            sequence: 1,
        },
    );
    until(|| owner.capture.phase().is_none());
    let private_reply = call(
        &target,
        PhoneRequest::Status {
            identifier: private_id,
        },
    );
    assert_eq!(private_reply.text, "Synthetic private phone.");
    assert!(private_reply.incognito);
    assert_eq!(
        services.history.recent(10).unwrap().len(),
        2,
        "Incognito cannot save phone History"
    );
    println!("phone stage: disconnect cancelled");
    let socket = mluva_core::phone::socket_path().unwrap();
    let done = Rc::new(std::cell::Cell::new(false));
    let finished = done.clone();
    let closing = owner.shutdown();
    glib::MainContext::default().spawn_local(async move {
        closing.await.unwrap();
        finished.set(true);
    });
    until(|| done.get());
    assert!(!socket.exists());
    let requests = peer.finish();
    assert_eq!(requests.len(), 3);
    println!(
        "PASS: independent phone IPC, native widget recording/level, ordered and deduplicated PCM, phone/PC Stop, History/copy, microphone exclusion, disconnect cancellation and shutdown."
    );
}
