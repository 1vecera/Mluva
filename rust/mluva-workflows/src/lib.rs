//! Recording orchestration shared by the native desktop and background capture.
//!
//! Each completed recording preserves immutable recognition, applies the frozen
//! capture rules, attempts delivery once, and records independent recovery state.

pub mod capture;
pub mod dictation;
pub mod meeting;
pub mod meeting_services;
pub mod meeting_session;
pub mod preparation;
pub mod screenshot_capture;
pub mod segment_cleanup;
pub mod services;
