# Hosted AWS beta acceptance — 11 October 2026

Live app: https://d24oqajhzma8jq.cloudfront.net/ · AWS account `762052017888` · `eu-central-1` · stack `mluva-everywhere-release`.

The public app shell is served through CloudFront from private S3. API Gateway rejects anonymous business requests with 401 and permits token-free browser preflight with 204. Lambda uses the account’s existing concurrency quota of 10; no quota increase was requested. Speech remains disabled, and no provider secret or paid speech session was created.

A private Chromium desktop used real Cognito OAuth code + PKCE sign-in and AWS endpoints. Three independent phone/laptop/tablet browser contexts on one temporary account exchanged live text in both directions and received shared DynamoDB history. A separate temporary account received neither that history nor live events. [Receipt](aws-browser-receipt.json), [phone](aws-phone.png), [laptop](aws-laptop.png), [publication checks](publish-receipt.json).

Two temporary accounts used suppressed email delivery and were deleted along with their account partitions after acceptance. No invitations were sent. The first deployment exceeded the existing Lambda reservation quota and rolled back; the failed stack and its verified-empty retained resources were removed. The accepted template makes reservations optional, builds from its lockfile and has a dedicated unauthenticated OPTIONS route while keeping business routes JWT protected.

This proves hosted text sync and real sign-in. It does not certify physical phone microphones, device-lock behavior, verification/recovery email delivery or sub-100 ms additional speech latency. The local three-device synthetic microphone/recovery/export/revocation check remains separate.
