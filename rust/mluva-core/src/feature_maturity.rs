//! Released human acceptance with explicitly experimental native additions.

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Maturity {
    Verified,
    Experimental,
}
impl Maturity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Verified => "Verified on Omarchy",
            Self::Experimental => "Experimental",
        }
    }
}
pub struct Capability {
    pub identifier: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
    pub maturity: Maturity,
}
pub const CAPABILITIES: &[Capability] = &[
    Capability {
        identifier: "herdr_voice",
        title: "Hold to talk to Herdr agents",
        summary: "Opt-in local routing needs the guarded Herdr integration; physical hold/release and optional ElevenLabs v4 Turbo voice acceptance remain pending.",
        maturity: Maturity::Experimental,
    },
    Capability {
        identifier: "screenshot_context",
        title: "Screenshot context and narrated annotations",
        summary: "Omarchy region capture, agent attachments and spoken Tensaku text boxes passed isolated checks; physical-key and microphone acceptance remain pending.",
        maturity: Maturity::Experimental,
    },
    Capability {
        identifier: "dictation",
        title: "Recording and transcription",
        summary: "F9 and the visible record control start, stop, and finalize ordinary dictation.",
        maturity: Maturity::Verified,
    },
    Capability {
        identifier: "recording_controls",
        title: "Recording setup controls",
        summary: "Language, microphone, function key, and recording behavior can be adjusted before capture.",
        maturity: Maturity::Verified,
    },
    Capability {
        identifier: "history",
        title: "History",
        summary: "Completed captures remain visible and recoverable in the local History surface.",
        maturity: Maturity::Verified,
    },
    Capability {
        identifier: "conversations",
        title: "Conversations",
        summary: "Original recognition, working edits, continued recordings and exports share one editor; existing saved rewrites remain readable.",
        maturity: Maturity::Verified,
    },
    Capability {
        identifier: "editable_documents",
        title: "Editable documents and automatic copy",
        summary: "Text can be edited, saved and copied; Copy and Save controls are always shown.",
        maturity: Maturity::Verified,
    },
    Capability {
        identifier: "provider_choice",
        title: "Speech provider choices",
        summary: "Compatible APIs and app-owned local models have controlled checks; cloud accounts need acceptance.",
        maturity: Maturity::Experimental,
    },
    Capability {
        identifier: "automatic_paste",
        title: "Automatic paste",
        summary: "Automatic delivery uses the exact captured target. Unsupported, changed or unavailable targets fall back to the clipboard; the visible record button is copy-only.",
        maturity: Maturity::Experimental,
    },
    Capability {
        identifier: "spoken_structure",
        title: "Spoken structure",
        summary: "Punctuation, paragraph, and scratch-that commands still need manual acceptance.",
        maturity: Maturity::Experimental,
    },
    Capability {
        identifier: "meeting_mode",
        title: "Meeting mode",
        summary: "Microphone plus system-audio capture, diarization, review, and archive still need manual acceptance.",
        maturity: Maturity::Experimental,
    },
    Capability {
        identifier: "desktop_overlay",
        title: "Shell menu and recording bar",
        summary: "The optional GNOME Shell menu and display-only bottom bar have off-screen evidence but still need live-desktop acceptance.",
        maturity: Maturity::Experimental,
    },
    Capability {
        identifier: "dictionary_suggestions",
        title: "Dictionary and vocabulary suggestions",
        summary: "Local replacements and review-only suggestions still need manual acceptance.",
        maturity: Maturity::Experimental,
    },
    Capability {
        identifier: "snippets",
        title: "Snippets",
        summary: "Explicit spoken expansions and portable typed-trigger storage still need manual acceptance.",
        maturity: Maturity::Experimental,
    },
    Capability {
        identifier: "recovery_privacy",
        title: "Advanced recovery, retention, and Incognito",
        summary: "Retries, reprocessing, exports, retention policies, and Incognito still need manual acceptance.",
        maturity: Maturity::Experimental,
    },
    Capability {
        identifier: "diagnostics",
        title: "Diagnostics export",
        summary: "Privacy-safe timing diagnostics still need manual acceptance.",
        maturity: Maturity::Experimental,
    },
    Capability {
        identifier: "omarchy_widget",
        title: "Omarchy recording widget",
        summary: "The themed widget starts and stops dictation and opens the editor; generation and review controls are removed.",
        maturity: Maturity::Verified,
    },
];
pub fn capability(identifier: &str) -> Option<&'static Capability> {
    CAPABILITIES
        .iter()
        .find(|capability| capability.identifier == identifier)
}
pub fn title(identifier: &str, title: &str) -> String {
    if capability(identifier).is_some_and(|item| item.maturity == Maturity::Experimental) {
        format!("{title} · Experimental")
    } else {
        title.into()
    }
}
pub fn description(identifier: &str, description: &str) -> String {
    if capability(identifier).is_some_and(|item| item.maturity == Maturity::Experimental) {
        format!("Experimental — {description}")
    } else {
        description.into()
    }
}
