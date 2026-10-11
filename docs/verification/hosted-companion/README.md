# Hosted companion verification

Prepared locally on 10 October 2026. The screenshots show production web UI with synthetic account, device and speech data. The app has not been deployed to AWS and these images do not establish physical-phone acceptance or production latency.

## Observed acceptance

Eight backend behavior checks cover account-isolated bidirectional streaming, nonpersistent partial text, one-use/expired/origin-bound connection tickets, device removal including pending tickets, ordered and deduplicated updates, account-expired sockets, immutable original history and retry receipts, deletion tombstones, bounded account-scoped history pagination, oversized Unicode, opt-in speech/consent/token rate limits, and atomic active-device limits.

Two additional speech finalization checks prove that a delayed earlier commit cannot settle Stop before the final audio response, and that a worklet flush failure still releases the microphone and keeps the stopped backup.

The isolated Chromium test connects separate phone, laptop and tablet browser contexts and exercises the actual page, API, WebSocket and AudioWorklet. It verifies phone → laptop/tablet multicast and laptop → phone live text, shared history, partial and committed synthetic speech before Stop, final commit before automatic save, failed cloud save followed by reload/recovery/retry, search and Markdown download, cloud deletion, nonpersistent recovery with history off, and receiver revocation. The backend speech peer accepts actual resampled PCM; no real microphone or paid provider is used. Browser-page errors are empty. [Receipts](browser-receipts.json) identify the fixture and isolation display.

CloudFormation passed `cfn-lint`; JavaScript syntax/static JSON checks and the Lambda module import passed. The native integration keeps phone audio in the existing widget/session, and its private GTK acceptance passes for phone/PC Stop, ordered PCM, History/copy, Incognito/retention, disconnect cancellation and PC microphone exclusion. The full native test gate, both strict Clippy configurations, formatting, generated-feature consistency, the private shortcut portal check and ShellCheck passed. The native gate uses disk-backed repository scratch because this host’s `/tmp` is memory-backed. The X11 check uses a staged `xclip` executable inside its private session; nothing was installed or copied to the host clipboard.

## Screenshots

- [Phone workspace](phone.png): 390 × 844 viewport, full page; Record, editable result, selected destination, incoming words and history.
- [Laptop workspace](laptop.png): 1365 × 900 viewport, full page; recording and receiving together.
- [Welcome](welcome.png): 390 × 844 viewport, sign-in explanation with the local-test disclosure visible.

The screenshots were inspected for clipping, layout overflow, readable controls, empty and settled state, and reachable history. Phone document width does not overflow its viewport. Device names and transcript content are synthetic.

## Remaining acceptance

Real Cognito sign-up/email verification/password recovery, deployed AWS JWT validation and DynamoDB/fan-out behavior, provider token issuance, physical iOS Safari/Android Chrome permission and install behavior, background/lock interruption, and representative hosted latency remain unverified. The sub-100 ms extra-delay target is not certified. The browser app offers explicit clipboard copy on each receiver; it does not synchronize the native desktop SQLite database or promise system-wide insertion on macOS/Windows.

Local protocol success is not evidence about radio latency, Lambda cold starts, provider latency, accuracy, live Wayland insertion or all physical phones. Cloud speech remains disabled by default until an authorized hosted acceptance run provides that evidence.
