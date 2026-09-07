![Mluva — Speak freely. Write clearly.](docs/assets/mluva-hero.svg)

Turn a spoken thought into text you can use. Press **F9**, say what you mean, and paste the result. Then polish the wording, organize a note, or ask for another draft. Mluva keeps your original and every completed rewrite together.

**[Install on Linux](linux/README.md#supported-desktop-contract)** · **[Omarchy integration](docs/omarchy-integration.md)** · **[Feature status](docs/feature-maturity.md)** · **[Contribute](#development)**

![Mluva's compact conversation workspace with searchable history, an original transcript and a structured rewrite](docs/assets/conversation-workspace.png)

<sub>Production Linux UI with synthetic example text. The app follows your Omarchy palette or system light/dark preference.</sub>

## From a thought to a finished draft

| You want to… | Mluva gives you… |
| --- | --- |
| Get an idea down quickly | Background dictation, live words and a completed result on the clipboard. |
| Clean up the wording | **Polish** removes filler and repairs phrasing; your source stays available. |
| Make sense of a long note | **Structure** creates a summary and organized points. |
| Keep your own voice | Custom instructions, follow-ups, saved prompts and a Copy action for each version. |
| Find it later | Automatic conversation titles and search across originals, rewrites and instructions. |

On **Omarchy**, a compact widget shows the latest three lines while you speak. When you finish, rewrite directly from the widget. Its countdown ring closes the review after eight idle seconds; hovering, keyboard focus, menus and active rewrites pause it. **Shift+F9** reopens the latest conversation when that shortcut is configured.

![Omarchy review widget with Polish, Structure, More, Copy, Open and a countdown ring](docs/assets/omarchy-review.png)

Closing the main window keeps Mluva available. Open the shell menu or application menu to quit.

## Try it on Linux

Install the [Linux dependencies](linux/README.md#supported-desktop-contract), provide `ELEVENLABS_API_KEY` through your secret manager, and optionally authenticate a local Codex installation for rewriting and generated titles. Then:

```bash
git clone https://github.com/1vecera/Mluva.git mluva
cd mluva
make linux-install
mluva
```

The installer installs for your user. Dictation copies completed text automatically. **Ctrl+Enter** sends a custom rewrite; rewriting changes the clipboard only when you choose **Copy**. See the [Linux guide](linux/README.md) for microphone selection, language, shortcuts, saved styles and recovery.

## Where things stand

Mluva is an early open-source project. The status is specific to each capability:

| Platform | Current boundary |
| --- | --- |
| Fedora 44 · GNOME 50 · Wayland | Recording, transcription, recording setup, History and custom saved styles have been manually accepted. |
| Omarchy · Hyprland · Quickshell | Theme integration, streaming review controls and the compact workspace are implemented and tested in isolation; live desktop acceptance remains pending. |
| macOS 14+ | Swift source preview with Apple Speech and Google Cloud recognition. No current signed, notarized public binary. |

Conversation rewriting, generated titles, Meeting mode and other advanced surfaces remain **Experimental**. Automatic insertion is disabled by default and is not yet reliable in the Fedora acceptance setup; the clipboard is the dependable delivery path. The [capability matrix](docs/feature-maturity.md) tracks these boundaries.

## Your text and the cloud

- **Recognition on Linux:** microphone audio goes to ElevenLabs Scribe. This is cloud dictation.
- **Rewriting and titles:** text goes through your locally authenticated Codex app-server. Automatic titles use up to 6,000 characters from each new conversation; disable them in Settings to keep local text labels. Existing history is not sent in bulk.
- **Local history:** originals, completed rewrites and titles stay together in a local SQLite database. You can rename, export or delete them. A manual title takes precedence over an automatic one.
- **Incognito:** Mluva saves neither history nor recovery audio and disables conversation rewriting and generated titles. Recognition still uses ElevenLabs; cancellation cannot recall audio already sent.

Audio retention, recovery, Command previews and other details are documented in the [product contract](docs/product-contract.md) and [Linux guide](linux/README.md).

## Development

Linux uses **Python, GTK 4, Libadwaita and PipeWire**, with a separate QML plugin for Omarchy. macOS uses Swift. No webview is required for the conversation workspace.

```bash
make linux-test linux-shortcut-test
shellcheck linux/*.sh linux/tests/*.sh scripts/*.sh linux/mluva-shell
```

The [workspace contract](docs/conversation-workspace.md) and [Omarchy guide](docs/omarchy-integration.md#verification) describe private-display integration tests, synthetic model subprocesses and the remaining manual checks. See the [UI design notes](docs/ui-design.md) for the decisions behind the layout. For macOS source setup and packaging, see [the macOS guide](docs/macos-development.md).

Bug reports are most useful with the platform, reproduction steps and expected behavior. Keep recordings, transcripts and credentials out of public issues. For larger changes, open an issue first to agree on scope.

Mluva means *speech* or *manner of speaking* in Czech. Pronounced roughly “MLOO-vah.” Released under the **[Apache License 2.0](LICENSE)**; see [third-party notices](THIRD_PARTY_NOTICES.md) and the [changelog](CHANGELOG.md).
