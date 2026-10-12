# Canonical address and bridge retirement preparation

Status: **prepared, not delivered**. No application, IAM, Cognito, TLS or DNS change has been applied. No installed app/widget/bridge has been replaced. The apex and every unrelated subdomain remain outside scope. Related read-only test [S27-620](https://linear.app/s27sgwy37/issue/S27-620) remains Daniel’s separate queued task.

## Read-only inventory · 12 October 2026

| Resource | Verified value |
| --- | --- |
| AWS account / region / stack | `762052017888` / `eu-central-1` / `mluva-everywhere-release` (`UPDATE_COMPLETE`) |
| CloudFront distribution | `E3HWK9SHYA4M6D`, `d24oqajhzma8jq.cloudfront.net`; no aliases, default CloudFront certificate |
| Static private bucket | `mluva-everywhere-release-site-byzhfneudjgo` |
| Cognito pool / public client | `eu-central-1_UcEjScSdb` / `44uvvv939hl9o7gna9mqssp8q5` |
| Managed login | `https://mluva-everywhere-762052017888.auth.eu-central-1.amazoncognito.com` |
| HTTP API | `https://2xphuumcxl.execute-api.eu-central-1.amazonaws.com` |
| WebSocket API | `wss://gr2zwsgdyc.execute-api.eu-central-1.amazonaws.com/live` |
| Cloudflare zone | `danielvecera.com`, `3b605e23359cadd266e35932ac881b3d`; exact-name DNS query returned no `mluva.danielvecera.com` record |
| Certificate | No certificate for this domain found in ACM `us-east-1`; ARN and validation record do not exist yet |

Existing Cognito callback is `https://d24oqajhzma8jq.cloudfront.net/callback.html`, logout is that origin’s `/`, code flow with token revocation enabled, access lifetime 60 minutes and refresh lifetime seven days. Lambda concurrency quota is 10; keep `ReservedConcurrency=0`.

## Bounded deployment batch for Daniel’s approval

Review the final PR commit and static artifact digest before authorizing this batch. Audience: existing account users and the public sign-in/app shell. Account text stays behind Cognito and active-device checks. Side effects: new public canonical hostname, callback registration, Lambda/API schema and code changes, static asset publication, certificate validation DNS and CloudFront invalidation. Application compute remains Lambda; no relay or VM is created.

- Request **one ACM certificate in `us-east-1`**, exact name `mluva.danielvecera.com`, DNS validation, with a task-specific idempotency token. Capture its returned ARN and ACM’s exact validation CNAME. Review that generated CNAME before publishing it in the existing Cloudflare zone. It cannot be specified truthfully before ACM issues it. Do not request `*.danielvecera.com` or an apex certificate.
- Build `hosted/template.yaml` with SAM and inspect an unexecuted change set for the existing stack. Preserve identity pool, DynamoDB table and S3 bucket logical IDs. There must be no replacement or deletion of durable resources. Parameters: existing `LoginDomainPrefix`, `CustomDomainName=mluva.danielvecera.com`, `CertificateArn=<issued exact ARN>`, `ReservedConcurrency=0`, `SpeechEnabled=false`, preserve the existing speech-secret parameter, `RewriteModelId=` and `RewriteModelArn=`. Rewriting stays off in this first domain batch; a specific model/resource and budget require a separate bounded activation approval.
- The template adds the CloudFront alias and validated ACM certificate (`sni-only`, `TLSv1.2_2021`). Cognito retains the old callback/logout, adds `https://mluva.danielvecera.com/callback.html`, `https://mluva.danielvecera.com/`, and native callback **`mluva://account`**. Both origins are accepted by HTTP CORS, unauthenticated OPTIONS, Lambda origin validation and WebSocket connect checks. Business routes remain JWT-protected. The native desktop entry handles the scheme; PKCE/state bind its callback to an initiated sign-in, expiring after ten minutes.
- Publish the reviewed Lambda/API update and prepared static files together. `node hosted/scripts/configure.mjs <describe-stacks.json> <local-directory>` produces configuration with canonical and legacy origins, stable APIs, Cognito client and native callback; it never uploads. Use its exact resulting directory for the approved S3 upload and invalidate only this distribution. The font is bundled locally. The old CloudFront address continues to serve working sign-in and callbacks.
- Once CloudFront reports `Deployed` and its certificate covers the hostname, create exactly the following Cloudflare record. Use DNS-only to leave TLS termination and caching with CloudFront. First use a temporary local resolver override to verify the hostname against CloudFront without changing public DNS. Check certificate/SNI, app assets, CORS preflight, both web callback URLs, native callback and three selected devices before completing the DNS cutover.

```json
{"type":"CNAME","name":"mluva","content":"d24oqajhzma8jq.cloudfront.net","ttl":300,"proxied":false}
```

Prepare the exact ACM validation CNAME, ARN, change set, commit and artifact hash before executing the approved writes. Credentials never belong in those artifacts. Updating just DNS is insufficient: the current certificate and callback list do not support the new address.

A local canonical static bundle was prepared with the currently verified API/client endpoints and the proposed hostname: [exact public configuration](canonical-config.json), [file SHA-256 manifest](static-manifest.sha256). The source input changes only the planned `WebOrigin`, adds `LegacyWebOrigin` and the native callback to the read-only stack output; these are proposed values, not deployed outputs. Rebuild after the approved stack update and compare against this manifest before uploading. No secret or access token belongs in static config.

## Old-address transition and rollback

The old address remains a valid alternate during transition. Its app displays an explicit link to the canonical address; it does not redirect in the middle of an OAuth flow. PKCE state, browser login, recovery drafts and device IDs are origin-bound. Users should save/export an unfinished old-origin draft before moving, then sign in and name the canonical browser as a new device. Cloud history already belongs to the same account; there is no bulk upload or automatic transfer of private local history. The updated API requires a validated Cognito login family (`origin_jti`) on each device. Existing beta device rows lack that binding: the updated web client enrolls a fresh UUID instead of claiming the old row, while the API rejects old sockets/tickets on their next use. Remove obsolete beta device rows to free slots; no history is removed by that enrollment change. Remove the old browser device when it is no longer needed. Keep the old Cognito callbacks until those users have moved.

Rollback DNS by deleting only the newly created `mluva` record; preserve the ACM validation CNAME and certificate while reviewing rollback. Keep both callback/origin entries and the existing CloudFront URL usable. Roll back to the immediately preceding *account-protocol-compatible* Lambda/static artifact if needed; the 2.3.0 API does not understand revisioned edits or active-device history authorization and is not a transparent rollback target after new edits. Back up the table before the approved deployment; restoring production data requires its own reviewed plan. Never delete account data to roll back UI code.

## Installed bridge retirement gate

`https://mluva.whatsupppmydude.uk`, its restricted Cloudflare Access policy, Tunnel, phone IPC service, installed native 2.3.0 and widget remain running throughout preparation. This change neither edits their configuration nor uninstalls them.

After canonical account deployment and Daniel’s physical-phone/desktop acceptance, separately approve retirement of the old phone service, its Tunnel hostname route and only its Access application. First verify native offline/local capture, selected-device delivery in both directions plus a third receiver, history/rewrite/merge convergence, revocation, recovery and no native audio in sync traffic. Preserve local SQLite, audio retention folders, config, models and original bridge recovery files. Export any unsaved browser draft before shutting its service down. Take a configuration backup and record the exact service/Tunnel/Access IDs at that time; do not infer them from this task. Re-enabling the saved route/service is the rollback until Daniel accepts retirement.

Public speech remains disabled until the latency gate passes on representative Czech phone and laptop networks, final words/recovery are verified, and Daniel approves its provider budget and activation. A token-mint Lambda is startup work; it must never become a long-lived audio relay.
