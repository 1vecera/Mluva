//! App-launched Tensaku, simultaneous narration and exact visual provider context.
use super::{
    accessibility::Accessibility, capture_window, clipboard, http, records, screenshot_wire,
    settle_for, until, widgets, window_id,
};
#[path = "current_codex_profile.rs"]
mod current_codex_profile;
use adw::prelude::*;
use glib::variant::ToVariant;
use mluva_core::history::HistoryInput;
use mluva_gtk::application::ApplicationDesktop;
use mluva_workflows::{capture::CapturePhase, services::ApplicationServices};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::Duration,
};

fn quote(path: &Path) -> String {
    format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"))
}
fn script(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}
fn sha(bytes: &[u8]) -> String {
    glib::compute_checksum_for_data(glib::ChecksumType::Sha256, bytes)
        .unwrap()
        .into()
}
fn unhex(value: &Value) -> Vec<u8> {
    value
        .as_str()
        .unwrap()
        .as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
fn read(path: impl AsRef<Path>) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn alive(pid: Option<u64>) -> bool {
    pid.is_some_and(|pid| {
        fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|text| {
            text.rsplit_once(')').unwrap().1.split_whitespace().next() != Some("Z")
        })
    })
}
fn command(args: &[&str]) -> String {
    let result = Command::new("xdotool").args(args).output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap().trim().into()
}
fn editor_button(label: &str, activate: bool) -> bool {
    // Keep the application's owner loop running while an independent DBus client
    // inspects both applications. A synchronous self-query would block GTK.
    let label = label.to_owned();
    let pending = thread::spawn(move || {
        let api = Accessibility::open();
        api.button(&label).is_some_and(|button| {
            !activate
                || api
                    .call(
                        &button,
                        "org.a11y.atspi.Action",
                        "DoAction",
                        Some(&(0i32,).to_variant()),
                    )
                    .and_then(|v| v.get::<(bool,)>())
                    == Some((true,))
        })
    });
    until(|| pending.is_finished());
    pending.join().unwrap()
}
fn click(label: &str) {
    until(|| editor_button(label, false));
    assert!(editor_button(label, true));
}

pub struct InstalledCodex {
    peer: http::Peer,
    reference: Value,
    root: PathBuf,
}
impl InstalledCodex {
    pub fn prepare(root: &Path, tools: &Path, target: &Path, row: &mut Value) -> Option<Self> {
        if row["name"] != "wide" {
            return None;
        }
        let cli = PathBuf::from(std::env::var_os("MLUVA_TEST_INSTALLED_CODEX")?);
        let reference: Value = serde_json::from_str(include_str!(
            "../fixtures/released-application-images-current-codex.json"
        ))
        .unwrap();
        assert_eq!(
            sha(include_bytes!(
                "../fixtures/released-application-images.json"
            )),
            reference["base"]["sha256"]
        );
        assert_eq!(sha(&fs::read(&cli).unwrap()), reference["sdk"]["sha256"]);
        let version = Command::new(&cli).arg("--version").output().unwrap();
        assert!(version.status.success());
        assert_eq!(
            String::from_utf8(version.stdout).unwrap().trim(),
            reference["sdk"]["version"].as_str().unwrap()
        );
        for patch in reference["layout_patches"].as_array().unwrap() {
            let field = row.pointer_mut(patch["pointer"].as_str().unwrap()).unwrap();
            assert_eq!(*field, patch["old"]);
            *field = patch["value"].clone();
        }
        let stages = row["stages"].as_array_mut().unwrap();
        stages.insert(stages.len() - 1, reference["reopened_state"].clone());
        let responses = reference["response_texts"].as_array().unwrap().iter().map(|text| {
            json!({"route":"codex","status":200,"events":http::codex_response::events(text.as_str().unwrap(),None),
                "request_log":root.join("codex-evidence/requests.jsonl")})
        }).collect::<Vec<_>>();
        let peer = http::Peer::new(&responses);
        current_codex_profile::prepare(root, tools, target, &cli, &peer.address);
        Some(Self {
            peer,
            reference,
            root: root.into(),
        })
    }
    pub fn evidence(&self) -> PathBuf {
        self.root.join("codex-evidence")
    }
    pub fn finish(&mut self, row: &Value) {
        let requests = self.peer.finish();
        assert_eq!(
            requests.len(),
            self.reference["request_settings"].as_array().unwrap().len()
        );
        for (index, wire) in requests.iter().enumerate() {
            assert_eq!(wire["path"], "/responses");
            let request = &wire["json"];
            assert_eq!(
                json!({"model":request["model"],"reasoning":request["reasoning"],"service_tier":request["service_tier"]}),
                self.reference["request_settings"][index]
            );
            let mut input = vec![];
            let mut environments = 0;
            for item in request["input"].as_array().unwrap() {
                if item["type"] != "message" || item["role"] != "user" {
                    continue;
                }
                for part in item["content"].as_array().unwrap() {
                    if let Some(text) = part["text"]
                        .as_str()
                        .filter(|text| text.starts_with("<environment_context>\n"))
                    {
                        let date = text
                            .split_once("<current_date>")
                            .unwrap()
                            .1
                            .split_once("</current_date>")
                            .unwrap()
                            .0;
                        chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap();
                        assert_eq!(
                            text.replacen(date, "$UTC_DATE", 1),
                            self.reference["sdk_environment_context"].as_str().unwrap()
                        );
                        environments += 1;
                    } else {
                        input.push(part.clone());
                    }
                }
            }
            assert_eq!(environments, 1, "one SDK-owned clock context");
            let expected = row["turns"][index]["input"]
                .as_array()
                .unwrap()
                .iter()
                .map(|part| match part["type"].as_str().unwrap() {
                    "text" => json!({"type":"input_text","text":part["text"]}),
                    "image" => json!({"type":"input_image","image_url":part["url"]}),
                    other => panic!("unexpected released image input {other}"),
                })
                .collect::<Vec<_>>();
            let mut input = json!(input);
            screenshot_wire::normalize(&mut input);
            assert_eq!(
                input,
                json!(expected),
                "ordered application text and exact image bytes"
            );
            let instruction = request["instructions"]
                .as_str()
                .or_else(|| {
                    request["input"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|item| item["type"] == "message" && item["role"] == "developer")
                        .flat_map(|item| item["content"].as_array().unwrap())
                        .find_map(|part| {
                            part["text"]
                                .as_str()
                                .filter(|text| text.starts_with("You transform dictated text."))
                        })
                })
                .unwrap();
            assert_eq!(
                instruction,
                self.reference["sdk"]["base_instructions"].as_str().unwrap()
            );
            assert!(request.get("tools").is_none_or(|tools| *tools == json!([])));
            assert!(
                request["input"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|item| !matches!(
                        item["type"].as_str(),
                        Some("additional_tools" | "tool_search_output")
                    ) || item["tools"] == json!([]))
            );
            let encoded = request.to_string();
            assert!(
                !encoded.contains("PRIVATE_INSTRUCTION_CANARY")
                    && !encoded.contains("PRIVATE_OVERRIDE_CANARY")
            );
        }
        assert!(!self.root.join("mcp-started").exists());
        for (file, text) in [
            ("AGENTS.md", "PRIVATE_INSTRUCTION_CANARY"),
            ("AGENTS.override.md", "PRIVATE_OVERRIDE_CANARY"),
        ] {
            assert_eq!(
                fs::read_to_string(self.root.join("home/.codex").join(file)).unwrap(),
                text
            );
        }
        let catalog = records(&self.evidence().join("catalog.jsonl"));
        assert!(
            !catalog.is_empty(),
            "actual CLI catalog snapshot was skipped"
        );
        for child in catalog {
            assert!(!Path::new(&format!("/proc/{}", child["pid"].as_u64().unwrap())).exists());
            assert!(!Path::new(child["cwd"].as_str().unwrap()).exists());
        }
        fs::write(self.root.join("native-application-images/actual-sdk.json"),serde_json::to_vec_pretty(&json!({"sdk":self.reference["sdk"],"requests":requests,"capabilities_empty":true,"canaries_preserved":true,"catalog_children_reaped":true})).unwrap()).unwrap();
    }
}

pub fn prepare(tools: &Path, directory: &Path, target: &Path, reference: &Value) -> PathBuf {
    let editor = PathBuf::from(
        std::env::var_os("MLUVA_TEST_EDITOR").expect("verified released Tensaku required"),
    );
    assert_eq!(sha(&fs::read(&editor).unwrap()), reference["editor_sha256"]);
    let annotation = directory.join("annotation-tools");
    let binaries = directory.join("bin");
    let editor_directory = directory.join("editor");
    for path in [&annotation, &binaries, &editor_directory] {
        fs::create_dir(path).unwrap();
    }
    symlink(editor, editor_directory.join("tensaku")).unwrap();
    symlink(
        target.join("audio-fixture-peer"),
        annotation.join("pw-record"),
    )
    .unwrap();
    fs::write(
        annotation.join("test-config.json"),
        serde_json::to_vec(&reference["pcm"]).unwrap(),
    )
    .unwrap();
    fs::write(
        directory.join("picker.json"),
        serde_json::to_vec(&reference["selection"]).unwrap(),
    )
    .unwrap();
    script(
        &tools.join("omarchy"),
        &format!(
            "#!/bin/sh\nexport MLUVA_SCREENSHOT_FIXTURE_ROOT={}\nexec {} \"$@\"\n",
            quote(directory),
            quote(&target.join("screenshot-picker-fixture-peer"))
        ),
    );
    script(
        &binaries.join("mluva-narrate"),
        &format!(
            "#!/bin/sh\nexport PATH={}:\"$PATH\"\nexec {} \"$@\"\n",
            quote(&annotation),
            quote(&target.join("mluva-narrate"))
        ),
    );
    let component = |path: &Path| {
        path.to_str()
            .unwrap()
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('$', "\\$")
            .replace('`', "\\`")
    };
    let wrapper = binaries.join("mluva-screenshot-editor");
    script(
        &wrapper,
        &include_str!("../../../../linux/resources/mluva-screenshot-editor.in")
            .replace("@EDITOR_DIR@", &component(&editor_directory))
            .replace("@BIN_DIR@", &component(&binaries)),
    );
    let adapter = binaries.join("editor-command");
    script(
        &adapter,
        &format!(
            "#!/bin/sh\nprintf '%s' \"$$\" > {}\nexport XDG_CONFIG_HOME={} XDG_DATA_HOME={} XDG_RUNTIME_DIR={} GSETTINGS_BACKEND=memory\nexec {} \"$@\" > {} 2>&1\n",
            quote(&directory.join("editor.pid")),
            quote(&directory.join("config")),
            quote(&directory.join("data")),
            quote(&directory.join("runtime")),
            quote(&wrapper),
            quote(&directory.join("editor.log"))
        ),
    );
    adapter
}

pub struct Flow<'a> {
    owner: &'a ApplicationDesktop,
    services: &'a ApplicationServices,
    reference: &'a Value,
    tools: &'a Path,
    evidence: &'a Path,
    peer: &'a http::Peer,
    sdk: Option<&'a InstalledCodex>,
    directory: PathBuf,
    spec: PathBuf,
    output: PathBuf,
    ids: BTreeMap<String, String>,
    capture: Option<String>,
    main_pid: Option<u64>,
    annotation_pid: Option<u64>,
    editor_pid: Option<u64>,
    states: Vec<Value>,
    layouts: Vec<Value>,
}
impl<'a> Flow<'a> {
    pub fn new(
        owner: &'a ApplicationDesktop,
        services: &'a ApplicationServices,
        reference: &'a Value,
        tools: &'a Path,
        root: &Path,
        codex: (&'a Path, Option<&'a InstalledCodex>),
        peer: &'a http::Peer,
    ) -> Self {
        let output = root.join("native-application-images");
        fs::create_dir(&output).unwrap();
        Self {
            owner,
            services,
            reference,
            tools,
            evidence: codex.0,
            peer,
            sdk: codex.1,
            spec: root.join("application-codex.json"),
            directory: services
                .paths
                .config
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .into(),
            output,
            ids: BTreeMap::new(),
            capture: None,
            main_pid: None,
            annotation_pid: None,
            editor_pid: None,
            states: vec![],
            layouts: vec![],
        }
    }
    fn identity(&mut self, id: &str) -> String {
        if self.capture.as_deref() == Some(id) {
            return "capture".into();
        }
        let next = format!("entry-{}", self.ids.len() + 1);
        self.ids.entry(id.into()).or_insert(next).clone()
    }
    fn turns(&self) -> usize {
        if let Some(sdk) = self.sdk {
            return sdk.peer.observed.lock().unwrap().len();
        }
        records(&self.evidence.join("requests.jsonl"))
            .iter()
            .filter(|r| r["message"]["method"] == "turn/start")
            .count()
    }
    fn snapshot(&mut self, name: &str) {
        settle_for(Duration::from_millis(150));
        let entries = self.services.history.recent(100).unwrap();
        let history=entries.iter().rev().map(|e|json!({"id":self.identity(&e.identifier),"raw":e.raw_text,"source":self.services.conversations.source_text(e,false).unwrap(),"output":e.delivered_text,"replies":self.services.conversations.replies(&e.identifier).unwrap().iter().map(|r|r.text.clone()).collect::<Vec<_>>(),"context":e.enhancement_context_sources})).collect::<Vec<_>>();
        let mut images = vec![];
        let owners = entries.iter().map(|e| (e.identifier.clone(), false)).chain(
            self.services
                .screenshots
                .pending_captures()
                .unwrap()
                .into_iter()
                .map(|id| (id, true)),
        );
        for (id, capture) in owners {
            for image in self.services.screenshots.recent(&id, capture).unwrap() {
                if let Some(offset) = image.captured_after_seconds {
                    assert!((0.0..8.0).contains(&offset));
                }
                images.push(json!({"owner":self.identity(&id),"capture":capture,"offset":image.captured_after_seconds.map(|_|"sampled"),"png":sha(&fs::read(image.path).unwrap())}));
            }
        }
        images.sort_by(|a, b| {
            (a["owner"].as_str(), a["png"].as_str()).cmp(&(b["owner"].as_str(), b["png"].as_str()))
        });
        let w = &self.owner.capture.page.workspace;
        let shelf = &w.screenshot_shelf.widget;
        let all = widgets(shelf);
        let pictures = all
            .iter()
            .filter_map(|w| w.downcast_ref::<gtk::Picture>())
            .map(|p| {
                sha(p
                    .paintable()
                    .unwrap()
                    .downcast::<gtk::gdk::Texture>()
                    .unwrap()
                    .save_to_png_bytes()
                    .as_ref())
            })
            .collect::<Vec<_>>();
        let labels = all
            .iter()
            .filter_map(|w| w.downcast_ref::<gtk::Label>())
            .map(|l| l.label().to_string())
            .collect::<Vec<_>>();
        let entry = w.entry().map(|e| self.identity(&e.identifier));
        let mut value = json!({"name":name,"recording":self.owner.capture.phase()==Some(CapturePhase::Recording),"processing":self.owner.capture.phase()==Some(CapturePhase::Processing),"entry":entry,"documents":w.documents().iter().map(|w|w.text()).collect::<Vec<_>>(),"history":history,"images":images,"shelf":{"visible":shelf.get_visible(),"labels":labels,"pictures":pictures},"clipboard":clipboard(),"http":self.peer.observed.lock().unwrap().len(),"turns":self.turns(),"main_alive":alive(self.main_pid),"annotation_alive":alive(self.annotation_pid),"editor_alive":alive(self.editor_pid)});
        if self.reference["name"] == "preparation-failure" && name != "closed" {
            value["status"] = json!(self.owner.capture.page.status.label().to_string());
        }
        if name == "closed" {
            for key in ["recording", "processing", "entry", "documents", "shelf"] {
                value.as_object_mut().unwrap().remove(key);
            }
        }
        fs::write(
            self.output.join(format!("{name}.json")),
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();
        for (key, actual) in value.as_object().unwrap() {
            assert_eq!(
                actual,
                &self.reference["stages"][self.states.len()][key],
                "image workspace {name}.{key}"
            );
        }
        self.states.push(value);
        eprintln!("IMAGE_STAGE {name} PASS");
    }
    fn layout(&mut self, name: &str) {
        let win = &self.owner.shell.window;
        let workspace = &self.owner.capture.page.workspace;
        let width = self.reference["params"]["width"].as_i64().unwrap() as i32;
        let height = self.reference["params"]["height"].as_i64().unwrap() as i32;
        // The requested disclosure has its own persistence owner. Preserve the
        // complete released viewport here by expanding it and giving its 52px
        // row additional space; retain both the raw frame and exact projection.
        workspace.rewrite_toggle.set_active(true);
        win.set_size_request(width + 10, height + 62);
        win.set_default_size(width + 10, height + 62);
        until(|| win.width() == width && win.height() == height + 52);
        until(|| {
            !widgets(win)
                .iter()
                .any(|w| w.is_mapped() && w.accessible_role() == gtk::AccessibleRole::Alert)
        });
        settle_for(Duration::from_millis(250));
        let shelf = &self.owner.capture.page.workspace.screenshot_shelf.widget;
        let rect = |w: &gtk::Widget| {
            let r = w.compute_bounds(win).unwrap();
            [r.x(), r.y(), r.width(), r.height()]
        };
        let mut value = json!({"name":name,"window":[width,height],"shelf":rect(shelf.upcast_ref()),"controls":widgets(shelf).into_iter().filter_map(|w|w.downcast::<gtk::Button>().ok()).filter(|b|b.is_mapped()).map(|b|json!({"tip":b.tooltip_text().map(String::from),"sensitive":b.get_sensitive(),"bounds":rect(b.upcast_ref())})).collect::<Vec<_>>()});
        let image = self.output.join(format!("{name}.png"));
        capture_window(&image);
        let pixels = Command::new("magick")
            .arg(&image)
            .args(["-depth", "8", "rgba:-"])
            .output()
            .unwrap();
        assert!(pixels.status.success());
        let bounds = workspace.rewrite_toggle.compute_bounds(win).unwrap();
        assert_eq!(bounds.height(), 36.0);
        assert!(workspace.prompt.is_mapped());
        let stride = (width as usize + 10) * 4;
        assert_eq!(pixels.stdout.len(), stride * (height as usize + 62));
        // compute_bounds omits the 5px CSD inset present in import's raw frame.
        let start = (bounds.y() as usize) + 5 - 8;
        assert!(start > 42 && start + 52 < height as usize + 62);
        let mut projected = pixels.stdout[..start * stride].to_vec();
        projected.extend_from_slice(&pixels.stdout[(start + 52) * stride..]);
        assert_eq!(projected.len(), stride * (height as usize + 10));
        value["pixels"] = json!(sha(&projected));
        fs::write(
            self.output.join(format!("{name}-projection.json")),
            serde_json::to_vec_pretty(&json!({"raw_frame":image,"raw_size":[width+10,height+62],
                "raw_rgba_sha256":sha(&pixels.stdout),"disclosure_bounds":rect(workspace.rewrite_toggle.upcast_ref()),
                "removed_rows":[start,start+52],"released_size":[width+10,height+10],"projected_rgba_sha256":sha(&projected)})).unwrap(),
        ).unwrap();
        assert_eq!(
            value,
            self.reference["layouts"][self.layouts.len()],
            "image workspace layout {name}"
        );
        self.layouts.push(value);
        win.set_size_request(width + 10, height + 10);
        win.set_default_size(width + 10, height + 10);
        command(&[
            "windowsize",
            "--sync",
            &window_id(),
            &(width + 10).to_string(),
            &(height + 10).to_string(),
        ]);
        until(|| win.width() == width && win.height() == height);
    }
    pub fn exercise(&mut self) {
        let win = &self.owner.shell.window;
        let w = &self.owner.capture.page.workspace;
        let width = self.reference["params"]["width"].as_i64().unwrap() as i32;
        let height = self.reference["params"]["height"].as_i64().unwrap() as i32;
        win.set_default_size(width + 10, height + 10);
        win.set_size_request(width + 10, height + 10);
        until(|| win.width() == width && win.height() == height);
        command(&["windowmove", &window_id(), "20", "20"]);
        if self.reference["name"] == "preparation-failure" {
            let mut spec = read(&self.spec);
            spec["catalog"] = json!([]);
            spec["model_gate"] = json!(self.directory.join("model-release"));
            fs::write(&self.spec, serde_json::to_vec(&spec).unwrap()).unwrap();
        }
        self.owner.settings.capture.cleanup.set_active(true);
        gtk::gdk::Display::default()
            .unwrap()
            .clipboard()
            .set_text("untouched image workspace clipboard");
        if self.reference["name"] == "preparation-failure" {
            let app = win.application().unwrap();
            app.activate_action("record", None);
            until(|| {
                self.owner.capture.phase() == Some(CapturePhase::Preparing)
                    && records(&self.evidence.join("requests.jsonl"))
                        .iter()
                        .any(|r| r["message"]["method"] == "model/list")
            });
            self.capture = self.owner.capture.session_identifier();
            self.snapshot("preparation-held");
            app.activate_action("screenshot", None);
            until(|| self.directory.join("ready.json").exists());
            fs::write(self.directory.join("release"), b"").unwrap();
            until(|| self.directory.join("editor.pid").exists());
            self.editor_pid = Some(
                fs::read_to_string(self.directory.join("editor.pid"))
                    .unwrap()
                    .parse()
                    .unwrap(),
            );
            let shot = self
                .services
                .screenshots
                .recent(self.capture.as_deref().unwrap(), true)
                .unwrap()
                .remove(0);
            self.snapshot("image-before-failure");
            fs::write(self.directory.join("model-release"), b"").unwrap();
            until(|| self.owner.capture.phase().is_none());
            self.snapshot("preparation-failed");
            assert!(
                self.services
                    .screenshots
                    .pending_captures()
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(self.services.history.recent(100).unwrap().len(), 1);
            assert!(shot.path.exists());
            assert!(!self.tools.join("raw.ready.json").exists());
            return;
        }
        let mut entries = vec![];
        for (index, text) in ["Earlier narration.", "Other conversation."]
            .into_iter()
            .enumerate()
        {
            let entry = self
                .services
                .history
                .add(HistoryInput {
                    language_code: "eng".into(),
                    delivery_outcome: "saved".into(),
                    ..HistoryInput::dictation(text, text)
                })
                .unwrap();
            let id = format!("00000000-0000-4000-8000-{:012}", index + 1);
            rusqlite::Connection::open(&self.services.history.database.path)
                .unwrap()
                .execute(
                    "UPDATE transcription_history SET identifier=?,created_at=? WHERE identifier=?",
                    rusqlite::params![id, "2026-03-17T13:40:00+00:00", entry.identifier],
                )
                .unwrap();
            self.ids.insert(
                id.clone(),
                if index == 0 { "parent" } else { "other" }.into(),
            );
            entries.push(self.services.history.find(&id).unwrap());
        }
        self.services
            .screenshots
            .add(
                &entries[0].identifier,
                &unhex(&self.reference["blue"]),
                false,
                None,
            )
            .unwrap();
        w.refresh_history().unwrap();
        w.show_conversation(Some(entries[0].clone()), &[], false)
            .unwrap();
        if self.reference["name"] == "wide" {
            w.continue_button.emit_clicked();
            assert!(
                self.owner.capture.phase().is_some(),
                "Continue did not start: ready={}, entry={:?}, status={}",
                self.owner.capture.page.record_button.get_sensitive(),
                w.entry().map(|entry| entry.identifier),
                self.owner.capture.page.status.text()
            );
            until(|| {
                (self.owner.capture.phase() == Some(CapturePhase::Recording)
                    && self.tools.join("raw.ready.json").exists())
                    || self.owner.capture.phase().is_none()
            });
            assert_eq!(
                self.owner.capture.phase(),
                Some(CapturePhase::Recording),
                "Continue failed: {}",
                self.owner.capture.page.status.text()
            );
            self.capture = self.owner.capture.session_identifier();
            self.main_pid = read(self.tools.join("raw.ready.json"))["pid"].as_u64();
            self.snapshot("recording");
            let target = PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap()).join("debug");
            let mut bridge = Command::new(target.join("mluva-shell"))
                .arg("screenshot")
                .spawn()
                .unwrap();
            until(|| bridge.try_wait().unwrap().is_some());
            assert!(bridge.wait().unwrap().success());
            until(|| self.directory.join("ready.json").exists());
            w.show_conversation(Some(entries[1].clone()), &[], true)
                .unwrap();
            self.snapshot("picker-other-conversation");
            fs::write(self.directory.join("release"), b"").unwrap();
            until(|| self.directory.join("editor.pid").exists());
            self.editor_pid = Some(
                fs::read_to_string(self.directory.join("editor.pid"))
                    .unwrap()
                    .parse()
                    .unwrap(),
            );
            let shot = self
                .services
                .screenshots
                .recent(self.capture.as_deref().unwrap(), true)
                .unwrap()
                .remove(0);
            click("Save and continue");
            until(|| !editor_button("Save and continue", false));
            self.snapshot("editor-first-use");
            until(|| editor_button("Add narration", false));
            assert_eq!(
                fs::read_link(format!("/proc/{}/exe", self.editor_pid.unwrap())).unwrap(),
                PathBuf::from(std::env::var_os("MLUVA_TEST_EDITOR").unwrap())
                    .canonicalize()
                    .unwrap()
            );
            let editor_window = command(&[
                "search",
                "--onlyvisible",
                "--pid",
                &self.editor_pid.unwrap().to_string(),
            ])
            .lines()
            .last()
            .unwrap()
            .to_owned();
            command(&[
                "windowmove",
                &editor_window,
                "20",
                "20",
                "windowsize",
                &editor_window,
                "1200",
                "800",
                "windowactivate",
                "--sync",
                &editor_window,
            ]);
            settle_for(Duration::from_millis(300));
            click("Add narration");
            until(|| editor_button("Choose text area", false));
            command(&[
                "mousemove",
                "240",
                "360",
                "mousedown",
                "1",
                "mousemove",
                "--sync",
                "1000",
                "490",
                "mouseup",
                "1",
            ]);
            let ready = self.directory.join("annotation-tools/raw.ready.json");
            until(|| ready.exists());
            self.annotation_pid = read(&ready)["pid"].as_u64();
            self.snapshot("simultaneous-narration");
            let before = fs::read(&shot.path).unwrap();
            click("Stop narration");
            until(|| fs::read(&shot.path).unwrap() != before);
            until(|| editor_button("Add narration", false));
            until(|| !alive(self.annotation_pid));
            let annotated = fs::read(&shot.path).unwrap();
            assert_eq!(
                annotated,
                unhex(&self.reference["annotated"]),
                "real Tensaku annotated PNG bytes"
            );
            fs::write(self.output.join("annotated.png"), &annotated).unwrap();
            let ocr = Command::new("tesseract")
                .arg(&shot.path)
                .args(["stdout", "--psm", "11"])
                .output()
                .unwrap();
            assert!(ocr.status.success());
            assert_eq!(
                String::from_utf8(ocr.stdout).unwrap().trim(),
                self.reference["ocr"].as_str().unwrap()
            );
            self.snapshot("annotation-saved");
            self.owner.capture.page.record_button.emit_clicked();
            until(|| self.owner.capture.phase().is_none());
            self.snapshot("capture-completed");
            let parent = self.services.history.find(&entries[0].identifier).unwrap();
            w.show_conversation(
                Some(parent.clone()),
                &self
                    .services
                    .conversations
                    .replies(&parent.identifier)
                    .unwrap(),
                false,
            )
            .unwrap();
            command(&["windowactivate", "--sync", &window_id()]);
            self.snapshot("frozen-owner");
            self.layout("populated");
            w.prompt
                .buffer()
                .set_text("Use these screenshots to explain the visible controls.");
            w.send.emit_clicked();
            until(|| {
                !self.owner.review.rewriting()
                    && self
                        .services
                        .conversations
                        .replies(&parent.identifier)
                        .unwrap()
                        .len()
                        == 1
            });
            self.snapshot("follow-up");
            if self.sdk.is_some() {
                // Reopen through ordinary navigation so measured first-text
                // timings cannot define the saved-view image oracle.
                w.show_conversation(Some(entries[1].clone()), &[], false)
                    .unwrap();
                w.show_conversation(
                    Some(parent.clone()),
                    &self
                        .services
                        .conversations
                        .replies(&parent.identifier)
                        .unwrap(),
                    false,
                )
                .unwrap();
                self.snapshot("reopened");
            }
            self.layout("rewritten");
        } else {
            self.services
                .screenshots
                .add(
                    &entries[0].identifier,
                    &unhex(&self.reference["annotated"]),
                    false,
                    None,
                )
                .unwrap();
            w.show_conversation(Some(entries[0].clone()), &[], false)
                .unwrap();
            self.snapshot("populated");
            self.layout("populated");
        }
    }
    pub fn audio_pids(&self) -> Vec<u64> {
        self.main_pid
            .into_iter()
            .chain(self.annotation_pid)
            .collect()
    }
    pub fn closed(&mut self) {
        self.snapshot("closed");
        assert_eq!(json!(self.states), self.reference["stages"]);
        assert_eq!(json!(self.layouts), self.reference["layouts"]);
        if self.editor_pid.is_some() {
            assert!(
                alive(self.editor_pid),
                "released app leaves external editors open"
            );
        }
        eprintln!(
            "Image workspace {}: {} states and {} layouts",
            self.reference["name"],
            self.states.len(),
            self.layouts.len()
        );
    }
}
impl Drop for Flow<'_> {
    fn drop(&mut self) {
        if let Some(pid) = self.editor_pid {
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
            until(|| !alive(Some(pid)));
            unsafe {
                libc::waitpid(pid as i32, std::ptr::null_mut(), 0);
            }
        }
    }
}
