//! Live delivery/editing through the existing application and real browser owner.
use super::{Portal, capture_ui, settle, text_transport, until};
use adw::prelude::*;
use glib::variant::ToVariant;
use mluva_core::config::AppConfig;
use mluva_gtk::application::ApplicationDesktop;
use mluva_workflows::services::ApplicationServices;
use serde_json::{Value, json};
use std::path::Path;

pub fn observe(owner: &ApplicationDesktop, services: &ApplicationServices) -> Value {
    let w = &owner.capture.page.workspace;
    let control = &owner.capture.page.live_mode;
    let preferences = |value: &AppConfig| json!({"enabled":value.live_rewrite_enabled,"continuous":value.live_rewrite_continuous,"template":value.live_rewrite_template});
    let replies = services.history.recent(100).unwrap().into_iter().rev().map(|entry| {
        services.conversations.replies(&entry.identifier).unwrap().into_iter()
            .map(|reply| json!({"text":reply.text,"instruction":reply.instruction,"model":reply.model}))
            .collect::<Vec<_>>()
    }).collect::<Vec<_>>();
    let copies = capture_ui::widgets(&w.messages)
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
        .filter(|button| button.icon_name().as_deref() == Some("edit-copy-symbolic"))
        .map(|button| json!({"mapped":button.is_mapped(),"sensitive":button.get_sensitive()}))
        .collect::<Vec<_>>();
    json!({"active":owner.live.active(),"finalizing":owner.live.finalizing(),"viewing_live":w.is_viewing_live(),"draft":w.live_draft(),"draft_status":w.live_draft_status.label().as_str(),"draft_mapped":w.live_draft_text.is_mapped(),"documents":w.documents().iter().map(|v|v.text()).collect::<Vec<_>>(),"notice":w.notice.label().as_str(),"replies":replies,"copy_buttons":copies,"control":{"label":control.label().map(String::from),"active":control.is_active(),"sensitive":control.get_sensitive()},"preferences":preferences(&services.config()),"persisted":preferences(&AppConfig::load(&services.paths.config.join("config.json")).unwrap())})
}

pub fn exercise(
    owner: &ApplicationDesktop,
    portal: &Portal,
    peer: &mut text_transport::Peer,
    evidence: &Path,
    row: &Value,
    audio: &Path,
    mut stage: impl FnMut(&str, Option<&Path>, &text_transport::Peer),
) {
    let page = &owner.capture.page;
    let w = &page.workspace;
    let application = owner.shell.window.application().unwrap();
    let operation = row["params"]["live"].as_str().unwrap();
    let edit = row["edit"].as_str().unwrap();
    let release =
        |name: &str| std::fs::write(evidence.join(format!("{name}.release")), b"").unwrap();
    until(|| w.live_draft() == row["responses"][1]["text"].as_str().unwrap());
    settle();
    stage("provisional-draft", Some(audio), peer);
    if matches!(operation, "pause" | "cancel") {
        let buffer = w.live_draft_text.buffer();
        buffer.insert(&mut buffer.end_iter(), edit);
        stage("manual-edit", Some(audio), peer);
        page.live_templates["grilling"].set_active(true);
        until(|| evidence.join("preview.request").exists());
        settle();
        stage("preview-pending", Some(audio), peer);
        if operation == "pause" {
            page.live_mode.set_active(false);
        } else {
            portal.drive(json!({"op":"Activated","id":"cancel-capture"}));
        }
        settle();
        stage(
            if operation == "pause" {
                "paused"
            } else {
                "cancelled"
            },
            Some(audio),
            peer,
        );
        release("preview");
        text_transport::settle(std::time::Duration::from_millis(500));
        stage("late-result-rejected", Some(audio), peer);
    }
    if operation == "cancel" {
        return;
    }
    if operation == "stale" {
        peer.request(json!({"operation":"focus","kind":"second"}));
        stage("target-changed", Some(audio), peer);
    }
    portal.drive(json!({"op":"Deactivated","id":"toggle-recording-f9"}));
    portal.drive(json!({"op":"Activated","id":"toggle-recording-f9"}));
    until(|| owner.capture.phase().is_none());
    if operation != "pause" {
        until(|| evidence.join("final.request").exists() && owner.live.finalizing());
        settle();
        stage("final-pending", Some(audio), peer);
        let entry = w.store.history.recent(1).unwrap().remove(0);
        application.activate_action(
            "review",
            Some(&("copy", entry.identifier.as_str(), "").to_variant()),
        );
        settle();
        stage("review-copy-refused", Some(audio), peer);
        if operation == "edit" {
            w.live_draft_text.grab_focus();
            let buffer = w.live_draft_text.buffer();
            buffer.insert(&mut buffer.end_iter(), edit);
            settle();
            stage("edited-during-finalization", Some(audio), peer);
        }
        release("final");
        if operation == "edit" {
            until(|| evidence.join("edited.request").exists());
            settle();
            stage("edit-kept-for-reconciliation", Some(audio), peer);
            release("edited");
        }
    }
    until(|| !owner.live.active());
    text_transport::settle(std::time::Duration::from_millis(300));
    stage("completed", Some(audio), peer);
    application.activate_action("history", None);
    settle();
    portal.drive(json!({"op":"Deactivated","id":"toggle-recording-f9"}));
    portal.drive(json!({"op":"Activated","id":"open-rewrite"}));
    until(|| owner.shell.stack.visible_child_name().as_deref() == Some("capture"));
    settle();
    stage("latest-conversation", Some(audio), peer);
}
