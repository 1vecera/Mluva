# Real application verification

Use this harness to reproduce a native Mluva journey through a separate CLI, with synthetic content and retained evidence. It launches the production GTK executable, calls its existing application actions over a private D-Bus connection, and inspects/edits the real GTK widgets through AT-SPI. It is development tooling; it adds no installed control service or production test mode.

## Run

The normal native dependencies and [isolated desktop prerequisites](README.md#isolated-ui-verification) are required, plus `jq` and ImageMagick (`import`, `identify`). The reused runner also checks Firefox, Openbox, `xdotool`, `xclip` and `xprop`; this journey does not launch a browser. Keep the Cargo target inside this checkout so it remains visible in the private filesystem. Build outside the private desktop:

```sh
export CARGO_TARGET_DIR="$PWD/tmp/native-build"
cargo build --locked -p mluva-gtk --bin mluva --example mluva_verify \
  -p mluva-audio --bin audio-fixture-peer --bin mluva-audio-cleanup
bash dev/verify.sh doctor
bash dev/verify.sh smoke "$PWD/tmp/verify-success"
```

Choose a new output directory under this checkout's `tmp/` for each run. `doctor` reports missing tools/binaries without starting a desktop. The session performs separate isolation and process-owner checks before controlling the app. Missing prerequisites or unsafe state fail with a nonzero exit; there is no fallback to the active desktop.

The smoke starts with empty History, completed onboarding, no account, and audio retention set to Never. It records synthetic PCM through the normal recording action and a loopback speech peer, checks the immutable original, edits the real editor, waits for autosave, quits normally, and opens the saved conversation after restarting Mluva. Machine-readable results and diagnostics remain in the output directory after the private session exits.

Exercise the failure path with an intentionally wrong expected working text:

```sh
bash dev/verify.sh smoke "$PWD/tmp/verify-failure" \
  --expect-working 'This expectation is deliberately wrong'
```

Expect a nonzero exit and a failed assertion with evidence. This demonstrates the harness's failure reporting; it does not simulate a speech provider outage.

Read `result.json` first: `passed` means the requested smoke or session command completed, `failed` reports the unmet check, and `blocked` means the private session could not be established. The process exit code must also be zero for success. A custom session's success covers its own commands and assertions; it does not imply that the smoke ran.

| Evidence | Purpose |
| --- | --- |
| `build.json`, `build.sha256` | Source revision/dirty state and hashes of the actual binaries used. |
| `session.*/trace.jsonl` | Timestamped CLI commands, results and expected values. |
| `session.*/fixture.json`, `provider-requests.json` | Declared substitutions, scope limits and observed loopback requests. |
| `session.*/final-state.json`, `invariants.json` | Saved original/working/delivered text, identity, clipboard and retention checks; invariants are written after a successful smoke. |
| `session.*/*.png`, `app.log`, `runner.log` | Actual private-desktop pixels, application output and runner diagnostics. `runner.log` is at the output root. |
| `session.*/failure-state.json`, `failure.png` | Last state, accessibility discovery and screenshot on a failed journey, when available. Startup failures may have only runner diagnostics. |

## Explore a session

Use a custom command inside the same isolated fixture. `$MLUVA_VERIFY_CLI` exists only inside the private session:

```sh
bash dev/verify.sh session "$PWD/tmp/verify-explore" -- bash -eu -c '
  "$MLUVA_VERIFY_CLI" doctor
  "$MLUVA_VERIFY_CLI" discover
  "$MLUVA_VERIFY_CLI" state
  "$MLUVA_VERIFY_CLI" assert count 0
  "$MLUVA_VERIFY_CLI" screenshot initial
'
```

The CLI emits JSON. Its commands cover `launch`, `discover`, `state`, `action`, `click`, `edit`, `assert`, and `screenshot`; use the runner's `--help` for the supported arguments. Assertion failures exit nonzero, so custom scripts should propagate failures. Keep scripts bounded and use the session's CLI instead of connecting to a host display, bus or application.

## Feature map

This map connects user reports to actual entry points and observable outcomes. Update the affected row when a control, shortcut or behavior changes. Listed access paths do not imply that every path is exercised by this smoke; physical shortcuts and live desktop permissions remain separate acceptance.

| User feature | Access paths | Expected outcome | Harness path and coverage |
| --- | --- | --- | --- |
| Record and stop dictation | Main **Dictate**/**Stop** button; command palette Start/Stop dictation; native `record` action; global recording shortcut (default F9) or `global-record`; Omarchy widget | Stop commits recognition, keeps the original separate, and applies retention. Manual capture copies only; global capture may insert into the exact captured target. A second toggle during preparation cancels. | `action record` starts/stops the actual app using synthetic PCM and a loopback speech peer. Saved original and working text are asserted. Button wiring, global capture, insertion and physical keys are not exercised. |
| Cancel a recording | **Cancel preparation** button while preparing; Escape in the capture window; configured global cancel shortcut (default Ctrl+Alt+Escape); native `cancel` action | Capture stops without delivering text; audio follows privacy rules. Escape in Settings/Welcome returns to capture instead, and dialogs may consume it. | `action cancel` is available for investigation. The initial smoke does not claim cancellation coverage; use the [application suite](../CONTRIBUTING.md#verification). |
| Edit saved dictation | Conversation editor reached from the sidebar or latest-conversation action; Ctrl+S or palette Save edits can also save | Working text autosaves after a pause; original recognition stays immutable. Failed saves retain the draft and retry. No Save button is required. | `edit TEXT` changes the actual editable GTK widget through AT-SPI. `assert raw`, `assert working` and `assert editor` inspect the resulting state. The smoke waits for autosave without Ctrl+S; storage-failure retry is not covered. |
| Copy current text | Per-document Copy icon; Ctrl+Shift+C; palette Copy current text | Explicit Copy uses the working edit. Autosave itself leaves the clipboard unchanged. | The private clipboard can be observed within the session. Explicit Copy control wiring is outside the initial smoke. |
| Reopen saved work | Sidebar conversation; menu Latest conversation; native `latest` action; latest shortcut (default Shift+F9); History via menu, Ctrl+H, palette or `history` action | The saved working edit returns with its original intact. Ctrl+P opens the command palette. | `action latest` and state/UI/storage assertions cover the single fresh conversation. Arbitrary sidebar selections and history search are outside this smoke. |
| Quit and restart | Menu Quit Mluva; native `quit` action; normal executable launch | Normal Quit flushes pending work and ends the process; save failure keeps it open. Closing the window only hides the resident application. | `action quit`, confirmed process exit, then `launch` exercise a real process restart. The main journey saves before Quit, so it does not independently prove debounce bypass or failure during shutdown. |
| Dictate directly to an agent | Main **Dictate to agent** / **Stop and send** button; configured agent key; `mluva herdr record` | Press once to start and again to stop. Key release and ordinary focus changes keep recording. Final text sends automatically after a two-second cancellation window. A five-minute safety limit retains a draft. | `make linux-agent-voice-test` covers capture, UI stall regression and recovery. `linux/tests/run_agent_end_to_end.sh /absolute/patched/herdr` drives private portal activation, real PCM capture, AT-SPI button activation, synthetic speech, the actual guarded Herdr server and actual Codex with loopback inference. |
| Automatically save settings | Settings → Workspace / Providers / Capture; agent shortcuts and routing are inside Capture | Valid choices persist automatically; text fields debounce, credentials save on leaving the field or Enter. Incomplete choices show errors. Active work retains immutable settings; pending provider changes retry afterward. | The assembled agent suite checks restartable storage for appearance, provider fields, routing keys, duplicate rejection and preserving settings changed on another page. |
| Choose or clear a Herdr recipient | Main recipient dropdown; Settings → Capture for the connection; installed `mluva herdr list`, `select NAME_OR_PANE [--socket PATH]`, `status`, `clear` | Fresh exact identity is selected in the resident app and remembered outside Incognito. Ambiguous/blocked targets fail; changing recipient disarms pending delivery while preserving the draft. Inspecting or switching sends no prompt. | Build `herdr-voice-fixture-peer` alongside the app, then `dev/verify.sh session "$PWD/tmp/verify-routing" -- bash "$PWD/linux/tests/support/herdr_cli.sh"`. The real CLI, app, dropdown, persistence and restart use a synthetic guarded peer; `make linux-agent-voice-test` also checks cancellation during the sending countdown. |

## Scope and safety

The fixture replaces the physical microphone with the existing deterministic PCM peer and the speech service with a local protocol peer providing one successful transcription per session. Start a fresh session for another independent recording. The application, actions, GTK editor, autosave, SQLite storage and restart are real. Screenshots and text in evidence are synthetic. Do not use this harness with private recordings, credentials or live account state.

The runner clears inherited environment values and uses private HOME/XDG directories, display, D-Bus, accessibility, network, PID and device namespaces. Only the new evidence directory is writable through the outer filesystem boundary. Cleanup is confined to the run's processes; the evidence directory is retained. The in-session CLI refuses an unverified environment or application owner.

Passing this journey does not establish physical microphone quality, F8/F9/F10 delivery, real sign-in, paid speech providers, live Wayland permissions or insertion into other applications. It makes no new performance claim. Keep using the [functional checklist](../docs/functional-checklist.md) and required native checks for those distinct scopes.
