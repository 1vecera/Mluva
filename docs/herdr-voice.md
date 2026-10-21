# Talk to a Herdr agent

This experimental native Linux feature sends a finished dictation to a selected local Codex or Claude Code session. It needs the [guarded Herdr integration](../integrations/herdr/README.md). Released Herdr 0.9.1 and unpatched 0.9.5 cannot safely support automatic sending; Mluva shows their metadata but disables delivery. No account is needed, and enabling routing never changes the selected speech provider.

Open **Settings → Capture → Agent routing** and enable agent dictation. Changes save automatically. The recipient dropdown appears below the existing editor. Choose a session by its Herdr name, agent type, project and pane; the full local socket is included in its tooltip to distinguish servers. The choice is remembered, but a restarted or renamed agent must be selected again. Mluva never matches a replacement by name alone.

The installed CLI can inspect and switch the running app's recipient, or toggle agent dictation, without opening a window or restarting either application:

```sh
mluva herdr list
mluva herdr select cos
mluva herdr status
mluva herdr select w1:p2 --socket /absolute/path/to/herdr.sock
mluva herdr clear
mluva herdr record
```

Commands print JSON and exit nonzero on failure. Mluva must already be running. `select` discovers a fresh guarded identity, enables routing and updates the live dropdown and saved choice together. Use an exact pane ID for duplicate names, adding `--socket` when multiple servers match. Unavailable or ambiguous recipients are rejected. `status` checks the saved identity against the current server; a saved name alone does not mean connected. `clear` removes the recipient. A switch request disarms pending automatic delivery and preserves the draft; list, status, select and clear never send a prompt; record starts or stops agent dictation and sends its finished text. Shortcuts, speech provider and optional spoken status stay unchanged. Incognito changes remain in memory.

The configured agent key (F8 by default, configurable to F12) first opens a session wheel without recording or sending. The remembered session is selected and named in the centre with its pane ID, status and record key. Press the key again to record, then again to stop and send. A S D J K L select the six clockwise choices on the current page: the left-hand keys rise up the left side and the right-hand keys descend the right, with each session's pane ID beneath its name. Blocked or missing sessions are marked unavailable. Left/Right page through longer lists and Escape cancels. Choices follow Herdr workspace/tab/pane-layout order, and a moved or missing session is never replaced by its neighbour. The normal **Dictate to agent** button and `mluva herdr record` retain their direct start/stop behavior.

From the wheel, the normal dictation key (F9 by default) starts a transcript without direct agent delivery. Its provisional destination is Mluva’s editable document: no automatic copy or paste. Press the dictation key again, or Stop, to finish; normal History, Incognito, retention and opted-in account-sharing policies still apply. The exact alternate destination awaits confirmation; it is isolated from agent routing. The [wheel acceptance record](verification/f12-session-wheel/README.md) covers selection, keyboard routing and its limits. Releasing the key or moving focus does not stop recording. Settings → Capture → Global shortcuts contains the normal dictation key, agent key and Keep draft key; each change saves automatically. The assigned desktop binding is displayed there. Both agent keys can be distinct F1–F24 keys, excluding the dictation key and F10.

Final recognition appears in the normal document editor with a full two-second **Sending to…** countdown. **Cancel** stops recording, recognition or the pending send. **Keep draft**, or **F7**, finishes capture and keeps editable text. Editing, changing recipient, navigating away, a missing/blocked session, screen lock or interrupted timing also disarm automatic sending. A five-minute recording safety limit stops automatic sending and retains the draft. Starting another recording replaces the pending send, while local saved conversations remain in History.

**Send draft** starts a fresh two-second window for the current text and selected recipient. Edits save automatically in the existing conversation store and preserve immutable recognition. Normal retention and account-sharing choices still apply. Routing never copies or pastes automatically, even when ordinary dictation's automatic clipboard settings are enabled.

| Status | Meaning and recovery |
| --- | --- |
| Sent | Herdr acknowledged writing the message and Enter to the same agent. Completion is separate. Working agents can accept the message without being stopped. |
| Recipient changed or unavailable | The destination disappeared, changed identity, was renamed or became blocked. Keep the draft, choose the intended available session and explicitly send it. |
| Delivery unconfirmed | Some input may already have reached the recipient. Mluva reads the receipt but never resends automatically. Check that session in Herdr; Send draft stays disabled for this attempt. Copy remains available. |
| Integration needed | Install the reviewed Herdr dependency after approval. There is no legacy prompt, clipboard, key-injection or focus fallback. |
| Interrupted | A lock, suspend, delayed timer or shutdown invalidated the send. Relaunch never replays pending messages. |

The selected destination stays fixed during capture; **Mluva never stops or pauses the recipient agent**. Once submission begins, cancellation cannot retract it. A guard failure after a partial write leaves delivery unconfirmed; inspect and clear any pending recipient input yourself. The client does not send Escape, Ctrl+C or an answer to permission prompts.

## Optional spoken status

Choose an ElevenLabs voice ID in Settings → Capture → Agent routing and explicitly enable **Speak a brief agent status update**. This uses `eleven_v4_turbo` through Text-to-Dialogue WebSocket, with the same saved ElevenLabs key used by native speech. That key also needs `text_to_speech` permission; a transcription-only key produces a specific error and leaves the text visible. Voice is disabled by default and may incur provider charges. No model substitution occurs.

**Read status** shows one metadata-based sentence about the selected agent. When speech is enabled it also plays that sentence. After an accepted message, one later blocked/done transition can speak once. Herdr currently provides no correlated response text, so this feature describes activity and never claims that a generic completed turn answered the dictated message. Replay uses the visible sentence; Mute stops playback independently of text status. Sentences are English; Czech voice quality is still an acceptance item.

Only the displayed sentence goes to ElevenLabs: no microphone audio, terminal history or prompt text is read for this purpose. The configured STT provider retains its existing local/cloud semantics. Reply audio is streamed in memory to PipeWire, bounded to 25 seconds/30 seconds of network work, and never saved by the app. A new dictation or Meeting capture cancels and reaps the player before opening audio capture. Incognito suppresses spoken status and creates no Mluva History or retained recording; a recipient can still retain messages explicitly sent to it.

Remote Herdr servers, browser Unix-socket access and browser-wide hotkeys are outside this first native implementation. Physical Wayland shortcut events, microphone feedback and natural English/Czech listening acceptance remain separate from the [isolated evidence](verification/herdr-voice/README.md).
