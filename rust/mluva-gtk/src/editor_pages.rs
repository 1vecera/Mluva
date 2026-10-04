//! Shared prompt/style discovery without changing active request snapshots or document drafts.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use adw::prelude::*;
use mluva_core::database::StoreResult;
use mluva_core::history::HistoryStore;
use mluva_core::personalization::PersonalizationStore;
use mluva_core::prompt_catalog::DEFAULTS;
use mluva_core::prompts::PromptStore;

use crate::personalization::PersonalizationPage;
use crate::prompt_editor::{Changed, Message, OpenEditor, PromptSession, PromptsPage, Writable};

pub struct EditorPages {
    pub prompts: Rc<PromptsPage>,
    pub personalization: Rc<PersonalizationPage>,
    pub prompt_store: Rc<RefCell<PromptStore>>,
    pub personalization_store: Rc<RefCell<PersonalizationStore>>,
    session: Rc<PromptSession>,
    parent: glib::WeakRef<gtk::Widget>,
    changed: Changed,
    message: Message,
}

impl EditorPages {
    pub fn new(
        parent: &impl IsA<gtk::Widget>,
        history: HistoryStore,
        personalization: Rc<RefCell<PersonalizationStore>>,
        prompts: Rc<RefCell<PromptStore>>,
        writable: Writable,
        changed: Changed,
        message: Message,
    ) -> StoreResult<Rc<Self>> {
        let styles = DEFAULTS
            .styles
            .iter()
            .chain(&personalization.borrow().state().custom_styles)
            .cloned()
            .collect::<Vec<_>>();
        prompts.borrow_mut().sync_styles(&styles)?;
        personalization.borrow_mut().prompt_store = Some(prompts.borrow().clone());
        let owner = Rc::new(RefCell::new(Weak::<Self>::new()));
        let target = owner.clone();
        let prompt_changed: Changed = Rc::new(move || {
            if let Some(pages) = target.borrow().upgrade() {
                if let Err(error) = pages.refresh_catalog() {
                    (pages.message)(&error.to_string());
                    return;
                }
                (pages.changed)();
                (pages.message)("Prompt saved · next request, or next recording for Live");
            }
        });
        let session =
            PromptSession::new(prompts.clone(), writable, prompt_changed, message.clone());
        let target = owner.clone();
        let open_editor: OpenEditor = Rc::new(move |identifier| {
            if let Some(pages) = target.borrow().upgrade() {
                pages.open_prompt(identifier);
            }
        });
        let prompt_page = PromptsPage::new(prompts.clone(), open_editor.clone())?;
        let target = owner.clone();
        let styles_changed: Changed = Rc::new(move || {
            if let Some(pages) = target.borrow().upgrade() {
                if let Err(error) = pages.sync_styles() {
                    (pages.message)(&error.to_string());
                    return;
                }
                (pages.changed)();
            }
        });
        let personalization_page = PersonalizationPage::new(
            personalization.clone(),
            history,
            message.clone(),
            styles_changed,
            Some(open_editor),
        );
        let pages = Rc::new(Self {
            prompts: prompt_page,
            personalization: personalization_page,
            prompt_store: prompts,
            personalization_store: personalization,
            session,
            parent: parent.as_ref().downgrade(),
            changed,
            message,
        });
        owner.replace(Rc::downgrade(&pages));
        Ok(pages)
    }

    pub fn open_prompt(&self, identifier: &str) {
        if let Some(parent) = self.parent.upgrade() {
            self.session.open(&parent, identifier);
        }
    }

    pub fn refresh_catalog(&self) -> StoreResult<()> {
        self.sync_styles()?;
        self.prompts.refresh()?;
        self.personalization.refresh();
        Ok(())
    }

    fn sync_styles(&self) -> StoreResult<()> {
        // Saved instructions remain the recovery baseline even when a prompt file overrides them.
        let styles = DEFAULTS
            .styles
            .iter()
            .chain(&self.personalization_store.borrow().state().custom_styles)
            .cloned()
            .collect::<Vec<_>>();
        let current = {
            let mut prompts = self.prompt_store.borrow_mut();
            prompts.sync_styles(&styles)?;
            prompts.clone()
        };
        self.personalization_store.borrow_mut().prompt_store = Some(current);
        Ok(())
    }
}
