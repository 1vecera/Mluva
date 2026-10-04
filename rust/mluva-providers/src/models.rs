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
    pub rewrite_effort: Option<String>,
    pub fast_tier: Option<String>,
    pub reasoning_efforts: Vec<String>,
}

impl Model {
    pub fn from_codex_catalog(row: &Value) -> Result<Self> {
        let invalid =
            || ProviderError::message("Codex app-server returned a malformed model catalog.");
        let string = |key: &str| {
            row.get(key)
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(invalid)
        };
        let efforts = match row.get("supportedReasoningEfforts") {
            None => vec![],
            Some(value) => value
                .as_array()
                .ok_or_else(invalid)?
                .iter()
                .map(|effort| {
                    effort
                        .get("reasoningEffort")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .ok_or_else(invalid)
                })
                .collect::<Result<Vec<_>>>()?,
        };
        let mut fast_tier = None;
        if let Some(tiers) = row.get("serviceTiers") {
            for tier in tiers.as_array().ok_or_else(invalid)? {
                let name = tier
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(invalid)?;
                let id = tier.get("id").and_then(Value::as_str).ok_or_else(invalid)?;
                if caseless::default_case_fold_str(name) == "fast"
                    || matches!(id, "fast" | "priority")
                {
                    fast_tier = Some(id.into());
                    break;
                }
            }
        }
        if fast_tier.is_none()
            && row
                .get("additionalSpeedTiers")
                .and_then(Value::as_array)
                .is_some_and(|tiers| tiers.iter().any(|tier| tier == "fast"))
        {
            fast_tier = Some("fast".into());
        }
        let identifier = string("model")?;
        Ok(Self {
            id: string("id")?,
            name: row.get("displayName").map_or_else(
                || Ok(identifier.clone()),
                |value| value.as_str().map(str::to_owned).ok_or_else(invalid),
            )?,
            identifier,
            is_default: row.get("isDefault").map(truthy).ok_or_else(invalid)?,
            hidden: row.get("hidden").is_some_and(truthy),
            rewrite_effort: if efforts.iter().any(|effort| effort == "low") {
                Some("low".into())
            } else {
                row.get("defaultReasoningEffort")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            },
            fast_tier,
            reasoning_efforts: efforts,
        })
    }
}

pub fn select_codex_model<'a>(models: &'a [Model], requested: Option<&str>) -> Result<&'a Model> {
    if let Some(requested) = requested {
        return models
            .iter()
            .find(|model| model.id == requested || model.identifier == requested)
            .ok_or_else(|| ProviderError::message("The configured Codex model is unavailable."));
    }
    let mut defaults = models.iter().filter(|model| model.is_default);
    let first = defaults.next();
    if defaults.next().is_some() || first.is_none() {
        return Err(ProviderError::message(
            "Codex app-server did not expose exactly one default model.",
        ));
    }
    Ok(first.unwrap())
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

fn effort_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    value.len() <= 32
        && chars
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

pub fn compatible_catalog(
    payload: &Value,
    capability: Option<&str>,
    selected: Option<&str>,
) -> Result<Vec<Model>> {
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
            && match capability {
                Some("speech") => !matches!(mode, "audio_transcription" | "transcription" | "stt"),
                Some("rewrite") => mode != "chat",
                _ => false,
            }
        {
            continue;
        }
        let efforts = info
            .get("supported_reasoning_efforts")
            .or_else(|| row.get("supported_reasoning_efforts"));
        let mut reasoning_efforts = Vec::new();
        if let Some(efforts) = efforts {
            for effort in efforts.as_array().ok_or_else(invalid)? {
                let effort = effort
                    .as_str()
                    .filter(|effort| effort_identifier(effort))
                    .ok_or_else(invalid)?;
                if !reasoning_efforts.iter().any(|existing| existing == effort) {
                    reasoning_efforts.push(effort.into());
                }
            }
        }
        let model = Model {
            id: id.into(),
            identifier: id.into(),
            name: id.into(),
            is_default: Some(id) == selected,
            hidden: false,
            rewrite_effort: None,
            fast_tier: None,
            reasoning_efforts,
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
