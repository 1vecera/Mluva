//! Shared external projections, with generated identity/timing normalized only.
use gtk::prelude::*;
use mluva_core::history::HistoryEntry;
use mluva_gtk::capture_view::CapturePage;
use serde_json::{Value, json};
use std::path::Path;

fn widgets(widget: &impl IsA<gtk::Widget>) -> Vec<gtk::Widget> {
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
        "output":{"visible":page.output_section.get_visible(),"text":buffer(&page.output_view)},
        "entry":page.workspace.entry().map(|entry|json!({"raw":entry.raw_text,"output":entry.delivered_text})),
        "callout":{"revealed":page.setup_callout.reveals_child(),"title":page.setup_title.label().to_string(),"body":page.setup_body.label().to_string(),"tooltip":normalize(page.setup_body.tooltip_text())}})
}
