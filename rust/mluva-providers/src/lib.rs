//! Provider sessions freeze route choices and never return provisional or cancelled text.

pub mod codex;
pub mod codex_policy;
pub mod compatible;
pub mod credentials;
pub mod elevenlabs;
pub mod languages;
pub mod local_assets;
pub mod models;
pub mod multipart;
pub mod onnx_runtime;
pub mod qwen;
pub mod realtime;
pub mod rewriting;
mod transport;

use serde::{Deserialize, Serialize};

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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TranscriptionResult {
    pub text: String,
    pub language_code: String,
    pub language_probability: Option<f64>,
    pub transcription_id: Option<String>,
    pub speaker_segments: Vec<SpeakerSegment>,
    pub audio_duration_seconds: Option<f64>,
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
