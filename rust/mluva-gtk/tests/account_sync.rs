//! Real GTK application with separate browser receivers, synthetic local capture and loopback OAuth/API peers.
use adw::prelude::*;
use mluva_core::{
    config::{AppConfig, AppPaths},
    history::HistoryInput,
};
use mluva_gtk::{
    application::{ApplicationDesktop, ApplicationPlatform},
    application_shell::ApplicationAction,
    async_runtime::DesktopRuntime,
    document_layout::DocumentResources,
    theme::ThemeController,
};
use mluva_providers::local_assets::MODEL_CATALOG;
use mluva_workflows::{
    capture::CapturePhase,
    services::{ApplicationServices, NativeBinaries},
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    rc::Rc,
    sync::mpsc,
    time::{Duration, Instant},
};
#[track_caller]
fn until(mut check: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(40);
    while !check() {
        assert!(Instant::now() < end, "account UI did not settle");
        while glib::MainContext::default().pending() {
            glib::MainContext::default().iteration(false);
        }
        std::thread::sleep(Duration::from_millis(4));
    }
}
fn widgets(w: &impl IsA<gtk::Widget>) -> Vec<gtk::Widget> {
    let mut result = vec![w.as_ref().clone()];
    let mut c = w.as_ref().first_child();
    while let Some(child) = c {
        result.extend(widgets(&child));
        c = child.next_sibling();
    }
    result
}
struct Peer {
    child: std::process::Child,
    replies: mpsc::Receiver<Value>,
}
impl Peer {
    fn new(root: &Path) -> Self {
        let mut child = Command::new("node")
            .arg(root.join("hosted/tests/native-peer.mjs"))
            .env("TMPDIR", "/tmp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let output = child.stdout.take().unwrap();
        let (tx, replies) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let Ok(line) = line else {
                    break;
                };
                if let Ok(value) = serde_json::from_str(&line)
                    && tx.send(value).is_err()
                {
                    break;
                }
            }
        });
        Self { child, replies }
    }
    fn next(&self) -> Value {
        let mut result = None;
        until(|| {
            if let Ok(value) = self.replies.try_recv() {
                result = Some(value);
            }
            result.is_some()
        });
        let result = result.unwrap();
        assert!(result.get("error").is_none(), "{result}");
        result
    }
    fn call(&mut self, value: Value) -> Value {
        writeln!(self.child.stdin.as_mut().unwrap(), "{value}").unwrap();
        self.next()
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn script(path: &Path, text: &str) {
    fs::write(path, text).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}
fn screenshot(window: &adw::ApplicationWindow, path: &Path) {
    use gtk::gdk::prelude::*;
    let clock = window.frame_clock().unwrap();
    let painted = Rc::new(Cell::new(false));
    let done = painted.clone();
    let signal = clock.connect_after_paint(move |_| done.set(true));
    window.queue_draw();
    clock.request_phase(gtk::gdk::FrameClockPhase::AFTER_PAINT);
    until(|| painted.get());
    clock.disconnect(signal);
    let paintable = gtk::WidgetPaintable::new(Some(window));
    let mut node = None;
    until(|| {
        let snapshot = gtk::Snapshot::new();
        paintable.snapshot(
            &snapshot,
            f64::from(window.width()),
            f64::from(window.height()),
        );
        node = snapshot.to_node();
        node.is_some()
    });
    let node = node.unwrap();
    let renderer = gtk::gsk::Renderer::for_surface(&window.surface().unwrap()).unwrap();
    let texture = renderer.render_texture(&node, None);
    texture.save_to_png(path).unwrap();
    renderer.unrealize();
}
#[test]
#[ignore = "requires private display, network, devices and independent browser peers"]
fn native_account_and_three_device_continuity() {
    let root = PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").expect("isolated runner"));
    assert!(!Path::new("/dev/snd").exists());
    assert!(std::env::var("WAYLAND_DISPLAY").is_err());
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let evidence = repo.join("tmp/account-evidence");
    fs::create_dir_all(&evidence).unwrap();
    let tools = root.join("application-tools");
    fs::create_dir_all(&tools).unwrap();
    script(
        &tools.join("pw-record"),
        "#!/bin/sh\ntrap 'exit 0' INT TERM\nwhile :; do /usr/bin/head -c 3200 /dev/zero; /usr/bin/sleep 0.05; done\n",
    );
    script(&tools.join("pw-dump"), "#!/bin/sh\nprintf '[]'\n");
    script(&tools.join("wpctl"), "#!/bin/sh\nexit 0\n");
    script(
        &tools.join("asr-worker"),
        "#!/bin/sh\nwhile IFS= read -r line; do printf '%s\\n' '{\"text\":\"Native words stay on this PC.\",\"language_code\":\"en\"}'; done\n",
    );
    // No real keyring; this peer proves stdin-only credential writes in the private session.
    script(
        &tools.join("secret-tool"),
        "#!/bin/sh\ncase \"$1\" in store) /usr/bin/cat >/dev/null;; lookup) exit 1;; clear) exit 0;; esac\n",
    );
    let browser = tools.join("account-browser");
    script(
        &browser,
        "#!/bin/sh\nprintf '%s' \"$1\" >\"$OFFSCREEN_SESSION_ROOT/authorization-url\"\n",
    );
    gio::AppInfo::create_from_commandline(
        browser.to_str().unwrap(),
        Some("Isolated account browser"),
        gio::AppInfoCreateFlags::SUPPORTS_URIS,
    )
    .unwrap()
    .set_as_default_for_type("x-scheme-handler/http")
    .unwrap();
    gtk::init().unwrap();
    adw::init().unwrap();
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceLight);
    let _theme = ThemeController::apply(
        root.join("theme"),
        &repo.join("rust/mluva-gtk/resources/fonts"),
    )
    .unwrap();
    let paths = AppPaths {
        config: root.join("native/config"),
        data: root.join("native/data"),
        runtime: root.join("native/runtime"),
    };
    let config = AppConfig {
        transcription_provider: "local".into(),
        local_model: "whisper-tiny".into(),
        welcome_completed: true,
        ..AppConfig::default()
    };
    config.save(&paths.config.join("config.json")).unwrap();
    let model = MODEL_CATALOG
        .iter()
        .find(|m| m.id == "whisper-tiny")
        .unwrap();
    let model_path = model.path(&paths.data);
    for file in &model.files {
        let path = model_path.join(&file.name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::File::create(path).unwrap().set_len(file.size).unwrap();
    }
    fs::write(model_path.join(".ready"), &model.revision).unwrap();
    let services = ApplicationServices::open(paths).unwrap();
    let old = services
        .history
        .add(HistoryInput::dictation(
            "Private old history",
            "Private old history",
        ))
        .unwrap();
    let app = adw::Application::builder()
        .application_id("com.mluva.AccountAcceptance")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(gio::Cancellable::NONE).unwrap();
    let runtime = DesktopRuntime::new().unwrap();
    let target = PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap()).join("debug");
    let native = ApplicationDesktop::new(
        &app,
        services.clone(),
        runtime.clone(),
        DocumentResources::from_directory(&repo.join("rust/mluva-gtk/resources")),
        NativeBinaries {
            asr_worker: tools.join("asr-worker"),
            audio_cleanup: target.join("mluva-audio-cleanup"),
            screenshot_editor: target.join("mluva-screenshot-editor"),
        },
        ApplicationPlatform {
            compact_recording: Rc::new(|_| {}),
            close: Rc::new(|| {}),
        },
    )
    .unwrap();
    native.shell.window.set_default_size(1100, 800);
    native.shell.present();
    until(|| native.capture.page.record_button.is_sensitive());
    assert!(
        gtk::IconTheme::for_display(&gtk::gdk::Display::default().unwrap())
            .has_icon(&native.shell.sync_button.icon_name().unwrap())
    );
    for width in [1100, 736, 420] {
        native.shell.navigate("capture");
        native.shell.window.set_default_size(width, 800);
        // GTK excludes the window's client-side shadow from the widget allocation.
        until(|| {
            (width - 24..=width).contains(&native.shell.window.width())
                && native.shell.sync_button.is_mapped()
        });
        assert!(native.shell.sync_button.grab_focus());
        screenshot(
            &native.shell.window,
            &evidence.join(format!("native-sync-{width}.png")),
        );
        native.shell.sync_button.emit_clicked();
        assert_eq!(
            native.shell.stack.visible_child_name().as_deref(),
            Some("settings")
        );
        assert_eq!(
            native.settings.view.visible_page_name().as_deref(),
            Some("account")
        );
        until(|| native.account.sign_in.is_mapped());
        assert!(!native.account.sync_new.is_active());
    }
    screenshot(
        &native.shell.window,
        &evidence.join("native-sync-account-minimum.png"),
    );
    native.shell.window.set_default_size(1100, 800);
    until(|| (1076..=1100).contains(&native.shell.window.width()));
    screenshot(&native.shell.window, &evidence.join("native-login.png"));
    assert!(
        native
            .shell
            .sync_button
            .tooltip_text()
            .unwrap()
            .contains("Signed out")
    );
    native.account.privacy(true);
    assert!(
        native
            .shell
            .sync_button
            .tooltip_text()
            .unwrap()
            .contains("Incognito")
    );
    assert!(!native.account.sign_in.is_sensitive());
    native.account.privacy(false);
    assert!(
        native
            .shell
            .sync_button
            .tooltip_text()
            .unwrap()
            .contains("Signed out")
    );
    native.account_callback("mluva://account?state=stale&code=stale");
    assert!(
        native
            .account
            .status
            .label()
            .contains("Start sign-in from Mluva")
    );
    // Native capture completes with no account, no provider socket and no network dependency.
    native.dispatch(ApplicationAction::Record);
    until(|| native.capture.phase() == Some(CapturePhase::Recording));
    native.dispatch(ApplicationAction::Record);
    until(|| native.capture.phase().is_none());
    assert_eq!(
        services.history.recent(1).unwrap()[0].raw_text,
        "Native words stay on this PC."
    );
    let before = services.config();
    let mut peer = Peer::new(&repo);
    let origin = peer.next()["origin"].as_str().unwrap().to_owned();
    assert_eq!(origin, std::env::var("MLUVA_ACCOUNT_ORIGIN").unwrap());
    native.shell.sync_button.emit_clicked();
    native.account.name.set_text("Native Lenovo");
    native.account.sign_in.emit_clicked();
    let authorize_path = root.join("authorization-url");
    until(|| authorize_path.exists());
    let url = fs::read_to_string(&authorize_path).unwrap();
    assert!(
        native
            .account
            .status
            .label()
            .contains("Finish signing in in your browser")
    );
    let callback = peer.call(json!({"action":"authorize","url":url}))["callback"]
        .as_str()
        .unwrap()
        .to_owned();
    native.shell.navigate("capture");
    native.account_callback(&callback);
    assert_eq!(
        native.settings.view.visible_page_name().as_deref(),
        Some("account")
    );
    assert_eq!(
        native.shell.stack.visible_child_name().as_deref(),
        Some("settings")
    );
    until(|| native.account.sign_out.is_visible());
    assert!(
        native
            .shell
            .sync_button
            .tooltip_text()
            .unwrap()
            .contains("Connected")
    );
    assert_eq!(before, services.config());
    native.account.sync_new.set_active(true);
    until(|| {
        widgets(&native.account.page)
            .iter()
            .filter(|w| w.is::<gtk::CheckButton>())
            .count()
            >= 2
    });
    for w in widgets(&native.account.page) {
        if let Ok(choice) = w.downcast::<gtk::CheckButton>() {
            choice.set_active(true);
        }
    }
    screenshot(&native.shell.window, &evidence.join("native-settings.png"));
    peer.call(json!({"action":"send","text":"Browser words arrive on native."}));
    until(|| {
        native
            .account
            .incoming_text
            .label()
            .contains("Browser words arrive on native.")
    });
    until(|| {
        services
            .history
            .recent(20)
            .unwrap()
            .iter()
            .any(|e| e.raw_text == "Browser words arrive on native.")
    });
    assert!(
        !peer.call(json!({"action":"documents"}))["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["id"] == old.identifier)
    );
    native.dispatch(ApplicationAction::Latest);
    screenshot(&native.shell.window, &evidence.join("native-received.png"));
    native.dispatch(ApplicationAction::Record);
    until(|| native.capture.phase() == Some(CapturePhase::Recording));
    screenshot(&native.shell.window, &evidence.join("native-recording.png"));
    native.dispatch(ApplicationAction::Record);
    until(|| native.capture.phase().is_none());
    peer.call(json!({"action":"live","text":"Native words stay on this PC."}));
    let mut records = Value::Null;
    until(|| {
        records = peer.call(json!({"action":"documents"}));
        records["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["rawText"] == "Native words stay on this PC.")
    });
    let id = records["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["rawText"] == "Native words stay on this PC.")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    services
        .conversations
        .save_text(&id, "Native edit after recording", None)
        .unwrap();
    native.account.sync();
    until(|| {
        peer.call(json!({"action":"documents"}))["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["id"] == id && e["text"] == "Native edit after recording")
    });
    peer.call(json!({"action":"edit","id":id,"text":"Web edit returned to native","rewrites":[{"id":"2026-10-12T20:00:00Z","createdAt":"2026-10-12T20:00:00Z","instruction":"Polish","text":"A completed shared rewrite","model":"synthetic"}]}));
    native.account.sync();
    until(|| {
        services
            .conversations
            .source_text(&services.history.find(&id).unwrap(), false)
            .unwrap()
            == "Web edit returned to native"
    });
    assert_eq!(
        services.conversations.replies(&id).unwrap()[0].text,
        "A completed shared rewrite"
    );
    native.dispatch(ApplicationAction::Latest);
    screenshot(&native.shell.window, &evidence.join("native-editing.png"));
    native.dispatch(ApplicationAction::History);
    screenshot(&native.shell.window, &evidence.join("native-history.png"));
    native.dispatch(ApplicationAction::Latest);
    let web_source = services
        .history
        .recent(30)
        .unwrap()
        .into_iter()
        .find(|e| e.raw_text == "Browser words arrive on native.")
        .unwrap();
    services
        .conversations
        .merge(&id, &web_source.identifier)
        .unwrap();
    native.account.sync();
    until(|| {
        peer.call(json!({"action":"documents"}))["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| {
                e["id"] == id
                    && e["segments"]
                        .as_array()
                        .is_some_and(|s| s.iter().any(|v| v["id"] == web_source.identifier))
            })
    });
    peer.call(json!({"action":"offline","enabled":true}));
    services
        .conversations
        .save_text(&id, "Offline native edit", None)
        .unwrap();
    native.account.sync();
    until(|| {
        native.account.status.label().contains("failed")
            || native.account.status.label().contains("Offline")
    });
    native.settings.view.set_visible_page(&native.account.page);
    native.dispatch(ApplicationAction::Settings);
    screenshot(&native.shell.window, &evidence.join("native-recovery.png"));
    peer.call(json!({"action":"offline","enabled":false}));
    native.account.sync();
    until(|| {
        peer.call(json!({"action":"documents"}))["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["id"] == id && e["text"] == "Offline native edit")
    });
    native.dispatch(ApplicationAction::Latest);
    let local_count = services.history.recent(100).unwrap().len();
    let account_count = peer.call(json!({"action":"documents"}))["entries"]
        .as_array()
        .unwrap()
        .len();
    native.settings.capture.incognito.set_active(true);
    assert!(services.config().incognito_mode);
    native.dispatch(ApplicationAction::Record);
    until(|| native.capture.phase() == Some(CapturePhase::Recording));
    native.dispatch(ApplicationAction::Record);
    until(|| native.capture.phase().is_none());
    assert_eq!(services.history.recent(100).unwrap().len(), local_count);
    assert_eq!(
        peer.call(json!({"action":"documents"}))["entries"]
            .as_array()
            .unwrap()
            .len(),
        account_count
    );
    native.settings.capture.incognito.set_active(false);
    until(|| native.account.status.label().contains("Connected"));
    let audit = peer.call(json!({"action":"audit"}));
    for request in audit["requests"].as_array().unwrap() {
        assert_ne!(request["path"], "/api/speech-token");
        if request["path"].as_str().unwrap().contains("history") {
            let body = request["body"].to_string();
            for forbidden in [
                "pcm",
                "retained_audio",
                "audio_path",
                "base64",
                "model_directory",
            ] {
                assert!(!body.contains(forbidden));
            }
        }
    }
    peer.call(json!({"action":"delete","id":id}));
    until(|| services.history.find(&id).is_err());
    peer.call(json!({"action":"revoke"}));
    until(|| native.account.sign_in.is_visible());
    assert!(services.history.find(&old.identifier).is_ok());
    fs::write(evidence.join("checks.json"),serde_json::to_vec_pretty(&json!({"environment":"private Xvfb, network and devices; real GTK and independent Chromium processes","checks":["native signed-out local capture with synthetic PCM and local worker","PKCE sign-in and named device","login preserves provider configuration","web to native and native to web plus third receiver","no old history upload","immutable originals, edits and completed rewrites","offline edit retry","signed-in Incognito capture keeps history and account unchanged","native audio absent from sync HTTP","delete propagates","revocation signs out and preserves original local history"],"limits":["no physical microphone or Wayland permission proof","synthetic Cognito and providers; no production changes"]})).unwrap()).unwrap();
    peer.call(json!({"action":"close"}));
    let finished = Rc::new(RefCell::new(false));
    let ready = finished.clone();
    let shutdown = native.shutdown();
    glib::MainContext::default().spawn_local(async move {
        shutdown.await.unwrap();
        *ready.borrow_mut() = true;
    });
    until(|| *finished.borrow());
}
