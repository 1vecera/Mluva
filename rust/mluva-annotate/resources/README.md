# Scrawl provenance

The mark classification in `src/sketch.rs`, the clause alignment and spoken-cue rules in `src/timing.rs` and `src/keyframes.rs`, and the keyframe budget, clock format and mark shapes in `src/model.rs` are adapted from [Scrawl](https://github.com/kitlangton/scrawl) by Kit Langton at commit [`f2ad4e8c75b6cc6171c38d85cd8755e4d9fe03fe`](https://github.com/kitlangton/scrawl/tree/f2ad4e8c75b6cc6171c38d85cd8755e4d9fe03fe) (`src/sketch.rs`, `src/keyframes.rs`, `src/model.rs`). Scrawl is MIT licensed; its notice is [scrawl-LICENSE](scrawl-LICENSE) and travels with the native package as `third-party-licenses/scrawl/LICENSE`.

Mluva keeps Scrawl's stroke, loop, arrow and clause thresholds. Its own code replaces Scrawl's video pipeline: keyframes come from sparse screen frames captured while recording, rendering uses Cairo and Pango, and the bundle layout is Mluva's.
