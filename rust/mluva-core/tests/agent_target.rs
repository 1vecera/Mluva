//! The target model: kinds, routes, delivery levels, identity and the saved shape.
use mluva_core::agent_target::*;
use serde_json::json;

fn guarded() -> Target {
    Target {
        herdr: Some(HerdrPane {
            socket: "/run/user/fixture/herdr.sock".into(),
            server_epoch: "epoch".into(),
            pane_id: "w1:p2".into(),
            guard: "a".repeat(64),
        }),
        name: "builder".into(),
        kind: AgentKind::CodexCli,
        project: "/synthetic/project".into(),
        status: AgentStatus::Idle,
        session_id: "0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b".into(),
        ..Target::default()
    }
}

fn outside() -> Target {
    Target {
        name: "notes-3f".into(),
        title: "Synthetic notes".into(),
        kind: AgentKind::ClaudeCode,
        project: "/synthetic/notes".into(),
        status: AgentStatus::Idle,
        session_id: "session-a".into(),
        pid: 4_100,
        ..Target::default()
    }
}

#[test]
fn kinds_keep_herdrs_names_and_tolerate_ones_this_version_does_not_know() {
    for (kind, name) in [
        (AgentKind::ClaudeCode, "claude"),
        (AgentKind::CodexCli, "codex"),
        (AgentKind::ClaudeDesktop, "claude_desktop"),
        (AgentKind::CodexDesktop, "codex_desktop"),
    ] {
        assert_eq!(serde_json::to_value(kind).unwrap(), json!(name));
        assert_eq!(kind.as_str(), name);
        assert_eq!(
            serde_json::from_value::<AgentKind>(json!(name)).unwrap(),
            kind
        );
    }
    assert_eq!(
        serde_json::from_value::<AgentKind>(json!("claude_code")).unwrap(),
        AgentKind::ClaudeCode
    );
    assert_eq!(
        serde_json::from_value::<AgentKind>(json!("future_agent")).unwrap(),
        AgentKind::Unknown
    );
    assert_eq!(AgentKind::from_herdr("shell"), None);
    assert!(AgentKind::ClaudeDesktop.is_desktop() && !AgentKind::CodexCli.is_desktop());
}

#[test]
fn delivery_levels_are_ordered_and_name_their_history_outcome() {
    assert!(Delivery::Send > Delivery::Queue);
    assert!(Delivery::Queue > Delivery::Prefill);
    assert!(Delivery::Prefill > Delivery::DraftOnly);
    let outcomes: Vec<_> = [
        Delivery::Send,
        Delivery::Queue,
        Delivery::Prefill,
        Delivery::DraftOnly,
    ]
    .iter()
    .map(|d| d.outcome())
    .collect();
    assert_eq!(outcomes, ["sent", "queued", "prefilled", "draft-kept"]);
    assert_eq!(
        serde_json::to_value(Delivery::DraftOnly).unwrap(),
        json!("draft_only")
    );
}

#[test]
fn only_a_guarded_herdr_pane_routes_and_herdr_never_answers_a_waiting_prompt() {
    let target = guarded();
    assert_eq!(
        (target.route(), target.delivery()),
        (Route::Herdr, Delivery::Send)
    );
    for (status, receives) in [
        (AgentStatus::Idle, true),
        (AgentStatus::Done, true),
        // Herdr writes into a working agent's composer without stopping it.
        (AgentStatus::Working, true),
        (AgentStatus::Blocked, false),
        (AgentStatus::Unknown, false),
    ] {
        let target = Target {
            status,
            ..guarded()
        };
        assert_eq!(target.can_receive(), receives, "{status:?}");
        assert!(!target.draft_only());
    }
    // An unguarded pane and a session outside Herdr are draft only, and say why.
    let mut unguarded = guarded();
    unguarded.herdr.as_mut().unwrap().guard.clear();
    for (target, hint) in [
        (unguarded, "Draft only: Herdr can't send to it now"),
        (outside(), "Draft only: this session has no inbox"),
    ] {
        assert_eq!(target.route(), Route::DraftOnly);
        assert_eq!(target.delivery(), Delivery::DraftOnly);
        assert!(target.draft_only() && !target.can_receive() && !target.has_transport());
        assert_eq!(target.draft_hint(), hint);
    }
    // A missing remembered choice is neither deliverable nor a draft target.
    let missing = Target {
        missing: true,
        ..guarded()
    };
    assert!(!missing.can_receive() && !missing.draft_only());
    // An unknown kind is never delivered to, whatever its route.
    let unknown = Target {
        kind: AgentKind::Unknown,
        ..guarded()
    };
    assert!(!unknown.can_receive());
}

#[test]
fn a_replaced_or_restarted_session_never_matches_a_remembered_choice() {
    let chosen = Target {
        pid: 4_200,
        started: "123456".into(),
        ..guarded()
    };
    let same = Target {
        title: "Renamed thread".into(),
        status: AgentStatus::Working,
        ..chosen.clone()
    };
    assert!(chosen.same_instance(&same), "renaming keeps the choice");
    // Herdr's guard, server or pane changing around the same process keeps the session: the
    // route is picked again at delivery.
    for change in 0..3 {
        let mut other = chosen.clone();
        let pane = other.herdr.as_mut().unwrap();
        match change {
            0 => pane.guard.clear(),
            1 => pane.guard = "b".repeat(64),
            _ => pane.server_epoch = "restarted".into(),
        }
        assert!(chosen.same_instance(&other), "change {change}");
    }
    // Another session, process, process start or product never matches.
    for change in 0..4 {
        let mut other = chosen.clone();
        match change {
            0 => other.session_id = "0199a1b2-c3d4-7e5f-8a9b-0c1d2effffff".into(),
            1 => other.pid = 4_300,
            2 => other.started = "999999".into(),
            _ => other.kind = AgentKind::CodexDesktop,
        }
        assert!(!chosen.same_instance(&other), "change {change}");
    }
    // Same name and session, but a new process: a resumed session is chosen again.
    let resumed = Target {
        pid: 4_200,
        ..outside()
    };
    assert!(!outside().same_instance(&resumed));
    // Without native sessions, Herdr's identity: the guard for guarded panes, else the pane.
    let bare = |pane: &str, guard: &str| Target {
        herdr: Some(HerdrPane {
            socket: "/run/user/fixture/herdr.sock".into(),
            server_epoch: "epoch".into(),
            pane_id: pane.into(),
            guard: guard.into(),
        }),
        name: pane.into(),
        kind: AgentKind::CodexCli,
        ..Target::default()
    };
    assert!(bare("w2:p1", "").same_instance(&bare("w2:p1", "")));
    assert!(!bare("w2:p1", "").same_instance(&bare("w2:p2", "")));
    assert!(bare("w2:p1", &"a".repeat(64)).same_instance(&bare("w2:p1", &"a".repeat(64))));
    assert!(!bare("w2:p1", &"a".repeat(64)).same_instance(&bare("w2:p1", &"b".repeat(64))));
    assert!(!bare("w2:p1", "").same_instance(&bare("w2:p1", &"a".repeat(64))));
    assert!(!bare("w2:p1", "").same_instance(&outside()));
}

#[test]
fn a_session_keeps_one_identity_while_herdrs_guard_comes_and_goes() {
    let mut session = Target {
        kind: AgentKind::ClaudeCode,
        pid: 4_100,
        started: "123456".into(),
        inbox: Some(ClaudeInbox {
            entry: "/run/user/fixture/claude/sessions/4100.json".into(),
            socket: "/run/user/fixture/cc-socks/4100.sock".into(),
            proc_start: "123456".into(),
        }),
        ..guarded()
    };
    let chosen = session.clone();
    assert_eq!(session.route(), Route::Herdr);
    // Waiting for the person: Herdr withdraws its guard, the inbox still queues to it.
    session.status = AgentStatus::Blocked;
    session.herdr.as_mut().unwrap().guard.clear();
    assert!(chosen.same_instance(&session));
    assert_eq!(session.route(), Route::ClaudeInbox);
    assert!(session.can_receive());
    // Ready again, guarded again: still the same choice, sent through Herdr.
    session.status = AgentStatus::Idle;
    session.herdr.as_mut().unwrap().guard = "c".repeat(64);
    assert!(chosen.same_instance(&session));
    assert_eq!(session.route(), Route::Herdr);
}

#[test]
fn the_command_line_selects_by_handle_pane_or_session_never_by_title() {
    let target = guarded();
    assert!(target.answers_to("builder"));
    assert!(target.answers_to("w1:p2"));
    assert!(target.answers_to("0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b"));
    let titled = outside();
    assert!(!titled.answers_to("Synthetic notes"));
    assert!(titled.answers_to("session-a"));
    let no_session = Target {
        session_id: String::new(),
        ..outside()
    };
    assert!(!no_session.answers_to(""));
}

#[test]
fn places_and_locators_tell_sessions_apart_without_herdr_fields_leaking() {
    assert_eq!(guarded().locator(), "w1:p2");
    assert_eq!(guarded().place(), "Herdr w1:p2");
    assert_eq!(guarded().place_to_check(), "Herdr");
    assert_eq!(outside().locator(), "notes-3f");
    assert_eq!(outside().place(), "terminal");
    let in_tmux = Target {
        host: Host::Tmux("work:@3.%7".into()),
        ..outside()
    };
    assert_eq!(
        (in_tmux.locator().as_str(), in_tmux.place().as_str()),
        ("work:@3.%7", "tmux work:@3.%7")
    );
    let background = Target {
        host: Host::Background,
        ..outside()
    };
    assert_eq!(background.place(), "background");
    let untitled = Target {
        title: String::new(),
        ..outside()
    };
    assert_eq!(untitled.locator(), "pid 4100");
    assert_eq!(
        guarded().whereabouts(),
        "Codex CLI · w1:p2 · Herdr builder · project"
    );
    assert_eq!(
        outside().whereabouts(),
        "Claude Code · notes-3f · terminal · notes"
    );
}

#[test]
fn the_saved_shape_stays_flat_and_loads_choices_from_before_targets() {
    let saved = serde_json::to_value(guarded()).unwrap();
    assert_eq!(saved["pane_id"], "w1:p2");
    assert_eq!(saved["guard"], "a".repeat(64));
    assert_eq!(saved["kind"], "codex");
    assert!(saved.get("herdr").is_none() && saved.get("missing").is_none());
    assert_eq!(serde_json::from_value::<Target>(saved).unwrap(), guarded());
    // A session outside Herdr saves empty Herdr fields and loads without a pane.
    let saved = serde_json::to_value(outside()).unwrap();
    assert_eq!(saved["socket"], "");
    let loaded: Target = serde_json::from_value(saved).unwrap();
    assert!(loaded.herdr.is_none());
    assert_eq!(loaded, outside());
    // Written by the release before targets: string kinds, no session fields.
    let legacy = json!({"socket": "/run/user/fixture/herdr.sock", "server_epoch": "epoch",
        "pane_id": "w1:p2", "guard": "a".repeat(64), "name": "builder", "kind": "codex",
        "project": "/synthetic/project", "status": "idle"});
    let loaded: Target = serde_json::from_value(legacy).unwrap();
    assert!(loaded.same_instance(&guarded()));
    assert_eq!(loaded.route(), Route::Herdr);
    // Unknown fields from a newer Mluva are ignored.
    let newer = json!({"name": "x", "kind": "codex_desktop", "future": {"nested": true}});
    assert_eq!(
        serde_json::from_value::<Target>(newer).unwrap().kind,
        AgentKind::CodexDesktop
    );
}

fn inbox() -> Option<ClaudeInbox> {
    Some(ClaudeInbox {
        entry: "/run/user/fixture/claude/sessions/4100.json".into(),
        socket: "/run/user/fixture/cc-socks/4100.sock".into(),
        proc_start: "123456".into(),
    })
}

#[test]
fn claude_codes_own_inbox_queues_when_herdr_cannot_send_and_never_outranks_it() {
    let queued = Target {
        inbox: inbox(),
        ..outside()
    };
    assert_eq!(
        (queued.route(), queued.delivery(), queued.can_receive()),
        (Route::ClaudeInbox, Delivery::Queue, true)
    );
    assert_eq!(
        queued.delivery_hint(),
        Some("Arrives as a message from another Claude session")
    );
    assert_eq!(queued.place_to_check(), "the session");
    // A working session queues the message behind its turn, and a waiting one behind its
    // prompt, which the message never answers. An unknown state is not queued to.
    for (status, receives) in [
        (AgentStatus::Working, true),
        (AgentStatus::Blocked, true),
        (AgentStatus::Unknown, false),
    ] {
        let target = Target {
            status,
            ..queued.clone()
        };
        assert_eq!(target.can_receive(), receives, "{status:?}");
    }
    // An unguarded Herdr pane with an inbox queues; Herdr's guard upgrades it to Send.
    let mut in_herdr = guarded();
    in_herdr.kind = AgentKind::ClaudeCode;
    in_herdr.inbox = inbox();
    assert_eq!(in_herdr.route(), Route::Herdr);
    in_herdr.herdr.as_mut().unwrap().guard.clear();
    assert_eq!(in_herdr.route(), Route::ClaudeInbox);
    // A Claude desktop Code session runs Claude Code and queues the same way; an inbox on
    // anything else is not a Claude Code inbox.
    let desktop = Target {
        kind: AgentKind::ClaudeDesktop,
        ..queued.clone()
    };
    assert_eq!(desktop.route(), Route::ClaudeInbox);
    let codex = Target {
        kind: AgentKind::CodexCli,
        ..queued.clone()
    };
    assert_eq!(codex.route(), Route::DraftOnly);
    assert_eq!(codex.draft_hint(), "Draft only: nothing can reach it");
    // The inbox and host are found afresh by discovery and never saved.
    let saved = serde_json::to_value(&queued).unwrap();
    assert!(saved.get("inbox").is_none() && saved.get("host").is_none());
    let loaded: Target = serde_json::from_value(saved).unwrap();
    assert!(loaded.inbox.is_none() && loaded.same_instance(&queued));
}

#[test]
fn a_new_claude_chat_is_one_prefill_target_with_its_own_identity_and_length_limit() {
    let chat = Target {
        kind: AgentKind::ClaudeDesktop,
        name: "claude-new-chat".into(),
        title: "New chat".into(),
        status: AgentStatus::Idle,
        app: Some(AppLink::ClaudeNewChat),
        ..Target::default()
    };
    assert_eq!(
        (chat.route(), chat.delivery(), chat.can_receive()),
        (Route::ClaudeNewChat, Delivery::Prefill, true)
    );
    assert_eq!(Route::ClaudeNewChat.max_chars(), Some(14_336));
    assert_eq!(Route::Herdr.max_chars(), None);
    assert_eq!(
        (chat.place().as_str(), chat.place_to_check()),
        ("app", "Claude")
    );
    assert!(chat.delivery_hint().is_some());
    // It survives saving, and nothing else ever matches it.
    let saved = serde_json::to_value(&chat).unwrap();
    assert_eq!(saved["app"], "claude_new_chat");
    let loaded: Target = serde_json::from_value(saved).unwrap();
    assert!(loaded.same_instance(&chat) && loaded.route() == Route::ClaudeNewChat);
    assert!(!chat.same_instance(&outside()) && !outside().same_instance(&chat));
    // The link belongs to Claude Desktop only.
    let elsewhere = Target {
        kind: AgentKind::ClaudeCode,
        ..chat.clone()
    };
    assert_eq!(elsewhere.route(), Route::DraftOnly);
    // A saved target without the field is no app surface.
    let plain = serde_json::to_value(outside()).unwrap();
    assert!(plain.get("app").is_none());
}
