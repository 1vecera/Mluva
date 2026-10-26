//! Every way Mluva reaches an agent session, behind one discovery pass and one delivery call.
//!
//! Each transport finds the sessions it can see and delivers only at the level it has been
//! verified to reach ([`mluva_core::agent_target::Route::delivery`]). Discovery joins the
//! sightings of one session, so a Claude Code session in a Herdr pane is one target whatever
//! sees it. Delivery picks the transport from the target itself; a target no transport reaches
//! is draft only and nothing connects for it.
//!
//! Every transport keeps the same guarantees: it re-checks the exact session immediately before
//! writing, refuses rather than retargets a replaced one, never writes again after an uncertain
//! write, and reads no terminal content, clipboard or focus.
use crate::{
    claude_inbox,
    claude_sessions::{self, ClaudeRegistry},
    herdr,
};
use mluva_core::agent_voice::{Route, SendOutcome, Submission, Target};
use std::{
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{Arc, Mutex},
};
use tokio_util::sync::CancellationToken;

/// At most this many Herdr servers are asked per pass.
const MAX_HERDR_SERVERS: usize = 16;

/// What one discovery pass may read.
#[derive(Clone, Default)]
pub struct Sources {
    /// Local Herdr sockets, from [`herdr::socket_candidates`].
    pub herdr: Vec<PathBuf>,
    /// Claude Code's own session registry, when this account has a Claude Code configuration.
    pub claude: Option<Arc<Mutex<ClaudeRegistry>>>,
}

/// One discovery pass over every source.
#[derive(Clone, Debug, Default)]
pub struct Found {
    /// Herdr's panes in order, then sessions Herdr does not show, in the order they started.
    pub targets: Vec<Target>,
    /// Some Herdr server answered.
    pub herdr: bool,
    /// Some Herdr server offers guarded prompts.
    pub guarded: bool,
    pub errors: Vec<String>,
}

impl Found {
    /// The panel's line for what discovery found; it names a transport only when one matters.
    pub fn summary(&self) -> String {
        if self.targets.is_empty() {
            return self.errors.last().cloned().unwrap_or_else(|| {
                "No sessions found. Start Claude Code, or Codex in Herdr, then refresh.".into()
            });
        }
        if self.targets.iter().any(Target::has_transport) {
            "Local agents · press to start and again to stop and deliver".into()
        } else if self.herdr && !self.guarded {
            "Herdr needs the guarded prompt integration. Sending is unavailable.".into()
        } else {
            "No session can be reached yet, so dictation keeps a draft in Mluva.".into()
        }
    }
}

/// Discover every session the sources can see. Herdr is only ever asked to list, and the
/// registry is read on a blocking thread. `None` when cancelled.
pub async fn discover(sources: Sources, cancel: &CancellationToken) -> Option<Found> {
    let herdr = async {
        let mut found = Found::default();
        for path in sources.herdr.into_iter().take(MAX_HERDR_SERVERS) {
            match herdr::discover(&path).await {
                Ok(discovery) => {
                    found.herdr = true;
                    found.guarded |= discovery.guarded;
                    found.targets.extend(discovery.targets);
                }
                Err(error) => found.errors.push(error.to_string()),
            }
        }
        found
    };
    let claude = async {
        let Some(registry) = sources.claude else {
            return vec![];
        };
        tokio::task::spawn_blocking(move || {
            registry
                .lock()
                .map(|mut registry| registry.sessions())
                .unwrap_or_default()
        })
        .await
        .unwrap_or_default()
    };
    let (mut found, sessions) = tokio::select! {
        _ = cancel.cancelled() => return None,
        found = async { tokio::join!(herdr, claude) } => found,
    };
    found.targets = claude_sessions::merge(std::mem::take(&mut found.targets), &sessions);
    Some(found)
}

/// A boxed delivery future, so transports can be held behind one trait object.
pub type Delivering<'a> = Pin<Box<dyn Future<Output = SendOutcome> + Send + 'a>>;

/// A way to deliver a finished dictation to a target.
///
/// An implementation delivers once, to exactly the session the submission names, at the level
/// its [`Route`] promises: it re-checks that session immediately before writing and returns
/// [`SendOutcome::Rejected`] when anything differs, [`SendOutcome::Unconfirmed`] when some
/// input may have reached it, and it never writes the same submission again.
pub trait Transport: Sync {
    fn route(&self) -> Route;
    fn deliver<'a>(
        &'a self,
        submission: &'a Submission,
        cancel: &'a CancellationToken,
    ) -> Delivering<'a>;
}

/// Herdr's guarded prompt and receipt.
pub struct HerdrTransport;

impl Transport for HerdrTransport {
    fn route(&self) -> Route {
        Route::Herdr
    }
    fn deliver<'a>(
        &'a self,
        submission: &'a Submission,
        cancel: &'a CancellationToken,
    ) -> Delivering<'a> {
        Box::pin(herdr::submit(submission, cancel))
    }
}

/// Claude Code's own per-session inbox, at Queue level.
pub struct ClaudeInboxTransport;

impl Transport for ClaudeInboxTransport {
    fn route(&self) -> Route {
        Route::ClaudeInbox
    }
    fn deliver<'a>(
        &'a self,
        submission: &'a Submission,
        cancel: &'a CancellationToken,
    ) -> Delivering<'a> {
        Box::pin(claude_inbox::submit(submission, cancel))
    }
}

/// Every transport this build carries.
const TRANSPORTS: &[&dyn Transport] = &[&HerdrTransport, &ClaudeInboxTransport];

/// The transport for a route; `None` for draft-only targets and routes this build lacks.
pub fn transport(route: Route) -> Option<&'static dyn Transport> {
    TRANSPORTS.iter().copied().find(|t| t.route() == route)
}

/// Deliver once through the target's own transport. A target no transport reaches is rejected
/// without connecting anywhere.
pub async fn deliver(submission: &Submission, cancel: &CancellationToken) -> SendOutcome {
    match transport(submission.target.route()) {
        Some(transport) => transport.deliver(submission, cancel).await,
        None => SendOutcome::Rejected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mluva_core::agent_voice::{AgentKind, AgentStatus, HerdrPane};

    fn target(guard: &str) -> Target {
        Target {
            herdr: Some(HerdrPane {
                socket: "/nonexistent/fixture/herdr.sock".into(),
                server_epoch: "epoch".into(),
                pane_id: "w1:p1".into(),
                guard: guard.into(),
            }),
            name: "fixture".into(),
            kind: AgentKind::ClaudeCode,
            status: AgentStatus::Idle,
            session_id: "fixture-session".into(),
            pid: 4_100,
            ..Target::default()
        }
    }

    #[test]
    fn every_route_with_a_delivery_level_has_exactly_one_transport() {
        for route in [Route::DraftOnly, Route::Herdr, Route::ClaudeInbox] {
            let carried = TRANSPORTS.iter().filter(|t| t.route() == route).count();
            let expected =
                usize::from(route.delivery() != mluva_core::agent_voice::Delivery::DraftOnly);
            assert_eq!(carried, expected, "{route:?}");
        }
    }

    #[tokio::test]
    async fn a_target_no_transport_reaches_is_rejected_without_connecting() {
        for unreachable in [
            target(""),
            Target {
                herdr: None,
                ..target("")
            },
        ] {
            assert_eq!(unreachable.route(), Route::DraftOnly);
            let submission = Submission {
                id: "operation".into(),
                target: unreachable,
                text: "Synthetic message".into(),
            };
            assert_eq!(
                deliver(&submission, &CancellationToken::new()).await,
                SendOutcome::Rejected
            );
        }
    }

    #[test]
    fn the_summary_names_herdr_only_when_it_decides_sending() {
        let reachable = Found {
            targets: vec![target(&"a".repeat(64))],
            herdr: true,
            guarded: true,
            errors: vec![],
        };
        assert!(reachable.summary().starts_with("Local agents"));
        let old_herdr = Found {
            targets: vec![target("")],
            herdr: true,
            guarded: false,
            errors: vec![],
        };
        assert!(old_herdr.summary().contains("guarded prompt integration"));
        let outside = Found {
            targets: vec![Target {
                herdr: None,
                ..target("")
            }],
            ..Found::default()
        };
        assert!(outside.summary().contains("keeps a draft"));
        assert!(Found::default().summary().starts_with("No sessions found"));
    }
}
