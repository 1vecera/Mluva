//! Persisted Capture/audio/privacy controls and their session-only privacy fallback.

use adw::prelude::*;
use mluva_audio::catalog::{PipeWireDeviceCatalog, PipeWireDeviceKind};
use mluva_core::{
    config::{AudioRetentionPolicy, CAPTURE_MODES},
    feature_maturity::{self, CAPABILITIES, Maturity},
};
use mluva_workflows::services::{ApplicationServices, InlineSavePolicy};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
    rc::Rc,
};

use crate::{prompt_editor::Message, settings_view::SettingsView};

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
const MODE_LABELS: &[&str] = &["Dictate", "Command", "Notes"];
const MODE_DESCRIPTIONS: &[(&str, &str)] = &[
    (
        "dictation",
        "Press the configured global function key once to start and again to finish, or use the copy-only button.",
    ),
    (
        "command_mode",
        "Speak an instruction for the explicitly selected text or captured caret.",
    ),
    (
        "notes_mode",
        "Capture a longer editable draft that remains acceptance-gated.",
    ),
];
const CLEANUP_SUBTITLE: &str =
    "Remove obvious filler and repair punctuation through the selected rewrite provider";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineEffect {
    Mode,
    Language,
    General,
    Titles,
    Privacy { persisted: bool },
    Routing,
    Style,
    Shortcut,
}

/// Application effects always see the already-published settings. Privacy is
/// delivered on a failed save too, so the parent invalidates in-flight owners.
pub type AudioRoutesChanged = Rc<dyn Fn(&str, &str)>;

#[derive(Clone)]
pub struct CapturePreferenceCallbacks {
    pub changed: Rc<dyn Fn(InlineEffect)>,
    pub summary_changed: Rc<dyn Fn()>,
    pub routes_changed: AudioRoutesChanged,
    pub history_changed: Rc<dyn Fn()>,
    pub status: Message,
    pub toast: Message,
    pub manage_styles: Rc<dyn Fn()>,
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
    pub pending_review: bool,
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
    pub mode: adw::ComboRow,
    pub language: adw::ComboRow,
    pub output_style: adw::ComboRow,
    pub style_instructions: adw::ActionRow,
    pub recording_key: adw::ComboRow,
    pub shortcut_status: adw::ActionRow,
    pub latest_shortcut_status: adw::ActionRow,
    pub remember_application: adw::SwitchRow,
    pub cleanup: adw::SwitchRow,
    pub automatic_titles: adw::SwitchRow,
    pub spoken_commands: adw::SwitchRow,
    pub auto_paste: adw::SwitchRow,
    pub microphone: adw::ComboRow,
    pub system_audio: adw::ComboRow,
    pub refresh_audio: gtk::Button,
    pub incognito: adw::SwitchRow,
    pub audio_retention: adw::ComboRow,
    pub history_retention: adw::ComboRow,
    pub manage_styles: gtk::Button,
    pub export_diagnostics: gtk::Button,
    services: Rc<ApplicationServices>,
    callbacks: CapturePreferenceCallbacks,
    tracking_available: bool,
    shortcuts_available: Cell<bool>,
    activity: RefCell<PreferenceActivity>,
    profile: RefCell<Option<String>>,
    catalog: RefCell<PipeWireDeviceCatalog>,
    catalog_error: RefCell<Option<String>>,
    language_codes: Vec<String>,
    style_identifiers: RefCell<Vec<Option<String>>>,
    microphone_targets: RefCell<Vec<Option<String>>>,
    system_targets: RefCell<Vec<Option<String>>>,
    refreshing_styles: Cell<bool>,
    refreshing_audio: Cell<bool>,
    syncing: Cell<bool>,
    cleanup_before_incognito: Cell<Option<bool>>,
}

impl CapturePreferences {
    pub fn new(
        services: Rc<ApplicationServices>,
        catalog: PipeWireDeviceCatalog,
        catalog_error: Option<String>,
        tracking_available: bool,
        callbacks: CapturePreferenceCallbacks,
    ) -> Rc<Self> {
        let config = services.config();
        let capture = page("capture", "Capture", "audio-input-microphone-symbolic");
        let defaults = group(
            &capture,
            "Capture defaults",
            Some("These choices apply to the next capture and remain unchanged while recording."),
        );
        let mode = combo(&defaults, "Capture mode", MODE_LABELS);
        mode.set_selected(
            CAPTURE_MODES
                .iter()
                .position(|mode| *mode == config.default_mode)
                .unwrap() as u32,
        );
        mode.set_tooltip_text(Some("Dictate: press the configured global function key to start and again to finish.\nCommand (Experimental): review a spoken edit or drafting instruction before delivery.\nNotes (Experimental): keep a longer editable draft until you copy or delete it.\nMeeting (Experimental): use the separate Meeting tab for explicit microphone and system-audio capture."));
        set_mode_description(&mode);
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
        let output_style = adw::ComboRow::builder().title("Output style").build();
        defaults.add(&output_style);
        let style_instructions = adw::ActionRow::builder()
            .title("Selected style instructions")
            .subtitle_lines(3)
            .build();
        defaults.add(&style_instructions);
        let manage_styles = action_button(
            &defaults,
            "Custom output modes",
            "Create or edit reusable instructions in Personalization",
            "Manage",
        );
        let shortcuts = group(
            &capture,
            "Global shortcut",
            Some(
                "Press once to start and again to stop; F9 is the practical default on a standard keyboard",
            ),
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
        let remember_application = switch(
            &behavior,
            "application_memory",
            "Remember mode and style per application",
            "Uses a local executable identity that is never sent to Codex",
            config.remember_per_application,
        );
        let cleanup = switch(
            &behavior,
            "faithful_cleanup",
            "Faithful cleanup",
            CLEANUP_SUBTITLE,
            false,
        );
        let automatic_titles = switch(
            &behavior,
            "conversations",
            "Automatic conversation titles",
            "Send an excerpt of each new conversation to the rewrite provider for a title. A local label is used if unavailable.",
            config.automatic_titles,
        );
        let spoken_commands = switch(
            &behavior,
            "spoken_structure",
            "Spoken structure",
            "Apply explicit punctuation, new line, new paragraph, and scratch-that commands",
            config.spoken_commands_enabled,
        );
        let auto_paste = switch(
            &behavior,
            "automatic_paste",
            "Automatic paste",
            if tracking_available {
                feature_maturity::capability("automatic_paste")
                    .unwrap()
                    .summary
            } else {
                "Experimental · Target tracking is unavailable; completed text remains on the clipboard"
            },
            config.auto_paste,
        );
        auto_paste.set_sensitive(tracking_available);
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
            mode,
            language,
            output_style,
            style_instructions,
            recording_key,
            shortcut_status,
            latest_shortcut_status,
            remember_application,
            cleanup,
            automatic_titles,
            spoken_commands,
            auto_paste,
            microphone,
            system_audio,
            refresh_audio,
            incognito,
            audio_retention,
            history_retention,
            manage_styles,
            export_diagnostics,
            services,
            callbacks,
            tracking_available,
            shortcuts_available: Cell::new(false),
            activity: RefCell::new(PreferenceActivity::default()),
            profile: RefCell::new(None),
            catalog: RefCell::new(catalog),
            catalog_error: RefCell::new(catalog_error),
            language_codes,
            style_identifiers: RefCell::new(Vec::new()),
            microphone_targets: RefCell::new(Vec::new()),
            system_targets: RefCell::new(Vec::new()),
            refreshing_styles: Cell::new(false),
            refreshing_audio: Cell::new(false),
            syncing: Cell::new(false),
            cleanup_before_incognito: Cell::new(None),
        });
        owner.populate_audio();
        owner.refresh_styles();
        owner.connect();
        owner.apply_incognito();
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
    pub fn set_profile(&self, application: Option<String>) {
        self.profile.replace(application);
        self.refresh_mode();
        self.refresh_styles();
    }
    pub fn profile(&self) -> Option<String> {
        self.profile.borrow().clone()
    }
    pub fn cleanup_enabled(&self) -> bool {
        self.cleanup.is_active()
    }
    pub fn selected_style(&self) -> Option<String> {
        self.style_identifiers
            .borrow()
            .get(self.output_style.selected() as usize)
            .cloned()
            .flatten()
    }
    pub fn set_activity(&self, activity: PreferenceActivity) {
        self.activity.replace(activity);
    }
    /// Capture lifecycle owns when controls become available; activity alone is
    /// also updated during preparation/Stop without implying an idle UI reset.
    pub fn set_controls_available(&self, available: bool) {
        for row in [
            &self.mode,
            &self.language,
            &self.recording_key,
            &self.microphone,
            &self.system_audio,
            &self.audio_retention,
            &self.history_retention,
        ] {
            row.set_sensitive(available);
        }
        for row in [
            &self.spoken_commands,
            &self.remember_application,
            &self.incognito,
        ] {
            row.set_sensitive(available);
        }
        self.refresh_audio.set_sensitive(available);
        self.auto_paste
            .set_sensitive(available && self.tracking_available);
        self.apply_incognito();
    }
    fn connect(self: &Rc<Self>) {
        for (row, action) in [
            (&self.mode, Self::mode_changed as fn(&Self)),
            (&self.language, Self::language_changed),
            (&self.recording_key, Self::key_changed),
            (&self.output_style, Self::style_changed),
            (&self.audio_retention, Self::privacy_changed),
            (&self.history_retention, Self::privacy_changed),
        ] {
            let weak = Rc::downgrade(self);
            row.connect_selected_notify(move |_| {
                if let Some(owner) = weak.upgrade()
                    && !owner.syncing.get()
                {
                    action(&owner);
                    (owner.callbacks.summary_changed)();
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
        for row in [
            &self.spoken_commands,
            &self.auto_paste,
            &self.remember_application,
        ] {
            let weak = Rc::downgrade(self);
            let summary = *row == self.auto_paste;
            row.connect_active_notify(move |_| {
                if let Some(owner) = weak.upgrade()
                    && !owner.syncing.get()
                {
                    owner.general_changed();
                    if summary {
                        (owner.callbacks.summary_changed)();
                    }
                }
            });
        }
        let weak = Rc::downgrade(self);
        self.automatic_titles.connect_active_notify(move |_| {
            if let Some(owner) = weak.upgrade()
                && !owner.syncing.get()
            {
                owner.titles_changed();
            }
        });
        let weak = Rc::downgrade(self);
        self.incognito.connect_active_notify(move |_| {
            if let Some(owner) = weak.upgrade()
                && !owner.syncing.get()
            {
                owner.privacy_changed();
                (owner.callbacks.summary_changed)();
            }
        });
        for (button, action) in [
            (
                &self.refresh_audio,
                Self::refresh_audio_devices as fn(&Self),
            ),
            (&self.manage_styles, Self::manage_styles_action),
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
    fn manage_styles_action(&self) {
        (self.callbacks.manage_styles)();
    }
    fn export_action(&self) {
        (self.callbacks.export_diagnostics)();
    }
    fn mode_changed(&self) {
        set_mode_description(&self.mode);
        let Some(mode) = CAPTURE_MODES.get(self.mode.selected() as usize) else {
            return;
        };
        let profile = self.profile();
        if let Err(error) = self.services.select_capture_mode(mode, profile.as_deref()) {
            self.status(&format!(
                "{} could not be saved: {error}",
                if self.services.config().remember_per_application && profile.is_some() {
                    "Application mode"
                } else {
                    "Default capture mode"
                }
            ));
            return;
        }
        self.changed(InlineEffect::Mode);
        self.status(
            if self.services.config().remember_per_application && profile.is_some() {
                "Capture mode saved for the identified application."
            } else {
                "Default capture mode saved."
            },
        );
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
        let result = if self.activity.borrow().capture_active() {
            Err("Stop the active capture before changing the global recording key.".into())
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
            self.status(&error);
            return;
        }
        self.shortcut_status.set_subtitle(&format!(
            "Requesting {key} through the desktop portal; approve the replacement if prompted…"
        ));
        self.changed(InlineEffect::Shortcut);
        (self.callbacks.summary_changed)();
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
        proposed.auto_paste = self.auto_paste.is_active();
        proposed.remember_per_application = self.remember_application.is_active();
        if let Err(error) = self
            .services
            .save_inline_config(proposed, InlineSavePolicy::Persistent)
        {
            self.status(&format!("General settings could not be saved: {error}"));
            return;
        }
        self.changed(InlineEffect::General);
        self.refresh_mode();
        self.refresh_styles();
        self.status("General settings saved for new captures.");
    }
    fn titles_changed(&self) {
        let mut proposed = self.services.config();
        proposed.automatic_titles = self.automatic_titles.is_active();
        if self
            .services
            .save_inline_config(proposed, InlineSavePolicy::SessionTitles)
            .is_err()
        {
            (self.callbacks.toast)(
                "Could not save the title preference. It applies for this session.",
            );
        }
        self.changed(InlineEffect::Titles);
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
            self.apply_incognito();
            self.changed(InlineEffect::Privacy { persisted: false });
            self.status("Privacy settings could not be saved. The Incognito choice applies for this session.");
            return;
        }
        self.apply_incognito();
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
    fn style_changed(&self) {
        if self.refreshing_styles.get() {
            return;
        }
        let config = self.services.config();
        let identifier = self.selected_style();
        let profile = self.profile();
        let result = self.services.personalization.borrow_mut().select_style(
            identifier.as_deref(),
            profile
                .as_deref()
                .filter(|_| config.remember_per_application),
            config.remember_per_application,
        );
        if let Err(error) = result {
            self.status(&format!("Output style could not be saved: {error}"));
            return;
        }
        self.update_style_instructions();
        self.changed(InlineEffect::Style);
        self.status(if config.remember_per_application && profile.is_some() {
            "Output style saved for the identified application."
        } else {
            "Default output style saved for new captures."
        });
    }
    fn refresh_mode(&self) {
        let config = self.services.config();
        let result = self.services.personalization.borrow().selected_mode(
            self.profile.borrow().as_deref(),
            config.remember_per_application,
            &config.default_mode,
        );
        match result {
            Ok(mode) => self.mode.set_selected(
                CAPTURE_MODES
                    .iter()
                    .position(|candidate| *candidate == mode)
                    .unwrap() as u32,
            ),
            Err(error) => self.status(&error.to_string()),
        }
    }
    pub fn refresh_styles(&self) {
        if let Err(error) = self.services.synchronize_style_prompts() {
            self.status(&error.to_string());
            return;
        }
        let config = self.services.config();
        let profile = self.profile();
        let selection = self
            .services
            .personalization
            .borrow()
            .selected_style(profile.as_deref(), config.remember_per_application);
        let styles = self.services.personalization.borrow().styles();
        let (Ok(selection), Ok(styles)) = (selection, styles) else {
            return;
        };
        let identifiers: Vec<_> = std::iter::once(None)
            .chain(styles.iter().map(|style| Some(style.identifier.clone())))
            .collect();
        let labels: Vec<_> = std::iter::once("Faithful (no saved style)".to_owned())
            .chain(styles.iter().map(|style| style.name.clone()))
            .collect();
        let selected = identifiers
            .iter()
            .position(|identifier| {
                *identifier == selection.as_ref().map(|style| style.identifier.clone())
            })
            .unwrap_or(0);
        self.refreshing_styles.set(true);
        self.style_identifiers.replace(identifiers);
        self.output_style.set_model(Some(&gtk::StringList::new(
            &labels.iter().map(String::as_str).collect::<Vec<_>>(),
        )));
        self.output_style.set_selected(selected as u32);
        self.update_style_instructions();
        self.refreshing_styles.set(false);
    }
    fn update_style_instructions(&self) {
        let style = self
            .services
            .personalization
            .borrow()
            .style(self.selected_style().as_deref());
        if let Ok(style) = style {
            let (name, instructions) = style.as_ref().map_or(("Faithful", "No generative style rewrite. Local spoken structure, dictionary, and explicit snippets still apply."), |style| (style.name.as_str(), style.instructions.as_str()));
            self.style_instructions.set_subtitle(instructions);
            self.output_style
                .set_tooltip_text(Some(&format!("{name}: {instructions}")));
        }
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
    pub fn apply_incognito(&self) {
        let incognito = self.incognito.is_active();
        let config = self.services.config();
        self.automatic_titles
            .set_sensitive(config.rewrite_provider != "none" && !incognito);
        if incognito && self.cleanup_before_incognito.get().is_none() {
            self.cleanup_before_incognito
                .set(Some(self.cleanup.is_active()));
            self.cleanup.set_active(false);
            self.cleanup.set_subtitle("Unavailable in Incognito because Codex processing cannot guarantee ephemeral handling");
        } else if !incognito && let Some(previous) = self.cleanup_before_incognito.take() {
            self.cleanup.set_active(previous);
            self.cleanup.set_subtitle(CLEANUP_SUBTITLE);
        }
        let activity = self.activity.borrow();
        let enabled = config.rewrite_provider != "none"
            && !incognito
            && !activity.capture_active()
            && !activity.pending_review;
        self.cleanup.set_sensitive(enabled);
        self.output_style.set_sensitive(enabled);
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
fn set_mode_description(row: &adw::ComboRow) {
    if let Some((feature, description)) = MODE_DESCRIPTIONS.get(row.selected() as usize) {
        row.set_subtitle(&feature_maturity::description(feature, description));
    }
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
