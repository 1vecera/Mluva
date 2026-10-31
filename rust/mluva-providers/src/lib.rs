//! Provider sessions freeze route choices and never return provisional or cancelled text.

pub mod account;
pub mod agent_platform;
pub mod agent_targets;
pub mod antigravity;
pub mod batch_preview;
pub mod catalog;
pub mod claude_desktop;
pub mod claude_inbox;
pub mod claude_sessions;
pub mod codex;
pub mod compatible;
pub mod credentials;
pub mod elevenlabs;
pub mod herdr;
pub mod languages;
pub mod local;
pub mod local_asr;
pub mod local_assets;
pub mod local_preview;
pub mod models;
pub mod multipart;
pub mod onnx_runtime;
pub mod opencode;
mod process_lifetime;
pub mod qwen;
pub mod realtime;
pub mod speech;
pub mod spoken_reply;
#[cfg(test)]
mod test_sockets;
mod transport;

use serde::{Deserialize, Serialize};

#[cfg(target_os = "macos")]
pub const USER_AGENT: &str = concat!("MluvaMac/", env!("CARGO_PKG_VERSION"));
#[cfg(not(target_os = "macos"))]
pub const USER_AGENT: &str = concat!("MluvaLinux/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ProviderError(pub String);

impl ProviderError {
    pub(crate) fn message(message: &str) -> Self {
        Self(message.into())
    }
}

pub type Result<T> = std::result::Result<T, ProviderError>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpeakerSegment {
    pub speaker: String,
    pub text: String,
    pub started_at_seconds: f64,
    pub ended_at_seconds: f64,
}

/// A recognized word on the recording clock, seconds from the first captured sample.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TimedWord {
    pub text: String,
    pub start_seconds: f64,
    pub end_seconds: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TranscriptionResult {
    pub text: String,
    pub language_code: String,
    pub language_probability: Option<f64>,
    pub transcription_id: Option<String>,
    pub speaker_segments: Vec<SpeakerSegment>,
    pub audio_duration_seconds: Option<f64>,
    /// Word times the provider returned with this result; never edited afterwards. Empty when
    /// the route reports none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<TimedWord>,
}

/// Credential values cannot appear in derived Debug output or transport diagnostics.
#[derive(Clone)]
pub struct Secret(String);
impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    pub(crate) fn value(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<credential>")
    }
}
