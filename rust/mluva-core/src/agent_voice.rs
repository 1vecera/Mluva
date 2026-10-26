//! Toggle recording with agent routing is a one-shot delivery intent, independent of capture providers.
//! No pending send is serialized or replayed after restart. The recipient is a [`Target`],
//! whatever transport reaches it.

use serde::{Deserialize, Serialize};
use std::{fs, io::Read, path::Path};

pub const CANCEL_WINDOW_MS: u64 = 2_000;
pub const MAX_RECORDING_MS: u64 = 5 * 60 * 1_000;
pub const MAX_MESSAGE_BYTES: usize = 32_768;

pub use crate::agent_target::{
    AgentKind, AgentStatus, AppLink, ClaudeInbox, CodexThread, Delivery, HerdrPane, Host, Route,
    Target,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VoiceSettings {
    pub enabled: bool,
    pub socket: Option<String>,
    pub selected: Option<Target>,
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
                |_| "Agent routing settings could not be read; the original file was preserved.",
            ),
            Ok(_) => Err("Agent routing settings are too large; the original file was preserved."),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(_) => {
                Err("Agent routing settings could not be read; the original file was preserved.")
            }
        }
    }

    pub fn save(&self, path: &Path, recording_key: &str) -> Result<(), &'static str> {
        self.validate(recording_key)?;
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|_| "Agent routing settings could not be saved.")?;
        crate::private_files::atomic_write_private(path, &bytes)
            .map_err(|_| "Agent routing settings could not be saved.")
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
    /// The session was busy or not open, so the transport withdrew the message unsent.
    Busy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoutePhase {
    Recording,
    Finalizing,
    Grace {
        deadline_ms: u64,
    },
    Draft(DraftReason),
    Submitting,
    /// The transport accepted the message at the target's delivery level: sent, queued or
    /// placed in the composer. The agent's own work on it is separate.
    Delivered,
    /// The session holds the message until the person approves it there. Never resent.
    Held,
    Unconfirmed,
}

#[derive(Clone, Debug)]
pub struct Attempt {
    pub id: String,
    pub target: Target,
    pub phase: RoutePhase,
    pub text: String,
    pub history_id: Option<String>,
    pub started_ms: u64,
    pub released_ms: Option<u64>,
    pub final_ms: Option<u64>,
    pub submitted_ms: Option<u64>,
    pub received_ms: Option<u64>,
    /// A queued message the agent has since taken into a turn.
    pub taken: bool,
    disarmed: Option<DraftReason>,
}

impl Attempt {
    /// History's word for what became of a delivered or held message.
    pub fn delivered_outcome(&self) -> Option<&'static str> {
        match self.phase {
            RoutePhase::Delivered if self.taken => Some("delivered"),
            RoutePhase::Delivered => Some(self.target.delivery().outcome()),
            RoutePhase::Held => Some("held"),
            _ => None,
        }
    }
}

impl Attempt {
    fn start_submission(&mut self, now_ms: u64) -> Submission {
        self.phase = RoutePhase::Submitting;
        self.submitted_ms = Some(now_ms);
        Submission {
            id: self.id.clone(),
            target: self.target.clone(),
            text: self.text.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Submission {
    pub id: String,
    pub target: Target,
    pub text: String,
}

/// What a transport reports for one delivery attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendOutcome {
    /// Accepted at the target's delivery level, for exactly that session: sent, queued or
    /// placed in its composer.
    Accepted,
    /// Queued, and the agent has already taken it into a turn.
    Taken,
    /// The session holds it until the person approves it there.
    Held,
    /// The session was busy or not open; the transport withdrew it before it ran.
    Busy,
    /// Refused before anything reached the session.
    Rejected,
    /// Some input may have reached the session; it is never written again automatically.
    Unconfirmed,
}

#[derive(Default)]
pub struct VoiceRoute {
    pub attempt: Option<Attempt>,
}

impl VoiceRoute {
    pub fn begin(&mut self, target: Target, now_ms: u64) -> Result<String, &'static str> {
        if self.attempt.as_ref().is_some_and(|a| {
            matches!(
                a.phase,
                RoutePhase::Recording | RoutePhase::Finalizing | RoutePhase::Submitting
            )
        }) {
            return Err("Finish the current recording or delivery first.");
        }
        // A pending send is never replaced unseen: it is sent or cancelled first.
        if self
            .attempt
            .as_ref()
            .is_some_and(|a| matches!(a.phase, RoutePhase::Grace { .. }))
        {
            return Err("A message is about to send. Press again to send it now, or cancel it.");
        }
        if !target.can_receive() {
            return Err(
                "Choose a connected, ready agent; a session waiting for your answer needs you first.",
            );
        }
        let id = uuid::Uuid::new_v4().to_string();
        self.attempt = Some(Attempt {
            id: id.clone(),
            target,
            phase: RoutePhase::Recording,
            text: String::new(),
            history_id: None,
            started_ms: now_ms,
            released_ms: None,
            final_ms: None,
            submitted_ms: None,
            received_ms: None,
            taken: false,
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
            } else if text.len() > MAX_MESSAGE_BYTES
                || a.target
                    .route()
                    .max_chars()
                    .is_some_and(|max| text.chars().count() > max)
            {
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

    pub fn observe(&mut self, targets: &[Target]) {
        if self.attempt.as_ref().is_some_and(|a| {
            matches!(
                a.phase,
                RoutePhase::Recording | RoutePhase::Finalizing | RoutePhase::Grace { .. }
            ) && !targets
                .iter()
                .any(|t| a.target.same_instance(t) && t.can_receive())
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
        Some(a.start_submission(now_ms))
    }

    /// Ends the cancel window early: the pending send goes now, to the same target and text. Outside the window nothing is consumed, so an uncertain or
    /// submitted message is never replayed.
    pub fn send_now(&mut self, now_ms: u64) -> Option<Submission> {
        let a = self.attempt.as_mut()?;
        matches!(a.phase, RoutePhase::Grace { .. }).then(|| a.start_submission(now_ms))
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
            SendOutcome::Accepted => RoutePhase::Delivered,
            SendOutcome::Taken => {
                a.taken = true;
                RoutePhase::Delivered
            }
            SendOutcome::Held => RoutePhase::Held,
            SendOutcome::Busy => RoutePhase::Draft(DraftReason::Busy),
            SendOutcome::Rejected => RoutePhase::Draft(DraftReason::Rejected),
            SendOutcome::Unconfirmed => RoutePhase::Unconfirmed,
        };
        true
    }

    /// A queued message the agent took into a turn after the receipt was read. Only the same,
    /// still-delivered attempt learns it.
    pub fn taken(&mut self, id: &str) -> bool {
        match self
            .attempt
            .as_mut()
            .filter(|a| a.id == id && a.phase == RoutePhase::Delivered && !a.taken)
        {
            Some(a) => {
                a.taken = true;
                true
            }
            None => false,
        }
    }
}
