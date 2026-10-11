//! The shared desktop graph: every page dispatches to its actual application owner.

use crate::{
    application_settings::{ApplicationSettings, ApplicationSettingsCallbacks},
    application_shell::{
        AdaptiveWorkspace, ApplicationAction, ApplicationShell, ShellCallbacks, ShellPages,
    },
    async_runtime::DesktopRuntime,
    capture_controller::{CaptureController, CaptureControllerCallbacks, CaptureOrigin},
    capture_preferences::{CapturePreferenceCallbacks, InlineEffect, PreferenceActivity},
    capture_view::{CaptureCallbacks, CapturePage, LiveSettingsChange},
    commands::{CommandContext, CommandState, application_commands},
    conversation_view::{ConversationCallbacks, ConversationWorkspace},
    document_layout::DocumentResources,
    global_shortcuts::GlobalShortcutService,
    history_controller::{HistoryController, HistoryControllerCallbacks},
    live_controller::{LiveCallbacks, LiveController},
    meeting_controller::{MeetingController, MeetingControllerCallbacks},
    overlay_state::OverlayPublisher,
    pending_review::{PendingReview, PendingReviewCallbacks},
    personalization::PersonalizationPage,
    review_controller::{ReviewCallbacks, ReviewController},
    rewrite_settings::RewriteSettings,
    text_target::{DeliveryTargetSnapshot, FocusedTextTargetTracker, TextTargetSnapshot},
    title_jobs::ConversationTitleJobs,
};
use adw::prelude::*;
use mluva_audio::catalog::PipeWireDeviceCatalog;
use mluva_core::config::AppConfig;
use mluva_providers::rewriting::RewriteClient;
use mluva_workflows::{
    capture::{CaptureOptions, CaptureSession},
    dictation::{WorkflowError, WorkflowOutcome},
    services::{ApplicationServices, CaptureServices, NativeBinaries},
};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

mod actions;
mod lifecycle;
mod phone;
mod screenshots;
mod shortcuts;

/// Distribution-owned compositor/bootstrap operations outside the shared graph.
pub struct ApplicationPlatform {
    pub compact_recording: Rc<dyn Fn(bool)>,
    pub close: Rc<dyn Fn()>,
}

struct CaptureContext {
    options: CaptureOptions,
    target: Option<DeliveryTargetSnapshot>,
    command_target: Option<TextTargetSnapshot>,
    continuation: Option<String>,
    session: Rc<CaptureSession>,
}

pub struct ApplicationDesktop {
    pub shell: Rc<ApplicationShell>,
    pub settings: Rc<ApplicationSettings>,
    pub capture: Rc<CaptureController>,
    pub history: Rc<HistoryController>,
    pub meeting: Rc<MeetingController>,
    pub pending: Rc<PendingReview>,
    pub live: Rc<LiveController>,
    pub review: Rc<ReviewController>,
    pub personalization: Rc<PersonalizationPage>,
    services: Rc<ApplicationServices>,
    application: adw::Application,
    runtime: Rc<DesktopRuntime>,
    binaries: NativeBinaries,
    platform: ApplicationPlatform,
    tracker: RefCell<Option<FocusedTextTargetTracker>>,
    shortcuts: RefCell<Option<Rc<GlobalShortcutService>>>,
    shortcut_generation: Cell<u64>,
    approved_recording: RefCell<Option<String>>,
    approved_rewrite: RefCell<Option<String>>,
    devices: RefCell<PipeWireDeviceCatalog>,
    titles: Rc<ConversationTitleJobs>,
    ready: RefCell<Option<Rc<CaptureServices>>>,
    readiness_generation: Cell<u64>,
    initialization_pending: Cell<bool>,
    initialization_failed: Cell<bool>,
    startup_record_requested: Cell<bool>,
    catalog: RefCell<Option<Rc<RewriteClient>>>,
    current: RefCell<Option<CaptureContext>>,
    screenshots: screenshots::ScreenshotOwners,
    overlay: RefCell<Option<OverlayPublisher>>,
    overlay_timer: RefCell<Option<glib::SourceId>>,
    closed: Cell<bool>,
    shutdown_complete: tokio_util::sync::CancellationToken,
    phone: phone::PhoneState,
}
type Link = Rc<RefCell<Weak<ApplicationDesktop>>>;
fn linked<R: Default>(link: &Link, call: impl FnOnce(&Rc<ApplicationDesktop>) -> R) -> R {
    let owner = link.borrow().upgrade();
    owner.map_or_else(R::default, |owner| call(&owner))
}
macro_rules! bind {
    ($link:expr, |$owner:ident $(, $arg:ident)*| $body:expr) => {{
        let link = $link.clone();
        Rc::new(move |$($arg),*| linked(&link, |$owner| $body))
    }};
}

impl ApplicationDesktop {
    pub fn new(
        application: &adw::Application,
        services: Rc<ApplicationServices>,
        runtime: Rc<DesktopRuntime>,
        resources: DocumentResources,
        binaries: NativeBinaries,
        platform: ApplicationPlatform,
    ) -> WorkflowOutcome<Rc<Self>> {
        let link: Link = Rc::new(RefCell::new(Weak::new()));
        let tracker = FocusedTextTargetTracker::new().ok();
        let (catalog, catalog_error) = match PipeWireDeviceCatalog::from_system(None) {
            Ok(catalog) => (catalog, None),
            Err(error) => (PipeWireDeviceCatalog::default(), Some(error.to_string())),
        };
        let workspace = ConversationWorkspace::new(
            services.conversations.clone(),
            ConversationCallbacks {
                copy: bind!(link, |app, text| app.copy_text(text)),
                rewrite: bind!(link, |app, text| app.review.request(text)),
                paste: bind!(link, |app, text| app.import_text(text)),
                open_archive: bind!(link, |app| app.dispatch(ApplicationAction::History)),
                save_prompt: bind!(link, |app, text| app.save_prompt(text)),
                cancel_rewrite: bind!(link, |app| app.review.cancel()),
                rename: bind!(link, |app, id, title| app.rename(id, title)),
                delete: bind!(link, |app, id| app.delete_conversation(id)),
                merge: bind!(link, |app, source, target| app
                    .merge_conversations(source, target)),
                continue_recording: bind!(link, |app, id| app.continue_recording(id)),
                capture_screenshot: Some(bind!(link, |app| app.request_screenshot())),
                edit_screenshot: bind!(link, |app, id| app.edit_screenshot(id)),
                remove_screenshot: bind!(link, |app, id| app.remove_screenshot(id)),
                edit_prompt: bind!(link, |app, id| app.settings.open_prompt(id)),
            },
            resources,
        )?;
        workspace.set_prompt_store(Some(services.prompts.clone()));
        workspace.set_config(services.config())?;
        workspace.set_private(services.config().incognito_mode);
        let rewrite = RewriteSettings::new(
            services.config(),
            bind!(link, |app| app.load_models()),
            bind!(link, |app, model, fast, effort| app
                .save_rewrite_settings(model, fast, effort)),
        );
        let page = CapturePage::new(
            workspace.clone(),
            rewrite,
            services.config(),
            CaptureCallbacks {
                toggle_recording: bind!(link, |app| app.toggle_recording(CaptureOrigin::Manual)),
                apply_live_settings: bind!(link, |app, change| app.apply_live_change(change)),
                toast: bind!(link, |app, text| app.shell.show_message(text)),
                open_prompt: bind!(link, |app, id| app.settings.open_prompt(id)),
                retry_initialization: bind!(link, |app| app.initialize_services(true)),
                accept_command: bind!(link, |app| app.pending.accept_command()),
                discard_command: bind!(link, |app| app.pending.discard_command()),
                copy_scratchpad: bind!(link, |app| app.pending.copy_notes()),
                delete_scratchpad: bind!(link, |app| app.pending.confirm_delete_notes()),
                output_changed: bind!(link, |app, value| app.pending.edit_notes(value)),
                announce: bind!(link, |app, text| app.announce(text)),
            },
        )?;
        let settings = ApplicationSettings::new(
            services.clone(),
            runtime.clone(),
            catalog.clone(),
            catalog_error,
            tracker.is_some(),
            ApplicationSettingsCallbacks {
                activity: bind!(link, |app| app.settings_activity()),
                committed: bind!(link, |app, update| app.settings_committed(update)),
                navigate: bind!(link, |app, name| app.shell.navigate(name)),
                prompts_changed: bind!(link, |app| app.refresh_style_controls()),
                message: bind!(link, |app, text| app.shell.show_message(text)),
                inline: CapturePreferenceCallbacks {
                    changed: bind!(link, |app, effect| app.inline_changed(effect)),
                    summary_changed: bind!(link, |app| app.refresh_status()),
                    routes_changed: bind!(link, |app, _microphone, _system| app.routes_changed()),
                    history_changed: bind!(link, |app| app.history_changed()),
                    status: bind!(link, |app, text| app.capture.page.set_status(text)),
                    toast: bind!(link, |app, text| app.shell.show_message(text)),
                    manage_styles: bind!(link, |app| app
                        .dispatch(ApplicationAction::Personalization)),
                    export_diagnostics: bind!(link, |app| app.export_diagnostics()),
                },
            },
        )?;
        let history = HistoryController::new(
            services.clone(),
            runtime.clone(),
            page.clone(),
            settings.capture.clone(),
            HistoryControllerCallbacks {
                activity: bind!(link, |app| app.activity()),
                pending_command: bind!(link, |app| app.pending.command_identifier()),
                changed: bind!(link, |app| app.history_changed()),
                idle: bind!(link, |app| {
                    app.idle();
                    app.clear_overlay();
                }),
                queue_title: bind!(link, |app, entry| app.titles.enqueue(entry)),
                close_screenshot: {
                    let link = link.clone();
                    Rc::new(move |id| {
                        link.borrow()
                            .upgrade()
                            .map_or(Ok(()), |app| app.close_screenshot_editor(id))
                    })
                },
                copy: bind!(link, |app, text| app.copy_text(text)),
            },
        )?;
        let pending = PendingReview::new(
            services.clone(),
            page.clone(),
            settings.capture.clone(),
            history.clone(),
            PendingReviewCallbacks {
                changed: bind!(link, |app| app.history_changed()),
                excluded_history: bind!(link, |app| app.excluded_history()),
            },
        );
        let meeting = MeetingController::new(
            services.clone(),
            runtime.clone(),
            page.clone(),
            settings.capture.clone(),
            catalog.clone(),
            MeetingControllerCallbacks {
                activity: bind!(link, |app| app.activity()),
                idle: bind!(link, |app| app.idle()),
                copy: bind!(link, |app, text| app.copy_text(text)),
            },
        )?;
        let live = LiveController::attach(
            workspace.clone(),
            runtime.clone(),
            services.config(),
            services.prompts.borrow().clone(),
            services.cwd.clone(),
            LiveCallbacks {
                images: {
                    let link = link.clone();
                    Rc::new(move |capture, conversation| {
                        link.borrow()
                            .upgrade()
                            .ok_or_else(|| "The application is closed.".to_owned())
                            .and_then(|app| {
                                app.capture_images(capture, conversation)
                                    .map_err(|e| e.to_string())
                            })
                    })
                },
                review: bind!(link, |app, id, phase, message| app
                    .review
                    .publish(id, phase, message)),
            },
        );
        let review = ReviewController::new(
            workspace.clone(),
            runtime.clone(),
            services.config(),
            services.prompts.borrow().clone(),
            services.personalization.clone(),
            services.cwd.clone(),
            ReviewCallbacks {
                capture_busy: bind!(link, |app| app.capture.phase().is_some()),
                publish: bind!(link, |app, state| app.publish(state)),
                open: bind!(link, |app| {
                    app.shell.navigate("capture");
                    app.shell.present();
                }),
                continue_recording: bind!(link, |app, id| app.continue_recording(id)),
            },
        );
        review.bind_live(&live);
        let capture = CaptureController::attach(
            page.clone(),
            runtime.clone(),
            {
                let link = link.clone();
                Rc::new(move |origin| {
                    link.borrow()
                        .upgrade()
                        .ok_or_else(|| WorkflowError::Invalid("The application is closed.".into()))
                        .and_then(|app| app.prepare_capture(origin))
                })
            },
            CaptureControllerCallbacks {
                wait_for_images: {
                    let link = link.clone();
                    Rc::new(move |id| match link.borrow().upgrade() {
                        Some(app) => app.wait_for_screenshot(id),
                        None => Box::pin(async {
                            Err(WorkflowError::Invalid("The application is closed.".into()))
                        }),
                    })
                },
                prepare_result: bind!(link, |app, session, result| app
                    .prepare_screenshot_result(session, result)),
                queue_title: bind!(link, |app, entry| app.titles.enqueue(entry)),
                images: {
                    let link = link.clone();
                    Rc::new(move |id| {
                        link.borrow()
                            .upgrade()
                            .ok_or_else(|| {
                                WorkflowError::Invalid("The application is closed.".into())
                            })
                            .and_then(|app| {
                                app.capture_images(
                                    Some(id),
                                    app.current
                                        .borrow()
                                        .as_ref()
                                        .and_then(|current| current.continuation.as_deref()),
                                )
                            })
                    })
                },
                completed: bind!(link, |app, completion| app.capture_completed(completion)),
                refresh_history: bind!(link, |app, result| app.capture_history_changed(result)),
                failed: bind!(link, |app, failure| app.capture_failed(failure)),
                cancelled: bind!(link, |app, id| app.capture_cancelled(id)),
                phase_changed: bind!(link, |app, phase| app.capture_phase(phase)),
                live_config_changed: bind!(link, |app, config| app.save_live_once(config)),
            },
        );
        capture.bind_live(live.clone());
        page.set_recording_action(bind!(link, |app| app.toggle_recording(CaptureOrigin::Manual)));
        let titles = ConversationTitleJobs::new(
            services.history.clone(),
            services.cwd.clone(),
            runtime.clone(),
            {
                let services = services.clone();
                Rc::new(move || services.config())
            },
            {
                let services = services.clone();
                Rc::new(move || {
                    services
                        .prompts
                        .borrow()
                        .read("title")
                        .map(|value| value.text)
                        .map_err(|e| e.to_string())
                })
            },
            bind!(link, |app, id| app.refresh_title(id)),
        );
        let personalization = PersonalizationPage::new(
            services.personalization.clone(),
            services.history.clone(),
            bind!(link, |app, text| app.shell.show_message(text)),
            bind!(link, |app| app.refresh_style_controls()),
            Some(bind!(link, |app, id| app.settings.open_prompt(id))),
        );
        let shell = ApplicationShell::new(
            application,
            ShellPages {
                capture: page.widget.clone().upcast(),
                settings: settings.view.widget.clone().upcast(),
                welcome: settings.welcome.widget.clone().upcast(),
                meeting: meeting.page.widget.clone().upcast(),
                history: history.page.widget.clone().upcast(),
                personalization: personalization.widget.clone().upcast(),
            },
            ShellCallbacks {
                invoke: bind!(link, |app, action| app.dispatch(action)),
                toggle_sidebar: bind!(link, |app| app.toggle_sidebar()),
                commands: bind!(link, |app| app.commands()),
                cancel_capture: bind!(link, |app| app.cancel()),
                compact: bind!(link, |app, compact| app.adapt(compact)),
            },
        );
        let owner = Rc::new(Self {
            shell,
            settings,
            capture,
            history,
            meeting,
            pending,
            live,
            review,
            personalization,
            services,
            application: application.clone(),
            runtime,
            binaries,
            platform,
            tracker: RefCell::new(tracker),
            shortcuts: RefCell::new(None),
            shortcut_generation: Cell::new(0),
            approved_recording: RefCell::new(None),
            approved_rewrite: RefCell::new(None),
            devices: RefCell::new(catalog),
            titles,
            ready: RefCell::new(None),
            readiness_generation: Cell::new(0),
            initialization_pending: Cell::new(false),
            initialization_failed: Cell::new(false),
            startup_record_requested: Cell::new(false),
            catalog: RefCell::new(None),
            current: RefCell::new(None),
            screenshots: screenshots::ScreenshotOwners::default(),
            overlay: RefCell::new(application.dbus_connection().and_then(|connection| {
                let mut publisher = OverlayPublisher::new(connection);
                publisher.clear().then_some(publisher)
            })),
            overlay_timer: RefCell::new(None),
            closed: Cell::new(false),
            shutdown_complete: tokio_util::sync::CancellationToken::new(),
            phone: phone::PhoneState::default(),
        });
        link.replace(Rc::downgrade(&owner));
        let weak = Rc::downgrade(&owner);
        crate::application_shell::register_actions(
            application,
            Rc::new(move |action| {
                if let Some(owner) = weak.upgrade() {
                    owner.dispatch(action);
                }
            }),
        );
        owner.refresh_style_controls();
        owner.pending.restore_notes();
        owner.open_latest(false)?;
        if !owner.services.config().welcome_completed {
            owner.shell.navigate("welcome");
        }
        if !owner.services.config_load_error.is_empty() {
            owner.shell.show_message(&owner.services.config_load_error);
        }
        owner.adapt(owner.shell.window.width() > 0 && owner.shell.window.width() <= 736);
        owner.initialize_services(true);
        if owner.start_phone_bridge().is_err() {
            owner.shell.show_message(
                "Phone microphone is unavailable. Check the private desktop runtime.",
            );
        }
        Ok(owner)
    }
    fn page(&self) -> &Rc<CapturePage> {
        &self.capture.page
    }
    fn workspace(&self) -> &Rc<ConversationWorkspace> {
        &self.capture.page.workspace
    }
}
