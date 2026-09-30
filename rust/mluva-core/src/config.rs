//! Non-secret settings with the released application's validation and migration rules.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Number, Value};
use thiserror::Error;

use crate::private_files::atomic_write_private;
use crate::text::trim;

pub const ELEVENLABS_KEY_VARIABLES: [&str; 3] = [
    "ELEVENLABS_API_KEY",
    "DAS_ITEM_ELEVEN_LABS_API_KEY__CREDENTIAL",
    "ELEVEN_LABS_STT_TOKEN",
];
pub const CAPTURE_MODES: [&str; 3] = ["dictation", "command", "scratchpad"];
pub const LIVE_TEMPLATES: [&str; 5] = [
    "grilling",
    "task-spec",
    "structured-note",
    "polish",
    "custom",
];
pub const LOCAL_MODELS: [&str; 6] = [
    "whisper-tiny",
    "whisper-base",
    "whisper-small",
    "parakeet-v3",
    "whisper-turbo",
    "qwen3-1.7b",
];

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("{0}")]
    Invalid(&'static str),
    #[error("Mluva config is not valid JSON")]
    Json,
    #[error("Mluva config contains unknown or incorrectly typed settings")]
    Fields,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioRetentionPolicy {
    Never,
    #[default]
    Failures,
    Always,
}

impl AudioRetentionPolicy {
    /// Retain unsuccessful captures only when the user selected recovery or permanent retention.
    pub fn should_retain(self, delivery_succeeded: bool) -> bool {
        match self {
            Self::Never => false,
            Self::Failures => !delivery_succeeded,
            Self::Always => true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppConfig {
    pub language_code: String,
    pub transcription_model: String,
    pub codex_model: Option<String>,
    pub rewrite_model: Option<String>,
    pub rewrite_fast_mode: bool,
    pub rewrite_reasoning_effort: Option<String>,
    pub litellm_reasoning_effort: Option<String>,
    pub rewrite_provider: String,
    pub litellm_base_url: String,
    pub litellm_model: Option<String>,
    pub litellm_api_key_env: String,
    pub transcription_provider: String,
    pub transcription_base_url: String,
    pub transcription_api_key_env: String,
    pub transcription_remote_model: Option<String>,
    pub local_model: String,
    pub local_device: String,
    pub transcription_chunk_seconds: i64,
    pub auto_copy_dictation: bool,
    pub auto_copy_rewrite: bool,
    pub show_copy_action: bool,
    pub show_save_action: bool,
    pub review_timeout_seconds: i64,
    pub smooth_scrolling: bool,
    pub scroll_duration_ms: i64,
    pub scroll_lookahead_lines: i64,
    pub widget_position: String,
    pub widget_lines: i64,
    pub widget_opacity: i64,
    pub history_sidebar_visible: bool,
    pub welcome_completed: bool,
    pub time_format: String,
    pub live_rewrite_enabled: bool,
    pub live_rewrite_continuous: bool,
    pub live_rewrite_template: String,
    pub live_rewrite_custom_instructions: String,
    pub live_rewrite_min_characters: i64,
    pub live_rewrite_interval_seconds: i64,
    pub microphone_target: Option<String>,
    pub system_audio_target: Option<String>,
    pub default_mode: String,
    pub global_recording_key: String,
    pub auto_paste: bool,
    pub automatic_titles: bool,
    pub spoken_commands_enabled: bool,
    pub remember_per_application: bool,
    pub audio_retention_policy: AudioRetentionPolicy,
    pub incognito_mode: bool,
    // The legacy schema permits any non-negative integer, without an i64/u64 upper bound.
    pub history_retention_days: Number,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            language_code: "eng".into(),
            transcription_model: "scribe_v2".into(),
            codex_model: None,
            rewrite_model: None,
            rewrite_fast_mode: false,
            rewrite_reasoning_effort: None,
            litellm_reasoning_effort: None,
            rewrite_provider: "codex".into(),
            litellm_base_url: "http://localhost:4000/v1".into(),
            litellm_model: None,
            litellm_api_key_env: "LITELLM_API_KEY".into(),
            transcription_provider: "elevenlabs".into(),
            transcription_base_url: "http://localhost:4000/v1".into(),
            transcription_api_key_env: "LITELLM_API_KEY".into(),
            transcription_remote_model: Some("whisper".into()),
            local_model: "qwen3-1.7b".into(),
            local_device: "cpu".into(),
            transcription_chunk_seconds: 8,
            auto_copy_dictation: true,
            auto_copy_rewrite: true,
            show_copy_action: true,
            show_save_action: true,
            review_timeout_seconds: 4,
            smooth_scrolling: true,
            scroll_duration_ms: 800,
            scroll_lookahead_lines: 2,
            widget_position: "bottom-center".into(),
            widget_lines: 5,
            widget_opacity: 82,
            history_sidebar_visible: false,
            welcome_completed: false,
            time_format: "24h".into(),
            live_rewrite_enabled: false,
            live_rewrite_continuous: true,
            live_rewrite_template: "grilling".into(),
            live_rewrite_custom_instructions: String::new(),
            live_rewrite_min_characters: 160,
            live_rewrite_interval_seconds: 4,
            microphone_target: None,
            system_audio_target: None,
            default_mode: "dictation".into(),
            global_recording_key: "F9".into(),
            auto_paste: false,
            automatic_titles: true,
            spoken_commands_enabled: true,
            remember_per_application: false,
            audio_retention_policy: AudioRetentionPolicy::Failures,
            incognito_mode: false,
            history_retention_days: Number::from(0),
        }
    }
}

impl AppConfig {
    /// Keep persisted routes and privacy choices inside the same vocabulary and bounds as 1.6.0.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.language_code != "auto"
            && (!(2..=3).contains(&self.language_code.len())
                || !self.language_code.bytes().all(|c| c.is_ascii_lowercase()))
        {
            return Err(ConfigError::Invalid(
                "language_code must be 'auto' or a lowercase ISO-639-1/3 code",
            ));
        }
        if self.transcription_model != "scribe_v2" {
            return Err(ConfigError::Invalid(
                "transcription_model must be 'scribe_v2'",
            ));
        }
        for identifier in [&self.codex_model, &self.rewrite_model, &self.litellm_model]
            .into_iter()
            .flatten()
        {
            validate_identifier(
                identifier,
                200,
                "codex_model must be a bounded single-line model identifier",
            )?;
        }
        if let Some(identifier) = &self.transcription_remote_model {
            validate_identifier(
                identifier,
                200,
                "codex_model must be a bounded single-line model identifier",
            )?;
        }
        for effort in [
            &self.rewrite_reasoning_effort,
            &self.litellm_reasoning_effort,
        ]
        .into_iter()
        .flatten()
        {
            let bytes = effort.as_bytes();
            if bytes.is_empty()
                || bytes.len() > 32
                || !bytes[0].is_ascii_lowercase()
                || !bytes.iter().all(|c| {
                    c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_' || *c == b'-'
                })
            {
                return Err(ConfigError::Invalid(
                    "Thinking level must be null or a bounded provider effort identifier",
                ));
            }
        }
        if !["codex", "litellm", "none"].contains(&self.rewrite_provider.as_str()) {
            return Err(ConfigError::Invalid(
                "rewrite_provider must be codex, litellm or none",
            ));
        }
        if !["elevenlabs", "litellm", "local"].contains(&self.transcription_provider.as_str()) {
            return Err(ConfigError::Invalid(
                "transcription_provider must be elevenlabs, litellm or local",
            ));
        }
        if !["cpu", "cuda"].contains(&self.local_device.as_str()) {
            return Err(ConfigError::Invalid("local_device must be cpu or cuda"));
        }
        if !LOCAL_MODELS.contains(&self.local_model.as_str()) {
            return Err(ConfigError::Invalid(
                "Choose a supported local speech model",
            ));
        }
        validate_provider_url(&self.litellm_base_url)?;
        validate_provider_url(&self.transcription_base_url)?;
        for name in [&self.litellm_api_key_env, &self.transcription_api_key_env] {
            let bytes = name.as_bytes();
            if bytes.is_empty()
                || bytes.len() > 128
                || !(bytes[0].is_ascii_alphabetic() || bytes[0] == b'_')
                || !bytes
                    .iter()
                    .all(|c| c.is_ascii_alphanumeric() || *c == b'_')
            {
                return Err(ConfigError::Invalid(
                    "API key settings must name an environment variable, never contain a key",
                ));
            }
        }
        for (value, minimum, maximum, message) in [
            (
                self.widget_lines,
                1,
                10,
                "widget_lines must be an integer from 1 to 10",
            ),
            (
                self.widget_opacity,
                10,
                100,
                "widget_opacity must be an integer from 10 to 100",
            ),
            (
                self.review_timeout_seconds,
                1,
                60,
                "review_timeout_seconds must be an integer from 1 to 60",
            ),
            (
                self.scroll_duration_ms,
                0,
                2000,
                "scroll_duration_ms must be an integer from 0 to 2000",
            ),
            (
                self.scroll_lookahead_lines,
                0,
                6,
                "scroll_lookahead_lines must be an integer from 0 to 6",
            ),
            (
                self.transcription_chunk_seconds,
                3,
                30,
                "transcription_chunk_seconds must be an integer from 3 to 30",
            ),
            (
                self.live_rewrite_min_characters,
                40,
                4000,
                "live_rewrite_min_characters must be an integer from 40 to 4000",
            ),
            (
                self.live_rewrite_interval_seconds,
                2,
                60,
                "live_rewrite_interval_seconds must be an integer from 2 to 60",
            ),
        ] {
            if !(minimum..=maximum).contains(&value) {
                return Err(ConfigError::Invalid(message));
            }
        }
        if !["bottom-left", "bottom-center", "bottom-right"]
            .contains(&self.widget_position.as_str())
        {
            return Err(ConfigError::Invalid("Unsupported widget position"));
        }
        if !["24h", "12h"].contains(&self.time_format.as_str()) {
            return Err(ConfigError::Invalid("Unsupported time format"));
        }
        if !LIVE_TEMPLATES.contains(&self.live_rewrite_template.as_str()) {
            return Err(ConfigError::Invalid("Unsupported live rewrite template"));
        }
        if self.live_rewrite_custom_instructions.chars().count() > 8000 {
            return Err(ConfigError::Invalid(
                "Custom live instructions must contain at most 8000 characters",
            ));
        }
        for (target, message) in [
            (
                &self.microphone_target,
                "microphone_target must be a bounded single-line PipeWire node name",
            ),
            (
                &self.system_audio_target,
                "system_audio_target must be a bounded single-line PipeWire node name",
            ),
        ] {
            if let Some(target) = target {
                validate_identifier(target, 512, message)?;
            }
        }
        if !CAPTURE_MODES.contains(&self.default_mode.as_str()) {
            return Err(ConfigError::Invalid("Unsupported default capture mode"));
        }
        if !(1..=24).any(|number| self.global_recording_key == format!("F{number}")) {
            return Err(ConfigError::Invalid(
                "global_recording_key must be one of F1 through F24",
            ));
        }
        if !self
            .history_retention_days
            .to_string()
            .bytes()
            .all(|c| c.is_ascii_digit())
        {
            return Err(ConfigError::Invalid(
                "history_retention_days must be a non-negative integer",
            ));
        }
        Ok(())
    }

    /// Interpret an existing file, preserving onboarding and retired-setting migrations without rewriting it.
    pub fn from_persisted_json(bytes: &[u8]) -> Result<Self, ConfigError> {
        let value: Value = serde_json::from_slice(bytes).map_err(|_| ConfigError::Json)?;
        let mut payload = value
            .as_object()
            .cloned()
            .ok_or(ConfigError::Invalid("Mluva config must be a JSON object"))?;
        payload
            .entry("welcome_completed")
            .or_insert(Value::Bool(true));
        payload.remove("voxtype_model");
        if payload
            .get("transcription_provider")
            .and_then(Value::as_str)
            == Some("voxtype")
        {
            payload.insert(
                "transcription_provider".into(),
                Value::String("local".into()),
            );
            payload.insert("welcome_completed".into(), Value::Bool(false));
        }
        if payload.get("local_model").and_then(Value::as_str) == Some("whisper-turbo") {
            payload.insert("local_model".into(), Value::String("qwen3-1.7b".into()));
            payload.insert("welcome_completed".into(), Value::Bool(false));
        }
        if let Some(legacy) = payload.remove("retain_audio_on_failure")
            && !legacy.is_null()
            && !payload.contains_key("audio_retention_policy")
        {
            payload.insert(
                "audio_retention_policy".into(),
                Value::String(
                    if value_is_truthy(&legacy) {
                        "failures"
                    } else {
                        "never"
                    }
                    .into(),
                ),
            );
        }
        // Deserializing Number through Value may coerce an exact large integer to a floating exponent.
        // Keep its decimal JSON representation through the settings migration instead.
        let migrated =
            serde_json::to_vec(&Value::Object(payload)).map_err(|_| ConfigError::Fields)?;
        let config: Self = serde_json::from_slice(&migrated).map_err(|_| ConfigError::Fields)?;
        config.validate()?;
        Ok(config)
    }

    /// Distinguish a first launch from a malformed existing settings file so user choices cannot be lost.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        match fs::read(path) {
            Ok(bytes) => Self::from_persisted_json(&bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error.into()),
        }
    }

    /// Save validated choices atomically, refusing to replace an existing document that needs repair.
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        self.validate()?;
        if path.exists() && Self::load(path).is_err() {
            return Err(ConfigError::Invalid(
                "config.json needs repair; refusing to overwrite it",
            ));
        }
        let mut bytes = serde_json::to_vec_pretty(self).map_err(|_| ConfigError::Fields)?;
        bytes.push(b'\n');
        atomic_write_private(path, &bytes)?;
        Ok(())
    }
}

fn value_is_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64() != Some(0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}

fn validate_identifier(
    value: &str,
    maximum: usize,
    message: &'static str,
) -> Result<(), ConfigError> {
    if trim(value) != value
        || value.is_empty()
        || value.chars().count() > maximum
        || value.chars().any(|c| c < ' ')
    {
        return Err(ConfigError::Invalid(message));
    }
    Ok(())
}

/// Reject URL credentials and secret components while retaining explicit loopback HTTP development endpoints.
pub fn validate_provider_url(value: &str) -> Result<(), ConfigError> {
    if trim(value) != value || value.chars().any(|c| c < ' ') {
        return Err(ConfigError::Invalid(
            "Provider URL must be a plain HTTP(S) base URL",
        ));
    }
    let parsed = url::Url::parse(value)
        .map_err(|_| ConfigError::Invalid("Provider URL must be a plain HTTP(S) base URL"))?;
    let authority = value
        .split_once("://")
        .map(|(_, rest)| rest.split(['/', '?', '#']).next().unwrap_or(""))
        .unwrap_or("");
    let raw_host = if authority.starts_with('[') {
        authority
            .split_once(']')
            .map(|(host, _)| &host[1..])
            .unwrap_or("")
    } else {
        authority.split(':').next().unwrap_or("")
    };
    // URL parsers canonicalize shortened or numeric IPv4 addresses; the reference admits only these exact hosts.
    let loopback = ["localhost", "127.0.0.1", "::1"].contains(&raw_host.to_lowercase().as_str());
    if authority.is_empty()
        || parsed.host_str().is_none()
        || !["http", "https"].contains(&parsed.scheme())
        || authority.contains('@')
        || parsed.query().is_some_and(|query| !query.is_empty())
        || parsed
            .fragment()
            .is_some_and(|fragment| !fragment.is_empty())
        || (parsed.scheme() == "http" && !loopback)
    {
        return Err(ConfigError::Invalid(
            "Use HTTPS (or loopback HTTP) without credentials, query parameters or fragments",
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppPaths {
    pub config: PathBuf,
    pub data: PathBuf,
    pub runtime: PathBuf,
}

impl AppPaths {
    /// Resolve the same XDG roots used by installed settings, data, recovery and shell integration.
    pub fn from_environ(environ: &BTreeMap<String, String>) -> Result<Self, ConfigError> {
        let fallback = |relative: &str| -> Result<PathBuf, ConfigError> {
            Ok(PathBuf::from(
                environ
                    .get("HOME")
                    .ok_or(ConfigError::Invalid("HOME is required without an XDG root"))?,
            )
            .join(relative))
        };
        let config = match environ.get("XDG_CONFIG_HOME") {
            Some(root) => PathBuf::from(root),
            None => fallback(".config")?,
        }
        .join("mluva");
        let data = match environ.get("XDG_DATA_HOME") {
            Some(root) => PathBuf::from(root),
            None => fallback(".local/share")?,
        }
        .join("mluva");
        let runtime = environ
            .get("XDG_RUNTIME_DIR")
            .map(|root| PathBuf::from(root).join("mluva"))
            .unwrap_or_else(|| data.join("runtime"));
        Ok(Self {
            config,
            data,
            runtime,
        })
    }
}
