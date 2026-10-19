//! One application-owned speech settings and setup graph.

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
    provider_settings::ProviderSettings,
    settings_view::{SaveSettings, SettingsView},
    welcome_view::WelcomeView,
    workspace_settings::WorkspaceSettings,
};

type Message = Rc<dyn Fn(&str)>;

pub struct ApplicationSettingsCallbacks {
    pub activity: Rc<dyn Fn() -> SettingsActivity>,
    /// Reconfigure current owners only after the atomic save. Active immutable
    /// recordings are governed by SettingsActivity and retain their snapshots.
    pub committed: Rc<dyn Fn(SettingsUpdate)>,
    pub navigate: Rc<dyn Fn(&str)>,
    pub message: Message,
    pub inline: CapturePreferenceCallbacks,
}

pub struct ApplicationSettings {
    pub view: Rc<SettingsView>,
    pub workspace: Rc<WorkspaceSettings>,
    pub providers: Rc<ProviderSettings>,
    pub welcome: Rc<WelcomeView>,
    pub capture: Rc<CapturePreferences>,
    services: Rc<ApplicationServices>,
    callbacks: ApplicationSettingsCallbacks,
}

impl ApplicationSettings {
    /// Searchable preference pages exclude informational maturity rows.
    pub(crate) fn command_pages(&self) -> [adw::PreferencesPage; 6] {
        [
            self.workspace.widget.clone(),
            self.providers.widget.clone(),
            self.capture.capture.clone(),
            self.capture.audio.clone(),
            self.capture.privacy.clone(),
            self.capture.advanced.clone(),
        ]
    }

    pub fn new(
        services: Rc<ApplicationServices>,
        runtime: Rc<DesktopRuntime>,
        catalog: PipeWireDeviceCatalog,
        catalog_error: Option<String>,
        callbacks: ApplicationSettingsCallbacks,
    ) -> StoreResult<Rc<Self>> {
        let current: Rc<RefCell<Weak<Self>>> = Rc::new(RefCell::new(Weak::new()));
        let weak = current.clone();
        let save: SaveSettings = Rc::new(move |changes| {
            weak.borrow()
                .upgrade()
                .is_some_and(|owner| owner.apply(changes))
        });
        let navigate = callbacks.navigate.clone();
        let view = SettingsView::new(Rc::new(move || navigate("capture")));
        let config = services.config();
        let workspace = WorkspaceSettings::new(&config, save.clone());
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
            callbacks.inline.clone(),
        );
        capture.add_pages(&view);
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
        let owner = Rc::new(Self {
            view,
            workspace,
            providers,
            welcome,
            capture,
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
}
