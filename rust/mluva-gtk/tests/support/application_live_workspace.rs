//! Live root lifecycle with actual PCM/HTTP/Codex, navigation and durable output.
use super::{application_commands, capture_window, clipboard, records, settle_for, until, widgets};
use adw::prelude::*;
use mluva_core::{config::AppConfig, history::HistoryInput};
use mluva_gtk::application::ApplicationDesktop;
use mluva_workflows::{capture::CapturePhase, services::ApplicationServices};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path, process::Command, time::Duration};

fn settle() {
    settle_for(Duration::from_millis(300));
}
fn identity(ids: &mut BTreeMap<String, String>, id: &str) -> String {
    let next = format!("entry-{}", ids.len() + 1);
    ids.entry(id.into()).or_insert(next).clone()
}
fn preferences(c: &AppConfig) -> Value {
    json!({"enabled":c.live_rewrite_enabled,"continuous":c.live_rewrite_continuous,"template":c.live_rewrite_template,"sidebar":c.history_sidebar_visible,"time_format":c.time_format,"widget_position":c.widget_position,"provider":c.rewrite_provider})
}
struct Flow<'a> {
    owner: &'a ApplicationDesktop,
    services: &'a ApplicationServices,
    reference: &'a Value,
    tools: &'a Path,
    root: &'a Path,
    evidence: &'a Path,
    spec: &'a Path,
    ids: BTreeMap<String, String>,
    count: usize,
    layouts: usize,
    pids: Vec<u64>,
}
impl Flow<'_> {
    fn snapshot(&mut self, name: &str) {
        let w = &self.owner.capture.page.workspace;
        let entry = w.entry().map(|e| identity(&mut self.ids, &e.identifier));
        let history=self.services.history.recent(100).unwrap().into_iter().rev().map(|e|json!({"id":identity(&mut self.ids,&e.identifier),"raw":e.raw_text,"output":e.delivered_text,"replies":self.services.conversations.replies(&e.identifier).unwrap().into_iter().map(|r|json!({"text":r.text,"instruction":r.instruction})).collect::<Vec<_>>()})).collect::<Vec<_>>();
        let live = &self.owner.capture.page.live_mode;
        let value = json!({"name":name,"page":self.owner.shell.stack.visible_child_name().map(String::from),"recording":self.owner.capture.phase()==Some(CapturePhase::Recording),"source":w.live_text.text(),"draft":w.live_draft(),"questions":w.live_questions.text(),"draft_status":w.live_draft_status.label().as_str(),"draft_mapped":w.live_draft_box.is_mapped(),"source_mapped":w.live_source_box.is_mapped(),"viewing_live":w.is_viewing_live(),"entry":entry,"documents":w.documents().iter().map(|v|v.text()).collect::<Vec<_>>(),"notice":w.notice.label().as_str(),"preferences":preferences(&self.services.config()),"persisted":preferences(&AppConfig::load(&self.services.paths.config.join("config.json")).unwrap()),"history":history,"clipboard":clipboard(),"sidebar":w.split.shows_sidebar(),"live_control":{"label":live.label().map(String::from),"active":live.is_active(),"sensitive":live.get_sensitive()}});
        fs::write(
            self.root.join(format!("{name}.json")),
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();
        let expected = &self.reference["stages"][self.count];
        let differences = value
            .as_object()
            .unwrap()
            .keys()
            .filter(|key| value[*key] != expected[*key])
            .collect::<Vec<_>>();
        assert!(
            differences.is_empty(),
            "Live workspace stage {name}; differing fields: {differences:?}; see {}",
            self.root.display()
        );
        self.count += 1;
        println!("LIVE_WORKSPACE_STAGE {name} PASS");
    }
    fn record(&mut self, repeats: usize) {
        let _ = fs::remove_file(self.tools.join("raw.ready.json"));
        let mut pcm = self.reference["pcm"].clone();
        pcm["pcm_hex"] = json!(pcm["pcm_hex"].as_str().unwrap().repeat(repeats));
        fs::write(
            self.tools.join("test-config.json"),
            serde_json::to_vec(&pcm).unwrap(),
        )
        .unwrap();
        self.owner.capture.page.record_button.emit_clicked();
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
        until(|| self.owner.capture.phase().is_none() && !self.owner.live.active());
        settle();
        self.snapshot(name);
    }
    fn turns(&self) -> usize {
        records(&self.evidence.join("requests.jsonl"))
            .iter()
            .filter(|r| r["message"]["method"] == "turn/start")
            .count()
    }
    fn provider(&self, result: &str, gated: bool) {
        let mut control = json!({"deltas":[result]});
        if gated {
            control["gate"] = json!(self.evidence.join("preview.release"));
        }
        let spec = json!({"scenario":"clean","evidence":self.evidence,"live_controls":{format!("preview|{}",self.reference["source"].as_str().unwrap()):control}});
        fs::write(self.spec, serde_json::to_vec(&spec).unwrap()).unwrap();
    }
    fn textures(&self) -> Vec<gtk::gdk::Texture> {
        widgets(&self.owner.capture.page.workspace.live_draft_box)
            .into_iter()
            .filter_map(|w| {
                w.downcast::<gtk::Picture>()
                    .ok()?
                    .paintable()?
                    .downcast::<gtk::gdk::Texture>()
                    .ok()
            })
            .collect()
    }
    fn texture_hashes(&self) -> Vec<String> {
        self.textures()
            .iter()
            .map(|texture| {
                glib::compute_checksum_for_data(
                    glib::ChecksumType::Sha256,
                    &texture.save_to_png_bytes(),
                )
                .unwrap()
                .to_string()
            })
            .collect()
    }
    fn layout(&mut self, name: &str) {
        assert!(
            Command::new("xdotool")
                .args(["mousemove", "0", "0"])
                .status()
                .unwrap()
                .success()
        );
        settle();
        let focused = gtk::prelude::GtkWindowExt::focus(&self.owner.shell.window).unwrap();
        let focus = json!({"visible":self.owner.shell.window.gets_focus_visible(),"type":focused.type_().name(),"tooltip":focused.tooltip_text().map(String::from)});
        let w = &self.owner.capture.page.workspace;
        let controls: [&gtk::Widget; 5] = [
            self.owner.capture.page.record_button.upcast_ref(),
            self.owner.capture.page.live_mode.upcast_ref(),
            w.live_source_box.upcast_ref(),
            w.live_draft_box.upcast_ref(),
            w.live_questions.upcast_ref(),
        ];
        let controls = controls
            .iter()
            .map(|w| {
                let r = w.compute_bounds(&self.owner.shell.window).unwrap();
                json!({"bounds":[r.x(),r.y(),r.width(),r.height()],"mapped":w.is_mapped()})
            })
            .collect::<Vec<_>>();
        let textures=self.textures().iter().enumerate().map(|(i,t)|{let bytes=t.save_to_png_bytes();fs::write(self.root.join(format!("{name}-diagram-{i}.png")),&bytes).unwrap();json!({"width":t.width(),"height":t.height(),"sha256":glib::compute_checksum_for_data(glib::ChecksumType::Sha256,&bytes).unwrap().as_str()})}).collect::<Vec<_>>();
        let value = json!({"name":name,"window":[self.owner.shell.window.width(),self.owner.shell.window.height()],"controls":controls,"textures":textures,"focus":focus});
        fs::write(
            self.root.join(format!("{name}-layout.json")),
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();
        capture_window(&self.root.join(format!("{name}.png")));
        assert_eq!(
            value, self.reference["layouts"][self.layouts],
            "Live layout {name}"
        );
        self.layouts += 1;
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
    let output = root.join("native-application-live-workspace");
    fs::create_dir(&output).unwrap();
    let spec = root.join("application-codex.json");
    let mut flow = Flow {
        owner,
        services,
        reference,
        tools,
        root: &output,
        evidence,
        spec: &spec,
        ids: BTreeMap::new(),
        count: 0,
        layouts: 0,
        pids: vec![],
    };
    let w = &owner.capture.page.workspace;
    let app = owner.shell.window.application().unwrap();
    gtk::gdk::Display::default()
        .unwrap()
        .clipboard()
        .set_text("untouched Live clipboard");
    flow.provider(reference["draft"].as_str().unwrap(), true);
    let old = services
        .history
        .add(HistoryInput {
            delivery_outcome: "ready".into(),
            ..HistoryInput::dictation("Saved conversation", "Saved conversation")
        })
        .unwrap();
    identity(&mut flow.ids, &old.identifier);
    rusqlite::Connection::open(&services.history.database.path)
        .unwrap()
        .execute(
            "UPDATE transcription_history SET created_at=? WHERE identifier=?",
            rusqlite::params![reference["seed_created"].as_str().unwrap(), old.identifier],
        )
        .unwrap();
    let old = services.history.find(&old.identifier).unwrap();
    w.refresh_history().unwrap();
    flow.record(30);
    flow.snapshot("waiting-for-speech");
    until(|| flow.turns() == 1);
    flow.snapshot("waiting-for-first-draft");
    let gate = evidence.join("preview.release");
    fs::write(&gate, "").unwrap();
    until(|| w.live_draft() == reference["draft"].as_str().unwrap());
    until(|| !flow.textures().is_empty());
    flow.snapshot("grilling-draft");
    flow.layout("grilling-wide");
    w.show_conversation(Some(old.clone()), &[], false).unwrap();
    settle();
    flow.snapshot("history-during-recording");
    w.live_navigation.emit_clicked();
    settle();
    flow.snapshot("return-to-live");
    flow.provider(reference["updated"].as_str().unwrap(), false);
    owner.capture.page.live_templates["task-spec"].set_active(true);
    until(|| flow.turns() == 2 && w.live_draft() == reference["updated"].as_str().unwrap());
    flow.snapshot("questions-updated");
    let buffer = w.live_draft_text.buffer();
    buffer.insert(&mut buffer.end_iter(), "\nKeep my deliberate edit.");
    flow.snapshot("manual-edit");
    fs::remove_file(&gate).unwrap();
    flow.provider(reference["late"].as_str().unwrap(), true);
    owner.capture.page.live_templates["grilling"].set_active(true);
    until(|| flow.turns() == 3);
    flow.snapshot("template-keeps-edit");
    owner.capture.page.live_mode.set_active(false);
    settle();
    flow.snapshot("paused-in-flight");
    fs::write(&gate, "").unwrap();
    settle_for(Duration::from_millis(400));
    flow.snapshot("late-result-rejected");
    assert_eq!(
        json!(
            owner
                .settings
                .apply(json!({"rewrite_provider":"litellm"}).as_object().unwrap())
        ),
        reference["busy_provider_accepted"]
    );
    for title in [
        "Settings · Widget position: Lower right",
        "Settings · Time format: 12-hour · 2:30 PM",
        "Toggle history sidebar",
    ] {
        application_commands::choose_action(owner, title);
        settle();
    }
    flow.snapshot("presentation-settings");
    flow.layout("grilling-sidebar");
    let previous = flow.texture_hashes();
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
    until(|| flow.texture_hashes() != previous);
    flow.layout("grilling-dark");
    let previous = flow.texture_hashes();
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceLight);
    until(|| flow.texture_hashes() != previous);
    owner.shell.window.set_default_size(480, 640);
    settle_for(Duration::from_millis(400));
    flow.snapshot("narrow");
    flow.layout("grilling-narrow");
    owner.shell.window.set_default_size(1060, 780);
    settle();
    w.show_conversation(Some(old.clone()), &[], false).unwrap();
    settle();
    flow.stop("paused-final-preserves-history");
    let saved = services
        .history
        .recent(100)
        .unwrap()
        .into_iter()
        .find(|e| e.identifier != old.identifier)
        .unwrap();
    let replies = services.conversations.replies(&saved.identifier).unwrap();
    w.show_conversation(Some(saved), &replies, false).unwrap();
    settle();
    let saved_textures = || {
        widgets(&w.messages)
            .into_iter()
            .filter_map(|widget| {
                widget
                    .downcast::<gtk::Picture>()
                    .ok()?
                    .paintable()?
                    .downcast::<gtk::gdk::Texture>()
                    .ok()
            })
            .collect::<Vec<_>>()
    };
    until(|| !saved_textures().is_empty());
    let textures=saved_textures().iter().enumerate().map(|(i,t)|{
        let bytes=t.save_to_png_bytes();fs::write(output.join(format!("saved-diagram-{i}.png")),&bytes).unwrap();
        json!({"width":t.width(),"height":t.height(),"sha256":glib::compute_checksum_for_data(glib::ChecksumType::Sha256,&bytes).unwrap().as_str()})
    }).collect::<Vec<_>>();
    assert_eq!(
        json!(textures),
        reference["saved_textures"],
        "reopened saved draft renders its actual stored Mermaid"
    );
    flow.snapshot("paused-draft-saved");
    for viewing in [false, true] {
        owner.capture.page.live_mode.set_active(true);
        flow.record(1);
        flow.snapshot(if viewing {
            "silent-live"
        } else {
            "silent-browsing-start"
        });
        w.live_draft_text
            .buffer()
            .set_text("My deliberate silent draft.");
        if !viewing {
            w.show_conversation(Some(old.clone()), &[], false).unwrap();
            settle();
        }
        flow.stop(if viewing {
            "silent-preserves-draft"
        } else {
            "silent-preserves-history"
        });
    }
    flow.record(1);
    flow.snapshot("new-capture-live-off");
    owner.capture.page.live_mode.set_active(true);
    settle();
    flow.snapshot("late-live-enable");
    w.live_draft_text
        .buffer()
        .set_text("My deliberate current-capture edit.");
    owner.capture.page.live_templates["task-spec"].set_active(true);
    settle();
    flow.snapshot("same-capture-template-keeps-edit");
    app.activate_action("cancel", None);
    until(|| owner.capture.phase().is_none());
    settle();
    flow.snapshot("cancelled");
    assert_eq!(flow.count, reference["stages"].as_array().unwrap().len());
    assert_eq!(flow.layouts, reference["layouts"].as_array().unwrap().len());
    flow.pids
}
