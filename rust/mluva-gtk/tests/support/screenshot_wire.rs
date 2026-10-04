use serde_json::{Value, json};

/// Only narration time is sampled; ordered image bytes and prompt text stay exact.
pub fn normalize(value: &mut Value) {
    match value {
        Value::Array(values) => values.iter_mut().for_each(normalize),
        Value::Object(values) => values.values_mut().for_each(normalize),
        Value::String(text) if text.contains("The attached screenshots are visual context") => {
            let (before, after) = text.rsplit_once('\n').unwrap();
            let mut metadata: Value = serde_json::from_str(after).unwrap();
            for image in metadata.as_array_mut().unwrap() {
                if let Some(offset) = image["captured_after_seconds"].as_f64() {
                    assert!((0.0..8.0).contains(&offset));
                    image["captured_after_seconds"] = json!("sampled");
                }
            }
            *text = format!("{before}\n{}", mluva_core::json::spaced(&metadata));
        }
        _ => {}
    }
}
