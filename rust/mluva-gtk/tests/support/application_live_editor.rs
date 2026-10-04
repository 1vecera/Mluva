//! Held finalization in the assembled application, including actual storage/copy faults.
use super::{Signals, clipboard, records, settle_for, until, widgets};
use adw::prelude::*;
use mluva_gtk::{application::ApplicationDesktop, conversation_view::ConversationWorkspace};
use mluva_workflows::{capture::CapturePhase, services::ApplicationServices};
use serde_json::{Value, json};
use std::{
    cell::RefCell, collections::BTreeMap, fs, os::unix::fs::PermissionsExt, path::Path, rc::Rc,
    time::Duration,
};

fn copies(w: &ConversationWorkspace) -> Vec<gtk::Button> {
    widgets(&w.messages)
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
        .filter(|button| button.icon_name().as_deref() == Some("edit-copy-symbolic"))
        .collect()
}
fn geometry(w: &ConversationWorkspace, window: &adw::ApplicationWindow) -> Value {
    let bounds = w.live_box.compute_bounds(window).unwrap();
    json!({"mapped":w.live_box.is_mapped() && w.live_draft_text.is_mapped(),"bounds":[bounds.x(),bounds.y(),bounds.width(),bounds.height()],"draft_scroll":w.live_draft_scroll.vadjustment().value(),"source_scroll":w.live_scroll.vadjustment().value()})
}
struct Flow<'a> {
    owner: &'a ApplicationDesktop,
    services: &'a ApplicationServices,
    reference: &'a Value,
    output: &'a Path,
    events: &'a Signals,
    stage: usize,
}
impl Flow<'_> {
    fn snapshot(&mut self, name: &str) {
        settle_for(Duration::from_millis(100));
        let w = &self.owner.capture.page.workspace;
        let events = self.events.borrow();
        let shell = events
            .iter()
            .rev()
            .find(|(name, _)| name == "ShellStateChanged")
            .unwrap()
            .1
            .child_value(0)
            .get::<BTreeMap<String, glib::Variant>>()
            .unwrap();
        let review = ["phase", "message", "preview", "show_copy"]
            .into_iter()
            .map(|key| {
                let value = shell.get(key).map_or(Value::Null, |v| {
                    if v.type_().as_str() == "b" {
                        json!(v.get::<bool>().unwrap())
                    } else {
                        json!(v.get::<String>().unwrap())
                    }
                });
                (key.to_owned(), value)
            })
            .collect::<serde_json::Map<_, _>>();
        drop(events);
        let history = self.services.history.recent(100).unwrap().iter().map(|e| json!({"raw":e.raw_text,"output":e.delivered_text,"replies":self.services.conversations.replies(&e.identifier).unwrap().iter().map(|r| json!({"text":r.text,"instruction":r.instruction})).collect::<Vec<_>>()})).collect::<Vec<_>>();
        let value = json!({"name":name,"live_active":self.owner.live.active(),"viewing_live":w.is_viewing_live(),"live_mapped":w.live_box.is_mapped(),"draft_mapped":w.live_draft_text.is_mapped(),"cancel_mapped":w.live_cancel.is_mapped(),"source":w.live_text.text(),"draft":w.live_draft(),"documents":w.documents().iter().map(|v|v.text()).collect::<Vec<_>>(),"notice":w.notice.label().as_str(),"clipboard":clipboard(),"history":history,"copy_buttons":copies(w).iter().map(|b|json!({"mapped":b.is_mapped(),"sensitive":b.get_sensitive()})).collect::<Vec<_>>(),"review":review});
        fs::write(
            self.output.join(format!("{name}.json")),
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();
        let expected = &self.reference["stages"][self.stage];
        let differences = value
            .as_object()
            .unwrap()
            .keys()
            .filter(|key| value[*key] != expected[*key])
            .collect::<Vec<_>>();
        assert!(
            differences.is_empty(),
            "Live editor {} stage {name}: {differences:?}; see {}",
            self.reference["name"],
            self.output.display()
        );
        self.stage += 1;
        println!("LIVE_EDITOR_STAGE {} {name} PASS", self.reference["name"]);
    }
}

pub fn exercise(
    owner: &ApplicationDesktop,
    services: &ApplicationServices,
    reference: &Value,
    tools: &Path,
    root: &Path,
    evidence: &Path,
    events: &Signals,
) -> Vec<u64> {
    let name = reference["name"].as_str().unwrap();
    let w = &owner.capture.page.workspace;
    let output = root.join(format!("native-live-editor-{name}"));
    fs::create_dir(&output).unwrap();
    let mut flow = Flow {
        owner,
        services,
        reference,
        output: &output,
        events,
        stage: 0,
    };
    let gate = evidence.join("final.release");
    let source = reference["source"].as_str().unwrap();
    let preview = reference["preview"].as_str().unwrap();
    let draft = reference["draft"].as_str().unwrap();
    let final_text = reference["final"].as_str().unwrap();
    let edit = reference["edit"].as_str().unwrap();
    let provider = |result: &str| {
        let spec = json!({"scenario":"clean","evidence":evidence,"live_controls":{format!("preview|{preview}"):{"deltas":[draft]},format!("final|{source}"):{"deltas":[result],"gate":gate}}});
        fs::write(
            root.join("application-codex.json"),
            serde_json::to_vec(&spec).unwrap(),
        )
        .unwrap();
    };
    provider(final_text);
    let mut pcm = reference["pcm"].clone();
    pcm["pcm_hex"] = json!(
        pcm["pcm_hex"]
            .as_str()
            .unwrap()
            .repeat(reference["pcm_repeats"].as_u64().unwrap() as usize)
    );
    fs::write(
        tools.join("test-config.json"),
        serde_json::to_vec(&pcm).unwrap(),
    )
    .unwrap();
    gtk::gdk::Display::default()
        .unwrap()
        .clipboard()
        .set_text("untouched Live editor clipboard");
    owner.capture.page.record_button.emit_clicked();
    until(|| {
        owner.capture.phase() == Some(CapturePhase::Recording)
            && tools.join("raw.ready.json").exists()
    });
    let ready: Value =
        serde_json::from_slice(&fs::read(tools.join("raw.ready.json")).unwrap()).unwrap();
    let pid = ready["pid"].as_u64().unwrap();
    until(|| w.live_draft() == draft);
    settle_for(Duration::from_millis(300));
    w.live_draft_scroll.vadjustment().set_value(80.0);
    w.live_scroll.vadjustment().set_value(90.0);
    settle_for(Duration::from_millis(100));
    flow.snapshot("recording-draft");
    let frames = Rc::new(RefCell::new(vec![geometry(w, &owner.shell.window)]));
    let sampled = frames.clone();
    let workspace = w.clone();
    let tick = owner.shell.window.add_tick_callback(move |window, _| {
        sampled.borrow_mut().push(geometry(&workspace, window));
        glib::ControlFlow::Continue
    });
    let turns = || {
        records(&evidence.join("requests.jsonl"))
            .iter()
            .filter(|r| r["message"]["method"] == "turn/start")
            .count()
    };
    owner.capture.page.record_button.emit_clicked();
    until(|| turns() == 2 && owner.capture.phase().is_none());
    let held_at = frames.borrow().len();
    until(|| {
        frames.borrow().len()
            >= held_at + reference["minimum_held_frames"].as_u64().unwrap() as usize
    });
    tick.remove();
    fs::write(
        output.join("frames.json"),
        serde_json::to_vec_pretty(&*frames.borrow()).unwrap(),
    )
    .unwrap();
    for (index, frame) in frames.borrow().iter().enumerate() {
        assert_eq!(
            *frame,
            reference["geometry"],
            "Live editor {name}, frame {index}; see {}",
            output.display()
        );
    }
    let entry = services.history.recent(100).unwrap().remove(0);
    flow.snapshot("final-pending");
    owner.shell.window.application().unwrap().activate_action(
        "review",
        Some(&("copy", entry.identifier.as_str(), "").to_variant()),
    );
    flow.snapshot("review-copy-refused");
    if name == "manual-edit" {
        let buffer = w.live_draft_text.buffer();
        buffer.insert(&mut buffer.end_iter(), edit);
        provider(&format!("{final_text}{edit}"));
        flow.snapshot("edited-during-finalization");
    } else if name == "save-failure" {
        rusqlite::Connection::open(&services.history.database.path).unwrap().execute_batch("CREATE TRIGGER reject_live_append BEFORE INSERT ON conversation_rewrites BEGIN SELECT RAISE(ABORT, 'Controlled save failure'); END").unwrap();
    } else if name == "copy-failure" {
        let path = evidence.join("copy-attempt");
        let path = path.to_str().unwrap().replace('\'', "'\\''");
        fs::write(
            tools.join("xclip"),
            format!("#!/bin/sh\ncat > '{path}'\nexit 17\n"),
        )
        .unwrap();
        fs::set_permissions(tools.join("xclip"), fs::Permissions::from_mode(0o700)).unwrap();
    } else if name == "cancel" {
        w.live_cancel.emit_clicked();
        until(|| !owner.live.active());
        flow.snapshot("cancelled-while-held");
        for process in records(&evidence.join("process.jsonl")) {
            until(|| !Path::new(&format!("/proc/{}", process["pid"])).exists());
        }
    }
    fs::write(&gate, "").unwrap();
    until(|| !owner.live.active());
    settle_for(Duration::from_millis(300));
    flow.snapshot("terminal");
    if name == "manual-edit" {
        gtk::gdk::Display::default()
            .unwrap()
            .clipboard()
            .set_text("review Copy positive control");
        owner.shell.window.application().unwrap().activate_action(
            "review",
            Some(&("copy", entry.identifier.as_str(), "").to_variant()),
        );
        until(|| clipboard().as_deref() == Some(format!("{final_text}{edit}").as_str()));
        flow.snapshot("review-copy-after-final");
    }
    if matches!(name, "save-failure" | "copy-failure") {
        if name == "save-failure" {
            rusqlite::Connection::open(&services.history.database.path)
                .unwrap()
                .execute_batch("DROP TRIGGER reject_live_append")
                .unwrap();
        } else {
            assert_eq!(
                fs::read_to_string(evidence.join("copy-attempt")).unwrap(),
                final_text
            );
            fs::remove_file(tools.join("xclip")).unwrap();
        }
        let buttons = copies(w);
        assert_eq!(buttons.len(), 2);
        buttons[1].emit_clicked();
        until(|| clipboard().as_deref() == Some(final_text));
        flow.snapshot("manual-copy-recovery");
    }
    assert_eq!(flow.stage, reference["stages"].as_array().unwrap().len());
    vec![pid]
}
