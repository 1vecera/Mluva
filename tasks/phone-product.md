# Mluva everywhere

## Intent

Make Mluva feel like a real product for people who want to speak on a phone and use their words on any laptop, or speak on a laptop and continue on a phone. Maximize the phone experience: obvious recording, trustworthy live feedback, easy account setup and device connection, recovery when connectivity fails, and useful history across devices. Deliver English product UI and documentation, retaining the existing native desktop microphone companion and adding a separately deployable hosted web app.

## Output format

Implement the product in this repository and deliver a reviewed branch and draft PR. The native companion must continue working with the existing desktop app. The hosted companion must include account onboarding, named and revocable devices, bidirectional live text across multiple devices, account-scoped cloud history with export and deletion, installable mobile UI, and runnable local acceptance using synthetic speech peers. Include AWS infrastructure, setup instructions, screenshots and an honest latency report. Completion means implementation and local verification are prepared for review; physical-phone acceptance and hosted measurements must be explicitly distinguished from local evidence.

## What to maximize

- A person unfamiliar with Mluva understands how to connect their devices and record without a technical explanation.
- Words arrive live on chosen devices; stopping, disconnecting and retrying preserve useful text without duplicate history or unexpected clipboard changes.
- A polished phone interface makes recording, destination, connection state, privacy and recovery clear with generous touch targets and restrained visual chrome.
- Accounts isolate every device, live session and history entry, including reconnects, revocation and expired authorization.
- The hosted service has no always-on application server and offers evidence about latency rather than an unsupported speed claim.

## Hard constraints

- Backend application compute must run at most on AWS Lambda; managed identity, API Gateway, DynamoDB and static hosting are acceptable. Deployments, publication and new paid service use require the existing delivery authorization.
- Never expose long-lived provider credentials or another account's text; preserve native Incognito and immutable raw recognition. Clipboard delivery requires a user action.
- Server transcription is acceptable only with measured p95 extra steady-state delay below 100 ms against a direct provider connection. A direct browser-to-Scribe route with short-lived server-issued credentials is the default candidate; do not add an audio relay or claim the threshold passed without representative hosted measurements.

## Soft constraints

- Reuse the existing Mluva identity and native phone capture implementation; prefer a small independently testable hosted boundary over a native rewrite.
- Support modern iOS Safari and Android Chrome plus desktop browsers through an installable web app; document operating-system background recording limits.
- Keep setup and delivery explicit, and make failed connections and unfinished text recoverable without requiring provider knowledge.

## Context

Start from current `origin/main` and incorporate the existing live phone implementation from draft PR [#90](https://github.com/1vecera/Mluva/pull/90). Daniel's request authorizes implementation and local verification, a task branch and draft PR, but not deployment or broadening the existing recorder's access. “Separate version” means a hosted companion alongside the local desktop companion. Hosted live streaming covers speech text; raw microphone audio travels directly to the selected speech service. Browser receivers provide copy and export; native system-wide paste on Windows/macOS is outside the browser's permission model.
