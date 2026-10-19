//! Shared external projections, with generated identity/timing normalized only.
use gtk::prelude::*;
use mluva_core::history::HistoryEntry;
use mluva_gtk::capture_view::CapturePage;
use serde_json::{Value, json};
use std::path::Path;

pub fn widgets(widget: &impl IsA<gtk::Widget>) -> Vec<gtk::Widget> {
    fn append(widget: gtk::Widget, result: &mut Vec<gtk::Widget>) {
        result.push(widget.clone());
        let mut child = widget.first_child();
        while let Some(current) = child {
            append(current.clone(), result);
            child = current.next_sibling();
        }
    }
    let mut result = vec![];
    append(widget.as_ref().clone(), &mut result);
    result
}
fn buffer(view: &impl IsA<gtk::TextView>) -> String {
    let buffer = view.as_ref().buffer();
    buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), true)
        .into()
}
pub fn entry(entry: Option<&HistoryEntry>) -> Value {
    let Some(entry) = entry else {
        return Value::Null;
    };
    let mut value = serde_json::to_value(entry).unwrap();
    value["identifier"] = json!("generated");
    value["created_at"] = json!("generated");
    if !value["retained_audio_path"].is_null() {
        value["retained_audio_path"] = json!("$AUDIO");
    }
    for key in ["recognition_ms", "enhancement_ms", "delivery_ms"] {
        if !value[key].is_null() {
            value[key] = json!("recorded");
        }
    }
    value
}
pub fn observe(page: &CapturePage, audio: Option<&Path>) -> Value {
    let normalize = |value: Option<glib::GString>| {
        value.map(|value| match audio {
            Some(audio) => value.replace(audio.to_str().unwrap(), "$AUDIO"),
            None => value.into(),
        })
    };
    let record = widgets(&page.record_button);
    json!({"status":page.status.label().to_string(),"tooltip":normalize(page.status.tooltip_text()),"title":page.status_title.label().to_string(),
        "record":{"labels":record.iter().filter_map(|widget|widget.downcast_ref::<gtk::Label>().map(|label|label.label().to_string())).collect::<Vec<_>>(),
            "icons":record.iter().filter_map(|widget|widget.downcast_ref::<gtk::Image>().and_then(|image|image.icon_name()).map(String::from)).collect::<Vec<_>>(),
            "sensitive":page.record_button.get_sensitive(),"suggested":page.record_button.has_css_class("suggested-action"),"destructive":page.record_button.has_css_class("destructive-action")},
        "live":{"visible":page.workspace.live_box.get_visible(),"phase":page.workspace.live_title.label().to_string(),"text":buffer(&page.workspace.live_text)},
        "entry":page.workspace.entry().map(|entry|json!({"raw":entry.raw_text,"output":entry.delivered_text})),
        "callout":{"revealed":page.setup_callout.reveals_child(),"title":page.setup_title.label().to_string(),"body":page.setup_body.label().to_string(),"tooltip":normalize(page.setup_body.tooltip_text())}})
}

/// Declared product changes applied to historical capture expectations only.
#[allow(dead_code)]
pub fn current_contract(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            fields.remove("requires_acceptance");
            if fields.contains_key("raw_text")
                && fields.get("identifier").is_some_and(|v| v == "generated")
            {
                // New labels are rendered from local text instead of stored by a title job.
                fields.insert("title".into(), Value::Null);
            }
            if fields
                .get("callout")
                .is_some_and(|v| v["revealed"] == true && v["title"] == "Capture problem")
            {
                fields.insert(
                    "status".into(),
                    json!("Capture failed. Open History to recover retained audio."),
                );
                fields.insert(
                    "tooltip".into(),
                    json!("Capture failed. Open History to recover retained audio."),
                );
            }
            if fields.contains_key("record") && fields.contains_key("live") {
                fields.remove("output");
            }
            if fields
                .get("guidance")
                .and_then(Value::as_str)
                .is_some_and(|s| s.starts_with("Dictation ready. Automatic copying is off."))
            {
                fields.insert("copied".into(), json!(true));
            }
            for (key, child) in fields.iter_mut() {
                if key == "delivery_outcome" && child == "ready" {
                    *child = json!("copied");
                }
                current_contract(child);
            }
        }
        Value::Array(values) => values.iter_mut().for_each(current_contract),
        Value::String(s) => {
            *s = s
                .replace(
                    "Ready · microphone capture started with Scribe v2 realtime.",
                    "Ready · microphone capture started.",
                )
                .replace(
                    "Finalizing committed ElevenLabs Scribe v2 Realtime text…",
                    "Finalizing recognized text…",
                )
                .replace(
                    "Dictation ready. Automatic copying is off.",
                    "Copied. Paste in the target application.",
                );
        }
        _ => {}
    }
}
