//! Root prompt actions, real request snapshots and a second process reopening saved files.
use super::{
    application_commands::key, capture_window, clipboard, records, settle_for, until, widgets,
    window_id,
};
use adw::prelude::*;
use mluva_core::{config::AppPaths, history::HistoryInput};
use mluva_gtk::application::ApplicationDesktop;
use mluva_workflows::{capture::CapturePhase, services::ApplicationServices};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

pub fn paths(root: &Path, reopening: bool) -> AppPaths {
    let persist = root.join("prompt-state");
    // The runner mounts private /tmp. A stable alias makes the actual file label
    // identical in both implementations while preserving evidence under root.
    let alias = Path::new("/tmp/mluva-prompt-comparison");
    if reopening {
        assert_eq!(alias.canonicalize().unwrap(), persist);
        assert_ne!(
            fs::read_to_string(persist.join("pid")).unwrap(),
            std::process::id().to_string()
        );
    } else {
        fs::create_dir(&persist).unwrap();
        symlink(&persist, alias).unwrap();
    }
    AppPaths {
        config: alias.join("config/mluva"),
        data: alias.join("data/mluva"),
        runtime: alias.join("runtime/mluva"),
    }
}
fn settle() {
    settle_for(Duration::from_millis(200));
}
fn labels(widget: &impl IsA<gtk::Widget>) -> Vec<String> {
    widgets(widget)
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Label>().ok())
        .map(|l| l.label().to_string())
        .collect()
}
fn editor(owner: &ApplicationDesktop) -> Option<(adw::Dialog, gtk::TextView)> {
    let dialogs = owner.shell.window.dialogs();
    (0..dialogs.n_items()).find_map(|i| {
        let dialog = dialogs.item(i)?.downcast::<adw::Dialog>().ok()?;
        let view = widgets(&dialog)
            .into_iter()
            .find_map(|w| w.downcast::<gtk::TextView>().ok())?;
        Some((dialog, view))
    })
}
fn text(view: &gtk::TextView) -> String {
    let b = view.buffer();
    b.text(&b.start_iter(), &b.end_iter(), false).to_string()
}
struct Flow<'a> {
    owner: &'a ApplicationDesktop,
    services: &'a ApplicationServices,
    application: &'a gtk::Application,
    reference: &'a Value,
    root: PathBuf,
    tools: &'a Path,
    evidence: &'a Path,
    spec: PathBuf,
    style: Option<String>,
    states: Vec<Value>,
    layouts: Vec<Value>,
    pids: Vec<u64>,
}
impl Flow<'_> {
    fn clean(&self, value: &str) -> String {
        self.style
            .as_ref()
            .map_or_else(|| value.to_owned(), |id| value.replace(id, "$STYLE"))
    }
    fn snapshot(&mut self, name: &str) {
        settle_for(Duration::from_millis(100));
        let w = &self.owner.capture.page.workspace;
        let dialog=self.owner.shell.window.visible_dialog().map(|d|json!({"title":d.title().as_str(),"labels":labels(&d).iter().map(|s|self.clean(s)).collect::<Vec<_>>(),"buttons":widgets(&d).into_iter().filter_map(|w|w.downcast::<gtk::Button>().ok()).filter_map(|b|b.label().filter(|s|!s.is_empty()).map(|l|json!({"label":l.as_str(),"sensitive":b.get_sensitive()}))).collect::<Vec<_>>()}));
        let edit=editor(self.owner).map(|(d,v)|json!({"text":text(&v),"identity":labels(&d).into_iter().find(|s|matches!(s.as_str(),"Local override"|"Built-in default"|"Original saved text")).unwrap()}));
        let directory = self.services.prompts.borrow().directory.clone();
        let mut files = serde_json::Map::new();
        if directory.exists() {
            for file in fs::read_dir(&directory).unwrap() {
                let file = file.unwrap().path();
                if file.extension().is_some_and(|x| x == "md") {
                    files.insert(self.clean(file.file_name().unwrap().to_str().unwrap()),json!({"text":fs::read_to_string(&file).unwrap(),"mode":fs::metadata(file).unwrap().permissions().mode()&0o777}));
                }
            }
        }
        let history=self.services.history.recent(100).unwrap().into_iter().map(|e|json!({"raw":e.raw_text,"output":e.delivered_text,"replies":self.services.conversations.replies(&e.identifier).unwrap().into_iter().map(|r|r.text).collect::<Vec<_>>()})).collect::<Vec<_>>();
        let mut value = json!({"name":name,"page":self.owner.shell.stack.visible_child_name().map(String::from),"dialog":dialog,"editor":edit,"files":files,"documents":w.documents().iter().map(|d|d.text()).collect::<Vec<_>>(),"prompt":w.prompt_text(),"draft":w.live_draft(),"template":self.services.config().live_rewrite_template,"recording":self.owner.capture.phase()==Some(CapturePhase::Recording),"clipboard":clipboard(),"history":history,"config_error":!self.services.config_load_error.is_empty()});
        if name == "restarted" {
            value["workspace_actions"] = json!({"title":w.title_button.get_sensitive(),"continuation":w.continue_button.is_visible(),"polish":w.quick_polish.get_sensitive(),"structure":w.structured_note.get_sensitive(),"save_prompt":w.save.get_sensitive(),"rewrite":w.send.get_sensitive()});
            value["notice"] = json!(w.notice.label().as_str());
        }
        if self.reference["name"] == "bad-config" {
            assert_eq!(
                fs::read_to_string(self.services.paths.config.join("config.json")).unwrap(),
                "{malformed configuration"
            );
        }
        fs::write(
            self.root.join(format!("{name}.json")),
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();
        let expected = &self.reference["stages"][self.states.len()];
        for (field, actual) in value.as_object().unwrap() {
            assert_eq!(actual, &expected[field], "prompt {name}.{field}");
        }
        assert_eq!(
            value.as_object().unwrap().len(),
            expected.as_object().unwrap().len()
        );
        self.states.push(value);
        eprintln!("PROMPT_STAGE {name} PASS");
    }
    fn respond(&self, label: &str) {
        let dialog = self.owner.shell.window.visible_dialog().unwrap();
        let button = widgets(&dialog)
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::Button>().ok())
            .find(|b| b.label().as_deref() == Some(label))
            .unwrap();
        // Include the stale Save after a privacy change, as in the released observer.
        button.emit_clicked();
        settle();
    }
    fn close(&self) {
        self.respond("Cancel");
        until(|| editor(self.owner).is_none());
    }
    fn choose(&self, name: &str) {
        until(|| self.owner.shell.window.visible_dialog().is_none());
        key("Control_L+p");
        until(|| self.owner.shell.window.visible_dialog().is_some());
        let panel = self.owner.shell.window.visible_dialog().unwrap();
        assert_eq!(panel.title(), "Commands");
        let search = widgets(&panel)
            .into_iter()
            .find_map(|w| w.downcast::<gtk::SearchEntry>().ok())
            .unwrap();
        let results = widgets(&panel)
            .into_iter()
            .find_map(|w| w.downcast::<gtk::ListBox>().ok())
            .unwrap();
        let title = format!("Settings · Edit prompt · {name}");
        search.set_text(&title);
        let find = || {
            widgets(&results)
                .into_iter()
                .filter_map(|w| w.downcast::<gtk::ListBoxRow>().ok())
                .find(|r| labels(r).contains(&title))
        };
        until(|| find().is_some());
        results.select_row(find().as_ref());
        key("Return");
        until(|| editor(self.owner).is_some_and(|(_, v)| v.is_mapped()));
    }
    fn edit(&self, value: &str) {
        editor(self.owner).unwrap().1.buffer().set_text(value);
    }
    fn hover(&self, widget: &impl IsA<gtk::Widget>) {
        let bounds = widget.compute_bounds(&self.owner.shell.window).unwrap();
        assert!(
            Command::new("xdotool")
                .args([
                    "mousemove",
                    "--window",
                    &window_id(),
                    &((bounds.x() + bounds.width() / 2.0) as i32).to_string(),
                    &((bounds.y() + bounds.height() / 2.0) as i32).to_string()
                ])
                .status()
                .unwrap()
                .success()
        );
        settle();
    }
    fn layout(&mut self, name: &str) {
        until(|| {
            !widgets(&self.owner.shell.window)
                .into_iter()
                .any(|w| w.is_mapped() && w.accessible_role() == gtk::AccessibleRole::Alert)
        });
        settle_for(Duration::from_millis(250));
        capture_window(&self.root.join(format!("{name}.png")));
        let dialog = self.owner.shell.window.visible_dialog();
        let parent = dialog
            .as_ref()
            .map(|d| d.clone().upcast::<gtk::Widget>())
            .unwrap_or_else(|| {
                self.owner
                    .capture
                    .page
                    .workspace
                    .quick_polish
                    .parent()
                    .unwrap()
            });
        let controls=widgets(&parent).into_iter().filter_map(|w|w.downcast::<gtk::Button>().ok()).filter(|b|b.is_mapped()).map(|b|{let r=b.compute_bounds(&self.owner.shell.window).unwrap();json!({"label":b.label().map(String::from),"sensitive":b.get_sensitive(),"bounds":[r.x(),r.y(),r.width(),r.height()]})}).collect::<Vec<_>>();
        let value = json!({"name":name,"window":[self.owner.shell.window.width(),self.owner.shell.window.height()],"dialog":dialog.map(|d|[d.width(),d.height()]),"controls":controls});
        fs::write(
            self.root.join(format!("{name}-layout.json")),
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();
        assert_eq!(
            value,
            self.reference["layouts"][self.layouts.len()],
            "prompt layout {name}"
        );
        self.layouts.push(value);
    }
    fn turns(&self) -> usize {
        records(&self.evidence.join("requests.jsonl"))
            .iter()
            .filter(|r| r["message"]["method"] == "turn/start")
            .count()
    }
    fn record(&mut self) {
        let _ = fs::remove_file(self.tools.join("raw.ready.json"));
        self.application.activate_action("record", None);
        until(|| {
            self.owner.capture.phase() == Some(CapturePhase::Recording)
                && self.tools.join("raw.ready.json").exists()
        });
        let ready: Value =
            serde_json::from_slice(&fs::read(self.tools.join("raw.ready.json")).unwrap()).unwrap();
        self.pids.push(ready["pid"].as_u64().unwrap());
    }
    fn cancel(&self) {
        self.application.activate_action("cancel", None);
        until(|| self.owner.capture.phase().is_none());
        until(|| !Path::new(&format!("/proc/{}", self.pids.last().unwrap())).exists());
        settle();
    }
    fn prompt_path(&self, id: &str) -> PathBuf {
        self.services.prompts.borrow().path(id).unwrap()
    }
}

pub fn exercise(
    owner: &ApplicationDesktop,
    services: &ApplicationServices,
    reference: &Value,
    tools: &Path,
    root: &Path,
    evidence: &Path,
    reopening: bool,
) -> Vec<u64> {
    let output = root.join(if reopening {
        "native-application-prompts-reopen"
    } else {
        "native-application-prompts"
    });
    fs::create_dir(&output).unwrap();
    let width = reference["params"]["width"].as_i64().unwrap() as i32;
    let height = reference["params"]["height"].as_i64().unwrap() as i32;
    let win = &owner.shell.window;
    let application = win.application().unwrap();
    win.set_default_size(width + 10, height + 10);
    win.set_size_request(width + 10, height + 10);
    settle_for(Duration::from_millis(300));
    assert_eq!([win.width(), win.height()], [width, height]);
    for args in [
        vec!["windowmove".into(), window_id(), "20".into(), "20".into()],
        vec!["mousemove".into(), "0".into(), "0".into()],
    ] {
        assert!(
            Command::new("xdotool")
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    }
    gtk::gdk::Display::default()
        .unwrap()
        .clipboard()
        .set_text("untouched prompt clipboard");
    let w = &owner.capture.page.workspace;
    let mut flow = Flow {
        owner,
        services,
        application: &application,
        reference,
        root: output,
        tools,
        evidence,
        spec: root.join("application-codex.json"),
        style: None,
        states: vec![],
        layouts: vec![],
        pids: vec![],
    };
    let name = reference["name"].as_str().unwrap();
    if reopening {
        flow.style = services
            .personalization
            .borrow()
            .styles()
            .unwrap()
            .into_iter()
            .find(|s| s.name == "Synthetic brief")
            .map(|s| s.identifier.to_lowercase());
        flow.choose("Live · Grilling");
        flow.snapshot("restarted");
        flow.layout("restart");
        flow.close();
    } else {
        if name == "wide" {
            let edit = w
                .quick_polish
                .parent()
                .unwrap()
                .last_child()
                .and_downcast::<gtk::Button>()
                .unwrap();
            assert!(!w.quick_polish.get_sensitive() && edit.get_sensitive());
            edit.emit_clicked();
            until(|| editor(owner).is_some());
            flow.snapshot("empty-polish");
            flow.close();
            let entry = services
                .history
                .add(HistoryInput {
                    delivery_outcome: "ready".into(),
                    ..HistoryInput::dictation("Synthetic speech", "Synthetic speech")
                })
                .unwrap();
            rusqlite::Connection::open(&services.history.database.path)
                .unwrap()
                .execute(
                    "UPDATE transcription_history SET identifier=?,created_at=? WHERE identifier=?",
                    rusqlite::params![
                        "00000000-0000-4000-8000-000000000001",
                        "2026-03-17T13:40:00+00:00",
                        entry.identifier
                    ],
                )
                .unwrap();
            let entry = services
                .history
                .find("00000000-0000-4000-8000-000000000001")
                .unwrap();
            w.refresh_history().unwrap();
            w.show_conversation(Some(entry), &[], false).unwrap();
            w.prompt.buffer().set_text("Keep a concise brief.");
            w.save.emit_clicked();
            settle();
            let d = win
                .visible_dialog()
                .and_downcast::<adw::AlertDialog>()
                .unwrap();
            d.extra_child()
                .and_downcast::<gtk::Entry>()
                .unwrap()
                .set_text("Synthetic brief");
            flow.respond("Save");
            until(|| win.visible_dialog().is_none());
            flow.style = Some(
                services
                    .personalization
                    .borrow()
                    .styles()
                    .unwrap()
                    .into_iter()
                    .find(|s| s.name == "Synthetic brief")
                    .unwrap()
                    .identifier
                    .to_lowercase(),
            );
            w.prompt.buffer().set_text("Unsent instructions");
            w.prompt.grab_focus();
            flow.hover(&owner.shell.header);
            until(|| edit.opacity() == 0.0);
            flow.snapshot("no-hover");
            flow.layout("no-hover");
            flow.hover(&w.quick_polish);
            until(|| edit.opacity() == 1.0);
            flow.snapshot("hover");
            flow.layout("hover");
            flow.hover(&owner.shell.header);
            edit.grab_focus();
            until(|| edit.has_focus());
            key("space");
            until(|| editor(owner).is_some());
            flow.snapshot("keyboard-polish");
            assert_eq!(flow.turns(), 0);
            flow.close();
        }
        flow.choose("Live · Grilling");
        flow.snapshot("editor");
        flow.layout("editor");
        flow.edit("Ask one useful question.\n\n## Architecture\nKeep exact Markdown and Žluťoučký text.\n");
        flow.respond("Cancel");
        flow.snapshot("discard-question");
        flow.respond("Keep editing");
        flow.respond("Save");
        until(|| editor(owner).is_none());
        flow.snapshot("saved");
        flow.choose("Live · Grilling");
        flow.respond("Restore default");
        flow.snapshot("reset-staged");
        flow.respond("Cancel");
        flow.respond("Discard");
        until(|| editor(owner).is_none());
        flow.snapshot("reset-discarded");
        flow.choose("Live · Grilling");
        flow.edit("Unsaved local draft");
        fs::write(flow.prompt_path("live-grilling"), "Externally edited").unwrap();
        flow.respond("Save");
        flow.snapshot("conflict");
        flow.layout("conflict");
        flow.respond("Cancel");
        flow.respond("Discard");
        until(|| editor(owner).is_none());
        flow.choose("Live · Grilling");
        flow.respond("Restore default");
        flow.respond("Save");
        until(|| editor(owner).is_none());
        flow.snapshot("reset-saved");
        if name == "wide" {
            for template in reference["templates"].as_array().unwrap() {
                let template = template.as_str().unwrap();
                owner.capture.page.live_templates[template]
                    .parent()
                    .unwrap()
                    .last_child()
                    .and_downcast::<gtk::Button>()
                    .unwrap()
                    .emit_clicked();
                until(|| editor(owner).is_some());
                flow.snapshot(&format!("template-{template}"));
                flow.close();
            }
            flow.choose("Style · Synthetic brief");
            flow.snapshot("style-command");
            flow.close();
            application.activate_action("settings", None);
            owner
                .settings
                .view
                .set_visible_page(&owner.settings.prompts.widget);
            settle();
            widgets(&owner.settings.prompts.widget)
                .into_iter()
                .filter_map(|w| w.downcast::<adw::ActionRow>().ok())
                .find(|r| r.title() == "Style · Synthetic brief")
                .unwrap()
                .emit_by_name::<()>("activated", &[]);
            flow.snapshot("style-settings");
            flow.close();
            application.activate_action("latest", None);
            owner.capture.page.live_mode.set_active(true);
            flow.record();
            until(|| flow.turns() == 1 && w.live_draft() == reference["draft"].as_str().unwrap());
            flow.snapshot("current-recording");
            w.live_draft_text
                .buffer()
                .set_text(reference["manual"].as_str().unwrap());
            flow.choose("Live · Grilling");
            flow.edit("Next recording instructions");
            flow.respond("Save");
            until(|| editor(owner).is_none());
            flow.snapshot("saved-during-recording");
            let mut spec: Value = serde_json::from_slice(&fs::read(&flow.spec).unwrap()).unwrap();
            let source = format!("preview|{}", reference["source"].as_str().unwrap());
            spec["live_controls"][&source]["gate"] = json!(evidence.join("restart.release"));
            fs::write(&flow.spec, serde_json::to_vec(&spec).unwrap()).unwrap();
            owner.capture.page.live_mode.set_active(false);
            owner.capture.page.live_mode.set_active(true);
            until(|| flow.turns() == 2);
            settle_for(Duration::from_millis(300));
            flow.snapshot("same-recording");
            flow.cancel();
            spec["live_controls"][&source]
                .as_object_mut()
                .unwrap()
                .remove("gate");
            fs::write(&flow.spec, serde_json::to_vec(&spec).unwrap()).unwrap();
            flow.record();
            until(|| flow.turns() == 3 && w.live_draft() == reference["draft"].as_str().unwrap());
            flow.snapshot("next-recording");
            flow.cancel();
            let entry = services
                .history
                .find("00000000-0000-4000-8000-000000000001")
                .unwrap();
            w.show_conversation(
                Some(entry.clone()),
                &services.conversations.replies(&entry.identifier).unwrap(),
                false,
            )
            .unwrap();
            settle();
            let popover = w.saved_prompts.popover().unwrap();
            popover.popup();
            until(|| popover.is_mapped());
            let row = widgets(&popover)
                .into_iter()
                .filter_map(|w| w.downcast::<gtk::Box>().ok())
                .find(|r| {
                    r.first_child()
                        .and_downcast::<gtk::Button>()
                        .is_some_and(|b| b.label().as_deref() == Some("Synthetic brief"))
                })
                .unwrap();
            row.last_child()
                .and_downcast::<gtk::Button>()
                .unwrap()
                .emit_clicked();
            until(|| editor(owner).is_some());
            flow.snapshot("style-menu");
            flow.close();
            popover.popdown();
            services
                .prompts
                .borrow()
                .save(
                    &format!("style-{}", flow.style.as_ref().unwrap()),
                    "External style instructions",
                    None,
                )
                .unwrap();
            widgets(&popover)
                .into_iter()
                .filter_map(|w| w.downcast::<gtk::Button>().ok())
                .find(|b| b.label().as_deref() == Some("Synthetic brief"))
                .unwrap()
                .emit_clicked();
            until(|| {
                !owner.review.rewriting()
                    && services
                        .conversations
                        .replies(&entry.identifier)
                        .unwrap()
                        .len()
                        == 1
            });
            flow.snapshot("style-executed");
            owner.settings.capture.incognito.set_active(true);
            flow.choose("Live · Custom");
            flow.edit("Private draft");
            flow.respond("Save");
            flow.snapshot("private-refused");
            flow.respond("Cancel");
            flow.respond("Discard");
            until(|| editor(owner).is_none());
        } else {
            flow.choose("Live · Grilling");
            flow.edit("Next recording instructions");
            flow.respond("Save");
            until(|| editor(owner).is_none());
            flow.snapshot("next-prompt-saved");
        }
        fs::write(
            root.join("prompt-state/pid"),
            std::process::id().to_string(),
        )
        .unwrap();
    }
    assert_eq!(json!(flow.states), reference["stages"]);
    assert_eq!(json!(flow.layouts), reference["layouts"]);
    eprintln!(
        "Prompt {name} reopen={reopening}: {} states and {} layouts",
        flow.states.len(),
        flow.layouts.len()
    );
    flow.pids
}
