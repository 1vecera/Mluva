//! Recording orchestration shared by the native desktop and background capture.
//!
//! Each completed recording preserves immutable recognition, applies the frozen
//! capture rules, attempts delivery once, and records independent recovery state.

pub mod dictation;
pub mod preparation;
pub mod segment_cleanup;
