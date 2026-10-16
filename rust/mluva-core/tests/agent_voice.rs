use mluva_core::agent_voice::*;
use std::os::unix::fs::PermissionsExt;

fn recipient() -> Recipient {
    Recipient {
        socket: "/tmp/private/herdr.sock".into(),
        server_epoch: "server-a".into(),
        pane_id: "w1:p2".into(),
        guard: "process-session-a".into(),
        name: "builder".into(),
        kind: "codex".into(),
        project: "example".into(),
        status: AgentStatus::Idle,
    }
}

#[test]
fn release_finalization_grace_and_acceptance_have_distinct_boundaries() {
    let mut route = VoiceRoute::default();
    let id = route.begin(recipient(), 0).unwrap();
    assert!(!route.recognized(&id, "volatile", None, 900));
    assert!(route.take_submission(20_000).is_none());
    assert!(route.release(21_000));
    assert!(!route.release(21_010));
    assert!(route.recognized(&id, "Final original.", Some("history-a".into()), 24_000));
    assert!(route.take_submission(25_999).is_none());
    let submission = route.take_submission(26_000).unwrap();
    assert_eq!(submission.text, "Final original.");
    assert_eq!(submission.recipient.guard, "process-session-a");
    assert!(route.take_submission(26_001).is_none());
    assert!(!route.disarm(DraftReason::Cancelled));
    assert!(!route.delivered("old-message", SendOutcome::Accepted, 26_010));
    assert!(route.delivered(&id, SendOutcome::Accepted, 26_100));
    assert!(!route.delivered(&id, SendOutcome::Unconfirmed, 26_200));
    let a = route.attempt.as_ref().unwrap();
    assert_eq!(a.phase, RoutePhase::Sent);
    assert_eq!(a.history_id.as_deref(), Some("history-a"));
    assert_eq!(a.released_ms, Some(21_000));
    assert_eq!(a.final_ms, Some(24_000));
}

#[test]
fn cancellation_and_draft_actions_win_before_network_dispatch() {
    for phase in 0..3 {
        for reason in [
            DraftReason::Kept,
            DraftReason::Edited,
            DraftReason::Cancelled,
            DraftReason::RecipientChanged,
            DraftReason::Interrupted,
        ] {
            let mut route = VoiceRoute::default();
            let id = route.begin(recipient(), 0).unwrap();
            if phase > 0 {
                route.release(100);
            }
            if phase > 1 {
                route.recognized(&id, "kept", None, 200);
            }
            assert!(route.disarm(reason));
            if phase == 0 {
                route.release(100);
            }
            if phase < 2 {
                route.recognized(&id, "kept", None, 200);
            }
            assert!(route.take_submission(2_200).is_none());
            assert_eq!(route.attempt.as_ref().unwrap().text, "kept");
            assert_eq!(
                route.attempt.as_ref().unwrap().phase,
                RoutePhase::Draft(reason)
            );
        }
    }
}

#[test]
fn identity_changes_never_retarget_and_timeouts_never_replay() {
    for change in 0..5 {
        let mut route = VoiceRoute::default();
        let id = route.begin(recipient(), 0).unwrap();
        route.release(100);
        route.recognized(&id, "private message", None, 200);
        let mut other = recipient();
        match change {
            0 => other.server_epoch = "restarted".into(),
            1 => other.guard = "replacement agent with the same name".into(),
            2 => other.pane_id = "w2:p2".into(),
            3 => other.socket = "/tmp/other-server.sock".into(),
            _ => other.status = AgentStatus::Blocked,
        }
        route.observe(&[other]);
        assert!(route.take_submission(2_200).is_none());
        assert_eq!(route.attempt.as_ref().unwrap().recipient, recipient());
    }
    let mut route = VoiceRoute::default();
    let id = route.begin(recipient(), 0).unwrap();
    route.release(100);
    route.recognized(&id, "message", None, 200);
    route.take_submission(2_200).unwrap();
    route.delivered(&id, SendOutcome::Unconfirmed, 4_000);
    assert!(route.take_submission(4_001).is_none());
    assert_eq!(
        route.attempt.as_ref().unwrap().phase,
        RoutePhase::Unconfirmed
    );
    let after_restart = VoiceRoute::default();
    assert!(after_restart.attempt.is_none());
}

#[test]
fn empty_failed_lost_release_and_suspended_work_never_send() {
    for text in ["", "  \n ", &"x".repeat(MAX_MESSAGE_BYTES + 1)] {
        let mut route = VoiceRoute::default();
        let id = route.begin(recipient(), 0).unwrap();
        assert!(route.lost_release(MAX_HOLD_MS));
        route.release(100);
        route.recognized(&id, text, None, 200);
        assert!(route.take_submission(2_200).is_none());
    }
    let mut route = VoiceRoute::default();
    let id = route.begin(recipient(), 0).unwrap();
    route.release(100);
    route.recognized(&id, "ready before suspension", None, 200);
    assert!(route.take_submission(60_000).is_none());
    assert_eq!(
        route.attempt.as_ref().unwrap().phase,
        RoutePhase::Draft(DraftReason::Interrupted)
    );
    let id = route.begin(recipient(), 61_000).unwrap();
    route.abandon_capture(DraftReason::Cancelled);
    assert!(!route.recognized(&id, "late result after cancellation", None, 62_000));
}

#[test]
fn settings_are_opt_in_private_and_preserve_invalid_files() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("voice.json");
    let mut settings = VoiceSettings::load(&path).unwrap();
    assert!(!settings.enabled && !settings.spoken_updates);
    settings.selected = Some(recipient());
    settings.save(&path, "F9").unwrap();
    assert_eq!(VoiceSettings::load(&path).unwrap(), settings);
    assert_eq!(path.metadata().unwrap().permissions().mode() & 0o777, 0o600);
    settings.hold_key = "F9".into();
    assert!(settings.save(&path, "F9").is_err());
    assert_eq!(VoiceSettings::load(&path).unwrap().hold_key, "F8");
    std::fs::write(&path, b"invalid original").unwrap();
    assert!(VoiceSettings::load(&path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"invalid original");
}
