//! Explicit vocabulary, snippets and output styles, with compatible private persistence.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use caseless::default_case_fold_str as fold;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::database::{StoreResult, invalid};
use crate::private_files::atomic_write_private;
use crate::prompt_catalog::{DEFAULTS, SavedStyle};
use crate::prompts::PromptStore;
use crate::text;

pub const MAX_DICTIONARY_ENTRIES: usize = 1_000;
pub const MAX_SNIPPETS: usize = 500;
pub const MAX_CUSTOM_STYLES: usize = 100;
const MODES: [&str; 3] = ["dictation", "command", "scratchpad"];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DictionaryCaseBehavior {
    #[default]
    #[serde(rename = "fixed")]
    Fixed,
    #[serde(rename = "matchSpoken")]
    MatchSpoken,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DictionaryReplacement {
    #[serde(rename = "id")]
    pub identifier: String,
    pub spoken: String,
    pub written: String,
    #[serde(rename = "bundleIdentifier")]
    pub application_identifier: Option<String>,
    #[serde(rename = "caseBehavior")]
    pub case_behavior: DictionaryCaseBehavior,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snippet {
    #[serde(rename = "id")]
    pub identifier: String,
    pub trigger: String,
    pub expansion: String,
    #[serde(rename = "typedTrigger")]
    pub typed_trigger: Option<String>,
    #[serde(rename = "bundleIdentifier")]
    pub application_identifier: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PersonalizationState {
    pub dictionary: Vec<DictionaryReplacement>,
    pub snippets: Vec<Snippet>,
    pub custom_styles: Vec<SavedStyle>,
    pub default_style_identifier: Option<String>,
    pub application_style_identifiers: BTreeMap<String, String>,
    pub application_style_disabled_identifiers: BTreeSet<String>,
    pub application_modes: BTreeMap<String, String>,
    pub application_provider_preferences: BTreeMap<String, String>,
    pub dismissed_vocabulary_suggestion_identifiers: BTreeSet<String>,
}

impl PersonalizationState {
    pub fn document(&self) -> Value {
        json!({
            "schemaVersion": 1,
            "dictionary": self.dictionary,
            "snippets": self.snippets,
            "styles": self.custom_styles.iter().map(|style| json!({
                "id": style.identifier, "name": style.name,
                "instructions": style.instructions, "isBuiltIn": false,
            })).collect::<Vec<_>>(),
            "defaultStyleID": self.default_style_identifier,
            "applicationStyleIDs": self.application_style_identifiers,
            "applicationStyleDisabled": self.application_style_disabled_identifiers,
            "applicationModes": self.application_modes,
            "applicationProviderPreferences": self.application_provider_preferences,
            "dismissedVocabularySuggestionIDs": self.dismissed_vocabulary_suggestion_identifiers,
        })
    }

    fn decode(payload: &Value) -> StoreResult<Self> {
        let object = payload
            .as_object()
            .ok_or_else(|| invalid("Personalization document must be a JSON object"))?;
        let rows = |label: &str| -> StoreResult<Vec<&Value>> {
            match object.get(label) {
                None => Ok(Vec::new()),
                Some(Value::Array(values)) if values.iter().all(Value::is_object) => {
                    Ok(values.iter().collect())
                }
                _ => Err(invalid(format!("{label} must be an array of objects"))),
            }
        };
        let dictionary = rows("dictionary")?
            .into_iter()
            .map(|item| {
                let behavior = match item.get("caseBehavior") {
                    None | Some(Value::String(_)) => item
                        .get("caseBehavior")
                        .and_then(Value::as_str)
                        .unwrap_or("fixed"),
                    _ => return Err(invalid("Dictionary caseBehavior must be a string")),
                };
                Ok(DictionaryReplacement {
                    identifier: required_identifier(item.get("id"))?,
                    spoken: required_value(item.get("spoken"), "Spoken phrase", 200)?,
                    written: required_value(item.get("written"), "Written replacement", 2_000)?,
                    application_identifier: optional_text(
                        item.get("bundleIdentifier"),
                        "Application identifier",
                        2_048,
                    )?,
                    case_behavior: match behavior {
                        "fixed" => DictionaryCaseBehavior::Fixed,
                        "matchSpoken" => DictionaryCaseBehavior::MatchSpoken,
                        _ => return Err(invalid("Unsupported dictionary case behavior")),
                    },
                })
            })
            .collect::<StoreResult<Vec<_>>>()?;
        let snippets = rows("snippets")?
            .into_iter()
            .map(|item| {
                let typed = match item.get("typedTrigger") {
                    None | Some(Value::Null) => None,
                    Some(Value::String(value)) => Some(value.as_str()),
                    _ => return Err(invalid("Typed trigger must be text")),
                };
                Ok(Snippet {
                    identifier: required_identifier(item.get("id"))?,
                    trigger: required_value(item.get("trigger"), "Spoken trigger", 200)?,
                    expansion: required_value(item.get("expansion"), "Snippet expansion", 20_000)?,
                    typed_trigger: typed_trigger(typed)?,
                    application_identifier: optional_text(
                        item.get("bundleIdentifier"),
                        "Application identifier",
                        2_048,
                    )?,
                })
            })
            .collect::<StoreResult<Vec<_>>>()?;
        let mut custom_styles = Vec::new();
        for item in rows("styles")? {
            let style = SavedStyle {
                identifier: required_identifier(item.get("id"))?,
                name: required_value(item.get("name"), "Style name", 100)?,
                instructions: required_value(
                    item.get("instructions"),
                    "Style instructions",
                    8_000,
                )?,
                is_built_in: false,
            };
            if !DEFAULTS.styles.iter().any(|built| {
                fold(&built.identifier) == fold(&style.identifier)
                    || fold(&built.name) == normalized_phrase(&style.name)
            }) {
                custom_styles.push(style);
            }
        }
        for (length, bound, label) in [
            (dictionary.len(), MAX_DICTIONARY_ENTRIES, "dictionary"),
            (snippets.len(), MAX_SNIPPETS, "snippets"),
            (custom_styles.len(), MAX_CUSTOM_STYLES, "styles"),
        ] {
            if length > bound {
                return Err(invalid(format!(
                    "Personalization {label} exceeds its supported bound"
                )));
            }
        }
        let available: BTreeMap<_, _> = DEFAULTS
            .styles
            .iter()
            .chain(&custom_styles)
            .map(|style| (fold(&style.identifier), style.identifier.clone()))
            .collect();
        let default = match object.get("defaultStyleID") {
            None | Some(Value::Null) => None,
            Some(Value::String(value)) => Some(identifier(value)?),
            _ => return Err(invalid("Style identifiers must be strings")),
        };
        let mut application_style_identifiers = BTreeMap::new();
        for (application, style) in
            string_mapping(object.get("applicationStyleIDs"), "applicationStyleIDs")?
        {
            let application = required_text(&application, "Application identifier", 2_048)?;
            if let Ok(style) = identifier(&style)
                && let Some(style) = available.get(&fold(&style))
            {
                application_style_identifiers.insert(application, style.clone());
            }
        }
        let mut disabled = BTreeSet::new();
        for item in string_set(
            object.get("applicationStyleDisabled"),
            "applicationStyleDisabled",
        )? {
            disabled.insert(required_text(&item, "Application identifier", 2_048)?);
        }
        let mut modes = string_mapping(object.get("applicationModes"), "applicationModes")?;
        modes.retain(|_, mode| MODES.contains(&mode.as_str()));
        Ok(Self {
            dictionary,
            snippets,
            custom_styles,
            default_style_identifier: default
                .and_then(|style| available.get(&fold(&style)).cloned()),
            application_style_identifiers,
            application_style_disabled_identifiers: disabled,
            application_modes: modes,
            application_provider_preferences: string_mapping(
                object.get("applicationProviderPreferences"),
                "applicationProviderPreferences",
            )?,
            dismissed_vocabulary_suggestion_identifiers: string_set(
                object.get("dismissedVocabularySuggestionIDs"),
                "dismissedVocabularySuggestionIDs",
            )?,
        })
    }
}

#[derive(Clone, Debug)]
pub struct PersonalizationStore {
    pub path: PathBuf,
    state: PersonalizationState,
    pub persistence_error: Option<String>,
    pub prompt_store: Option<PromptStore>,
}

impl PersonalizationStore {
    pub fn new(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref().to_owned();
        let loaded = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<Value>(&bytes)
                .map_err(Into::into)
                .and_then(|value| PersonalizationState::decode(&value)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(PersonalizationState::default())
            }
            Err(error) => Err(error.into()),
        };
        let (state, persistence_error) = match loaded {
            Ok(state) => (state, None),
            Err(error) => (PersonalizationState::default(), Some(error.to_string())),
        };
        Self {
            path,
            state,
            persistence_error,
            prompt_store: None,
        }
    }

    pub fn state(&self) -> &PersonalizationState {
        &self.state
    }

    /// Resolve prompt-file overrides without changing saved style identities or selections.
    pub fn styles(&self) -> StoreResult<Vec<SavedStyle>> {
        let mut styles = DEFAULTS
            .styles
            .iter()
            .chain(&self.state.custom_styles)
            .cloned()
            .collect::<Vec<_>>();
        if let Some(prompts) = &self.prompt_store {
            for style in &mut styles {
                let key = format!("style-{}", style.identifier.to_ascii_lowercase());
                if prompts
                    .catalog()
                    .iter()
                    .any(|prompt| prompt.identifier == key)
                {
                    style.instructions = prompts.read(&key)?.text;
                }
            }
        }
        Ok(styles)
    }

    pub fn style(&self, id: Option<&str>) -> StoreResult<Option<SavedStyle>> {
        let Some(id) = id else {
            return Ok(None);
        };
        Ok(self
            .styles()?
            .into_iter()
            .find(|style| fold(&style.identifier) == fold(id)))
    }

    pub fn recognition_context(&self) -> Vec<String> {
        context(&self.state.dictionary, &self.state.snippets)
    }

    pub fn scoped_recognition_context(
        &self,
        application: Option<&str>,
    ) -> StoreResult<Vec<String>> {
        Ok(context(
            &self.scoped_dictionary(application)?,
            &self.scoped_snippets(application)?,
        ))
    }

    fn commit(&mut self, state: PersonalizationState) -> StoreResult<()> {
        if let Some(error) = &self.persistence_error {
            return Err(invalid(format!(
                "Personalization changes are disabled until the malformed document is repaired: {error}"
            )));
        }
        fn sorted(value: Value) -> Value {
            match value {
                Value::Array(values) => Value::Array(values.into_iter().map(sorted).collect()),
                Value::Object(values) => Value::Object(
                    values
                        .into_iter()
                        .map(|(key, value)| (key, sorted(value)))
                        .collect::<BTreeMap<_, _>>()
                        .into_iter()
                        .collect(),
                ),
                other => other,
            }
        }
        let mut content = serde_json::to_vec_pretty(&sorted(state.document()))?;
        content.push(b'\n');
        atomic_write_private(&self.path, &content)?;
        self.state = state;
        Ok(())
    }

    pub fn save_dictionary_replacement(
        &mut self,
        spoken: &str,
        written: &str,
        application: Option<&str>,
        case_behavior: DictionaryCaseBehavior,
    ) -> StoreResult<DictionaryReplacement> {
        let spoken = required_text(spoken, "Spoken phrase", 200)?;
        let written = required_text(written, "Written replacement", 2_000)?;
        let application = scope(application)?;
        let index = self.state.dictionary.iter().position(|rule| {
            normalized_phrase(&rule.spoken) == normalized_phrase(&spoken)
                && rule.application_identifier == application
        });
        if index.is_none() && self.state.dictionary.len() >= MAX_DICTIONARY_ENTRIES {
            return Err(invalid("Dictionary supports at most 1,000 entries"));
        }
        let rule = DictionaryReplacement {
            identifier: index
                .map(|index| self.state.dictionary[index].identifier.clone())
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
            spoken,
            written,
            application_identifier: application,
            case_behavior,
        };
        let mut next = self.state.clone();
        if let Some(index) = index {
            next.dictionary[index] = rule.clone();
        } else {
            next.dictionary.push(rule.clone());
        }
        self.commit(next)?;
        Ok(rule)
    }

    pub fn delete_dictionary_replacement(&mut self, id: &str) -> StoreResult<()> {
        let mut next = self.state.clone();
        next.dictionary.retain(|rule| rule.identifier != id);
        if next.dictionary.len() == self.state.dictionary.len() {
            return Err(invalid("Dictionary entry no longer exists"));
        }
        self.commit(next)
    }

    pub fn save_snippet(
        &mut self,
        trigger: &str,
        expansion: &str,
        typed: Option<&str>,
        application: Option<&str>,
    ) -> StoreResult<Snippet> {
        let trigger = required_text(trigger, "Spoken trigger", 200)?;
        let expansion = required_text(expansion, "Snippet expansion", 20_000)?;
        let typed = typed_trigger(typed)?;
        let application = scope(application)?;
        let index = self.state.snippets.iter().position(|item| {
            item.application_identifier == application
                && (normalized_phrase(&item.trigger) == normalized_phrase(&trigger)
                    || (typed.is_some() && item.typed_trigger == typed))
        });
        if index.is_none() && self.state.snippets.len() >= MAX_SNIPPETS {
            return Err(invalid("Snippets support at most 500 entries"));
        }
        let snippet = Snippet {
            identifier: index
                .map(|index| self.state.snippets[index].identifier.clone())
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
            trigger,
            expansion,
            typed_trigger: typed,
            application_identifier: application,
        };
        let mut next = self.state.clone();
        if let Some(index) = index {
            next.snippets[index] = snippet.clone();
        } else {
            next.snippets.push(snippet.clone());
        }
        self.commit(next)?;
        Ok(snippet)
    }

    pub fn delete_snippet(&mut self, id: &str) -> StoreResult<()> {
        let mut next = self.state.clone();
        next.snippets.retain(|item| item.identifier != id);
        if next.snippets.len() == self.state.snippets.len() {
            return Err(invalid("Snippet no longer exists"));
        }
        self.commit(next)
    }

    pub fn dismiss_vocabulary_suggestion(&mut self, id: &str) -> StoreResult<()> {
        if id.is_empty() {
            return Err(invalid("Vocabulary suggestion identifier cannot be empty"));
        }
        if self
            .state
            .dismissed_vocabulary_suggestion_identifiers
            .contains(id)
        {
            return Ok(());
        }
        let mut next = self.state.clone();
        next.dismissed_vocabulary_suggestion_identifiers
            .insert(id.into());
        self.commit(next)
    }

    pub fn save_style(&mut self, name: &str, instructions: &str) -> StoreResult<SavedStyle> {
        let name = required_text(name, "Style name", 100)?;
        let instructions = required_text(instructions, "Style instructions", 8_000)?;
        let key = normalized_phrase(&name);
        if DEFAULTS.styles.iter().any(|style| fold(&style.name) == key) {
            return Err(invalid("Built-in style names cannot be replaced"));
        }
        let index = self
            .state
            .custom_styles
            .iter()
            .position(|style| normalized_phrase(&style.name) == key);
        if index.is_none() && self.state.custom_styles.len() >= MAX_CUSTOM_STYLES {
            return Err(invalid("Custom styles support at most 100 entries"));
        }
        let style = SavedStyle {
            identifier: index
                .map(|index| self.state.custom_styles[index].identifier.clone())
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
            name,
            instructions,
            is_built_in: false,
        };
        let mut next = self.state.clone();
        if let Some(index) = index {
            next.custom_styles[index] = style.clone();
        } else {
            next.custom_styles.push(style.clone());
        }
        self.commit(next)?;
        Ok(style)
    }

    pub fn update_style(
        &mut self,
        id: &str,
        name: &str,
        instructions: &str,
    ) -> StoreResult<SavedStyle> {
        let current = self
            .style(Some(id))?
            .filter(|style| !style.is_built_in)
            .ok_or_else(|| invalid("Custom style no longer exists"))?;
        let name = required_text(name, "Style name", 100)?;
        let instructions = required_text(instructions, "Style instructions", 8_000)?;
        if self.styles()?.iter().any(|style| {
            normalized_phrase(&style.name) == normalized_phrase(&name)
                && fold(&style.identifier) != fold(id)
        }) {
            return Err(invalid("Style names must be unique"));
        }
        let style = SavedStyle {
            identifier: current.identifier,
            name,
            instructions,
            is_built_in: false,
        };
        let mut next = self.state.clone();
        for item in &mut next.custom_styles {
            if item.identifier == style.identifier {
                *item = style.clone();
            }
        }
        self.commit(next)?;
        Ok(style)
    }

    pub fn delete_style(&mut self, id: &str) -> StoreResult<()> {
        let current = self
            .style(Some(id))?
            .filter(|style| !style.is_built_in)
            .ok_or_else(|| invalid("Custom style no longer exists"))?;
        let mut next = self.state.clone();
        next.custom_styles
            .retain(|style| style.identifier != current.identifier);
        if next.default_style_identifier.as_deref() == Some(&current.identifier) {
            next.default_style_identifier = None;
        }
        next.application_style_identifiers
            .retain(|_, style| *style != current.identifier);
        self.commit(next)
    }

    pub fn select_style(
        &mut self,
        id: Option<&str>,
        application: Option<&str>,
        remember: bool,
    ) -> StoreResult<()> {
        let selected = self.style(id)?;
        if id.is_some() && selected.is_none() {
            return Err(invalid("Style no longer exists"));
        }
        let id = selected.map(|style| style.identifier);
        let application = scope(application)?;
        let mut next = self.state.clone();
        if let Some(application) = application.filter(|_| remember) {
            if let Some(id) = id {
                next.application_style_disabled_identifiers
                    .remove(&application);
                next.application_style_identifiers.insert(application, id);
            } else {
                next.application_style_identifiers.remove(&application);
                next.application_style_disabled_identifiers
                    .insert(application);
            }
        } else {
            next.default_style_identifier = id;
        }
        self.commit(next)
    }

    pub fn selected_style(
        &self,
        application: Option<&str>,
        remember: bool,
    ) -> StoreResult<Option<SavedStyle>> {
        let application = scope(application)?;
        if let Some(application) = application.filter(|_| remember) {
            if self
                .state
                .application_style_disabled_identifiers
                .contains(&application)
            {
                return Ok(None);
            }
            self.style(
                self.state
                    .application_style_identifiers
                    .get(&application)
                    .map(String::as_str),
            )
        } else {
            self.style(self.state.default_style_identifier.as_deref())
        }
    }

    pub fn has_application_style_selection(&self, application: &str) -> StoreResult<bool> {
        let application = required_text(application, "Application identifier", 2_048)?;
        Ok(self
            .state
            .application_style_identifiers
            .contains_key(&application)
            || self
                .state
                .application_style_disabled_identifiers
                .contains(&application))
    }

    pub fn select_mode(
        &mut self,
        mode: &str,
        application: Option<&str>,
        remember: bool,
    ) -> StoreResult<()> {
        if !MODES.contains(&mode) {
            return Err(invalid(format!("Unsupported capture mode: {mode}")));
        }
        if let Some(application) = scope(application)?.filter(|_| remember) {
            let mut next = self.state.clone();
            next.application_modes.insert(application, mode.into());
            self.commit(next)?;
        }
        Ok(())
    }

    pub fn selected_mode(
        &self,
        application: Option<&str>,
        remember: bool,
        fallback: &str,
    ) -> StoreResult<String> {
        if !MODES.contains(&fallback) {
            return Err(invalid(format!("Unsupported capture mode: {fallback}")));
        }
        Ok(scope(application)?
            .filter(|_| remember)
            .and_then(|application| self.state.application_modes.get(&application).cloned())
            .unwrap_or_else(|| fallback.into()))
    }

    pub fn scoped_dictionary(
        &self,
        application: Option<&str>,
    ) -> StoreResult<Vec<DictionaryReplacement>> {
        let application = scope(application)?;
        let scoped = self
            .state
            .dictionary
            .iter()
            .filter(|rule| application.is_some() && rule.application_identifier == application)
            .cloned()
            .collect::<Vec<_>>();
        let keys = scoped
            .iter()
            .map(|rule| normalized_phrase(&rule.spoken))
            .collect::<BTreeSet<_>>();
        let mut global = self
            .state
            .dictionary
            .iter()
            .filter(|rule| {
                rule.application_identifier.is_none()
                    && !keys.contains(&normalized_phrase(&rule.spoken))
            })
            .cloned()
            .collect::<Vec<_>>();
        global.extend(scoped);
        Ok(global)
    }

    pub fn scoped_snippets(&self, application: Option<&str>) -> StoreResult<Vec<Snippet>> {
        let application = scope(application)?;
        let scoped = self
            .state
            .snippets
            .iter()
            .filter(|item| application.is_some() && item.application_identifier == application)
            .cloned()
            .collect::<Vec<_>>();
        let spoken = scoped
            .iter()
            .map(|item| normalized_phrase(&item.trigger))
            .collect::<BTreeSet<_>>();
        let typed = scoped
            .iter()
            .filter_map(|item| item.typed_trigger.as_deref())
            .collect::<BTreeSet<_>>();
        let mut global = self
            .state
            .snippets
            .iter()
            .filter(|item| {
                item.application_identifier.is_none()
                    && !spoken.contains(&normalized_phrase(&item.trigger))
                    && !item
                        .typed_trigger
                        .as_deref()
                        .is_some_and(|token| typed.contains(token))
            })
            .cloned()
            .collect::<Vec<_>>();
        global.extend(scoped);
        Ok(global)
    }

    pub fn process_transcript(
        &self,
        source: &str,
        application: Option<&str>,
        variables: &BTreeMap<String, String>,
    ) -> StoreResult<String> {
        Ok(personalize_transcript(
            source,
            &self.scoped_dictionary(application)?,
            &self.scoped_snippets(application)?,
            variables,
        ))
    }

    pub fn expand_typed_trigger(
        &self,
        trigger: &str,
        application: Option<&str>,
        variables: &BTreeMap<String, String>,
    ) -> StoreResult<Option<String>> {
        Ok(self
            .scoped_snippets(application)?
            .iter()
            .rev()
            .find(|item| item.typed_trigger.as_deref() == Some(trigger))
            .map(|item| render_snippet_expansion(&item.expansion, variables)))
    }
}

fn required_text(value: &str, label: &str, bound: usize) -> StoreResult<String> {
    let value = text::trim(value);
    if value.is_empty() {
        return Err(invalid(format!("{label} cannot be blank")));
    }
    if value.chars().count() > bound {
        return Err(invalid(format!(
            "{label} supports at most {} characters",
            match bound {
                2_048 => "2,048",
                2_000 => "2,000",
                20_000 => "20,000",
                8_000 => "8,000",
                100 => "100",
                _ => "200",
            }
        )));
    }
    Ok(value.into())
}

fn required_value(value: Option<&Value>, label: &str, bound: usize) -> StoreResult<String> {
    required_text(
        value
            .and_then(Value::as_str)
            .ok_or_else(|| invalid(format!("{label} must be text")))?,
        label,
        bound,
    )
}

fn optional_text(value: Option<&Value>, label: &str, bound: usize) -> StoreResult<Option<String>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        other => required_value(other, label, bound).map(Some),
    }
}

fn scope(value: Option<&str>) -> StoreResult<Option<String>> {
    value
        .map(|value| required_text(value, "Application identifier", 2_048))
        .transpose()
}

fn typed_trigger(value: Option<&str>) -> StoreResult<Option<String>> {
    let Some(value) = value.filter(|value| !text::trim(value).is_empty()) else {
        return Ok(None);
    };
    let value = required_text(value, "Typed trigger", 200)?;
    if value.chars().any(text::whitespace) {
        return Err(invalid("Typed triggers cannot contain whitespace"));
    }
    Ok(Some(value))
}

fn identifier(value: &str) -> StoreResult<String> {
    // Legacy documents accept compact, braced and URN UUIDs, with arbitrary hyphen placement.
    let value = value.replace("urn:", "").replace("uuid:", "");
    let compact = value.trim_matches(['{', '}']).replace('-', "");
    Uuid::parse_str(&compact)
        .map(|value| value.to_string())
        .map_err(|_| invalid("badly formed hexadecimal UUID string"))
}

fn required_identifier(value: Option<&Value>) -> StoreResult<String> {
    match value {
        Some(Value::String(value)) => identifier(value),
        _ => Ok(Uuid::new_v4().to_string()),
    }
}

fn string_mapping(value: Option<&Value>, label: &str) -> StoreResult<BTreeMap<String, String>> {
    match value {
        None | Some(Value::Null) => Ok(BTreeMap::new()),
        Some(Value::Object(values)) => values
            .iter()
            .map(|(key, value)| {
                value
                    .as_str()
                    .map(|value| (key.clone(), value.into()))
                    .ok_or_else(|| invalid(format!("{label} must map strings to strings")))
            })
            .collect(),
        _ => Err(invalid(format!("{label} must map strings to strings"))),
    }
}

fn string_set(value: Option<&Value>, label: &str) -> StoreResult<BTreeSet<String>> {
    match value {
        None => Ok(BTreeSet::new()),
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| invalid(format!("{label} must be an array of strings")))
            })
            .collect(),
        _ => Err(invalid(format!("{label} must be an array of strings"))),
    }
}

pub fn normalized_phrase(value: &str) -> String {
    fold(value)
        .split(text::whitespace)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn context(dictionary: &[DictionaryReplacement], snippets: &[Snippet]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    dictionary
        .iter()
        .flat_map(|item| [&item.spoken, &item.written])
        .chain(snippets.iter().map(|item| &item.trigger))
        .filter(|value| seen.insert(normalized_phrase(value)))
        .cloned()
        .collect()
}

const SPACE: &str = r"[\s\x1c-\x1f]";

fn case_phrase(phrase: &str) -> String {
    let mut pattern = String::new();
    for character in phrase.chars() {
        match character {
            ' ' => pattern.push_str(&format!("{SPACE}+")),
            _ => pattern.push_str(&text::ignore_case_literal(character)),
        }
    }
    pattern
}

fn phrase_expression(phrase: &str, snippet: bool) -> Regex {
    let mut pattern = if snippet {
        case_phrase("snippet ")
    } else {
        String::new()
    };
    pattern.push_str(&case_phrase(phrase));
    Regex::new(&pattern).expect("escaped bounded phrase")
}

fn replace_phrase(
    source: &str,
    expression: &Regex,
    replacement: impl Fn(&str) -> String,
) -> String {
    let mut result = String::with_capacity(source.len());
    let (mut copied, mut scan) = (0, 0);
    while let Some(found) = expression.find_at(source, scan) {
        let before = source[..found.start()].chars().next_back();
        let mut end = found.end();
        if !before.is_some_and(text::word_character)
            && source[end..]
                .chars()
                .next()
                .is_some_and(text::word_character)
        {
            for (length, character) in found.as_str().char_indices().rev() {
                if !text::word_character(character) {
                    let prefix = &found.as_str()[..length];
                    if expression.find(prefix).is_some_and(|candidate| {
                        candidate.start() == 0 && candidate.end() == prefix.len()
                    }) {
                        end = found.start() + length;
                        break;
                    }
                }
            }
        }
        let after = source[end..].chars().next();
        if !before.is_some_and(text::word_character) && !after.is_some_and(text::word_character) {
            result.push_str(&source[copied..found.start()]);
            result.push_str(&replacement(&source[found.start()..end]));
            copied = end;
            scan = end;
            if found.start() == end {
                let Some(character) = source[end..].chars().next() else {
                    break;
                };
                scan += character.len_utf8();
            }
        } else {
            // A rejected start must not consume an overlapping valid phrase.
            let Some(character) = source[found.start()..].chars().next() else {
                break;
            };
            scan = found.start() + character.len_utf8();
        }
    }
    result.push_str(&source[copied..]);
    result
}

fn replacement_text(rule: &DictionaryReplacement, matched: &str) -> String {
    if rule.case_behavior == DictionaryCaseBehavior::Fixed {
        return rule.written.clone();
    }
    let letters = matched
        .chars()
        .filter(|&character| text::alphabetic(character))
        .collect::<Vec<_>>();
    if letters.is_empty() {
        return rule.written.clone();
    }
    if letters.iter().all(|&character| text::uppercase(character)) {
        return text::upper(&rule.written);
    }
    if letters.iter().all(|&character| text::lowercase(character)) {
        return text::lower(&rule.written);
    }
    if text::uppercase(letters[0])
        && letters[1..]
            .iter()
            .all(|&character| text::lowercase(character))
    {
        let lowered = text::lower(&rule.written);
        let mut chars = lowered.chars();
        return chars
            .next()
            .map(|first| text::upper(&first.to_string()) + chars.as_str())
            .unwrap_or_default();
    }
    rule.written.clone()
}

pub fn personalize_transcript(
    source: &str,
    dictionary: &[DictionaryReplacement],
    snippets: &[Snippet],
    variables: &BTreeMap<String, String>,
) -> String {
    let mut result = source.to_owned();
    let mut dictionary = dictionary.iter().collect::<Vec<_>>();
    dictionary.sort_by_key(|item| std::cmp::Reverse(item.spoken.chars().count()));
    for rule in dictionary {
        result = replace_phrase(
            &result,
            &phrase_expression(&rule.spoken, false),
            |matched| replacement_text(rule, matched),
        );
    }
    let mut snippets = snippets.iter().collect::<Vec<_>>();
    snippets.sort_by_key(|item| std::cmp::Reverse(item.trigger.chars().count()));
    for item in snippets {
        let expansion = render_snippet_expansion(&item.expansion, variables);
        result = replace_phrase(&result, &phrase_expression(&item.trigger, true), |_| {
            expansion.clone()
        });
    }
    normalize_whitespace(&result)
}

pub fn render_snippet_expansion(expansion: &str, variables: &BTreeMap<String, String>) -> String {
    static VARIABLE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\{\{([A-Za-z][A-Za-z0-9_]*)\}\}").unwrap());
    let normalized = variables
        .iter()
        .map(|(key, value)| (fold(key), value))
        .collect::<BTreeMap<_, _>>();
    VARIABLE
        .replace_all(expansion, |capture: &regex::Captures<'_>| {
            normalized
                .get(&fold(&capture[1]))
                .map(|value| value.as_str())
                .unwrap_or(&capture[0])
                .to_owned()
        })
        .into_owned()
}

fn normalize_whitespace(source: &str) -> String {
    static HORIZONTAL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\t ]+").unwrap());
    static PUNCTUATION: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(&format!("{SPACE}+([,.;:!?])")).unwrap());
    static PARAGRAPHS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n{3,}").unwrap());
    let newlines = source.replace("\r\n", "\n").replace('\r', "\n");
    let lines = newlines
        .split('\n')
        .map(|line| {
            let horizontal = HORIZONTAL.replace_all(line, " ");
            text::trim(&PUNCTUATION.replace_all(&horizontal, "$1")).to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n");
    text::trim(&PARAGRAPHS.replace_all(&lines, "\n\n")).to_owned()
}

/// Resolve documented variables through the process's current local calendar and locale.
pub fn snippet_variables() -> BTreeMap<String, String> {
    use std::ffi::{CStr, CString};
    let mut current: libc::time_t = 0;
    // SAFETY: time/localtime_r/strftime receive valid owned output buffers and fixed format strings.
    let local = unsafe {
        libc::time(&mut current);
        let mut local: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&current, &mut local).is_null() {
            return BTreeMap::new();
        }
        local
    };
    let formatted = |format: &str| {
        let format = CString::new(format).unwrap();
        let mut buffer = [0_i8; 512];
        // SAFETY: the buffer is writable, the format is terminated, and local is initialized above.
        let size =
            unsafe { libc::strftime(buffer.as_mut_ptr(), buffer.len(), format.as_ptr(), &local) };
        if size == 0 {
            String::new()
        } else {
            // SAFETY: successful strftime writes a trailing NUL within the provided buffer.
            unsafe { CStr::from_ptr(buffer.as_ptr()) }
                .to_string_lossy()
                .into_owned()
        }
    };
    let date = formatted("%x");
    let time = formatted("%H:%M");
    BTreeMap::from([
        ("date".into(), date.clone()),
        ("time".into(), time.clone()),
        ("datetime".into(), format!("{date} {time}")),
        ("weekday".into(), formatted("%A")),
    ])
}

struct ProtectedExpression {
    expression: Regex,
    start_word: bool,
    end_word: bool,
    path: bool,
    numbered_identifier: bool,
}

static PROTECTED: LazyLock<Vec<ProtectedExpression>> = LazyLock::new(|| {
    let digit = text::decimal_expression();
    [
        (
            format!(
                "{}{}?://[^\\s\\x1c-\\x1f<>()]+",
                case_phrase("http"),
                text::ignore_case_literal('s')
            ),
            false,
            false,
            false,
            false,
        ),
        (
            r"/[A-Za-z0-9._~-]+(?:/[A-Za-z0-9._~-]+)+".into(),
            false,
            false,
            true,
            false,
        ),
        (r"`[^`\n]+`".into(), false, false, false, false),
        (
            r"--[A-Za-z0-9][A-Za-z0-9-]*".into(),
            false,
            false,
            false,
            false,
        ),
        (
            [
                "do not", "does not", "did not", "not", "never", "without", "cannot", "can't",
                "won't", "don't",
            ]
            .map(case_phrase)
            .join("|"),
            true,
            true,
            false,
            false,
        ),
        (
            r"[A-Za-z][A-Za-z0-9]*_[A-Za-z0-9_]+".into(),
            true,
            true,
            false,
            false,
        ),
        (
            r"[a-z]+(?:[A-Z][A-Za-z0-9]*)+".into(),
            true,
            true,
            false,
            false,
        ),
        (
            format!("{digit}+(?:\\.{digit}+)?%"),
            true,
            false,
            false,
            false,
        ),
        (
            r"[A-Za-z]+(?:-[A-Za-z0-9]+)+".into(),
            true,
            true,
            false,
            true,
        ),
        (
            format!("{digit}+(?:\\.{digit}+)*"),
            true,
            true,
            false,
            false,
        ),
    ]
    .into_iter()
    .map(
        |(pattern, start_word, end_word, path, numbered_identifier)| ProtectedExpression {
            expression: Regex::new(&pattern).expect("protected token expression"),
            start_word,
            end_word,
            path,
            numbered_identifier,
        },
    )
    .collect()
});

fn protected_match(
    expression: &ProtectedExpression,
    source: &str,
    mut scan: usize,
) -> Option<std::ops::Range<usize>> {
    while let Some(found) = expression.expression.find_at(source, scan) {
        let before = source[..found.start()]
            .chars()
            .next_back()
            .is_some_and(text::word_character);
        let after = source[found.end()..]
            .chars()
            .next()
            .is_some_and(text::word_character);
        let numbered = !expression.numbered_identifier
            || source[found.start()..]
                .chars()
                .take_while(|character| character.is_ascii_alphanumeric() || *character == '-')
                .any(|character| character.is_ascii_digit());
        if (!expression.start_word && !expression.path || !before) && numbered {
            if !expression.end_word || !after {
                return Some(found.range());
            }
            // A word-boundary check can make a greedy version or hyphenated identifier
            // fall back to an earlier complete component, as in "1.2.3é" or "gpt-4-fó".
            for (length, character) in found.as_str().char_indices().rev() {
                if length > 0 && !text::word_character(character) {
                    let prefix = &found.as_str()[..length];
                    if expression.expression.find(prefix).is_some_and(|candidate| {
                        candidate.start() == 0 && candidate.end() == prefix.len()
                    }) {
                        return Some(found.start()..found.start() + length);
                    }
                }
            }
        }
        scan = found.start() + found.as_str().chars().next().unwrap().len_utf8();
    }
    None
}

fn contains_token(token: &str, source: &str) -> bool {
    let camel = token
        .as_bytes()
        .windows(2)
        .any(|pair| pair[0].is_ascii_lowercase() && pair[1].is_ascii_uppercase());
    if token.contains("://")
        || token.starts_with(['/', '`'])
        || token.starts_with("--")
        || token.contains('_')
        || camel
    {
        source.contains(token)
    } else {
        fold(source).contains(&fold(token))
    }
}

/// Generative output must preserve syntax-sensitive terms, numbers and negation.
pub fn integrity_violations(source: &str, candidate: &str, vocabulary: &[String]) -> Vec<String> {
    let mut tokens = BTreeSet::new();
    let mut scan = 0;
    while scan < source.len() {
        let found = PROTECTED
            .iter()
            .enumerate()
            .filter_map(|(priority, expression)| {
                protected_match(expression, source, scan).map(|found| (priority, found))
            })
            .min_by_key(|(priority, found)| (found.start, *priority));
        let Some((_, found)) = found else {
            break;
        };
        tokens.insert(source[found.clone()].to_owned());
        scan = found.end;
    }
    tokens.extend(
        vocabulary
            .iter()
            .filter(|term| contains_token(term, source))
            .cloned(),
    );
    tokens
        .into_iter()
        .filter(|token| !contains_token(token, candidate))
        .collect()
}
