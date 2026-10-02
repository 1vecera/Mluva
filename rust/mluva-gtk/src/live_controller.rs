//! Whole live drafts, revision guards and durable final reconciliation on GLib.
use crate::{async_runtime::DesktopRuntime, conversation_view::ConversationWorkspace};
use gtk::prelude::*;
use mluva_core::{
    config::AppConfig,
    delivery::{DeliveryOptions, deliver_text},
    live::{LiveRewriteSchedule, initial_draft, live_prompt},
    prompt_catalog::DEFAULTS,
    prompts::PromptStore,
    screenshots::ImageInput,
    text,
};
use mluva_providers::rewriting::RewriteClient;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    path::PathBuf,
    rc::Rc,
    time::Instant,
};

pub type LiveImages = Rc<dyn Fn(Option<&str>, Option<&str>) -> Result<Vec<ImageInput>, String>>;
pub type LiveReview = Rc<dyn Fn(&str, &str, &str)>;
pub struct LiveCallbacks {
    pub images: LiveImages,
    pub review: LiveReview,
}
struct Session {
    identifier: String,
    config: AppConfig,
    schedule: LiveRewriteSchedule,
    continuation: Option<String>,
    continuation_source: String,
    final_entry: Option<String>,
    final_text: String,
    last_model: String,
    client: Option<Rc<RewriteClient>>,
}

pub struct LiveController {
    pub workspace: Rc<ConversationWorkspace>,
    runtime: Rc<DesktopRuntime>,
    prompts: PromptStore,
    cwd: PathBuf,
    callbacks: LiveCallbacks,
    config: RefCell<AppConfig>,
    frozen_prompts: RefCell<Option<(String, BTreeMap<String, String>)>>,
    session: RefCell<Option<Session>>,
    revision: Cell<u64>,
    updating: Cell<bool>,
    shutting_down: Cell<bool>,
    once_active: Cell<bool>,
    origin: Instant,
    changed: RefCell<Option<glib::SignalHandlerId>>,
}
impl LiveController {
    pub fn attach(
        workspace: Rc<ConversationWorkspace>,
        runtime: Rc<DesktopRuntime>,
        config: AppConfig,
        prompts: PromptStore,
        cwd: PathBuf,
        callbacks: LiveCallbacks,
    ) -> Rc<Self> {
        let owner = Rc::new(Self {
            workspace,
            runtime,
            prompts,
            cwd,
            callbacks,
            config: RefCell::new(config),
            frozen_prompts: RefCell::new(None),
            session: RefCell::new(None),
            revision: Cell::new(0),
            updating: Cell::new(false),
            shutting_down: Cell::new(false),
            once_active: Cell::new(false),
            origin: Instant::now(),
            changed: RefCell::new(None),
        });
        let weak = Rc::downgrade(&owner);
        let signal = owner
            .workspace
            .live_draft_text
            .buffer()
            .connect_changed(move |_| {
                if let Some(owner) = weak.upgrade()
                    && !owner.updating.get()
                {
                    owner.revision.set(owner.revision.get() + 1);
                }
            });
        *owner.changed.borrow_mut() = Some(signal);
        owner
    }
    pub fn set_config(&self, config: AppConfig) {
        *self.config.borrow_mut() = config;
    }
    pub fn active(&self) -> bool {
        self.session.borrow().is_some()
    }
    pub fn finalizing(&self) -> bool {
        self.session
            .borrow()
            .as_ref()
            .is_some_and(|session| session.final_entry.is_some())
    }
    pub fn final_entry(&self) -> Option<String> {
        self.session
            .borrow()
            .as_ref()
            .and_then(|session| session.final_entry.clone())
    }
    pub fn owns(&self, identifier: &str) -> bool {
        self.session
            .borrow()
            .as_ref()
            .is_some_and(|session| session.identifier == identifier)
    }
    /// The application persists this updated preference and reconfigures services.
    /// The current final rewrite still uses the session's frozen configuration.
    pub fn consume_once(&self) -> Option<AppConfig> {
        let mut config = self.config.borrow_mut();
        if self.once_active.replace(false)
            && config.live_rewrite_enabled
            && !config.live_rewrite_continuous
        {
            config.live_rewrite_enabled = false;
            Some(config.clone())
        } else {
            None
        }
    }
    pub fn begin(
        &self,
        identifier: &str,
        mode: &str,
        incognito: bool,
        continuation: Option<&str>,
        preserve_draft: bool,
    ) -> Result<(), String> {
        let config = self.config.borrow().clone();
        if self
            .frozen_prompts
            .borrow()
            .as_ref()
            .is_none_or(|(session, _)| session != identifier)
        {
            *self.frozen_prompts.borrow_mut() = Some((
                identifier.into(),
                self.prompts.snapshot().map_err(|error| error.to_string())?,
            ));
        }
        let mut draft = if preserve_draft {
            self.workspace.live_draft()
        } else {
            initial_draft(
                &config,
                self.frozen_prompts
                    .borrow()
                    .as_ref()
                    .map(|(_, prompts)| prompts),
            )?
        };
        let mut continuation_source = String::new();
        if let Some(identifier) = continuation {
            let entry = self
                .workspace
                .store
                .history
                .find(identifier)
                .map_err(|error| error.to_string())?;
            continuation_source = self
                .workspace
                .store
                .source_text(&entry, false)
                .map_err(|error| error.to_string())?;
            if !preserve_draft {
                draft = self
                    .workspace
                    .store
                    .replies(identifier)
                    .map_err(|error| error.to_string())?
                    .last()
                    .map(|reply| reply.text.clone())
                    .unwrap_or_else(|| continuation_source.clone());
            }
        }
        self.cancel();
        let enabled = config.live_rewrite_enabled && !incognito && mode == "dictation";
        self.once_active
            .set(enabled && !config.live_rewrite_continuous);
        self.updating.set(true);
        self.workspace
            .show_live_draft(&draft, "Waiting for speech…");
        self.updating.set(false);
        self.workspace.set_live_draft_available(enabled);
        if enabled {
            *self.session.borrow_mut() = Some(Session {
                identifier: identifier.into(),
                schedule: LiveRewriteSchedule::new(
                    config.live_rewrite_min_characters,
                    config.live_rewrite_interval_seconds as f64,
                ),
                config,
                continuation: continuation.map(Into::into),
                continuation_source,
                final_entry: None,
                final_text: String::new(),
                last_model: String::new(),
                client: None,
            });
            self.revision.set(0);
            self.workspace.live_draft_follower.follow(true);
        }
        Ok(())
    }
    pub fn offer(self: &Rc<Self>, source: &str, final_snapshot: bool) {
        if self.shutting_down.get() || self.config.borrow().incognito_mode {
            return;
        }
        let mut slot = self.session.borrow_mut();
        let Some(session) = slot.as_mut() else {
            return;
        };
        let source = if !final_snapshot
            && !session.continuation_source.is_empty()
            && !text::trim(source).is_empty()
        {
            format!(
                "{}\n\n{source}",
                session
                    .continuation_source
                    .trim_end_matches(text::whitespace)
            )
        } else {
            source.into()
        };
        if session.schedule.paused {
            drop(slot);
            if final_snapshot {
                self.save(true);
            }
            return;
        }
        if final_snapshot
            && !session.schedule.in_flight
            && ((session.schedule.last_final && text::trim(&source) == session.schedule.last_text)
                || session.schedule.failed)
        {
            let stale = session.schedule.failed;
            drop(slot);
            self.save(stale);
            return;
        }
        let Some(snapshot) =
            session
                .schedule
                .take(&source, self.origin.elapsed().as_secs_f64(), final_snapshot)
        else {
            return;
        };
        let prepared = (|| {
            let prompt = live_prompt(
                &session.config,
                &snapshot,
                &self.workspace.live_draft(),
                final_snapshot,
                self.frozen_prompts
                    .borrow()
                    .as_ref()
                    .map(|(_, prompts)| prompts),
            )?;
            let images = (self.callbacks.images)(
                session
                    .final_entry
                    .is_none()
                    .then_some(session.identifier.as_str()),
                session
                    .final_entry
                    .as_deref()
                    .or(session.continuation.as_deref()),
            )?;
            let client = Rc::new(
                RewriteClient::new(&session.config, None, None)
                    .map_err(|error| error.to_string())?,
            );
            Ok::<_, String>((prompt, images, client))
        })();
        let (prompt, images, client) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                session.schedule.finish(false);
                drop(slot);
                self.workspace.live_draft_status.set_label(&error);
                if final_snapshot {
                    self.save(true);
                }
                return;
            }
        };
        let identifier = session.identifier.clone();
        session.client = Some(client.clone());
        drop(slot);
        self.workspace
            .live_draft_status
            .set_label(if final_snapshot {
                "Reconciling the final transcript…"
            } else {
                "Updating live draft · provisional recognition…"
            });
        let revision = self.revision.get();
        let cwd = self.cwd.clone();
        let weak = Rc::downgrade(self);
        self.runtime.spawn(async move {
            let outcome = client.transform(&prompt, &cwd, &images, None).await;
            client.close().await;
            if let Some(owner) = weak.upgrade() {
                let (result, model) = outcome
                    .map(|outcome| (outcome.text, outcome.model))
                    .unwrap_or_default();
                owner.finished(&identifier, &client, revision, &result, &model);
            }
        });
    }
    fn finished(
        self: &Rc<Self>,
        identifier: &str,
        client: &Rc<RewriteClient>,
        revision: u64,
        result: &str,
        model: &str,
    ) {
        if self.shutting_down.get() || self.config.borrow().incognito_mode {
            return;
        }
        let mut slot = self.session.borrow_mut();
        let Some(session) = slot.as_mut().filter(|session| {
            session.identifier == identifier
                && session
                    .client
                    .as_ref()
                    .is_some_and(|current| Rc::ptr_eq(current, client))
        }) else {
            return;
        };
        session.client = None;
        session.schedule.finish(!result.is_empty());
        let final_snapshot = session.schedule.last_final;
        let publish = !result.is_empty() && revision == self.revision.get();
        if publish {
            session.last_model = model.into();
        } else if !result.is_empty() {
            session.schedule.last_text.clear();
        }
        let finalize = session
            .final_entry
            .as_ref()
            .map(|_| session.final_text.clone());
        drop(slot);
        if publish {
            self.updating.set(true);
            self.workspace.show_live_draft(
                result,
                if final_snapshot {
                    "Live draft · final transcript reconciled"
                } else {
                    "Live draft · provisional until Stop"
                },
            );
            self.updating.set(false);
        } else {
            self.workspace
                .live_draft_status
                .set_label(if result.is_empty() {
                    "Live rewrite paused · provider failed; your draft is kept"
                } else {
                    "Your edit kept · awaiting next update"
                });
        }
        if let Some(source) = finalize {
            self.offer(&source, true);
        }
    }
    /// Completion supplies canonical full recognition, including continued source.
    pub fn finish_capture(self: &Rc<Self>, identifier: &str, source: &str) {
        let mut slot = self.session.borrow_mut();
        let Some(session) = slot.as_mut() else {
            return;
        };
        session.final_entry = Some(identifier.into());
        session.final_text = source.into();
        drop(slot);
        self.workspace.set_busy(true, "Finishing live draft…");
        self.offer(source, true);
    }
    fn save(&self, stale: bool) {
        let mut slot = self.session.borrow_mut();
        let Some(session) = slot.as_ref() else {
            return;
        };
        let Some(identifier) = session.final_entry.clone() else {
            return;
        };
        let draft = self.workspace.live_draft();
        let paused = session.schedule.paused;
        let mut instruction = format!(
            "Live rewrite · {}",
            DEFAULTS
                .live_templates
                .iter()
                .find(|template| template.identifier == session.config.live_rewrite_template)
                .expect("validated template")
                .name
        );
        if paused {
            instruction.push_str(" (paused; final transcript not reconciled)");
        } else if stale {
            instruction.push_str(" (partial draft; final update failed)");
        }
        let saved = (|| {
            let entry = self.workspace.store.history.find(&identifier)?;
            if !text::trim(&draft).is_empty() {
                self.workspace.store.append(
                    &identifier,
                    &instruction,
                    &draft,
                    if session.last_model.is_empty() {
                        "live-draft"
                    } else {
                        &session.last_model
                    },
                )?;
            }
            let viewing = self.workspace.is_viewing_live();
            self.workspace.finish_live();
            if viewing
                || self
                    .workspace
                    .entry()
                    .is_some_and(|entry| entry.identifier == identifier)
            {
                self.workspace.show_conversation(
                    Some(entry),
                    &self.workspace.store.replies(&identifier)?,
                    false,
                )?;
            }
            self.workspace.set_busy(
                false,
                if paused {
                    "Paused draft saved. Review it against the final transcript."
                } else if stale {
                    "Live draft saved. Final update failed; review the remaining gaps."
                } else if session.config.auto_copy_rewrite {
                    "Live draft copied."
                } else {
                    "Live draft saved."
                },
            );
            self.workspace.refresh_history()?;
            Ok::<(), mluva_core::database::StoreError>(())
        })();
        let mut phase = "ready";
        let mut message = if paused {
            "Paused draft · not reconciled."
        } else if stale {
            "Partial draft · final update failed."
        } else {
            ""
        };
        if saved.is_err() {
            phase = "review-error";
            message = "Could not save the live draft. Open Mluva to copy it.";
            self.workspace.finish_live();
            let _ = self.workspace.show_transient(&session.final_text, &draft);
            self.workspace.set_busy(
                false,
                "Could not save the live draft. Your original and draft remain available to copy.",
            );
        } else if !text::trim(&draft).is_empty()
            && session.config.auto_copy_rewrite
            && !stale
            && !paused
            && deliver_text(&draft, false, DeliveryOptions::default()).is_err()
        {
            message = "Automatic copy failed. Use Copy.";
            self.workspace.set_busy(
                false,
                "Live draft saved. Automatic copy failed; use the Copy icon.",
            );
        }
        *slot = None;
        drop(slot);
        (self.callbacks.review)(&identifier, phase, message);
    }
    pub fn pause(&self) {
        if let Some(session) = self.session.borrow_mut().as_mut() {
            if let Some(client) = session.client.take() {
                client.cancel();
            }
            session.schedule.paused = true;
            session.schedule.in_flight = false;
            self.workspace
                .live_draft_status
                .set_label("Live rewrite paused · draft kept");
        }
    }
    pub fn cancel(&self) {
        if let Some(session) = self.session.borrow_mut().take()
            && let Some(client) = session.client
        {
            client.cancel();
        }
        self.workspace.live_draft_box.set_visible(false);
        self.workspace.set_busy(false, "");
    }
    pub fn shutdown(&self) {
        self.shutting_down.set(true);
        self.cancel();
    }
}
impl Drop for LiveController {
    fn drop(&mut self) {
        self.shutting_down.set(true);
        if let Some(signal) = self.changed.get_mut().take() {
            self.workspace.live_draft_text.buffer().disconnect(signal);
        }
        if let Some(session) = self.session.get_mut().take()
            && let Some(client) = session.client
        {
            client.cancel();
        }
    }
}
