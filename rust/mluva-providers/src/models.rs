use crate::{ProviderError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::LazyLock;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub identifier: String,
    pub name: String,
    pub is_default: bool,
    pub hidden: bool,
}

static PRINTABLE: LazyLock<Vec<(u32, u32)>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../resources/printable-ranges.json"))
        .expect("frozen printable properties")
});

pub fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= 200
        && value.chars().all(|character| {
            let code = u32::from(character);
            let index = PRINTABLE.partition_point(|(_, last)| *last < code);
            PRINTABLE
                .get(index)
                .is_some_and(|(first, last)| (*first..=*last).contains(&code))
                && !mluva_core::text::whitespace(character)
        })
}

pub fn valid_catalog_name(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= 200
        && value.chars().all(|character| {
            let code = u32::from(character);
            let index = PRINTABLE.partition_point(|(_, last)| *last < code);
            PRINTABLE
                .get(index)
                .is_some_and(|(first, last)| (*first..=*last).contains(&code))
        })
}

pub(crate) fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
        Value::Number(value) => value.as_f64() != Some(0.0),
    }
}

pub(crate) fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Bool(value) => Some(f64::from(u8::from(*value))),
        _ => value.as_f64(),
    }
}

pub fn compatible_catalog(payload: &Value, selected: Option<&str>) -> Result<Vec<Model>> {
    let invalid = || ProviderError::message("The provider returned an invalid model catalog.");
    let rows = payload
        .get("data")
        .and_then(Value::as_array)
        .filter(|rows| rows.len() <= 2_000)
        .ok_or_else(invalid)?;
    let mut models = Vec::new();
    let mut positions = HashMap::new();
    for row in rows {
        let id = row
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| valid_identifier(id))
            .ok_or_else(invalid)?;
        let info = match row.get("model_info") {
            Some(value) => value.as_object().ok_or_else(invalid)?,
            None => {
                static EMPTY: LazyLock<serde_json::Map<String, Value>> =
                    LazyLock::new(serde_json::Map::new);
                &EMPTY
            }
        };
        let mode = info.get("mode").or_else(|| row.get("mode"));
        if mode.is_some_and(|value| !value.is_null() && !value.is_string()) {
            return Err(invalid());
        }
        if let Some(mode) = mode.and_then(Value::as_str).filter(|mode| !mode.is_empty())
            && !matches!(mode, "audio_transcription" | "transcription" | "stt")
        {
            continue;
        }
        let model = Model {
            id: id.into(),
            identifier: id.into(),
            name: id.into(),
            is_default: Some(id) == selected,
            hidden: false,
        };
        if let Some(&index) = positions.get(id) {
            models[index] = model;
        } else {
            positions.insert(id, models.len());
            models.push(model);
        }
    }
    Ok(models)
}
