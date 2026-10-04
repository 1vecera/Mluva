//! Rapid ASR growth, identical updates and opaque whole-draft corrections.
use super::{scroll_frames, settle};
use adw::prelude::*;
use mluva_core::config::AppConfig;
use mluva_gtk::conversation_view::ConversationWorkspace;
use serde_json::{Value, json};
use std::{fs, path::Path, rc::Rc};

pub fn exercise(w: &Rc<ConversationWorkspace>, window: &adw::Window, root: &Path) {
    let reference: Value =
        serde_json::from_str(include_str!("../fixtures/released-live-stability.json")).unwrap();
    assert_eq!(
        reference["reference"],
        "5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f"
    );
    assert_eq!(
        reference["gtk"],
        json!([
            gtk::major_version(),
            gtk::minor_version(),
            gtk::micro_version()
        ])
    );
    assert_eq!(reference["pango"], gtk::pango::version_string().as_str());
    w.finish_live();
    w.set_screenshot_context(None, None);
    w.show_conversation(None, &[], false).unwrap();
    window.set_default_size(1060, 780);
    w.set_config(AppConfig {
        rewrite_provider: "codex".into(),
        live_rewrite_enabled: true,
        scroll_duration_ms: 350,
        ..Default::default()
    })
    .unwrap();
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(true);
    w.set_live_draft_available(true);
    w.show_live_draft("", "Waiting for speech…");
    w.set_live("00:00", "", true);
    // Earlier page cases deliberately hide panes; begin this scenario with the
    // same two visible panes as the fresh released workspace.
    if !w.source_toggle.is_active() {
        w.source_toggle.emit_clicked();
    }
    if !w.draft_toggle.is_active() {
        w.draft_toggle.emit_clicked();
    }
    scroll_frames(w, 0.3);
    let mut states = Vec::new();
    let mut snapshot = |name: &str, extra: Value| {
        let panes = [&w.live_source_box, &w.live_draft_box]
            .iter()
            .map(|pane| {
                let b = pane.compute_bounds(window).unwrap();
                json!([b.x(), b.y(), b.width(), b.height()])
            })
            .collect::<Vec<_>>();
        let a = w.live_scroll.vadjustment();
        let end = w.live_text.iter_location(&w.live_text.buffer().end_iter());
        let visible = w.live_text.visible_rect();
        let mut value = json!({"name":name,"source":w.live_text.text(),"draft":w.live_draft(),"panes":panes,"draft_mapped":w.live_draft_box.is_mapped(),"top":w.live_text.top_margin(),"at_origin":a.value()==0.0,"end_visible":visible.y()<=end.y() && end.y()<visible.y()+visible.height()});
        value
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        fs::write(
            root.join(format!("stability-{name}.json")),
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();
        assert_eq!(
            value,
            reference["stages"][states.len()],
            "stability stage {name}"
        );
        states.push(value);
        println!("STABILITY_STAGE {name} PASS");
    };
    snapshot("empty-panes", json!({}));
    let mut source = String::from("The first words start at the top.");
    w.set_live("00:01", &source, true);
    scroll_frames(w, 0.3);
    snapshot("first-words", json!({}));
    let mut samples = Vec::new();
    for index in 0..48 {
        source.push_str(" Useful words fill each line naturally.");
        w.set_live(&format!("00:{:02}", index + 2), &source, true);
        samples.extend(scroll_frames(w, 0.035));
        assert_eq!(w.live_text.text(), source);
    }
    samples.extend(scroll_frames(w, 0.6));
    let a = w.live_scroll.vadjustment();
    let reverse = samples.windows(2).map(|p| p[0] - p[1]).fold(0.0, f64::max);
    snapshot(
        "rapid-growth",
        json!({"monotonic":reverse<=1.0,"overflow":a.value()>100.0,"intermediate":samples.iter().any(|v|0.0<*v && *v<a.value())}),
    );
    let before = [a.upper(), a.value(), f64::from(w.live_text.top_margin())];
    for _ in 0..100 {
        w.set_live("00:51", &source, true);
    }
    scroll_frames(w, 0.6);
    let after = [a.upper(), a.value(), f64::from(w.live_text.top_margin())];
    snapshot(
        "identical-updates",
        json!({"extent_unchanged":before.iter().zip(after).all(|(a,b)|(a-b).abs()<=1.0)}),
    );
    w.show_live_draft(
        "## Task\nKeep the source exact.\n\n## Next step\nReview on Tuesday.",
        "",
    );
    scroll_frames(w, 0.3);
    snapshot("first-draft", json!({}));
    for ending in ["Wednesday.", "Wednesday morning.", "Thursday."] {
        let draft = format!("## Task\nKeep the source exact.\n\n## Next step\nReview on {ending}");
        w.show_live_draft(&draft, "");
        let mut cursor = w.live_draft_text.buffer().start_iter();
        let mut opaque = true;
        while !cursor.is_end() {
            for tag in cursor.tags() {
                if tag.is_foreground_set() {
                    opaque &= tag.foreground_rgba().unwrap().alpha() == 1.0;
                }
            }
            cursor.forward_char();
        }
        assert_eq!(w.live_draft(), draft);
        snapshot(&format!("draft-{ending}"), json!({"opaque":opaque}));
        scroll_frames(w, 0.04);
    }
    w.set_live("00:52", "A much shorter corrected recognition.", true);
    scroll_frames(w, 0.6);
    snapshot("short-correction", json!({}));
    w.finish_live();
    w.show_live_draft("", "Waiting for speech…");
    w.set_live("00:00", "A fresh recording begins here.", true);
    scroll_frames(w, 0.6);
    snapshot("fresh-capture", json!({}));
    assert_eq!(states.len(), reference["stages"].as_array().unwrap().len());
    fs::write(
        root.join("stability-positions.json"),
        serde_json::to_vec_pretty(
            &json!({"samples":samples,"maximum_reverse":reverse,"before":before,"after":after}),
        )
        .unwrap(),
    )
    .unwrap();
    w.finish_live();
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
    settle();
}
