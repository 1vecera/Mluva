//! The shared desktop graph: every page dispatches to its actual application owner.

use crate::{
    application_settings::{ApplicationSettings, ApplicationSettingsCallbacks},
    application_shell::{
        AdaptiveWorkspace, ApplicationAction, ApplicationShell, ShellCallbacks, ShellPages,
    },
    async_runtime::DesktopRuntime,
    capture_controller::{CaptureController, CaptureControllerCallbacks, CaptureOrigin},
    capture_preferences::{CapturePreferenceCallbacks, InlineEffect, PreferenceActivity},
    capture_view::{CaptureCallbacks, CapturePage},
    commands::{CommandContext, CommandState, application_commands},
    conversation_view::{ConversationCallbacks, ConversationWorkspace},
    document_layout::DocumentResources,
    global_shortcuts::GlobalShortcutService,
    history_controller::{HistoryController, HistoryControllerCallbacks},
    meeting_controller::{MeetingController, MeetingControllerCallbacks},
    overlay_state::OverlayPublisher,
    personalization::PersonalizationPage,
    text_target::{DeliveryTargetSnapshot, FocusedTextTargetTracker},
};
use adw::prelude::*;
use mluva_audio::catalog::PipeWireDeviceCatalog;
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
mod annotations;
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
    continuation: Option<String>,
    agent_attempt: Option<String>,
    transcript_only: bool,
    session: Rc<CaptureSession>,
}

pub struct ApplicationDesktop {
    pub account: Rc<crate::account_controller::AccountController>,
    pub agent_voice: Rc<crate::agent_voice_controller::AgentVoiceController>,
    capture_audio_waiting: Cell<bool>,
    pub shell: Rc<ApplicationShell>,
    pub settings: Rc<ApplicationSettings>,
    pub capture: Rc<CaptureController>,
    pub history: Rc<HistoryController>,
    pub meeting: Rc<MeetingController>,
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
    ready: RefCell<Option<Rc<CaptureServices>>>,
    readiness_generation: Cell<u64>,
    initialization_pending: Cell<bool>,
    initialization_failed: Cell<bool>,
    startup_record_requested: Cell<bool>,
    current: RefCell<Option<CaptureContext>>,
    screenshots: screenshots::ScreenshotOwners,
    annotations: annotations::AnnotationOwners,
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
                open_archive: bind!(link, |app| app.dispatch(ApplicationAction::History)),
                rename: bind!(link, |app, id, title| app.rename(id, title)),
                delete: bind!(link, |app, id| app.delete_conversation(id)),
                merge: bind!(link, |app, source, target| app
                    .merge_conversations(source, target)),
                continue_recording: bind!(link, |app, id| app.continue_recording(id)),
                capture_screenshot: Some(bind!(link, |app| app.request_screenshot())),
                edit_screenshot: bind!(link, |app, id| app.edit_screenshot(id)),
                remove_screenshot: bind!(link, |app, id| app.remove_screenshot(id)),
            },
            resources,
        )?;
        workspace.set_config(services.config())?;
        workspace.set_private(services.config().incognito_mode);
        let page = CapturePage::new(
            workspace.clone(),
            services.config(),
            CaptureCallbacks {
                toggle_recording: bind!(link, |app| app.toggle_recording(CaptureOrigin::Manual)),
                toast: bind!(link, |app, text| app.shell.show_message(text)),
                retry_initialization: bind!(link, |app| app.initialize_services(true)),
                announce: bind!(link, |app, text| app.announce(text)),
            },
        )?;
        let settings = ApplicationSettings::new(
            services.clone(),
            runtime.clone(),
            catalog.clone(),
            catalog_error,
            ApplicationSettingsCallbacks {
                activity: bind!(link, |app| app.settings_activity()),
                committed: bind!(link, |app, update| app.settings_committed(update)),
                navigate: bind!(link, |app, name| app.shell.navigate(name)),
                message: bind!(link, |app, text| app.shell.show_message(text)),
                inline: CapturePreferenceCallbacks {
                    changed: bind!(link, |app, effect| app.inline_changed(effect)),
                    routes_changed: bind!(link, |app, _microphone, _system| app.routes_changed()),
                    history_changed: bind!(link, |app| app.history_changed()),
                    status: bind!(link, |app, text| app.capture.page.set_status(text)),
                    toast: bind!(link, |app, text| app.shell.show_message(text)),
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
                changed: bind!(link, |app| app.history_changed()),
                idle: bind!(link, |app| {
                    app.idle();
                    app.clear_overlay();
                }),
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
        let meeting = MeetingController::new(
            services.clone(),
            runtime.clone(),
            page.clone(),
            settings.capture.clone(),
            catalog.clone(),
            MeetingControllerCallbacks {
                stop_playback: bind!(link, |app| {
                    app.agent_voice
                        .disarm(mluva_core::agent_voice::DraftReason::Interrupted);
                    app.agent_voice.stop_voice()
                }),
                activity: bind!(link, |app| app.activity()),
                idle: bind!(link, |app| app.idle()),
                copy: bind!(link, |app, text| app.copy_text(text)),
            },
        )?;
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
                appendix: {
                    let link = link.clone();
                    Rc::new(move |session: &CaptureSession| {
                        link.borrow()
                            .upgrade()
                            .and_then(|app| app.annotation_appendix(&session.identifier))
                    })
                },
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
                preview_changed: bind!(link, |app| app.publish_preview()),
            },
        );
        page.set_recording_action(bind!(link, |app| app.toggle_recording(CaptureOrigin::Manual)));
        let personalization = PersonalizationPage::new(
            services.personalization.clone(),
            services.history.clone(),
            bind!(link, |app, text| app.shell.show_message(text)),
        );
        let shell = ApplicationShell::new(
            application,
            ShellPages {
                capture: page.widget.clone().upcast(),
                settings: settings.view.widget.clone().upcast(),
                welcome: settings.welcome_page.clone().upcast(),
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
        let account = crate::account_controller::AccountController::new(
            runtime.clone(),
            services.conversations.clone(),
            workspace.clone(),
            bind!(link, |app| app.history_changed()),
        );
        settings.view.prepend(&account.page);
        account
            .status
            .bind_property("label", &shell.sync_button, "tooltip-text")
            .transform_to(|_, status: String| Some(format!("Account sync · {status}")))
            .sync_create()
            .build();
        page.widget.add_top_bar(&account.incoming);
        let agent_voice = crate::agent_voice_controller::AgentVoiceController::new(
            runtime.clone(),
            services.clone(),
            page.clone(),
            crate::agent_voice_controller::AgentVoiceCallbacks {
                toggle: bind!(link, |app| app.agent_toggle()),
                select: bind!(link, |app| app.agent_shortcut()),
                transcript: bind!(link, |app| {
                    app.dispatch(ApplicationAction::GlobalRecord);
                }),
                release: bind!(link, |app| app.agent_release()),
                cancel: bind!(link, |app| {
                    app.cancel();
                }),
                interrupt: bind!(link, |app| app.agent_interrupted()),
                settings_changed: bind!(link, |app| app.restart_shortcuts()),
                capture_busy: bind!(link, |app| app.capture.phase().is_some()
                    || app.capture_audio_waiting.get()
                    || app.meeting.busy()
                    || app.ready.borrow().is_none()),
                draft_saved: bind!(link, |app| {
                    let _ = app.history.page.refresh();
                    app.account.sync();
                }),
                copy: bind!(link, |app, text| app.copy_text(text)),
            },
        );
        page.widget.add_bottom_bar(&agent_voice.view.widget);
        page.buttons.append(&agent_voice.view.record);
        settings
            .capture
            .shortcuts
            .add(&agent_voice.view.hold_key_row);
        settings
            .capture
            .shortcuts
            .add(&agent_voice.view.draft_key_row);
        settings
            .capture
            .shortcuts
            .add(&agent_voice.view.shortcut_status);
        settings.capture.capture.add(&agent_voice.view.settings);
        let (annotations, annotation_error) = annotations::AnnotationOwners::new(&services);
        settings.capture.capture.add(&annotations.group);
        {
            let link = link.clone();
            annotations.row.connect_active_notify(move |row| {
                let active = row.is_active();
                linked(&link, |app| app.set_annotation_enabled(active));
            });
        }
        let owner = Rc::new(Self {
            agent_voice,
            capture_audio_waiting: Cell::new(false),
            account,
            shell,
            settings,
            capture,
            history,
            meeting,
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
            ready: RefCell::new(None),
            readiness_generation: Cell::new(0),
            initialization_pending: Cell::new(false),
            initialization_failed: Cell::new(false),
            startup_record_requested: Cell::new(false),
            current: RefCell::new(None),
            screenshots: screenshots::ScreenshotOwners::default(),
            annotations,
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
        // Shift ink and pins as application actions: Hyprland's temporary bindings use the
        // portal instead, other layer-shell desktops and the offscreen harness call these.
        for (name, gesture) in [
            ("annotation-ink-down", Some(true)),
            ("annotation-ink-up", Some(false)),
            ("annotation-pin", None),
        ] {
            let weak = Rc::downgrade(&owner);
            let action = gio::SimpleAction::new(name, None);
            action.connect_activate(move |_, _| {
                if let Some(owner) = weak.upgrade().filter(|owner| !owner.closed.get()) {
                    match gesture {
                        Some(down) => owner.annotation_ink(down),
                        None => owner.annotation_pin(),
                    }
                }
            });
            application.add_action(&action);
        }
        owner.open_latest(false)?;
        if let Some(connection) = application.dbus_connection() {
            owner.agent_voice.register_cli(&connection).map_err(|_| {
                WorkflowError::Invalid("Mluva's local routing commands could not start.".into())
            })?;
        }
        if !owner.services.config().welcome_completed {
            owner.settings.show_welcome();
        }
        if !owner.services.config_load_error.is_empty() {
            owner.shell.show_message(&owner.services.config_load_error);
        }
        if !owner.services.recovery_message.is_empty() {
            owner.shell.show_message(&owner.services.recovery_message);
        }
        if let Some(message) = annotation_error {
            owner.shell.show_message(message);
        }
        owner.clean_annotation_leftovers();
        owner.adapt(owner.shell.window.width() > 0 && owner.shell.window.width() <= 736);
        owner.initialize_services(true);
        owner
            .account
            .privacy(owner.services.config().incognito_mode);
        owner.account.restore();
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
