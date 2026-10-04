//! Exact external text targets. Only Command capture reads an explicit selection.
//!
//! libatspi proxies and listeners stay on the GLib thread that first initializes them.
//! Dispatch desktop capture/restoration there; send content-free receipts to workers.

use crate::atspi::{self, EditableText, FocusListener, Node, Text};
use caseless::Caseless;
use glib::variant::ToVariant;
use mluva_core::terminal_target::{
    TerminalTargetSnapshot, capture_hyprland_terminal_target, hyprland_terminal_tracking_available,
};
use std::{cell::RefCell, fmt, fs, path::Path, rc::Rc};

pub const MAX_SELECTED_TEXT_CHARACTERS: i64 = 2_000;
const MAX_ACCESSIBLE_NODES: usize = 20_000;

/// Read the live status without auto-starting a disabled accessibility service.
pub fn system_accessibility_enabled() -> bool {
    let query = || -> Option<bool> {
        let connection = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).ok()?;
        let result = connection
            .call_sync(
                Some("org.a11y.Bus"),
                "/org/a11y/bus",
                "org.freedesktop.DBus.Properties",
                "Get",
                Some(&("org.a11y.Status", "IsEnabled").to_variant()),
                Some(glib::VariantTy::new("(v)").ok()?),
                gio::DBusCallFlags::NO_AUTO_START,
                1_000,
                gio::Cancellable::NONE,
            )
            .ok()?;
        result.child_value(0).as_variant()?.get::<bool>()
    };
    query().unwrap_or(false)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackerError {
    AccessibilityDisabled,
    RegistrationRejected,
    WrongThread,
}
impl fmt::Display for TrackerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::AccessibilityDisabled => "Desktop accessibility is disabled",
            Self::RegistrationRejected => "AT-SPI rejected global focus-event registration",
            Self::WrongThread => "AT-SPI must stay on its GLib thread",
        })
    }
}
impl std::error::Error for TrackerError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextSelectionTooLargeError {
    pub characters: usize,
    pub maximum: i64,
}
impl fmt::Display for TextSelectionTooLargeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn grouped(value: impl fmt::Display) -> String {
            let digits = value.to_string();
            let mut result = String::new();
            for (index, digit) in digits.chars().enumerate() {
                if digit.is_ascii_digit()
                    && index != 0
                    && (digits.len() - index).is_multiple_of(3)
                    && !result.ends_with('-')
                {
                    result.push(',');
                }
                result.push(digit);
            }
            result
        }
        write!(
            formatter,
            "The selected text is {} characters; Command mode allows at most {}. Shorten the selection before recording.",
            grouped(self.characters),
            grouped(self.maximum)
        )
    }
}
impl std::error::Error for TextSelectionTooLargeError {}

#[derive(Default)]
struct Focus {
    node: Option<Node>,
    event_received: bool,
}

/// Register before initial discovery: a focus event during a query remains authoritative.
pub struct FocusedTextTargetTracker {
    runtime: bool,
    own_process_id: u32,
    focus: Rc<RefCell<Focus>>,
    listener: Option<FocusListener>,
}

impl FocusedTextTargetTracker {
    pub fn new() -> Result<Self, TrackerError> {
        let runtime = system_accessibility_enabled();
        if !runtime && !hyprland_terminal_tracking_available(None) {
            return Err(TrackerError::AccessibilityDisabled);
        }
        let own_process_id = std::process::id();
        let focus = Rc::new(RefCell::new(Focus::default()));
        let listener = if runtime {
            if !atspi::initialize() {
                return Err(TrackerError::WrongThread);
            }
            let state = focus.clone();
            let listener = FocusListener::new(Rc::new(move |source, detail| {
                // libatspi may dispatch nested events while querying the owner. Never hold a
                // RefCell borrow across such a call, including in capture/restore/discovery.
                let (source, process_id) = match source {
                    Some(source) => match source.process_id() {
                        Some(pid) => (Some(source), pid),
                        None => (None, own_process_id),
                    },
                    None => (None, own_process_id),
                };
                let mut focus = state.borrow_mut();
                focus.event_received = true;
                if detail == 0 {
                    if source.is_none() || source == focus.node {
                        focus.node = None;
                    }
                } else {
                    focus.node = (process_id != own_process_id).then_some(source).flatten();
                }
            }))
            .ok_or(TrackerError::RegistrationRejected)?;
            let initial = Node::desktop().and_then(|root| find_focused(root, own_process_id, true));
            let mut state = focus.borrow_mut();
            if !state.event_received {
                state.node = initial;
            }
            Some(listener)
        } else {
            None
        };
        Ok(Self {
            runtime,
            own_process_id,
            focus,
            listener,
        })
    }

    pub fn capture_delivery_target(&self) -> Option<DeliveryTargetSnapshot> {
        capture_hyprland_terminal_target()
            .map(DeliveryTargetSnapshot::Terminal)
            .or_else(|| {
                self.capture(false, 0)
                    .ok()
                    .flatten()
                    .map(DeliveryTargetSnapshot::Text)
            })
    }

    pub fn capture_text_target(
        &self,
        maximum_selected_characters: i64,
    ) -> Result<Option<TextTargetSnapshot>, TextSelectionTooLargeError> {
        self.capture(true, maximum_selected_characters)
    }

    pub fn capture_application_identifier(&self) -> Option<String> {
        if !self.runtime {
            return None;
        }
        let focused = self.focus.borrow().node.clone()?;
        if focused.role()? == atspi::PASSWORD_TEXT {
            return None;
        }
        application_identifier(&focused).flatten()
    }

    fn capture(
        &self,
        include_selection: bool,
        maximum: i64,
    ) -> Result<Option<TextTargetSnapshot>, TextSelectionTooLargeError> {
        if !self.runtime {
            return Ok(None);
        }
        let node = self.focus.borrow().node.clone();
        capture(
            node,
            self.own_process_id,
            include_selection,
            maximum,
            Some(self.focus.clone()),
        )
    }

    pub fn close(&mut self) {
        drop(self.listener.take());
        self.focus.borrow_mut().node = None;
    }
}
impl Drop for FocusedTextTargetTracker {
    fn drop(&mut self) {
        self.close();
    }
}

/// In-memory offsets and an optional explicitly selected Command input; never persist the proxy.
#[derive(Clone)]
pub struct TextTargetSnapshot {
    accessible: Node,
    text: Text,
    editable: Option<EditableText>,
    selected_text: Option<String>,
    selection: Option<(i32, i32)>,
    caret_offset: i32,
    application_identifier: Option<String>,
    current: Option<Rc<RefCell<Focus>>>,
}
impl fmt::Debug for TextTargetSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TextTargetSnapshot")
            .field("selection", &self.selection)
            .field("caret_offset", &self.caret_offset)
            .field("application_identifier", &self.application_identifier)
            .field("editable", &self.editable.is_some())
            .finish_non_exhaustive()
    }
}
impl TextTargetSnapshot {
    pub fn selected_text(&self) -> Option<&str> {
        self.selected_text.as_deref()
    }
    pub fn selection(&self) -> Option<(i32, i32)> {
        self.selection
    }
    pub fn caret_offset(&self) -> i32 {
        self.caret_offset
    }
    pub fn application_identifier(&self) -> Option<&str> {
        self.application_identifier.as_deref()
    }
    pub fn has_selection(&self) -> bool {
        self.selection.is_some()
    }
    pub fn editable_text_available(&self) -> bool {
        self.editable.is_some()
    }
    pub fn without_selected_text(&self) -> Self {
        Self {
            accessible: self.accessible.clone(),
            text: self.text.clone(),
            editable: self.editable.clone(),
            selected_text: None,
            selection: self.selection,
            caret_offset: self.caret_offset,
            application_identifier: self.application_identifier.clone(),
            current: self.current.clone(),
        }
    }

    pub fn restore(&self) -> bool {
        let restore = || -> Option<bool> {
            if self.editable.is_none()
                && self
                    .current
                    .as_ref()
                    .is_some_and(|current| current.borrow().node.as_ref() != Some(&self.accessible))
            {
                return Some(false);
            }
            if !self.accessible.has_state(atspi::FOCUSED)? && !self.accessible.grab_focus()? {
                return Some(false);
            }
            match self.selection {
                None => self.text.set_caret(self.caret_offset),
                Some((start, end)) => {
                    if self.text.selection_count()? > 0 {
                        self.text.set_selection(start, end)
                    } else {
                        self.text.add_selection(start, end)
                    }
                }
            }
        };
        restore().unwrap_or(false)
    }

    /// Check only the caret. Gecko reports UTF-16 units; other targets use Unicode characters.
    pub fn confirm_insertion(&self, inserted: &str) -> Option<bool> {
        let start = i64::from(self.selection.map_or(self.caret_offset, |(start, _)| start));
        let length = if firefox(self.application_identifier()) {
            inserted.encode_utf16().count()
        } else {
            inserted.chars().count()
        };
        let expected = start.checked_add(i64::try_from(length).ok()?)?;
        Some(i64::from(self.text.caret()?) == expected)
    }

    /// A failed direct mutation is uncertain; callers must never retry it with keyboard input.
    pub fn insert_text(&self, inserted: &str) -> Option<bool> {
        let editable = self.editable.as_ref()?;
        let insert = || -> Option<bool> {
            if let Some((start, end)) = self.selection
                && !editable.delete(start, end)?
            {
                return Some(false);
            }
            let start = self.selection.map_or(self.caret_offset, |(start, _)| start);
            if !editable.insert(start, inserted)? {
                return Some(false);
            }
            if let Ok(length) = i32::try_from(inserted.chars().count())
                && let Some(caret) = start.checked_add(length)
            {
                let _ = self.text.set_caret(caret);
            }
            Some(true)
        };
        Some(insert().unwrap_or(false))
    }
}

#[derive(Debug, Clone)]
pub enum DeliveryTargetSnapshot {
    Text(TextTargetSnapshot),
    Terminal(TerminalTargetSnapshot),
}
impl mluva_workflows::dictation::DeliveryTarget for DeliveryTargetSnapshot {
    fn application_identifier(&self) -> Option<&str> {
        self.application_identifier()
    }
    fn restore(&self) -> mluva_core::delivery::TargetResult<bool> {
        Ok(self.restore())
    }
    fn insert_text(&self, inserted: &str) -> mluva_core::delivery::TargetResult<Option<bool>> {
        Ok(self.insert_text(inserted))
    }
    fn confirm_insertion(
        &self,
        inserted: &str,
    ) -> mluva_core::delivery::TargetResult<Option<bool>> {
        Ok(self.confirm_insertion(inserted))
    }
}
impl DeliveryTargetSnapshot {
    pub fn without_selected_text(&self) -> Self {
        match self {
            Self::Text(target) => Self::Text(target.without_selected_text()),
            Self::Terminal(target) => Self::Terminal(target.clone()),
        }
    }
    pub fn application_identifier(&self) -> Option<&str> {
        match self {
            Self::Text(target) => target.application_identifier(),
            Self::Terminal(target) => Some(target.application_identifier()),
        }
    }
    pub fn restore(&self) -> bool {
        match self {
            Self::Text(target) => target.restore(),
            Self::Terminal(target) => target.restore(),
        }
    }
    pub fn insert_text(&self, text: &str) -> Option<bool> {
        match self {
            Self::Text(target) => target.insert_text(text),
            Self::Terminal(target) => target.insert_text(text),
        }
    }
    pub fn confirm_insertion(&self, text: &str) -> Option<bool> {
        match self {
            Self::Text(target) => target.confirm_insertion(text),
            Self::Terminal(target) => target.confirm_insertion(text),
        }
    }
}

pub fn capture_focused_text_target(
    maximum: i64,
) -> Result<Option<TextTargetSnapshot>, TextSelectionTooLargeError> {
    if !atspi::initialize() {
        return Ok(None);
    }
    let own_pid = std::process::id();
    capture(
        Node::desktop().and_then(|root| find_focused(root, own_pid, false)),
        own_pid,
        true,
        maximum,
        None,
    )
}
pub fn capture_focused_delivery_target() -> Option<TextTargetSnapshot> {
    if !atspi::initialize() {
        return None;
    }
    let own_pid = std::process::id();
    capture(
        Node::desktop().and_then(|root| find_focused(root, own_pid, false)),
        own_pid,
        false,
        0,
        None,
    )
    .ok()
    .flatten()
}
pub fn capture_focused_application_identifier() -> Option<String> {
    if !atspi::initialize() {
        return None;
    }
    let node = Node::desktop().and_then(|root| find_focused(root, std::process::id(), false))?;
    if node.role()? == atspi::PASSWORD_TEXT {
        return None;
    }
    application_identifier(&node).flatten()
}

fn capture(
    node: Option<Node>,
    own_pid: u32,
    include_selection: bool,
    maximum: i64,
    current: Option<Rc<RefCell<Focus>>>,
) -> Result<Option<TextTargetSnapshot>, TextSelectionTooLargeError> {
    let prepare = || -> Option<TextTargetSnapshot> {
        let node = node?;
        if node.process_id()? == own_pid || node.role()? == atspi::PASSWORD_TEXT {
            return None;
        }
        let text = node.text()?;
        let editable = if node.has_state(atspi::EDITABLE)? {
            let editable = node.editable_text();
            if editable.is_some() && firefox(application_identifier(&node)?.as_deref()) {
                None
            } else {
                editable
            }
        } else {
            None
        };
        let caret_offset = text.caret()?;
        let selection = if text.selection_count()? <= 0 {
            if caret_offset < 0 {
                return None;
            }
            None
        } else {
            let (start, end) = text.selection()?;
            if start < 0 || end < start {
                return None;
            }
            Some((start, end))
        };
        let selected_text = if include_selection {
            match selection {
                Some((start, end)) => Some(text.get_text(start, end)?),
                None => None,
            }
        } else {
            None
        };
        Some(TextTargetSnapshot {
            accessible: node,
            text,
            editable,
            caret_offset,
            selection,
            selected_text,
            application_identifier: None,
            current,
        })
    };
    let Some(mut snapshot) = prepare() else {
        return Ok(None);
    };
    if let Some(selected) = snapshot.selected_text() {
        let characters = selected.chars().count();
        if maximum < 0 || u64::try_from(characters).unwrap_or(u64::MAX) > maximum as u64 {
            return Err(TextSelectionTooLargeError {
                characters,
                maximum,
            });
        }
    }
    let Some(identifier) = application_identifier(&snapshot.accessible) else {
        return Ok(None);
    };
    snapshot.application_identifier = identifier;
    Ok(Some(snapshot))
}

/// Preserve query failure separately from a process that has no readable local identifier.
fn application_identifier(node: &Node) -> Option<Option<String>> {
    let pid = node.process_id()?;
    if pid == 0 {
        return Some(None);
    }
    let directory = Path::new("/proc").join(pid.to_string());
    match fs::read_link(directory.join("exe")) {
        Ok(executable) => {
            let name = executable.to_string_lossy();
            let name = name.strip_suffix(" (deleted)").unwrap_or(&name);
            Some((!name.is_empty()).then(|| {
                Path::new(name)
                    .components()
                    .collect::<std::path::PathBuf>()
                    .to_string_lossy()
                    .into_owned()
            }))
        }
        Err(_) => match fs::read_to_string(directory.join("comm")) {
            Ok(name) => {
                let name = mluva_core::text::trim(&name);
                Some((!name.is_empty()).then(|| format!("process:{name}")))
            }
            Err(error) if error.kind() == std::io::ErrorKind::InvalidData => None,
            Err(_) => Some(None),
        },
    }
}

fn firefox(identifier: Option<&str>) -> bool {
    let identifier = identifier.unwrap_or_default();
    let basename = Path::new(identifier.strip_prefix("process:").unwrap_or(identifier))
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    matches!(
        basename
            .chars()
            .default_case_fold()
            .collect::<String>()
            .as_str(),
        "firefox" | "firefox-bin" | "firefox-esr"
    )
}

fn find_focused(root: Node, own_pid: u32, require_active: bool) -> Option<Node> {
    let mut active_roots = Vec::new();
    for index in 0..root.child_count().unwrap_or(0) {
        let Some(application) = root.child(index) else {
            continue;
        };
        let Some(pid) = application.process_id() else {
            continue;
        };
        let own = pid == own_pid;
        if own && !require_active {
            continue;
        }
        for index in 0..application.child_count().unwrap_or(0) {
            let Some(window) = application.child(index) else {
                continue;
            };
            if window.has_state(atspi::ACTIVE) == Some(true) {
                if own {
                    return None;
                }
                active_roots.push(window);
            }
        }
    }
    if require_active && active_roots.len() != 1 {
        return None;
    }
    let mut stack = if active_roots.is_empty() {
        vec![root]
    } else {
        active_roots.into_iter().rev().collect()
    };
    let mut visited = 0;
    let mut candidates = Vec::new();
    while !stack.is_empty() && visited < MAX_ACCESSIBLE_NODES {
        let node = stack.pop()?;
        visited += 1;
        if node.process_id() == Some(own_pid) {
            continue;
        }
        if node.has_state(atspi::FOCUSED) == Some(true) && node.text().is_some() {
            candidates.push(node.clone());
        }
        for index in (0..node.child_count().unwrap_or(0)).rev() {
            if let Some(child) = node.child(index) {
                stack.push(child);
            }
        }
    }
    (candidates.len() == 1 && stack.is_empty()).then(|| candidates.remove(0))
}
