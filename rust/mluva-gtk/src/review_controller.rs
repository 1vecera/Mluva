//! One owned document rewrite and deliberate review actions on the GTK context.
use crate::{
    async_runtime::DesktopRuntime, conversation_view::ConversationWorkspace,
    live_controller::LiveController, overlay_state::OverlayState,
};
use gtk::prelude::*;
use mluva_core::{
    config::AppConfig,
    conversation::rewrite_prompt,
    delivery::{DeliveryOptions, deliver_text},
    history::HistoryEntry,
    personalization::PersonalizationStore,
    prompts::PromptStore,
    screenshots::ScreenshotStore,
};
use mluva_providers::rewriting::{RewriteClient, RewriteError};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::{Rc, Weak},
    time::Instant,
};

pub struct ReviewCallbacks {
    pub capture_busy: Rc<dyn Fn() -> bool>,
    pub publish: Rc<dyn Fn(OverlayState)>,
    pub open: Rc<dyn Fn()>,
    pub continue_recording: Rc<dyn Fn(&str)>,
}
struct Request {
    identifier: String,
    instruction: String,
    draft: Option<String>,
    auto_copy: bool,
    client: Rc<RewriteClient>,
    preview: RefCell<String>,
    progress: RefCell<Option<glib::JoinHandle<()>>>,
}
pub struct ReviewController {
    pub workspace: Rc<ConversationWorkspace>,
    runtime: Rc<DesktopRuntime>,
    config: RefCell<AppConfig>,
    prompts: PromptStore,
    personalization: Rc<RefCell<PersonalizationStore>>,
    screenshots: ScreenshotStore,
    cwd: PathBuf,
    callbacks: ReviewCallbacks,
    live: RefCell<Option<Weak<LiveController>>>,
    current: RefCell<Option<String>>,
    request: RefCell<Option<Rc<Request>>>,
    shutting_down: Cell<bool>,
}
impl ReviewController {
    pub fn new(
        workspace: Rc<ConversationWorkspace>,
        runtime: Rc<DesktopRuntime>,
        config: AppConfig,
        prompts: PromptStore,
        personalization: Rc<RefCell<PersonalizationStore>>,
        cwd: PathBuf,
        callbacks: ReviewCallbacks,
    ) -> Rc<Self> {
        let screenshots = ScreenshotStore::new(&workspace.store.history.database.path);
        Rc::new(Self {
            workspace,
            runtime,
            config: RefCell::new(config),
            prompts,
            personalization,
            screenshots,
            cwd,
            callbacks,
            live: RefCell::new(None),
            current: RefCell::new(None),
            request: RefCell::new(None),
            shutting_down: Cell::new(false),
        })
    }
    pub fn bind_live(&self, live: &Rc<LiveController>) {
        assert!(Rc::ptr_eq(&self.workspace, &live.workspace));
        *self.live.borrow_mut() = Some(Rc::downgrade(live));
    }
    fn live(&self) -> Option<Rc<LiveController>> {
        self.live.borrow().as_ref().and_then(Weak::upgrade)
    }
    pub fn set_config(&self, config: AppConfig) {
        *self.config.borrow_mut() = config;
    }
    pub fn current(&self) -> Option<String> {
        self.current.borrow().clone()
    }
    pub fn rewriting(&self) -> bool {
        self.request.borrow().is_some()
    }
    pub fn rewrite_identifier(&self) -> Option<String> {
        self.request
            .borrow()
            .as_ref()
            .map(|request| request.identifier.clone())
    }
    pub fn publish(self: &Rc<Self>, identifier: &str, phase: &str, message: &str) {
        if self.config.borrow().incognito_mode {
            self.dismiss();
            return;
        }
        let snapshot = (|| {
            let entry = self.workspace.store.history.find(identifier)?;
            let replies = self.workspace.store.replies(identifier)?;
            let preview = replies
                .last()
                .map(|reply| reply.text.clone())
                .unwrap_or(self.workspace.store.source_text(&entry, true)?);
            Ok::<_, mluva_core::database::StoreError>(preview)
        })();
        let Ok(mut preview) = snapshot else {
            self.dismiss();
            if self.rewrite_identifier().as_deref() == Some(identifier) {
                self.cancel();
            }
            return;
        };
        self.current.replace(Some(identifier.into()));
        let mut phase = phase;
        let mut message = message;
        if self
            .live()
            .is_some_and(|live| live.final_entry().as_deref() == Some(identifier))
        {
            phase = "rewriting";
            preview = self.workspace.live_draft();
            message = "Reconciling the final transcript…";
        }
        if phase == "rewriting"
            && let Some(request) = self
                .request
                .borrow()
                .as_ref()
                .filter(|request| request.identifier == identifier)
            && !request.preview.borrow().is_empty()
        {
            preview = request.preview.borrow().clone();
        }
        let mut state = self.state(phase, identifier, preview, message);
        state.review_options = self
            .personalization
            .borrow()
            .styles()
            .unwrap_or_default()
            .into_iter()
            .map(|style| (style.identifier, style.name))
            .collect();
        (self.callbacks.publish)(state);
    }
    fn state(&self, phase: &str, identifier: &str, preview: String, message: &str) -> OverlayState {
        let config = self.config.borrow();
        OverlayState {
            phase: phase.into(),
            preview,
            review_identifier: identifier.into(),
            message: message.into(),
            review_timeout_seconds: config.review_timeout_seconds,
            show_copy_action: config.show_copy_action,
            smooth_scrolling: config.smooth_scrolling
                && config.scroll_duration_ms > 0
                && gtk::Settings::default()
                    .is_none_or(|settings| settings.is_gtk_enable_animations()),
            scroll_duration_ms: config.scroll_duration_ms,
            scroll_lookahead_lines: config.scroll_lookahead_lines,
            widget_position: config.widget_position.clone(),
            widget_lines: config.widget_lines,
            widget_opacity: config.widget_opacity,
            rewrite_enabled: config.rewrite_provider != "none",
            ..OverlayState::default()
        }
    }
    pub fn dismiss(&self) {
        self.current.borrow_mut().take();
        (self.callbacks.publish)(OverlayState::default());
    }
    /// Revoke completed-note actions while capture replaces the visible overlay.
    pub fn clear_for_capture(&self) {
        self.current.borrow_mut().take();
    }
    pub fn action(self: &Rc<Self>, operation: &str, identifier: &str, option: &str) {
        if self.shutting_down.get()
            || self.config.borrow().incognito_mode
            || identifier.is_empty()
            || self.current.borrow().as_deref() != Some(identifier)
        {
            return;
        }
        if operation == "dismiss" {
            self.dismiss();
            return;
        }
        let Ok(entry) = self.workspace.store.history.find(identifier) else {
            self.dismiss();
            return;
        };
        let final_live = self.live().and_then(|live| live.final_entry());
        let active = self.rewrite_identifier().as_deref() == Some(identifier)
            || final_live.as_deref() == Some(identifier);
        match operation {
            "rewrite" => {
                let instruction = match option {
                    "polish" => self
                        .prompts
                        .read("rewrite-polish")
                        .map(|prompt| Some(prompt.text)),
                    "structure" => self
                        .prompts
                        .read("rewrite-structure")
                        .map(|prompt| Some(prompt.text)),
                    _ => self
                        .personalization
                        .borrow()
                        .style(Some(option))
                        .map(|style| style.map(|style| style.instructions)),
                };
                if let Ok(Some(instruction)) = instruction {
                    self.begin(identifier, &instruction);
                }
            }
            "cancel" if active => self.cancel(),
            "copy" if !active => {
                let output = self
                    .workspace
                    .store
                    .replies(identifier)
                    .map(|replies| replies.last().map(|reply| reply.text.clone()))
                    .and_then(|reply| {
                        reply
                            .map(Ok)
                            .unwrap_or_else(|| self.workspace.store.source_text(&entry, true))
                    });
                if output.is_ok_and(|output| {
                    deliver_text(&output, false, DeliveryOptions::default()).is_ok()
                }) {
                    self.publish(identifier, "ready", "Copied");
                } else {
                    self.publish(identifier, "review-error", "Could not copy. Try again.");
                }
            }
            "continue" => (self.callbacks.continue_recording)(identifier),
            "open" => {
                (self.callbacks.open)();
                if let Ok(replies) = self.workspace.store.replies(identifier) {
                    let _ = self
                        .workspace
                        .show_conversation(Some(entry), &replies, false);
                }
                if final_live.as_deref() == Some(identifier) {
                    self.workspace.live_draft_text.grab_focus();
                } else {
                    self.workspace.prompt.grab_focus();
                }
                self.dismiss();
            }
            _ => {}
        }
    }
    pub fn request(self: &Rc<Self>, instruction: &str) {
        if let Some(entry) = self.workspace.entry()
            && self.workspace.save_edits(None)
        {
            self.begin(&entry.identifier, instruction);
        }
    }
    pub fn begin(self: &Rc<Self>, identifier: &str, instruction: &str) {
        if self.shutting_down.get()
            || self.config.borrow().rewrite_provider == "none"
            || self.live().is_some_and(|live| live.finalizing())
        {
            return;
        }
        let current_request = self.request.borrow().clone();
        if let Some(request) = current_request {
            if self.current.borrow().as_deref() == Some(identifier)
                && request.identifier != identifier
            {
                self.publish(
                    identifier,
                    "ready",
                    "Another note is rewriting. Try again shortly.",
                );
            }
            return;
        }
        if (self.callbacks.capture_busy)() {
            return;
        }
        if self.config.borrow().incognito_mode {
            self.workspace
                .set_busy(false, "Rewriting is unavailable in Incognito.");
            return;
        }
        let prepared = (|| {
            let entry = self.workspace.store.history.find(identifier)?;
            let replies = self.workspace.store.replies(identifier)?;
            let source = self.workspace.store.source_text(&entry, false)?;
            let prompt = rewrite_prompt(&entry, &replies, instruction, Some(&source))?;
            let images = self.screenshots.snapshot(identifier, false)?;
            Ok::<_, mluva_core::database::StoreError>((prompt, images))
        })();
        let Ok((prompt, images)) = prepared else {
            self.workspace.set_busy(
                false,
                "This conversation cannot be rewritten. Reopen it or start with shorter text.",
            );
            if self.current.borrow().as_deref() == Some(identifier) {
                self.publish(identifier, "review-error", "Open this note to review it.");
            }
            return;
        };
        let config = self.config.borrow().clone();
        let Ok(client) = RewriteClient::new(&config, None, None) else {
            self.workspace.set_busy(
                false,
                "Rewrite failed. Check the selected provider, then try again. Your text is safe.",
            );
            return;
        };
        let request = Rc::new(Request {
            identifier: identifier.into(),
            instruction: instruction.into(),
            draft: (instruction == self.workspace.prompt_text()).then(|| instruction.into()),
            auto_copy: config.auto_copy_rewrite,
            client: Rc::new(client),
            preview: RefCell::new(String::new()),
            progress: RefCell::new(None),
        });
        *self.request.borrow_mut() = Some(request.clone());
        self.workspace.set_busy(true, "Rewriting…");
        self.workspace.set_rewrite_preview(identifier, "");
        if self
            .workspace
            .entry()
            .is_some_and(|entry| entry.identifier == identifier)
        {
            self.workspace.scroll_to_latest();
        }
        self.publish(identifier, "rewriting", "");
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel::<String>();
        let weak = Rc::downgrade(self);
        let weak_request = Rc::downgrade(&request);
        let progress = self.runtime.spawn(async move {
            while let Some(preview) = receiver.recv().await {
                if let Some(owner) = weak.upgrade()
                    && let Some(request) = weak_request.upgrade()
                {
                    owner.progress(&request, &preview);
                } else {
                    break;
                }
            }
        });
        *request.progress.borrow_mut() = Some(progress);
        let weak = Rc::downgrade(self);
        let cwd = self.cwd.clone();
        self.runtime.spawn(async move {
            let started = Instant::now();
            let mut first = None;
            let mut last = None;
            let mut preview = String::new();
            let mut delta = |text: &str| {
                if text.is_empty() {
                    return;
                }
                preview.push_str(text);
                let now = started.elapsed();
                if first.is_none() {
                    first = Some(now.as_secs_f64());
                }
                if last.is_none_or(|last: std::time::Duration| {
                    now - last >= std::time::Duration::from_millis(50)
                }) {
                    last = Some(now);
                    let _ = sender.send(preview.clone());
                }
            };
            let result = request
                .client
                .transform(&prompt, &cwd, &images, Some(&mut delta))
                .await;
            request.client.close().await;
            if let Some(owner) = weak.upgrade() {
                owner.finished(&request, result, first);
            }
        });
    }
    fn owns(&self, request: &Rc<Request>) -> bool {
        self.request
            .borrow()
            .as_ref()
            .is_some_and(|current| Rc::ptr_eq(current, request))
    }
    fn progress(&self, request: &Rc<Request>, preview: &str) {
        if self.shutting_down.get() || self.config.borrow().incognito_mode || !self.owns(request) {
            return;
        }
        request.preview.replace(preview.into());
        self.workspace
            .set_rewrite_preview(&request.identifier, preview);
        if self.current.borrow().as_deref() == Some(&request.identifier) {
            (self.callbacks.publish)(self.state(
                "rewriting",
                &request.identifier,
                preview.into(),
                "",
            ));
        }
    }
    fn finished(
        self: &Rc<Self>,
        request: &Rc<Request>,
        result: Result<mluva_providers::rewriting::RewriteResult, RewriteError>,
        first: Option<f64>,
    ) {
        if self.shutting_down.get() || !self.owns(request) {
            return;
        }
        self.request.borrow_mut().take();
        if let Some(progress) = request.progress.borrow_mut().take() {
            progress.abort();
        }
        self.workspace.clear_rewrite_preview();
        if self.config.borrow().incognito_mode {
            self.workspace
                .set_busy(false, "Rewrite discarded because Incognito is enabled.");
            return;
        }
        let (result, model, failure) = match result {
            Ok(result) => (result.text, result.model, String::new()),
            Err(RewriteError::UnsupportedRewriteSpeed) => (
                String::new(),
                String::new(),
                RewriteError::UnsupportedRewriteSpeed.to_string(),
            ),
            Err(_) => (
                String::new(),
                String::new(),
                "Rewrite failed. Check the selected provider, then try again. Your text is safe."
                    .into(),
            ),
        };
        if result.is_empty() {
            self.workspace.set_busy(false, &failure);
            if self.current.borrow().as_deref() == Some(&request.identifier) {
                self.publish(
                    &request.identifier,
                    "review-error",
                    "Rewrite failed. Try again or open the note.",
                );
            }
            return;
        }
        let saved = (|| {
            let entry = self.workspace.store.history.find(&request.identifier)?;
            self.workspace.store.append(
                &request.identifier,
                &request.instruction,
                &result,
                &model,
            )?;
            Ok::<HistoryEntry, mluva_core::database::StoreError>(entry)
        })();
        let Ok(entry) = saved else {
            self.workspace.set_busy(
                false,
                "Could not save the rewrite. The conversation may have been deleted.",
            );
            if self.current.borrow().as_deref() == Some(&request.identifier) {
                self.publish(
                    &request.identifier,
                    "review-error",
                    "Could not save the rewrite. Original unchanged.",
                );
            }
            return;
        };
        if self
            .workspace
            .entry()
            .is_some_and(|entry| entry.identifier == request.identifier)
        {
            if request
                .draft
                .as_ref()
                .is_some_and(|draft| *draft == self.workspace.prompt_text())
            {
                self.workspace.prompt.buffer().set_text("");
            }
            if let Ok(replies) = self.workspace.store.replies(&request.identifier) {
                let _ = self
                    .workspace
                    .show_conversation(Some(entry), &replies, false);
            }
        }
        let timing = first
            .map(|seconds| format!(" · first text {seconds:.1} s"))
            .unwrap_or_default();
        let copied =
            request.auto_copy && deliver_text(&result, false, DeliveryOptions::default()).is_ok();
        if copied || !request.auto_copy {
            self.workspace.set_busy(
                false,
                &format!(
                    "Rewrite {}{timing}.",
                    if copied { "copied" } else { "saved" }
                ),
            );
        } else {
            self.workspace.set_busy(
                false,
                &format!("Rewrite saved{timing}. Automatic copy failed; use the Copy icon."),
            );
        }
        let _ = self.workspace.refresh_history();
        if self.current.borrow().as_deref() == Some(&request.identifier) {
            self.publish(&request.identifier, "ready", "");
        }
    }
    pub fn cancel(self: &Rc<Self>) {
        if let Some(live) = self.live().filter(|live| live.finalizing()) {
            let identifier = live.final_entry().expect("final Live owner");
            live.cancel();
            self.workspace.finish_live();
            self.workspace
                .set_busy(false, "Live rewrite cancelled. Your original is safe.");
            self.publish(
                &identifier,
                "ready",
                "Live rewrite cancelled. Original kept.",
            );
            return;
        }
        let request = self.request.borrow_mut().take();
        if let Some(request) = request {
            if let Some(progress) = request.progress.borrow_mut().take() {
                progress.abort();
            }
            request.client.cancel();
            self.workspace.clear_rewrite_preview();
            self.workspace
                .set_busy(false, "Rewrite cancelled. Your original is safe.");
            if self.current.borrow().as_deref() == Some(&request.identifier) {
                self.publish(&request.identifier, "ready", "");
            }
        }
    }
    pub fn shutdown(&self) {
        self.shutting_down.set(true);
        if let Some(request) = self.request.borrow_mut().take() {
            if let Some(progress) = request.progress.borrow_mut().take() {
                progress.abort();
            }
            request.client.cancel();
        }
        self.workspace.clear_rewrite_preview();
        self.dismiss();
    }
}
impl Drop for ReviewController {
    fn drop(&mut self) {
        self.shutdown();
    }
}
