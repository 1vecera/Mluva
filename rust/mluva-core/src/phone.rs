//! Private, same-user IPC for the browser microphone. No credentials cross it.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const MAX_CHUNK_BYTES: usize = 32_000;
pub const MAX_PCM_BYTES: usize = 32_000 * 7_200;
pub const MAX_MESSAGE_BYTES: usize = 65_536;
pub const HISTORY_PREFIX: &str = "mluva-web:";

pub fn valid_identifier(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|id| id.to_string() == value)
}

pub fn socket_path() -> std::io::Result<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(|root| PathBuf::from(root).join("mluva/phone.sock"))
        .ok_or_else(|| std::io::Error::other("A private desktop runtime is required."))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum PhoneRequest {
    Start {
        identifier: String,
    },
    Audio {
        identifier: String,
        sequence: u64,
        pcm: String,
    },
    Stop {
        identifier: String,
        sequence: u64,
    },
    Cancel {
        identifier: String,
    },
    Status {
        identifier: String,
    },
}
impl PhoneRequest {
    pub fn identifier(&self) -> &str {
        match self {
            Self::Start { identifier }
            | Self::Audio { identifier, .. }
            | Self::Stop { identifier, .. }
            | Self::Cancel { identifier }
            | Self::Status { identifier } => identifier,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PhoneReply {
    pub identifier: String,
    pub phase: String,
    pub sequence: u64,
    pub text: String,
    pub copied: bool,
    #[serde(default)]
    pub incognito: bool,
    pub message: String,
}
impl PhoneReply {
    pub fn error(identifier: &str, message: &str) -> Self {
        Self {
            identifier: identifier.into(),
            phase: "error".into(),
            message: message.into(),
            ..Self::default()
        }
    }
}
