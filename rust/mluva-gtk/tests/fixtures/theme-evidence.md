# Native theme lifecycle

`theme_contracts.rs` owns both the existing stylesheet/parser contract and the real open-window lifecycle. The latter replaces `linux/tests/theme_ui_smoke.py` and consolidates the former scheme rewrite/removal/drop block from `document_widget.rs`. Production theme, CSS, fonts and application code are unchanged.

## Independent reference

`released-theme-lifecycle.json` records unchanged v1.6.0 at `5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f`, GTK 4.22.4, the exact installed Tokyo Night/Rose Pine input palettes and ten observed states. Its SHA-256 is `7f289e9bd6df5e20fba9693ba4be5543596f2386fbe40c0d21d5bd5f6d8d6293` (14,311 bytes/238 lines). The temporary collector checks every released Python module and bundled font against the pinned Git blobs before loading, then checks the actual imported module paths and hashes afterward. It does not alter the released controller or its timer/signal behavior. Reference output is `tmp/native-theme-reference.log` and `tmp/native-theme-reference-evidence/`.

Nine complete open-window observations compare actual Adwaita scheme/darkness, five applied GTK named colors and allocated dimensions. They cover initial system light, both symlink destinations, malformed data, rewritten palette, file removal, system dark/light changes while the palette is absent and recovery. The controller's real timer notices filesystem changes; the test never calls a private reload function. A second matching observation and a mapped window precede each capture.

The tenth released state observes no reload after `close`. Native Drop already additionally removes its CSS provider and restores the prior scheme; these native cleanup semantics predate this conversion. The check compares the shared post-close scheme/darkness, checks provider removal separately and retains the previous native dark-to-prior-scheme restoration check. It does not claim identical still-open-window CSS after controller teardown.

## Native execution

Use the existing guarded runner with its documented dependencies, plus ImageMagick `import`:

```sh
export CARGO_HOME="$(realpath -m -- "${CARGO_HOME:-$HOME/.cargo}")"
export RUSTUP_HOME="$(realpath -m -- "${RUSTUP_HOME:-$HOME/.rustup}")"
export CARGO_TARGET_DIR="$(realpath -m -- "${CARGO_TARGET_DIR:-$PWD/tmp/native-build}")"
cargo test --locked -p mluva-gtk --test theme_contracts --test document_widget --no-run
bash dev/run-isolated-browser.sh tmp/native-theme -- \
  cargo test --locked -p mluva-gtk --test theme_contracts --test document_widget -- \
  --include-ignored --test-threads=1
```

The native owner verifies private HOME/XDG/Xauthority, X11 without a host Wayland signature, a different network namespace and masked input/audio/display devices. Only private files, display and buses are used. Palettes are frozen inputs; no installed Omarchy theme or live configuration is modified. The named-color lookup uses GTK's deprecated public API to observe installed CSS; it is not a production hook.

The final focused run passes all three tests with Python and uv blocked and both interpreter traps positively checked: the existing six stylesheet/seven palette cases, all 56 document-widget observations and the new live theme comparison. Evidence is `tmp/native-theme-reviewed.log` and `tmp/native-theme-reviewed-evidence/session.2sS5og/native-theme/`. The known document-widget mixed-CR/LF Pango warning remains. Earlier native evidence is `tmp/native-theme-focused.log`; its widget-texture captures were replaced with actual X11 surface captures to match the reference's border/alpha behavior.

All nine final 520×200 X11 PNGs have zero differing pixels against their corresponding released captures, recorded in `tmp/native-theme-pixel-comparison.log`. Dark/light captures were inspected for text, control bounds and colors. Animations/cursor blinking were disabled equally. This establishes the synthetic component window at this renderer/scale, not full application, other viewports, native accessibility, animation timing or Wayland theme acceptance.

Full `make test` passes 139 tests, zero failures and 68 ignored environment checks across 98 suites, both strict all-target Clippy configurations, formatting and generated-document consistency. `make linux-python-test` passes 571 tests, Ruff and formatting for 160 files, retaining the existing PyGObject warning. Logs are `tmp/native-theme-final-native.log`, `tmp/native-theme-final-python.log` and `tmp/native-theme-final-gates.log`; the guarded driver finishes successfully in `tmp/native-theme-final-gates-evidence/session.j7gck7/`. ShellCheck and diff checks pass. No hosted run or new performance claim is made.

## Retired coverage

The removed Python helper's `main` (72 lines, introduced by `30d552a`, renamed by `19c7579`) protected live symlink switching, canvas/scheme application, malformed recovery and hook shutdown. Repository callers were its direct documented invocation; there is no import or shared helper to retain. The native theme owner now covers these observable outcomes without inspecting private timer/signal fields. The 37-line native lifecycle block formerly appended to the document test is consolidated here rather than repeated. The Python theme implementation and its two parser tests remain because the still-maintained Python application calls them. Discovery is recorded in `tmp/native-theme-review.md`.

Final review leaves production/tooling unchanged; test/support adds 264 and removes 111 lines, with the independent 238-line data fixture counted separately. The shared theme/style parser remains the single owner of exact stylesheet and validation expectations. No test-only production API or compatibility alias was introduced.

Full Python removal and whole-application/theme/scale/accessibility/Wayland acceptance remain open. The protected installed app/widget remain 1.6.0, and every whole-workflow parity row remains Pending.
