//! First-run setup through real credential, pinned HTTPS artifact and local ASR boundaries.
use super::{
    application_commands, capture_window, clipboard, drain, records, settle_for, until, widgets,
    window_id,
};
use adw::prelude::*;
use mluva_audio::wav::WaveReader;
use mluva_core::config::{AppConfig, AppPaths};
use mluva_gtk::{application::ApplicationDesktop, provider_settings::ProviderSection};
use mluva_providers::local_assets::MODEL_CATALOG;
use mluva_workflows::{capture::CapturePhase, services::ApplicationServices};
use serde_json::{Value, json};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command},
    thread,
    time::{Duration, Instant},
};

fn read(path: impl AsRef<Path>) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn write(path: impl AsRef<Path>, value: &Value) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
fn digest(path: &Path) -> String {
    let mut hash = glib::Checksum::new(glib::ChecksumType::Sha256).unwrap();
    let mut file = fs::File::open(path).unwrap();
    let mut block = [0; 65_536];
    loop {
        let count = file.read(&mut block).unwrap();
        if count == 0 {
            break;
        }
        hash.update(&block[..count]);
    }
    hash.string().unwrap()
}
fn verified(path: &Path, spec: &Value) {
    assert_eq!(fs::metadata(path).unwrap().len(), spec["size"]);
    assert_eq!(digest(path), spec["sha256"]);
}
fn wait(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(180);
    while !predicate() {
        assert!(Instant::now() < deadline, "Onboarding did not settle");
        drain();
        thread::sleep(Duration::from_millis(5));
    }
    drain();
}

pub fn paths(root: &Path) -> AppPaths {
    let state = root.join("onboarding-state");
    AppPaths {
        config: state.join("config/mluva"),
        data: state.join("data/mluva"),
        runtime: state.join("runtime/mluva"),
    }
}

pub struct Artifacts {
    root: PathBuf,
    child: Option<Child>,
}
impl Artifacts {
    pub fn prepare(root: &Path, tools: &Path, target: &Path, reference: &Value) -> Self {
        let assets = PathBuf::from(std::env::var_os("MLUVA_TEST_ONBOARDING_ASSETS").unwrap());
        let sample = assets.join("speech.wav");
        assert_eq!(digest(&sample), reference["sample_sha256"]);
        let mut audio = WaveReader::open(&sample).unwrap();
        assert!(audio.compatible());
        let pcm = audio
            .read_frames(audio.metadata.frame_count as usize)
            .unwrap();
        let hex = pcm
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        write(
            tools.join("test-config.json"),
            &json!({"pcm_hex":hex,"wait":true}),
        );
        write(
            root.join("provider-keyring/keyring.json"),
            &json!({"lookup":{"exit":1},"store":{"exit":1,"stderr":"Keyring unavailable"}}),
        );
        let root = root.join("artifact-server");
        assert_eq!(
            PathBuf::from(std::env::var_os("SSL_CERT_FILE").unwrap()),
            root.join("cert.pem")
        );
        let runtime = &reference["runtime"]["assets"]["cpu"];
        let archive = assets.join(runtime["url"].as_str().unwrap().rsplit('/').next().unwrap());
        verified(&archive, runtime);
        let mut responses = vec![
            json!({"host":"github.com","uri":runtime["url"].as_str().unwrap().strip_prefix("https://github.com").unwrap(),"file":archive,"release":root.join("release")}),
        ];
        let model = &reference["model"];
        for file in model["files"].as_array().unwrap() {
            let path = assets.join(file["name"].as_str().unwrap());
            verified(&path, file);
            responses.push(json!({"host":"huggingface.co","uri":format!("/{}/resolve/{}/{}",model["repo"].as_str().unwrap(),model["revision"].as_str().unwrap(),file["name"].as_str().unwrap()),"file":path}));
        }
        responses[1]["pause_after"] = json!(16 * 1024 * 1024);
        responses[1]["resume"] = json!(root.join("resume"));
        write(root.join("spec.json"), &json!({"responses":responses}));
        let log = fs::File::create(root.join("server.log")).unwrap();
        let child = Command::new(target.join("examples/artifact_https_peer"))
            .arg(&root)
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap();
        let mut owner = Self {
            root,
            child: Some(child),
        };
        until(|| {
            assert!(owner.child.as_mut().unwrap().try_wait().unwrap().is_none());
            owner.root.join("ready.json").exists()
        });
        owner
    }
    pub fn finish(&mut self, expected: &Value) {
        fs::write(self.root.join("stop"), "").unwrap();
        let child = self.child.as_mut().unwrap();
        until(|| child.try_wait().unwrap().is_some());
        assert!(child.wait().unwrap().success());
        self.child.take();
        let receipts = (0..expected.as_array().unwrap().len())
            .map(|i| read(self.root.join(format!("response-{i}.json"))))
            .collect::<Vec<_>>();
        assert_eq!(
            json!(receipts),
            *expected,
            "actual pinned artifact transfers"
        );
    }
}
impl Drop for Artifacts {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn config(config: AppConfig) -> Value {
    let mut value = serde_json::to_value(config).unwrap();
    value.as_object_mut().unwrap().retain(|key, _| {
        matches!(
            key.as_str(),
            "welcome_completed"
                | "transcription_provider"
                | "local_model"
                | "local_device"
                | "language_code"
                | "rewrite_provider"
                | "live_rewrite_enabled"
                | "automatic_titles"
                | "widget_lines"
                | "widget_opacity"
                | "widget_position"
                | "auto_paste"
        )
    });
    value
}
fn provider(section: &ProviderSection, identifier: &str) {
    let index = section
        .scope
        .providers()
        .iter()
        .position(|p| p.id == identifier)
        .unwrap();
    section.provider_row.set_selected(index);
}

struct Flow<'a> {
    owner: &'a ApplicationDesktop,
    services: &'a ApplicationServices,
    reference: &'a Value,
    root: &'a Path,
    evidence: &'a Path,
    output: PathBuf,
    stages: usize,
    layouts: usize,
}
impl Flow<'_> {
    fn snapshot(&mut self, name: &str) {
        settle_for(Duration::from_millis(250));
        let view = &self.owner.settings.welcome;
        let local = &view.speech.local;
        let workspace = &self.owner.capture.page.workspace;
        let model_requests = fs::read_dir(self.root.join("artifact-server"))
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_str()
                    .unwrap()
                    .starts_with("request-")
            })
            .count();
        let history = self
            .services
            .history
            .recent(100)
            .unwrap()
            .into_iter()
            .rev()
            .map(|e| json!({"raw":e.raw_text,"text":e.delivered_text}))
            .collect::<Vec<_>>();
        let value = json!({"name":name,"page":self.owner.shell.stack.visible_child_name().map(String::from),"step":view.step(),"title":view.title.label().as_str(),"step_label":view.step_label.label().as_str(),"next":{"label":view.next.label().map(String::from),"sensitive":view.next.get_sensitive()},"back":view.back.get_visible(),"notice":view.status.label().as_str(),"speech":view.speech.provider().id,"rewrite":view.rewrite.provider().id,"key_empty":view.speech.api_key_entry.text().is_empty(),"local":{"selected":local.selected(),"device":local.device(),"language":local.languages.language(),"label":local.label.label().as_str(),"ready":local.is_ready(),"status":local.status.label().as_str(),"button_visible":local.button.get_visible(),"button_sensitive":local.button.get_sensitive(),"progress_visible":local.progress.get_visible(),"progress":local.progress.fraction(),"progress_text":local.progress.text().map(String::from)},"appearance":view.appearance.values(),"saved":config(AppConfig::load(&self.services.paths.config.join("config.json")).unwrap()),"active":config(self.services.config()),"live_sensitive":self.owner.capture.page.live_mode.get_sensitive(),"rewrites":[workspace.quick_polish.get_sensitive(),workspace.structured_note.get_sensitive(),workspace.send.get_sensitive()],"history":history,"clipboard":clipboard(),"model_requests":model_requests,"turns":records(&self.evidence.join("requests.jsonl")).iter().filter(|r|r["message"]["method"]=="turn/start").count()});
        write(self.output.join(format!("{name}.json")), &value);
        let expected = &self.reference["stages"][self.stages];
        assert_eq!(
            value.as_object().unwrap().len(),
            expected.as_object().unwrap().len()
        );
        for (key, actual) in value.as_object().unwrap() {
            assert_eq!(actual, &expected[key], "onboarding {name}.{key}");
        }
        self.stages += 1;
        eprintln!("ONBOARDING_STAGE {name} PASS");
    }
    fn layout(&mut self, name: &str) {
        settle_for(Duration::from_millis(250));
        let win = &self.owner.shell.window;
        let path = self.output.join(format!("{name}.png"));
        capture_window(&path);
        let pixels = Command::new("magick")
            .arg(&path)
            .args(["-depth", "8", "rgba:-"])
            .output()
            .unwrap();
        assert!(pixels.status.success());
        let hash =
            glib::compute_checksum_for_data(glib::ChecksumType::Sha256, &pixels.stdout).unwrap();
        let value = json!({"name":name,"window":[win.width(),win.height()],"pixels":hash.as_str()});
        write(self.output.join(format!("{name}-layout.json")), &value);
        let expected = &self.reference["layouts"][self.layouts];
        assert_eq!(value["name"], expected["name"]);
        assert_eq!(value["window"], expected["window"]);
        if let Some(marks) = expected["retired_model_marks"].as_array() {
            let geometry = Command::new("magick")
                .args(["identify", "-format", "%w %h"])
                .arg(&path)
                .output()
                .unwrap();
            assert!(geometry.status.success());
            let size = String::from_utf8(geometry.stdout)
                .unwrap()
                .split_whitespace()
                .map(|v| v.parse::<usize>().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(json!(size), expected["image_size"]);
            let [width, height] = size[..] else {
                panic!("image geometry")
            };
            let mut pixels = pixels.stdout;
            assert_eq!(pixels.len(), width * height * 4);
            for mark in marks {
                let coordinates = mark.as_array().unwrap();
                let [x, y, w, h] = coordinates.as_slice() else {
                    panic!("mark rectangle")
                };
                let (x, y, w, h) = (
                    x.as_u64().unwrap() as usize,
                    y.as_u64().unwrap() as usize,
                    w.as_u64().unwrap() as usize,
                    h.as_u64().unwrap() as usize,
                );
                assert!(w > 0 && h > 0 && x + w <= width && y + h <= height);
                for row in y..y + h {
                    pixels[(row * width + x) * 4..(row * width + x + w) * 4].fill(0);
                }
            }
            let hash =
                glib::compute_checksum_for_data(glib::ChecksumType::Sha256, &pixels).unwrap();
            assert_eq!(
                json!(hash.as_str()),
                expected["pixels_without_retired_model_marks"],
                "onboarding layout outside the two retired model marks: {name}"
            );
        } else {
            assert_eq!(
                value["pixels"], expected["pixels"],
                "onboarding layout {name}"
            );
        }
        self.layouts += 1;
    }
    fn model(&self, identifier: &str) {
        let index = MODEL_CATALOG
            .iter()
            .filter(|m| m.offered)
            .position(|m| m.id == identifier)
            .unwrap();
        self.owner
            .settings
            .welcome
            .speech
            .local
            .scale
            .set_value(index as f64);
    }
    fn language(&mut self, query: &str, tooltip: &str, screen: Option<&str>) {
        self.owner
            .settings
            .welcome
            .speech
            .local
            .languages
            .widget
            .emit_clicked();
        until(|| self.owner.shell.window.visible_dialog().is_some());
        let dialog = self.owner.shell.window.visible_dialog().unwrap();
        let search = widgets(&dialog)
            .into_iter()
            .find_map(|w| w.downcast::<gtk::SearchEntry>().ok())
            .unwrap();
        search.set_text(query);
        settle_for(Duration::from_millis(300));
        if let Some(name) = screen {
            self.layout(name);
        }
        let button = widgets(&dialog)
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::Button>().ok())
            .find(|b| b.tooltip_text().as_deref() == Some(tooltip) && b.is_mapped())
            .unwrap();
        button.emit_clicked();
        until(|| !dialog.is_mapped());
    }
}

pub fn exercise(
    owner: &ApplicationDesktop,
    services: &ApplicationServices,
    reference: &Value,
    tools: &Path,
    root: &Path,
    evidence: &Path,
) -> Vec<u64> {
    let output = root.join("native-application-onboarding");
    fs::create_dir(&output).unwrap();
    let mut flow = Flow {
        owner,
        services,
        reference,
        root,
        evidence,
        output,
        stages: 0,
        layouts: 0,
    };
    let win = &owner.shell.window;
    let view = &owner.settings.welcome;
    let width = reference["params"]["width"].as_i64().unwrap() as i32;
    let height = reference["params"]["height"].as_i64().unwrap() as i32;
    win.set_default_size(width + 10, height + 10);
    win.set_size_request(width + 10, height + 10);
    until(|| win.width() == width && win.height() == height);
    assert!(
        Command::new("xdotool")
            .args(["windowmove", &window_id(), "20", "20"])
            .status()
            .unwrap()
            .success()
    );
    gtk::gdk::Display::default()
        .unwrap()
        .clipboard()
        .set_text("untouched onboarding clipboard");
    until(|| view.widget.is_mapped());
    flow.snapshot("speech");
    flow.layout("speech");
    view.speech
        .api_key_entry
        .set_text("synthetic-onboarding-key");
    view.next.emit_clicked();
    until(|| view.next.get_sensitive() && view.status.label().to_lowercase().contains("keyring"));
    flow.snapshot("key-save-failed");
    write(
        root.join("provider-keyring/keyring.json"),
        &json!({"lookup":{"exit":1},"store":{}}),
    );
    view.speech
        .api_key_entry
        .set_text("synthetic-onboarding-key");
    view.next.emit_clicked();
    until(|| view.step() == 1);
    flow.snapshot("key-saved");
    view.back.emit_clicked();
    until(|| view.step() == 0);
    provider(&view.speech, "local");
    flow.snapshot("local-selected");
    let artifacts = root.join("artifact-server");
    assert!(!artifacts.join("request-0.json").exists());
    view.speech.local.button.emit_clicked();
    wait(|| artifacts.join("request-0.json").exists());
    flow.snapshot("download-held");
    flow.layout("download-held");
    view.next.emit_clicked();
    assert_eq!(view.step(), 0);
    fs::write(artifacts.join("release"), "").unwrap();
    wait(|| artifacts.join("paused-1.json").exists());
    let fraction = reference["stages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "download-progress")
        .unwrap()["local"]["progress"]
        .as_f64()
        .unwrap();
    wait(|| view.speech.local.progress.fraction() >= fraction);
    flow.snapshot("download-progress");
    fs::write(artifacts.join("resume"), "").unwrap();
    wait(|| view.speech.local.is_ready() && !view.speech.local.progress.get_visible());
    flow.snapshot("local-ready");
    flow.layout("local-ready");
    // CPU weights and runtime are real and ready; a missing CUDA runtime must still gate Continue.
    view.speech.local.gpu.set_active(true);
    until(|| !view.next.get_sensitive());
    assert!(!view.speech.is_ready());
    view.next.emit_clicked();
    assert_eq!(view.step(), 0);
    view.speech.local.gpu.set_active(false);
    until(|| view.speech.is_ready() && view.next.get_sensitive());
    flow.model("whisper-tiny");
    flow.language("slk", "Slovenčina", None);
    flow.model("qwen3-1.7b");
    flow.snapshot("unsupported-language");
    flow.language("eng", "English", Some("language-modal"));
    flow.snapshot("language-ready");
    flow.language("deu", "Deutsch", None);
    assert_eq!(view.speech.values()["language_code"], "deu");
    flow.language("eng", "English", None);
    view.next.emit_clicked();
    until(|| view.step() == 1);
    flow.snapshot("rewrite-codex");
    flow.layout("rewrite-codex");
    // Observe actual animation, then prove Skip stops it without reading a private timer.
    let settings = gtk::Settings::default().unwrap();
    settings.set_gtk_enable_animations(true);
    provider(&view.rewrite, "none");
    until(|| !view.polish_preview.widget.is_mapped());
    provider(&view.rewrite, "codex");
    until(|| {
        view.polish_preview.progress.fraction() > 0.0
            && view.polish_preview.progress.fraction() < 1.0
    });
    provider(&view.rewrite, "none");
    until(|| !view.polish_preview.widget.is_mapped());
    let preview = || {
        (
            view.polish_preview.caption.label(),
            view.polish_preview.text.label(),
            view.polish_preview.progress.fraction(),
        )
    };
    let stopped = preview();
    settle_for(Duration::from_millis(300));
    assert_eq!(
        preview(),
        stopped,
        "hidden polishing preview kept animating"
    );
    settings.set_gtk_enable_animations(false);
    flow.snapshot("rewrite-skipped");
    flow.layout("rewrite-skipped");
    view.next.emit_clicked();
    until(|| view.step() == 2);
    view.appearance.lines.set_value(3.0);
    view.appearance.opacity.set_value(40.0);
    view.appearance.position.set_selected(0);
    flow.snapshot("appearance-three");
    flow.layout("appearance-three");
    view.appearance.lines.set_value(5.0);
    view.appearance.opacity.set_value(82.0);
    flow.snapshot("appearance-five");
    flow.layout("appearance-five");
    view.next.emit_clicked();
    until(|| owner.shell.stack.visible_child_name().as_deref() == Some("capture"));
    flow.snapshot("completed");
    until(|| owner.capture.page.record_button.get_sensitive());
    owner.capture.page.record_button.emit_clicked();
    wait(|| {
        owner.capture.phase() == Some(CapturePhase::Recording)
            && tools.join("raw.ready.json").exists()
    });
    let pid = read(tools.join("raw.ready.json"))["pid"].as_u64().unwrap();
    flow.snapshot("recording");
    owner.capture.page.record_button.emit_clicked();
    wait(|| owner.capture.phase().is_none());
    let entry = services.history.recent(1).unwrap().remove(0);
    owner.review.begin(&entry.identifier, "Polish");
    flow.snapshot("recognized");
    flow.layout("recognized");
    win.application().unwrap().activate_action("settings", None);
    until(|| owner.settings.view.widget.is_mapped());
    for page in owner.settings.view.pages() {
        let button = widgets(&owner.settings.view.widget)
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::ToggleButton>().ok())
            .find(|b| b.label().as_deref() == Some(page.title().as_str()))
            .unwrap();
        button.set_active(true);
        assert_eq!(owner.settings.view.visible_page_name(), page.name());
    }
    flow.snapshot("settings-navigation");
    application_commands::choose_action(owner, "Settings · Welcome and provider setup");
    until(|| view.widget.is_mapped());
    flow.snapshot("setup-reopened");
    assert_eq!(flow.stages, reference["stages"].as_array().unwrap().len());
    assert_eq!(flow.layouts, reference["layouts"].as_array().unwrap().len());
    vec![pid]
}
