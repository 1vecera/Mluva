# Final implementation review

Status: prepared for review on `feat/s27-624-account-sync`, based on `61d9a3d5db9b88dcd362dbba182d1482380f0c7b`. The review covered the account boundary, original-text preservation, retries/concurrency, cancellation, privacy, deployment effects and actual GTK/browser layouts. The [check receipts](checks/validation.json) distinguish local proof from remaining deployment and physical-device checks.

## Corrections verified before handoff

- Bound HTTP device access to Cognito's validated login family and included revocation absence in DynamoDB transactions. A removed device's retained/refreshed JWT cannot select another device, re-enroll or win a concurrent registration. Legacy unbound devices cannot reuse tickets. Other login families and accounts remain usable.
- Invalidated late browser token refreshes and business responses after sign-out. Closed WebSockets ignore late events; refresh responses cannot restore local credentials after either same-tab or cross-tab sign-out.
- Kept frozen pending edits and operation IDs until acknowledgement. A retry receipt older than the current revision returns a conflict instead of pretending an obsolete edit is current. Explicit conflict resolution preserves both versions until the user chooses.
- Preserved original recognition and source segments during edits and atomic merges. Native imports cannot steal unrelated private recordings or turn a continuation into a conversation cycle. Signing out removes downloaded account cache while preserving owned local originals and retained audio.
- Cancelled native in-flight rewrite results when account changes replace or delete their source. Browser source/rewrite generations similarly reject late results after cancellation, edits, deletion and sign-out.
- Reviewed Incognito boundaries, native account opt-in and typed sync payloads. Native capture keeps its selected provider; native audio/model/config paths do not enter account HTTP traffic. Old private history needs explicit sharing.
- Compared actual native screenshots with browser recording, editing, history, settings, login and recovery states. Corrected the login checkbox layout; phone controls retain usable touch targets and tested viewports have no horizontal overflow.

## Handoff boundaries

The native gate passed 155 tests with 74 intentionally ignored integration cases, both strict Clippy configurations, formatting and feature checks. The affected account integration ran separately against real GTK and two Chromium peers; existing application and shortcut acceptance also passed. Twenty hosted unit/protocol tests and thirteen browser acceptance observations passed. Scoped provider measurements used synthetic content only.

The live AWS stack, Cognito callbacks, Cloudflare DNS, installed app/widget and restricted bridge were not changed. The public repository's ordinary branch and draft-PR visibility is the authorized delivery surface for this work. The manual GitHub workflow does not deploy on push; no hosted CI was started or paid for.

Remaining proof is explicit: physical phone/microphone and Wayland permissions, real Cognito native callback after an approved deployment, representative-network speech latency and hosted fan-out timing. The limited Scribe experiment's 72.3 ms p95 added committed-text delay does not certify that gate. Public speech and rewriting remain disabled; the [cutover plan](cutover.md) requires separately authorized deployment and activation. S27-620 remains Daniel's separate task.
