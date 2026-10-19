//! Compact Herdr controls in the existing editor, using its native typography and spacing.
use adw::prelude::*;
use mluva_core::agent_voice::VoiceSettings;

pub struct AgentVoiceView {
    pub widget: gtk::Box,
    pub recipients: gtk::DropDown,
    pub refresh: gtk::Button,
    pub hold: gtk::Button,
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
    pub options: gtk::MenuButton,
    pub enabled: gtk::CheckButton,
    pub socket: gtk::Entry,
    pub hold_key: gtk::DropDown,
    pub draft_key: gtk::DropDown,
    pub spoken: gtk::CheckButton,
    pub voice_id: gtk::Entry,
    pub save: gtk::Button,
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
        .max_width_chars(40)
        .build()
}
fn setting(body: &gtk::Box, title: &str, child: &impl IsA<gtk::Widget>) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let title = label(title);
    title.set_hexpand(true);
    row.append(&title);
    row.append(child);
    body.append(&row);
}
impl AgentVoiceView {
    pub fn new(settings: &VoiceSettings) -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 6);
        widget.set_margin_start(16);
        widget.set_margin_end(16);
        widget.set_margin_top(8);
        widget.set_margin_bottom(8);
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let recipients = gtk::DropDown::from_strings(&["Choose a Herdr agent"]);
        recipients.set_enable_search(true);
        recipients.set_hexpand(true);
        recipients.set_tooltip_text(Some(
            "Local Herdr session · the destination stays fixed while you speak",
        ));
        // Keep long names and paths from imposing a minimum desktop width.
        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            item.set_child(Some(
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
        let refresh = icon("view-refresh-symbolic", "Refresh local Herdr agents");
        let options = gtk::MenuButton::builder()
            .icon_name("system-users-symbolic")
            .tooltip_text("Talk to Herdr · routing and spoken status settings")
            .has_frame(false)
            .build();
        let hold = gtk::Button::builder()
            .label("Hold to talk")
            .tooltip_text("Hold the mouse button or Space; release to finish speaking")
            .build();
        row.append(&recipients);
        row.append(&refresh);
        widget.append(&row);
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let status = label("Choose Herdr voice settings to enable routing.");
        status.set_hexpand(true);
        status.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        status.set_width_chars(4);
        status.set_accessible_role(gtk::AccessibleRole::Status);
        status.add_css_class("caption");
        let keep = gtk::Button::with_label("Keep draft");
        let cancel = gtk::Button::with_label("Cancel");
        actions.append(&hold);
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
        reply.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        reply.set_width_chars(4);
        reply.set_selectable(true);
        reply.add_css_class("caption");
        let replay = icon(
            "media-playback-start-symbolic",
            "Replay the visible status sentence",
        );
        let mute = gtk::ToggleButton::builder()
            .icon_name("audio-volume-muted-symbolic")
            .tooltip_text("Mute spoken status; text remains available")
            .has_frame(false)
            .active(!settings.spoken_updates)
            .build();
        feedback.append(&read);
        feedback.append(&reply);
        feedback.append(&replay);
        feedback.append(&mute);
        widget.append(&feedback);

        let popover = gtk::Popover::new();
        let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
        crate::document_layout::margins(&body, 16);
        body.set_width_request(310);
        let title = label("Talk to Herdr");
        title.add_css_class("heading");
        body.append(&title);
        let enabled = gtk::CheckButton::with_label("Enable agent routing");
        enabled.set_active(settings.enabled);
        body.append(&enabled);
        let explanation = label(
            "Hold to speak, release to finish. Your message sends after a two-second cancel window. Your selected speech provider stays unchanged.",
        );
        explanation.add_css_class("caption");
        body.append(&explanation);
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
        setting(&body, "Hold to talk", &hold_key);
        setting(&body, "Keep draft", &draft_key);
        let shortcut_status = label("The desktop must approve both shortcuts.");
        shortcut_status.add_css_class("caption");
        body.append(&shortcut_status);
        let socket = gtk::Entry::builder()
            .placeholder_text("Automatic local Herdr discovery")
            .text(settings.socket.as_deref().unwrap_or(""))
            .tooltip_text(
                "Optional absolute local Unix socket path; remote servers are not enabled",
            )
            .build();
        body.append(&label("Herdr connection"));
        body.append(&socket);
        body.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        let spoken = gtk::CheckButton::with_label("Speak a brief status update");
        spoken.set_active(settings.spoken_updates);
        body.append(&spoken);
        let voice_id = gtk::Entry::builder()
            .placeholder_text("ElevenLabs voice ID")
            .text(&settings.voice_id)
            .build();
        body.append(&voice_id);
        body.append(&gtk::LinkButton::with_label(
            "https://elevenlabs.io/app/voice-library",
            "Choose a voice in ElevenLabs",
        ));
        let privacy = label(
            "Optional paid voice uses ElevenLabs v4 Turbo and your saved API key. Only the displayed sentence is sent for speech. No microphone audio or terminal history is used. Incognito keeps no Mluva history; your recipient may retain messages.",
        );
        privacy.add_css_class("caption");
        body.append(&privacy);
        let settings_status = label("");
        settings_status.add_css_class("error");
        body.append(&settings_status);
        let save = gtk::Button::with_label("Save settings");
        save.add_css_class("suggested-action");
        body.append(&save);
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .width_request(350)
            .propagate_natural_height(true)
            .max_content_height(580)
            .child(&body)
            .build();
        popover.set_child(Some(&scroll));
        options.set_popover(Some(&popover));
        Self {
            widget,
            recipients,
            refresh,
            hold,
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
            options,
            enabled,
            socket,
            hold_key,
            draft_key,
            spoken,
            voice_id,
            save,
            settings_status,
            shortcut_status,
        }
    }
}
