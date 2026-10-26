# Mluva for macOS

A native menu-bar app for macOS 26 with Liquid Glass, drawn in its own [drafting language](DESIGN.md): JetBrains Mono as on Linux, keys drawn as legends, hairline rules and leader lines, graphite glass for what floats, and red only for what is live, chosen or marked. It shares settings, History, personalization, Scribe recognition, agent discovery and delivery, and the screen annotation planner with the Linux app through the Rust core; Swift owns what is bound to the Mac desktop.

| Area | macOS behavior |
| --- | --- |
| Dictation | F9 starts and stops anywhere. ElevenLabs Scribe Realtime streams live words into the floating pill; if it fails, the whole recording is sent as one upload. Incognito keeps audio off disk and has no upload fallback. |
| Delivery | The text is copied first, then pasted with ⌘V only into the exact field and window that had focus at the start, and never into a secure field. Without Accessibility it stays copied. |
| Agents | F12 opens the session wheel with the shared [target model](../docs/agent-routing.md): Herdr's panes, Claude Code sessions from Claude Code's own session list, live Codex threads and a new Claude chat. A letter key or click chooses one; F12 again records for it. After a two-second cancel window, which F12 ends early, the text goes through the session's own transport: Herdr's guarded prompt sends, Claude Code's inbox queues, Codex sends to an idle thread and Claude opens a new chat with it. A session nothing reaches gets an editable draft. History records what became of it. |
| Screen annotation | While recording, hold Shift and move to draw; Shift-click drops a numbered pin. Captures leave out Mluva's own windows, raw frames stay in memory, and marked recordings deliver keyframes and a summary with the text. Needs Screen Recording; off in Incognito. |
| History | Search, edit with autosave, the original recognition kept apart, keyframes beside their dictation, Markdown export and delete. |
| Settings | General, Speech (Keychain-stored ElevenLabs key), Shortcuts, Agents, Annotation, Vocabulary and Privacy, saved at once in the Linux file format. |

## Build and test

Requires macOS 26, Swift 6.2 or newer (the Command Line Tools are enough) and the repository's Rust 1.95 toolchain.

```sh
macos/scripts/test.sh                              # Rust bridge library, then Swift tests
cargo test -p mluva-ffi                            # the bridge against real temporary stores
macos/scripts/build-app.sh release --universal     # .build/app/Mluva.app for Apple silicon and Intel
```

`build-rust.sh` builds the `mluva-ffi` static library for each architecture; `build-app.sh` builds the Swift package per architecture, joins them, adds the icon and menu-bar mark, and signs ad hoc with the hardened runtime and the microphone entitlement. A Developer ID signature and notarization are not part of the build.

## How it fits together

`rust/mluva-ffi` is a C ABI over the shared core: `mluva_call` takes a JSON method and parameters and returns a JSON result; long work returns a task identifier and reports through one event callback, as do realtime previews and agent deliveries. The bridge keeps the targets of the latest discovery, so what a transport needs and never saves, such as a Claude Code inbox, stays in the core; the app names a target by the record it was shown. PCM goes in through `mluva_realtime_audio` as signed little-endian 16 kHz 16-bit mono. Files live in `~/Library/Application Support/Mluva/config` and `data`, owner-only, with the same names and formats as the Linux XDG layout.

| Change | Owner |
| --- | --- |
| Bridge methods and their tests | `rust/mluva-ffi` |
| What differs on macOS in reaching agent sessions | `rust/mluva-providers/src/agent_platform.rs` |
| Bridge client, models, Keychain, permissions, shortcuts, login item | `macos/Sources/MluvaKit/Core`, `System` |
| Microphone, recording, recognition fallback, delivery | `macos/Sources/MluvaKit/Dictation` |
| Session wheel layout and agent route | `macos/Sources/MluvaKit/Agents` |
| Ink overlay, frame capture and keyframe rendering | `macos/Sources/MluvaKit/Annotation` |
| Menu bar, pill, wheel, History, Settings, Welcome | `macos/Sources/Mluva` |

## Permissions

The microphone is the only requirement. Accessibility adds pasting into the captured field and Screen Recording adds screen annotation. Input Monitoring is not requested: the function-key shortcuts are Carbon hot keys, and the Shift that draws is read as modifier state. The Welcome window asks for the microphone and Accessibility; Settings → Privacy requests any of them and opens the right System Settings pane when one was denied.

## Evidence mode

`MLUVA_EVIDENCE=<directory>` runs the real app on synthetic data in a temporary home, opens each surface in light and dark, saves screenshots and a short recording of its own windows, and quits. It registers no shortcuts, opens no microphone, contacts no provider or Herdr server and reads none of this Mac's own agent sessions. macOS lets an app capture its own windows over the wallpaper without Screen Recording, so it needs no grant. Use it instead of driving the desktop. The current set is in [docs/verification/macos-app](../docs/verification/macos-app/README.md).

## Limits

- Herdr releases without the guarded prompt API are discovered, but their panes are draft-only unless another transport reaches the session, as on Linux. The [guarded patch](../integrations/herdr/README.md) applies to Herdr 0.9.5 and builds and passes its guarded, schema, wire and PTY tests on macOS with Zig 0.16.0.
- Mluva starts no Codex app server on the Mac; Codex threads are reached through Codex's own daemon when it runs, and are draft-only otherwise.
- A Claude Code session in bypass-permissions mode holds inbox messages until you approve them there.
- A Shift-click during recording pins instead of reaching the app underneath; that is how the overlay keeps pins out of other apps.
- The build is signed ad hoc. Distribution to other Macs needs a Developer ID signature and notarization.
- Local speech models, meetings, narrated screenshots and the hosted companion remain Linux-only.
