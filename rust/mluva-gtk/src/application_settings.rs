//! One application-owned speech settings and setup graph.

use adw::prelude::*;
use mluva_audio::catalog::PipeWireDeviceCatalog;
use mluva_core::database::StoreResult;
use mluva_workflows::services::{
    ApplicationServices, SettingsActivity, SettingsKind, SettingsUpdate,
};
use serde_json::{Map, Value, json};
use std::{
    cell::{OnceCell, RefCell},
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
    pub welcome_page: gtk::Box,
    welcome: OnceCell<Rc<WelcomeView>>,
    create_welcome: Box<dyn Fn() -> Rc<WelcomeView>>,
    pub capture: Rc<CapturePreferences>,
    services: Rc<ApplicationServices>,
    callbacks: ApplicationSettingsCallbacks,
}

impl ApplicationSettings {
    /// Search uses the same preference rows as the continuous settings page.
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
        let weak = current.clone();
        let view = SettingsView::new(Rc::new(move || {
            if let Some(owner) = weak.borrow().upgrade() {
                owner.flush();
            }
            navigate("capture");
        }));
        let config = services.config();
        let workspace = WorkspaceSettings::new(&config, save.clone());
        let credential_target = current.clone();
        let providers = ProviderSettings::new(
            &config,
            services.paths.data.clone(),
            runtime.clone(),
            services.credentials.clone(),
            save.clone(),
            None,
            Rc::new(move || {
                let Some(owner) = credential_target.borrow().upgrade() else {
                    return false;
                };
                let activity = (owner.callbacks.activity)();
                if activity.preparing || activity.processing || activity.recording {
                    return false;
                }
                (owner.callbacks.committed)(SettingsUpdate {
                    config: owner.services.config(),
                    changed: Default::default(),
                    kind: SettingsKind::Providers,
                });
                true
            }),
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
        // Completed onboarding is not on the normal startup path. Construct
        // its duplicate provider/appearance controls only when it is opened.
        let welcome_page = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let welcome_services = services.clone();
        let create_welcome = Box::new(move || {
            WelcomeView::new(
                &welcome_services.config(),
                welcome_services.paths.data.clone(),
                runtime.clone(),
                welcome_services.credentials.clone(),
                save.clone(),
                finish.clone(),
            )
        });
        let owner = Rc::new(Self {
            view,
            workspace,
            providers,
            welcome_page,
            welcome: OnceCell::new(),
            create_welcome,
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
    pub fn flush(&self) {
        self.workspace.apply();
        self.providers.apply();
    }
    pub fn show(&self) {
        let config = self.services.config();
        self.workspace.refresh_config(&config);
        self.providers.refresh_config(&config);
        (self.callbacks.navigate)("settings");
    }
    pub fn show_welcome(&self) {
        let welcome = self.welcome.get_or_init(|| {
            let welcome = (self.create_welcome)();
            self.welcome_page.append(&welcome.widget);
            welcome
        });
        welcome.refresh_config(&self.services.config());
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
