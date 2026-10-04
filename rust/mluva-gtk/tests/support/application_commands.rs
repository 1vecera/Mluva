//! Joined command routing through the actual application, private keys, clipboard and stores.
use super::{capture_window, clipboard, settle, settle_for, until, widgets};
use adw::prelude::*;
use mluva_core::history::HistoryInput;
use mluva_gtk::application::ApplicationDesktop;
use mluva_workflows::{capture::CapturePhase, services::ApplicationServices};
use serde_json::{Value, json};
use std::{cell::RefCell, fs, path::Path, process::Command, rc::Rc, time::Duration};

fn key(chord: &str) {
    let program = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples/private_input");
    let output = Command::new(program).args(["key", chord]).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    settle_for(Duration::from_millis(150));
}
struct Panel {
    dialog: adw::Dialog,
    search: gtk::SearchEntry,
    results: gtk::ListBox,
    scroll: gtk::ScrolledWindow,
    empty: gtk::Label,
}
impl Panel {
    fn current(owner: &ApplicationDesktop) -> Option<Self> {
        let dialog = owner.shell.window.visible_dialog()?;
        assert_eq!(dialog.title(), "Commands");
        let tree = widgets(&dialog);
        Some(Self {
            search: tree
                .iter()
                .find_map(|widget| widget.clone().downcast::<gtk::SearchEntry>().ok())
                .unwrap(),
            results: tree
                .iter()
                .find_map(|widget| widget.clone().downcast::<gtk::ListBox>().ok())
                .unwrap(),
            scroll: tree
                .iter()
                .find_map(|widget| widget.clone().downcast::<gtk::ScrolledWindow>().ok())
                .unwrap(),
            empty: tree
                .iter()
                .find_map(|widget| {
                    widget
                        .clone()
                        .downcast::<gtk::Label>()
                        .ok()
                        .filter(|label| label.text() == "No matching actions")
                })
                .unwrap(),
            dialog,
        })
    }
    fn open(owner: &ApplicationDesktop) -> Self {
        until(|| owner.shell.window.visible_dialog().is_none());
        key("Control_L+p");
        until(|| Self::current(owner).is_some_and(|panel| panel.search.is_mapped()));
        Self::current(owner).unwrap()
    }
    fn rows(&self) -> Vec<adw::ActionRow> {
        widgets(&self.results)
            .into_iter()
            .filter_map(|widget| widget.downcast().ok())
            .collect()
    }
    fn selected(&self) -> Option<adw::ActionRow> {
        self.results
            .selected_row()
            .and_then(|row| row.downcast().ok())
    }
    fn snapshot(&self) -> Value {
        let focus = self.dialog.focus();
        let rows = self.rows();
        let document = rows
            .iter()
            .filter(|row| {
                [
                    "Copy current text",
                    "Polish text",
                    "Save edits",
                    "Rewrite with an instruction",
                ]
                .contains(&row.title().as_str())
            })
            .map(|row| (row.title().to_string(), json!(row.get_sensitive())))
            .collect::<serde_json::Map<_, _>>();
        json!({"search":self.search.text().as_str(),"focused":focus.is_some_and(|focus| focus == self.search || focus.is_ancestor(&self.search)),"empty":self.empty.get_visible(),"document_actions":document,"selected":self.selected().map(|row| row.title().to_string()),"visible_rows":if self.search.text().is_empty(){Value::Null}else{json!(rows.iter().map(|row| row.title().to_string()).collect::<Vec<_>>())}})
    }
    fn select(&self, title: &str) {
        self.search.set_text(title);
        until(|| self.rows().iter().any(|row| row.title() == title));
        self.results.select_row(Some(
            &self
                .rows()
                .into_iter()
                .find(|row| row.title() == title)
                .unwrap(),
        ));
    }
    fn selected_visible(&self) -> bool {
        self.selected()
            .and_then(|row| row.compute_bounds(&self.scroll))
            .is_some_and(|bounds| {
                bounds.y() >= -1.0
                    && bounds.y() + bounds.height() <= self.scroll.height() as f32 + 1.0
            })
    }
}
fn close(owner: &ApplicationDesktop) {
    key("Escape");
    until(|| owner.shell.window.visible_dialog().is_none());
}
fn choose(owner: &ApplicationDesktop, title: &str) {
    Panel::current(owner).unwrap().select(title);
    key("Return");
    until(|| owner.shell.window.visible_dialog().is_none());
}
pub(super) fn choose_action(owner: &ApplicationDesktop, title: &str) {
    Panel::open(owner);
    choose(owner, title);
}
struct Flow<'a> {
    owner: &'a ApplicationDesktop,
    services: &'a ApplicationServices,
    reference: &'a Value,
    root: &'a Path,
    states: Vec<Value>,
    layouts: Vec<Value>,
    escape_receipts: Rc<RefCell<Vec<&'static str>>>,
}
fn notifications(owner: &ApplicationDesktop) -> Vec<Vec<String>> {
    widgets(&owner.shell.toast_overlay)
        .into_iter()
        .filter(|w| w.is_mapped() && w.accessible_role() == gtk::AccessibleRole::Alert)
        .map(|alert| {
            widgets(&alert)
                .into_iter()
                .filter_map(|w| w.downcast::<gtk::Label>().ok())
                .filter(|label| label.is_mapped())
                .map(|label| label.text().to_string())
                .collect()
        })
        .collect()
}
impl Flow<'_> {
    fn record(&mut self, name: &str) {
        let workspace = &self.owner.capture.page.workspace;
        let entry = workspace.entry();
        let raw = entry.as_ref().map(|entry| {
            self.services
                .history
                .find(&entry.identifier)
                .unwrap()
                .raw_text
        });
        let replies = entry
            .as_ref()
            .map(|entry| {
                self.services
                    .conversations
                    .replies(&entry.identifier)
                    .unwrap()
                    .into_iter()
                    .map(|reply| reply.text)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut actual = json!({"name":name,"page":self.owner.shell.stack.visible_child_name().map(String::from),"panel":Panel::current(self.owner).map(|panel| panel.snapshot()),"recording":self.owner.capture.phase()==Some(CapturePhase::Recording),"clipboard":clipboard(),"raw":raw,"replies":replies,"documents":workspace.documents().iter().map(|view|view.text()).collect::<Vec<_>>()});
        if name.starts_with("settings") {
            actual["notifications"] = json!(notifications(self.owner));
            actual["escape_keys"] = json!(*self.escape_receipts.borrow());
        }
        fs::write(
            self.root.join(format!("{name}.json")),
            serde_json::to_vec_pretty(&actual).unwrap(),
        )
        .unwrap();
        assert_eq!(
            actual,
            self.reference["stages"][self.states.len()],
            "command stage {name}"
        );
        self.states.push(actual);
    }
    fn layout(&mut self, name: &str) {
        let panel = Panel::current(self.owner).unwrap();
        let window = &self.owner.shell.window;
        settle();
        let mut observation = json!({"name":name,"window":[window.width(),window.height()],"panel":[panel.dialog.width(),panel.dialog.height()],"selected_visible":panel.selected_visible(),"selected":panel.selected().map(|row|row.title().to_string())});
        if name == "wide" {
            let workspace = &self.owner.capture.page.workspace;
            observation["workspace_controls"] = json!([
                &workspace.continue_button,
                &workspace.structured_note,
            ]
            .iter()
            .map(|button| {
                let bounds = button.compute_bounds(window).unwrap();
                json!({"label":button.label().map(String::from),"bounds":[bounds.x(),bounds.y(),bounds.width(),bounds.height()]})
            })
            .collect::<Vec<_>>());
        }
        let expected = &self.reference["layouts"][self.layouts.len()];
        fs::write(
            self.root.join(format!("{name}-layout.json")),
            serde_json::to_vec_pretty(&observation).unwrap(),
        )
        .unwrap();
        assert_eq!(observation, *expected, "command layout {name}");
        self.layouts.push(observation);
        capture_window(&self.root.join(format!("{name}.png")));
    }
}

pub fn exercise(
    owner: &Rc<ApplicationDesktop>,
    services: &ApplicationServices,
    application: &adw::Application,
    reference: &Value,
    tools: &Path,
    root: &Path,
) -> u64 {
    let root = root.join("native-application-commands");
    fs::create_dir(&root).unwrap();
    gtk::gdk::Display::default()
        .unwrap()
        .clipboard()
        .set_text("untouched command clipboard");
    let mut flow = Flow {
        owner,
        services,
        reference,
        root: &root,
        states: vec![],
        layouts: vec![],
        escape_receipts: Rc::new(RefCell::new(vec![])),
    };
    Panel::open(owner);
    flow.record("empty");
    close(owner);
    application.activate_action("record", None);
    until(|| {
        owner.capture.phase() == Some(CapturePhase::Recording)
            && tools.join("raw.ready.json").exists()
    });
    let ready: Value =
        serde_json::from_slice(&fs::read(tools.join("raw.ready.json")).unwrap()).unwrap();
    let pid = ready["pid"].as_u64().unwrap();
    Panel::open(owner);
    flow.record("recording");
    close(owner);
    flow.record("dialog-escape-recording");
    let popover = owner.capture.page.live_menu.popover().unwrap();
    popover.popup();
    until(|| popover.is_mapped());
    key("Escape");
    until(|| !popover.is_mapped());
    flow.record("popover-escape-recording");
    assert!(Path::new(&format!("/proc/{pid}")).exists());
    owner.capture.page.record_button.emit_clicked();
    until(|| owner.capture.phase().is_none());
    settle();
    let entry = services.history.recent(1).unwrap().pop().unwrap();
    let reply = services
        .conversations
        .append(
            &entry.identifier,
            "Polish",
            "# A note\n**Keep** this.",
            "fixture",
        )
        .unwrap();
    let workspace = &owner.capture.page.workspace;
    workspace
        .show_conversation(Some(entry.clone()), &[reply], false)
        .unwrap();
    settle();
    let editor = workspace.documents().pop().unwrap();
    editor.grab_focus();
    editor
        .buffer()
        .insert(&mut editor.buffer().end_iter(), " Edited.");
    Panel::open(owner);
    flow.record("editor-focused");
    flow.layout("wide");
    close(owner);
    Panel::open(owner);
    choose(owner, "Copy current text");
    flow.record("copied");
    Panel::open(owner);
    choose(owner, "Save edits");
    flow.record("saved");
    let panel = Panel::open(owner);
    panel.search.set_text("does-not-exist");
    until(|| panel.empty.get_visible());
    key("Return");
    flow.record("empty-search");
    key("Control_L+p");
    until(|| owner.shell.window.visible_dialog().is_none());
    let panel = Panel::open(owner);
    panel.select("Polish text");
    until(|| panel.rows().len() == 1);
    workspace.set_busy(true, "Finishing…");
    key("Return");
    flow.record("busy-stale");
    close(owner);
    workspace.set_busy(false, "Ready");
    application.activate_action("history", None);
    Panel::open(owner);
    flow.record("hidden-document");
    close(owner);
    application.activate_action("latest", None);
    let panel = Panel::open(owner);
    panel.select("Copy current text");
    until(|| panel.rows().len() == 1);
    let other = services
        .history
        .add(HistoryInput::dictation("Other note", "Other note"))
        .unwrap();
    workspace
        .show_conversation(Some(other), &[], false)
        .unwrap();
    key("Return");
    flow.record("changed-document");
    close(owner);
    workspace
        .show_conversation(
            Some(entry.clone()),
            &services.conversations.replies(&entry.identifier).unwrap(),
            false,
        )
        .unwrap();
    // Escape first dismisses a visible notification; without one it navigates.
    // Reach each state through actual Copy and expiry instead of timing a prior toast.
    until(|| notifications(owner).is_empty());
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let received = flow.escape_receipts.clone();
    keys.connect_key_pressed(move |_, key, _, _| {
        if key == gtk::gdk::Key::Escape {
            received.borrow_mut().push("pressed");
        }
        glib::Propagation::Proceed
    });
    let received = flow.escape_receipts.clone();
    keys.connect_key_released(move |_, key, _, _| {
        if key == gtk::gdk::Key::Escape {
            received.borrow_mut().push("released");
        }
    });
    owner.shell.window.add_controller(keys.clone());
    let received = flow.escape_receipts.clone();
    let escape = || {
        let before = received.borrow().len();
        key("Escape");
        until(|| received.borrow().len() == before + 2);
    };
    choose_action(owner, "Copy current text");
    until(|| !notifications(owner).is_empty());
    choose_action(owner, "Settings");
    flow.record("settings");
    escape();
    until(|| notifications(owner).is_empty());
    flow.record("settings-escape");
    escape();
    flow.record("settings-back");
    choose_action(owner, "Copy current text");
    until(|| !notifications(owner).is_empty());
    choose_action(owner, "Settings");
    flow.record("settings-before-expiry");
    until(|| notifications(owner).is_empty());
    flow.record("settings-after-expiry");
    escape();
    flow.record("settings-expired-back");
    owner.shell.window.remove_controller(&keys);
    application.activate_action("settings", None);
    owner.shell.window.set_default_size(480, 680);
    settle();
    let panel = Panel::open(owner);
    flow.record("narrow-first");
    flow.layout("narrow-first");
    for _ in 1..panel
        .rows()
        .iter()
        .filter(|row| row.get_sensitive())
        .count()
    {
        let previous = panel.results.selected_row();
        key("Down");
        until(|| panel.results.selected_row() != previous);
    }
    until(|| panel.selected_visible());
    flow.record("narrow-last");
    flow.layout("narrow-last");
    close(owner);
    owner.shell.window.set_default_size(420, 520);
    settle();
    Panel::open(owner);
    flow.record("minimum");
    flow.layout("minimum");
    close(owner);
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
        serde_json::to_vec_pretty(&json!({"states":flow.states,"layouts":flow.layouts})).unwrap(),
    )
    .unwrap();
    eprintln!(
        "Assembled command workflow: {} released states and {} layouts",
        flow.states.len(),
        flow.layouts.len()
    );
    pid
}
