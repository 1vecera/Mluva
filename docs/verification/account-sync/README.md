# Account continuity and desktop/web parity

**Prepared implementation; deployment and installed-app cutover are pending review.** The task branch adds native PKCE sign-in, optional text-only account sharing, selected-device live delivery, shared revisioned history and the corresponding browser editor workflows. The current installed native/widget/bridge and hosted production remain unchanged. [Portable brief](../../../tasks/account-device-sync-and-web-parity.md) · [parity inventory](parity.md) · [concrete cutover plan](cutover.md).

## Observed behavior

The real GTK application ran in a private Xvfb, HOME/XDG, D-Bus, network/PID/device namespace with two separate Chromium receivers. It recorded signed out through synthetic PCM and a local worker, signed in through an independent loopback OAuth/PKCE peer, kept its native provider configuration identical, exchanged text in both directions and delivered native words to a third receiver. Original local history was not uploaded. Saved edits, completed rewrites, merge segments, an offline edit retry, deletion and revocation converged. HTTP audit found no native PCM, audio file, model path or speech-token request. Signed-in Incognito capture left local history and the account unchanged. Original local recordings survived sign-out. This verifies production GTK/transport/storage orchestration; the local speech worker and Cognito peer were synthetic.

The real Chromium suite used isolated phone (390 × 844), laptop (1365 × 900), tablet (768 × 1024) and minimum phone (320 × 740) viewports. It verified two-way and third-device delivery, a lost live acknowledgement followed by reconnect/retry without duplication, immutable originals, concurrent editing with explicit resolution, server rewrite preview/apply, synthetic Scribe partial/Stop/finalization, continuation, atomic merge, account-scoped recovery after reload, cancellation of a late rewrite, Command source/instruction/preview behavior, Incognito, revoked-device HTTP rejection and browser sign-out. Protocol tests independently covered separate-account isolation, tombstones, bounded frames, one-use tickets, expired tokens, CAS races, receipt replay after newer edits, revoked-token re-enrollment attempts and revocation racing a device registration. [Browser checks](checks/browser.json) · [GTK checks](checks/native-account.json) · [recorded check output](checks/validation.json) · [final review](review.md).

Native core storage tests cover frozen retries across edits/restart, conflict preservation, owner partitions, deletion, downloaded-cache cleanup, and preservation of original local segments when signing out after a merge. The existing application acceptance passed 15 workflows / 147 GTK/store states plus its settings/actions suite. Portal shortcut acceptance passed 21 workflows / 59 states. Native capture/provider implementations and the three managed local model definitions were not changed.

## Verification commands

From the repository worktree, with the repository-pinned Rust toolchain and native prerequisites:

```sh
# Keep test temp paths on disk and short enough for Unix-domain sockets.
mkdir -p /home/vecera/tmp/s27-624-native
TMPDIR=/home/vecera/tmp/s27-624-native make linux-test
make linux-shortcut-test
make linux-application-test
make linux-account-test
uv run --no-project --with shellcheck-py shellcheck install.sh linux/*.sh linux/tests/*.sh dev/*.sh linux/mluva-shell
cd hosted
npm ci --ignore-scripts
npm run check
npm run format:check
npm test
npm run test:browser
```

The native gate includes workspace tests, both strict Clippy configurations, rustfmt and generated-feature consistency. Initial attempts exposed host `/tmp` being memory-backed and a too-long worktree scratch path in existing fixtures; the short on-disk `TMPDIR` above resolves those environment assumptions. GUI prerequisites missing on Lenovo were downloaded from the Arch package archive, signature-verified with the existing Arch keyring and extracted under worktree `tmp/gui-prerequisites`; no packages or desktop configuration were installed. For that private check only, PATH, LD_LIBRARY_PATH and XDG_DATA_DIRS referenced the extracted files.

`cfn-lint` checked the SAM template with default and canonical-host parameter shapes. Its only warning is W1030 on the optional empty certificate ARN; the template condition and assertion require a supplied us-east-1 ARN before using a custom alias. An actual certificate/ACM validation record is deliberately not created. Both npm dependency locks are updated for the optional Bedrock client. The existing GitHub workflow is manual and contains no branch-push deployment; hosted CI was not started or paid for.

## Responsive comparison

The native screenshots are actual GTK renderings, not a web recreation. The browser uses the native JetBrains Mono font (bundled with OFL), restrained green palette, document workspace, history sidebar and compact persistent capture bar. Phone history becomes a separate view, settings a scrollable dialog and controls keep 44-pixel touch targets. Native density is adapted to a narrow screen rather than scaled down.

| State | Native | Laptop browser | Phone browser |
| --- | --- | --- | --- |
| Login | [Native](screenshots/native-login.png) | [Laptop](screenshots/login-laptop.png) | [Phone](screenshots/login-phone.png) |
| Recording | [Native](screenshots/native-recording.png) | [Laptop](screenshots/recording-laptop.png) | [Phone](screenshots/recording-phone.png) |
| Completed/editing | [Native](screenshots/native-editing.png) | [Laptop](screenshots/editing-laptop.png) | [Phone](screenshots/editing-phone.png) |
| History | [Native](screenshots/native-history.png) | [Laptop](screenshots/history-laptop.png) | [Phone](screenshots/history-phone.png) |
| Settings | [Native](screenshots/native-settings.png) | [Laptop](screenshots/settings-laptop.png) | [Phone](screenshots/settings-phone.png) |
| Recovery | [Native](screenshots/native-recovery.png) | [Laptop](screenshots/recovery-laptop.png) | [Phone](screenshots/recovery-phone.png) |

Also checked: [explicit conflict](screenshots/conflict-laptop.png), [dark theme](screenshots/dark-laptop.png), [320-pixel phone](screenshots/minimum-phone.png). Automated checks assert no horizontal overflow. Screenshots and tests use synthetic words only.

## Provider and deployment evidence

Thirty real Scribe pairs measured 72.3 ms p95 added committed-text delay with matching complete final words. **This does not pass the representative-network activation gate.** It used this Lenovo network and deterministic PCM adapters; see the [experiment, raw timings and fixture](speech/README.md). A separate single synthetic Bedrock Converse request completed with `end_turn` in 419 ms using the proposed bounded text route’s payload: [result](bedrock-rewrite.json). Neither provider was enabled in the public app.

The [AWS read-only inventory](domain-inventory.json) and [exact DNS-name query](domain-dns-inventory.json) establish the current custom-domain starting state. The [cutover plan](cutover.md) covers certificate, alias, both web origins, Cognito/native callbacks, CORS, WebSocket origins, atomic API/static publication, old-origin draft migration, rollback and separately approved bridge retirement. Existing production text-only verification from 11 October remains [historical evidence](../hosted-companion/aws/README.md), not proof that this branch is deployed. [S27-620](https://linear.app/s27sgwy37/issue/S27-620) remains a separate queued read-only task owned by Daniel.

## Explicit limits

No physical microphone, Android/iPhone installation, Wayland callback/permission dialog, real account email/recovery delivery, or new live Cognito native callback was exercised. Those require the approved callback/domain deployment and deliberate physical-device checks. The test browser blocked service workers for deterministic fault injection; the shipped worker remains network-only and caches no private content. Browser launch needs a network; there is no claim of offline web transcription or reliable background recording. Browser clipboard requires permission/user gesture and cannot target arbitrary windows. System audio, OS shortcuts and screenshot narration stay native.

Web templates are editable per browser/account; native prompt overrides, provider settings and models stay local. Web rewriting is an explicit server preview/apply workflow; the native experimental continuously generated Live rewrite remains native. Device access is bound to the validated Cognito `origin_jti` login family, including its refreshed access tokens. Revocation blocks that family from reading history, using another device ID or registering another device; transactional checks cover in-flight writes. A fresh sign-in creates a new login family, so device revocation does not replace password/account recovery. Existing beta devices without a session binding must enroll a fresh device once after deployment. Late browser refreshes cannot restore a signed-out login, including when another tab clears it. Sign-out cannot recall text already copied or exported by a receiver. Deletion removes live records and uses content-free retry tombstones; existing operational backup retention still applies.
