//! Continued capture through the actual application, saved records and private clipboard.
use super::{Signals, capture_window, clipboard, records, settle, settle_for, until, widgets};
use adw::prelude::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use mluva_core::{config::AppConfig, history::HistoryInput};
use mluva_gtk::application::ApplicationDesktop;
use mluva_workflows::{capture::CapturePhase, services::ApplicationServices};
use serde_json::{Value, json};
use std::{
    cell::RefCell, collections::BTreeMap, fs, path::Path, process::Command, rc::Rc, time::Duration,
};

fn identity(ids: &mut BTreeMap<String, String>, id: &str) -> String {
    let next = format!("entry-{}", ids.len() + 1);
    ids.entry(id.to_owned()).or_insert(next).clone()
}
fn preferences(config: &AppConfig) -> Value {
    json!({"enabled":config.live_rewrite_enabled,"continuous":config.live_rewrite_continuous,"model":config.rewrite_model,"effort":config.rewrite_reasoning_effort})
}
fn select(row: &adw::ComboRow, label: &str) {
    let model = row.model().unwrap();
    let selected = (0..model.n_items())
        .find(|index| {
            model
                .item(*index)
                .unwrap()
                .downcast::<gtk::StringObject>()
                .unwrap()
                .string()
                == label
        })
        .unwrap_or_else(|| panic!("choice {label} is unavailable"));
    row.set_selected(selected);
}

pub fn requests(mut requests: Vec<Value>) -> Value {
    for request in &mut requests {
        if let Some(encoded) = request["fields"]["file"].as_str() {
            let wav = STANDARD.decode(encoded).unwrap();
            let checksum =
                glib::compute_checksum_for_data(glib::ChecksumType::Sha256, &wav).unwrap();
            request["fields"]["file"] = json!({"bytes":wav.len(),"sha256":checksum.as_str()});
        }
    }
    json!(requests)
}
pub fn turns(evidence: &Path) -> Value {
    let mut model = Value::Null;
    let mut turns = vec![];
    for record in records(&evidence.join("requests.jsonl")) {
        let message = &record["message"];
        if message["method"] == "thread/start" {
            model = message["params"]["model"].clone();
        }
        if message["method"] == "turn/start" {
            turns.push(json!({"input":message["params"]["input"],"model":model,"effort":message["params"]["effort"]}));
        }
    }
    json!(turns)
}

struct Flow<'a> {
    owner: &'a ApplicationDesktop,
    services: &'a ApplicationServices,
    application: &'a gtk::Application,
    signals: &'a Signals,
    reference: &'a Value,
    root: &'a Path,
    tools: &'a Path,
    ids: BTreeMap<String, String>,
    states: Vec<Value>,
    layouts: Vec<Value>,
    pids: Vec<u64>,
}
impl Flow<'_> {
    fn record(&mut self, name: &str) {
        let workspace = &self.owner.capture.page.workspace;
        let mut entries = self.services.history.recent(100).unwrap();
        entries.reverse();
        let history = entries.iter().map(|entry| {
            json!({"id":identity(&mut self.ids,&entry.identifier),"raw":entry.raw_text,"source":self.services.conversations.source_text(entry,false).unwrap(),"replies":self.services.conversations.replies(&entry.identifier).unwrap().iter().map(|reply|reply.text.clone()).collect::<Vec<_>>()})
        }).collect::<Vec<_>>();
        let history_rows = widgets(&self.owner.history.page.widget)
            .into_iter()
            .filter_map(|widget| {
                let (title, subtitle) = if let Some(row) = widget.downcast_ref::<adw::ActionRow>() {
                    (row.title(), row.subtitle().unwrap_or_default())
                } else if let Some(row) = widget.downcast_ref::<adw::ExpanderRow>() {
                    (row.title(), row.subtitle())
                } else {
                    return None;
                };
                let selected = ["Current text", "Raw transcript", "Original recordings"]
                    .contains(&title.as_str())
                    || subtitle.starts_with("Dictation");
                // Capture creates these timestamps; outcome and all user text stay exact.
                let subtitle = if subtitle.starts_with("Dictation · ")
                    && subtitle.matches(" · ").count() >= 3
                {
                    format!("{} · $CREATED", subtitle.rsplitn(3, " · ").last().unwrap())
                } else {
                    subtitle.to_string()
                };
                selected.then(|| json!({"title":title.as_str(),"subtitle":subtitle}))
            })
            .collect::<Vec<_>>();
        let entry = workspace
            .entry()
            .map(|entry| identity(&mut self.ids, &entry.identifier));
        let conversations = self
            .services
            .conversations
            .search("", 100, None)
            .unwrap()
            .iter()
            .map(|entry| identity(&mut self.ids, &entry.identifier))
            .collect::<Vec<_>>();
        let phase = self.owner.capture.phase().map(|phase| match phase {
            CapturePhase::Preparing => "preparing",
            CapturePhase::Recording => "recording",
            CapturePhase::Processing => "processing",
            other => panic!("unexpected observed capture phase {other:?}"),
        });
        let live = &self.owner.capture.page.live_mode;
        let mut value = json!({
            "name":name,"page":self.owner.shell.stack.visible_child_name().map(String::from),"phase":phase,
            "source":workspace.live_text.text(),"draft":workspace.live_draft(),"viewing_live":workspace.is_viewing_live(),
            "draft_toggle_sensitive":workspace.draft_toggle.get_sensitive(),"draft_visible":workspace.live_draft_box.get_visible(),
            "entry":entry,"documents":workspace.documents().iter().map(|view|view.text()).collect::<Vec<_>>(),"notice":workspace.notice.label().as_str(),
            "preferences":preferences(&self.services.config()),"persisted":preferences(&AppConfig::load(&self.services.paths.config.join("config.json")).unwrap()),
            "live_control":{"label":live.label().map(String::from),"active":live.is_active(),"sensitive":live.get_sensitive()},
            "continue_control":{"visible":workspace.continue_button.get_visible(),"sensitive":workspace.continue_button.get_sensitive()},
            "history":history,"conversations":conversations,"history_rows":history_rows,"clipboard":clipboard()
        });
        value["widget"] = json!(phase.map(|phase| {
            let before = self.signals.borrow().len();
            self.application.activate_action("status", None);
            until(|| self.signals.borrow().len() >= before + 2);
            let observed = || {
                self.signals
                    .borrow()
                    .iter()
                    .rev()
                    .find_map(|(name, value)| {
                        if name != "ShellStateChanged" {
                            return None;
                        }
                        let shell = value
                            .child_value(0)
                            .get::<BTreeMap<String, glib::Variant>>()
                            .unwrap();
                        (shell["phase"].str() == Some(phase)).then(|| {
                            json!({
                                "preview":shell["preview"].str().unwrap(),
                                "delivery":self.signals.borrow().iter().rev().find(|(name,value)|name=="StateChanged" && value.child_value(1).str()==Some(phase)).unwrap().1.child_value(8).str().unwrap()
                            })
                        })
                    })
            };
            until(|| observed().is_some());
            observed().unwrap()
        }));
        fs::write(
            self.root.join(format!("{name}.json")),
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();
        assert_eq!(
            value,
            self.reference["stages"][self.states.len()],
            "continuation stage {name}"
        );
        self.states.push(value);
        println!("CONTINUATION_STAGE {name} PASS");
    }
    fn audio(&self, repeats: usize) {
        let _ = fs::remove_file(self.tools.join("raw.ready.json"));
        let mut pcm = self.reference["pcm"].clone();
        pcm["pcm_hex"] = json!(pcm["pcm_hex"].as_str().unwrap().repeat(repeats));
        fs::write(
            self.tools.join("test-config.json"),
            serde_json::to_vec(&pcm).unwrap(),
        )
        .unwrap();
    }
    fn recording(&mut self) {
        until(|| {
            self.owner.capture.phase() == Some(CapturePhase::Recording)
                && self.tools.join("raw.ready.json").exists()
        });
        let ready: Value =
            serde_json::from_slice(&fs::read(self.tools.join("raw.ready.json")).unwrap()).unwrap();
        self.pids.push(ready["pid"].as_u64().unwrap());
        settle();
    }
    fn stop(&mut self, name: &str) {
        self.owner.capture.page.record_button.emit_clicked();
        if name == "continuation-saved" {
            self.record("continuation-processing");
        }
        until(|| self.owner.capture.phase().is_none() && !self.owner.live.active());
        settle();
        self.record(name);
    }
    fn cancel(&mut self, name: &str) {
        self.application.activate_action("cancel", None);
        until(|| self.owner.capture.phase().is_none());
        settle();
        self.record(name);
    }
    fn review_continue(&self, identifier: &str) {
        self.application
            .activate_action("review", Some(&("continue", identifier, "").to_variant()));
    }
    fn layout(&mut self, name: &str) {
        // The shared runner's earlier workflows can leave the private pointer
        // over a control; match the released continuation observer's idle pointer.
        assert!(
            Command::new("xdotool")
                .args(["mousemove", "0", "0"])
                .status()
                .unwrap()
                .success()
        );
        // Match the released observer after popup CSS transitions and resize allocation.
        settle_for(Duration::from_millis(300));
        let window = &self.owner.shell.window;
        let workspace = &self.owner.capture.page.workspace;
        let controls=[workspace.continue_button.clone().upcast::<gtk::Widget>(),self.owner.capture.page.live_mode.clone().upcast(),self.owner.capture.page.record_button.clone().upcast()].iter().map(|widget|{
            let bounds=widget.compute_bounds(window).unwrap();
            json!({"bounds":[bounds.x(),bounds.y(),bounds.width(),bounds.height()],"visible":widget.get_visible(),"sensitive":widget.get_sensitive()})
        }).collect::<Vec<_>>();
        let value =
            json!({"name":name,"window":[window.width(),window.height()],"controls":controls});
        fs::write(
            self.root.join(format!("{name}-layout.json")),
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();
        assert_eq!(
            value,
            self.reference["layouts"][self.layouts.len()],
            "continuation layout {name}"
        );
        self.layouts.push(value);
        capture_window(&self.root.join(format!("{name}.png")));
    }
}

pub fn exercise(
    owner: &ApplicationDesktop,
    services: &ApplicationServices,
    reference: &Value,
    tools: &Path,
    root: &Path,
    evidence: &Path,
    signals: &Signals,
) -> Vec<u64> {
    let root = root.join("native-application-continuation");
    fs::create_dir(&root).unwrap();
    gtk::gdk::Display::default()
        .unwrap()
        .clipboard()
        .set_text("untouched continuation clipboard");
    let application = owner.shell.window.application().unwrap();
    let mut flow = Flow {
        owner,
        services,
        application: &application,
        signals,
        reference,
        root: &root,
        tools,
        ids: BTreeMap::new(),
        states: vec![],
        layouts: vec![],
        pids: vec![],
    };
    let workspace = &owner.capture.page.workspace;
    let source = services
        .history
        .add(HistoryInput {
            delivery_outcome: "copied".into(),
            ..HistoryInput::dictation("First raw.", "First.")
        })
        .unwrap();
    identity(&mut flow.ids, &source.identifier);
    let reply = services
        .conversations
        .append(&source.identifier, "Polish", "First polished.", "fixture")
        .unwrap();
    workspace
        .show_conversation(Some(source.clone()), &[reply], false)
        .unwrap();
    settle();
    workspace.documents()[0]
        .buffer()
        .set_text("My edited original.");
    workspace
        .documents()
        .last()
        .unwrap()
        .buffer()
        .set_text("My edited rewrite.");
    flow.record("edited-parent");
    let updates = Rc::new(RefCell::new(Vec::<String>::new()));
    let received = updates.clone();
    let source_signal = workspace.live_text.buffer().connect_changed(move |buffer| {
        received.borrow_mut().push(
            buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), true)
                .to_string(),
        )
    });
    flow.audio(1);
    workspace.continue_button.emit_clicked();
    flow.record("continuation-preparing");
    flow.recording();
    flow.record("continuation-recording");
    flow.stop("continuation-saved");
    application.activate_action("history", None);
    flow.record("continued-history");
    let current = widgets(&owner.history.page.widget)
        .into_iter()
        .filter_map(|widget| widget.downcast::<adw::ActionRow>().ok())
        .find(|row| row.title() == "Current text")
        .unwrap();
    widgets(&current)
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
        .find(|button| button.label().as_deref() == Some("Copy"))
        .unwrap()
        .emit_clicked();
    flow.record("history-copy");
    let segment = services
        .history
        .continuations(&source.identifier)
        .unwrap()
        .remove(0);
    owner
        .history
        .page
        .focus_entry(Some(&segment.identifier))
        .unwrap();
    assert_eq!(
        owner.history.page.focused_identifier().as_deref(),
        Some(source.identifier.as_str())
    );
    widgets(&owner.history.page.widget)
        .into_iter()
        .filter_map(|widget| widget.downcast::<adw::ExpanderRow>().ok())
        .find(|row| row.title() == "Original recordings")
        .unwrap()
        .set_expanded(true);
    flow.record("history-originals");
    application.activate_action("latest", None);
    let live = &owner.capture.page.live_mode;
    live.set_active(true);
    flow.record("once-selected");
    flow.audio(30);
    flow.review_continue(&source.identifier);
    flow.recording();
    flow.record("once-recording");
    until(|| {
        workspace
            .live_text
            .text()
            .ends_with("More words.\n\nMore words.")
    });
    until(|| {
        workspace.live_draft_status.label() == "Live draft · provisional until Stop"
            && !turns(evidence).as_array().unwrap().is_empty()
    });
    flow.record("once-preview");
    flow.stop("once-saved");
    workspace.live_text.buffer().disconnect(source_signal);
    let mut revisions = vec![];
    for value in updates.borrow().iter().filter(|text| !text.is_empty()) {
        if revisions.last() != Some(value) {
            revisions.push(value.clone());
        }
    }
    assert_eq!(
        json!(revisions),
        reference["source_revisions"],
        "every nonempty source revision retains its parent"
    );
    live.set_active(true);
    live.set_active(false);
    flow.record("continuous-selected");
    flow.audio(1);
    workspace.continue_button.emit_clicked();
    flow.recording();
    flow.record("continuous-recording");
    flow.cancel("continuous-cancelled");
    live.set_active(false);
    flow.record("live-off");
    let picker = &owner.capture.page.rewrite_settings;
    picker.widget.popup();
    until(|| picker.model_row.is_sensitive());
    select(&picker.model_row, "Fixture");
    select(&picker.thinking_row.widget, "High");
    flow.record("thinking-high");
    select(&picker.model_row, "Second");
    flow.record("thinking-reset");
    let config_path = services.paths.config.join("config.json");
    let saved = fs::read(&config_path).unwrap();
    fs::remove_file(&config_path).unwrap();
    fs::create_dir(&config_path).unwrap();
    select(&picker.thinking_row.widget, "Low");
    assert_eq!(picker.thinking_row.widget.selected(), 0);
    fs::remove_dir(&config_path).unwrap();
    fs::write(&config_path, saved).unwrap();
    flow.record("thinking-save-failed");
    picker.widget.popdown();
    for (width, height, name) in [(1060, 780, "wide"), (420, 520, "minimum")] {
        owner.shell.window.set_default_size(width, height);
        flow.layout(name);
    }
    owner.shell.window.set_default_size(1060, 780);
    settle();
    flow.audio(1);
    owner.capture.page.record_button.emit_clicked();
    flow.recording();
    flow.stop("plain-saved");
    let plain = workspace.entry().unwrap();
    for index in 1..=2 {
        workspace
            .show_conversation(
                Some(services.history.find(&source.identifier).unwrap()),
                &services.conversations.replies(&source.identifier).unwrap(),
                false,
            )
            .unwrap();
        settle();
        flow.audio(1);
        flow.review_continue(&plain.identifier);
        flow.record(&format!("widget-{index}-preparing"));
        flow.recording();
        flow.stop(&format!("widget-{index}-saved"));
    }
    flow.audio(1);
    workspace.continue_button.emit_clicked();
    flow.cancel("preparing-cancelled");
    live.set_active(true);
    flow.audio(1);
    workspace.continue_button.emit_clicked();
    flow.recording();
    flow.stop("silent-continuation");
    flow.audio(1);
    owner.capture.page.record_button.emit_clicked();
    flow.recording();
    flow.record("fresh-recording");
    flow.cancel("fresh-cancelled");
    assert_eq!(
        flow.states.len(),
        reference["stages"].as_array().unwrap().len()
    );
    assert_eq!(
        flow.layouts.len(),
        reference["layouts"].as_array().unwrap().len()
    );
    fs::write(
        root.join("receipt.json"),
        serde_json::to_vec_pretty(
            &json!({"states":flow.states,"layouts":flow.layouts,"source_revisions":revisions}),
        )
        .unwrap(),
    )
    .unwrap();
    flow.pids
}
