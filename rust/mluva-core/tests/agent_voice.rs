use mluva_core::agent_voice::*;
use std::os::unix::fs::PermissionsExt;

fn herdr_pane(pane: &str, guard: &str) -> Option<HerdrPane> {
    Some(HerdrPane {
        socket: "/tmp/private/herdr.sock".into(),
        server_epoch: "server-a".into(),
        pane_id: pane.into(),
        guard: guard.into(),
    })
}

fn recipient() -> Target {
    Target {
        herdr: herdr_pane("w1:p2", "process-session-a"),
        name: "builder".into(),
        title: String::new(),
        kind: AgentKind::CodexCli,
        project: "example".into(),
        status: AgentStatus::Idle,
        ..Default::default()
    }
}

/// A Claude Code session Herdr cannot send to, or one found only in Claude Code's registry.
fn draft_only() -> Target {
    Target {
        name: "notes-3f".into(),
        title: "Synthetic notes".into(),
        kind: AgentKind::ClaudeCode,
        project: "/synthetic/notes".into(),
        status: AgentStatus::Idle,
        session_id: "session-a".into(),
        pid: 4_100,
        ..Default::default()
    }
}

#[test]
fn draft_hint_says_why_nothing_reaches_the_session() {
    // Outside Herdr, a Claude Code session without an inbox (older, `--bare`, `--restricted`).
    assert_eq!(
        draft_only().draft_hint(),
        "Draft only: this session has no inbox"
    );
    // A Claude Code session typed into a Herdr pane can be guarded; without a guard yet,
    // the hint must not tell the user to restart it from Herdr.
    let in_herdr = Target {
        herdr: herdr_pane("w1:p4", ""),
        ..draft_only()
    };
    assert_eq!(
        in_herdr.draft_hint(),
        "Draft only: Herdr can't send to it now"
    );
}

#[test]
fn draft_only_sessions_match_by_native_session_and_process_and_never_send() {
    let native = draft_only();
    assert!(native.draft_only() && !native.can_receive());
    let mut route = VoiceRoute::default();
    assert!(route.begin(native.clone(), 0).is_err());
    // The same session seen through Herdr, unmanaged, is the same choice.
    let in_herdr = Target {
        herdr: herdr_pane("w1:p4", ""),
        ..draft_only()
    };
    assert!(native.same_instance(&in_herdr));
    // A resumed session in a new process, or a new session in the same pane, is not.
    let resumed = Target {
        pid: 4_200,
        ..draft_only()
    };
    let replaced = Target {
        session_id: "session-b".into(),
        ..in_herdr.clone()
    };
    assert!(!native.same_instance(&resumed));
    assert!(!in_herdr.same_instance(&replaced));
    // Herdr's guard coming or going does not change which session it is.
    let guarded = Target {
        herdr: herdr_pane("w1:p4", &"g".repeat(64)),
        ..in_herdr.clone()
    };
    assert!(guarded.same_instance(&in_herdr) && in_herdr.same_instance(&guarded));
    // A sighting without the native session falls back to Herdr's guarded identity.
    assert!(guarded.same_instance(&Target {
        session_id: String::new(),
        pid: 0,
        ..guarded.clone()
    }));
    let missing = Target {
        missing: true,
        ..draft_only()
    };
    assert!(missing.same_instance(&native) && !missing.draft_only());
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
    assert_eq!(
        submission.target.herdr.as_ref().unwrap().guard,
        "process-session-a"
    );
    assert!(route.take_submission(26_001).is_none());
    assert!(!route.disarm(DraftReason::Cancelled));
    assert!(!route.delivered("old-message", SendOutcome::Accepted, 26_010));
    assert!(route.delivered(&id, SendOutcome::Accepted, 26_100));
    assert!(!route.delivered(&id, SendOutcome::Unconfirmed, 26_200));
    let a = route.attempt.as_ref().unwrap();
    assert_eq!(a.phase, RoutePhase::Delivered);
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
fn a_second_press_in_the_cancel_window_sends_once_to_the_same_target() {
    let mut route = VoiceRoute::default();
    let id = route.begin(recipient(), 0).unwrap();
    route.release(100);
    route.recognized(&id, "send me now", Some("history-1".into()), 200);
    assert!(matches!(
        route.attempt.as_ref().unwrap().phase,
        RoutePhase::Grace { .. }
    ));
    // A new recording never replaces a message that is still waiting to send.
    assert!(route.begin(recipient(), 300).is_err());
    let submission = route
        .send_now(400)
        .expect("the second press ends the countdown");
    assert_eq!(
        (submission.id.as_str(), submission.text.as_str()),
        (id.as_str(), "send me now")
    );
    assert_eq!(submission.target, recipient());
    assert_eq!(
        route.attempt.as_ref().unwrap().phase,
        RoutePhase::Submitting
    );
    // Once taken, neither another press nor the countdown deadline can send it again.
    assert!(route.send_now(500).is_none());
    assert!(route.take_submission(2_200).is_none());
    assert!(route.delivered(&id, SendOutcome::Accepted, 600));
    assert_eq!(route.attempt.as_ref().unwrap().phase, RoutePhase::Delivered);
    assert!(route.send_now(700).is_none());
}

#[test]
fn a_press_outside_the_cancel_window_never_sends_or_replays() {
    let mut route = VoiceRoute::default();
    assert!(route.send_now(0).is_none());
    let id = route.begin(recipient(), 0).unwrap();
    assert!(route.send_now(10).is_none(), "recording");
    route.release(100);
    assert!(route.send_now(150).is_none(), "finalizing");
    route.recognized(&id, "kept", None, 200);
    assert!(route.disarm(DraftReason::Edited));
    assert!(route.send_now(300).is_none(), "a draft stays a draft");
    for outcome in [SendOutcome::Unconfirmed, SendOutcome::Rejected] {
        let mut route = VoiceRoute::default();
        let id = route.begin(recipient(), 0).unwrap();
        route.release(100);
        route.recognized(&id, "once", None, 200);
        assert!(route.send_now(250).is_some());
        assert!(route.delivered(&id, outcome, 300));
        assert!(route.send_now(400).is_none());
        assert!(route.take_submission(2_200).is_none());
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
        let pane = other.herdr.as_mut().unwrap();
        match change {
            0 => pane.server_epoch = "restarted".into(),
            1 => pane.guard = "replacement agent with the same name".into(),
            2 => pane.pane_id = "w2:p2".into(),
            3 => pane.socket = "/tmp/other-server.sock".into(),
            _ => other.status = AgentStatus::Blocked,
        }
        route.observe(&[other]);
        assert!(route.take_submission(2_200).is_none());
        assert_eq!(route.attempt.as_ref().unwrap().target, recipient());
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
fn empty_failed_timed_out_and_suspended_work_never_send() {
    for text in ["", "  \n ", &"x".repeat(MAX_MESSAGE_BYTES + 1)] {
        let mut route = VoiceRoute::default();
        let id = route.begin(recipient(), 0).unwrap();
        assert!(route.recording_timed_out(MAX_RECORDING_MS));
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
    // A choice saved before native sessions existed still loads with its guarded identity.
    let legacy = serde_json::json!({"enabled": true, "selected": {"socket": "/tmp/private/herdr.sock",
        "server_epoch": "server-a", "pane_id": "w1:p2", "guard": "process-session-a", "name": "builder",
        "kind": "codex", "project": "example", "status": "idle"}});
    std::fs::write(&path, legacy.to_string()).unwrap();
    let loaded = VoiceSettings::load(&path).unwrap().selected.unwrap();
    assert!(loaded.same_instance(&recipient()) && loaded.session_id.is_empty());
    // The draft-only choice round-trips; the in-memory missing marker is never saved.
    settings.hold_key = "F8".into();
    settings.selected = Some(Target {
        missing: true,
        ..draft_only()
    });
    settings.save(&path, "F9").unwrap();
    assert_eq!(
        VoiceSettings::load(&path).unwrap().selected,
        Some(draft_only())
    );
    std::fs::write(&path, b"invalid original").unwrap();
    assert!(VoiceSettings::load(&path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"invalid original");
}

#[test]
fn a_queued_message_can_be_taken_held_or_withdrawn_busy_and_none_is_replayed() {
    let deliver = |outcome: SendOutcome| {
        let mut route = VoiceRoute::default();
        let id = route.begin(recipient(), 0).unwrap();
        route.release(100);
        route.recognized(&id, "queued text", Some("history-q".into()), 200);
        route.take_submission(2_200).unwrap();
        assert!(route.delivered(&id, outcome, 2_300));
        assert!(route.take_submission(5_000).is_none() && route.send_now(5_000).is_none());
        (route, id)
    };
    let (mut queued, id) = deliver(SendOutcome::Accepted);
    let attempt = queued.attempt.as_ref().unwrap();
    assert_eq!(attempt.phase, RoutePhase::Delivered);
    // The fixture's route is Herdr: accepted means sent.
    assert_eq!(attempt.delivered_outcome(), Some("sent"));
    // The agent taking it later is learned once, by the same attempt only.
    assert!(!queued.taken("another-attempt"));
    assert!(queued.taken(&id));
    assert!(!queued.taken(&id));
    assert_eq!(
        queued.attempt.as_ref().unwrap().delivered_outcome(),
        Some("delivered")
    );
    let (taken, _) = deliver(SendOutcome::Taken);
    assert_eq!(
        taken.attempt.as_ref().unwrap().delivered_outcome(),
        Some("delivered")
    );
    let (held, id) = deliver(SendOutcome::Held);
    assert_eq!(held.attempt.as_ref().unwrap().phase, RoutePhase::Held);
    assert_eq!(
        held.attempt.as_ref().unwrap().delivered_outcome(),
        Some("held")
    );
    let mut held = held;
    assert!(!held.taken(&id), "a held message is not delivered");
    let (busy, _) = deliver(SendOutcome::Busy);
    assert_eq!(
        busy.attempt.as_ref().unwrap().phase,
        RoutePhase::Draft(DraftReason::Busy)
    );
    assert_eq!(busy.attempt.as_ref().unwrap().delivered_outcome(), None);
}

#[test]
fn text_longer_than_the_route_carries_stays_a_draft_rather_than_being_cut() {
    let chat = Target {
        kind: AgentKind::ClaudeDesktop,
        name: "claude-new-chat".into(),
        status: AgentStatus::Idle,
        app: Some(AppLink::ClaudeNewChat),
        ..Default::default()
    };
    for (length, phase_is_draft) in [(14_336, false), (14_337, true)] {
        let mut route = VoiceRoute::default();
        let id = route.begin(chat.clone(), 0).unwrap();
        route.release(100);
        // Two-byte characters: the limit counts characters, not bytes.
        route.recognized(&id, &"ž".repeat(length), None, 200);
        assert_eq!(
            route.attempt.as_ref().unwrap().phase == RoutePhase::Draft(DraftReason::TooLong),
            phase_is_draft,
            "{length}"
        );
    }
}

/// A Claude Code session in Herdr, guarded, with its own inbox as well.
fn guarded_with_inbox(guard: &str, status: AgentStatus) -> Target {
    Target {
        herdr: herdr_pane("w1:p4", guard),
        inbox: Some(ClaudeInbox {
            entry: "/run/user/fixture/claude/sessions/4100.json".into(),
            socket: "/run/user/fixture/cc-socks/4100.sock".into(),
            proc_start: "123456".into(),
        }),
        started: "123456".into(),
        status,
        ..draft_only()
    }
}

#[test]
fn a_pending_message_follows_its_session_across_herdrs_guard_and_uses_the_route_of_the_moment() {
    let mut route = VoiceRoute::default();
    let chosen = guarded_with_inbox(&"g".repeat(64), AgentStatus::Idle);
    let id = route.begin(chosen.clone(), 0).unwrap();
    // The session waits on a prompt: Herdr withdraws its guard, and is renamed meanwhile.
    let waiting = Target {
        title: "Renamed meanwhile".into(),
        ..guarded_with_inbox("", AgentStatus::Blocked)
    };
    route.observe(std::slice::from_ref(&waiting));
    let attempt = route.attempt.as_ref().unwrap();
    assert_eq!(
        attempt.phase,
        RoutePhase::Recording,
        "not turned into a draft"
    );
    assert_eq!(attempt.target.route(), Route::ClaudeInbox);
    assert_eq!(
        attempt.target.display_name(),
        "Synthetic notes",
        "the chosen name stays"
    );
    route.release(100);
    route.recognized(&id, "follow me", None, 200);
    // Guarded again before the countdown ends: Herdr's prompt sends it.
    route.observe(&[guarded_with_inbox(&"h".repeat(64), AgentStatus::Idle)]);
    let submission = route.take_submission(2_200).unwrap();
    assert_eq!(submission.target.route(), Route::Herdr);
    assert_eq!(
        submission.target.herdr.as_ref().unwrap().guard,
        "h".repeat(64)
    );
    // A different process in the same pane never takes the message.
    let mut route = VoiceRoute::default();
    let id = route.begin(chosen, 0).unwrap();
    route.release(100);
    route.recognized(&id, "never elsewhere", None, 200);
    route.observe(&[Target {
        pid: 4_999,
        ..guarded_with_inbox(&"g".repeat(64), AgentStatus::Idle)
    }]);
    assert_eq!(
        route.attempt.as_ref().unwrap().phase,
        RoutePhase::Draft(DraftReason::Unavailable)
    );
}
