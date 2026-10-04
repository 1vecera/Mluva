//! Native setup/preferences against unchanged released widgets and external catalogs/keyring.
#![recursion_limit = "512"]
use adw::prelude::*;
use mluva_core::config::AppConfig;
use mluva_gtk::{
    appearance_settings::AppearanceSettings,
    async_runtime::DesktopRuntime,
    document_layout::DocumentResources,
    local_model_settings::LocalModelSettings,
    provider_settings::{ProviderSection, ProviderSettings},
    settings_view::SaveSettings,
    theme::ThemeController,
    welcome_view::WelcomeView,
    workspace_settings::WorkspaceSettings,
};
use mluva_providers::{catalog::Scope, credentials::CredentialStore, local_assets::MODEL_CATALOG};
use mluva_workflows::services::{ApplicationServices, SettingsActivity};
use serde_json::{Map, Value, json};
use std::{
    cell::RefCell,
    collections::VecDeque,
    fs,
    io::{Read, Write},
    net::TcpListener,
    os::{
        fd::AsRawFd,
        unix::fs::{OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
#[path = "support/fixture_states.rs"]
mod fixture_states;

fn drain() {
    while glib::MainContext::default().pending() {
        glib::MainContext::default().iteration(false);
    }
}
fn settle() {
    let end = Instant::now() + Duration::from_millis(100);
    while Instant::now() < end {
        drain();
        thread::sleep(Duration::from_millis(3));
    }
}
fn check(actual: Value, expected: &Value, name: &str) {
    fn differences(actual: &Value, expected: &Value, path: &str, result: &mut Vec<String>) {
        match (actual, expected) {
            (Value::Object(a), Value::Object(b)) => {
                for (key, value) in a {
                    differences(
                        value,
                        b.get(key).unwrap_or(&Value::Null),
                        &format!("{path}.{key}"),
                        result,
                    );
                }
                for key in b.keys().filter(|key| !a.contains_key(*key)) {
                    result.push(format!("{path}.{key}: missing"));
                }
            }
            (a, b) if a != b => result.push(format!("{path}: actual={a}, expected={b}")),
            _ => {}
        }
    }
    if actual != *expected {
        let root = PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").unwrap());
        fs::write(
            root.join("provider-mismatch-actual.json"),
            serde_json::to_vec_pretty(&actual).unwrap(),
        )
        .unwrap();
        fs::write(
            root.join("provider-mismatch-expected.json"),
            serde_json::to_vec_pretty(expected).unwrap(),
        )
        .unwrap();
        let mut result = Vec::new();
        differences(&actual, expected, "state", &mut result);
        panic!("{name}: {}", result.join("\n"));
    }
}
fn until(mut ready: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(9);
    while !ready() {
        assert!(
            Instant::now() < end,
            "Native preferences owner did not complete"
        );
        drain();
        thread::sleep(Duration::from_millis(3));
    }
    settle();
}
fn widgets(widget: &impl IsA<gtk::Widget>) -> Vec<gtk::Widget> {
    fn visit(widget: gtk::Widget, rows: &mut Vec<gtk::Widget>) {
        rows.push(widget.clone());
        let mut child = widget.first_child();
        while let Some(current) = child {
            visit(current.clone(), rows);
            child = current.next_sibling();
        }
    }
    let mut rows = Vec::new();
    visit(widget.as_ref().clone(), &mut rows);
    rows
}
fn button(row: &gtk::Button) -> Value {
    json!({"label":row.label().map(String::from),"tooltip":row.tooltip_text().map(String::from),"visible":row.get_visible(),"sensitive":row.get_sensitive()})
}
fn combo(row: &adw::ComboRow) -> Value {
    let labels = row
        .model()
        .and_downcast::<gtk::StringList>()
        .map(|model| {
            (0..model.n_items())
                .map(|i| model.string(i).unwrap().to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    json!({"title":row.title().to_string(),"subtitle":row.subtitle().map(String::from),"visible":row.get_visible(),"sensitive":row.get_sensitive(),"selected":row.selected(),"labels":labels})
}
fn section(row: &ProviderSection) -> Value {
    json!({"title":row.widget.title().to_string(),"scope":row.scope.name(),"provider":row.provider().id,"values":row.values(),"description":row.description.label().to_string(),"key":{"visible":row.api_key_entry.get_visible(),"empty":row.api_key_entry.text().is_empty(),"hint":row.key_hint.get_visible()},
    "local":{"visible":row.local.widget.get_visible(),"selected":row.local.selected(),"device":row.local.device(),"language":row.local.languages.widget.label().map(String::from),"ready":row.local.is_ready(),"downloaded":row.local.downloaded(),"label":row.local.label.label().to_string(),"requirements":row.local.requirements.label().to_string(),"status":row.local.status.label().to_string(),"progress":row.local.progress.text().map(String::from),"progress_visible":row.local.progress.get_visible(),"download":button(&row.local.button)},
    "model":combo(&row.model_row),"manual":{"text":row.model_entry.text().to_string(),"visible":row.model_entry.get_visible()},"fast":{"active":row.fast_row.is_active(),"visible":row.fast_row.get_visible(),"sensitive":row.fast_row.get_sensitive(),"subtitle":row.fast_row.subtitle().map(String::from)},"thinking":combo(&row.thinking_row.widget),"connection":{"visible":row.connection.get_visible(),"subtitle":row.connection.subtitle().map(String::from),"refresh":button(&row.refresh_button)},"advanced":{"visible":row.advanced.get_visible(),"endpoint":row.endpoint_entry.text().to_string(),"key_variable":row.key_entry.text().to_string()},"preview":{"visible":row.preview.get_visible(),"chunk":row.chunk_row.value()},"status":{"text":row.status.label().to_string(),"visible":row.status.get_visible()}})
}
fn appearance(row: &AppearanceSettings) -> Value {
    json!({"values":row.values(),"caption":row.caption.label().to_string(),"lines":row.line_label.label().to_string(),"opacity":row.opacity_label.label().to_string(),"positions":row.position.buttons.iter().map(|button|button.label().map(String::from)).collect::<Vec<_>>()})
}
fn updated(config: &AppConfig, changes: &Value) -> AppConfig {
    let mut document = serde_json::to_value(config).unwrap();
    document
        .as_object_mut()
        .unwrap()
        .extend(changes.as_object().unwrap().clone());
    serde_json::from_value(document).unwrap()
}
fn normalized(value: Value, base: &str) -> Value {
    match value {
        Value::String(value) => Value::String(value.replace(base, "$HTTP")),
        Value::Array(values) => Value::Array(
            values
                .into_iter()
                .map(|value| normalized(value, base))
                .collect(),
        ),
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, normalized(value, base)))
                .collect(),
        ),
        value => value,
    }
}
fn action(row: &Rc<ProviderSection>, step: &Value, config: &AppConfig) {
    match step["op"].as_str().unwrap() {
        "observe" => {}
        "provider" => row
            .provider_row
            .set_selected(step["index"].as_u64().unwrap() as usize),
        "manual" => {
            row.model_row
                .set_selected(row.model_row.model().unwrap().n_items() - 1);
            row.model_entry.set_text(step["value"].as_str().unwrap());
        }
        "model" => row
            .model_row
            .set_selected(step["index"].as_u64().unwrap() as u32),
        "endpoint" => row.endpoint_entry.set_text(step["value"].as_str().unwrap()),
        "key_env" => row.key_entry.set_text(step["value"].as_str().unwrap()),
        "fast" => row.fast_row.set_active(step["value"].as_bool().unwrap()),
        "thinking" => row
            .thinking_row
            .widget
            .set_selected(step["index"].as_u64().unwrap() as u32),
        "chunk" => row.chunk_row.set_value(step["value"].as_f64().unwrap()),
        "local" => row.local.scale.set_value(
            MODEL_CATALOG
                .iter()
                .filter(|model| model.offered)
                .position(|model| model.id == step["value"].as_str().unwrap())
                .unwrap() as f64,
        ),
        "language" => row.local.languages.choose(step["value"].as_str().unwrap()),
        "gpu" => row.local.gpu.set_active(step["value"].as_bool().unwrap()),
        "refresh_config" => row.refresh_config(&updated(config, &step["changes"])),
        "load" => row.refresh_button.emit_clicked(),
        "stop" => row.stop_lookup(),
        "unmap" => row.widget.emit_by_name::<()>("unmap", &[]),
        "key" => row.api_key_entry.set_text(step["value"].as_str().unwrap()),
        op => panic!("unknown released action {op}"),
    }
}
fn records(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn children_closed(evidence: &Path) {
    let processes = records(&evidence.join("process.jsonl"));
    until(|| {
        processes
            .iter()
            .all(|process| !Path::new(&format!("/proc/{}", process["pid"])).exists())
    });
    for process in processes {
        assert!(
            !Path::new(process["cwd"].as_str().unwrap()).exists(),
            "catalog child workspace remains"
        );
    }
}
struct CatalogServer {
    base: String,
    calls: Arc<Mutex<Vec<Value>>>,
    stop: Arc<AtomicBool>,
    join: Option<thread::JoinHandle<()>>,
}
impl CatalogServer {
    fn new(gate: Option<PathBuf>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let observed = calls.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let running = stop.clone();
        let mut responses = VecDeque::from([
            json!({"data":[{"id":"custom-chat","model_info":{"mode":"chat"}},{"id":"custom-speech","model_info":{"mode":"stt"}},{"id":"unknown-alias"}]}),
        ]);
        let join = thread::spawn(move || {
            let mut children = Vec::new();
            while !running.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let body = responses.pop_front().expect("unexpected catalog request");
                        let gate = gate.clone();
                        let observed = observed.clone();
                        children.push(thread::spawn(move ||{stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();let mut bytes=Vec::new();while !bytes.ends_with(b"\r\n\r\n"){let mut byte=[0];stream.read_exact(&mut byte).unwrap();bytes.push(byte[0]);}let headers=String::from_utf8(bytes).unwrap();let path=headers.lines().next().unwrap().split_whitespace().nth(1).unwrap().to_owned();let authorization=headers.lines().any(|line|line.to_ascii_lowercase().starts_with("authorization:"));observed.lock().unwrap().push(json!({"path":path,"authorization":authorization}));if let Some(gate)=gate{let end=Instant::now()+Duration::from_secs(9);while !gate.exists(){assert!(Instant::now()<end,"catalog gate not released");thread::sleep(Duration::from_millis(2));}}let body=serde_json::to_vec(&body).unwrap();let head=format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len());let _=stream.write_all(head.as_bytes());let _=stream.write_all(&body);}));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(error) => panic!("catalog endpoint failed: {error}"),
                }
            }
            for child in children {
                child.join().unwrap();
            }
        });
        Self {
            base,
            calls,
            stop,
            join: Some(join),
        }
    }
}
impl Drop for CatalogServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.join.take().unwrap().join().unwrap();
    }
}
fn keyring(root: &Path, failed: bool) {
    fs::create_dir_all(root).unwrap();
    fs::write(
        root.join("keyring.json"),
        serde_json::to_vec(
            &json!({"lookup":{"stdout":""},"store":{"exit":if failed{1}else{0},"sleep_ms":30}}),
        )
        .unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "requires private display/session/network/device isolation and native external catalog/keyring peers"]
fn released_provider_welcome_workspace_forms() {
    let root =
        PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").expect("private desktop runner"));
    assert!(std::env::var_os("MLUVA_HOST_NET_NS").is_some());
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
    for name in ["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XAUTHORITY"] {
        assert!(
            fs::canonicalize(std::env::var_os(name).unwrap())
                .unwrap()
                .starts_with(&root)
        );
    }
    for name in [
        "ELEVENLABS_API_KEY",
        "DAS_ITEM_ELEVEN_LABS_API_KEY__CREDENTIAL",
        "ELEVEN_LABS_STT_TOKEN",
        "LITELLM_API_KEY",
    ] {
        assert!(std::env::var_os(name).is_none());
    }
    let tools = root.join("provider-tools");
    fs::create_dir_all(&tools).unwrap();
    assert_eq!(
        std::env::var_os("PATH")
            .unwrap()
            .to_string_lossy()
            .split(':')
            .next()
            .unwrap(),
        tools.to_str().unwrap()
    );
    let key_root = PathBuf::from(
        std::env::var_os("CREDENTIAL_FIXTURE_ROOT").expect("private synthetic keyring"),
    );
    assert!(key_root.starts_with(&root));
    let target = PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap()).join("debug");
    let adapter = tools.join("codex");
    let quote = |path: &Path| format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"));
    let specfile = root.join("provider-codex.json");
    fs::write(
        &adapter,
        format!(
            "#!/bin/sh\nexec {} serve {} \"$@\"\n",
            quote(&target.join("codex-fixture-peer")),
            quote(&specfile)
        ),
    )
    .unwrap();
    fs::set_permissions(&adapter, fs::Permissions::from_mode(0o700)).unwrap();
    std::os::unix::fs::symlink(
        target.join("credential-fixture-peer"),
        tools.join("secret-tool"),
    )
    .unwrap();
    let fixture = fixture_states::load(
        include_str!("fixtures/released-provider-pages.json"),
        &["sections", "pages", "welcome", "workspace", "downloads"],
        "ui",
    );
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
    let runtime = DesktopRuntime::new().unwrap();
    let mut observations = 0;
    for row in fixture["sections"].as_array().unwrap() {
        let name = row["name"].as_str().unwrap();
        let directory = root.join(name);
        fs::create_dir_all(&directory).unwrap();
        let scope = if row["scope"] == "speech" {
            Scope::Speech
        } else {
            Scope::Rewrite
        };
        let kind = row["catalog_kind"].as_str();
        let gate = directory.join("release");
        let server = CatalogServer::new(if kind == Some("stale-endpoint") {
            Some(gate.clone())
        } else {
            None
        });
        let config = updated(&AppConfig::default(), &row["initial"]);
        let mut config = serde_json::to_value(config).unwrap();
        fn expand(value: &mut Value, base: &str) {
            match value {
                Value::String(text) => *text = text.replace("$HTTP", base),
                Value::Object(map) => {
                    for v in map.values_mut() {
                        expand(v, base);
                    }
                }
                _ => {}
            }
        }
        expand(&mut config, &server.base);
        let config: AppConfig = serde_json::from_value(config).unwrap();
        let evidence = directory.join("evidence");
        fs::create_dir(&evidence).unwrap();
        let mut spec =
            json!({"scenario":if kind==Some("failure"){"reject"}else{"clean"},"evidence":evidence});
        if kind == Some("invalid-name") {
            spec["catalog"] = json!([{"id":"bad","model":"bad-model","displayName":"Private\nname","isDefault":true}]);
        }
        if matches!(kind, Some("stale-provider" | "unmapped" | "reopened")) {
            spec["model_gate"] = json!(gate);
        }
        fs::write(&specfile, serde_json::to_vec(&spec).unwrap()).unwrap();
        let owner = ProviderSection::new(&config, scope, directory.join("data"), runtime.clone());
        if let Some(kind) = kind {
            settle();
            assert_eq!(
                normalized(section(&owner), &server.base),
                row["stages"][0]["ui"],
                "{name}: initial"
            );
            owner.refresh_button.emit_clicked();
            assert_eq!(
                normalized(section(&owner), &server.base),
                row["stages"][1]["ui"],
                "{name}: loading"
            );
            let compatible = matches!(kind, "compatible" | "stale-endpoint");
            until(|| {
                if compatible {
                    !server.calls.lock().unwrap().is_empty()
                } else if kind == "failure" {
                    owner.refresh_button.get_sensitive()
                } else {
                    records(&evidence.join("requests.jsonl"))
                        .iter()
                        .any(|row| row["message"]["method"] == "model/list")
                }
            });
            match kind {
                "stale-provider" => owner.provider_row.set_selected(1),
                "stale-endpoint" => owner
                    .endpoint_entry
                    .set_text(&format!("{}/changed/v1", server.base)),
                "unmapped" => owner.widget.emit_by_name::<()>("unmap", &[]),
                "reopened" => owner.refresh_config(&AppConfig {
                    rewrite_model: Some("new-draft".into()),
                    ..config.clone()
                }),
                _ => {}
            }
            fs::write(&gate, []).unwrap();
            until(|| owner.refresh_button.get_sensitive());
            assert_eq!(
                normalized(section(&owner), &server.base),
                row["stages"][2]["ui"],
                "{name}: finished"
            );
            if matches!(kind, "codex" | "hidden") {
                action(
                    &owner,
                    &json!({"op":"manual","value":"not-in-catalog"}),
                    &config,
                );
                settle();
                assert_eq!(
                    normalized(section(&owner), &server.base),
                    row["stages"][3]["ui"],
                    "{name}: manual after list"
                );
            }
            assert_eq!(
                json!(*server.calls.lock().unwrap()),
                row["http"],
                "{name}: actual endpoint requests"
            );
            observations += row["stages"].as_array().unwrap().len();
        } else {
            for stage in row["stages"].as_array().unwrap() {
                action(&owner, &stage["action"], &config);
                settle();
                assert_eq!(section(&owner), stage["ui"], "{name}: {}", stage["action"]);
                observations += 1;
            }
        }
        owner.stop_lookup();
        owner.local.stop();
        drop(owner);
        children_closed(&evidence);
    }
    for row in fixture["pages"].as_array().unwrap() {
        let name = row["name"].as_str().unwrap();
        keyring(&key_root, row["key_fail"] == true);
        let calls = Rc::new(RefCell::new(Vec::new()));
        let saved = calls.clone();
        let allowed = name != "save-rejected";
        let save: SaveSettings = Rc::new(move |changes| {
            saved.borrow_mut().push(json!(changes));
            allowed
        });
        let config = AppConfig::default();
        let owner = ProviderSettings::new(
            &config,
            root.join(name),
            runtime.clone(),
            Arc::new(CredentialStore::new()),
            save,
            None,
        );
        for step in row["actions"].as_array().unwrap() {
            action(
                if step["scope"] == "speech" {
                    &owner.speech
                } else {
                    &owner.rewrite
                },
                step,
                &config,
            );
        }
        let observe = || json!({"speech":section(&owner.speech),"rewrite":section(&owner.rewrite),"status":owner.status.label().to_string(),"sensitive":owner.widget.get_sensitive(),"calls":*calls.borrow()});
        assert_eq!(observe(), row["stages"][0]["ui"], "{name}: edited");
        owner.apply_button.emit_clicked();
        assert_eq!(observe(), row["stages"][1]["ui"], "{name}: applied");
        until(|| owner.widget.get_sensitive());
        assert_eq!(observe(), row["stages"][2]["ui"], "{name}: settled");
        observations += 3;
        drop(owner);
    }
    for row in fixture["welcome"].as_array().unwrap() {
        let name = row["name"].as_str().unwrap();
        keyring(&key_root, row["key_fail"] == true);
        let calls = Rc::new(RefCell::new(Vec::new()));
        let saved = calls.clone();
        let allowed = name != "save-rejected";
        let save: SaveSettings = Rc::new(move |changes| {
            saved.borrow_mut().push(json!(changes));
            allowed
        });
        let finished = Rc::new(RefCell::new(Vec::new()));
        let finish = finished.clone();
        let config = AppConfig::default();
        let owner = WelcomeView::new(
            &config,
            root.join(format!("welcome-{name}")),
            runtime.clone(),
            Arc::new(CredentialStore::new()),
            save,
            Rc::new(move || finish.borrow_mut().push(true)),
        );
        let window = adw::Window::builder()
            .default_width(1060)
            .default_height(780)
            .content(&owner.widget)
            .build();
        window.present();
        settle();
        for stage in row["stages"].as_array().unwrap() {
            let step = &stage["action"];
            if let Some(scope) = step["scope"].as_str() {
                action(
                    if scope == "speech" {
                        &owner.speech
                    } else {
                        &owner.rewrite
                    },
                    step,
                    &config,
                );
            } else {
                match step["op"].as_str().unwrap() {
                    "observe" => {}
                    "next" => {
                        owner.next.emit_clicked();
                        until(|| {
                            owner.next.get_sensitive()
                                || [
                                    "missing-key",
                                    "key-failed",
                                    "key-invalid",
                                    "local-not-ready",
                                ]
                                .contains(&name)
                                    && owner.status.label() != "Saving key to your desktop keyring…"
                        });
                    }
                    "back" => owner.back.emit_clicked(),
                    "reopen" => owner.refresh_config(&AppConfig {
                        rewrite_provider: "none".into(),
                        widget_lines: 3,
                        ..config.clone()
                    }),
                    "appearance" => {
                        owner
                            .appearance
                            .position
                            .set_selected(step["position"].as_u64().unwrap() as usize);
                        owner
                            .appearance
                            .lines
                            .set_value(step["lines"].as_f64().unwrap());
                        owner
                            .appearance
                            .opacity
                            .set_value(step["opacity"].as_f64().unwrap());
                        owner
                            .appearance
                            .paste
                            .set_active(step["paste"].as_bool().unwrap());
                    }
                    op => panic!("unknown welcome step {op}"),
                }
            }
            settle();
            settle();
            settle();
            let observed = json!({"step":owner.step(),"route":owner.steps.visible_child_name().map(String::from),"title":owner.title.label().to_string(),"step_label":owner.step_label.label().to_string(),"progress":owner.progress.fraction(),"speech":section(&owner.speech),"rewrite":section(&owner.rewrite),"appearance":appearance(&owner.appearance),"polish_visible":owner.polish_preview.widget.get_visible(),"polish":{"caption":owner.polish_preview.caption.label().to_string(),"text":owner.polish_preview.text.label().to_string(),"progress":owner.polish_preview.progress.fraction()},"back":button(&owner.back),"next":button(&owner.next),"status":owner.status.label().to_string(),"calls":*calls.borrow(),"finished":*finished.borrow()});
            check(observed, &stage["ui"], &format!("welcome {name}: {step}"));
            observations += 1;
        }
        window.destroy();
        settle();
        drop(owner);
    }
    for row in fixture["workspace"].as_array().unwrap() {
        let name = row["name"].as_str().unwrap();
        let config = AppConfig {
            rewrite_provider: if name == "skip-rejects-live" {
                "none"
            } else {
                "codex"
            }
            .into(),
            ..Default::default()
        };
        let calls = Rc::new(RefCell::new(Vec::new()));
        let saved = calls.clone();
        let allowed = !matches!(name, "edited-rejected" | "skip-rejects-live");
        let edited = Rc::new(RefCell::new(Vec::new()));
        let prompts = edited.clone();
        let owner = WorkspaceSettings::new(
            &config,
            Rc::new(move |changes| {
                saved.borrow_mut().push(json!(changes));
                allowed
            }),
            Some(Rc::new(move |prompt| {
                prompts.borrow_mut().push(prompt.to_owned())
            })),
        );
        for stage in row["stages"].as_array().unwrap() {
            let step = &stage["action"];
            match step["op"].as_str().unwrap() {
                "observe" => {}
                "row" => {
                    let row = widgets(&owner.widget)
                        .into_iter()
                        .find(|widget| {
                            widget
                                .downcast_ref::<adw::PreferencesRow>()
                                .is_some_and(|row| {
                                    row.title().as_str() == step["title"].as_str().unwrap()
                                })
                        })
                        .unwrap();
                    match step["kind"].as_str().unwrap() {
                        "combo" => row
                            .downcast_ref::<adw::ComboRow>()
                            .unwrap()
                            .set_selected(step["value"].as_u64().unwrap() as u32),
                        "spin" => row
                            .downcast_ref::<adw::SpinRow>()
                            .unwrap()
                            .set_value(step["value"].as_f64().unwrap()),
                        "switch" => row
                            .downcast_ref::<adw::SwitchRow>()
                            .unwrap()
                            .set_active(step["value"].as_bool().unwrap()),
                        _ => unreachable!(),
                    }
                }
                "prompt" => widgets(&owner.widget)
                    .into_iter()
                    .find_map(|widget| {
                        widget
                            .downcast::<adw::ActionRow>()
                            .ok()
                            .filter(|row| row.title() == "Edit Live prompt")
                    })
                    .unwrap()
                    .emit_by_name::<()>("activated", &[]),
                "reopen" => owner.refresh_config(&AppConfig {
                    live_rewrite_enabled: true,
                    live_rewrite_continuous: true,
                    live_rewrite_template: "polish".into(),
                    widget_lines: 1,
                    ..config.clone()
                }),
                "apply" => owner.apply_button.emit_clicked(),
                _ => unreachable!(),
            }
            settle();
            let menus: Map<_, _> = widgets(&owner.widget)
                .iter()
                .filter_map(|widget| widget.downcast_ref::<adw::ComboRow>())
                .map(|row| (row.title().to_string(), combo(row)))
                .collect();
            assert_eq!(
                json!({"values":owner.values(),"appearance":appearance(&owner.appearance),"menus":menus,"status":owner.status.label().to_string(),"calls":*calls.borrow(),"edited":*edited.borrow()}),
                stage["ui"],
                "workspace {name}: {step}"
            );
            observations += 1;
        }
    }
    for row in fixture["downloads"].as_array().unwrap() {
        let name = row["name"].as_str().unwrap();
        let data = root.join(format!("download-{name}"));
        fs::create_dir_all(data.join("models")).unwrap();
        let lock = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(data.join("models/.download.lock"))
            .unwrap();
        assert_eq!(
            unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
            0
        );
        let changed = Rc::new(RefCell::new(Vec::new()));
        let calls = changed.clone();
        let owner = LocalModelSettings::new(
            data,
            runtime.clone(),
            "qwen3-1.7b",
            "cpu",
            "eng",
            Rc::new(move |id| calls.borrow_mut().push(id.to_owned())),
            Rc::new(|_| {}),
        );
        let observe = || json!({"selected":owner.selected(),"status":owner.status.label().to_string(),"progress":owner.progress.text().map(String::from),"fraction":owner.progress.fraction(),"progress_visible":owner.progress.get_visible(),"button":button(&owner.button),"changed":*changed.borrow()});
        check(
            observe(),
            &row["stages"][0]["ui"],
            &format!("download {name}: initial"),
        );
        owner.button.emit_clicked();
        check(
            observe(),
            &row["stages"][1]["ui"],
            &format!("download {name}: started"),
        );
        match name {
            "selection-invalidates" => owner.scale.set_value(
                MODEL_CATALOG
                    .iter()
                    .filter(|model| model.offered)
                    .position(|model| model.id == "parakeet-v3")
                    .unwrap() as f64,
            ),
            "stop-invalidates" => owner.stop(),
            _ => {}
        }
        if name == "locked" {
            until(|| owner.button.get_sensitive());
        } else {
            settle();
            settle();
        }
        check(
            observe(),
            &row["stages"][2]["ui"],
            &format!("download {name}: finished"),
        );
        owner.stop();
        drop(lock);
        observations += 3;
    }
    // A save applies real store policy and refreshes the same page immediately;
    // no config borrow may survive that external callback.
    let paths = mluva_core::config::AppPaths {
        config: root.join("save-owner/config"),
        data: root.join("save-owner/data"),
        runtime: root.join("save-owner/runtime"),
    };
    let services = ApplicationServices::open(paths).unwrap();
    let slot = Rc::new(RefCell::new(std::rc::Weak::<WorkspaceSettings>::new()));
    let page = slot.clone();
    let stored = services.clone();
    let owner = WorkspaceSettings::new(
        &services.config(),
        Rc::new(move |changes| {
            let update = stored
                .apply_settings(changes, &SettingsActivity::default())
                .unwrap()
                .unwrap();
            if let Some(page) = page.borrow().upgrade() {
                page.refresh_config(&update.config);
            }
            true
        }),
        None,
    );
    slot.replace(Rc::downgrade(&owner));
    widgets(&owner.widget)
        .into_iter()
        .find_map(|widget| {
            widget
                .downcast::<adw::SwitchRow>()
                .ok()
                .filter(|row| row.title() == "Show history sidebar by default")
        })
        .unwrap()
        .set_active(true);
    owner.apply_button.emit_clicked();
    assert!(services.config().history_sidebar_visible);
    assert!(
        AppConfig::load(&services.paths.config.join("config.json"))
            .unwrap()
            .history_sidebar_visible
    );
    assert_eq!(owner.status.label(), "Settings saved");
    // Teardown during a real gated lookup invalidates the page before cleanup.
    // Retained GTK labels never receive late catalogs; owned children are reaped.
    let evidence = root.join("catalog-owner-exit");
    fs::create_dir(&evidence).unwrap();
    let gate = evidence.join("release");
    fs::write(
        &specfile,
        serde_json::to_vec(&json!({"scenario":"clean","evidence":evidence,"model_gate":gate}))
            .unwrap(),
    )
    .unwrap();
    let owner = ProviderSection::new(
        &AppConfig::default(),
        Scope::Rewrite,
        root.join("exit-data"),
        runtime.clone(),
    );
    owner.refresh_button.emit_clicked();
    until(|| {
        records(&evidence.join("requests.jsonl"))
            .iter()
            .any(|row| row["message"]["method"] == "model/list")
    });
    let status = owner.status.clone();
    drop(owner);
    let stopped = status.label();
    fs::write(&gate, []).unwrap();
    children_closed(&evidence);
    for _ in 0..6 {
        settle();
    }
    assert_eq!(status.label(), stopped);
    assert!(records(&evidence.join("requests.jsonl")).iter().all(|row| {
        !["thread/start", "turn/start"].contains(&row["message"]["method"].as_str().unwrap_or(""))
    }));
    println!(
        "Matched {} section transactions, {} provider saves, {} welcome flows, {} workspace forms and {observations} GTK states.",
        fixture["sections"].as_array().unwrap().len(),
        fixture["pages"].as_array().unwrap().len(),
        fixture["welcome"].as_array().unwrap().len(),
        fixture["workspace"].as_array().unwrap().len()
    );
}
