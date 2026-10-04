//! Provider choices through the assembled application and independent external peers.
use super::{
    capture_window, clipboard, http, records, settle, settle_for, until, widgets, window_id,
};
use adw::prelude::*;
use mluva_core::config::AppConfig;
use mluva_gtk::{application::ApplicationDesktop, provider_settings::ProviderSection};
use mluva_providers::{local_assets::MODEL_CATALOG, onnx_runtime::ONNX_RUNTIME};
use mluva_workflows::{capture::CapturePhase, services::ApplicationServices};
use serde_json::{Value, json};
use std::{cell::Cell, fs, os::unix::fs::symlink, path::Path, process::Command, time::Duration};

pub fn prepare_credentials(root: &Path, tools: &Path, target: &Path) {
    let keyring = root.join("provider-keyring");
    assert_eq!(
        Path::new(&std::env::var_os("CREDENTIAL_FIXTURE_ROOT").unwrap()),
        keyring
    );
    fs::create_dir(&keyring).unwrap();
    fs::write(
        keyring.join("keyring.json"),
        br#"{"lookup":{"stdout":"synthetic-provider-key\n"}}"#,
    )
    .unwrap();
    symlink(
        target.join("credential-fixture-peer"),
        tools.join("secret-tool"),
    )
    .unwrap();
}

pub fn prepare_model(data: &Path) {
    // Readiness only: sparse pinned files never claim download or inference coverage.
    let model = MODEL_CATALOG
        .iter()
        .find(|m| m.id == "whisper-tiny")
        .unwrap();
    let sparse = |path: &Path, size| {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::File::create(path).unwrap().set_len(size).unwrap();
    };
    for file in &model.files {
        sparse(&model.path(data).join(&file.name), file.size);
    }
    fs::write(model.path(data).join(".ready"), &model.revision).unwrap();
    let runtime = ONNX_RUNTIME.root(data, "cpu");
    for member in ONNX_RUNTIME.cpu.iter().flat_map(|archive| &archive.members) {
        sparse(&runtime.join(&member.path), member.size);
    }
    fs::write(
        runtime.join(".native-ready"),
        ONNX_RUNTIME.stamp("cpu").unwrap(),
    )
    .unwrap();
}

fn combo(row: &adw::ComboRow) -> Value {
    let labels = row
        .model()
        .and_downcast::<gtk::StringList>()
        .map(|model| {
            (0..model.n_items())
                .map(|i| model.string(i).unwrap().to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    json!({"selected":row.selected(),"labels":labels})
}
fn section(row: &ProviderSection) -> Value {
    json!({"provider":row.provider().id,"model":combo(&row.model_row),"manual":row.model_entry.text().as_str(),"manual_visible":row.model_entry.get_visible(),"fast":row.fast_row.is_active(),"thinking":combo(&row.thinking_row.widget),"status":row.status.label().as_str(),"refresh":row.refresh_button.get_sensitive(),"local":row.local.selected(),"ready":row.local.is_ready()})
}
fn normalize(value: &mut Value, base: &str) {
    match value {
        Value::String(text) => *text = text.replace(base, "$HTTP"),
        Value::Array(values) => values.iter_mut().for_each(|v| normalize(v, base)),
        Value::Object(values) => values.values_mut().for_each(|v| normalize(v, base)),
        _ => {}
    }
}
fn config(value: AppConfig) -> Value {
    let mut value = serde_json::to_value(value).unwrap();
    value.as_object_mut().unwrap().retain(|key, _| {
        matches!(
            key.as_str(),
            "transcription_provider"
                | "rewrite_provider"
                | "rewrite_model"
                | "rewrite_fast_mode"
                | "rewrite_reasoning_effort"
                | "local_model"
                | "local_device"
                | "transcription_remote_model"
                | "transcription_base_url"
                | "transcription_api_key_env"
                | "litellm_model"
                | "litellm_reasoning_effort"
                | "litellm_base_url"
                | "litellm_api_key_env"
        )
    });
    value
}
fn choose(row: &adw::ComboRow, label: &str) {
    let strings = row.model().and_downcast::<gtk::StringList>().unwrap();
    let index = (0..strings.n_items())
        .find(|i| strings.string(*i).as_deref() == Some(label))
        .unwrap_or_else(|| panic!("missing {label}: {}", combo(row)));
    row.set_selected(index);
}
fn manual(row: &ProviderSection, text: &str) {
    row.model_row
        .set_selected(row.model_row.model().unwrap().n_items() - 1);
    assert!(row.model_entry.get_visible());
    row.model_entry.set_text(text);
}
fn refresh(row: &ProviderSection) {
    row.refresh_button.emit_clicked();
    until(|| row.refresh_button.get_sensitive());
    settle_for(Duration::from_millis(100));
}
fn check(actual: &Value, expected: &Value, root: &Path, name: &str) {
    fs::write(
        root.join(format!("providers-{name}.json")),
        serde_json::to_vec_pretty(actual).unwrap(),
    )
    .unwrap();
    for (field, expected) in expected.as_object().unwrap() {
        assert_eq!(
            &actual[field], expected,
            "provider workspace {name}.{field}"
        );
    }
    assert_eq!(
        actual.as_object().unwrap().len(),
        expected.as_object().unwrap().len()
    );
}

pub fn exercise(
    owner: &ApplicationDesktop,
    services: &ApplicationServices,
    reference: &Value,
    tools: &Path,
    root: &Path,
    evidence: &Path,
    peer: &http::Peer,
) -> Vec<u64> {
    let name = reference["name"].as_str().unwrap();
    let window = &owner.shell.window;
    let width = reference["params"]["width"].as_i64().unwrap() as i32;
    let height = reference["params"]["height"].as_i64().unwrap() as i32;
    window.set_default_size(width, height);
    settle_for(Duration::from_millis(100));
    let surface = window.surface().unwrap();
    window.set_size_request(
        width + surface.width() - window.width(),
        height + surface.height() - window.height(),
    );
    until(|| window.width() == width && window.height() == height);
    for args in [
        vec!["windowmove".into(), window_id(), "20".into(), "20".into()],
        vec!["mousemove".into(), "0".into(), "0".into()],
    ] {
        assert!(
            Command::new("xdotool")
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    }
    gtk::gdk::Display::default()
        .unwrap()
        .clipboard()
        .set_text("untouched provider clipboard");
    let application = window.application().unwrap();
    let view = &owner.settings.view;
    let page = &owner.settings.providers;
    let speech = &page.speech;
    let rewrite = &page.rewrite;
    let open = || {
        application.activate_action("settings", None);
        view.set_visible_page_name("providers");
        until(|| page.widget.is_mapped());
    };
    let apply = || {
        page.apply_button.emit_clicked();
        until(|| owner.capture.page.record_button.get_sensitive());
        settle_for(Duration::from_millis(100));
    };
    open();
    settle_for(Duration::from_millis(200));
    let stages = reference["stages"].as_array().unwrap();
    let position = Cell::new(0);
    let snapshot = |name: &str| {
        settle_for(Duration::from_millis(100));
        let inline = &owner.capture.page.rewrite_settings;
        let mut state = json!({"name":name,"page":owner.shell.stack.visible_child_name().map(String::from),"settings":view.visible_page_name().map(String::from),
            "saved":config(AppConfig::load(&services.paths.config.join("config.json")).unwrap()),"active":config(services.config()),"speech":section(speech),"rewrite":section(rewrite),"notice":page.status.label().as_str(),
            "capture":{"ready":owner.capture.page.record_button.get_sensitive(),"status":owner.capture.page.status.label().as_str(),"recording":owner.capture.phase()==Some(CapturePhase::Recording)},
            "inline":{"model":combo(&inline.model_row),"refresh":inline.refresh.get_sensitive(),"status":inline.status.label().as_str(),"fast":inline.fast_row.is_active(),"thinking":combo(&inline.thinking_row.widget)},"history":services.history.recent(100).unwrap().len(),"clipboard":clipboard()});
        normalize(&mut state, &peer.address);
        check(&state, &stages[position.get()], root, name);
        position.set(position.get() + 1);
    };
    let gate = evidence.join("catalog.release");
    let incoming = evidence.join("catalog.arrived");
    snapshot("default");
    assert!(records(&evidence.join("requests.jsonl")).is_empty());
    assert!(peer.observed.lock().unwrap().is_empty());
    if name == "flow" {
        refresh(rewrite);
        choose(&rewrite.model_row, "GPT-5.4");
        rewrite.fast_row.set_active(true);
        choose(&rewrite.thinking_row.widget, "High");
        apply();
        snapshot("native-saved");
        let specfile = root.join("application-codex.json");
        let mut spec: Value = serde_json::from_slice(&fs::read(&specfile).unwrap()).unwrap();
        spec["model_gate"] = json!(gate);
        fs::write(&specfile, serde_json::to_vec(&spec).unwrap()).unwrap();
        view.close();
        until(|| !page.widget.is_mapped());
        owner.capture.page.rewrite_settings.refresh.emit_clicked();
        until(|| {
            records(&evidence.join("requests.jsonl"))
                .iter()
                .filter(|r| r["message"]["method"] == "model/list")
                .count()
                == 2
        });
        let pending_pid = records(&evidence.join("process.jsonl")).last().unwrap()["pid"]
            .as_u64()
            .unwrap();
        open();
        speech.provider_row.set_selected(1);
        speech.local.scale.set_value(0.0);
        apply();
        until(|| !Path::new(&format!("/proc/{pending_pid}")).exists());
        snapshot("local-saved-inline-cancelled");
        spec.as_object_mut().unwrap().remove("model_gate");
        fs::write(specfile, serde_json::to_vec(&spec).unwrap()).unwrap();
    }
    let mut pids = vec![];
    if matches!(name, "flow" | "details" | "error") {
        for (row, key, path, alias) in [
            (speech, "FIXTURE_SPEECH_KEY", "/speech", "manual-speech"),
            (rewrite, "FIXTURE_REWRITE_KEY", "/rewrite", "manual-writer"),
        ] {
            row.provider_row.set_selected(2);
            row.endpoint_entry
                .set_text(&format!("{}{path}", peer.address));
            row.key_entry.set_text(key);
            manual(row, alias);
            refresh(row);
        }
        snapshot("custom-discovered");
        if name == "details" {
            speech.advanced.set_expanded(true);
        }
        if name == "flow" {
            choose(&speech.model_row, "speech-alias");
            choose(&rewrite.model_row, "writer-alias");
            choose(&rewrite.thinking_row.widget, "Medium");
            apply();
            snapshot("custom-saved");
            refresh(rewrite);
            snapshot("offline-catalog");
            choose(&rewrite.model_row, "writer-alias");
            manual(rewrite, "offline-alias");
            apply();
            snapshot("offline-saved");
            let before = fs::read(services.paths.config.join("config.json")).unwrap();
            rewrite
                .endpoint_entry
                .set_text("https://user:fixture-secret@example.test/v1");
            page.apply_button.emit_clicked();
            assert_eq!(
                fs::read(services.paths.config.join("config.json")).unwrap(),
                before
            );
            assert!(!page.status.label().contains("fixture-secret"));
            snapshot("invalid-rejected");
            rewrite
                .endpoint_entry
                .set_text(&format!("{}/rewrite", peer.address));
            view.close();
            until(|| !page.widget.is_mapped());
            application.activate_action("record", None);
            until(|| {
                owner.capture.phase() == Some(CapturePhase::Recording)
                    && tools.join("raw.ready.json").exists()
            });
            let ready: Value =
                serde_json::from_slice(&fs::read(tools.join("raw.ready.json")).unwrap()).unwrap();
            pids.push(ready["pid"].as_u64().unwrap());
            open();
            manual(rewrite, "busy-alias");
            page.apply_button.emit_clicked();
            assert_eq!(
                fs::read(services.paths.config.join("config.json")).unwrap(),
                before
            );
            snapshot("recording-rejected");
            application.activate_action("cancel", None);
            until(|| owner.capture.phase().is_none());
            apply();
            snapshot("after-cancel-saved");
            rewrite.refresh_button.emit_clicked();
            until(|| incoming.exists());
            rewrite.provider_row.set_selected(0);
            fs::write(&gate, "release").unwrap();
            until(|| evidence.join("catalog.sent").exists());
            assert_eq!(peer.observed.lock().unwrap().len(), 4);
            settle_for(Duration::from_millis(200));
            snapshot("late-catalog-ignored");
            apply();
            snapshot("switched-native-saved");
            view.close();
            until(|| !page.widget.is_mapped());
            open();
            snapshot("reopened");
        }
    }
    assert_eq!(position.get(), stages.len());
    let scroll = widgets(&page.widget)
        .into_iter()
        .find_map(|w| w.downcast::<gtk::ScrolledWindow>().ok())
        .unwrap();
    let adjustment = scroll.vadjustment();
    for expected in reference["layouts"].as_array().unwrap() {
        let side = expected["name"].as_str().unwrap();
        adjustment.set_value(if side == "top" {
            0.0
        } else {
            adjustment.upper() - adjustment.page_size()
        });
        settle_for(Duration::from_millis(200));
        let mut controls = serde_json::Map::new();
        for (key, widget) in [
            (
                "speech",
                speech.provider_row.widget.upcast_ref::<gtk::Widget>(),
            ),
            ("speech-model", speech.model_row.upcast_ref()),
            ("speech-manual", speech.model_entry.upcast_ref()),
            ("speech-details", speech.advanced.upcast_ref()),
            ("rewrite", rewrite.provider_row.widget.upcast_ref()),
            ("rewrite-model", rewrite.model_row.upcast_ref()),
            ("rewrite-manual", rewrite.model_entry.upcast_ref()),
            ("fast", rewrite.fast_row.upcast_ref()),
            ("thinking", rewrite.thinking_row.widget.upcast_ref()),
            ("apply", page.apply_button.upcast_ref()),
            ("notice", page.status.upcast_ref()),
        ] {
            let mut value = json!({"mapped":widget.is_mapped()});
            if widget.is_mapped() {
                let r = widget.compute_bounds(window).unwrap();
                value["bounds"] = json!([r.x(), r.y(), r.width(), r.height()]);
            }
            controls.insert(key.into(), value);
        }
        let layout = json!({"name":side,"window":[window.width(),window.height()],"page":[page.widget.width(),page.widget.height()],"scroll":[adjustment.upper(),adjustment.page_size(),adjustment.value()],"controls":controls});
        capture_window(&root.join(format!("providers-{side}.png")));
        check(&layout, expected, root, &format!("layout-{side}"));
    }
    let bounds = page.apply_button.compute_bounds(&page.widget).unwrap();
    assert!(bounds.y() >= 0.0 && bounds.y() + bounds.height() <= page.widget.height() as f32);
    settle();
    pids
}
