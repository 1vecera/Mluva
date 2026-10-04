//! Stable JSON spacing and property order for released prompt and export contracts.

use serde_json::Value;

pub fn spaced(value: &Value) -> String {
    match value {
        Value::Array(values) => format!(
            "[{}]",
            values.iter().map(spaced).collect::<Vec<_>>().join(", ")
        ),
        Value::Object(values) => format!(
            "{{{}}}",
            values
                .iter()
                .map(|(key, value)| format!(
                    "{}: {}",
                    serde_json::to_string(key).unwrap(),
                    spaced(value)
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        _ => serde_json::to_string(value).unwrap(),
    }
}
