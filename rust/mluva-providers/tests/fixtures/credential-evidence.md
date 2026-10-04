# Native credential evidence

The unchanged behavior reference is v1.6.0 at `5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f`. Ignored collectors verify `credentials.py`, `config.py` and `elevenlabs.py` against that commit before collecting observations. The comparison uses an independent native synthetic `secret-tool` process and actual loopback speech requests. Maintained Rust tests need no interpreter or reference implementation.

| Fixture | Independent observations | SHA-256 |
| --- | --- | --- |
| `released-credentials.json` | 69 entered-key validation decisions | `a4c9788614b9eda67146f5fb7df7e6945dfad414132df936978dadac71ac8e29` |
| `released-credentials-wire.json` | 21 process sessions, 175 operations, 31 actual keyring calls and 88 selected-key speech uploads | `e5273f379c2a3b1079061b3e8804fcb4b4d73615b01944f7e27b58d20a7810dd` |

Validation preserves Unicode whitespace, the 4,096-character limit rather than a UTF-8 byte limit, embedded NUL handling and exact untrimmed stdin bytes. Store arguments contain only the released operation, label and application/provider attributes. Keys never appear in command arguments or diagnostics, and the production credential wrapper retains redacted Debug output.

The application-owned cache preserves saved-key precedence over inherited credentials, unavailable/empty lookup caching, unchanged values after an external keyring edit, invalidation after a successful save and preservation after a failed save. Explicit environment maps bypass keyring discovery; 129 such decisions include precedence, blank/information-separator values and custom variable lists. Eighty-eight actual loopback requests establish which selected credential reaches the speech client. Tests use only synthetic values and bytes.

Lookup retains the two-second budget and save retains sixty seconds. Both timeouts are exercised at their actual released durations, with process disappearance checked after completion. Missing executables, nonzero exits, malformed text, ignored diagnostics, an early-closing stdin and concurrent large stderr/input are compared through actual subprocesses. Input writing and captured output draining run concurrently; a 4,096-emoji key cannot deadlock behind keyring diagnostics. Successful lookup output preserves text-mode CR/LF normalization and Unicode trimming.

Review corrected silent fallback on malformed environment bytes. Unrelated malformed variables and lower-priority values cannot disturb an already-selected key; saved keys still win. An invalid higher-priority credential fails before any HTTP request rather than selecting another account's key. Native malformed-text diagnostics use controlled messages instead of reproducing interpreter codec exceptions or private bytes. The fixtures compare those failure categories, not exact codec diagnostic text. Complete controller error-display acceptance remains pending.

The three credential tests pass; the complete workspace passes 98 ordinary native tests, and formatting/Clippy with warnings denied pass. Three private GTK tests and the installed-Codex test remain excluded from the ordinary run and retain their explicitly executed evidence. The credential test children have empty inherited environments plus explicit synthetic HOME, PATH, locale and fixture settings. They have no desktop secret-service address, managed credentials, real account/content, microphone or visible desktop access. `credential-fixture-peer` must never ship.

Real desktop keyring/service availability, provider settings/welcome integration, complete application workflow parity, local CPU/GPU inference, distribution, final Python removal and performance measurements remain required. The installed application and widget remain the verified 1.6.0 reference.
