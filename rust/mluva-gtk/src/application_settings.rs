//! One application-owned settings/setup graph, sharing actual config and prompt stores.

use adw::prelude::*;
use mluva_audio::catalog::PipeWireDeviceCatalog;
use mluva_core::database::StoreResult;
use mluva_workflows::services::{
    ApplicationServices, SettingsActivity, SettingsKind, SettingsUpdate,
};
use serde_json::{Map, Value, json};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use crate::{
    async_runtime::DesktopRuntime,
    capture_preferences::{CapturePreferenceCallbacks, CapturePreferences},
    prompt_editor::{Message, PromptSession, PromptsPage},
    provider_settings::ProviderSettings,
    settings_view::{SaveSettings, SettingsView},
    welcome_view::WelcomeView,
    workspace_settings::WorkspaceSettings,
};

pub struct ApplicationSettingsCallbacks {
    pub activity: Rc<dyn Fn() -> SettingsActivity>,
    /// Reconfigure current owners only after the atomic save. Active immutable
    /// recordings are governed by SettingsActivity and retain their snapshots.
    pub committed: Rc<dyn Fn(SettingsUpdate)>,
    pub navigate: Rc<dyn Fn(&str)>,
    pub prompts_changed: Rc<dyn Fn()>,
    pub message: Message,
    pub inline: CapturePreferenceCallbacks,
}

pub struct ApplicationSettings {
    pub view: Rc<SettingsView>,
    pub workspace: Rc<WorkspaceSettings>,
    pub providers: Rc<ProviderSettings>,
    pub welcome: Rc<WelcomeView>,
    pub capture: Rc<CapturePreferences>,
    pub prompts: Rc<PromptsPage>,
    pub prompt_session: Rc<PromptSession>,
    services: Rc<ApplicationServices>,
    callbacks: ApplicationSettingsCallbacks,
}

impl ApplicationSettings {
    pub fn new(
        services: Rc<ApplicationServices>,
        runtime: Rc<DesktopRuntime>,
        catalog: PipeWireDeviceCatalog,
        catalog_error: Option<String>,
        tracking_available: bool,
        callbacks: ApplicationSettingsCallbacks,
    ) -> StoreResult<Rc<Self>> {
        let current: Rc<RefCell<Weak<Self>>> = Rc::new(RefCell::new(Weak::new()));
        let weak = current.clone();
        let save: SaveSettings = Rc::new(move |changes| {
            weak.borrow()
                .upgrade()
                .is_some_and(|owner| owner.apply(changes))
        });
        let weak = current.clone();
        let open = Rc::new(move |identifier: &str| {
            if let Some(owner) = weak.borrow().upgrade() {
                owner.open_prompt(identifier);
            }
        });
        let navigate = callbacks.navigate.clone();
        let view = SettingsView::new(Rc::new(move || navigate("capture")));
        let config = services.config();
        let workspace = WorkspaceSettings::new(&config, save.clone(), Some(open.clone()));
        let providers = ProviderSettings::new(
            &config,
            services.paths.data.clone(),
            runtime.clone(),
            services.credentials.clone(),
            save.clone(),
            None,
        );
        view.add(&workspace.widget);
        view.add(&providers.widget);
        let capture = CapturePreferences::new(
            services.clone(),
            catalog,
            catalog_error,
            tracking_available,
            callbacks.inline.clone(),
        );
        capture.add_pages(&view);
        let prompts = PromptsPage::new(services.prompts.clone(), open)?;
        if !services.config_load_error.is_empty() {
            prompts.set_load_error(&services.config_load_error);
        }
        view.add(&prompts.widget);
        let weak = current.clone();
        let finish = Rc::new(move || {
            if let Some(owner) = weak.borrow().upgrade() {
                owner.finish_welcome();
            }
        });
        let welcome = WelcomeView::new(
            &config,
            services.paths.data.clone(),
            runtime,
            services.credentials.clone(),
            save,
            finish,
        );
        let saved = services.clone();
        let weak = current.clone();
        let prompt_session = PromptSession::new(
            services.prompts.clone(),
            Rc::new(move || !saved.config().incognito_mode),
            Rc::new(move || {
                if let Some(owner) = weak.borrow().upgrade() {
                    owner.prompts_changed();
                }
            }),
            callbacks.message.clone(),
        );
        let owner = Rc::new(Self {
            view,
            workspace,
            providers,
            welcome,
            capture,
            prompts,
            prompt_session,
            services,
            callbacks,
        });
        current.replace(Rc::downgrade(&owner));
        Ok(owner)
    }
    pub fn apply(&self, changes: &Map<String, Value>) -> bool {
        let activity = (self.callbacks.activity)();
        match self.services.apply_settings(changes, &activity) {
            Ok(Some(update)) => {
                if update.kind != SettingsKind::Unchanged {
                    (self.callbacks.committed)(update);
                }
                true
            }
            Ok(None) | Err(_) => false,
        }
    }
    pub fn show(&self) {
        let config = self.services.config();
        self.workspace.refresh_config(&config);
        self.providers.refresh_config(&config);
        if let Err(error) = self.prompts.refresh() {
            (self.callbacks.message)(&error.to_string());
        }
        (self.callbacks.navigate)("settings");
    }
    pub fn show_welcome(&self) {
        self.welcome.refresh_config(&self.services.config());
        (self.callbacks.navigate)("welcome");
    }
    fn finish_welcome(&self) {
        if self.apply(json!({"welcome_completed":true}).as_object().unwrap()) {
            (self.callbacks.navigate)("capture");
        } else {
            (self.callbacks.message)(
                "Could not save setup. Try again when the current operation finishes.",
            );
        }
    }
    pub fn open_prompt(&self, identifier: &str) {
        if let Some(parent) = self
            .view
            .widget
            .root()
            .and_then(|root| root.upcast::<glib::Object>().downcast::<gtk::Widget>().ok())
        {
            self.prompt_session.open(&parent, identifier);
        }
    }
    pub fn prompts_changed(&self) {
        self.capture.refresh_styles();
        if let Err(error) = self.prompts.refresh() {
            (self.callbacks.message)(&error.to_string());
        }
        (self.callbacks.prompts_changed)();
        (self.callbacks.message)("Prompt saved · next request, or next recording for Live");
    }
}
