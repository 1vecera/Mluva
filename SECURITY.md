# Security policy

## Supported versions

| Surface | Security support |
| --- | --- |
| Current Fedora Linux release and `main` | Supported |
| macOS source preview | Reports accepted; no supported public binary is claimed |
| Older commits and local forks | Best effort only |

## Report a vulnerability

Use [GitHub private vulnerability reporting](https://github.com/1vecera/Mluva/security/advisories/new) so the report is not disclosed before a fix is available. If that form is unavailable, open a public issue containing no exploit details or sensitive material and ask the maintainer to establish a private channel.

Do not submit API keys, OAuth tokens, service-account files, real recordings, transcripts, clipboard contents, application identities, window titles, or diagnostic exports containing user data. Use synthetic reproduction data and describe the affected boundary, expected protection, observed behavior, and smallest reproducible setup.

Particularly relevant boundaries include credential launch and logging, audio retention and Incognito erasure, cloud disclosure, password-field exclusion, AT-SPI target restoration, exact-once insertion, clipboard recovery, Codex sandboxing, session-bus overlay state, file permissions, and path traversal.
