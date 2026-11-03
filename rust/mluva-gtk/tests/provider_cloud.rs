//! Cloud provider settings: keys, region, price-labelled mode and the benchmark section.
use adw::prelude::*;
use mluva_core::config::AppConfig;
use mluva_gtk::{async_runtime::DesktopRuntime, provider_settings::ProviderSection};
use serde_json::{Map, Value};
use std::rc::Rc;
#[path = "support/dictation_ui.rs"]
mod ui;

/// Whether the edited `values` would be accepted over the `saved` settings, as the page saves them.
fn accepted(saved: &AppConfig, values: &Map<String, Value>) -> bool {
    let mut document = serde_json::to_value(saved).unwrap();
    for (name, value) in values {
        document[name] = value.clone();
    }
    serde_json::from_value::<AppConfig>(document).is_ok_and(|config| config.validate().is_ok())
}

/// Every label text under `widget`, in tree order.
fn texts(widget: &impl IsA<gtk::Widget>) -> Vec<String> {
    ui::widgets(widget)
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Label>().ok())
        .map(|label| label.label().to_string())
        .collect()
}

#[test]
#[ignore = "requires private display/network and no host microphone"]
fn cloud_providers_show_their_keys_region_prices_and_the_benchmark() {
    let root = ui::isolated();
    let runtime = DesktopRuntime::new().unwrap();
    let config = AppConfig::default();
    let section = ProviderSection::new(&config, root.join("model-data"), runtime.clone());
    let window = adw::Window::builder()
        .title("Mluva speech")
        .default_width(760)
        .default_height(1400)
        .build();
    let page = adw::PreferencesPage::new();
    page.add(&section.widget);
    window.set_content(Some(&page));
    window.present();
    ui::settle(0.2);

    // Microsoft MAI is first and carries the Suggested label; nothing else does.
    assert_eq!(section.provider_row.buttons.len(), 6);
    let suggested: Vec<_> = section
        .provider_row
        .buttons
        .iter()
        .map(|button| {
            button
                .child()
                .is_some_and(|child| texts(&child).contains(&"Suggested".into()))
        })
        .collect();
    assert_eq!(suggested, [true, false, false, false, false, false]);

    for (id, key_title, region, realtime, batch) in [
        (
            "microsoft",
            "Azure Speech key",
            true,
            "Realtime · $0.54 / audio hour",
            "Batch · $0.10 / audio hour",
        ),
        (
            "elevenlabs",
            "ElevenLabs API key",
            false,
            "Realtime · $0.39 / audio hour",
            "Batch · $0.22 / audio hour",
        ),
        (
            "xai",
            "xAI API key",
            false,
            "Realtime · $0.20 / audio hour",
            "Batch · $0.10 / audio hour",
        ),
        (
            "mistral",
            "Mistral API key",
            false,
            "Realtime · $0.48 / audio hour",
            "Batch · $0.24 / audio hour",
        ),
    ] {
        section.select_provider(id);
        ui::settle(0.05);
        assert!(section.api_key_entry.get_visible(), "{id}");
        assert_eq!(section.api_key_entry.title(), key_title);
        assert_eq!(section.region_entry.get_visible(), region, "{id}");
        assert!(section.mode_section.get_visible(), "{id}");
        assert!(section.mode_title.get_visible(), "{id}");
        assert_eq!(
            section.mode_row.buttons[0].label().as_deref(),
            Some(realtime)
        );
        assert_eq!(section.mode_row.buttons[1].label().as_deref(), Some(batch));
        assert_eq!(section.values()["transcription_provider"], id);
        // Realtime is the default; Batch is one explicit click and says what it changes.
        assert_eq!(section.values()["transcription_mode"], "realtime");
        section.mode_row.set_selected(1);
        assert_eq!(section.values()["transcription_mode"], "batch", "{id}");
        assert!(section.mode_hint.label().contains("after Stop"), "{id}");
        assert_eq!(
            section
                .mode_hint
                .label()
                .contains("introductory price until 31 December 2026"),
            id == "microsoft"
        );
        section.mode_row.set_selected(0);
        assert_eq!(section.values()["transcription_mode"], "realtime");
        // No model picker for the fixed cloud models.
        assert!(!section.model_row.get_visible());
        assert!(!section.advanced.get_visible());
    }

    // The Azure region is typed as Azure shows it and stored as its name.
    section.select_provider("microsoft");
    section.region_entry.set_text(" Sweden Central ");
    assert_eq!(section.values()["microsoft_region"], "swedencentral");

    // A region that is not a valid name blocks saving Microsoft, but never another provider: the
    // saved region stands in for it there, so switching away is not a dead end.
    let saved = AppConfig::default();
    section.select_provider("microsoft");
    section.region_entry.set_text("not.a/region");
    assert!(!accepted(&saved, &section.values()));
    for id in ["xai", "mistral", "elevenlabs", "local", "litellm"] {
        section.select_provider(id);
        let values = section.values();
        assert_eq!(values["microsoft_region"], "", "{id}");
        assert!(accepted(&saved, &values), "{id}");
    }
    section.select_provider("microsoft");
    assert!(!accepted(&saved, &section.values()));
    section.region_entry.set_text("");

    // Local and custom routes have neither keys, regions nor a mode.
    for id in ["local", "litellm"] {
        section.select_provider(id);
        ui::settle(0.05);
        assert!(!section.api_key_entry.get_visible(), "{id}");
        assert!(!section.region_entry.get_visible(), "{id}");
        assert!(!section.mode_title.get_visible(), "{id}");
        assert!(!section.mode_section.get_visible(), "{id}");
        assert!(!section.mode_row.widget.is_visible(), "{id}");
    }

    // The explanation is one expander with the benchmark and the one remaining exclusion.
    section.select_provider("microsoft");
    section.why.set_expanded(true);
    ui::settle(0.2);
    let explanation = texts(&section.why).join("\n");
    for needle in [
        "5.3% against 10.8%",
        "Mistral Voxtral Small is the most accurate batch route",
        "Alibaba Fun-ASR",
        "MAI realtime",
        "Czech technical",
        "preview without an SLA",
    ] {
        assert!(explanation.contains(needle), "{needle}\n{explanation}");
    }
    // Mistral is offered now, so the page no longer says it is not.
    assert!(
        !explanation.contains("API keys stay inactive"),
        "{explanation}"
    );
    ui::paint(&window, "provider-cloud-choices");
    // Both choices wrap, so each fits the narrowest settings page (420 px, with its margins).
    for widget in [&section.provider_row.widget, &section.mode_row.widget] {
        let (minimum, _, _, _) = widget.measure(gtk::Orientation::Horizontal, -1);
        assert!(minimum <= 380, "{minimum}");
    }

    section.stop_lookup();
    window.close();
    let shutdown = runtime.shutdown(async {});
    let done = Rc::new(std::cell::Cell::new(false));
    let flag = done.clone();
    glib::MainContext::default().spawn_local(async move {
        shutdown.await.unwrap();
        flag.set(true);
    });
    ui::until(|| done.get());
}
