# Released provider observations

The immutable reference is Mluva v1.6.0 at `5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f`. Reference collectors compare imported module bytes against that commit before executing them. Collection uses synthetic credentials, text, images and audio in private scratch files and real loopback services. The maintained tests, fixture peers and production library are native Rust; collectors are ignored scratch and are not shipped.

`released-providers.json` contains 4,820 independent observations from Python 3.14.7 with Unicode 16.0.0. SHA-256: `5d521f86b5472094157efae4e6a449fd949d9b21bec290ee696cb34bcf4a8170`.

| Contract | Observations | Native boundary |
| --- | ---: | --- |
| Model identifiers | 2,886 | Frozen printable ranges, whitespace and length |
| Compatible catalogs | 177 | Capability filters, explicit model, duplicate order, efforts and invalid rows |
| Batch metadata | 1,670 | Text, language/probability/duration, contiguous speakers and Unicode decimal labels |
| Realtime decoding | 7 | Object/UTF-8/JSON validation |
| Realtime errors | 45 | Controlled messages without provider bodies |
| Realtime endpoint queries | 15 | Existing/duplicate parameters, language detection and overrides |
| Outbound PCM messages | 8 | Exact compact JSON/base64, optional empty commit |
| Compatible rewrites | 12 | Exact ASCII JSON request bytes, fragmented SSE, provisional deltas and completion/error rules |

`released-wire.json` contains seven HTTP and eleven WebSocket observations through actual released clients and external loopback servers, without replacing their transport methods or response objects. SHA-256: `cdb2ea7504a364364729feae72dd621a9b62f9856916b0484f3b0b0902bd7ab2`. Multipart comparison normalizes only the random boundary. Visual requests preserve image order, offsets, prompt escaping and exact PNG bytes. WebSocket comparison records actual audio/commit order, overshoot, cadence, tail/no-extra-commit, empty sessions, provisional-only failure, detected language, disabled callbacks and controlled errors. Error scenarios wait until the client observes the failure before finalization, so scheduling does not decide whether a final commit races ahead of it. The fixture server explicitly accepts the large synthetic cadence chunks; its unrelated default frame limit must not reject them.

The 21 provider tests additionally use real native HTTP/WebSocket peers, TLS handshakes, files and child processes to check cancellation before headers and during response bodies, reusable close versus permanent cancel, redirect refusal, socket/document/response limits, large batch meetings, readiness, ping/binary events, incoming size limits, callback dispatch ordering, callback disposal after cancel, server closure after commit and a saturated queue. Native PCM recording feeds the native WebSocket session in order; a failed stream retains the entire private WAV. TLS tests generate a temporary self-signed certificate with `openssl`, give only the fixture child its trust path, and reject a wrong hostname or absent trust. Malformed environment keys fail before any connection and do not enter diagnostics. They do not alter host trust, read managed credentials, contact providers or open audio devices.

The compatible client's native branded User-Agent differs from the reference's interpreter-injected `Python-urllib/3.14` header. Explicit request payloads and required headers are compared; complete HTTP header byte identity is not claimed. ElevenLabs preserves the existing branded User-Agent. Invalid/nonstandard JSON numeric extensions and an exhaustive malformed-provider corpus still need acceptance review as part of complete workflow parity.

Native Codex JSONL transport and the shared rewrite factory now have separate [catalog, process and installed-CLI evidence](codex-evidence.md). They preserve explicit/hidden models, fast/thinking choices, process/environment/instruction isolation and the 40,000-character document rewrite limit. The ordinary workspace passes 95 tests; the installed Codex check was additionally executed with its private loopback provider.

Native speech credentials add [validation, keyring subprocess and selected-key upload evidence](credential-evidence.md). Three tests compare 69 validation decisions and 21 process sessions, including real lookup/save timeouts, saved-key precedence, failed-save cache retention and malformed environment bytes. The complete ordinary workspace now passes 98 tests.

This proves component behavior on this Linux machine, not complete app parity, remote service availability or a speed improvement. Complete provider controllers, desktop secret-service/settings integration, managed local CPU/GPU inference and UI/history/privacy/delivery workflows remain pending. `provider-fixture-peer`, `codex-fixture-peer`, `credential-fixture-peer` and `audio-fixture-peer` are development binaries and must never be included by a blanket workspace install. The installed 1.6.0 app and widget remain unchanged.
