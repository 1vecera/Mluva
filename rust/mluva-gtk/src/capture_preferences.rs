//! Persisted Capture/audio/privacy controls and their session-only privacy fallback.

use adw::prelude::*;
use mluva_audio::catalog::{PipeWireDeviceCatalog, PipeWireDeviceKind};
use mluva_core::{
    config::AudioRetentionPolicy,
    feature_maturity::{self, CAPABILITIES, Maturity},
};
use mluva_workflows::services::{ApplicationServices, InlineSavePolicy};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
    rc::Rc,
};

use crate::settings_view::SettingsView;
type Message = Rc<dyn Fn(&str)>;

pub const LANGUAGE_OPTIONS: &[(&str, &str)] = &[
    ("auto", "Auto-detect"),
    ("eng", "English"),
    ("ces", "Czech"),
    ("spa", "Spanish"),
    ("fra", "French"),
    ("deu", "German"),
    ("ita", "Italian"),
    ("por", "Portuguese"),
    ("nld", "Dutch"),
    ("jpn", "Japanese"),
    ("zho", "Mandarin Chinese"),
    ("kor", "Korean"),
    ("pol", "Polish"),
    ("rus", "Russian"),
    ("slk", "Slovak"),
    ("ukr", "Ukrainian"),
];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineEffect {
    Language,
    General,
    Privacy { persisted: bool },
    Routing,
    Shortcut,
}

/// Application effects always see the already-published settings. Privacy is
/// delivered on a failed save too, so the parent invalidates in-flight owners.
pub type AudioRoutesChanged = Rc<dyn Fn(&str, &str)>;

#[derive(Clone)]
pub struct CapturePreferenceCallbacks {
    pub changed: Rc<dyn Fn(InlineEffect)>,
    pub routes_changed: AudioRoutesChanged,
    pub history_changed: Rc<dyn Fn()>,
    pub status: Message,
    pub toast: Message,
    pub export_diagnostics: Rc<dyn Fn()>,
}

#[derive(Clone, Default)]
pub struct PreferenceActivity {
    pub preparing: bool,
    pub processing: bool,
    pub recording: bool,
    pub retrying: bool,
    pub meeting_processing: bool,
    pub meeting_retrying: bool,
    pub meeting_recording: bool,
    pub excluded_history: BTreeSet<String>,
}
impl PreferenceActivity {
    fn capture_active(&self) -> bool {
        self.preparing
            || self.processing
            || self.recording
            || self.meeting_processing
            || self.meeting_retrying
            || self.meeting_recording
    }
    fn refresh_blocked(&self) -> bool {
        self.capture_active() || self.retrying
    }
}

pub struct CapturePreferences {
    pub capture: adw::PreferencesPage,
    pub audio: adw::PreferencesPage,
    pub privacy: adw::PreferencesPage,
    pub maturity: adw::PreferencesPage,
    pub advanced: adw::PreferencesPage,
    pub language: adw::ComboRow,
    pub recording_key: adw::ComboRow,
    pub shortcuts: adw::PreferencesGroup,
    pub shortcut_status: adw::ActionRow,
    pub latest_shortcut_status: adw::ActionRow,
    pub spoken_commands: adw::SwitchRow,
    pub microphone: adw::ComboRow,
    pub system_audio: adw::ComboRow,
    pub refresh_audio: gtk::Button,
    pub incognito: adw::SwitchRow,
    pub audio_retention: adw::ComboRow,
    pub history_retention: adw::ComboRow,
    pub export_diagnostics: gtk::Button,
    services: Rc<ApplicationServices>,
    callbacks: CapturePreferenceCallbacks,
    shortcuts_available: Cell<bool>,
    activity: RefCell<PreferenceActivity>,
    catalog: RefCell<PipeWireDeviceCatalog>,
    catalog_error: RefCell<Option<String>>,
    language_codes: Vec<String>,
    microphone_targets: RefCell<Vec<Option<String>>>,
    system_targets: RefCell<Vec<Option<String>>>,
    refreshing_audio: Cell<bool>,
    syncing: Cell<bool>,
}

impl CapturePreferences {
    pub fn new(
        services: Rc<ApplicationServices>,
        catalog: PipeWireDeviceCatalog,
        catalog_error: Option<String>,
        callbacks: CapturePreferenceCallbacks,
    ) -> Rc<Self> {
        let config = services.config();
        let capture = page("capture", "Capture", "audio-input-microphone-symbolic");
        let defaults = group(
            &capture,
            "Capture defaults",
            Some("These choices apply to the next capture and remain unchanged while recording."),
        );
        let mut language_codes: Vec<_> = LANGUAGE_OPTIONS
            .iter()
            .map(|(code, _)| (*code).to_owned())
            .collect();
        let mut language_names: Vec<_> = LANGUAGE_OPTIONS
            .iter()
            .map(|(_, name)| (*name).to_owned())
            .collect();
        if !language_codes.contains(&config.language_code) {
            language_codes.push(config.language_code.clone());
            language_names.push(format!("Custom ISO code ({})", config.language_code));
        }
        let language = combo(
            &defaults,
            "Transcription language",
            &language_names
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        );
        language.set_selected(
            language_codes
                .iter()
                .position(|code| *code == config.language_code)
                .unwrap() as u32,
        );
        language.set_subtitle("Auto-detect lets the selected speech provider choose the language");
        let shortcuts = group(
            &capture,
            "Global shortcuts",
            Some("Press once to start and again to stop. F9 is the practical default."),
        );
        let keys: Vec<_> = (1..=24).map(|number| format!("F{number}")).collect();
        let recording_key = combo(
            &shortcuts,
            "Recording key",
            &keys.iter().map(String::as_str).collect::<Vec<_>>(),
        );
        recording_key.set_selected(
            keys.iter()
                .position(|key| *key == config.global_recording_key)
                .unwrap() as u32,
        );
        recording_key.set_subtitle("F13–F24 typically have the fewest conflicts but usually need a programmable keyboard layer; F9 is the practical default on a standard keyboard");
        recording_key.set_tooltip_text(Some("Press once to start and again to stop. F1–F12 may already be used by applications, desktop features, or keyboard firmware. F13–F24 are usually least used but are absent from most standard keyboards."));
        let shortcut_status = adw::ActionRow::builder()
            .title("Desktop approval")
            .subtitle(format!(
                "Requesting {} through the desktop portal…",
                config.global_recording_key
            ))
            .build();
        shortcuts.add(&shortcut_status);
        let latest_shortcut_status = adw::ActionRow::builder()
            .title("Open latest conversation")
            .subtitle(
                "Shift+F9 needs desktop approval. The shell menu also opens your latest dictation.",
            )
            .build();
        shortcuts.add(&latest_shortcut_status);
        let behavior = group(&capture, "Behavior", None);
        let spoken_commands = switch(
            &behavior,
            "spoken_structure",
            "Spoken structure",
            "Apply explicit punctuation, new line, new paragraph, and scratch-that commands",
            config.spoken_commands_enabled,
        );
        let audio = page("audio", "Audio", "audio-card-symbolic");
        let routing = group(&audio, "Audio routing", None);
        let microphone = adw::ComboRow::builder().title("Microphone").build();
        routing.add(&microphone);
        let system_audio = adw::ComboRow::builder()
            .title(feature_maturity::title(
                "meeting_mode",
                "Meeting system output",
            ))
            .subtitle("Meeting captures all audio playing through the selected sink")
            .build();
        routing.add(&system_audio);
        let refresh_audio = action_button(
            &routing,
            "PipeWire device snapshot",
            "Refresh reads node metadata only; it never opens or records a device",
            "Refresh",
        );
        let privacy = page("privacy", "Privacy", "changes-prevent-symbolic");
        let retention = group(&privacy, "Privacy and retention", None);
        let incognito = switch(
            &retention,
            "recovery_privacy",
            "Incognito",
            "New sessions write no local history or recovery audio. Recognition uses your selected speech provider.",
            config.incognito_mode,
        );
        let audio_retention = combo(
            &retention,
            &feature_maturity::title("recovery_privacy", "Keep audio"),
            &["Never", "Failures", "Always"],
        );
        audio_retention.set_selected(match config.audio_retention_policy {
            AudioRetentionPolicy::Never => 0,
            AudioRetentionPolicy::Failures => 1,
            AudioRetentionPolicy::Always => 2,
        });
        let history_retention = combo(
            &retention,
            &feature_maturity::title("recovery_privacy", "Keep history"),
            &["Forever", "7 days", "30 days", "90 days"],
        );
        history_retention.set_selected(
            [0, 7, 30, 90]
                .iter()
                .position(|days| config.history_retention_days.as_i64() == Some(*days))
                .unwrap_or(0) as u32,
        );
        let maturity = maturity_page();
        let advanced = page("advanced", "Advanced", "applications-engineering-symbolic");
        let diagnostics = group(&advanced, "Diagnostics", None);
        let export_diagnostics = action_button(
            &diagnostics,
            &feature_maturity::title("diagnostics", "Privacy-safe timing export"),
            "Configuration and stage timings only; excludes audio, text, targets, credentials, and errors",
            "Export",
        );
        let owner = Rc::new(Self {
            capture,
            audio,
            privacy,
            maturity,
            advanced,
            language,
            recording_key,
            shortcuts,
            shortcut_status,
            latest_shortcut_status,
            spoken_commands,
            microphone,
            system_audio,
            refresh_audio,
            incognito,
            audio_retention,
            history_retention,
            export_diagnostics,
            services,
            callbacks,
            shortcuts_available: Cell::new(false),
            activity: RefCell::new(PreferenceActivity::default()),
            catalog: RefCell::new(catalog),
            catalog_error: RefCell::new(catalog_error),
            language_codes,
            microphone_targets: RefCell::new(Vec::new()),
            system_targets: RefCell::new(Vec::new()),
            refreshing_audio: Cell::new(false),
            syncing: Cell::new(false),
        });
        owner.populate_audio();
        owner.connect();
        owner
    }
    pub fn add_pages(&self, settings: &Rc<SettingsView>) {
        for page in [
            &self.capture,
            &self.audio,
            &self.privacy,
            &self.maturity,
            &self.advanced,
        ] {
            settings.add(page);
        }
    }
    pub fn set_shortcuts_available(&self, available: bool) {
        self.shortcuts_available.set(available);
    }

    pub fn set_activity(&self, activity: PreferenceActivity) {
        self.activity.replace(activity);
    }
    /// Capture lifecycle owns when controls become available; activity alone is
    /// also updated during preparation/Stop without implying an idle UI reset.
    pub fn set_controls_available(&self, available: bool) {
        for row in [
            &self.language,
            &self.recording_key,
            &self.microphone,
            &self.system_audio,
            &self.audio_retention,
            &self.history_retention,
        ] {
            row.set_sensitive(available);
        }
        for row in [&self.spoken_commands, &self.incognito] {
            row.set_sensitive(available);
        }
        self.refresh_audio.set_sensitive(available);
    }
    fn connect(self: &Rc<Self>) {
        for (row, action) in [
            (&self.language, Self::language_changed as fn(&Self)),
            (&self.recording_key, Self::key_changed),
            (&self.audio_retention, Self::privacy_changed),
            (&self.history_retention, Self::privacy_changed),
        ] {
            let weak = Rc::downgrade(self);
            row.connect_selected_notify(move |_| {
                if let Some(owner) = weak.upgrade()
                    && !owner.syncing.get()
                {
                    action(&owner);
                }
            });
        }
        for row in [&self.microphone, &self.system_audio] {
            let weak = Rc::downgrade(self);
            row.connect_selected_notify(move |_| {
                if let Some(owner) = weak.upgrade()
                    && !owner.syncing.get()
                {
                    owner.audio_changed();
                }
            });
        }
        {
            let row = &self.spoken_commands;
            let weak = Rc::downgrade(self);
            row.connect_active_notify(move |_| {
                if let Some(owner) = weak.upgrade()
                    && !owner.syncing.get()
                {
                    owner.general_changed();
                }
            });
        }
        let weak = Rc::downgrade(self);
        self.incognito.connect_active_notify(move |_| {
            if let Some(owner) = weak.upgrade()
                && !owner.syncing.get()
            {
                owner.privacy_changed();
            }
        });
        for (button, action) in [
            (
                &self.refresh_audio,
                Self::refresh_audio_devices as fn(&Self),
            ),
            (&self.export_diagnostics, Self::export_action),
        ] {
            let weak = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                if let Some(owner) = weak.upgrade() {
                    action(&owner);
                }
            });
        }
    }
    fn status(&self, message: &str) {
        (self.callbacks.status)(message);
    }
    fn changed(&self, effect: InlineEffect) {
        (self.callbacks.changed)(effect);
    }

    fn export_action(&self) {
        (self.callbacks.export_diagnostics)();
    }

    fn language_changed(&self) {
        let Some(code) = self.language_codes.get(self.language.selected() as usize) else {
            return;
        };
        let mut proposed = self.services.config();
        if *code == proposed.language_code {
            return;
        }
        proposed.language_code.clone_from(code);
        if let Err(error) = self
            .services
            .save_inline_config(proposed, InlineSavePolicy::Persistent)
        {
            self.syncing.set(true);
            self.language.set_selected(
                self.language_codes
                    .iter()
                    .position(|code| *code == self.services.config().language_code)
                    .unwrap() as u32,
            );
            self.syncing.set(false);
            self.status(&format!(
                "Transcription language could not be saved: {error}"
            ));
            return;
        }
        self.changed(InlineEffect::Language);
        self.status(&format!(
            "Transcription language saved: {}.",
            LANGUAGE_OPTIONS
                .iter()
                .find(|(candidate, _)| *candidate == code)
                .map_or(code.as_str(), |(_, name)| *name)
        ));
    }
    fn key_changed(&self) {
        if self.recording_key.selected() >= 24 {
            return;
        }
        let key = format!("F{}", self.recording_key.selected() + 1);
        let mut proposed = self.services.config();
        if key == proposed.global_recording_key {
            return;
        }
        let agent = mluva_core::agent_voice::VoiceSettings::load(
            &self.services.paths.config.join("agent-voice.json"),
        )
        .ok();
        let result = if self.activity.borrow().capture_active() {
            Err("Stop the active capture before changing the global recording key.".into())
        } else if agent
            .as_ref()
            .is_some_and(|a| a.enabled && (a.hold_key == key || a.draft_key == key))
        {
            Err("Choose a different key: this key is already assigned to agent dictation or Keep draft.".into())
        } else {
            proposed.global_recording_key.clone_from(&key);
            self.services
                .save_inline_config(proposed, InlineSavePolicy::Persistent)
                .map_err(|error| format!("Global recording key could not be saved: {error}"))
        };
        if let Err(error) = result {
            self.syncing.set(true);
            self.recording_key.set_selected(
                self.services.config().global_recording_key[1..]
                    .parse::<u32>()
                    .unwrap()
                    - 1,
            );
            self.syncing.set(false);
            self.shortcut_status.set_subtitle(&error);
            self.status(&error);
            return;
        }
        self.shortcut_status.set_subtitle(&format!(
            "Requesting {key} through the desktop portal; approve the replacement if prompted…"
        ));
        self.changed(InlineEffect::Shortcut);
        self.status(&format!(
            "{key} saved; {}.",
            if self.shortcuts_available.get() {
                "the desktop portal is replacing the global binding"
            } else {
                "global shortcuts are disabled for this process"
            }
        ));
    }
    fn general_changed(&self) {
        let mut proposed = self.services.config();
        proposed.spoken_commands_enabled = self.spoken_commands.is_active();
        if let Err(error) = self
            .services
            .save_inline_config(proposed, InlineSavePolicy::Persistent)
        {
            self.status(&format!("General settings could not be saved: {error}"));
            return;
        }
        self.changed(InlineEffect::General);
        self.status("General settings saved for new captures.");
    }

    fn privacy_changed(&self) {
        let mut proposed = self.services.config();
        proposed.incognito_mode = self.incognito.is_active();
        proposed.audio_retention_policy = match self.audio_retention.selected() {
            0 => AudioRetentionPolicy::Never,
            1 => AudioRetentionPolicy::Failures,
            _ => AudioRetentionPolicy::Always,
        };
        let Some(days) = [0, 7, 30, 90]
            .get(self.history_retention.selected() as usize)
            .copied()
        else {
            return;
        };
        proposed.history_retention_days = days.into();
        if let Err(error) = self
            .services
            .save_inline_config(proposed, InlineSavePolicy::SessionIncognito)
        {
            self.status(&format!("Privacy settings could not be saved: {error}"));
            self.changed(InlineEffect::Privacy { persisted: false });
            self.status("Privacy settings could not be saved. The Incognito choice applies for this session.");
            return;
        }
        self.changed(InlineEffect::Privacy { persisted: true });
        let excluded = self.activity.borrow().excluded_history.clone();
        if let Err(error) = self.services.prune_history(&excluded) {
            self.status(&format!(
                "Privacy settings saved, but history retention failed: {error}"
            ));
            return;
        }
        (self.callbacks.history_changed)();
        self.status(if self.services.config().incognito_mode {
            "Incognito enabled for new sessions: no local history or recovery audio; Recognition uses your selected speech provider."
        } else { "Privacy and retention settings saved." });
    }

    fn populate_audio(&self) {
        self.refreshing_audio.set(true);
        let config = self.services.config();
        let catalog = self.catalog.borrow();
        for (row, target, kind, stored) in [
            (
                &self.microphone,
                &config.microphone_target,
                PipeWireDeviceKind::Microphone,
                &self.microphone_targets,
            ),
            (
                &self.system_audio,
                &config.system_audio_target,
                PipeWireDeviceKind::SystemOutput,
                &self.system_targets,
            ),
        ] {
            let mut targets: Vec<_> = std::iter::once(None)
                .chain(
                    catalog
                        .devices(kind)
                        .iter()
                        .map(|device| Some(device.target.clone())),
                )
                .collect();
            let mut labels: Vec<_> = std::iter::once("Default (automatic)".to_owned())
                .chain(
                    catalog
                        .devices(kind)
                        .iter()
                        .map(|device| device.name.clone()),
                )
                .collect();
            if !targets.contains(target) {
                targets.push(target.clone());
                labels.push(catalog.display_name(kind, target.as_deref()));
            }
            let index = targets
                .iter()
                .position(|candidate| candidate == target)
                .unwrap();
            stored.replace(targets);
            row.set_model(Some(&gtk::StringList::new(
                &labels.iter().map(String::as_str).collect::<Vec<_>>(),
            )));
            row.set_selected(index as u32);
        }
        self.microphone.set_subtitle(
            self.catalog_error
                .borrow()
                .as_deref()
                .unwrap_or("Used by Dictation and explicit Meeting capture"),
        );
        drop(catalog);
        self.refreshing_audio.set(false);
        self.publish_routes();
    }
    fn publish_routes(&self) {
        let config = self.services.config();
        let catalog = self.catalog.borrow();
        let microphone = catalog.display_name(
            PipeWireDeviceKind::Microphone,
            config.microphone_target.as_deref(),
        );
        let system = catalog.display_name(
            PipeWireDeviceKind::SystemOutput,
            config.system_audio_target.as_deref(),
        );
        drop(catalog);
        (self.callbacks.routes_changed)(&microphone, &system);
    }
    fn audio_changed(&self) {
        if self.refreshing_audio.get() {
            return;
        }
        let microphone = self
            .microphone_targets
            .borrow()
            .get(self.microphone.selected() as usize)
            .cloned();
        let system = self
            .system_targets
            .borrow()
            .get(self.system_audio.selected() as usize)
            .cloned();
        let (Some(microphone), Some(system)) = (microphone, system) else {
            return;
        };
        let mut proposed = self.services.config();
        // Model replacement can defer a selected notification until the outer
        // callback returns. A rejected save has already restored these routes;
        // don't recursively attempt that same failed write.
        if proposed.microphone_target == microphone && proposed.system_audio_target == system {
            return;
        }
        proposed.microphone_target = microphone;
        proposed.system_audio_target = system;
        if let Err(error) = self
            .services
            .save_inline_config(proposed, InlineSavePolicy::Persistent)
        {
            self.status(&format!("Audio routing could not be saved: {error}"));
            // Keep the current models alive until their selection notification
            // has returned. Replacing them here invalidates Libadwaita's active
            // selection object and can crash inside g_object_notify_by_pspec.
            let saved = self.services.config();
            let microphone = self
                .microphone_targets
                .borrow()
                .iter()
                .position(|target| *target == saved.microphone_target)
                .unwrap() as u32;
            let system = self
                .system_targets
                .borrow()
                .iter()
                .position(|target| *target == saved.system_audio_target)
                .unwrap() as u32;
            self.refreshing_audio.set(true);
            self.microphone.set_selected(microphone);
            self.system_audio.set_selected(system);
            self.refreshing_audio.set(false);
            self.publish_routes();
            return;
        }
        self.changed(InlineEffect::Routing);
        self.publish_routes();
        self.status("Audio routing saved for future captures.");
    }
    pub fn refresh_audio_devices(&self) {
        if self.activity.borrow().refresh_blocked() {
            self.status("Finish the active capture or retry before refreshing audio devices.");
            return;
        }
        match PipeWireDeviceCatalog::from_system(None) {
            Ok(catalog) => {
                let message = format!(
                    "Audio devices refreshed: {} microphone source(s), {} system output(s).",
                    catalog.microphones.len(),
                    catalog.system_outputs.len()
                );
                self.catalog.replace(catalog);
                self.catalog_error.take();
                self.populate_audio();
                self.status(&message);
            }
            Err(error) => {
                self.catalog_error.replace(Some(error.to_string()));
                self.status(&error.to_string());
                self.populate_audio();
            }
        }
    }
}

fn page(name: &str, title: &str, icon: &str) -> adw::PreferencesPage {
    adw::PreferencesPage::builder()
        .name(name)
        .title(title)
        .icon_name(icon)
        .build()
}
fn group(
    page: &adw::PreferencesPage,
    title: &str,
    description: Option<&str>,
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder().title(title).build();
    group.set_description(description);
    page.add(&group);
    group
}
fn combo(group: &adw::PreferencesGroup, title: &str, labels: &[&str]) -> adw::ComboRow {
    let row = adw::ComboRow::builder()
        .title(title)
        .model(&gtk::StringList::new(labels))
        .build();
    group.add(&row);
    row
}
fn switch(
    group: &adw::PreferencesGroup,
    feature: &str,
    title: &str,
    subtitle: &str,
    active: bool,
) -> adw::SwitchRow {
    let row = adw::SwitchRow::builder()
        .title(feature_maturity::title(feature, title))
        .subtitle(subtitle)
        .active(active)
        .build();
    group.add(&row);
    row
}
fn action_button(
    group: &adw::PreferencesGroup,
    title: &str,
    subtitle: &str,
    label: &str,
) -> gtk::Button {
    let row = adw::ActionRow::builder()
        .title(title)
        .subtitle(subtitle)
        .build();
    let button = gtk::Button::builder()
        .label(label)
        .valign(gtk::Align::Center)
        .build();
    row.add_suffix(&button);
    group.add(&row);
    button
}
fn maturity_page() -> adw::PreferencesPage {
    let page = page(
        "feature-maturity",
        "Feature maturity",
        "emblem-important-symbolic",
    );
    for (maturity, title, description) in [
        (
            Maturity::Verified,
            "Verified on Omarchy",
            "Part of the daily, end-to-end tested Omarchy workflow. Fedora has not been verified recently.",
        ),
        (
            Maturity::Experimental,
            "Experimental",
            "Available for testing, but not yet accepted as reliable. Automated evidence alone does not promote it.",
        ),
    ] {
        let group = group(&page, title, Some(description));
        for capability in CAPABILITIES
            .iter()
            .filter(|capability| capability.maturity == maturity)
        {
            let row = adw::ActionRow::builder()
                .title(capability.title)
                .subtitle(capability.summary)
                .build();
            let badge = gtk::Label::builder()
                .label(maturity.label())
                .valign(gtk::Align::Center)
                .build();
            badge.add_css_class("vs-maturity-badge");
            badge.add_css_class(if maturity == Maturity::Verified {
                "vs-verified"
            } else {
                "vs-experimental"
            });
            row.add_suffix(&badge);
            group.add(&row);
        }
    }
    page
}
