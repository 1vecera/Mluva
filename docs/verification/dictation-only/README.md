# Dictation directly to agents

Prepared on Lenovo on 18 October 2026 from main `31c1021a6dbfc83e33161bf17b5e6bfa24cf0cd6`, after merging [PR98](https://github.com/1vecera/Mluva/pull/98). This follow-up simplifies both native Mluva and its hosted client. No installed application, local model, hosted stack, DNS record or private phone bridge was changed by this branch.

| Surface | Result |
| --- | --- |
| Capture | One ordinary dictation mode; existing Stop/Cancel, live speech previews and deterministic vocabulary remain. Global capture automatically inserts only into its verified original target; app/widget capture copies. |
| Agents | Existing named Herdr selection, hold-to-talk, two-second cancel window, explicit saved draft and uncertain-delivery protection remain. |
| Editor | One source editor, always-visible Copy/Save, working edits separate from raw recognition, smooth following with manual scroll-away and system reduced motion. |
| Removed | Polish, manual/live rewriting, cleanup/model-generation clients, prompts/templates, generated titles, Notes/Command modes and completed-widget review/countdown. Hosted rewrite endpoint, Bedrock dependency and IAM permission are removed too. |
| Existing data | Old replies stay readable/editable/exportable. Retired configuration cannot reactivate generation. Speech choice, local models, privacy, retention and saved originals survive. Unfinished Notes recover once into local History without resurrecting deletions; private drafts are erased. |
| Accounts / other capture | Optional text-only account sync, server web transcription, screenshots/narrated annotations and the separate Meeting recorder remain. Login never selects a different native speech provider. |

## Reproduce

Use the repository prerequisites and an isolated desktop. The core native gate builds the locked optimized bundle before running tests and both strict Clippy configurations:

```sh
make linux-test linux-shortcut-test
make linux-application-test linux-conversation-test linux-provider-settings-test
make linux-account-test linux-agent-voice-test linux-text-target-test linux-omarchy-test
bash linux/tests/run_application_smoke.sh history
bash linux/tests/run_application_smoke.sh capture
bash linux/tests/run_application_smoke.sh bootstrap
bash linux/tests/run_application_smoke.sh meeting
bash linux/tests/run_application_smoke.sh phone
make linux-screenshot-test
cd hosted
npm ci --ignore-scripts --no-audit --no-fund
npm test
npm run check
npm run format:check
npm run test:browser
```

`bootstrap` builds a private production bundle before checking single-instance actions and Quit. The GUI runners isolate display, session/accessibility buses, network, HOME/XDG and devices. Synthetic providers use no managed credentials or paid services. Use a short private TMPDIR outside `/tmp` for the native sandbox delivery tests when the checkout path is long.

## Verification evidence

All checks below passed on Lenovo on 18 October 2026. The full native gate ran after the last production edit; the final test-only fixture adjustments were checked with their private runners, strict workspace Clippy and formatting. No hosted CI was started; the repository workflow is dispatch-only. See [test ownership](test-audit.md) for consolidated versus retired tests. Historical release reports remain separate.

| Boundary | Actual result and receipt |
| --- | --- |
| Native gate | Optimized bundle, workspace tests, strict workspace and installer-without-default-features Clippy, formatting, generated maturity, plus 21 shortcut workflows / 59 states: [native gate](checks/native-gate.log), [final Clippy](checks/final-clippy.log). Tests that require physical/GPU/long-deadline execution remain explicitly ignored; they are not claimed as run. |
| Dictation workflow | 33 retained real HTTP/SQLite/private-clipboard cases: immutable originals, frozen vocabulary, automatic copying, deferred delivery, Incognito, retention, recognition failures, recovery and truthful diagnostics: [workflow receipt](checks/workflows-check.log). |
| Application / editor / settings | Actual GTK capture, continuation into edited text, save, legacy replies, cancellation, recovery, stale actions, navigation, scroll-away, reduced motion and provider/onboarding controls: [application](checks/application-check.log), [conversation](checks/conversation-check.log), [providers](checks/providers-check.log). |
| Speech / delivery / recovery | Local readiness without credentials, capture lifecycle across all three local model routes with synthetic CPU/CUDA workers, Scribe fallback, exact-target insertion and no target text reads, retained history recovery: [capture](checks/capture-check.log), [targets](checks/text-targets-check.log), [history](checks/history-check.log). These are protocol/lifecycle checks, not model accuracy or physical GPU inference. |
| Accounts / agents | Native sign-in and three-device text continuity; held Herdr routing, cancellation, editing, recipient replacement and uncertain delivery: [account](checks/account-check.log), [Herdr](checks/agent-voice-check.log). |
| Other retained capture | Screenshot attachment/editing/SQLite-failure/late-picker recovery, 9 Meeting workflows / 47 states, private phone IPC, native widget recording/motion/owner loss, and packaged startup/single-instance/Quit: [screenshots](checks/screenshots-check.log), [Meeting](checks/meeting-check.log), [phone](checks/phone-check.log), [widget](checks/widget-check.log), [bootstrap](checks/bootstrap-check.log). |
| Hosted | 20 unit tests, syntax and formatting; 12 browser receipts covering three receivers, both directions, ordered retry/reconnect, concurrent edits, legacy replies, removed generation route/mode controls, synthetic microphone transcription, history merge, save recovery, Incognito, revocation and sign-out: [unit](checks/hosted-unit.log), [browser](checks/hosted-browser.log), [syntax](checks/hosted-syntax.log), [format](checks/hosted-format.log). |
| Shell / final diff | ShellCheck and `git diff --check` passed. |

The private workflow command used the same isolated display/network/clipboard boundary as the GUI suites:

```sh
# Keep Cargo caches outside the runner's disposable HOME.
export CARGO_HOME=/home/vecera/.cargo RUSTUP_HOME=/home/vecera/.rustup
bwrap --die-with-parent --bind / / --dev /dev --tmpfs /run/dbus -- \
  bash dev/run-isolated-browser.sh "$PWD/tmp/workflow-check" -- \
  cargo test --locked -p mluva-workflows --test recording -- --ignored --test-threads=1 --nocapture
```

The full native gate used `TMPDIR=/home/vecera/tmp/mluva-voice-tests` (private mode 0700) for short sandbox socket paths. Native checks reused the existing target cache; each GUI scenario used its own HOME, buses, display and synthetic fixtures. Log paths identify the local evidence directories; runner teardown messages such as “X connection broken” occur after successful assertions.

## Screen review

These are actual private GTK and browser renders using synthetic text. Impeccable review covered the single editor, visible actions, responsive density, recording/status clarity, error recovery and touch controls; one correction pass fixed stale provider/failure wording and the Herdr entry-point hint. Existing branding and typography remain.

| State | Native | Web |
| --- | --- | --- |
| Entry | [Empty editor](screenshots/native-empty-wide.png), [onboarding](screenshots/native-onboarding.png) | [Phone login](screenshots/web-login-phone.png) |
| Recording | [Wide](screenshots/native-recording-wide.png) | [Phone](screenshots/web-recording-phone.png), [laptop](screenshots/web-recording-laptop.png) |
| Completed / editing | [Wide](screenshots/native-editing-wide.png), [narrow](screenshots/native-editing-narrow.png) | [Phone](screenshots/web-editing-phone.png), [laptop](screenshots/web-editing-laptop.png) |
| Settings | [Native](screenshots/native-settings-wide.png) | [Phone](screenshots/web-settings-phone.png) |
| History / recovery | [Native recovery](screenshots/native-recovery-wide.png) | [History](screenshots/web-history-phone.png), [recovery](screenshots/web-recovery-phone.png) |

## Deployment and physical limits

This is prepared as a draft PR. The earlier merge and local build are recorded separately in `/home/vecera/tmp/mluva-herdr-merge-local-build-result.md`. Updating the installed application/widget and deploying the hosted stack requires a later approved rollout of this branch. Deploy the full hosted template and client together so stale rewrite controls receive the removed-route response; existing history remains compatible. Native package upgrades should keep app and widget together.

Private X11 and synthetic audio prove application behavior, not physical Wayland key approval, microphone quality, browser background permissions or every target editor. Existing Herdr protocol support must be installed before routing can be enabled. The earlier voice-provider permission limitation remains unchanged. The separate queued S27-620 read-only deployment test is not completed by this work.
