//! Exact external text targets. Only Command capture reads an explicit selection.
//!
//! libatspi proxies and listeners stay on the GLib thread that first initializes them.
//! Dispatch desktop capture/restoration there; send content-free receipts to workers.

use crate::atspi::{self, EditableText, EventListener, Node, Text};
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
    listener: Option<EventListener>,
    applications_listener: Option<EventListener>,
}

impl FocusedTextTargetTracker {
    pub fn new() -> Result<Self, TrackerError> {
        let runtime = system_accessibility_enabled();
        if !runtime && !hyprland_terminal_tracking_available(None) {
            return Err(TrackerError::AccessibilityDisabled);
        }
        let own_process_id = std::process::id();
        let focus = Rc::new(RefCell::new(Focus::default()));
        let mut applications_listener = None;
        let listener = if runtime {
            if !atspi::initialize() {
                return Err(TrackerError::WrongThread);
            }
            let state = focus.clone();
            let listener = EventListener::focus(Rc::new(move |source, detail, _, _| {
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
            let root = Node::desktop();
            if let Some(root) = &root {
                let watched = root.clone();
                applications_listener = Some(
                    EventListener::applications_added(Rc::new(move |source, index, _, _| {
                        if source.as_ref() == Some(&watched) && index >= 0 {
                            announce_accessibility_client(watched.child(index), own_process_id);
                        }
                    }))
                    .ok_or(TrackerError::RegistrationRejected)?,
                );
                // Chromium exposes its web tree only after a normal extended-properties
                // request. Query application roots only; discard all returned attributes.
                for index in 0..root
                    .child_count()
                    .unwrap_or(0)
                    .min(MAX_ACCESSIBLE_NODES as i32)
                {
                    announce_accessibility_client(root.child(index), own_process_id);
                }
            }
            let initial = root.and_then(|root| find_focused(root, own_process_id, true));
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
            applications_listener,
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
        drop(self.applications_listener.take());
        drop(self.listener.take());
        self.focus.borrow_mut().node = None;
    }
}

fn announce_accessibility_client(application: Option<Node>, own_pid: u32) {
    if let Some(application) = application
        && application
            .process_id()
            .is_some_and(|pid| pid != 0 && pid != own_pid)
    {
        application.request_attributes();
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
    retained_selection: Option<Rc<RetainedSelection>>,
}

#[derive(Default)]
struct SelectionChanges {
    changed: bool,
    insertion: Option<(i32, i32)>,
}

/// Chromium's selection offsets can be converted twice. Keep the actual selection in place
/// and watch only numeric changes to this exact node, never the text event payload.
struct RetainedSelection {
    characters: i32,
    changes: Rc<RefCell<SelectionChanges>>,
    _listeners: [EventListener; 3],
}

impl RetainedSelection {
    fn capture(node: &Node, text: &Text) -> Option<Self> {
        if !atspi::read_pending_events() {
            return None;
        }
        // This query processes events already queued before the snapshot's listeners exist.
        if text.selection_count()? != 1 {
            return None;
        }
        let application = node.application()?;
        let changes = Rc::new(RefCell::new(SelectionChanges::default()));
        let changed = {
            let node = node.clone();
            let changes = changes.clone();
            Rc::new(move |source: Option<Node>, _, _, _| {
                if source.as_ref() == Some(&node) {
                    changes.borrow_mut().changed = true;
                }
            })
        };
        let selection = EventListener::selection_changed(&application, changed.clone())?;
        let caret = EventListener::caret_moved(&application, changed)?;
        let content = {
            let node = node.clone();
            let changes = changes.clone();
            EventListener::text_changed(
                &application,
                Rc::new(move |source, start, length, inserted| {
                    if source.as_ref() == Some(&node) {
                        let mut changes = changes.borrow_mut();
                        changes.changed = true;
                        changes.insertion = (inserted && start >= 0 && length > 0)
                            .then(|| start.checked_add(length).map(|end| (start, end)))
                            .flatten();
                    }
                }),
            )?
        };
        Some(Self {
            characters: text.character_count()?,
            changes,
            _listeners: [selection, caret, content],
        })
    }

    fn unchanged(&self, text: &Text, caret: i32) -> Option<bool> {
        if !atspi::read_pending_events() {
            return Some(false);
        }
        if self.changes.borrow().changed {
            return Some(false);
        }
        let unchanged = text.character_count()? == self.characters
            && text.caret()? == caret
            && text.selection_count()? == 1;
        Some(unchanged && !self.changes.borrow().changed)
    }

    fn confirm(&self, text: &Text, caret_before: i32, inserted: &str) -> Option<bool> {
        if !atspi::read_pending_events() {
            return None;
        }
        let caret = i64::from(text.caret()?);
        if text.selection_count()? != 0 {
            return Some(false);
        }
        let length = i64::try_from(inserted.chars().count()).ok()?;
        let lower = caret.checked_sub(length)?;
        let removed = length - (i64::from(text.character_count()?) - i64::from(self.characters));
        if lower < 0 || removed <= 0 || ![lower, lower + removed].contains(&i64::from(caret_before))
        {
            return Some(false);
        }
        let changes = self.changes.borrow();
        if !changes.changed {
            return Some(false);
        }
        // Chromium sometimes reports the preserved prefix in its insertion range. It must
        // still cover the entire proposed insertion and agree with the collapsed caret.
        // An identical replacement or a reduced diff cannot confirm the complete text.
        let (start, end) = changes.insertion?;
        (i64::from(start) <= lower && caret <= i64::from(end)).then_some(true)
    }
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
            retained_selection: self.retained_selection.clone(),
        }
    }

    pub fn restore(&self) -> bool {
        let restore = || -> Option<bool> {
            if let Some(retained) = &self.retained_selection {
                let current = self.current.as_ref()?;
                if current.borrow().node.as_ref() != Some(&self.accessible)
                    || !self.accessible.has_state(atspi::FOCUSED)?
                    || !retained.unchanged(&self.text, self.caret_offset)?
                {
                    return Some(false);
                }
                return Some(
                    current.borrow().node.as_ref() == Some(&self.accessible)
                        && !retained.changes.borrow().changed,
                );
            }
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

    /// Check caret/range metadata without reading text. Gecko uses UTF-16; others use characters.
    pub fn confirm_insertion(&self, inserted: &str) -> Option<bool> {
        if let Some(retained) = &self.retained_selection {
            if self.current.as_ref()?.borrow().node.as_ref() != Some(&self.accessible) {
                return Some(false);
            }
            return retained.confirm(&self.text, self.caret_offset, inserted);
        }
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
        if self.retained_selection.is_some() {
            return None;
        }
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
        let identifier = application_identifier(&node)?;
        let selection_count = text.selection_count()?;
        let retained_selection =
            if !include_selection && selection_count == 1 && chromium(identifier.as_deref()) {
                Some(Rc::new(RetainedSelection::capture(&node, &text)?))
            } else {
                None
            };
        let editable = if node.has_state(atspi::EDITABLE)? {
            let editable = node.editable_text();
            if editable.is_some()
                && (firefox(identifier.as_deref()) || retained_selection.is_some())
            {
                None
            } else {
                editable
            }
        } else {
            None
        };
        let caret_offset = text.caret()?;
        let selection = if selection_count <= 0 {
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
        if retained_selection
            .as_ref()
            .is_some_and(|retained| retained.unchanged(&text, caret_offset) != Some(true))
        {
            return None;
        }
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
            application_identifier: identifier,
            current,
            retained_selection,
        })
    };
    let Some(snapshot) = prepare() else {
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
    matches!(
        executable_name(identifier).as_str(),
        "firefox" | "firefox-bin" | "firefox-esr"
    )
}

fn chromium(identifier: Option<&str>) -> bool {
    matches!(
        executable_name(identifier).as_str(),
        "chromium"
            | "chromium-browser"
            | "chrome"
            | "google-chrome"
            | "google-chrome-stable"
            | "google-chrome-beta"
            | "google-chrome-unstable"
    )
}

fn executable_name(identifier: Option<&str>) -> String {
    let identifier = identifier.unwrap_or_default();
    let basename = Path::new(identifier.strip_prefix("process:").unwrap_or(identifier))
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    basename.chars().default_case_fold().collect()
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
