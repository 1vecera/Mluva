//! The immutable released human-acceptance registry; native acceptance is tracked separately.

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
        identifier: "screenshot_context",
        title: "Screenshot context and narrated annotations",
        summary: "Omarchy region capture, visual AI context and spoken Tensaku text boxes passed isolated checks; physical-key and microphone acceptance remain pending.",
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
        identifier: "saved_styles",
        title: "Custom saved styles",
        summary: "A user-created writing style can be selected and applied to dictated text.",
        maturity: Maturity::Verified,
    },
    Capability {
        identifier: "conversations",
        title: "Rewrite conversations",
        summary: "Originals, Quick Polish, Structured Note, follow-ups and conversation export form the daily Omarchy workspace.",
        maturity: Maturity::Verified,
    },
    Capability {
        identifier: "editable_documents",
        title: "Editable documents and automatic copy",
        summary: "Originals and rewrites can be edited, saved and copied automatically in the Omarchy workflow.",
        maturity: Maturity::Verified,
    },
    Capability {
        identifier: "provider_choice",
        title: "Independent rewrite and speech providers",
        summary: "Compatible APIs and app-owned local models have controlled checks; cloud accounts need acceptance.",
        maturity: Maturity::Experimental,
    },
    Capability {
        identifier: "live_rewrite",
        title: "Live structured rewriting",
        summary: "Opt-in task, note, polish and custom drafts expose missing information; model quality needs acceptance.",
        maturity: Maturity::Experimental,
    },
    Capability {
        identifier: "automatic_paste",
        title: "Automatic paste",
        summary: "Known limitation: insertion is disabled by default and depends on the target application and desktop; clipboard delivery is the standard workflow.",
        maturity: Maturity::Experimental,
    },
    Capability {
        identifier: "faithful_cleanup",
        title: "Faithful cleanup",
        summary: "Optional Codex cleanup preserves the raw transcript and falls back safely, but still needs manual acceptance.",
        maturity: Maturity::Experimental,
    },
    Capability {
        identifier: "spoken_structure",
        title: "Spoken structure",
        summary: "Punctuation, paragraph, and scratch-that commands still need manual acceptance.",
        maturity: Maturity::Experimental,
    },
    Capability {
        identifier: "command_mode",
        title: "Command mode",
        summary: "Spoken editing instructions are previewed before delivery and still need manual acceptance.",
        maturity: Maturity::Experimental,
    },
    Capability {
        identifier: "notes_mode",
        title: "Notes mode",
        summary: "Longer acceptance-gated drafts and their relaunch recovery still need manual acceptance.",
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
        identifier: "application_memory",
        title: "Per-application memory and context",
        summary: "Remembered modes, styles, rules, and bounded context controls still need manual acceptance.",
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
        summary: "The themed widget, completed-note rewrites and opening the workspace are used daily on Omarchy.",
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
