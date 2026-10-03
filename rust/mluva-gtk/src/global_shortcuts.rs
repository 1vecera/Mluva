//! Own the compositor's immutable shortcut sessions on a private connection.
//!
//! Portal approval is asynchronous and may outlive a settings change. Serialize
//! replacements, retain the key each session requested, and acknowledge cleanup
//! before releasing this owner. No input is synthesized here.
use crate::async_runtime::DesktopRuntime;
use glib::variant::ToVariant;
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    fmt,
    future::Future,
    rc::Rc,
};
use tokio::{sync::Mutex, sync::oneshot};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

mod session;

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const SHORTCUTS: &str = "org.freedesktop.portal.GlobalShortcuts";
const REQUEST: &str = "org.freedesktop.portal.Request";
const SESSION: &str = "org.freedesktop.portal.Session";
const REQUEST_PATH: &str = "/org/freedesktop/portal/desktop/request";
const SESSION_PATH: &str = "/org/freedesktop/portal/desktop/session";
const CANCEL: &str = "cancel-capture";
const REWRITE: &str = "open-rewrite";
const CLOSED: &str = "The global shortcut request was closed.";
type Properties = BTreeMap<String, glib::Variant>;
type Result<T> = std::result::Result<T, PortalError>;
type BindingCallback = Rc<dyn Fn(&str, Option<&str>)>;
type RewriteBindingCallback = Rc<dyn Fn(Option<&str>)>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortalError(String);
impl fmt::Display for PortalError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(output)
    }
}
impl std::error::Error for PortalError {}
impl From<&str> for PortalError {
    fn from(value: &str) -> Self {
        Self(value.into())
    }
}

/// Every action is supplied by the application, including latest-conversation
/// navigation. The selected key accompanies recording-binding notifications so
/// a delayed approval cannot overwrite the current settings' status.
#[derive(Clone)]
pub struct ShortcutCallbacks {
    pub toggle_recording: Rc<dyn Fn()>,
    pub cancel: Rc<dyn Fn()>,
    pub open_rewrite: Rc<dyn Fn()>,
    pub binding_changed: BindingCallback,
    pub rewrite_binding_changed: RewriteBindingCallback,
    pub error: Rc<dyn Fn(&str)>,
}

pub fn recording_shortcut_id(key: &str) -> Result<String> {
    if !(1..=24).any(|number| key == format!("F{number}")) {
        return Err("function_key must be one of F1 through F24".into());
    }
    Ok(format!("toggle-recording-{}", key.to_lowercase()))
}

pub struct GlobalShortcutService {
    runtime: Rc<DesktopRuntime>,
    callbacks: ShortcutCallbacks,
    preferred_key: RefCell<String>,
    bound_key: RefCell<Option<String>>,
    session: RefCell<Option<Rc<session::PortalSession>>>,
    replacing: Mutex<()>,
    tasks: TaskTracker,
    started: Cell<bool>,
    closing: Cell<bool>,
}
impl GlobalShortcutService {
    pub fn new(
        runtime: Rc<DesktopRuntime>,
        recording_key: &str,
        callbacks: ShortcutCallbacks,
    ) -> Result<Rc<Self>> {
        recording_shortcut_id(recording_key)?;
        Ok(Rc::new(Self {
            runtime,
            callbacks,
            preferred_key: RefCell::new(recording_key.into()),
            bound_key: RefCell::new(None),
            session: RefCell::new(None),
            replacing: Mutex::new(()),
            tasks: TaskTracker::new(),
            started: Cell::new(false),
            closing: Cell::new(false),
        }))
    }

    pub fn start(self: &Rc<Self>) {
        if self.closing.get() || self.started.replace(true) {
            return;
        }
        self.schedule(self.preferred_key.borrow().clone());
    }

    pub fn set_recording_key(self: &Rc<Self>, key: &str) -> Result<()> {
        recording_shortcut_id(key)?;
        if self.closing.get() || *self.preferred_key.borrow() == key {
            return Ok(());
        }
        *self.preferred_key.borrow_mut() = key.into();
        if self.started.get() {
            self.schedule(key.into());
        }
        Ok(())
    }

    fn schedule(self: &Rc<Self>, key: String) {
        let owner = self.clone();
        self.runtime.spawn(self.tasks.track_future(async move {
            owner.replace_session(&key).await;
        }));
    }

    async fn replace_session(self: &Rc<Self>, key: &str) {
        let _serial = self.replacing.lock().await;
        if self.closing.get()
            || *self.preferred_key.borrow() != key
            || self.bound_key.borrow().as_deref() == Some(key)
        {
            return;
        }
        let previous = self.session.borrow_mut().take();
        if let Some(previous) = previous {
            previous.close().await;
        }
        self.bound_key.borrow_mut().take();
        if self.closing.get() {
            return;
        }
        let session = session::PortalSession::new(key, self.callbacks.clone());
        *self.session.borrow_mut() = Some(session.clone());
        match session.connect().await {
            Ok(bound) => {
                *self.bound_key.borrow_mut() = Some(key.into());
                if !self.closing.get() && *self.preferred_key.borrow() == key {
                    session.report_bindings(&bound);
                }
            }
            Err(error) => {
                session.close().await;
                if !self.closing.get() {
                    (self.callbacks.error)(&error.0);
                }
                let mut current = self.session.borrow_mut();
                if current
                    .as_ref()
                    .is_some_and(|value| Rc::ptr_eq(value, &session))
                {
                    current.take();
                }
                drop(current);
                if !self.closing.get() && *self.preferred_key.borrow() == key {
                    (self.callbacks.binding_changed)(key, None);
                }
            }
        }
    }

    /// Stop accepting events immediately, close any outstanding approval and
    /// compositor session, then wait for all queued replacements to return.
    pub fn shutdown(self: &Rc<Self>) -> impl Future<Output = ()> + use<> {
        self.closing.set(true);
        let session = self.session.borrow().clone();
        if let Some(session) = &session {
            session.cancel.cancel();
        }
        let owner = self.clone();
        async move {
            if let Some(session) = session {
                session.close().await;
            }
            owner.tasks.close();
            owner.tasks.wait().await;
            owner.session.borrow_mut().take();
            owner.bound_key.borrow_mut().take();
        }
    }
}

#[derive(Debug)]
struct BoundShortcut {
    id: String,
    trigger: String,
}
fn bound_shortcuts(value: &glib::Variant) -> Result<Vec<BoundShortcut>> {
    let signature = value.type_().as_str();
    if !signature.starts_with('a') || signature.starts_with("a{") || signature == "ay" {
        return Err("The portal returned an invalid shortcut list.".into());
    }
    let mut shortcuts = vec![];
    for entry in value.iter() {
        if !entry.is_container() || entry.n_children() != 2 {
            return Err("The portal returned an invalid shortcut entry.".into());
        }
        let identifier = entry.child_value(0);
        let Some(id) = identifier.str() else {
            return Err("The portal returned an invalid shortcut entry.".into());
        };
        let properties = entry
            .child_value(1)
            .get::<Properties>()
            .ok_or_else(|| PortalError::from("The portal returned an invalid shortcut entry."))?;
        for key in ["description", "trigger_description"] {
            if properties
                .get(key)
                .is_some_and(|value| value.str().is_none())
            {
                return Err(PortalError(format!(
                    "The portal returned an invalid {key} property."
                )));
            }
        }
        shortcuts.push(BoundShortcut {
            id: id.into(),
            trigger: properties
                .get("trigger_description")
                .and_then(|value| value.str())
                .unwrap_or("")
                .into(),
        });
    }
    Ok(shortcuts)
}
