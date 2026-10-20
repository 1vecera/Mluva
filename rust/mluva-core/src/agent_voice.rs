//! Toggle recording with agent routing is a one-shot delivery intent, independent of capture providers.
//! No pending send is serialized or replayed after restart.

use serde::{Deserialize, Serialize};
use std::{fs, io::Read, path::Path};

pub const CANCEL_WINDOW_MS: u64 = 2_000;
pub const MAX_RECORDING_MS: u64 = 5 * 60 * 1_000;
pub const MAX_MESSAGE_BYTES: usize = 32_768;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recipient {
    pub socket: String,
    pub server_epoch: String,
    pub pane_id: String,
    /// Opaque server-issued identity, including the actual agent process/session.
    pub guard: String,
    pub name: String,
    pub kind: String,
    pub project: String,
    pub status: AgentStatus,
}

impl Recipient {
    pub fn same_instance(&self, other: &Self) -> bool {
        self.socket == other.socket
            && self.server_epoch == other.server_epoch
            && self.pane_id == other.pane_id
            && self.guard == other.guard
    }

    pub fn can_receive(&self) -> bool {
        !self.guard.is_empty()
            && matches!(self.kind.as_str(), "codex" | "claude" | "claude_code")
            && matches!(
                self.status,
                AgentStatus::Idle | AgentStatus::Done | AgentStatus::Working
            )
    }

    /// Metadata-only fallback; it deliberately makes no claim about a particular turn.
    pub fn spoken_status(&self) -> String {
        let name: String = self
            .name
            .chars()
            .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
            .take(48)
            .collect();
        let name = if name.trim().is_empty() {
            "Your agent"
        } else {
            name.trim()
        };
        match self.status {
            AgentStatus::Idle => format!("{name} is ready for your next message."),
            AgentStatus::Working => {
                format!("{name} is working; its latest result is not confirmed yet.")
            }
            AgentStatus::Blocked => {
                format!("{name} needs your attention in Herdr before it can continue.")
            }
            AgentStatus::Done => {
                format!("{name} has finished a turn; open its session to review the result.")
            }
            AgentStatus::Unknown => {
                format!("{name} is unavailable or its current activity is not clear yet.")
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Idle,
    Working,
    Blocked,
    Done,
    #[default]
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VoiceSettings {
    pub enabled: bool,
    pub socket: Option<String>,
    pub selected: Option<Recipient>,
    pub hold_key: String,
    pub draft_key: String,
    pub spoken_updates: bool,
    pub voice_id: String,
}

impl Default for VoiceSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            socket: None,
            selected: None,
            hold_key: "F8".into(),
            draft_key: "F7".into(),
            spoken_updates: false,
            voice_id: String::new(),
        }
    }
}

impl VoiceSettings {
    pub fn validate(&self, recording_key: &str) -> Result<(), &'static str> {
        for key in [&self.hold_key, &self.draft_key] {
            if !(1..=24).any(|n| *key == format!("F{n}")) {
                return Err("Choose a function key from F1 through F24.");
            }
            if key == recording_key || key == "F10" {
                return Err("Choose a key other than dictation or screenshot capture.");
            }
        }
        if self.hold_key == self.draft_key {
            return Err("Agent dictation and Keep draft need different shortcuts.");
        }
        if self
            .socket
            .as_ref()
            .is_some_and(|s| !s.starts_with('/') || s.len() > 103 || s.contains('\0'))
        {
            return Err("Choose an absolute local Herdr socket path.");
        }
        if self.voice_id.len() > 128
            || !self
                .voice_id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
        {
            return Err("Choose a valid ElevenLabs voice.");
        }
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self, &'static str> {
        let read = fs::File::open(path).and_then(|file| {
            let mut bytes = Vec::new();
            file.take(16_385).read_to_end(&mut bytes)?;
            Ok(bytes)
        });
        match read {
            Ok(bytes) if bytes.len() <= 16_384 => serde_json::from_slice(&bytes).map_err(
                |_| "Herdr voice settings could not be read; the original file was preserved.",
            ),
            Ok(_) => Err("Herdr voice settings are too large; the original file was preserved."),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(_) => {
                Err("Herdr voice settings could not be read; the original file was preserved.")
            }
        }
    }

    pub fn save(&self, path: &Path, recording_key: &str) -> Result<(), &'static str> {
        self.validate(recording_key)?;
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|_| "Herdr voice settings could not be saved.")?;
        crate::private_files::atomic_write_private(path, &bytes)
            .map_err(|_| "Herdr voice settings could not be saved.")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DraftReason {
    Kept,
    Edited,
    RecipientChanged,
    Unavailable,
    Cancelled,
    Interrupted,
    Empty,
    TooLong,
    Rejected,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoutePhase {
    Recording,
    Finalizing,
    Grace { deadline_ms: u64 },
    Draft(DraftReason),
    Submitting,
    Sent,
    Unconfirmed,
}

#[derive(Clone, Debug)]
pub struct Attempt {
    pub id: String,
    pub recipient: Recipient,
    pub phase: RoutePhase,
    pub text: String,
    pub history_id: Option<String>,
    pub started_ms: u64,
    pub released_ms: Option<u64>,
    pub final_ms: Option<u64>,
    pub submitted_ms: Option<u64>,
    pub received_ms: Option<u64>,
    disarmed: Option<DraftReason>,
}

#[derive(Clone, Debug)]
pub struct Submission {
    pub id: String,
    pub recipient: Recipient,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendOutcome {
    Accepted,
    Rejected,
    Unconfirmed,
}

#[derive(Default)]
pub struct VoiceRoute {
    pub attempt: Option<Attempt>,
}

impl VoiceRoute {
    pub fn begin(&mut self, recipient: Recipient, now_ms: u64) -> Result<String, &'static str> {
        if self.attempt.as_ref().is_some_and(|a| {
            matches!(
                a.phase,
                RoutePhase::Recording | RoutePhase::Finalizing | RoutePhase::Submitting
            )
        }) {
            return Err("Finish the current recording or delivery first.");
        }
        if !recipient.can_receive() {
            return Err(
                "Choose a connected, ready agent; permission prompts need your attention in Herdr.",
            );
        }
        let id = uuid::Uuid::new_v4().to_string();
        self.attempt = Some(Attempt {
            id: id.clone(),
            recipient,
            phase: RoutePhase::Recording,
            text: String::new(),
            history_id: None,
            started_ms: now_ms,
            released_ms: None,
            final_ms: None,
            submitted_ms: None,
            received_ms: None,
            disarmed: None,
        });
        Ok(id)
    }

    pub fn release(&mut self, now_ms: u64) -> bool {
        let Some(a) = self
            .attempt
            .as_mut()
            .filter(|a| a.phase == RoutePhase::Recording)
        else {
            return false;
        };
        a.phase = RoutePhase::Finalizing;
        a.released_ms = Some(now_ms);
        true
    }

    pub fn recognized(
        &mut self,
        id: &str,
        text: &str,
        history_id: Option<String>,
        now_ms: u64,
    ) -> bool {
        let Some(a) = self
            .attempt
            .as_mut()
            .filter(|a| a.id == id && a.phase == RoutePhase::Finalizing)
        else {
            return false;
        };
        a.text = text.to_owned();
        a.history_id = history_id;
        a.final_ms = Some(now_ms);
        let reason = a.disarmed.or_else(|| {
            if text.trim().is_empty() {
                Some(DraftReason::Empty)
            } else if text.len() > MAX_MESSAGE_BYTES {
                Some(DraftReason::TooLong)
            } else {
                None
            }
        });
        a.phase = match reason {
            Some(reason) => RoutePhase::Draft(reason),
            None => RoutePhase::Grace {
                deadline_ms: now_ms.saturating_add(CANCEL_WINDOW_MS),
            },
        };
        true
    }

    /// Cancellation is synchronous. Once submission starts the UI must not promise recall.
    pub fn disarm(&mut self, reason: DraftReason) -> bool {
        let Some(a) = self.attempt.as_mut() else {
            return false;
        };
        match a.phase {
            RoutePhase::Recording | RoutePhase::Finalizing => a.disarmed = Some(reason),
            RoutePhase::Grace { .. } => {
                a.disarmed = Some(reason);
                a.phase = RoutePhase::Draft(reason);
            }
            _ => return false,
        }
        true
    }

    pub fn abandon_capture(&mut self, reason: DraftReason) {
        if let Some(a) = self.attempt.as_mut()
            && matches!(
                a.phase,
                RoutePhase::Recording | RoutePhase::Finalizing | RoutePhase::Grace { .. }
            )
        {
            a.disarmed = Some(reason);
            a.phase = RoutePhase::Draft(reason);
        }
    }

    pub fn observe(&mut self, recipients: &[Recipient]) {
        if self.attempt.as_ref().is_some_and(|a| {
            matches!(
                a.phase,
                RoutePhase::Recording | RoutePhase::Finalizing | RoutePhase::Grace { .. }
            ) && !recipients
                .iter()
                .any(|r| a.recipient.same_instance(r) && r.can_receive())
        }) {
            self.disarm(DraftReason::Unavailable);
        }
    }

    pub fn recording_timed_out(&self, now_ms: u64) -> bool {
        self.attempt.as_ref().is_some_and(|a| {
            a.phase == RoutePhase::Recording
                && now_ms.saturating_sub(a.started_ms) >= MAX_RECORDING_MS
        })
    }

    /// Consumes the automatic-send permission before asynchronous network work starts.
    pub fn take_submission(&mut self, now_ms: u64) -> Option<Submission> {
        let a = self.attempt.as_mut()?;
        if !matches!(a.phase, RoutePhase::Grace { deadline_ms } if now_ms >= deadline_ms) {
            return None;
        }
        if matches!(a.phase, RoutePhase::Grace { deadline_ms } if now_ms.saturating_sub(deadline_ms) > 1_000)
        {
            a.phase = RoutePhase::Draft(DraftReason::Interrupted);
            return None;
        }
        a.phase = RoutePhase::Submitting;
        a.submitted_ms = Some(now_ms);
        Some(Submission {
            id: a.id.clone(),
            recipient: a.recipient.clone(),
            text: a.text.clone(),
        })
    }

    pub fn delivered(&mut self, id: &str, outcome: SendOutcome, now_ms: u64) -> bool {
        let Some(a) = self
            .attempt
            .as_mut()
            .filter(|a| a.id == id && a.phase == RoutePhase::Submitting)
        else {
            return false;
        };
        a.received_ms = Some(now_ms);
        a.phase = match outcome {
            SendOutcome::Accepted => RoutePhase::Sent,
            SendOutcome::Rejected => RoutePhase::Draft(DraftReason::Rejected),
            SendOutcome::Unconfirmed => RoutePhase::Unconfirmed,
        };
        true
    }
}
