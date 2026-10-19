# Autosave and narrated screenshot handoff

Prepared on Lenovo on 19 October 2026 from `056dbb9a4b3e93a14158ffdbf1b4647df8df527d`. This follow-up makes native and hosted note/title edits save after a short typing pause, removes the visible Save controls, and gives native dictation a stable Markdown path with local images and timing context. Original recognition remains separate from working text. No deployment or local installation is part of this branch.

## Behavior and recovery

Native edits remain keyed to their original conversation when navigating. A failed SQLite write retains the visible draft and retries; ordinary Quit attempts to flush first. Incognito does not create new notes or Markdown exports. Hosted autosave serializes revisioned operations, retries the same operation after an uncertain acknowledgement, and saves later typing as the next revision. Explicit conflict resolution remains necessary. Local recovery respects the history and Incognito controls.

**Copy as path** flushes edits, writes `notes/<conversation-id>.md` privately, and copies its absolute path. The document contains the current dictation, original recognition/continuations, and numbered screenshot callouts with relative PNG links. Capture, image-save and observed editor-close times are distinct, with recording identifiers and elapsed offsets where known. Speech excerpts are provisional live previews, not word-level alignment. Deleting a note or imported account cache removes the generated document.

Tensaku's **Close editor** button and Escape keep the last saved image without changing the clipboard. Save retains annotations in Mluva's attachment; Save as creates a separate export. Mluva observes saves to its managed attachment while it is running. Events after Mluva exits are not observed. A local coding agent can follow the Markdown's image paths; a browser chat cannot read arbitrary PC paths and needs the files uploaded.

## Runnable checks

Use a dedicated `CARGO_TARGET_DIR` and the disposable offscreen environment described in the repository. These commands exercise production code with private SQLite databases, synthetic capture and independent browser/provider peers; they do not use a physical microphone or a paid speech provider.

```sh
make linux-test
bash linux/tests/run_application_smoke.sh conversation
bash linux/tests/run_application_smoke.sh application
bash linux/tests/run_application_smoke.sh screenshots
bash linux/tests/run_application_smoke.sh agent-voice
bash linux/tests/run_application_smoke.sh account
npm --prefix hosted run check
npm --prefix hosted run format:check
npm --prefix hosted test
npm --prefix hosted run test:browser
bash linux/build-narrated-editor.sh "$PWD/tmp/narrated-editor"
```

The core persistence contract checks real Markdown/image paths, separate capture/save/close timestamps, immutable recognition, private file permissions, edit refresh and deletion without resurrection. The GTK tests cover autosave/cursor continuity, navigation, storage failure/retry, Copy as path and screenshot/editor lifecycle. The browser suite covers typing during a held save acknowledgement, blank working text with retained originals, reconnect/retry, conflicts, three receivers, Incognito and revocation. The actual patched Tensaku binary was also exercised on a private X11 display: Close and Escape both exited successfully while the image and private clipboard stayed unchanged.

Results on Lenovo:

| Check | Result |
| --- | --- |
| `make linux-test` | Pass: optimized bundle, 156 tests, both strict Clippy configurations, formatting and generated-feature consistency; 66 opt-in tests remain ignored in this command. |
| Private GTK conversation/application/agent-voice | Pass: real editor/storage behavior, original retention, autosave, navigation and routing controls. |
| Private GTK screenshot lifecycle | Pass: 16 workflows / 75 application states, actual file-save observation, close-without-save, SQLite failure recovery and late cancellation. |
| Private native account + browser peers | Pass: native login/callback/sign-out, three-device continuity and account isolation. |
| Hosted checks | Syntax/static JSON, formatting and 20 tests pass. |
| Real Chromium scenarios | [14 passing receipts](browser-receipts.txt), including history off, delayed acknowledgements and explicit conflict resolution. |
| Actual patched Tensaku | Release build passes; Close and Escape preserve image and clipboard. |

After the full native gate, the final empty-title adjustment was rebuilt in release mode and rechecked with strict workspace Clippy, formatting and the private conversation suite. An empty title retains the previous one and never traps the user in the editor. The ordinary application and agent-voice checks were run before the later title-recovery refinement; the final conversation suite exercises that refinement against real SQLite failures.

## Visual review

Impeccable's distillation and craft-floor checks were applied to the actual native/web states. Save controls are absent, the editor retains its working space, status feedback remains visible, and the native Copy as path action fits the narrow layout. The existing Mluva visual system and editor layout are preserved. All images below contain synthetic text or shapes.

| State | Evidence |
| --- | --- |
| Native editing, laptop | [Screenshot](screenshots/native-editing-laptop.png) |
| Native editing, narrow window | [Screenshot](screenshots/native-editing-narrow.png) |
| Web editing, phone | [Screenshot](screenshots/web-editing-phone.png) |
| Web editing, laptop | [Screenshot](screenshots/web-editing-laptop.png) |
| Screenshot editor with Close | [Screenshot](screenshots/screenshot-editor.png), [exit receipt](editor-exit-receipt.json) |

## Related local investigation

A separate named Herdr server, `mluva-voice`, runs the existing guarded-protocol build (Herdr 0.9.5, protocol 22). A new **Mluva voice** tab attaches to it without replacing existing sessions. The real configured Codex session accepted a guarded text submission and returned the requested response. This establishes local server discovery and text delivery; it does not establish physical F8/microphone operation. The previously installed native service was started with its portal shortcut registration disabled; no claim of an active F8 route is made here.

The production sign-in configuration, Cognito authorization page and registered native `mluva://account` handler were checked without exchanging personal credentials. The local Secret Service default collection is unlocked. The reported interactive sign-in failure has not been reproduced or fixed. The native account test uses isolated OAuth/API peers and verifies callback/login, device sync and sign-out; it cannot establish the outcome of Daniel's real browser login.
