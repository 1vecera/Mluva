//! The released command catalog, bound to live application controls and document identity.

use std::rc::Rc;

use adw::prelude::*;

use crate::command_palette::Command;
use crate::settings_view::SettingsView;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandAction {
    ToggleRecording,
    CopyOutput,
    SaveEdits,
    History,
    ShowLive,
    ToggleSidebar,
    Settings,
    Welcome,
    WidgetPositionSetting,
    WorkspaceSetting { name: String, value: String },
}

/// Read this from the current widgets/session at each dispatch, rather than caching availability.
#[derive(Clone, Default)]
pub struct CommandState {
    pub page: String,
    pub document: Option<String>,
    pub editor: Option<gtk::Widget>,
    pub recording: bool,
    pub preparing: bool,
    pub record_sensitive: bool,
    pub live_visible: bool,
    pub can_copy: bool,
    pub busy: bool,
    pub private: bool,
    pub edit_draft: bool,
    pub live_active: bool,
    pub viewing_live: bool,
    pub processing: bool,
    pub incognito: bool,
}

pub struct CommandContext {
    pub state: Rc<dyn Fn() -> CommandState>,
    pub invoke: Rc<dyn Fn(CommandAction)>,
}

#[derive(Clone, Copy)]
enum Availability {
    Always,
    Recording,
    Copy,
    Save,
    LiveConversation,
}

fn command(
    context: &Rc<CommandContext>,
    snapshot: &Rc<CommandState>,
    description: (&str, &str, &str, &str),
    action: CommandAction,
    availability: Availability,
) -> Command {
    let (title, icon, keywords, shortcut) = description;
    let invoke = context.invoke.clone();
    let state = context.state.clone();
    let original = snapshot.clone();
    Command {
        title: title.into(),
        icon: icon.into(),
        keywords: keywords.into(),
        shortcut: shortcut.into(),
        run: Rc::new(move || invoke(action.clone())),
        enabled: Rc::new(move || {
            if matches!(availability, Availability::Always) {
                return true;
            }
            let current = state();
            let document = original.editor.is_some()
                && current.page == "capture"
                && current.document == original.document
                && current.editor == original.editor;
            match availability {
                Availability::Always => true,
                Availability::Recording => {
                    current.record_sensitive
                        && current.preparing == original.preparing
                        && current.recording == original.recording
                }
                Availability::Copy => document && current.can_copy,
                Availability::Save => {
                    document
                        && !current.busy
                        && !current.private
                        && !current.live_visible
                        && current.document.is_some()
                        && current.edit_draft
                }
                Availability::LiveConversation => current.live_active,
            }
        }),
    }
}

/// Freeze command identities at panel open; closures retain the current document and session guards.
pub fn application_commands(
    context: &Rc<CommandContext>,
    settings: &Rc<SettingsView>,
    settings_pages: &[adw::PreferencesPage],
) -> Vec<Command> {
    let snapshot = Rc::new((context.state)());
    let record_title = if snapshot.preparing {
        "Cancel preparation"
    } else if snapshot.recording {
        "Stop dictation"
    } else {
        "Start dictation"
    };
    let record_icon = if snapshot.recording {
        "media-playback-stop-symbolic"
    } else {
        "audio-input-microphone-symbolic"
    };
    let items = [
        (
            record_title,
            record_icon,
            "record microphone capture F9",
            "",
            CommandAction::ToggleRecording,
            Availability::Recording,
        ),
        (
            "Copy current text",
            "edit-copy-symbolic",
            "clipboard output result",
            "<Control><Shift>c",
            CommandAction::CopyOutput,
            Availability::Copy,
        ),
        (
            "Save edits",
            "document-save-symbolic",
            "keep document note",
            "<Control>s",
            CommandAction::SaveEdits,
            Availability::Save,
        ),
        (
            "History",
            "document-open-recent-symbolic",
            "archive search",
            "<Control>h",
            CommandAction::History,
            Availability::Always,
        ),
        (
            "Live conversation",
            "audio-input-microphone-symbolic",
            "current recording return",
            "",
            CommandAction::ShowLive,
            Availability::LiveConversation,
        ),
        (
            "Toggle history sidebar",
            "sidebar-show-symbolic",
            "navigation show hide",
            "<Control>b",
            CommandAction::ToggleSidebar,
            Availability::Always,
        ),
        (
            "Settings",
            "preferences-system-symbolic",
            "speech models appearance preferences",
            "<Control>comma",
            CommandAction::Settings,
            Availability::Always,
        ),
        (
            "Settings · Welcome and provider setup",
            "go-home-symbolic",
            "onboarding speech recognition local cloud model",
            "",
            CommandAction::Welcome,
            Availability::Always,
        ),
        (
            "Settings · Workspace · Floating widget position",
            "preferences-system-symbolic",
            "recorder appearance left center right",
            "",
            CommandAction::WidgetPositionSetting,
            Availability::Always,
        ),
    ];
    let mut commands: Vec<_> = items
        .into_iter()
        .map(|(title, icon, keywords, shortcut, action, availability)| {
            command(
                context,
                &snapshot,
                (title, icon, keywords, shortcut),
                action,
                availability,
            )
        })
        .collect();
    let positions = [
        ("bottom-left", "Lower left"),
        ("bottom-center", "Bottom"),
        ("bottom-right", "Lower right"),
    ];
    let times = [("24h", "24-hour · 14:30"), ("12h", "12-hour · 2:30 PM")];
    for (name, choices, title) in [
        ("widget_position", positions.as_slice(), "Widget position"),
        ("time_format", times.as_slice(), "Time format"),
    ] {
        for (value, label) in choices {
            commands.push(command(
                context,
                &snapshot,
                (
                    &format!("Settings · {title}: {label}"),
                    "preferences-system-symbolic",
                    "settings preferences",
                    "",
                ),
                CommandAction::WorkspaceSetting {
                    name: name.into(),
                    value: (*value).into(),
                },
                Availability::Always,
            ));
        }
    }
    commands.extend(settings_commands(
        settings,
        settings_pages,
        context.invoke.clone(),
    ));
    commands
}

/// Discover actual rows within the application's ordered preference index.
pub fn settings_commands(
    settings: &Rc<SettingsView>,
    pages: &[adw::PreferencesPage],
    invoke: Rc<dyn Fn(CommandAction)>,
) -> Vec<Command> {
    fn visit(
        widget: &gtk::Widget,
        page: &adw::PreferencesPage,
        group: &str,
        settings: &Rc<SettingsView>,
        invoke: &Rc<dyn Fn(CommandAction)>,
        output: &mut Vec<Command>,
    ) {
        let group_title = widget
            .downcast_ref::<adw::PreferencesGroup>()
            .map(|group_widget| group_widget.title())
            .filter(|title| !title.is_empty());
        let current_group = group_title.as_deref().unwrap_or(group);
        if let Some(row) = widget.downcast_ref::<adw::PreferencesRow>()
            && !row.title().is_empty()
        {
            let title = [
                "Settings",
                page.title().as_str(),
                current_group,
                row.title().as_str(),
            ]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
            let keywords = widget
                .downcast_ref::<adw::ActionRow>()
                .map(|row| {
                    format!(
                        "preferences configure {}",
                        row.subtitle().unwrap_or_default()
                    )
                })
                .unwrap_or_else(|| "preferences configure".into());
            // The panel owns these bindings until dismissal, including rows refreshed out of the UI.
            // These commands are not installed on the original widgets, so retaining them creates no widget cycle.
            let settings = settings.clone();
            let page = page.clone();
            let row = row.clone();
            let invoke = invoke.clone();
            output.push(Command {
                title,
                icon: "preferences-system-symbolic".into(),
                keywords,
                shortcut: String::new(),
                enabled: Rc::new(|| true),
                run: Rc::new(move || {
                    invoke(CommandAction::Settings);
                    settings.focus_row(&page, &row);
                }),
            });
        }
        let mut child = widget.first_child();
        while let Some(widget) = child {
            visit(&widget, page, current_group, settings, invoke, output);
            child = widget.next_sibling();
        }
    }
    let mut output = Vec::new();
    for page in pages {
        visit(page.upcast_ref(), page, "", settings, &invoke, &mut output);
    }
    output
}
