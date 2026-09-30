# Automatic paste verification

Verified on 30 September 2026 against remote `main` at `113f10f`. This change fixes missing terminal capture on Hyprland, clipboard-versus-primary-selection terminal bindings, and X11 delivery when Wayland tools are also installed. It is a draft change; no merge, release or application installation was performed.

## Real target acceptance

Each target ran in a separate real process. The controller used production `FocusedTextTargetTracker`, captured the focused target, called `restore()`, and passed its insertion, revalidation and confirmation callbacks to production `deliver_text()`. No fake clipboard or input injector was used. Target observations, rather than the delivery receipt alone, had to match the entire expected text exactly.

| Target | Session and insertion route | Independent observation |
| --- | --- | --- |
| GTK Entry, caret and selected text | Private Wayland; native AT-SPI editing | Target process read the final text and caret; exact insertion and selection replacement. |
| GTK TextView, caret and selected text | Private Wayland; native AT-SPI editing | Target process read the final multiline buffer; exact insertion and selection replacement. |
| Chromium input | Private Wayland and private X11; symbolic keyboard paste | Local fixture's input event reported the exact final value. |
| Chromium textarea | Private Wayland; symbolic keyboard paste | Local fixture's input event reported the exact multiline value. |
| Chromium contenteditable | Private Wayland; symbolic keyboard paste | Local fixture's input event reported the exact final text, including the existing trailing space. |
| VS Code editor | Private Wayland; symbolic keyboard paste | Saved synthetic document matched the exact expected text. |
| Foot and Ghostty, default bindings | Private Wayland; Hyprland terminal capture and Ctrl+Shift+V | Raw TTY child received exactly one bracketed paste containing the expected multiline UTF-8 text. It did not execute shell input. |

The [target receipts](target-receipts.json) contain only synthetic text and observations. The payload was `Příliš žluťoučký kůň 🐎`, with `\nDruhý řádek.` for multiline cases. Caret cases retained `Mluva: `; selection cases replaced `replace` in `Before replace after`. Exact matching rejects duplicated insertion as well as lost characters. Terminal observations proved delivery in the fixture, while production receipts correctly remained `paste-unconfirmed` because Mluva does not read terminal contents.

The Wayland fixtures ran in nested Hyprland under headless Cage, with Bubblewrap exposing only a render node and private display, D-Bus, AT-SPI and XDG state. No physical input devices, host compositor sockets or live clipboard were exposed. X11 fixtures used the repository's `dev/run-isolated.sh` and Xvfb. Chromium and VS Code used fresh isolated profiles with extensions disabled; Chromium's page was served on loopback. The accessibility broker and registry ran on the private session bus using `ATSPI_DBUS_IMPLEMENTATION=dbus-daemon`. A persistent virtual keyboard kept the headless Wayland seat present while testing Chromium.

Observed versions: Hyprland 0.56.2, GTK 4.22.4, AT-SPI 2.60.6, wtype 0.4, Chromium 151.0.7922.173, VS Code 1.137.0, Foot 1.27.0 and Ghostty 1.3.1. Wayland fixtures disabled XWayland. The X11 Chromium check ran with both Wayland and X11 toolkits available, so it exercised backend selection rather than hiding the wrong helper.

## Regression and repository checks

| Check | Result |
| --- | --- |
| Baseline terminal capture | On `113f10f`, a mapped and focused private Foot window produced no delivery target and no usable AT-SPI text focus. The fixed version captured its exact window/process identity and delivered once. |
| Backend regression on baseline | The new backend test failed both explicit and inferred X11 cases on the old delivery module: it chose `wl-copy` instead of `xclip`. The Wayland case passed. All three cases pass with the fix. |
| Terminal identity changes | Tests verify clipboard recovery with no input dispatch after window, PID or executable changes, unmapping, absent window data or compositor disconnection. Ordinary windows cannot authorize terminal paste through a spoofed title/class. |
| Terminal routing | All seven recognized executable names are covered: Foot, footclient, Alacritty, Ghostty, Kitty, WezTerm and wezterm-gui. Only Foot and Ghostty were executed as real terminal targets. |
| `make linux-test linux-shortcut-test linux-text-target-test` | 605 tests, feature-document consistency, Ruff lint/format, private portal registration/binding/lifecycle, and native cross-process X11 Unicode insertion/confirmation pass. |
| Pyright on delivery and the new terminal module | Zero errors and warnings. |
| Repository ShellCheck and `git diff --check` | Pass. ShellCheck ran through an ephemeral `uv run --no-project --with shellcheck-py` environment. |

The repeatable repository checks are:

```bash
make linux-test linux-shortcut-test linux-text-target-test
uv run --no-project --with shellcheck-py shellcheck install.sh linux/*.sh linux/tests/*.sh dev/*.sh linux/mluva-shell
git diff --check
```

For a real-target replay, create the synthetic target in a disposable desktop, start the production tracker before focusing it, position the caret or selection as described above, and deliver the synthetic payload with the captured callbacks. Observe the target from its own process or saved file. The local acceptance scripts and complete logs are retained under the task worktree's ignored `tmp/`; they are host-specific fixtures, not a new supported test runner. Deterministic regression coverage lives in `linux/tests/test_delivery.py` and `linux/tests/test_terminal_target.py`.

## Limits

These checks prove delivery into the listed real targets in isolated sessions. They do not test physical F9, real speech providers, Fedora GNOME/Mutter, arbitrary application accessibility, custom terminal bindings, the optional ydotool daemon, or tabs/panes/input-mode changes inside the same terminal window. Terminal paste remains one attempt and unconfirmed; moved or stale targets remain copy-only. The installed app and Daniel's live desktop were untouched.

The native X11 smoke emitted existing headless systemd, DRI3 and AT-SPI cache warnings while all target assertions passed. The deterministic suite emitted one existing PyGObject deprecation warning. Hosted CI was not triggered; the repository workflow is manual.
