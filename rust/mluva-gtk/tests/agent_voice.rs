//! Full desktop graph with private PCM, HTTP and Herdr peers. No host input or audio.
use adw::prelude::*;
use mluva_core::{
    agent_voice::{DraftReason, RoutePhase, VoiceSettings},
    config::{AppConfig, AppPaths},
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
    fs,
    io::{BufRead, BufReader},
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    rc::Rc,
    thread,
    time::{Duration, Instant},
};
#[path = "../../mluva-workflows/tests/support/http.rs"]
mod http;
fn drain() {
    while glib::MainContext::default().pending() {
        glib::MainContext::default().iteration(false);
    }
}
#[track_caller]
fn until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(12);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "private application did not settle"
        );
        drain();
        thread::sleep(Duration::from_millis(5));
    }
    drain();
}
fn settle(ms: u64) {
    let start = Instant::now();
    while start.elapsed().as_millis() < ms as u128 {
        drain();
        thread::sleep(Duration::from_millis(5));
    }
}
struct Peer(Child);
impl Drop for Peer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn wire(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}
fn sent(path: &Path) -> Vec<Value> {
    wire(path)
        .into_iter()
        .filter(|r| r["method"] == "agent.prompt_guarded")
        .collect()
}
fn state(path: &Path, value: &Value) {
    let tmp = path.with_extension("next");
    fs::write(&tmp, serde_json::to_vec(value).unwrap()).unwrap();
    fs::rename(tmp, path).unwrap();
}
fn screenshot(owner: &ApplicationDesktop, root: &Path, name: &str, width: i32) {
    owner.shell.window.set_default_size(width, 760);
    settle(100);
    let output = Command::new("xdotool")
        .args(["search", "--onlyvisible", "--name", "^Mluva$"])
        .output()
        .unwrap();
    let windows = String::from_utf8(output.stdout).unwrap();
    let id = windows.lines().next().unwrap();
    Command::new("xdotool")
        .args(["windowsize", id, &width.to_string(), "760"])
        .status()
        .unwrap();
    settle(130);
    assert!(
        Command::new("import")
            .args(["-window", id])
            .arg(root.join(name))
            .status()
            .unwrap()
            .success()
    );
    assert!(owner.agent_voice.view.widget.width() <= owner.shell.window.width());
}
/// The wheel grows with its sessions but always fits the private screen.
fn wheel_fits_screen(wheel: &mluva_gtk::agent_session_wheel::SessionWheel) -> bool {
    let screen = gtk::gdk::Display::default()
        .unwrap()
        .monitors()
        .item(0)
        .and_downcast::<gtk::gdk::Monitor>()
        .unwrap()
        .geometry();
    wheel.window.width() <= screen.width() && wheel.window.height() <= screen.height()
}
/// A synthetic process outside this test's process tree, registered as an interactive
/// Claude Code session in the private home's registry, with a generated transcript title.
struct NativeSession(u32, PathBuf);
impl Drop for NativeSession {
    fn drop(&mut self) {
        // SAFETY: signals only the orphaned synthetic sleep this fixture started.
        unsafe { libc::kill(self.0 as i32, libc::SIGKILL) };
        let _ = fs::remove_file(&self.1);
    }
}
fn native_session(root: &Path, id: &str, title: &str) -> NativeSession {
    use std::os::unix::fs::PermissionsExt;
    // The shell exits at once, so the sleep is no descendant of this process (Mluva's own).
    let output = Command::new("sh")
        .args(["-c", "sleep 600 >/dev/null 2>&1 & echo $!"])
        .output()
        .unwrap();
    let pid: u32 = String::from_utf8(output.stdout)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    let start = stat[stat.rfind(')').unwrap() + 1..]
        .split_whitespace()
        .nth(19)
        .unwrap()
        .to_owned();
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    assert!(home.starts_with(root));
    let config = home.join(".claude");
    let project = config.join("projects/-synthetic-native");
    for directory in [config.join("sessions"), project.clone()] {
        fs::create_dir_all(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
    }
    fs::write(
        project.join(format!("{id}.jsonl")),
        format!(
            "{}\n",
            json!({"type": "ai-title", "aiTitle": title, "sessionId": id})
        ),
    )
    .unwrap();
    let entry = config.join(format!("sessions/{pid}.json"));
    fs::write(
        &entry,
        json!({"pid": pid, "sessionId": id, "cwd": "/synthetic/native", "startedAt": 1,
            "kind": "interactive", "name": "native-3a", "nameSource": "derived", "status": "idle",
            "procStart": start})
        .to_string(),
    )
    .unwrap();
    NativeSession(pid, entry)
}
/// A synthetic Claude Code session outside Herdr and outside this test's process tree, with its
/// own inbox, registered in the private home's registry by the inbox fixture peer.
struct InboxSession(u32, PathBuf);
impl Drop for InboxSession {
    fn drop(&mut self) {
        // SAFETY: signals only the orphaned fixture peer this helper started.
        unsafe { libc::kill(self.0 as i32, libc::SIGKILL) };
        let _ = fs::remove_file(&self.1);
    }
}
fn inbox_session(id: &str, title: &str, wire: &Path) -> InboxSession {
    let config = PathBuf::from(std::env::var_os("HOME").unwrap()).join(".claude");
    let sockets = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").unwrap()).join("cc-socks");
    // The shell exits at once, so the peer is no descendant of this process (Mluva's own).
    let output = Command::new("sh")
        .args(["-c", r#""$0" "$@" >/dev/null 2>&1 & echo $!"#])
        .arg(env!("CARGO_BIN_EXE_claude-inbox-fixture-peer"))
        .arg(&config)
        .arg(&sockets)
        .args([id, "/synthetic/inbox", title])
        .arg(wire)
        .output()
        .unwrap();
    let pid: u32 = String::from_utf8(output.stdout)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let entry = config.join(format!("sessions/{pid}.json"));
    until(|| entry.exists());
    InboxSession(pid, entry)
}
/// The dropdown position of the first session whose row contains `text`.
fn row_containing(owner: &ApplicationDesktop, text: &str) -> Option<u32> {
    let model = owner.agent_voice.view.recipients.model()?;
    (0..model.n_items()).find(|i| {
        model
            .item(*i)
            .and_downcast::<gtk::StringObject>()
            .is_some_and(|row| row.string().contains(text))
    })
}
#[test]
#[ignore = "requires private GTK, network, devices and fixture peers"]
fn toggle_route_cancel_edit_replace_and_uncertain_delivery_in_the_real_application() {
    let root = PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").unwrap());
    assert!(PathBuf::from(std::env::var_os("HOME").unwrap()).starts_with(&root));
    assert_ne!(
        fs::read_link("/proc/self/ns/net")
            .unwrap()
            .to_str()
            .unwrap(),
        std::env::var("MLUVA_HOST_NET_NS").unwrap()
    );
    for path in ["/dev/snd", "/dev/input", "/dev/uinput"] {
        assert!(!Path::new(path).exists());
    }
    let tools = root.join("application-tools");
    fs::create_dir(&tools).unwrap();
    let target = PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap()).join("debug");
    symlink(target.join("audio-fixture-peer"), tools.join("pw-record")).unwrap();
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-application.json")).unwrap();
    fs::write(
        tools.join("test-config.json"),
        serde_json::to_vec(&fixture["cases"][0]["pcm"]).unwrap(),
    )
    .unwrap();
    let text = "Synthetic message: verify the selected agent only.";
    // The twelfth capture recognises nothing: an agent route must keep it, not drop it.
    let responses: Vec<_> = (0..14).map(|n| json!({"route":"speech", "status":200, "delay_ms":160, "payload":{"text":if n == 11 { "" } else { text }, "language_code":"eng"}})).collect();
    let mut speech = http::Peer::new(&responses);
    let paths = AppPaths {
        config: root.join("config/mluva"),
        data: root.join("data/mluva"),
        runtime: root.join("runtime/mluva"),
    };
    let config = AppConfig {
        transcription_provider: "litellm".into(),
        transcription_base_url: format!("{}/v1", speech.address),
        transcription_api_key_env: "AGENT_VOICE_SYNTHETIC_KEY".into(),
        transcription_remote_model: Some("whisper".into()),
        welcome_completed: true,
        ..Default::default()
    };
    config.save(&paths.config.join("config.json")).unwrap();
    let socket = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").unwrap()).join("voice.sock");
    let peer_state = root.join("herdr-state.json");
    let audit = root.join("herdr-wire.jsonl");
    let mut data = json!({"agents": [
        {"guard": "a".repeat(64), "agent": {"name":"Chief of staff", "pane_id":"w1:p1", "agent":"codex", "agent_status":"working", "cwd":"/synthetic/product"}},
        {"guard": "b".repeat(64), "agent": {"name":"Chief of staff", "pane_id":"w1:p2", "agent":"claude", "agent_status":"idle", "cwd":"/synthetic/research", "terminal_title_stripped":"Claude Code",
            "agent_session": {"source": "herdr:claude", "agent": "claude", "kind": "id", "value": "research-session"}}}
    ]});
    // Claude Code's own registry and transcript name the Herdr pane by its session ID.
    let research = native_session(&root, "research-session", "Research notes");
    state(&peer_state, &data);
    let mut peer = Peer(
        Command::new(env!("CARGO_BIN_EXE_herdr-voice-fixture-peer"))
            .arg(&socket)
            .arg(&peer_state)
            .arg(&audit)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut ready = String::new();
    BufReader::new(peer.0.stdout.take().unwrap())
        .read_line(&mut ready)
        .unwrap();
    assert_eq!(ready.trim(), "herdr-voice-peer-ready");
    VoiceSettings {
        enabled: true,
        socket: Some(socket.to_string_lossy().into()),
        ..Default::default()
    }
    .save(&paths.config.join("agent-voice.json"), "F9")
    .unwrap();
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
        ThemeController::apply(root.join("theme"), resources.font.parent().unwrap()).unwrap();
    let app = adw::Application::builder()
        .application_id("com.mluva.AgentVoiceVerification")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gio::Cancellable>).unwrap();
    let runtime = DesktopRuntime::new().unwrap();
    let services = ApplicationServices::open(paths.clone()).unwrap();
    let owner = ApplicationDesktop::new(
        &app,
        services.clone(),
        runtime,
        resources,
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
    owner.shell.present();
    until(|| {
        owner.capture.page.record_button.is_sensitive()
            && owner
                .agent_voice
                .view
                .recipients
                .model()
                .is_some_and(|m| m.n_items() == 3)
    });
    owner.agent_voice.view.recipients.set_selected(1);
    settle(50);
    gtk::gdk::Display::default()
        .unwrap()
        .clipboard()
        .set_text("Clipboard must remain unchanged");
    screenshot(&owner, &root, "agent-ready-laptop.png", 1100);
    let window_ids = Command::new("xdotool")
        .args(["search", "--onlyvisible", "--name", "^Mluva$"])
        .output()
        .unwrap();
    let window_ids = String::from_utf8(window_ids.stdout).unwrap();
    assert!(
        Command::new("xdotool")
            .args([
                "windowactivate",
                "--sync",
                window_ids.lines().next().unwrap()
            ])
            .status()
            .unwrap()
            .success()
    );
    settle(100);
    owner.settings.show();
    assert!(owner.settings.view.set_visible_page_name("capture"));
    screenshot(&owner, &root, "agent-settings-laptop.png", 1100);
    owner.shell.navigate("capture");
    // Saved opt-in controls both the native panel and the private persisted settings.
    owner.agent_voice.view.enabled.set_active(false);
    assert!(!owner.agent_voice.view.widget.is_visible());
    assert!(
        !VoiceSettings::load(&paths.config.join("agent-voice.json"))
            .unwrap()
            .enabled
    );
    owner.agent_voice.view.enabled.set_active(true);
    assert!(owner.agent_voice.view.widget.is_visible());
    assert!(
        VoiceSettings::load(&paths.config.join("agent-voice.json"))
            .unwrap()
            .enabled
    );
    // Settings save themselves; shortcuts share the normal Capture page.
    owner.agent_voice.view.hold_key.set_selected(11);
    assert_eq!(
        VoiceSettings::load(&paths.config.join("agent-voice.json"))
            .unwrap()
            .hold_key,
        "F12"
    );
    owner.settings.capture.recording_key.set_selected(11);
    assert_eq!(
        services.config().global_recording_key,
        "F9",
        "Duplicate shortcuts must be rejected"
    );
    owner.settings.workspace.appearance.opacity.set_value(71.0);
    until(|| services.config().widget_opacity == 71);
    assert_eq!(
        AppConfig::load(&paths.config.join("config.json"))
            .unwrap()
            .widget_opacity,
        71
    );
    owner.settings.capture.language.set_selected(2);
    assert_eq!(services.config().language_code, "ces");
    owner.settings.providers.speech.chunk_row.set_value(15.0);
    until(|| services.config().transcription_chunk_seconds == 15);
    assert_eq!(
        services.config().language_code,
        "ces",
        "Autosaving a provider field must preserve changes made elsewhere"
    );
    assert_eq!(
        AppConfig::load(&paths.config.join("config.json"))
            .unwrap()
            .transcription_chunk_seconds,
        15
    );
    owner
        .settings
        .providers
        .speech
        .endpoint_entry
        .set_text("invalid URL");
    settle(500);
    assert_eq!(
        services.config().transcription_base_url,
        speech.address.to_string() + "/v1"
    );
    owner
        .settings
        .providers
        .speech
        .endpoint_entry
        .set_text(&services.config().transcription_base_url);
    until(|| owner.capture.page.record_button.is_sensitive());
    // F12 is a selector first. Real private X11 keys choose, cancel and confirm.
    let wheel_key = |key: &str| {
        let output = Command::new("xdotool")
            .args(["search", "--onlyvisible", "--name", "^Choose a session$"])
            .output()
            .unwrap();
        let id = String::from_utf8(output.stdout).unwrap();
        let id = id.lines().next().unwrap();
        assert!(
            Command::new("xdotool")
                .args(["windowactivate", "--sync", id])
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new("xdotool")
                .args(["key", "--clearmodifiers", key])
                .status()
                .unwrap()
                .success()
        );
        settle(100);
    };
    owner.agent_voice.shortcut();
    until(|| {
        owner.agent_voice.wheel.window.is_visible()
            && owner.agent_voice.wheel.entries.borrow().len() == 2
    });
    assert!(owner.capture.phase().is_none());
    assert!(sent(&audit).is_empty());
    // The wheel names a session by its agent's own title, here from Claude Code's transcript
    // matched by session ID; Herdr's label remains for routing.
    let second = owner.agent_voice.wheel.entries.borrow()[1].clone();
    assert_eq!(
        (second.display_name(), second.name.as_str()),
        ("Research notes", "Chief of staff")
    );
    owner.agent_voice.shortcut_binding(Some("F11"), Some("F7"));
    settle(100);
    assert_eq!(owner.agent_voice.wheel.record_key.label().as_str(), "F11");
    owner.agent_voice.shortcut_binding(None, None);
    settle(100);
    assert_eq!(
        owner
            .agent_voice
            .wheel
            .selected
            .borrow()
            .as_ref()
            .unwrap()
            .locator(),
        "w1:p1"
    );
    for (width, name) in [
        (420, "wheel-minimum.png"),
        (736, "wheel-narrow.png"),
        (1060, "wheel-wide.png"),
    ] {
        owner.shell.window.set_default_size(width, 760);
        settle(100);
        let output = Command::new("xdotool")
            .args(["search", "--onlyvisible", "--name", "^Choose a session$"])
            .output()
            .unwrap();
        let id = String::from_utf8(output.stdout).unwrap();
        assert!(
            Command::new("import")
                .args(["-window", id.lines().next().unwrap()])
                .arg(root.join(name))
                .status()
                .unwrap()
                .success()
        );
        assert!(wheel_fits_screen(&owner.agent_voice.wheel));
    }
    wheel_key("s");
    assert_eq!(
        owner.agent_voice.settings().selected.unwrap().locator(),
        "w1:p2"
    );
    wheel_key("Escape");
    assert!(!owner.agent_voice.wheel.window.is_visible());
    assert!(owner.capture.phase().is_none());
    owner.agent_voice.shortcut();
    assert_eq!(
        owner
            .agent_voice
            .wheel
            .selected
            .borrow()
            .as_ref()
            .unwrap()
            .locator(),
        "w1:p2"
    );
    // Second F12 starts, with key repeat unable to toggle a second time.
    wheel_key("F12");
    until(|| owner.capture.phase() == Some(CapturePhase::Recording));
    assert!(!owner.agent_voice.wheel.window.is_visible());
    owner.dispatch(mluva_gtk::application_shell::ApplicationAction::Cancel);
    until(|| owner.capture.phase().is_none());
    assert!(sent(&audit).is_empty());
    // F12 -> F9 is provisionally local/editable and never copies, pastes or sends.
    fs::remove_file(tools.join("raw.ready.json")).unwrap();
    owner.agent_voice.shortcut();
    wheel_key("F9");
    until(|| owner.capture.phase() == Some(CapturePhase::Recording));
    settle(60);
    owner.dispatch(mluva_gtk::application_shell::ApplicationAction::GlobalRecord);
    until(|| owner.capture.phase().is_none());
    let transcript = owner.capture.page.workspace.entry().unwrap_or_else(|| {
        panic!(
            "Local transcript missing: status={}, notice={}, error={:?}, provider_requests={}",
            owner.capture.page.status.label(),
            owner.capture.page.workspace.notice.label(),
            owner.capture.page.setup_body.tooltip_text(),
            speech.observed.lock().unwrap().len()
        )
    });
    assert_eq!(transcript.raw_text, text);
    assert_eq!(
        glib::MainContext::default()
            .block_on(
                gtk::gdk::Display::default()
                    .unwrap()
                    .clipboard()
                    .read_text_future()
            )
            .unwrap()
            .as_deref(),
        Some("Clipboard must remain unchanged")
    );
    settle(2200);
    assert!(sent(&audit).is_empty());
    owner.capture.page.workspace.documents()[0]
        .buffer()
        .set_text("Edited local transcript.");
    let local_entry = owner.capture.page.workspace.entry().unwrap();
    until(|| {
        !owner
            .capture
            .page
            .workspace
            .has_unsaved_edits(&local_entry.identifier)
    });
    assert_eq!(
        services
            .history
            .find(&local_entry.identifier)
            .unwrap()
            .raw_text,
        text
    );
    assert_eq!(
        services
            .conversations
            .source_text(&local_entry, false)
            .unwrap(),
        "Edited local transcript."
    );
    // A Claude Code session typed into a Herdr pane, and one outside Herdr found only in Claude
    // Code's own registry, are offered after Herdr's guarded panes. Neither can be sent to:
    // the second agent key records an editable draft, and the next one stops it.
    data["unmanaged"] = json!([{"name": "", "pane_id": "w1:p3", "agent": "claude",
        "agent_status": "idle", "terminal_title_stripped": "Typed in a pane", "agent_session":
        {"source": "herdr:claude", "agent": "claude", "kind": "id", "value": "typed-session"}}]);
    state(&peer_state, &data);
    let native = native_session(&root, "native-session", "Native draft notes");
    owner.agent_voice.refresh();
    until(|| owner.agent_voice.view.recipients.model().unwrap().n_items() == 5);
    owner.agent_voice.shortcut();
    until(|| owner.agent_voice.wheel.entries.borrow().len() == 4);
    let offered: Vec<_> = owner
        .agent_voice
        .wheel
        .entries
        .borrow()
        .iter()
        .map(|t| {
            (
                t.herdr
                    .as_ref()
                    .map_or_else(String::new, |pane| pane.pane_id.clone()),
                t.display_name().to_owned(),
                t.can_receive(),
            )
        })
        .collect();
    assert_eq!(
        offered,
        [
            ("w1:p1".into(), "Chief of staff".into(), true),
            ("w1:p2".into(), "Research notes".into(), true),
            ("w1:p3".into(), "Typed in a pane".into(), false),
            (String::new(), "Native draft notes".into(), false),
        ]
    );
    for (key, session) in [("d", "typed-session"), ("f", "native-session")] {
        wheel_key(key);
        assert_eq!(
            owner.agent_voice.settings().selected.unwrap().session_id,
            session
        );
        assert!(owner.agent_voice.wheel.record.is_sensitive());
    }
    let _ = fs::remove_file(tools.join("raw.ready.json"));
    wheel_key("F12");
    until(|| owner.capture.phase() == Some(CapturePhase::Recording));
    assert!(!owner.agent_voice.wheel.window.is_visible());
    assert!(owner.agent_voice.route.borrow().attempt.is_none());
    settle(60);
    owner.agent_voice.shortcut();
    until(|| owner.capture.phase().is_none());
    let draft = owner.capture.page.workspace.entry().unwrap();
    assert_eq!(draft.raw_text, text);
    assert_ne!(draft.identifier, local_entry.identifier);
    settle(2200);
    assert!(sent(&audit).is_empty());
    assert_eq!(
        glib::MainContext::default()
            .block_on(
                gtk::gdk::Display::default()
                    .unwrap()
                    .clipboard()
                    .read_text_future()
            )
            .unwrap()
            .as_deref(),
        Some("Clipboard must remain unchanged")
    );
    owner.agent_voice.view.recipients.set_selected(0);
    data.as_object_mut().unwrap().remove("unmanaged");
    state(&peer_state, &data);
    drop(native);
    owner.agent_voice.refresh();
    until(|| owner.agent_voice.view.recipients.model().unwrap().n_items() == 3);
    // Without Herdr's panes, the registry alone is the wheel: the research session stays,
    // named by its own title, but now draft-only.
    let herdr_agents = std::mem::replace(&mut data["agents"], json!([]));
    state(&peer_state, &data);
    owner.agent_voice.refresh();
    until(|| owner.agent_voice.view.recipients.model().unwrap().n_items() == 2);
    owner.agent_voice.shortcut();
    until(|| owner.agent_voice.wheel.entries.borrow().len() == 1);
    let alone = owner.agent_voice.wheel.entries.borrow()[0].clone();
    assert_eq!(
        (
            alone.display_name(),
            alone.draft_only(),
            alone.herdr.is_none()
        ),
        ("Research notes", true, true)
    );
    owner.dispatch(mluva_gtk::application_shell::ApplicationAction::Cancel);
    // The paging checks below count Herdr panes alone.
    drop(research);
    data["agents"] = herdr_agents;
    state(&peer_state, &data);
    owner.agent_voice.refresh();
    until(|| owner.agent_voice.view.recipients.model().unwrap().n_items() == 3);
    // More than one wheel page, non-alphabetical server order, reorder and loss.
    let original_agents = data["agents"].clone();
    // More sessions than one page of 26 letter keys, in non-alphabetical server order.
    for i in 3..=30 {
        data["agents"].as_array_mut().unwrap().push(json!({"guard":"d".repeat(64),"agent":{"name":if i == 3 { "long-session-name-for-regression".to_owned() } else { format!("Z session {}", 31-i) },"pane_id":format!("w1:p{i}"),"agent":"codex","agent_status":"idle"}}));
    }
    let all_agents = data["agents"].clone();
    for count in [0, 1, 2, 3, 6, 20] {
        owner.agent_voice.view.recipients.set_selected(0);
        data["agents"] = json!(all_agents.as_array().unwrap()[..count].to_vec());
        state(&peer_state, &data);
        owner.agent_voice.refresh();
        until(|| owner.agent_voice.view.recipients.model().unwrap().n_items() == count as u32 + 1);
        owner.agent_voice.shortcut();
        settle(100);
        assert!(owner.agent_voice.wheel.window.is_visible());
        assert!(wheel_fits_screen(&owner.agent_voice.wheel));
        assert!(owner.capture.phase().is_none());
        owner.dispatch(mluva_gtk::application_shell::ApplicationAction::Cancel);
    }
    data["agents"] = all_agents;
    state(&peer_state, &data);
    owner.agent_voice.refresh();
    until(|| owner.agent_voice.view.recipients.model().unwrap().n_items() == 31);
    owner.agent_voice.shortcut();
    assert_eq!(
        owner.agent_voice.wheel.entries.borrow()[0].locator(),
        "w1:p1"
    );
    settle(100);
    let wheel_window = Command::new("xdotool")
        .args(["search", "--onlyvisible", "--name", "^Choose a session$"])
        .output()
        .unwrap();
    let wheel_window = String::from_utf8(wheel_window.stdout).unwrap();
    assert!(
        Command::new("import")
            .args(["-window", wheel_window.lines().next().unwrap()])
            .arg(root.join("wheel-thirty-sessions.png"))
            .status()
            .unwrap()
            .success()
    );
    wheel_key("Right");
    until(|| owner.agent_voice.wheel.page.get() == 1);
    // A refresh that cannot read Herdr while the wheel is open is a failed read, not an empty
    // list: the wheel keeps its thirty sessions and its page.
    let lists = |audit: &Path| {
        fs::read_to_string(audit)
            .unwrap_or_default()
            .matches("agent.guarded_list")
            .count()
    };
    let before = lists(&audit);
    data["mode"] = json!("list-down");
    state(&peer_state, &data);
    owner.agent_voice.refresh();
    until(|| lists(&audit) > before);
    settle(300);
    assert_eq!(owner.agent_voice.wheel.page.get(), 1);
    assert_eq!(owner.agent_voice.wheel.entries.borrow().len(), 30);
    data["mode"] = Value::Null;
    state(&peer_state, &data);
    wheel_key("a");
    assert_eq!(
        owner.agent_voice.settings().selected.unwrap().locator(),
        "w1:p27"
    );
    data["agents"].as_array_mut().unwrap().reverse();
    state(&peer_state, &data);
    owner.agent_voice.refresh();
    until(|| owner.agent_voice.wheel.entries.borrow()[0].locator() == "w1:p30");
    assert_eq!(
        owner.agent_voice.settings().selected.unwrap().locator(),
        "w1:p27"
    );
    data["agents"]
        .as_array_mut()
        .unwrap()
        .retain(|a| a["agent"]["pane_id"] != "w1:p27");
    state(&peer_state, &data);
    owner.agent_voice.refresh();
    until(|| !owner.agent_voice.wheel.record.is_sensitive());
    owner.agent_voice.shortcut();
    assert!(
        owner.capture.phase().is_none(),
        "A missing target must not silently choose another session"
    );
    assert_eq!(
        owner.agent_voice.settings().selected.unwrap().locator(),
        "w1:p27"
    );
    data["agents"] = original_agents;
    state(&peer_state, &data);
    owner.agent_voice.refresh();
    until(|| owner.agent_voice.view.recipients.model().unwrap().n_items() == 4);
    owner.agent_voice.view.recipients.set_selected(1);
    let record = || {
        let _ = fs::remove_file(tools.join("raw.ready.json"));
        owner.agent_voice.view.record.emit_clicked();
        until(|| owner.capture.phase() == Some(CapturePhase::Recording));
        settle(60);
    };
    let finish = || {
        owner.agent_voice.view.record.emit_clicked();
        until(|| owner.capture.phase().is_none());
    };
    // Full capture uses the selected provider, no clipboard delivery, and one guarded send.
    // The live text is on the capture page whenever a route recording starts, wherever the
    // window was left, and without Mluva taking the keyboard from the agent.
    owner.shell.navigate("history");
    let was_active = owner.shell.window.is_active();
    record();
    assert_eq!(
        owner.shell.stack.visible_child_name().as_deref(),
        Some("capture")
    );
    assert_eq!(owner.shell.window.is_active(), was_active);
    // Normal main-thread work must not be confused with losing a held shortcut.
    thread::sleep(Duration::from_millis(1250));
    drain();
    assert_eq!(
        owner.capture.phase(),
        Some(CapturePhase::Recording),
        "A brief UI stall interrupted agent dictation"
    );
    screenshot(&owner, &root, "agent-recording-laptop.png", 1100);
    finish();
    assert!(matches!(
        owner
            .agent_voice
            .route
            .borrow()
            .attempt
            .as_ref()
            .unwrap()
            .phase,
        RoutePhase::Grace { .. }
    ));
    screenshot(&owner, &root, "agent-countdown-laptop.png", 1100);
    until(|| {
        owner
            .agent_voice
            .route
            .borrow()
            .attempt
            .as_ref()
            .unwrap()
            .phase
            == RoutePhase::Delivered
    });
    assert_eq!(sent(&audit).len(), 1);
    assert_eq!(sent(&audit)[0]["params"]["text"], text);
    assert_eq!(sent(&audit)[0]["params"]["target"], "w1:p1");
    let attempt = owner.agent_voice.route.borrow().attempt.clone().unwrap();
    assert!(attempt.submitted_ms.unwrap() - attempt.final_ms.unwrap() >= 2000);
    // The History row learns that the message went out.
    let outcome = |id: &Option<String>| services.history.find(id.as_ref().unwrap()).unwrap();
    assert_eq!(outcome(&attempt.history_id).delivery_outcome, "sent");
    assert_eq!(services.config().transcription_provider, "litellm");
    // Keep draft during recording; editing and saving do not alter immutable recognition.
    record();
    owner.agent_voice.view.keep.emit_clicked();
    until(|| owner.capture.phase().is_none());
    assert!(matches!(
        owner
            .agent_voice
            .route
            .borrow()
            .attempt
            .as_ref()
            .unwrap()
            .phase,
        RoutePhase::Draft(_)
    ));
    owner.capture.page.workspace.documents()[0]
        .buffer()
        .set_text("Edited synthetic request.");
    until(|| {
        !owner
            .capture
            .page
            .workspace
            .has_unsaved_edits(&owner.capture.page.workspace.entry().unwrap().identifier)
    });
    let history_id = owner
        .agent_voice
        .route
        .borrow()
        .attempt
        .as_ref()
        .unwrap()
        .history_id
        .clone()
        .unwrap();
    let entry = services.history.find(&history_id).unwrap();
    assert_eq!(entry.raw_text, text);
    assert_eq!(entry.delivery_outcome, "draft-kept");
    assert_eq!(
        services.conversations.source_text(&entry, false).unwrap(),
        "Edited synthetic request."
    );
    screenshot(&owner, &root, "agent-draft-narrow.png", 420);
    settle(2200);
    assert_eq!(sent(&audit).len(), 1);
    // Explicit sending re-arms this editor, so a subsequent edit cancels that new attempt too.
    owner.agent_voice.view.send_draft.emit_clicked();
    assert!(matches!(
        owner
            .agent_voice
            .route
            .borrow()
            .attempt
            .as_ref()
            .unwrap()
            .phase,
        RoutePhase::Grace { .. }
    ));
    owner.capture.page.workspace.documents()[0]
        .buffer()
        .set_text("Second edited synthetic request.");
    settle(2200);
    assert_eq!(sent(&audit).len(), 1);
    owner.agent_voice.view.send_draft.emit_clicked();
    until(|| {
        owner
            .agent_voice
            .route
            .borrow()
            .attempt
            .as_ref()
            .unwrap()
            .phase
            == RoutePhase::Delivered
    });
    assert_eq!(sent(&audit).len(), 2);
    assert_eq!(
        sent(&audit)[1]["params"]["text"],
        "Second edited synthetic request."
    );
    let edited_entry = services.history.find(&history_id).unwrap();
    assert_eq!(edited_entry.raw_text, text);
    assert_eq!(
        services
            .conversations
            .source_text(&edited_entry, false)
            .unwrap(),
        "Second edited synthetic request."
    );
    // Editing during grace and a CLI recipient switch each disarm immediately.
    record();
    finish();
    owner.capture.page.workspace.documents()[0]
        .buffer()
        .set_text("Do not send this edit automatically.");
    settle(2200);
    assert_eq!(sent(&audit).len(), 2);
    record();
    finish();
    let mut switch = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            app.dbus_connection()
                .unwrap()
                .unique_name()
                .unwrap()
                .as_str(),
            "--object-path",
            mluva_gtk::herdr_cli::PATH,
            "--method",
            "com.mluva.Linux.Herdr.Execute",
            &json!({"command":"select","target":"w1:p2","socket":socket}).to_string(),
        ])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    until(|| switch.try_wait().unwrap().is_some());
    let switched = switch.wait_with_output().unwrap();
    assert!(switched.status.success());
    assert!(
        String::from_utf8(switched.stdout)
            .unwrap()
            .contains("\"connected\":true")
    );
    assert_eq!(owner.agent_voice.view.recipients.selected(), 2);
    assert!(matches!(
        owner
            .agent_voice
            .route
            .borrow()
            .attempt
            .as_ref()
            .unwrap()
            .phase,
        RoutePhase::Draft(_)
    ));
    let buffer = owner.capture.page.workspace.documents()[0].buffer();
    assert_eq!(
        buffer.text(&buffer.start_iter(), &buffer.end_iter(), true),
        text
    );
    settle(2200);
    assert_eq!(sent(&audit).len(), 2);
    // A renamed/replaced occupant is rejected by the actual transport even before the next poll.
    owner.agent_voice.view.recipients.set_selected(1);
    record();
    finish();
    data["agents"][0]["guard"] = json!("c".repeat(64));
    state(&peer_state, &data);
    owner.agent_voice.refresh();
    settle(2300);
    assert_eq!(sent(&audit).len(), 2);
    screenshot(&owner, &root, "agent-unavailable-narrow.png", 420);
    // Select the replacement explicitly. Lost acknowledgement reads a receipt and never resends.
    owner.agent_voice.view.recipients.set_selected(1);
    data["mode"] = json!("drop-ack");
    state(&peer_state, &data);
    record();
    finish();
    until(|| {
        owner
            .agent_voice
            .route
            .borrow()
            .attempt
            .as_ref()
            .unwrap()
            .phase
            == RoutePhase::Delivered
    });
    assert_eq!(sent(&audit).len(), 3);
    data["mode"] = json!("uncertain");
    state(&peer_state, &data);
    record();
    finish();
    until(|| {
        owner
            .agent_voice
            .route
            .borrow()
            .attempt
            .as_ref()
            .unwrap()
            .phase
            == RoutePhase::Unconfirmed
    });
    assert!(!owner.agent_voice.view.send_draft.is_sensitive());
    let unconfirmed = owner.agent_voice.route.borrow().attempt.clone().unwrap();
    assert_eq!(
        outcome(&unconfirmed.history_id).delivery_outcome,
        "unconfirmed"
    );
    screenshot(&owner, &root, "agent-unconfirmed-laptop.png", 1100);
    settle(2300);
    assert_eq!(sent(&audit).len(), 4);
    // The normal Stop control also finishes an agent capture. Cancel during grace wins.
    record();
    owner.capture.page.record_button.emit_clicked();
    until(|| owner.capture.phase().is_none());
    assert!(matches!(
        owner
            .agent_voice
            .route
            .borrow()
            .attempt
            .as_ref()
            .unwrap()
            .phase,
        RoutePhase::Grace { .. }
    ));
    let cancelled = owner.agent_voice.route.borrow().attempt.clone().unwrap();
    owner.agent_voice.view.cancel.emit_clicked();
    settle(2200);
    assert_eq!(sent(&audit).len(), 4);
    assert_eq!(outcome(&cancelled.history_id).delivery_outcome, "cancelled");
    // A second F12 while the countdown runs sends at once, once, to the same target. It
    // neither opens the wheel nor turns the pending send into a draft.
    data.as_object_mut().unwrap().remove("mode");
    state(&peer_state, &data);
    record();
    finish();
    let pending = owner.agent_voice.route.borrow().attempt.clone().unwrap();
    assert!(matches!(pending.phase, RoutePhase::Grace { .. }));
    owner.agent_voice.shortcut();
    assert!(!owner.agent_voice.wheel.window.is_visible());
    until(|| {
        owner
            .agent_voice
            .route
            .borrow()
            .attempt
            .as_ref()
            .unwrap()
            .phase
            == RoutePhase::Delivered
    });
    let now_sent = owner.agent_voice.route.borrow().attempt.clone().unwrap();
    assert_eq!(now_sent.id, pending.id);
    assert!(
        now_sent.submitted_ms.unwrap() - now_sent.final_ms.unwrap() < 2000,
        "the second press must not wait for the countdown"
    );
    assert_eq!(sent(&audit).len(), 5);
    assert_eq!(sent(&audit)[4]["params"]["text"], text);
    assert_eq!(sent(&audit)[4]["params"]["target"], "w1:p1");
    settle(2300);
    assert_eq!(
        sent(&audit).len(),
        5,
        "the countdown must not send it again"
    );
    assert_eq!(outcome(&now_sent.history_id).delivery_outcome, "sent");
    // A recording that recognises nothing is kept in History with its audio, and says so.
    record();
    finish();
    let empty = owner.agent_voice.route.borrow().attempt.clone().unwrap();
    assert_eq!(empty.phase, RoutePhase::Draft(DraftReason::Empty));
    let empty_row = outcome(&empty.history_id);
    assert_eq!(empty_row.delivery_outcome, "no-speech");
    assert!(empty_row.raw_text.trim().is_empty());
    assert!(
        Path::new(empty_row.retained_audio_path.as_deref().unwrap()).exists(),
        "the recording must survive to be recognised again"
    );
    settle(2300);
    assert_eq!(sent(&audit).len(), 5);
    // A Claude Code session outside Herdr with its own inbox: the recording is queued through
    // it once, after the same countdown, and nothing reaches Herdr.
    let inbox_wire = root.join("claude-inbox-wire.jsonl");
    let inbox = inbox_session("inbox-session", "Queued notes", &inbox_wire);
    owner.agent_voice.refresh();
    until(|| row_containing(&owner, "Queued notes").is_some());
    let row = row_containing(&owner, "Queued notes").unwrap();
    assert!(
        owner
            .agent_voice
            .view
            .recipients
            .model()
            .unwrap()
            .item(row)
            .and_downcast::<gtk::StringObject>()
            .unwrap()
            .string()
            .ends_with("· Queue")
    );
    owner.agent_voice.view.recipients.set_selected(row);
    record();
    finish();
    until(|| {
        owner
            .agent_voice
            .route
            .borrow()
            .attempt
            .as_ref()
            .unwrap()
            .phase
            == RoutePhase::Delivered
    });
    let queued = owner.agent_voice.route.borrow().attempt.clone().unwrap();
    assert_eq!(queued.target.session_id, "inbox-session");
    assert!(queued.submitted_ms.unwrap() - queued.final_ms.unwrap() >= 2000);
    assert_eq!(outcome(&queued.history_id).delivery_outcome, "queued");
    let frames = || -> Vec<Value> {
        fs::read_to_string(&inbox_wire)
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    };
    assert_eq!(frames().len(), 1);
    assert_eq!(frames()[0]["session_id"], "inbox-session");
    assert_eq!(frames()[0]["message"]["content"], text);
    assert_eq!(frames()[0]["uuid"], queued.id.as_str());
    settle(2300);
    assert_eq!(frames().len(), 1, "a queued message is never written again");
    assert_eq!(sent(&audit).len(), 5);
    drop(inbox);
    owner.agent_voice.view.recipients.set_selected(1);
    // Incognito uses the same explicit recipient but never creates local History.
    let saved = services.history.recent(100).unwrap().len();
    owner.settings.capture.incognito.set_active(true);
    assert!(services.config().incognito_mode);
    record();
    finish();
    owner.agent_voice.keep_draft();
    assert!(
        owner
            .agent_voice
            .route
            .borrow()
            .attempt
            .as_ref()
            .unwrap()
            .history_id
            .is_none()
    );
    assert_eq!(services.history.recent(100).unwrap().len(), saved);
    settle(2200);
    assert_eq!(sent(&audit).len(), 5);
    // Cancellation during capture does not finalize or submit its audio.
    record();
    owner.agent_voice.view.cancel.emit_clicked();
    until(|| owner.capture.phase().is_none());
    assert_eq!(sent(&audit).len(), 5);
    // The route leaves a trace of every step, with no text, session title or credential, and
    // Incognito leaves none.
    let events = services.diagnostics.recent(1_000).unwrap();
    let route: Vec<_> = events.iter().filter(|e| e.mode == "agent-voice").collect();
    for stage in [
        "route-wheel-opened",
        "route-wheel-closed",
        "route-capture-started",
        "route-capture-stopped",
        "route-countdown-started",
        "route-send-accepted",
        "route-send-unconfirmed",
        "route-draft-kept",
        "route-draft-edited",
        "route-draft-cancelled",
        "route-draft-recipient-changed",
        "route-draft-empty",
    ] {
        assert!(route.iter().any(|e| e.stage == stage), "missing {stage}");
    }
    let traced = serde_json::to_string(&route).unwrap();
    for private in [
        text,
        "Chief of staff",
        "Research notes",
        "Queued notes",
        "SYNTHETIC_KEY",
    ] {
        assert!(!traced.contains(private), "{private}");
    }
    let mut accepted: Vec<_> = route
        .iter()
        .filter(|e| e.stage == "route-send-accepted")
        .map(|e| e.provider.as_str())
        .collect();
    accepted.sort_unstable();
    assert_eq!(
        accepted,
        [
            "guarded-agent",
            "guarded-agent",
            "guarded-agent",
            "guarded-agent",
            "queued-agent"
        ],
        "four sends and one queued message; the incognito attempt is not traced"
    );
    let clipboard = gtk::gdk::Display::default().unwrap().clipboard();
    assert_eq!(
        glib::MainContext::default()
            .block_on(clipboard.read_text_future())
            .unwrap()
            .as_deref(),
        Some("Clipboard must remain unchanged")
    );
    let ended = owner.shutdown();
    glib::MainContext::default().block_on(ended).unwrap();
    let observed = speech.finish();
    assert_eq!(observed.len(), 14);
    assert!(wire(&audit).iter().all(|r| {
        [
            "agent.guarded_list",
            "agent.list",
            "agent.prompt_guarded",
            "agent.prompt_receipt",
        ]
        .contains(&r["method"].as_str().unwrap())
    }));
    fs::write(root.join("voice-result.json"), serde_json::to_vec_pretty(&json!({"captures":16,"recognized":14,"queued_through_inbox":1,"session_wheel":true,"draft_only_sessions_no_delivery":true,"wheel_keyboard_order_reorder_loss":true,"alternate_transcript_no_delivery":true,"guarded_submissions":5,"second_press_sends_once":true,"empty_recognition_kept":true,"history_learns_outcome":true,"route_traced_without_text":true,"live_page_on_start":true,"cancelled_or_disarmed_captures":8,"edited_draft_send":true,"editing_rearmed_draft_cancels":true,"settings_opt_in_persisted":true,"receipt_reconciliation":true,"recognition_immutable":true,"incognito_not_saved":true,"clipboard_unchanged":true,"selected_provider_unchanged":true,"timing_sample_size":1,"release_to_final_ms":attempt.final_ms.unwrap()-attempt.released_ms.unwrap(),"intentional_grace_ms":attempt.submitted_ms.unwrap()-attempt.final_ms.unwrap(),"submit_to_receipt_ms":attempt.received_ms.unwrap()-attempt.submitted_ms.unwrap()})).unwrap()).unwrap();
}
