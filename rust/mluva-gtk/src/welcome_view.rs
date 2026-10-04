//! Three-step onboarding through the same provider and recorder controls as Settings.
use crate::{
    appearance_settings::AppearanceSettings,
    async_runtime::DesktopRuntime,
    document_layout::margins,
    provider_settings::ProviderSection,
    settings_view::{SaveSettings, proposed_config},
};
use adw::prelude::*;
use mluva_core::{config::AppConfig, text};
use mluva_providers::{catalog::Scope, credentials::CredentialStore};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    sync::Arc,
    time::Duration,
};

pub struct PolishPreview {
    pub widget: gtk::Box,
    pub caption: gtk::Label,
    pub text: gtk::Label,
    pub progress: gtk::ProgressBar,
    tick: Cell<usize>,
    timer: RefCell<Option<glib::SourceId>>,
}
impl PolishPreview {
    pub fn new() -> Rc<Self> {
        let widget = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(14)
            .margin_top(24)
            .margin_bottom(16)
            .build();
        widget.add_css_class("card");
        let caption = gtk::Label::builder()
            .label("Polishing preview")
            .xalign(0.0)
            .margin_start(18)
            .margin_top(16)
            .build();
        caption.add_css_class("heading");
        widget.append(&caption);
        let text = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .margin_start(18)
            .margin_end(18)
            .height_request(72)
            .build();
        widget.append(&text);
        let progress = gtk::ProgressBar::builder()
            .margin_start(18)
            .margin_end(18)
            .margin_bottom(18)
            .build();
        widget.append(&progress);
        let owner = Rc::new(Self {
            widget,
            caption,
            text,
            progress,
            tick: Cell::new(0),
            timer: RefCell::new(None),
        });
        let weak = Rc::downgrade(&owner);
        owner.widget.connect_map(move |_| {
            if let Some(owner) = weak.upgrade() {
                owner.start();
            }
        });
        let weak = Rc::downgrade(&owner);
        owner.widget.connect_unmap(move |_| {
            if let Some(owner) = weak.upgrade() {
                owner.stop();
            }
        });
        owner
    }
    fn start(self: &Rc<Self>) {
        self.tick.set(0);
        if self.widget.settings().is_gtk_enable_animations() {
            let weak = Rc::downgrade(self);
            self.timer.replace(Some(glib::timeout_add_local(
                Duration::from_millis(90),
                move || {
                    if let Some(owner) = weak.upgrade() {
                        owner.animate();
                        glib::ControlFlow::Continue
                    } else {
                        glib::ControlFlow::Break
                    }
                },
            )));
        } else {
            self.text
                .set_label("Let’s meet Friday at 10 to review the launch.");
            self.caption.set_label("Polishing preview · ready");
            self.progress.set_fraction(1.0);
        }
    }
    fn stop(&self) {
        if let Some(timer) = self.timer.borrow_mut().take() {
            timer.remove();
        }
    }
    fn animate(&self) {
        let raw = "so um let’s meet friday at ten to review the launch";
        let frame = self.tick.get() % 120;
        if frame < 52 {
            self.caption.set_label("Polishing preview · speaking");
            self.text
                .set_label(&(raw.chars().take(frame + 1).collect::<String>() + "▌"));
            self.progress.set_fraction(frame as f64 / 120.0);
        } else if frame < 70 {
            self.caption.set_label(&format!(
                "Polishing preview · refining{}",
                ".".repeat(frame / 4 % 3 + 1)
            ));
            self.text.set_label(raw);
            self.progress.set_fraction(frame as f64 / 120.0);
        } else {
            self.caption.set_label("Polishing preview · ready");
            self.text
                .set_label("Let’s meet Friday at 10 to review the launch.");
            self.progress.set_fraction(1.0);
        }
        self.tick.set(self.tick.get() + 1);
    }
}
impl Drop for PolishPreview {
    fn drop(&mut self) {
        self.stop();
    }
}

pub struct WelcomeView {
    pub widget: gtk::Box,
    pub steps: gtk::Stack,
    pub speech: Rc<ProviderSection>,
    pub rewrite: Rc<ProviderSection>,
    pub appearance: Rc<AppearanceSettings>,
    pub title: gtk::Label,
    pub step_label: gtk::Label,
    pub progress: gtk::ProgressBar,
    pub polish_preview: Rc<PolishPreview>,
    pub status: gtk::Label,
    pub back: gtk::Button,
    pub next: gtk::Button,
    config: RefCell<AppConfig>,
    step: Cell<usize>,
    save: SaveSettings,
    finish: Rc<dyn Fn()>,
    runtime: Rc<DesktopRuntime>,
    credentials: Arc<CredentialStore>,
    saving_key: Cell<bool>,
    checking_key: Cell<bool>,
    generation: Cell<u64>,
    timer: RefCell<Option<glib::SourceId>>,
}
impl WelcomeView {
    pub fn new(
        config: &AppConfig,
        data: PathBuf,
        runtime: Rc<DesktopRuntime>,
        credentials: Arc<CredentialStore>,
        save: SaveSettings,
        finish: Rc<dyn Fn()>,
    ) -> Rc<Self> {
        let widget = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .vexpand(true)
            .build();
        let steps = gtk::Stack::builder()
            .vexpand(true)
            .vhomogeneous(false)
            .hhomogeneous(false)
            .build();
        let speech = ProviderSection::new(config, Scope::Speech, data.clone(), runtime.clone());
        let rewrite = ProviderSection::new(config, Scope::Rewrite, data, runtime.clone());
        speech.widget.set_title("");
        rewrite.widget.set_title("");
        let appearance = AppearanceSettings::new(config, Rc::new(|| {}));
        appearance.widget.set_title("");
        appearance.widget.set_description(None);
        let title = gtk::Label::builder()
            .label("Choose speech recognition")
            .xalign(0.0)
            .wrap(true)
            .build();
        title.add_css_class("title-1");
        let header = gtk::Box::new(gtk::Orientation::Vertical, 8);
        margins(&header, 16);
        let label = gtk::Label::builder()
            .label("Set up Mluva in three steps")
            .xalign(0.0)
            .build();
        label.add_css_class("dim-label");
        header.append(&label);
        header.append(&title);
        let step_label = gtk::Label::builder().xalign(0.0).build();
        header.append(&step_label);
        let progress = gtk::ProgressBar::new();
        header.append(&progress);
        widget.append(&header);
        let speech_page = adw::PreferencesPage::new();
        speech_page.add(&speech.widget);
        let rewrite_page = adw::PreferencesPage::new();
        rewrite_page.add(&rewrite.widget);
        let polish_preview = PolishPreview::new();
        let group = adw::PreferencesGroup::new();
        group.add(&polish_preview.widget);
        rewrite_page.add(&group);
        let appearance_page = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .margin_start(20)
            .margin_end(20)
            .child(&appearance.widget)
            .build();
        steps.add_named(&speech_page, Some("speech"));
        steps.add_named(&rewrite_page, Some("rewrite"));
        steps.add_named(&appearance_page, Some("appearance"));
        widget.append(&steps);
        let footer = gtk::Box::new(gtk::Orientation::Vertical, 8);
        margins(&footer, 20);
        let status = gtk::Label::builder().wrap(true).xalign(0.0).build();
        footer.append(&status);
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let back = gtk::Button::with_label("Back");
        buttons.append(&back);
        buttons.append(&gtk::Label::builder().hexpand(true).build());
        let next = gtk::Button::with_label("Continue");
        next.add_css_class("suggested-action");
        buttons.append(&next);
        footer.append(&buttons);
        widget.append(&footer);
        let owner = Rc::new(Self {
            widget,
            steps,
            speech,
            rewrite,
            appearance,
            title,
            step_label,
            progress,
            polish_preview,
            status,
            back,
            next,
            config: RefCell::new(config.clone()),
            step: Cell::new(0),
            save,
            finish,
            runtime,
            credentials,
            saving_key: Cell::new(false),
            checking_key: Cell::new(false),
            generation: Cell::new(0),
            timer: RefCell::new(None),
        });
        let weak = Rc::downgrade(&owner);
        owner.back.connect_clicked(move |_| {
            if let Some(owner) = weak.upgrade() {
                owner.step.set(owner.step.get().saturating_sub(1));
                owner.show_step();
            }
        });
        let weak = Rc::downgrade(&owner);
        owner.next.connect_clicked(move |_| {
            if let Some(owner) = weak.upgrade() {
                owner.advance();
            }
        });
        let weak = Rc::downgrade(&owner);
        owner.widget.connect_map(move |_| {
            if let Some(owner) = weak.upgrade()
                && owner.timer.borrow().is_none()
            {
                let weak = Rc::downgrade(&owner);
                owner.timer.replace(Some(glib::timeout_add_local(
                    Duration::from_millis(200),
                    move || {
                        if let Some(owner) = weak.upgrade() {
                            owner.refresh();
                            glib::ControlFlow::Continue
                        } else {
                            glib::ControlFlow::Break
                        }
                    },
                )));
            }
        });
        let weak = Rc::downgrade(&owner);
        owner.widget.connect_unmap(move |_| {
            if let Some(owner) = weak.upgrade() {
                owner.unmapped();
            }
        });
        owner.refresh();
        owner
    }
    pub fn step(&self) -> usize {
        self.step.get()
    }
    pub fn refresh_config(&self, config: &AppConfig) {
        self.generation.set(self.generation.get() + 1);
        self.config.replace(config.clone());
        self.speech.refresh_config(config);
        self.rewrite.refresh_config(config);
        self.appearance.refresh_config(config);
        self.step.set(0);
        self.show_step();
    }
    fn unmapped(&self) {
        if let Some(timer) = self.timer.borrow_mut().take() {
            timer.remove();
        }
        self.speech.local.stop();
    }
    fn refresh(&self) {
        let step = self.step.get();
        self.step_label.set_label(&format!(
            "Step {} of 3 · {}",
            step + 1,
            ["Speech", "Polishing", "Recorder"][step]
        ));
        self.progress.set_fraction((step + 1) as f64 / 3.0);
        self.polish_preview
            .widget
            .set_visible(self.rewrite.provider().id == "codex");
        self.back.set_visible(step > 0);
        self.next
            .set_label(if step == 2 { "Open Mluva" } else { "Continue" });
        self.next.set_sensitive(
            !self.saving_key.get()
                && !self.checking_key.get()
                && (step != 0 || self.speech.is_ready()),
        );
    }
    fn show_step(&self) {
        self.steps
            .set_visible_child_name(["speech", "rewrite", "appearance"][self.step.get()]);
        self.title.set_label(
            [
                "Choose speech recognition",
                "Optional: polish your words",
                "Make Mluva yours",
            ][self.step.get()],
        );
        self.status.set_label("");
        self.refresh();
    }
    pub fn advance(self: &Rc<Self>) {
        if self.saving_key.get() || self.checking_key.get() || !self.speech.is_ready() {
            return;
        }
        if self.step.get() == 0 && self.speech.provider().id == "elevenlabs" {
            let key = text::trim(&self.speech.api_key_entry.text()).to_owned();
            if !key.is_empty() {
                self.speech.api_key_entry.set_text("");
                self.saving_key.set(true);
                self.status.set_label("Saving key to your desktop keyring…");
                self.refresh();
                let credentials = self.credentials.clone();
                let weak = Rc::downgrade(self);
                self.runtime.spawn(async move {
                    let result = credentials.store_speech_key(&key).await;
                    if let Some(owner) = weak.upgrade() {
                        owner.saving_key.set(false);
                        match result {
                            Ok(()) => {
                                owner.status.set_label("Key saved. You can continue.");
                                owner.step.set(1);
                                owner.show_step();
                            }
                            Err(error) => owner.status.set_label(&error.to_string()),
                        }
                        owner.refresh();
                    }
                });
                return;
            }
            // Keyring reads remain off the UI context. Only the still-current
            // step and route may act on a completion from this lookup.
            self.checking_key.set(true);
            self.refresh();
            let generation = self.generation.get();
            let values = self.speech.values();
            let credentials = self.credentials.clone();
            let weak = Rc::downgrade(self);
            self.runtime.spawn(async move {
                let result = credentials.elevenlabs_api_key().await;
                if let Some(owner) = weak.upgrade() {
                    owner.checking_key.set(false);
                    if owner.generation.get() == generation
                        && owner.step.get() == 0
                        && owner.speech.values() == values
                    {
                        if result.is_ok() {
                            owner.advance_validated();
                        } else {
                            owner.status.set_label(
                                "Paste your ElevenLabs key above, or choose a local model.",
                            );
                        }
                    }
                    owner.refresh();
                }
            });
            return;
        }
        self.advance_validated();
    }
    fn advance_validated(&self) {
        let mut changes = self.speech.values();
        changes.extend(self.rewrite.values());
        changes.extend(self.appearance.values());
        if changes["rewrite_provider"] == "none" {
            changes.insert("live_rewrite_enabled".into(), Value::Bool(false));
            changes.insert("automatic_titles".into(), Value::Bool(false));
        }
        if let Err(error) = proposed_config(&self.config.borrow(), &changes) {
            self.status.set_label(&error.to_string());
            return;
        }
        let section = if self.step.get() == 0 {
            &self.speech
        } else {
            &self.rewrite
        };
        if section.provider().id == "litellm"
            && section.values()[section.provider().model_field]
                .as_str()
                .is_none_or(str::is_empty)
        {
            self.status
                .set_label("Enter the compatible endpoint's model ID before continuing.");
            return;
        }
        if self.step.get() < 2 {
            self.step.set(self.step.get() + 1);
            self.show_step();
        } else if (self.save)(&changes) {
            (self.finish)();
        } else {
            self.status
                .set_label("Could not save settings. Your choices are kept; please try again.");
        }
    }
}
impl Drop for WelcomeView {
    fn drop(&mut self) {
        self.unmapped();
    }
}
