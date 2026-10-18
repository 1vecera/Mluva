//! Speech-only settings: explicit choices, held catalog replies and two-step setup.
use adw::prelude::*;
use mluva_core::config::AppConfig;
use mluva_gtk::{
    async_runtime::DesktopRuntime, provider_settings::ProviderSection, welcome_view::WelcomeView,
};
use mluva_providers::credentials::CredentialStore;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::VecDeque,
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
#[path = "support/dictation_ui.rs"]
mod ui;
#[test]
#[ignore = "requires private display/network and no host microphone"]
fn speech_choices_catalog_recovery_and_setup_keep_the_selected_route() {
    let root = ui::isolated();
    let runtime = DesktopRuntime::new().unwrap();
    let server = CatalogServer::new(None);
    let config = AppConfig {
        transcription_provider: "litellm".into(),
        transcription_base_url: server.base.clone(),
        transcription_api_key_env: "MLUVA_UNUSED_TEST_KEY".into(),
        transcription_remote_model: Some("my-deployment".into()),
        ..Default::default()
    };
    let section = ProviderSection::new(&config, root.join("model-data"), runtime.clone());
    let window = adw::Window::builder()
        .title("Mluva speech")
        .default_width(700)
        .default_height(800)
        .build();
    let page = adw::PreferencesPage::new();
    page.add(&section.widget);
    window.set_content(Some(&page));
    window.present();
    section.refresh_button.emit_clicked();
    ui::until(|| section.refresh_button.get_sensitive());
    assert_eq!(
        section.values()["transcription_remote_model"],
        "my-deployment"
    );
    assert_eq!(
        server.calls.lock().unwrap().as_slice(),
        [json!({"path":"/models","authorization":false})]
    );
    let labels = section
        .model_row
        .model()
        .unwrap()
        .downcast::<gtk::StringList>()
        .unwrap();
    let labels = (0..labels.n_items())
        .map(|i| labels.string(i).unwrap().to_string())
        .collect::<Vec<_>>();
    assert!(labels.iter().any(|name| name.contains("custom-speech")));
    assert!(!labels.iter().any(|name| name.contains("custom-chat")));
    let gate = root.join("release-catalog");
    let held = CatalogServer::new(Some(gate.clone()));
    section.endpoint_entry.set_text(&held.base);
    section.refresh_button.emit_clicked();
    ui::until(|| !held.calls.lock().unwrap().is_empty());
    section.provider_row.set_selected(1);
    assert_eq!(section.provider().id, "local");
    fs::write(gate, "").unwrap();
    ui::settle(0.2);
    assert_eq!(section.provider().id, "local");
    assert!(!section.model_row.get_visible());
    assert_eq!(section.values()["transcription_provider"], "local");
    section.provider_row.set_selected(2);
    section.endpoint_entry.set_text("not a URL");
    section.refresh_button.emit_clicked();
    assert!(section.status.get_visible());
    assert!(section.status.label().contains("Check"));
    // Setup has the same speech route and no generation credentials or opt-in.
    let saved = Rc::new(RefCell::new(vec![]));
    let writes = saved.clone();
    let done = Rc::new(std::cell::Cell::new(false));
    let finished = done.clone();
    let welcome = WelcomeView::new(
        &config,
        root.join("welcome-data"),
        runtime.clone(),
        Arc::new(CredentialStore::new()),
        Rc::new(move |changes| {
            writes.borrow_mut().push(changes.clone());
            true
        }),
        Rc::new(move || finished.set(true)),
    );
    window.set_content(Some(&welcome.widget));
    ui::settle(0.1);
    assert_eq!(
        welcome.steps.visible_child_name().as_deref(),
        Some("speech")
    );
    welcome.next.emit_clicked();
    ui::settle(0.1);
    assert_eq!(
        welcome.steps.visible_child_name().as_deref(),
        Some("appearance")
    );
    welcome.next.emit_clicked();
    ui::until(|| done.get());
    assert!(
        saved
            .borrow()
            .iter()
            .all(|changes| !changes.contains_key("rewrite_provider"))
    );
    ui::paint(&window, "native-onboarding");
    section.stop_lookup();
    welcome.speech.stop_lookup();
    window.close();
    let shutdown = runtime.shutdown(async {});
    let done = Rc::new(std::cell::Cell::new(false));
    let flag = done.clone();
    glib::MainContext::default().spawn_local(async move {
        shutdown.await.unwrap();
        flag.set(true);
    });
    ui::until(|| done.get());
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
