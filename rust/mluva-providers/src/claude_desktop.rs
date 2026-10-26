//! A new chat in the Claude desktop app, opened by link with the dictation in its composer.
//!
//! Claude Desktop documents `claude://claude.ai/new?q=<text>`: it opens a new chat with the text
//! prefilled for the person to review and send, and never submits it. Existing chats live on
//! claude.ai and no local interface reaches them, so this one target stands for "a new chat".
//! Opening a link raises and focuses the app, which is acceptable only because the person chose
//! this target; no other target ever falls back to it. The text travels in the link, so the
//! system's link opener and Claude Desktop receive it on their command lines. Longer text than
//! Claude Desktop keeps is refused, never cut.
use crate::{
    agent_platform::{self, Opened, Opener},
    agent_targets::Receipt,
};
use mluva_core::agent_voice::{
    AgentKind, AgentStatus, AppLink, Route, SendOutcome, Submission, Target,
};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

const NEW_CHAT_LINK: &str = "claude://claude.ai/new?q=";
/// Linux limits one argument to 128 KiB; an encoded link must stay below it.
const MAX_LINK_BYTES: usize = 120_000;
/// How long a check for the installed app is trusted; `xdg-mime` is not cheap.
const HANDLER_TTL: Duration = Duration::from_secs(5 * 60);

/// The one Claude desktop chat target: a new chat.
pub fn new_chat() -> Target {
    Target {
        kind: AgentKind::ClaudeDesktop,
        name: "claude-new-chat".into(),
        title: "New chat".into(),
        status: AgentStatus::Idle,
        app: Some(AppLink::ClaudeNewChat),
        ..Target::default()
    }
}

/// Whether Claude Desktop is set up here: its user data exists in this account (it has been
/// opened) and an application handles `claude://` links. The handler is checked at most every
/// few minutes.
pub async fn installed() -> bool {
    if !agent_platform::claude_desktop_data().is_some_and(|data| data.is_dir()) {
        return false;
    }
    static CHECKED: Mutex<Option<(Instant, bool)>> = Mutex::new(None);
    let cached = CHECKED
        .lock()
        .ok()
        .and_then(|checked| *checked)
        .filter(|(at, _)| at.elapsed() < HANDLER_TTL);
    if let Some((_, installed)) = cached {
        return installed;
    }
    let installed = agent_platform::scheme_handler("claude").await.is_some();
    if let Ok(mut checked) = CHECKED.lock() {
        *checked = Some((Instant::now(), installed));
    }
    installed
}

/// Open a new chat with the text in its composer, once.
pub async fn submit(submission: &Submission, cancel: &CancellationToken) -> Receipt {
    submit_with(submission, cancel, &Opener::system()).await
}

async fn submit_with(
    submission: &Submission,
    cancel: &CancellationToken,
    opener: &Opener,
) -> Receipt {
    let route = submission.target.route();
    let link = format!("{NEW_CHAT_LINK}{}", encode(&submission.text));
    if route != Route::ClaudeNewChat
        || !submission.target.can_receive()
        || submission.text.trim().is_empty()
        || route
            .max_chars()
            .is_some_and(|max| submission.text.chars().count() > max)
        || link.len() > MAX_LINK_BYTES
        || cancel.is_cancelled()
    {
        return SendOutcome::Rejected.into();
    }
    match opener.open(&link).await {
        Opened::Handed => SendOutcome::Accepted,
        Opened::NotStarted => SendOutcome::Rejected,
        // The app may have the link; it is never opened twice.
        Opened::Uncertain => SendOutcome::Unconfirmed,
    }
    .into()
}

/// Percent-encode everything but RFC 3986's unreserved characters, so spaces, newlines, `+`,
/// `&` and `#` reach the composer as typed.
fn encode(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// A link opener that records each link it is given and exits with `exit`.
    fn recording_opener(temp: &tempfile::TempDir, exit: u8) -> (Opener, std::path::PathBuf) {
        let log = temp.path().join(format!("opened-{exit}"));
        let script = temp.path().join(format!("opener-{exit}"));
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$1\" >> {}\nexit {exit}\n",
                log.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        (
            Opener {
                candidates: vec![vec![script.into()]],
            },
            log,
        )
    }

    fn submission(text: &str) -> Submission {
        Submission {
            id: "operation".into(),
            target: new_chat(),
            text: text.into(),
        }
    }

    #[test]
    fn the_new_chat_target_prefills_and_is_one_stable_choice() {
        let target = new_chat();
        assert_eq!(target.route(), Route::ClaudeNewChat);
        assert_eq!(
            target.delivery(),
            mluva_core::agent_voice::Delivery::Prefill
        );
        assert!(target.can_receive() && target.same_instance(&new_chat()));
        let saved: Target = serde_json::from_value(serde_json::to_value(&target).unwrap()).unwrap();
        assert!(saved.same_instance(&target));
        assert_eq!(saved.route(), Route::ClaudeNewChat);
        // A Claude Code session never matches it, whatever its fields.
        let session = Target {
            app: None,
            kind: AgentKind::ClaudeCode,
            ..target
        };
        assert!(!session.same_instance(&new_chat()));
    }

    #[test]
    fn text_is_encoded_whole() {
        assert_eq!(
            encode("Zkontroluj build & testy #1 + 50%\nDíky 🙂"),
            "Zkontroluj%20build%20%26%20testy%20%231%20%2B%2050%25%0AD%C3%ADky%20%F0%9F%99%82"
        );
        assert_eq!(encode("a-b.c_d~e"), "a-b.c_d~e");
    }

    #[tokio::test]
    async fn the_link_is_opened_once_and_its_result_reported() {
        let temp = tempfile::tempdir().unwrap();
        let (opener, log) = recording_opener(&temp, 0);
        let receipt = submit_with(
            &submission("Shrň změny"),
            &CancellationToken::new(),
            &opener,
        )
        .await;
        assert_eq!(receipt.outcome, SendOutcome::Accepted);
        assert!(receipt.taken.is_none());
        assert_eq!(
            std::fs::read_to_string(&log).unwrap(),
            "claude://claude.ai/new?q=Shr%C5%88%20zm%C4%9Bny\n"
        );
        let (failing, _) = recording_opener(&temp, 4);
        assert_eq!(
            submit_with(&submission("Again"), &CancellationToken::new(), &failing)
                .await
                .outcome,
            SendOutcome::Unconfirmed
        );
        let missing = Opener {
            candidates: vec![vec![temp.path().join("missing").into()]],
        };
        assert_eq!(
            submit_with(&submission("Nowhere"), &CancellationToken::new(), &missing)
                .await
                .outcome,
            SendOutcome::Rejected
        );
    }

    #[tokio::test]
    async fn longer_text_than_claude_keeps_and_other_targets_are_never_opened() {
        let temp = tempfile::tempdir().unwrap();
        let (opener, log) = recording_opener(&temp, 0);
        let fits = "a".repeat(14_336);
        assert_eq!(
            submit_with(&submission(&fits), &CancellationToken::new(), &opener)
                .await
                .outcome,
            SendOutcome::Accepted
        );
        let refused = [
            submission(&"a".repeat(14_337)),
            // Within the character limit, but too long for one command-line argument.
            submission(&"🙂".repeat(14_000)),
            submission("   "),
            Submission {
                target: Target {
                    app: None,
                    ..new_chat()
                },
                ..submission("Not a new chat")
            },
        ];
        for sending in refused {
            assert_eq!(
                submit_with(&sending, &CancellationToken::new(), &opener)
                    .await
                    .outcome,
                SendOutcome::Rejected
            );
        }
        let cancelled = CancellationToken::new();
        cancelled.cancel();
        assert_eq!(
            submit_with(&submission("Late"), &cancelled, &opener)
                .await
                .outcome,
            SendOutcome::Rejected
        );
        assert_eq!(std::fs::read_to_string(&log).unwrap().lines().count(), 1);
    }
}
