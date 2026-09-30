# Omarchy keyboard recording and automatic paste

Verified on 30 September 2026. Three reproducible failures prevented automatic paste on the local installation:

- An Omarchy F9 binding to `mluva-shell record` started the bar/button copy-only path. Enabling automatic paste did not change that origin. Installed 1.5.4 produced no delivery target, a clipboard receipt and zero terminal paste events.
- Firefox 154 exposed an editable accessibility interface that silently ignored insertion. Mluva reported success while the independent page observer recorded unchanged text and zero input events.
- A tracker restarted while the field was already focused waited for a new focus event. The next recording captured no target and remained copy-only.

The fix exposes the existing global callback as `mluva-shell global-record`, keeps Firefox on the existing guarded keyboard path, accounts for its UTF-16 caret offsets, and seeds initial focus from one active external window. Later focus events remain authoritative. The bar/button `record` action remains copy-only, including when a different control stops that capture. Password, stale-target and ambiguity guards remain in effect; a truncated initial tree scan fails closed.

## Evidence and limits

The complete installed application capture callbacks, workflow, clipboard and delivery code ran against separate target processes in disposable Wayland sessions on this host. Audio capture and the speech response were synthetic. Early checks used the installed interpreter and layout with only the proposed modules overlaid read-only. Subsequent checks used the actual installed files without an overlay. The live installation was updated only when its status was idle, preserving settings and focus and restarting in the background.

Hyprland dispatched the configured command through its native Lua execution API; this is not a physical F9 invocation. Each case had a separate session bus, accessibility bus, XDG state and nested headless compositor. No host input devices or display sockets were exposed. Private virtual input prepared the synthetic targets. GTK and browsers observed their own text, browser observers also counted input events, and an isolated VS Code development extension observed its document change. Terminal observers counted bracketed paste events. Code and Firefox used private homes and profiles.

| Case | Independent target observation | App receipt |
| --- | --- | --- |
| Original installed 1.5.4, `record` | Initial terminal text unchanged; zero paste events | Copied; automatic paste not armed |
| Shortcut hotfix alone, Firefox INPUT | Field unchanged; zero input events | Incorrectly reported pasted |
| Tracker restarted on an already focused Firefox field, before startup fix | Field unchanged; zero input events | Copied; no target captured |
| GTK Entry, caret | Exact Czech Unicode and emoji at captured caret | Pasted and confirmed |
| GTK TextView, selection | Exact multiline replacement of selected text | Pasted and confirmed |
| Chromium INPUT | Exact text in the focused field | Pasted and confirmed |
| Chromium TEXTAREA | Exact multiline text; one input event | Pasted and confirmed |
| Chromium CONTENTEDITABLE | Exact text in the focused editable region | Pasted and confirmed |
| Firefox INPUT | Exact text; one input event | Pasted and confirmed |
| Firefox TEXTAREA | Exact multiline text; one input event | Pasted and confirmed |
| Firefox CONTENTEDITABLE | Exact text; one input event | Pasted and confirmed |
| Already focused Firefox field, restarted tracker with startup fix | Exact text; one input event | Pasted and confirmed |
| Already focused GTK field, restarted tracker with startup fix | Exact text at captured caret | Pasted and confirmed |
| VS Code document | Exact text; one document change | Pasted and confirmed |
| Foot terminal | Exact multiline text; one bracketed paste | Paste unconfirmed by the app |
| Ghostty terminal | Exact multiline text; one bracketed paste | Paste unconfirmed by the app |

Terminal receipts remain unconfirmed because Mluva does not read terminal contents. The independent synthetic observer establishes the result only for these acceptance cases. Other applications, terminal tabs/panes/input modes, actual microphone/provider operation and a physical key remain separate limits.

The local app and Omarchy widget remain version 1.5.4 with a task-branch hotfix; this patch is not yet a published release. F9 invokes `mluva-shell global-record`, portal shortcuts are disabled for the compositor binding, and Hyprland reports no configuration errors. All corrections are installed and running. The final installed files, without any overlay, also pass the restarted-tracker cases in Firefox and GTK. Physical user acceptance remains pending.

Regressions failed against the previous action map, Firefox routing and initial-focus behavior, then passed with the patch. `make linux-test` passes 615 tests, feature-document consistency, Ruff lint and formatting. Hosted CI is configured for manual invocation only and was not started.

The compact synthetic receipts are beside this report in `receipts.json`. Scratch harnesses, profiles, installation backups and logs remain under the worktree's `tmp/` and are not published. The Firefox findings are consistent with its [ATK mutation callbacks](https://github.com/mozilla-firefox/firefox/blob/main/accessible/atk/nsMaiInterfaceEditableText.cpp) and [native caret implementation](https://github.com/mozilla-firefox/firefox/blob/main/accessible/basetypes/HyperTextAccessibleBase.cpp).
