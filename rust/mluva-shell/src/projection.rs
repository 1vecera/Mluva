//! Status stays content-free unless the caller explicitly requests the floating preview.
use glib::Variant;
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn integer(value: Option<&Variant>, default: i64, min: i64, max: i64) -> i64 {
    let number = value
        .and_then(|v| {
            v.get::<i64>()
                .or_else(|| v.get::<i32>().map(i64::from))
                .or_else(|| v.get::<u32>().map(i64::from))
                .or_else(|| v.get::<u64>().map(|v| v.min(i64::MAX as u64) as i64))
        })
        .unwrap_or(default);
    number.clamp(min, max)
}
fn boolean(value: Option<&Variant>, default: bool) -> bool {
    value.and_then(Variant::get::<bool>).unwrap_or(default)
}
fn string<'a>(values: &'a BTreeMap<String, Variant>, key: &str) -> &'a str {
    values.get(key).and_then(Variant::str).unwrap_or_default()
}

pub fn state(parameters: &Variant, overlay: bool) -> Value {
    if parameters.type_().as_str() != if overlay { "(a{sv})" } else { "(bssussdss)" } {
        return json!({"phase":"unavailable","elapsed":0});
    }
    if !overlay {
        let phase = parameters.child_get::<String>(1);
        if !parameters.child_get::<bool>(0) || phase == "hidden" {
            return json!({"phase":"idle","elapsed":0});
        }
        let phase = if ["preparing", "recording", "processing", "copied", "error"]
            .contains(&phase.as_str())
        {
            phase.as_str()
        } else {
            "unavailable"
        };
        return json!({"phase":phase, "elapsed":parameters.child_get::<u32>(3).min(86400)});
    }
    let values = parameters.child_get::<BTreeMap<String, Variant>>(0);
    let phase = string(&values, "phase");
    if ![
        "preparing",
        "recording",
        "processing",
        "error",
        "ready",
        "rewriting",
        "review-error",
    ]
    .contains(&phase)
    {
        return json!({"phase":"idle","elapsed":0});
    }
    let preview = string(&values, "preview")
        .split(mluva_core::text::whitespace)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let start = preview.chars().count().saturating_sub(4096);
    let position = string(&values, "widget_position");
    let level = values
        .get("level")
        .and_then(Variant::get::<f64>)
        .unwrap_or(0.0);
    // Python's nested min/max maps NaN to zero, unlike f64::min's NaN handling.
    let level = if level.is_nan() {
        0.0
    } else {
        level.clamp(0.0, 1.0)
    };
    let mut result = json!({
        "phase":phase,
        "elapsed":integer(values.get("elapsed"),0,0,86400),
        "level":level,
        "preview":preview.chars().skip(start).collect::<String>(),
        "preview_start":integer(values.get("preview_start"),0,0,i64::from(i32::MAX)),
        "rewrite_enabled":boolean(values.get("rewrite_enabled"),true),
        "widget_lines":integer(values.get("widget_lines"),5,1,10),
        "widget_opacity":integer(values.get("widget_opacity"),82,10,100),
        "review_timeout":integer(values.get("review_timeout"),4,1,60),
        "show_copy":boolean(values.get("show_copy"),true),
        "smooth_scrolling":boolean(values.get("smooth_scrolling"),true),
        "scroll_duration":integer(values.get("scroll_duration"),800,0,2000),
        "scroll_lookahead":integer(values.get("scroll_lookahead"),2,0,6),
        "widget_position":if ["bottom-left","bottom-center","bottom-right"].contains(&position) {position} else {"bottom-center"}
    });
    if ["ready", "rewriting", "review-error"].contains(&phase) {
        result["identifier"] = json!(
            string(&values, "identifier")
                .chars()
                .take(36)
                .collect::<String>()
        );
        let options = values
            .get("options")
            .and_then(Variant::get::<Vec<(String, String)>>)
            .unwrap_or_default();
        result["options"] = json!(options.into_iter().take(128).map(|(identifier,label)| json!({"value":identifier.chars().take(36).collect::<String>(),"label":label.chars().take(64).collect::<String>()})).collect::<Vec<_>>());
        result["message"] = json!(
            string(&values, "message")
                .chars()
                .take(96)
                .collect::<String>()
        );
    }
    result
}
