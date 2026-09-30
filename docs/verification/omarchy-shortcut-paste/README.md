# Omarchy keyboard recording and automatic paste

Verified on 30 September 2026. An Omarchy F9 binding to `mluva-shell record` starts the bar/button capture path, which deliberately copies only. Enabling automatic paste does not change that origin. The installed 1.5.4 app reproduced the failure: no delivery target captured, clipboard receipt, zero terminal paste events.

The fix exposes the existing global recording callback as `mluva-shell global-record`. A Hyprland key can now start an eligible capture without a portal session or presenting Mluva's window. The app and widget's `record` action remains copy-only. Stopping through a different control preserves the original capture choice.

## Evidence and limits

The full installed application capture callbacks, workflow, clipboard and delivery implementations ran against separate target processes in disposable Wayland sessions on this host. Audio capture and the speech response were synthetic. The installed layout and interpreter were used; a read-only filesystem overlay replaced only `app.py` and `shell_bridge.py` with the proposed patch. The existing live application and its active recording were untouched during these checks.

Hyprland dispatched the configured command through its native Lua execution API; this is not a physical F9 invocation. Each case had a separate session bus, accessibility bus, XDG state and nested headless compositor. No host input devices or display sockets were exposed. The synthetic terminal observer counted bracketed paste events; GTK and Chromium observed their own text, and an isolated VS Code development extension observed its document change. The Code CLI also used a private home, profile and extension directory.

| Case | Independent target observation | App receipt |
| --- | --- | --- |
| Unpatched installed 1.5.4, copy-only `record` | Initial terminal text unchanged; zero paste events | Copied; automatic paste not armed |
| GTK Entry, caret | Exact Czech Unicode and emoji at captured caret | Pasted and confirmed |
| GTK TextView, selection | Exact multiline replacement of selected text | Pasted and confirmed |
| Chromium INPUT | Exact text in the focused field | Pasted and confirmed |
| Chromium TEXTAREA | Exact multiline text in the focused field | Pasted and confirmed |
| Chromium CONTENTEDITABLE | Exact text in the focused editable region | Pasted and confirmed |
| VS Code document | Exact text; one document change | Pasted and confirmed |
| Foot terminal | Exact multiline text; one bracketed paste | Paste unconfirmed by the app |
| Ghostty terminal | Exact multiline text; one bracketed paste | Paste unconfirmed by the app |

The terminal receipts correctly remain unconfirmed because Mluva does not read terminal contents. The independent synthetic terminal observer establishes the result only for these acceptance cases. Tabs, panes, input mode, other terminal executables, actual microphone/provider operation and the physical key remain separate limits. The live installation is pending completion of its active recording; these receipts do not claim it has already been replaced.

The unit regression failed against the previous action map and bridge allowlist, then passed with the patch. It exercises the real application action objects and prevents a Stop action from changing a copy-only capture into an automatic paste. `make linux-test` passes all 609 tests, feature-document consistency, Ruff lint and formatting. Hosted CI is configured for manual invocation only and was not started.

The compact synthetic receipts are beside this report in `receipts.json`. The scratch harness, private profiles and logs remain under the task worktree's `tmp/`; they are not installed into the user session.
