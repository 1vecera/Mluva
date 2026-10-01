# Native Rust implementation

This workspace is the in-progress replacement for the released 1.6.0 application. `mluva-core` implements settings and migrations, deterministic spoken commands, SQLite history/conversations, retention and exports, screenshot ownership and frozen visual input, prompt/style overrides, bounded titles, unresolved Scratchpad recovery, personalization, vocabulary suggestions, Markdown parsing and source-preserving word edits. `mluva-gtk` provides the native application window, command catalog/search, settings navigation, document widget, personalization page, shared prompt editor/discovery and theme controller. The complete application, capture/providers, local inference, delivery and distribution remain tracked in [the acceptance matrix](../docs/rust-port-parity.md).

Build with Rust 1.95 and the platform development libraries for SQLite, GTK 4.12 or later, Libadwaita 1.5 or later and Fontconfig. The private input comparison example also needs X11 and Xtst. From the repository root:

```sh
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

The checks operate on synthetic fixtures and temporary files. For task-local scratch storage, create `tmp/native-tests` and set `TMPDIR` to its absolute path when running tests. Cargo tests use the [frozen released core outputs](mluva-core/tests/fixtures/README.md) and [native widget observations](mluva-gtk/tests/fixtures/README.md) without invoking an interpreter or importing the reference application. The installed 1.6.0 application remains the usable reference while parity is established.

The desktop tests are ignored by ordinary Cargo runs because they must have an isolated display, session bus, accessibility registry and data directories. Build the guarded private input helper, then run the tests on an unused private display with the dependencies checked by `dev/run-isolated.sh`:

```sh
cargo build --locked -p mluva-gtk --example private_input
test ! -e /tmp/.X11-unix/X173
env OFFSCREEN_ENABLE_ATSPI=1 OFFSCREEN_DISPLAY_NUMBER=173 \
  XDG_CURRENT_DESKTOP=offscreen GDK_SCALE=1 GDK_DPI_SCALE=1 \
  dev/run-isolated.sh tmp/native-document-widget -- \
  cargo test --locked -p mluva-gtk --tests -- \
  --ignored --test-threads=1
```

Serialize desktop comparisons. Run the released reference and native surface sequentially inside one private session; automatic Xvfb display selection can race between concurrent helpers. The fixture records GTK 4.22.4 and Pango 1.58.2, and the test explicitly refuses changed renderer versions until new observations are collected from the unchanged release. Mixed CR/LF input currently triggers the same Pango cursor warning in both implementations; the recorded source, text, tags, layout and selection observations still match. This component evidence does not establish complete application parity or a performance improvement.

The prompt editor test also pins Libadwaita 1.9.3. Its 81 native observations cover Save/Cancel, restore, damaged files, external conflicts, privacy changes and retention of an open draft when another caller requests a prompt. The shared catalog updates saved-style previews without changing recovery baselines or user JSON. A separate external-input comparison records ten rendered states with motion and cursor blinking disabled: nine match exact RGBA8 pixels; the initial standard maximize icon retains a documented one-channel difference on 16 pixels. Full animation, application integration and other sizes/scales remain pending.

The application shell test adds 99 independent observations of command availability, search/keyboard dispatch, preference navigation, adaptive window sizing, modal guards, hide/reopen and prompt opening inside the window. It also checks 13 public action names/types and two actual cross-process action receipts on its private session bus. Capture, welcome, meeting, history and Workspace/Capture preference contents are synthetic boundaries in this component comparison; their complete workflows remain pending. Four of eight rendered window states match RGBA8 exactly; the others differ at 3–23 pixels by at most two channel units. The [curated evidence](mluva-gtk/tests/fixtures/application-shell-desktop-evidence.json) retains these differences. There is no complete runnable native app or installed Rust replacement yet.
