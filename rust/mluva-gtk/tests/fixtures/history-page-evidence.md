# Released History archive page

Invariant observation fields now use the shared [lossless fixture storage](fixture-storage.md). Native equality and identical canonical hashes preserve every complete original observation, action and source metadata; the existing comparisons and scope below are unchanged.

The reference is unchanged Mluva v1.6.0 at `5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f`. [The fixture](released-history-page.json), SHA-256 `1eeaf39dd10739e4c5c089d271a1a55ed44e5f742fe99f2105568b458e5d020d`, records nine actual released History pages and 64 GTK/store states. The external observer verifies every imported application module against that commit. Its scripts and raw evidence remain ignored scratch; the native test consumes only static data.

The production HistoryPage uses the existing native HistoryStore and ConversationStore. Real widgets and signals exercise titles, Unicode trimming, unchanged/invalid/corrected text, raw restoration, copy and retry callbacks, provenance/timings, legacy captions, 12/24-hour dates, grouped recordings and durable rewrites, child-to-parent focus, refresh, unsaved-title preservation, exact Markdown/JSON exports and cancelled/denied/confirmed deletion. The comparison observes actual row order, text, expanded state, entries, editors, button visibility/sensitivity, dialog responses, database records, callback events and exported bytes. Focus comes from the running page's owner. Existing historical IDs/timestamps are controlled database inputs; only the private filesystem root is normalized.

The independent legacy-caption cases exposed capitalization after underscores and Unicode title mappings. Native title casing now uses the existing frozen Unicode 16 properties plus full title mappings, preserving expansion and contextual final sigma. The existing upper/lower corpus remains part of the workspace regression checks.

Run the desktop check inside the disposable helper with its documented native environment:

The fixed timestamps were observed in `Europe/Prague`; select that timezone explicitly when replaying the date-format assertions.

```sh
bash dev/run-isolated-browser.sh tmp/native-history-page -- \
  env TZ=Europe/Prague cargo test --locked -p mluva-gtk --test history_page -- --ignored --test-threads=1 --nocapture
```

The executed check passed all 64 states on a private display with separate HOME/XDG, session/accessibility buses, network/PID/mount namespaces and masked input/audio/GPU devices. No host input, installed settings, real credentials or user content were used. Copy/delivery/retry/reprocess and deletion eligibility are explicit parent callback boundaries here; the actual database deletion and page mutations run normally. Full parent recovery actions, visual/accessibility comparisons across sizes/scales and complete application acceptance remain pending. This does not establish complete History or application parity, and every full-workflow acceptance row remains Pending.
