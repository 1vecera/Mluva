//! Real sidebar/title actions through the assembled application and durable stores.
use super::{
    application_commands::key, capture_window, clipboard, records, settle_for, until, widgets,
    window_id,
};
use adw::prelude::*;
use mluva_core::history::HistoryInput;
use mluva_gtk::{application::ApplicationDesktop, conversation_view::ConversationWorkspace};
use mluva_workflows::{capture::CapturePhase, services::ApplicationServices};
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command, time::Duration};

const IDS: [&str; 2] = [
    "00000000-0000-4000-8000-000000000001",
    "00000000-0000-4000-8000-000000000002",
];
fn settle() {
    settle_for(Duration::from_millis(100));
}
fn labels(widget: &impl IsA<gtk::Widget>) -> Vec<String> {
    widgets(widget)
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Label>().ok())
        .map(|l| l.label().to_string())
        .collect()
}
fn row(w: &ConversationWorkspace, title: &str) -> gtk::ListBoxRow {
    widgets(&w.history_list)
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::ListBoxRow>().ok())
        .find(|r| labels(r).first().is_some_and(|label| label == title))
        .unwrap()
}
fn menu(w: &ConversationWorkspace, title: &str) -> gtk::MenuButton {
    widgets(&row(w, title))
        .into_iter()
        .find_map(|w| w.downcast::<gtk::MenuButton>().ok())
        .unwrap()
}
fn action(w: &ConversationWorkspace, title: &str, label: &str) {
    w.split.set_show_sidebar(true);
    let menu = menu(w, title);
    menu.popup();
    until(|| menu.popover().unwrap().is_mapped());
    let button = widgets(&menu.popover().unwrap())
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Button>().ok())
        .find(|b| b.label().as_deref() == Some(label))
        .unwrap();
    assert!(button.is_sensitive());
    button.emit_clicked();
    settle();
}
fn respond(owner: &ApplicationDesktop, label: &str) {
    let dialog = owner.shell.window.visible_dialog().unwrap();
    let button = widgets(&dialog)
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Button>().ok())
        .find(|b| b.label().as_deref() == Some(label))
        .unwrap();
    assert!(button.is_sensitive());
    button.emit_clicked();
    until(|| owner.shell.window.visible_dialog().is_none());
    settle();
}
fn select_target(owner: &ApplicationDesktop) {
    let dialog = owner
        .shell
        .window
        .visible_dialog()
        .and_downcast::<adw::AlertDialog>()
        .unwrap();
    let content = dialog.extra_child().unwrap();
    let search = content
        .first_child()
        .and_downcast::<gtk::SearchEntry>()
        .unwrap();
    search.set_text("Roadmap");
    settle_for(Duration::from_millis(300));
    let choices = widgets(&content)
        .into_iter()
        .find_map(|w| w.downcast::<gtk::ListBox>().ok())
        .unwrap();
    until(|| choices.row_at_index(0).is_some() && choices.row_at_index(1).is_none());
    choices.select_row(choices.row_at_index(0).as_ref());
    assert!(dialog.is_response_enabled("merge"));
}
fn select(w: &ConversationWorkspace, title: &str, id: &str) {
    w.split.set_show_sidebar(true);
    row(w, title).emit_by_name::<()>("activate", &[]);
    settle();
    assert_eq!(w.entry().unwrap().identifier, id);
}
struct Flow<'a> {
    owner: &'a ApplicationDesktop,
    services: &'a ApplicationServices,
    application: &'a adw::Application,
    reference: &'a Value,
    tools: &'a Path,
    root: &'a Path,
    states: Vec<Value>,
    layouts: Vec<Value>,
    pids: Vec<u64>,
}
impl Flow<'_> {
    fn record(&mut self, name: &str) {
        settle_for(Duration::from_millis(80));
        let w = &self.owner.capture.page.workspace;
        let documents = w.documents();
        let menus = widgets(&w.history_list)
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::ListBoxRow>().ok())
            .filter_map(|row| {
                let menu = widgets(&row)
                    .into_iter()
                    .find_map(|w| w.downcast::<gtk::MenuButton>().ok())?;
                Some(json!({"labels":labels(&row),"sensitive":menu.get_sensitive()}))
            })
            .collect::<Vec<_>>();
        let mut entries = self.services.history.recent(100).unwrap();
        entries.sort_by(|a, b| a.identifier.cmp(&b.identifier));
        let history=entries.iter().map(|e|json!({"id":e.identifier,"raw":e.raw_text,"output":e.delivered_text,"title":e.title,"replies":self.services.conversations.replies(&e.identifier).unwrap().into_iter().map(|r|r.text).collect::<Vec<_>>()})).collect::<Vec<_>>();
        let titles = self
            .owner
            .history
            .page
            .rows
            .borrow()
            .iter()
            .map(|(id, row)| (id.clone(), json!(row.title().as_str())))
            .collect::<serde_json::Map<_, _>>();
        let dialog=self.owner.shell.window.visible_dialog().and_downcast::<adw::AlertDialog>().map(|d|json!({"heading":d.heading().map(String::from),"body":d.body().as_str(),"merge":d.has_response("merge").then(||d.is_response_enabled("merge"))}));
        let actual = json!({"name":name,"selected":w.entry().map(|e|e.identifier),"documents":documents.iter().map(|d|d.text()).collect::<Vec<_>>(),"selections":documents.iter().map(|d|{let b=d.buffer();[b.iter_at_mark(&b.get_insert()).offset(),b.iter_at_mark(&b.selection_bound()).offset()]}).collect::<Vec<_>>(),"prompt":w.prompt_text(),"title":{"view":w.title_stack.visible_child_name().map(String::from),"text":w.title_entry.text().as_str(),"error":w.title_entry.has_css_class("error"),"label":w.conversation_title.label().as_str(),"sensitive":w.title_button.get_sensitive()},"menus":menus,"history_titles":titles,"history":history,"conversations":self.services.conversations.search("",100,None).unwrap().into_iter().map(|e|e.identifier).collect::<Vec<_>>(),"private":self.services.config().incognito_mode,"recording":self.owner.capture.phase()==Some(CapturePhase::Recording),"clipboard":clipboard(),"dialog":dialog});
        fs::write(
            self.root.join(format!("{name}.json")),
            serde_json::to_vec_pretty(&actual).unwrap(),
        )
        .unwrap();
        let expected = &self.reference["stages"][self.states.len()];
        for (field, value) in actual.as_object().unwrap() {
            assert_eq!(value, &expected[field], "management {name}.{field}");
        }
        assert_eq!(
            actual.as_object().unwrap().len(),
            expected.as_object().unwrap().len()
        );
        self.states.push(actual);
    }
    fn layout(&mut self, name: &str, popover: Option<&gtk::Popover>) {
        settle_for(Duration::from_millis(200));
        let window = &self.owner.shell.window;
        let size = if let Some(popover) = popover {
            let snapshot = gtk::Snapshot::new();
            gtk::WidgetPaintable::new(Some(popover)).snapshot(
                &snapshot,
                f64::from(popover.width()),
                f64::from(popover.height()),
            );
            let viewport =
                gtk::graphene::Rect::new(0.0, 0.0, popover.width() as f32, popover.height() as f32);
            popover
                .native()
                .unwrap()
                .renderer()
                .unwrap()
                .render_texture(snapshot.to_node().unwrap(), Some(&viewport))
                .save_to_png(self.root.join(format!("{name}.png")))
                .unwrap();
            [popover.width(), popover.height()]
        } else {
            capture_window(&self.root.join(format!("{name}.png")));
            [window.width(), window.height()]
        };
        let actual = json!({"name":name,"size":size});
        assert_eq!(actual, self.reference["layouts"][self.layouts.len()]);
        self.layouts.push(actual);
    }
    fn recording(&mut self) {
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
    }
}
pub fn exercise(
    owner: &ApplicationDesktop,
    services: &ApplicationServices,
    application: &adw::Application,
    reference: &Value,
    tools: &Path,
    root: &Path,
    evidence: &Path,
) -> Vec<u64> {
    let root = root.join("native-application-management");
    fs::create_dir(&root).unwrap();
    let width = reference["params"]["width"].as_i64().unwrap() as i32;
    let height = reference["params"]["height"].as_i64().unwrap() as i32;
    let win = &owner.shell.window;
    win.set_default_size(width, height);
    win.set_size_request(width, height);
    settle_for(Duration::from_millis(300));
    assert_eq!(
        [
            win.surface().unwrap().width(),
            win.surface().unwrap().height()
        ],
        [width, height]
    );
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
    let w = &owner.capture.page.workspace;
    for (index, text) in ["First original", "Second original"]
        .into_iter()
        .enumerate()
    {
        let entry = services
            .history
            .add(HistoryInput::dictation(text, text))
            .unwrap();
        rusqlite::Connection::open(&services.history.database.path)
            .unwrap()
            .execute(
                "UPDATE transcription_history SET identifier=?,created_at=? WHERE identifier=?",
                rusqlite::params![
                    IDS[index],
                    format!("2026-03-17T13:4{index}:00+00:00"),
                    entry.identifier
                ],
            )
            .unwrap();
        services
            .history
            .update_title(IDS[index], Some(["Roadmap", "Meeting notes"][index]))
            .unwrap();
    }
    let reply = services
        .conversations
        .append(IDS[1], "Polish", "Second polished", "fixture")
        .unwrap();
    w.refresh_history().unwrap();
    owner.history.page.refresh().unwrap();
    w.show_conversation(
        Some(services.history.find(IDS[1]).unwrap()),
        &[reply],
        false,
    )
    .unwrap();
    settle();
    let editor = w.documents()[0].clone();
    editor.buffer().set_text("Edited source");
    editor.buffer().select_range(
        &editor.buffer().iter_at_offset(2),
        &editor.buffer().iter_at_offset(7),
    );
    w.prompt.buffer().set_text("Unsent follow-up");
    gtk::gdk::Display::default()
        .unwrap()
        .clipboard()
        .set_text("untouched management clipboard");
    let mut flow = Flow {
        owner,
        services,
        application,
        reference,
        tools,
        root: &root,
        states: vec![],
        layouts: vec![],
        pids: vec![],
    };
    w.title_button.emit_clicked();
    w.title_entry.set_text("  Renamed meeting  ");
    flow.record("inline-title");
    flow.layout("inline-title", None);
    w.title_entry.emit_activate();
    flow.record("renamed");
    w.title_button.emit_clicked();
    w.title_entry.set_text(" ");
    w.title_entry.emit_activate();
    flow.record("invalid-title");
    w.title_entry.set_text("Discard me");
    key("Escape");
    flow.record("cancelled-title");
    action(w, "Roadmap", "Rename");
    w.title_entry.set_text("Wrong destination");
    select(w, "Renamed meeting", IDS[1]);
    w.title_entry.emit_activate();
    flow.record("stale-title");
    w.split.set_show_sidebar(true);
    let menu = menu(w, "Renamed meeting");
    menu.popup();
    until(|| menu.popover().unwrap().is_mapped());
    flow.record("sidebar-menu");
    flow.layout("sidebar-menu", None);
    flow.layout("sidebar-actions", menu.popover().as_ref());
    menu.popdown();
    action(w, "Renamed meeting", "Merge with…");
    flow.record("merge-unselected");
    select_target(owner);
    flow.record("merge-selected");
    flow.layout("merge-dialog", None);
    respond(owner, "Cancel");
    flow.record("merge-cancelled");
    action(w, "Renamed meeting", "Merge with…");
    select_target(owner);
    w.send.emit_clicked();
    until(|| {
        records(&evidence.join("requests.jsonl"))
            .iter()
            .any(|r| r["message"]["method"] == "turn/start")
    });
    flow.record("merge-rewriting");
    respond(owner, "Merge");
    flow.record("merge-rewrite-blocked");
    w.cancel.emit_clicked();
    until(|| !owner.review.rewriting());
    flow.record("rewrite-cancelled");
    w.prompt.buffer().set_text("Unsent follow-up");
    action(w, "Renamed meeting", "Merge with…");
    select_target(owner);
    flow.recording();
    flow.record("merge-recording");
    respond(owner, "Merge");
    flow.record("merge-blocked");
    flow.cancel();
    select(w, "Renamed meeting", IDS[1]);
    flow.record("recording-cancelled");
    action(w, "Renamed meeting", "Merge with…");
    select_target(owner);
    respond(owner, "Merge");
    flow.record("merged");
    w.title_button.emit_clicked();
    w.title_entry.set_text("Private rename must not persist");
    action(w, "Roadmap", "Delete…");
    owner.settings.capture.incognito.set_active(true);
    respond(owner, "Delete");
    flow.record("private-delete-blocked");
    w.title_entry.emit_activate();
    flow.record("private-rename-blocked");
    owner.settings.capture.incognito.set_active(false);
    select(w, "Roadmap", IDS[0]);
    action(w, "Roadmap", "Delete…");
    flow.recording();
    flow.record("delete-recording");
    respond(owner, "Delete");
    flow.record("delete-recording-blocked");
    flow.cancel();
    select(w, "Roadmap", IDS[0]);
    flow.record("delete-recording-cancelled");
    action(w, "Roadmap", "Delete…");
    respond(owner, "Cancel");
    flow.record("delete-cancelled");
    action(w, "Roadmap", "Delete…");
    respond(owner, "Delete");
    flow.record("deleted");
    flow.layout("deleted-empty-state", None);
    assert_eq!(json!(flow.states), reference["stages"]);
    assert_eq!(json!(flow.layouts), reference["layouts"]);
    eprintln!(
        "Management {}: {} states and {} layouts",
        reference["name"],
        flow.states.len(),
        flow.layouts.len()
    );
    flow.pids
}
