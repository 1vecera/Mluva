//! Codex threads as targets, reached through Codex's own message queue.
//!
//! [`threads`] finds live threads by their held writer locks, [`server`] chooses the app server
//! Mluva talks to, [`rpc`] speaks its JSON-RPC, and [`queue`] sends a message to a thread only
//! if the thread takes it at once. The app-server protocol is documented; the queue methods are
//! experimental, and the lock directory and session index are Codex's internal files, so every
//! step is checked and anything unexpected leaves the thread draft only.
pub mod queue;
pub mod rpc;
pub mod server;
#[cfg(test)]
mod test_peer;
pub mod threads;

use std::path::PathBuf;

/// `$CODEX_HOME`, defaulting to `~/.codex`.
pub fn codex_home() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".codex")))
}
