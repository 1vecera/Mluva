//! The shared agent target model as the shell sees it: every session Mluva can dictate to,
//! what reaches it, and what became of a dictation sent there.
//!
//! The bridge keeps the targets of the latest discovery pass, so what a transport needs and
//! never saves (a Claude Code inbox, a live Codex thread) stays in the core. The shell names a
//! target by the record it was shown, and recording begins only for the same session in the
//! latest pass.
use mluva_core::{
    agent_voice::{Attempt, DraftReason, RoutePhase, SendOutcome, Target},
    history::HistoryStore,
};
use serde_json::{Value, json};

/// One discovered target with the core's own judgements about it.
pub(crate) fn describe(target: &Target) -> Value {
    let delivery = target.delivery();
    json!({
        "target": target,
        "display_name": target.display_name(),
        "kind": target.kind.label(),
        "place": target.place(),
        "delivery": delivery.as_str(),
        "verb": delivery.verb(),
        "delivery_hint": target.delivery_hint(),
        "draft_only": target.draft_only(),
        "draft_hint": target.draft_hint(),
        "can_receive": target.can_receive(),
        "spoken_status": target.spoken_status(),
    })
}

/// The attempt's phase, and why a draft was kept.
pub(crate) fn phase(phase: &RoutePhase) -> Value {
    match phase {
        RoutePhase::Recording => json!({"phase": "recording"}),
        RoutePhase::Finalizing => json!({"phase": "finalizing"}),
        RoutePhase::Grace { deadline_ms } => json!({"phase": "grace", "deadline_ms": deadline_ms}),
        RoutePhase::Draft(why) => json!({"phase": "draft", "reason": reason_name(*why)}),
        RoutePhase::Submitting => json!({"phase": "submitting"}),
        RoutePhase::Delivered => json!({"phase": "delivered"}),
        RoutePhase::Held => json!({"phase": "held"}),
        RoutePhase::Unconfirmed => json!({"phase": "unconfirmed"}),
    }
}

/// The current attempt as the shell shows it.
pub(crate) fn attempt(attempt: &Attempt) -> Value {
    let mut state = phase(&attempt.phase);
    let target = &attempt.target;
    state["id"] = json!(attempt.id);
    state["target"] = json!(target);
    state["display_name"] = json!(target.display_name());
    state["delivery"] = json!(target.delivery().as_str());
    state["delivery_hint"] = json!(target.delivery_hint());
    state["place_to_check"] = json!(target.place_to_check());
    state["taken"] = json!(attempt.taken);
    state["text"] = json!(attempt.text);
    state["history_id"] = json!(attempt.history_id);
    state
}

fn reason_name(reason: DraftReason) -> &'static str {
    match reason {
        DraftReason::Kept => "kept",
        DraftReason::Edited => "edited",
        DraftReason::RecipientChanged => "recipient_changed",
        DraftReason::Unavailable => "unavailable",
        DraftReason::Cancelled => "cancelled",
        DraftReason::Interrupted => "interrupted",
        DraftReason::Empty => "empty",
        DraftReason::TooLong => "too_long",
        DraftReason::Rejected => "rejected",
        DraftReason::Busy => "busy",
    }
}

/// The reasons a shell may give for keeping a draft.
pub(crate) fn draft_reason(name: &str) -> Result<DraftReason, String> {
    Ok(match name {
        "kept" => DraftReason::Kept,
        "edited" => DraftReason::Edited,
        "recipient_changed" => DraftReason::RecipientChanged,
        "unavailable" => DraftReason::Unavailable,
        "cancelled" => DraftReason::Cancelled,
        "interrupted" => DraftReason::Interrupted,
        _ => return Err("Unknown reason for keeping a draft.".into()),
    })
}

/// The `route` event's word for what a transport reported.
pub(crate) fn outcome_name(outcome: SendOutcome) -> &'static str {
    match outcome {
        SendOutcome::Accepted => "accepted",
        SendOutcome::Taken => "taken",
        SendOutcome::Held => "held",
        SendOutcome::Busy => "busy",
        SendOutcome::Rejected => "rejected",
        SendOutcome::Unconfirmed => "unconfirmed",
    }
}

/// History's word for what became of the attempt's text, as the Linux app records it; `None`
/// while it is still under way, and for no speech, which the row already says.
fn history_outcome(attempt: &Attempt) -> Option<&'static str> {
    match attempt.phase {
        RoutePhase::Delivered | RoutePhase::Held => attempt.delivered_outcome(),
        RoutePhase::Unconfirmed => Some("unconfirmed"),
        RoutePhase::Draft(reason) => match reason {
            DraftReason::Kept
            | DraftReason::Edited
            | DraftReason::Interrupted
            | DraftReason::TooLong => Some("draft-kept"),
            DraftReason::RecipientChanged | DraftReason::Unavailable | DraftReason::Rejected => {
                Some("recipient-unavailable")
            }
            DraftReason::Busy => Some("busy"),
            DraftReason::Cancelled => Some("cancelled"),
            DraftReason::Empty => None,
        },
        _ => None,
    }
}

/// A queued message the agent took after another dictation had started: its own row learns.
pub(crate) fn record_taken(history: &HistoryStore, row: &str) {
    write_outcome(history, row, "delivered");
}

/// Tell the attempt's History row what became of its text. Nothing without a row, which
/// Incognito never writes, and nothing when the row already says so.
pub(crate) fn record_outcome(history: &HistoryStore, attempt: &Attempt) {
    if let (Some(outcome), Some(id)) = (history_outcome(attempt), attempt.history_id.as_deref()) {
        write_outcome(history, id, outcome);
    }
}

fn write_outcome(history: &HistoryStore, id: &str, outcome: &str) {
    let Ok(entry) = history.find(id) else {
        return;
    };
    if entry.delivery_outcome != outcome {
        let _ = history.mark_delivered(
            id,
            &entry.delivered_text,
            outcome,
            entry.retained_audio_path.is_some(),
            None,
        );
    }
}
