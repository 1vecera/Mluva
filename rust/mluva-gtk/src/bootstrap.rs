//! Own one resident application process. Remote activations use GApplication's
//! existing bus contract and never open a second set of local stores.

use crate::{
    application::{ApplicationDesktop, ApplicationPlatform},
    application_shell::{ApplicationAction, register_actions},
    async_runtime::DesktopRuntime,
    document_layout::DocumentResources,
    theme::ThemeController,
};
use adw::prelude::*;
use mluva_workflows::services::{ApplicationServices, NativeBinaries};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
};

const APPLICATION_ID: &str = "com.mluva.Linux";
const START_ERROR: &str = "Mluva could not start. Check its installed resources and local data.";

#[derive(Default)]
struct Process {
    desktop: RefCell<Option<Rc<ApplicationDesktop>>>,
    theme: RefCell<Option<Rc<ThemeController>>>,
    runtime: RefCell<Option<Rc<DesktopRuntime>>>,
    shutdown: RefCell<Option<glib::JoinHandle<()>>>,
    failed: Cell<bool>,
}

impl Process {
    fn activate(self: &Rc<Self>, application: &adw::Application) {
        if self.desktop.borrow().is_none() && self.initialize(application).is_err() {
            self.failed.set(true);
            self.theme.borrow_mut().take();
            eprintln!("{START_ERROR}");
            application.quit();
            return;
        }
        if let Some(desktop) = self.desktop.borrow().as_ref() {
            desktop.shell.present();
        }
    }

    fn initialize(
        self: &Rc<Self>,
        application: &adw::Application,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // A relocated installation has bin/ and resources/ siblings. Do not
        // fall back to a checkout, inherited Python path or another install.
        let executable = std::env::current_exe()?;
        let root = executable
            .parent()
            .and_then(|path| path.parent())
            .ok_or_else(|| std::io::Error::other("Missing application directory"))?;
        let resources = root.join("resources");
        let state = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
            })
            .ok_or_else(|| std::io::Error::other("Missing state directory"))?;
        let theme = ThemeController::apply(
            state.join("omarchy/current/theme"),
            &resources.join("fonts"),
        )?;
        gtk::IconTheme::for_display(&gtk::gdk::Display::default().ok_or("Missing display")?)
            .add_search_path(&resources);
        self.theme.replace(Some(theme));
        let services = ApplicationServices::from_environment()?;
        let runtime = DesktopRuntime::new()?;
        let weak = Rc::downgrade(self);
        let desktop = ApplicationDesktop::new(
            application,
            services,
            runtime.clone(),
            DocumentResources::from_directory(&resources),
            NativeBinaries::beside_application()?,
            ApplicationPlatform {
                // The released application does not construct the obsolete
                // in-window RecordingStatusBar; its live document adapts itself.
                compact_recording: Rc::new(|_| {}),
                close: Rc::new(move || {
                    if let Some(process) = weak.upgrade() {
                        process.theme.borrow_mut().take();
                    }
                }),
            },
        )?;
        self.runtime.replace(Some(runtime));
        self.desktop.replace(Some(desktop));
        Ok(())
    }

    fn action(self: &Rc<Self>, application: &adw::Application, action: ApplicationAction) {
        let cold = self.desktop.borrow().is_none();
        if cold {
            match action {
                ApplicationAction::Quit => {
                    application.quit();
                    return;
                }
                ApplicationAction::Latest
                | ApplicationAction::Record
                | ApplicationAction::History
                | ApplicationAction::Settings
                | ApplicationAction::Commands => self.activate(application),
                _ => return,
            }
        }
        let desktop = self.desktop.borrow().clone();
        if let Some(desktop) = desktop {
            if cold && action == ApplicationAction::Record {
                desktop.record_after_startup();
            } else {
                desktop.dispatch(action);
            }
        }
    }

    fn shutting_down(&self) {
        if let Some(desktop) = self.desktop.borrow().as_ref() {
            self.shutdown.replace(Some(desktop.shutdown()));
        }
    }

    fn finish(&self) {
        // The GApplication shutdown signal may run after its main loop stops.
        // Keep the GTK context and native I/O runtime alive for owned cleanup.
        if let Some(done) = self.shutdown.borrow_mut().take() {
            let _ = glib::MainContext::default().block_on(done);
        }
        self.desktop.borrow_mut().take();
        self.theme.borrow_mut().take();
        self.runtime.borrow_mut().take();
    }
}

pub fn run(arguments: &[String]) -> glib::ExitCode {
    let application = adw::Application::builder()
        .application_id(APPLICATION_ID)
        .build();
    let process = Rc::new(Process::default());
    let actions = process.clone();
    let app = application.downgrade();
    register_actions(
        &application,
        Rc::new(move |action| {
            if let Some(app) = app.upgrade() {
                actions.action(&app, action);
            }
        }),
    );
    let activated = process.clone();
    application.connect_activate(move |application| activated.activate(application));
    let closing = process.clone();
    application.connect_shutdown(move |_| closing.shutting_down());
    let result = application.run_with_args(arguments);
    process.finish();
    if process.failed.get() {
        glib::ExitCode::FAILURE
    } else {
        result
    }
}
