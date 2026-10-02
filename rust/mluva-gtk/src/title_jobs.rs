//! Bounded title work; every durable write stays on the application owner.

use crate::async_runtime::DesktopRuntime;
use mluva_core::{
    config::AppConfig,
    history::{HistoryEntry, HistoryStore},
    text,
    titles::{clean_title, fallback_title, title_prompt},
};
use mluva_providers::rewriting::RewriteClient;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    path::PathBuf,
    rc::Rc,
    time::Duration,
};

pub const MAX_PENDING_TITLES: usize = 20;

pub struct ConversationTitleJobs {
    history: HistoryStore,
    cwd: PathBuf,
    runtime: Rc<DesktopRuntime>,
    config: Rc<dyn Fn() -> AppConfig>,
    instructions: Rc<dyn Fn() -> Result<String, String>>,
    changed: Rc<dyn Fn(&str)>,
    pending: RefCell<VecDeque<(String, String)>>,
    client: RefCell<Option<Rc<RewriteClient>>>,
    closed: Cell<bool>,
}
impl ConversationTitleJobs {
    pub fn new(
        history: HistoryStore,
        cwd: PathBuf,
        runtime: Rc<DesktopRuntime>,
        config: Rc<dyn Fn() -> AppConfig>,
        instructions: Rc<dyn Fn() -> Result<String, String>>,
        changed: Rc<dyn Fn(&str)>,
    ) -> Rc<Self> {
        Rc::new(Self {
            history,
            cwd,
            runtime,
            config,
            instructions,
            changed,
            pending: RefCell::new(VecDeque::new()),
            client: RefCell::new(None),
            closed: Cell::new(false),
        })
    }

    pub fn active(&self) -> bool {
        self.client.borrow().is_some()
    }
    pub fn pending(&self) -> usize {
        self.pending.borrow().len()
    }

    pub fn enqueue(self: &Rc<Self>, entry: &HistoryEntry) {
        let config = (self.config)();
        if self.closed.get()
            || config.incognito_mode
            || entry.title.as_ref().is_some_and(|title| !title.is_empty())
            || text::trim(&entry.raw_text).is_empty()
        {
            return;
        }
        let fallback = fallback_title(&entry.raw_text);
        if !self
            .history
            .save_generated_title(&entry.identifier, &fallback, None)
            .unwrap_or(false)
        {
            return;
        }
        (self.changed)(&entry.identifier);
        if config.rewrite_provider != "none"
            && config.automatic_titles
            && self.pending() < MAX_PENDING_TITLES
        {
            self.pending
                .borrow_mut()
                .push_back((entry.identifier.clone(), fallback));
            self.start_next();
        }
    }

    fn start_next(self: &Rc<Self>) {
        let config = (self.config)();
        if self.active()
            || self.closed.get()
            || config.incognito_mode
            || !config.automatic_titles
            || config.rewrite_provider == "none"
        {
            return;
        }
        loop {
            let Some((identifier, fallback)) = self.pending.borrow_mut().pop_front() else {
                return;
            };
            let Ok(entry) = self.history.find(&identifier) else {
                continue;
            };
            if entry.title.as_deref() != Some(&fallback) {
                continue;
            }
            let Ok(instructions) = (self.instructions)() else {
                continue;
            };
            let prompt = title_prompt(&entry, Some(&instructions));
            let Ok(client) = RewriteClient::new(
                &config,
                Some(Duration::from_secs(10)),
                Some(Duration::from_secs(20)),
            ) else {
                continue;
            };
            let client = Rc::new(client);
            *self.client.borrow_mut() = Some(client.clone());
            let model = if config.rewrite_provider == "litellm" {
                config.litellm_model
            } else {
                config.codex_model
            };
            let cwd = self.cwd.clone();
            let weak = Rc::downgrade(self);
            self.runtime.spawn(async move {
                let title = match client.resolve_model(model.as_deref()).await {
                    Ok(model) => client
                        .transform_title(&prompt, &cwd, &model)
                        .await
                        .ok()
                        .and_then(|title| clean_title(&title)),
                    Err(_) => None,
                };
                client.close().await;
                if let Some(owner) = weak.upgrade() {
                    owner.finished(&client, &identifier, &fallback, title.as_deref());
                }
            });
            return;
        }
    }

    fn finished(
        self: &Rc<Self>,
        client: &Rc<RewriteClient>,
        identifier: &str,
        fallback: &str,
        title: Option<&str>,
    ) {
        if !self
            .client
            .borrow()
            .as_ref()
            .is_some_and(|current| Rc::ptr_eq(current, client))
        {
            return;
        }
        self.client.borrow_mut().take();
        let config = (self.config)();
        if self.closed.get()
            || config.incognito_mode
            || !config.automatic_titles
            || config.rewrite_provider == "none"
        {
            return;
        }
        if let Some(title) = title
            && self
                .history
                .save_generated_title(identifier, title, Some(fallback))
                .unwrap_or(false)
        {
            (self.changed)(identifier);
        }
        self.start_next();
    }

    pub fn cancel(&self) {
        self.pending.borrow_mut().clear();
        if let Some(client) = self.client.borrow_mut().take() {
            client.cancel();
        }
    }
    pub fn close(&self) {
        self.closed.set(true);
        self.cancel();
    }
}
impl Drop for ConversationTitleJobs {
    fn drop(&mut self) {
        self.close();
    }
}
