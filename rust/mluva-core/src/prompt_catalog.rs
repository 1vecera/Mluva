//! Reviewed task text and stable style identities from the released application.

use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prompt {
    pub identifier: String,
    pub name: String,
    pub purpose: String,
    pub default: String,
    pub built_in: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftTemplate {
    pub name: String,
    pub purpose: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveTemplate {
    pub identifier: String,
    pub name: String,
    pub purpose: String,
    pub instructions: String,
    pub draft: Option<DraftTemplate>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedStyle {
    pub identifier: String,
    pub name: String,
    pub instructions: String,
    pub is_built_in: bool,
}

#[derive(Debug, Deserialize)]
pub struct PromptCatalog {
    pub prompts: Vec<Prompt>,
    pub live_templates: Vec<LiveTemplate>,
    pub styles: Vec<SavedStyle>,
}

pub static DEFAULTS: LazyLock<PromptCatalog> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../resources/default-prompts.json"))
        .expect("reviewed prompt resource")
});
