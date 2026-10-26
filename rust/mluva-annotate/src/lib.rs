//! Screen annotation for one recording: classify the marks drawn while speaking, choose the
//! keyframes an agent should see, match each one to what was said, and write a local bundle.
//!
//! Mark classification, clause alignment and the spoken-cue and novelty rules are adapted from
//! Scrawl by Kit Langton (MIT); see `resources/README.md` for the pinned upstream commit.
pub mod bundle;
pub mod geometry;
pub mod keyframes;
pub mod model;
#[cfg(feature = "render")]
pub mod render;
pub mod sketch;
pub mod timing;
