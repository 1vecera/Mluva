//! Agent dictation beside the normal recorder; configuration lives in Settings.
use adw::prelude::*;
use mluva_core::agent_voice::VoiceSettings;

pub struct AgentVoiceView {
    pub widget: gtk::Box,
    pub recipients: gtk::DropDown,
    pub refresh: gtk::Button,
    pub record: gtk::Button,
    pub status: gtk::Label,
    pub keep: gtk::Button,
    pub cancel: gtk::Button,
    pub send_draft: gtk::Button,
    pub copy_draft: gtk::Button,
    pub draft_actions: gtk::Box,
    pub feedback: gtk::Box,
    pub reply: gtk::Label,
    pub read: gtk::Button,
    pub replay: gtk::Button,
    pub mute: gtk::ToggleButton,
    pub settings: adw::PreferencesGroup,
    pub enabled: gtk::CheckButton,
    pub socket: adw::EntryRow,
    pub hold_key: gtk::DropDown,
    pub draft_key: gtk::DropDown,
    pub hold_key_row: adw::ActionRow,
    pub draft_key_row: adw::ActionRow,
    pub spoken: gtk::CheckButton,
    pub voice_id: adw::EntryRow,
    pub settings_status: gtk::Label,
    pub shortcut_status: gtk::Label,
}
fn icon(name: &str, tooltip: &str) -> gtk::Button {
    gtk::Button::builder()
        .icon_name(name)
        .tooltip_text(tooltip)
        .has_frame(false)
        .build()
}
fn label(text: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .max_width_chars(48)
        .build()
}
fn key_row(title: &str, subtitle: &str, key: &gtk::DropDown) -> adw::ActionRow {
    key.set_valign(gtk::Align::Center);
    let row = adw::ActionRow::builder()
        .title(title)
        .subtitle(subtitle)
        .build();
    row.add_suffix(key);
    row
}
impl AgentVoiceView {
    pub fn new(settings: &VoiceSettings) -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 6);
        crate::document_layout::margins(&widget, 16);
        widget.set_margin_top(4);
        widget.set_margin_bottom(8);
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let recipients = gtk::DropDown::from_strings(&["Choose a session"]);
        recipients.set_enable_search(true);
        recipients.set_hexpand(true);
        recipients.set_tooltip_text(Some("The selected agent receives your finished dictation"));
        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(|_, item| {
            item.downcast_ref::<gtk::ListItem>()
                .unwrap()
                .set_child(Some(
                    &gtk::Label::builder()
                        .xalign(0.0)
                        .ellipsize(gtk::pango::EllipsizeMode::End)
                        .max_width_chars(28)
                        .build(),
                ));
        });
        factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            if let (Some(value), Some(child)) = (
                item.item().and_downcast::<gtk::StringObject>(),
                item.child().and_downcast::<gtk::Label>(),
            ) {
                child.set_label(&value.string());
                child.set_tooltip_text(Some(&value.string()));
            }
        });
        recipients.set_factory(Some(&factory));
        let refresh = icon("view-refresh-symbolic", "Refresh local agent sessions");
        row.append(&recipients);
        row.append(&refresh);
        widget.append(&row);
        let record = gtk::Button::builder()
            .label("Dictate to agent")
            .tooltip_text("Press to start; press again to stop and send to the selected agent")
            .valign(gtk::Align::Center)
            .build();
        record.add_css_class("suggested-action");
        record.add_css_class("ml-record-toggle");
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let status = label("Choose an agent. Dictation sends automatically when you stop.");
        status.set_hexpand(true);
        status.set_width_chars(4);
        status.set_accessible_role(gtk::AccessibleRole::Status);
        status.add_css_class("caption");
        let keep = gtk::Button::with_label("Keep draft");
        let cancel = gtk::Button::with_label("Cancel");
        actions.append(&status);
        actions.append(&keep);
        actions.append(&cancel);
        widget.append(&actions);
        let draft_actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let send_draft = gtk::Button::with_label("Send draft");
        let copy_draft = gtk::Button::with_label("Copy draft");
        draft_actions.append(&send_draft);
        draft_actions.append(&copy_draft);
        widget.append(&draft_actions);
        let feedback = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let read = gtk::Button::with_label("Read status");
        read.add_css_class("flat");
        let reply = label("");
        reply.set_hexpand(true);
        reply.set_width_chars(4);
        reply.set_selectable(true);
        reply.add_css_class("caption");
        let replay = icon(
            "media-playback-start-symbolic",
            "Replay the visible status sentence",
        );
        let mute = gtk::ToggleButton::builder()
            .icon_name("audio-volume-muted-symbolic")
            .tooltip_text("Mute spoken status")
            .has_frame(false)
            .active(!settings.spoken_updates)
            .build();
        feedback.append(&read);
        feedback.append(&reply);
        feedback.append(&replay);
        feedback.append(&mute);
        widget.append(&feedback);

        let preferences = adw::PreferencesGroup::builder().title("Agent routing").description("Changes save automatically. Press Dictate to agent to start, then press again to stop and send. A two-second countdown lets you cancel.").build();
        let enabled = gtk::CheckButton::with_label("Enable agent dictation");
        enabled.set_active(settings.enabled);
        preferences.add(&enabled);
        let keys: Vec<_> = (1..=24).map(|n| format!("F{n}")).collect();
        let key_refs: Vec<_> = keys.iter().map(String::as_str).collect();
        let hold_key = gtk::DropDown::from_strings(&key_refs);
        hold_key.set_selected(
            keys.iter()
                .position(|s| s == &settings.hold_key)
                .unwrap_or(7) as u32,
        );
        let draft_key = gtk::DropDown::from_strings(&key_refs);
        draft_key.set_selected(
            keys.iter()
                .position(|s| s == &settings.draft_key)
                .unwrap_or(6) as u32,
        );
        let hold_key_row = key_row(
            "Agent dictation key",
            "Press once to start and again to stop and send",
            &hold_key,
        );
        let draft_key_row = key_row(
            "Keep agent draft",
            "Stop agent dictation without sending",
            &draft_key,
        );
        let shortcut_status = label("Waiting for desktop shortcut approval…");
        shortcut_status.add_css_class("caption");
        let socket = adw::EntryRow::builder()
            .title("Herdr socket · blank for automatic discovery")
            .text(settings.socket.as_deref().unwrap_or(""))
            .build();
        preferences.add(&socket);
        let spoken = gtk::CheckButton::with_label("Speak a brief agent status update");
        spoken.set_active(settings.spoken_updates);
        preferences.add(&spoken);
        let voice_id = adw::EntryRow::builder()
            .title("ElevenLabs voice ID · optional spoken status")
            .text(&settings.voice_id)
            .build();
        preferences.add(&voice_id);
        let privacy = label(
            "Spoken status uses the saved ElevenLabs key and may incur charges. Only the visible status sentence is sent for speech.",
        );
        privacy.add_css_class("caption");
        preferences.add(&privacy);
        let settings_status = label("");
        settings_status.add_css_class("error");
        preferences.add(&settings_status);
        Self {
            widget,
            recipients,
            refresh,
            record,
            status,
            keep,
            cancel,
            send_draft,
            copy_draft,
            draft_actions,
            feedback,
            reply,
            read,
            replay,
            mute,
            settings: preferences,
            enabled,
            socket,
            hold_key,
            draft_key,
            hold_key_row,
            draft_key_row,
            spoken,
            voice_id,
            settings_status,
            shortcut_status,
        }
    }
}
