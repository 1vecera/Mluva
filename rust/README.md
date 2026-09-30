# Native Rust implementation

This workspace is the in-progress replacement for the released 1.6.0 application. `mluva-core` implements settings and migrations, deterministic spoken commands, SQLite history/conversations, retention and exports, screenshot ownership and frozen visual input, prompt/style overrides, bounded titles and unresolved Scratchpad recovery. The complete native UI, capture/providers, local inference, delivery and distribution remain tracked in [the acceptance matrix](../docs/rust-port-parity.md).

Build with Rust 1.95 and the platform SQLite development library. From the repository root:

```sh
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

The checks operate on synthetic fixtures and temporary files. For task-local scratch storage, create `tmp/native-tests` and set `TMPDIR` to its absolute path when running tests. Cargo tests use the [frozen released outputs](mluva-core/tests/fixtures/README.md) without invoking an interpreter or importing the reference application. The installed 1.6.0 application remains the usable reference while parity is established.
