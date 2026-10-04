//! Fresh-process compact layout comparison through actual application I/O.
use super::{capture_window, clipboard, records, settle_for, until, widgets, window_id};
use adw::prelude::*;
use mluva_core::history::HistoryInput;
use mluva_gtk::application::ApplicationDesktop;
use mluva_workflows::{capture::CapturePhase, services::ApplicationServices};
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command, time::Duration};

pub fn exercise(
    owner: &ApplicationDesktop,
    services: &ApplicationServices,
    reference: &Value,
    tools: &Path,
    root: &Path,
    evidence: &Path,
) -> Vec<u64> {
    let name = reference["name"].as_str().unwrap();
    let params = &reference["params"];
    let width = params["width"].as_i64().unwrap() as i32;
    let height = params["height"].as_i64().unwrap() as i32;
    let window = &owner.shell.window;
    let page = &owner.capture.page;
    let w = &page.workspace;
    let gate = evidence.join("provider.release");
    let source = reference["source"].as_str().unwrap();
    let transcript = reference["transcript"].as_str().unwrap();
    let draft = reference["draft"].as_str().unwrap();
    let partial = reference["partial"].as_str().unwrap();
    let spec = json!({"scenario":"clean","evidence":evidence,
        "document_controls":{reference["polish"].as_str().unwrap():{"deltas":[partial],"completion_gate":gate}},
        "live_controls":{format!("preview|{transcript}"):{"deltas":[draft]},format!("final|{transcript}"):{"deltas":[draft],"gate":gate}}});
    fs::write(
        root.join("application-codex.json"),
        serde_json::to_vec(&spec).unwrap(),
    )
    .unwrap();
    window.set_default_size(width, height);
    window.set_size_request(width, height);
    gtk::gdk::Display::default()
        .unwrap()
        .clipboard()
        .set_text("untouched compact clipboard");
    for text in reference["seed"].as_array().unwrap() {
        let text = text.as_str().unwrap();
        services
            .history
            .add(HistoryInput {
                delivery_outcome: "copied".into(),
                ..HistoryInput::dictation(text, text)
            })
            .unwrap();
    }
    let entry = services
        .history
        .add(HistoryInput {
            delivery_outcome: "copied".into(),
            ..HistoryInput::dictation(source, source)
        })
        .unwrap();
    services
        .history
        .update_title(&entry.identifier, Some("A simpler dictation workflow"))
        .unwrap();
    services
        .conversations
        .append(
            &entry.identifier,
            reference["seed_instruction"].as_str().unwrap(),
            reference["reply"].as_str().unwrap(),
            "fixture-model",
        )
        .unwrap();
    let db = rusqlite::Connection::open(&services.history.database.path).unwrap();
    for (index, item) in services.history.recent(100).unwrap().iter().enumerate() {
        db.execute(
            "UPDATE transcription_history SET created_at=? WHERE identifier=?",
            rusqlite::params![
                reference["seed_dates"][index].as_str().unwrap(),
                item.identifier
            ],
        )
        .unwrap();
    }
    drop(db);
    let entry = services.history.find(&entry.identifier).unwrap();
    w.refresh_history().unwrap();
    w.show_conversation(
        Some(entry.clone()),
        &services.conversations.replies(&entry.identifier).unwrap(),
        false,
    )
    .unwrap();
    if name == "finalizing-empty" {
        w.show_conversation(None, &[], false).unwrap();
    }
    let turns = || {
        records(&evidence.join("requests.jsonl"))
            .iter()
            .filter(|r| r["message"]["method"] == "turn/start")
            .count()
    };
    let mut pids = Vec::new();
    let finalizing = matches!(name, "finalizing" | "finalizing-empty");
    let recording = matches!(name, "recording" | "processing" | "live-draft") || finalizing;
    if name == "empty" {
        w.show_conversation(None, &[], false).unwrap();
    } else if name == "rewriting" {
        w.quick_polish.emit_clicked();
        until(|| {
            turns() == 1
                && widgets(&w.messages)
                    .iter()
                    .filter_map(|q| q.downcast_ref::<gtk::TextView>())
                    .any(|view| {
                        let b = view.buffer();
                        b.text(&b.start_iter(), &b.end_iter(), true) == partial
                    })
        });
    } else if recording {
        let mut pcm = reference["pcm"].clone();
        pcm["pcm_hex"] = json!(
            pcm["pcm_hex"]
                .as_str()
                .unwrap()
                .repeat(if name == "processing" { 1 } else { 30 })
        );
        fs::write(
            tools.join("test-config.json"),
            serde_json::to_vec(&pcm).unwrap(),
        )
        .unwrap();
        page.record_button.emit_clicked();
        until(|| {
            owner.capture.phase() == Some(CapturePhase::Recording)
                && tools.join("raw.ready.json").exists()
        });
        let ready: Value =
            serde_json::from_slice(&fs::read(tools.join("raw.ready.json")).unwrap()).unwrap();
        pids.push(ready["pid"].as_u64().unwrap());
        if name != "processing" {
            until(|| w.live_text.text() == transcript);
            until(|| w.live_draft() == draft);
            if name == "recording" {
                w.draft_toggle.emit_clicked();
            }
        }
        if name == "processing" || finalizing {
            page.record_button.emit_clicked();
            until(|| {
                if name == "processing" {
                    evidence.join("speech.arrived").exists()
                } else {
                    turns() == 2 && w.notice.label() == "Finishing live draft…"
                }
            });
        }
    }
    until(|| {
        window
            .surface()
            .is_some_and(|surface| surface.width() == width && surface.height() == height)
    });
    assert!(
        Command::new("xdotool")
            .args(["windowmove", &window_id(), "20", "20"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("xdotool")
            .args(["mousemove", "0", "0"])
            .status()
            .unwrap()
            .success()
    );
    settle_for(Duration::from_millis(250));
    let controls: [(&str, &gtk::Widget); 16] = [
        ("continue", w.continue_button.upcast_ref()),
        ("polish", w.quick_polish.upcast_ref()),
        ("structure", w.structured_note.upcast_ref()),
        ("prompts", w.saved_prompts.upcast_ref()),
        (
            "rewrite-settings",
            page.rewrite_settings.widget.upcast_ref(),
        ),
        ("save", w.save.upcast_ref()),
        ("cancel", w.cancel.upcast_ref()),
        ("send", w.send.upcast_ref()),
        ("notice", w.notice.upcast_ref()),
        ("status", page.status.upcast_ref()),
        ("record", page.record_button.upcast_ref()),
        ("live-mode", page.live_mode.upcast_ref()),
        ("live-menu", page.live_menu.upcast_ref()),
        ("live-cancel", w.live_cancel.upcast_ref()),
        ("original", w.source_toggle.upcast_ref()),
        ("draft", w.draft_toggle.upcast_ref()),
    ];
    let controls = controls
        .into_iter()
        .map(|(name, widget)| {
            let mut value = json!({"mapped":widget.is_mapped(),"sensitive":widget.is_sensitive()});
            if widget.is_mapped() {
                let b = widget.compute_bounds(window).unwrap();
                let minimum = widget.measure(gtk::Orientation::Horizontal, -1).0;
                assert!(
                    b.x() >= 0.0
                        && b.y() >= 0.0
                        && b.x() + b.width() <= window.width() as f32
                        && b.y() + b.height() <= window.height() as f32,
                    "clipped {name}: {b:?}"
                );
                assert!(
                    b.width() >= minimum as f32,
                    "undersized {name}: {b:?}, minimum {minimum}"
                );
                value["bounds"] = json!([b.x(), b.y(), b.width(), b.height()]);
                value["minimum"] = json!(minimum);
                value["labels"] = json!(
                    widgets(widget)
                        .iter()
                        .filter_map(|q| q
                            .downcast_ref::<gtk::Label>()
                            .map(|q| q.text().to_string()))
                        .collect::<Vec<_>>()
                );
            }
            (name.into(), value)
        })
        .collect::<serde_json::Map<_, _>>();
    let mut aligned = serde_json::Map::new();
    for (name, widget) in [
        ("heading", &w.heading),
        ("live", &w.live_box),
        ("messages", &w.messages),
        ("composer", &w.composer),
        ("recording", &page.action_bar),
    ] {
        if widget.is_mapped() {
            aligned.insert(
                name.into(),
                json!(widget.compute_bounds(window).unwrap().x()),
            );
        }
    }
    let xs = aligned
        .values()
        .map(|value| value.as_f64().unwrap())
        .collect::<Vec<_>>();
    assert!(aligned.contains_key("heading") || aligned.contains_key("live"));
    assert!(
        xs.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            - xs.iter().copied().fold(f64::INFINITY, f64::min)
            <= 1.0
    );
    let live_header = if w.live_header.get_visible() {
        assert!(w.live_header.is_mapped() && !w.heading.is_mapped());
        let a = w.live_header.compute_bounds(window).unwrap();
        let b = w.live_box.compute_bounds(window).unwrap();
        assert!(a.y() + a.height() <= b.y());
        json!([a.y() + a.height(), b.y()])
    } else {
        Value::Null
    };
    let content = window.content().unwrap();
    let minimum = [
        content.measure(gtk::Orientation::Horizontal, -1).0,
        content.measure(gtk::Orientation::Vertical, width - 10).0,
    ];
    assert!(minimum[0] <= width - 10 && minimum[1] <= height - 10);
    if width <= 600 {
        assert!(
            w.split.is_collapsed()
                && !w.split.shows_sidebar()
                && w.split.content().unwrap().width() >= width - 30
        );
    }
    let surface = window.surface().unwrap();
    let mut layout = json!({"window":[window.width(),window.height()],"surface":[surface.width(),surface.height()],"scale":surface.scale_factor(),"minimum":minimum,"collapsed":w.split.is_collapsed(),"sidebar":w.split.shows_sidebar(),"controls":controls,"aligned":aligned,"live_header":live_header,"source":w.live_text.text(),"draft":w.live_draft(),"documents":w.documents().iter().map(|view|view.text()).collect::<Vec<_>>(),"raw":services.history.find(&entry.identifier).unwrap().raw_text,"reply":services.conversations.replies(&entry.identifier).unwrap()[0].text,"clipboard":clipboard()});
    if name == "wide" {
        let sidebar_heading = widgets(&w.split.sidebar().unwrap())
            .into_iter()
            .find(|q| q.has_css_class("ml-wordmark"))
            .unwrap()
            .parent()
            .unwrap();
        assert!(sidebar_heading.is_mapped() && w.heading.is_mapped());
        let a = sidebar_heading.compute_bounds(window).unwrap();
        let b = w.heading.compute_bounds(window).unwrap();
        assert!((a.y() - b.y()).abs() <= 1.0);
        layout["sidebar_header"] = json!([a.y(), b.y()]);
    }
    assert_eq!(json!(surface.scale_factor()), params["scale"]);
    let output = root.join(format!("native-compact-{name}"));
    fs::create_dir(&output).unwrap();
    fs::write(
        output.join("layout.json"),
        serde_json::to_vec_pretty(&layout).unwrap(),
    )
    .unwrap();
    capture_window(&output.join("workspace.png"));
    let png = fs::read(output.join("workspace.png")).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(
        u32::from_be_bytes(png[16..20].try_into().unwrap()),
        (width * surface.scale_factor()) as u32
    );
    assert_eq!(
        u32::from_be_bytes(png[20..24].try_into().unwrap()),
        (height * surface.scale_factor()) as u32,
        "virtual display clipped the scaled window"
    );
    assert_eq!(
        layout,
        reference["layout"],
        "compact {name}; see {}",
        output.display()
    );
    println!("COMPACT_LAYOUT {name} PASS");
    if name == "rewriting" {
        w.cancel.emit_clicked();
        until(|| !w.cancel.get_visible());
        fs::write(&gate, "").unwrap();
    } else if name == "processing" {
        fs::write(&gate, "").unwrap();
        until(|| owner.capture.phase().is_none());
    } else if finalizing {
        fs::write(&gate, "").unwrap();
        until(|| !owner.live.active());
    } else if recording {
        window
            .application()
            .unwrap()
            .activate_action("cancel", None);
        until(|| owner.capture.phase().is_none());
    }
    settle_for(Duration::from_millis(200));
    window.close();
    assert!(!window.get_visible());
    window
        .application()
        .unwrap()
        .activate_action("latest", None);
    until(|| window.get_visible());
    settle_for(Duration::from_millis(100));
    let latest = w.entry().unwrap();
    let reopened = json!({"visible":window.get_visible(),"raw":latest.raw_text,"replies":services.conversations.replies(&latest.identifier).unwrap().iter().map(|reply|reply.text.clone()).collect::<Vec<_>>()});
    fs::write(
        output.join("reopened.json"),
        serde_json::to_vec_pretty(&reopened).unwrap(),
    )
    .unwrap();
    assert_eq!(
        reopened, reference["reopened"],
        "latest after compact {name}"
    );
    println!("COMPACT_REOPENED {name} PASS");
    pids
}
