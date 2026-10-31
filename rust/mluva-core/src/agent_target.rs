//! One model for every agent session Mluva can dictate to, whatever runs it and however it is
//! reached.
//!
//! A [`Target`] is a running session: what it is ([`AgentKind`]), what it is called, what it is
//! doing ([`AgentStatus`]), where it runs and how a finished dictation reaches it ([`Route`] and
//! its [`Delivery`] level). Discovery and delivery live with each transport; the session wheel,
//! the routing owner, the command line, History and diagnostics read only this model, so a new
//! transport plugs in here and in its own module without changing them.
//!
//! Identity never comes from a name or a position. A target matches a remembered choice only by
//! the identity its transport vouches for ([`Target::same_instance`]), and a target that cannot
//! be reached is still shown, as draft only.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// The agent product running a session.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentKind {
    /// Claude Code in a terminal, a multiplexer pane or Herdr.
    #[serde(rename = "claude", alias = "claude_code")]
    ClaudeCode,
    /// A Code session of the Claude desktop app, which runs its own Claude Code.
    #[serde(rename = "claude_desktop")]
    ClaudeDesktop,
    /// The Codex command-line agent in a terminal, a multiplexer pane or Herdr.
    #[serde(rename = "codex")]
    CodexCli,
    /// A thread in the Codex desktop app.
    #[serde(rename = "codex_desktop")]
    CodexDesktop,
    /// Google Antigravity CLI in a terminal, a multiplexer pane or Herdr.
    #[serde(
        rename = "agy",
        alias = "antigravity",
        alias = "antigravity_cli",
        alias = "antigravity-cli"
    )]
    AntigravityCli,
    /// A kind this version does not know, kept so a newer saved choice still loads.
    #[default]
    #[serde(other, rename = "unknown")]
    Unknown,
}

impl AgentKind {
    /// The product's name as people say it.
    pub fn label(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::ClaudeDesktop => "Claude Desktop",
            Self::CodexCli => "Codex CLI",
            Self::CodexDesktop => "Codex Desktop",
            Self::AntigravityCli => "Antigravity CLI",
            Self::Unknown => "Agent",
        }
    }
    /// The identifier Herdr and the command line use: `claude`, `codex`, ...
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude",
            Self::ClaudeDesktop => "claude_desktop",
            Self::CodexCli => "codex",
            Self::CodexDesktop => "codex_desktop",
            Self::AntigravityCli => "agy",
            Self::Unknown => "unknown",
        }
    }
    /// Herdr's agent identifiers for the kinds it reports; anything else is not a target.
    pub fn from_herdr(agent: &str) -> Option<Self> {
        match agent {
            "claude" | "claude_code" => Some(Self::ClaudeCode),
            "codex" => Some(Self::CodexCli),
            "agy" | "antigravity" | "antigravity-cli" | "antigravity_cli" => {
                Some(Self::AntigravityCli)
            }
            _ => None,
        }
    }
    pub fn is_claude_code(self) -> bool {
        self == Self::ClaudeCode
    }
    pub fn is_antigravity(self) -> bool {
        self == Self::AntigravityCli
    }
    /// A Claude Code process, in a terminal or as a Code session of the Claude desktop app,
    /// which runs its own bundled Claude Code.
    pub fn runs_claude_code(self) -> bool {
        matches!(self, Self::ClaudeCode | Self::ClaudeDesktop)
    }
    /// A Codex thread, in a terminal or in the Codex desktop app.
    pub fn is_codex(self) -> bool {
        matches!(self, Self::CodexCli | Self::CodexDesktop)
    }
    /// A desktop app rather than a terminal session.
    pub fn is_desktop(self) -> bool {
        matches!(self, Self::ClaudeDesktop | Self::CodexDesktop)
    }
}

/// What a session is doing, as its own agent or Herdr reports it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Idle,
    Working,
    /// Waiting for the person: a permission prompt, a question or an open dialog.
    Blocked,
    Done,
    #[default]
    #[serde(other)]
    Unknown,
}

/// How far Mluva can take a finished dictation for a target, weakest first.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Delivery {
    /// Nothing reaches it: recording keeps an editable draft in Mluva.
    #[default]
    DraftOnly,
    /// Placed in the agent's composer; the person submits it there.
    Prefill,
    /// Accepted by the agent's own message queue for exactly that session; the agent takes it
    /// when it is ready, which may be after its current turn.
    Queue,
    /// Written and submitted to exactly that session, and acknowledged.
    Send,
}

impl Delivery {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DraftOnly => "draft_only",
            Self::Prefill => "prefill",
            Self::Queue => "queue",
            Self::Send => "send",
        }
    }
    /// The word for what happens, as the wheel and the panel print it.
    pub fn verb(self) -> &'static str {
        match self {
            Self::DraftOnly => "Draft only",
            Self::Prefill => "Prefill",
            Self::Queue => "Queue",
            Self::Send => "Send",
        }
    }
    /// History's outcome once the transport accepted a message at this level.
    pub fn outcome(self) -> &'static str {
        match self {
            Self::Send => "sent",
            Self::Queue => "queued",
            Self::Prefill => "prefilled",
            Self::DraftOnly => "draft-kept",
        }
    }
}

/// The transport that carries a dictation to a target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Route {
    /// No transport reaches the session.
    DraftOnly,
    /// Herdr's guarded prompt: the server re-checks its guard for the exact agent process and
    /// pane before writing, and returns a receipt for the operation.
    Herdr,
    /// Claude Code's own per-session inbox for cross-session messages. The session admits the
    /// message to its prompt queue and takes it between tool calls or as a new turn; the agent
    /// sees it as a message from another Claude session, not as typed by the person.
    ClaudeInbox,
    /// A new Claude desktop chat opened by link with the text in its composer, for the person
    /// to review and send. It raises the app, so it serves only a target chosen for it.
    ClaudeNewChat,
    /// Codex's own message queue for a live thread, sent if the thread is idle: the message is
    /// queued, confirmed as that thread's turn by its client ID, and withdrawn unsent if the
    /// thread does not take it at once.
    CodexQueue,
    /// Antigravity CLI's own per-session queue for cross-session messages. The session admits
    /// the message to its prompt queue and takes it as its next turn.
    AntigravityQueue,
}

impl Route {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DraftOnly => "none",
            Self::Herdr => "herdr",
            Self::ClaudeInbox => "claude_inbox",
            Self::ClaudeNewChat => "claude_new_chat",
            Self::CodexQueue => "codex_queue",
            Self::AntigravityQueue => "antigravity_queue",
        }
    }
    /// The level this transport has been verified to deliver at.
    pub fn delivery(self) -> Delivery {
        match self {
            Self::DraftOnly => Delivery::DraftOnly,
            Self::Herdr => Delivery::Send,
            Self::ClaudeInbox | Self::AntigravityQueue => Delivery::Queue,
            Self::ClaudeNewChat => Delivery::Prefill,
            Self::CodexQueue => Delivery::Send,
        }
    }
    /// Whether the transport may deliver to a session in this state. Herdr writes into a
    /// working agent's composer without stopping it but never answers a waiting prompt. The
    /// inbox queues behind a running turn or a waiting prompt, which it never answers. A new
    /// chat has no state.
    fn accepts(self, status: AgentStatus) -> bool {
        match self {
            Self::DraftOnly => false,
            Self::Herdr => matches!(
                status,
                AgentStatus::Idle | AgentStatus::Done | AgentStatus::Working
            ),
            Self::ClaudeInbox | Self::AntigravityQueue => matches!(
                status,
                AgentStatus::Idle | AgentStatus::Done | AgentStatus::Working | AgentStatus::Blocked
            ),
            Self::ClaudeNewChat => true,
            // Sending only if idle settles the state at send time, so even an unknown one is
            // offered; a busy thread keeps the draft.
            Self::CodexQueue => true,
        }
    }

    /// The longest text, in characters, the transport carries whole; longer text stays a
    /// draft rather than being cut.
    pub fn max_chars(self) -> Option<usize> {
        match self {
            // Claude Desktop cuts a link's prompt at this length.
            Self::ClaudeNewChat => Some(14_336),
            Self::DraftOnly
            | Self::Herdr
            | Self::ClaudeInbox
            | Self::CodexQueue
            | Self::AntigravityQueue => None,
        }
    }
}

/// A pane on a local Herdr server.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HerdrPane {
    /// The server's local socket.
    pub socket: String,
    pub server_epoch: String,
    pub pane_id: String,
    /// Herdr's opaque identity for the exact agent process and session in this pane. Empty when
    /// Herdr will not vouch for the session, which then cannot be sent to through Herdr.
    pub guard: String,
}

impl HerdrPane {
    fn same_pane(&self, other: &Self) -> bool {
        self.socket == other.socket
            && self.server_epoch == other.server_epoch
            && self.pane_id == other.pane_id
    }
}

/// A Claude Code session's inbox socket, as its registry entry named it when discovered. The
/// transport re-reads that entry and checks the connected process before it writes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClaudeInbox {
    /// The session's registry entry, `<config>/sessions/<pid>.json`.
    pub entry: String,
    /// The inbox socket the entry names.
    pub socket: String,
    /// The process start time the entry recorded, which `/proc` confirmed at discovery.
    pub proc_start: String,
}

/// A live Codex thread: the lock file whose exclusive hold makes it live, and its holder.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CodexThread {
    /// `$CODEX_HOME/thread-writer-locks/<thread>.lock`.
    pub lock: String,
    /// The process holding it: a Codex TUI, the shared daemon or Codex Desktop's server.
    pub holder: u32,
}

/// A live Antigravity CLI session: the presence lock file whose exclusive hold makes it live,
/// and its holder process.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AntigravitySession {
    /// The conversation ID.
    pub conversation_id: String,
    /// Path to the presence lock file (`~/.gemini/antigravity-cli/presence/<conversation_id>.lock`).
    pub lock: String,
    /// The process holding it: an active `agy` process.
    pub holder: u32,
    /// The holder's start time as recorded by `/proc`, for identity re-checking.
    #[serde(default)]
    pub proc_start: String,
}

/// An application surface reached by link rather than a running session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppLink {
    /// A new chat in the Claude desktop app.
    ClaudeNewChat,
}

/// Where a session outside Herdr runs, as its agent reports it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Host {
    /// A terminal Mluva cannot name.
    #[default]
    Terminal,
    /// A tmux pane, as `<session>:@<window>.%<pane>`.
    Tmux(String),
    /// A background session with no terminal (`claude --bg`).
    Background,
    /// Hosted by an application rather than a terminal, such as an editor or the Agent SDK.
    App,
    /// Held open by Codex's shared app-server daemon, for a TUI attached to it.
    Daemon,
}

/// A running agent session, wherever it runs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "TargetRecord", into = "TargetRecord")]
pub struct Target {
    pub kind: AgentKind,
    /// A short handle: Herdr's agent label (used by `mluva agents select`), else the agent's
    /// own generated handle, else the pane.
    pub name: String,
    /// The agent's own session title: a Codex thread name, a Claude Code session title. Empty
    /// until the agent names the session.
    pub title: String,
    pub project: String,
    pub status: AgentStatus,
    /// The agent's own session or thread ID.
    pub session_id: String,
    /// The agent process, when known; 0 otherwise.
    pub pid: u32,
    /// That process's start time as the kernel counts it, so a reused PID is told apart; empty
    /// when unknown.
    pub started: String,
    /// The Herdr pane the session runs in, when Herdr lists it.
    pub herdr: Option<HerdrPane>,
    /// Claude Code's inbox for the session, when its registry names one. Never saved.
    pub inbox: Option<ClaudeInbox>,
    /// Where the session runs when Herdr does not show it. Never saved.
    pub host: Host,
    /// An application surface reached by link, such as a new Claude desktop chat.
    pub app: Option<AppLink>,
    /// The live Codex thread, when its writer lock is held. Never saved.
    pub codex: Option<CodexThread>,
    /// A live Antigravity CLI session, when its presence lock is held. Never saved.
    pub antigravity: Option<AntigravitySession>,
    /// A remembered choice that discovery no longer reports. Never saved.
    pub missing: bool,
}

impl Target {
    /// The strongest transport that would carry a dictation, whatever the session is doing
    /// now: Herdr's guarded prompt, else Claude Code's inbox.
    pub fn route(&self) -> Route {
        if self.guarded_pane().is_some() {
            Route::Herdr
        } else if self.inbox.is_some() && self.kind.runs_claude_code() {
            Route::ClaudeInbox
        } else if self.codex.is_some() && self.kind.is_codex() {
            Route::CodexQueue
        } else if self.antigravity.is_some() && self.kind.is_antigravity() {
            Route::AntigravityQueue
        } else if self.app == Some(AppLink::ClaudeNewChat) && self.kind == AgentKind::ClaudeDesktop
        {
            Route::ClaudeNewChat
        } else {
            Route::DraftOnly
        }
    }

    /// The strongest level a dictation reaches this target at.
    pub fn delivery(&self) -> Delivery {
        self.route().delivery()
    }

    /// A transport reaches the session, whether or not it can take a message right now.
    pub fn has_transport(&self) -> bool {
        self.route() != Route::DraftOnly
    }

    /// The session is present and its transport may deliver now.
    pub fn can_receive(&self) -> bool {
        let route = self.route();
        !self.missing && self.kind != AgentKind::Unknown && route.accepts(self.status)
    }

    /// Present, but nothing can deliver to it: recording keeps an editable draft in Mluva.
    pub fn draft_only(&self) -> bool {
        !self.missing && !self.has_transport()
    }

    /// Why a draft-only session cannot be delivered to.
    pub fn draft_hint(&self) -> &'static str {
        if self.herdr.is_some() {
            // Herdr withholds a guard until it confirms the session and sees it ready.
            "Draft only: Herdr can't send to it now"
        } else if self.kind.runs_claude_code() {
            // Older, `--bare` and `--restricted` sessions open no inbox.
            "Draft only: this session has no inbox"
        } else if self.kind.is_antigravity() {
            "Draft only: this session has no queue"
        } else {
            "Draft only: nothing can reach it"
        }
    }

    /// What the route does with a message when it is not a plain send; `None` for Send and for
    /// draft-only targets, whose hint is [`Self::draft_hint`].
    pub fn delivery_hint(&self) -> Option<&'static str> {
        match self.route() {
            Route::DraftOnly | Route::Herdr => None,
            Route::ClaudeInbox if self.status == AgentStatus::Blocked => {
                Some("Waits behind its prompt, as a message from another Claude session")
            }
            Route::ClaudeInbox => Some("Arrives as a message from another Claude session"),
            Route::ClaudeNewChat => Some("Opens Claude with it in a new chat; you send it there"),
            Route::CodexQueue => Some("Sends if the thread is idle; otherwise your draft is kept"),
            Route::AntigravityQueue if self.status == AgentStatus::Blocked => {
                Some("Waits behind its prompt, as a message in Antigravity's queue")
            }
            Route::AntigravityQueue => Some("Arrives as a message in Antigravity's queue"),
        }
    }

    /// The same running session, whatever route reaches it now. A session is its product, its
    /// own session or thread ID and its process (PID, and start time when both sides know it),
    /// so Herdr's guard appearing or going away keeps the choice, while a restarted, resumed or
    /// cleared session, or a new session in the same pane, never takes it over. Without a native
    /// session on both sides, such as a choice saved before sessions were known, Herdr's own
    /// identity decides: its guard when Herdr vouches for both, else the pane when neither has a
    /// session. A link to an app surface is that surface.
    pub fn same_instance(&self, other: &Self) -> bool {
        if self.app.is_some() || other.app.is_some() {
            return self.app == other.app && self.kind == other.kind;
        }
        match (self.session_id.is_empty(), other.session_id.is_empty()) {
            (false, false) => {
                self.kind == other.kind
                    && self.session_id == other.session_id
                    && self.pid == other.pid
                    && (self.started.is_empty()
                        || other.started.is_empty()
                        || self.started == other.started)
            }
            (no_session, other_no_session) => match (self.guarded_pane(), other.guarded_pane()) {
                (Some(a), Some(b)) => a.same_pane(b) && a.guard == b.guard,
                (None, None) if no_session && other_no_session => {
                    match (&self.herdr, &other.herdr) {
                        (Some(a), Some(b)) => a.same_pane(b),
                        _ => false,
                    }
                }
                _ => false,
            },
        }
    }

    /// The Herdr pane, when Herdr vouches for the session in it.
    fn guarded_pane(&self) -> Option<&HerdrPane> {
        self.herdr.as_ref().filter(|pane| !pane.guard.is_empty())
    }

    /// The name a person recognizes: the agent's own session title, else the handle.
    pub fn display_name(&self) -> &str {
        if self.title.trim().is_empty() {
            &self.name
        } else {
            &self.title
        }
    }

    /// What tells this session apart from another with the same name: its Herdr or tmux pane,
    /// else the agent's own handle, else its process.
    pub fn locator(&self) -> String {
        if let Some(pane) = &self.herdr {
            pane.pane_id.clone()
        } else if let Host::Tmux(pane) = &self.host {
            pane.clone()
        } else if self.name != self.display_name() {
            self.name.clone()
        } else if self.pid != 0 {
            format!("pid {}", self.pid)
        } else {
            self.session_id.clone()
        }
    }

    /// Where the session runs, in a few words: `Herdr w1:p2`, `tmux work:@3.%7`, `terminal`.
    pub fn place(&self) -> String {
        if let Some(pane) = &self.herdr {
            return format!("Herdr {}", pane.pane_id);
        }
        if self.app.is_some() {
            return "app".into();
        }
        match &self.host {
            Host::Terminal => "terminal".into(),
            Host::Tmux(pane) => format!("tmux {pane}"),
            Host::Background => "background".into(),
            Host::App => "app".into(),
            Host::Daemon => "Codex daemon".into(),
        }
    }

    /// Where to look after an uncertain delivery: `Herdr`, or the session itself.
    pub fn place_to_check(&self) -> &'static str {
        match self.route() {
            Route::Herdr => "Herdr",
            Route::ClaudeNewChat => "Claude",
            Route::CodexQueue => "the Codex thread",
            Route::ClaudeInbox | Route::AntigravityQueue | Route::DraftOnly => "the session",
        }
    }

    /// The project folder's own name, for menus and tooltips.
    pub fn project_name(&self) -> &str {
        Path::new(&self.project)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Local")
    }

    /// A second line for tooltips: product, place, handle and project.
    pub fn whereabouts(&self) -> String {
        match &self.herdr {
            Some(pane) => format!(
                "{} · {} · Herdr {} · {}",
                self.kind.label(),
                pane.pane_id,
                self.name,
                self.project_name()
            ),
            None => format!(
                "{} · {} · {} · {}",
                self.kind.label(),
                self.locator(),
                self.place(),
                self.project_name()
            ),
        }
    }

    /// Whether a command-line query names this target: its handle, its Herdr pane or its
    /// native session ID. Titles are not matched; they change and repeat.
    pub fn answers_to(&self, query: &str) -> bool {
        self.name == query
            || self
                .herdr
                .as_ref()
                .is_some_and(|pane| pane.pane_id == query)
            || (!self.session_id.is_empty() && self.session_id == query)
    }

    /// Metadata-only status sentence; it deliberately makes no claim about a particular turn.
    pub fn spoken_status(&self) -> String {
        let name: String = self
            .display_name()
            .chars()
            .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
            .take(48)
            .collect();
        let name = if name.trim().is_empty() {
            "Your agent"
        } else {
            name.trim()
        };
        match self.status {
            AgentStatus::Idle => format!("{name} is ready for your next message."),
            AgentStatus::Working => {
                format!("{name} is working; its latest result is not confirmed yet.")
            }
            AgentStatus::Blocked if self.herdr.is_some() => {
                format!("{name} needs your attention in Herdr before it can continue.")
            }
            // Answered where the session runs.
            AgentStatus::Blocked => {
                format!("{name} needs your attention before it can continue.")
            }
            AgentStatus::Done => {
                format!("{name} has finished a turn; open its session to review the result.")
            }
            AgentStatus::Unknown => {
                format!("{name} is unavailable or its current activity is not clear yet.")
            }
        }
    }
}

/// The saved shape of a target. Herdr's fields stay flat, as choices saved before targets
/// existed were written, so those load unchanged and an older Mluva can still read new ones.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct TargetRecord {
    socket: String,
    server_epoch: String,
    pane_id: String,
    guard: String,
    name: String,
    title: String,
    kind: AgentKind,
    project: String,
    status: AgentStatus,
    session_id: String,
    pid: u32,
    #[serde(skip_serializing_if = "String::is_empty")]
    started: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    app: Option<AppLink>,
}

impl From<TargetRecord> for Target {
    fn from(record: TargetRecord) -> Self {
        let in_herdr = !record.socket.is_empty() || !record.pane_id.is_empty();
        Self {
            herdr: in_herdr.then_some(HerdrPane {
                socket: record.socket,
                server_epoch: record.server_epoch,
                pane_id: record.pane_id,
                guard: record.guard,
            }),
            kind: record.kind,
            name: record.name,
            title: record.title,
            project: record.project,
            status: record.status,
            session_id: record.session_id,
            pid: record.pid,
            started: record.started,
            inbox: None,
            host: Host::default(),
            app: record.app,
            codex: None,
            antigravity: None,
            missing: false,
        }
    }
}

impl From<Target> for TargetRecord {
    fn from(target: Target) -> Self {
        let pane = target.herdr.unwrap_or_default();
        Self {
            socket: pane.socket,
            server_epoch: pane.server_epoch,
            pane_id: pane.pane_id,
            guard: pane.guard,
            name: target.name,
            title: target.title,
            kind: target.kind,
            project: target.project,
            status: target.status,
            session_id: target.session_id,
            pid: target.pid,
            started: target.started,
            app: target.app,
        }
    }
}
