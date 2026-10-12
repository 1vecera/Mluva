# Mluva: one account, live sync and desktop/web parity

## Intent

Turn Mluva into one coherent product across the native desktop app and installable web app. A person signs in once on each device, speaks on any device, sees the same live words on their selected devices, edits or rewrites them, and returns to consistent history. Preserve the product's local-first premise: native capture and the local speech route run on the user's PC, with text alone synchronized; web microphone audio is transcribed by a server speech provider and its text is distributed through the same account. “Translated” here means speech-to-text, not a new language-translation feature.

Use the existing native desktop app as the reference for core functionality, information architecture and visual quality. Bring the web experience into real functional and visual parity, adapting it well to phones. Replace the separate PC-dependent phone bridge with this common account path after proving the replacement. Prepare `mluva.danielvecera.com` as the canonical hosted address under Daniel's newly registered Cloudflare domain.

## Output format

Implement in https://github.com/1vecera/Mluva from fresh `origin/main`, on an isolated task worktree. Deliver a reviewed branch and draft PR with `Fixes S27-624`, a portable task spec at `tasks/account-device-sync-and-web-parity.md`, a concise desktop/web parity matrix, runnable verification evidence, responsive screenshots and the concrete custom-domain deployment/cutover plan. Completion is a verified implementation prepared for review, with any unavailable physical-device proof or unavoidable browser limitations identified honestly; a styled landing page or account-settings mockup is not completion.

Codex Lenovo implements and verifies this task on the current PC through the assigned Herdr run. Daniel's final correction supersedes the earlier Claw Mini request. The implementation must demonstrate native sign-in/sign-out, named and revocable devices, web-to-native and native-to-web live delivery plus a third receiver, consistent saved originals/edits/rewrites/history, retry without duplication, offline/reconnect recovery, concurrent-edit handling and separate-account isolation. Show that native recording still works without an account or network, that login does not change its speech provider, and that native microphone audio is absent from sync requests.

Inventory the native workflows before deciding the parity boundary: capture/Stop/Cancel, live previews, editing, rewriting and task modes, prompts/templates, history/search/conversation controls, settings, privacy, recovery, copy and export. Implement the core counterparts in the web app, including usable server transcription and a clear, polished login/onboarding flow. Supply browser equivalents for OS-specific affordances; document genuine permission/platform limits rather than pretending a browser can operate arbitrary desktop windows. Compare actual native and web states at phone and laptop sizes, including recording, completed/editing, history, settings, login and failure/recovery.

## What to maximize

- Reliable continuity: selected devices see ordered live words and converged final notes, without duplicate sessions, lost edits or stale results overwriting newer work.
- Local-first trust: native work remains useful offline; account sync is optional and understandable; local originals, retained audio, settings and models survive the change.
- One polished product: desktop and web share the Mluva identity, typography, controls, terminology and calm recording/editor experience, with fast, simple onboarding and generous phone touch targets.

## Hard constraints

- Native audio must not be uploaded for account sync or silently routed through the hosted recognizer. Preserve immutable recognition separately from working/delivered text, account isolation, credential secrecy and Incognito/retention rules. Do not upload existing private history on sign-in without a clear opt-in; revocation, sign-out and deletion must stop stale access and resurrection.
- Reuse the managed AWS architecture with application compute at most on Lambda; no always-on audio relay or VM. The earlier cloud speech gate still applies: measured p95 extra steady-state delay below 100 ms against a direct provider connection. Distinguish startup, recognition and fan-out timing, use representative measurements, and report a failed gate instead of enabling speech or claiming an unsupported speed result.
- Authority is implementation, isolated verification, scoped synthetic provider measurements with existing managed credentials, branch push and draft PR. Prepare the exact AWS/TLS/DNS/Cognito/callback changes for `mluva.danielvecera.com`; do not merge, release, modify production, enable public paid speech, cut over DNS or stop the installed bridge before Daniel approves the concrete result. Leave the apex and other project subdomains alone; do not send invitations or messages to people.

## Soft constraints

- Reuse the existing native Rust/GTK4 app, hosted web app, accounts, device tickets, live transport and history contracts. Prefer a common session/data protocol over parallel implementations; preserve the three managed local models and explicit provider choices. An existing explicitly selected cloud provider remains a user choice; account login itself must never make local recording cloud dependent.
- Make the current desktop experience the visual reference; use the existing assets in `docs/brand/`. Avoid a generic card dashboard, giant microphone-only UI or marketing decoration replacing the native editor and useful controls. Adapt desktop density thoughtfully to a phone rather than squeezing a desktop screenshot into it.
- Treat the old `mluva-web`/Cloudflare Tunnel/phone IPC route as a migration surface, not the new architecture. Provide a safe retirement path after the account replacement passes; preserve retained local data and keep the current installation usable during preparation.

## Context

Current main and release v2.3.0: `61d9a3d5db9b88dcd362dbba182d1482380f0c7b`, delivered in PRs #91 and #92 on 11 October. Native and hosted histories are currently separate, native has no account login/sync, and hosted microphone transcription is disabled. This task supersedes the old spec's requirement to retain the bridge as the product path, but does not erase its privacy/latency requirements. Read repo `AGENTS.md`, `CLAUDE.md`, `CONTRIBUTING.md`, `docs/product-contract.md`, `docs/functional-checklist.md`, `tasks/phone-product.md` and `hosted/README.md` before implementation.

Existing hosted beta: https://d24oqajhzma8jq.cloudfront.net/. AWS account `762052017888`, region `eu-central-1`, stack `mluva-everywhere-release`; Cognito, API Gateway HTTP/WebSocket, Lambda, DynamoDB, private S3 origin and CloudFront. Lambda account concurrency quota is 10; the accepted template makes reservations optional. Browser preflight has its own unauthenticated OPTIONS route while business routes require access JWTs. Real sign-in, three-browser live text/history and account isolation were verified; evidence is in `docs/verification/hosted-companion/aws/`. Existing hosted speech can use short-lived server-minted Scribe credentials and direct browser-to-provider audio streaming: recognition happens on the provider server without forcing audio through a long-lived Lambda relay. Evaluate this against the preserved latency gate.

Cloudflare zone `danielvecera.com` was read-only verified active on 12 October, zone ID `3b605e23359cadd266e35932ac881b3d`; no `mluva.danielvecera.com` record exists. Prepare the hostname, CloudFront certificate/alias, canonical app config, Cognito redirect/logout and native login callback, API CORS/WebSocket origin checks, and safe old-URL transition together. Do not make a DNS-only change that breaks TLS or account login.

Current native app and Omarchy widget are both 2.3.0 on Lenovo. The installed private recorder at https://mluva.whatsupppmydude.uk remains active with its existing restricted Cloudflare Access policy; do not broaden or disrupt it during preparation. The original checkout `/home/vecera/code/mluva` is on an unrelated dirty design branch: use a new worktree. An already-built native cache is available at `/home/vecera/code/mluva/.claude/worktrees/browser-recorder/tmp/native-build`; coordinate access or use a separate target directory. The saved task brief is also available at `/home/vecera/code/mluva/.claude/worktrees/account-sync-parity-brief/tasks/account-device-sync-and-web-parity.md`.

Related existing personal task: S27-620, “Test Mluva's existing AWS deployment end to end,” is a separate queued read-only test owned by Daniel; link it for context without changing its ownership or claiming it is complete.
