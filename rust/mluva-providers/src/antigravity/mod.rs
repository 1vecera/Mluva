//! Antigravity CLI per-session discovery, conversation titles and message queue.
pub mod queue;
pub mod sessions;
pub mod titles;

pub use queue::submit;
pub use sessions::{AntigravitySessions, merge};
pub use titles::{AntigravityTitles, antigravity_data_dir};
