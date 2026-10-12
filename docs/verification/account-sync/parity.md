# Native workflow inventory and web boundary

The native Rust/GTK application is the reference: one conversation workspace, searchable history sidebar, source and rewrite documents, compact recording controls and full-window settings. Account sync is optional; speech selection is independent.

| Workflow | Native owner | Web counterpart / boundary |
| --- | --- | --- |
| Capture, Stop, Cancel, live preview | CaptureController, CapturePage | Direct browser → Scribe with a server-issued temporary token; compact editor controls; finalization and cancellation invalidate late work |
| Originals, edits, delivered text | HistoryStore, ConversationStore | Same account note protocol, immutable original, revisioned working text and completed rewrites |
| Rewrite, task modes, prompts | ReviewController, PromptStore | Explicit server text rewrite, editable templates, preview/apply for Command and Notes; no desktop selection access |
| History, search, rename, delete, merge | ConversationWorkspace, HistoryController | Conversation navigation, search, revisioned titles, export and account deletion; preserved original segments for merges |
| Settings and privacy | ApplicationSettings | Account/device settings, retention, Incognito, provider disclosure; native model/settings/audio stay local |
| Recovery and conflicts | CaptureController and account ledger | Account-scoped durable pending text, frozen retry IDs, compare-and-swap revisions, explicit conflict resolution |
| Copy and export | Native delivery and HistoryStore | Explicit browser clipboard and Markdown/JSON download; no arbitrary-window insertion |
| Meeting and screenshots | Native microphone + monitor, screenshot portal | Browser microphone notes; OS capture, monitor mixing, global shortcuts and screenshot editor stay native |

The [verification report](README.md) links runnable acceptance checks and actual screenshots for each state. Web templates are local to each browser/account, and native experimental continuously generated Live rewrite remains native; web offers explicit server preview/apply. Permission and physical-device limits are recorded separately from implemented core workflows.
