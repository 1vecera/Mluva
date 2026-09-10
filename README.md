![Mluva — Speak a rough idea. Shape it into useful text.](docs/assets/mluva-hero.svg)

**Native dictation and rewriting for Linux. At home on Omarchy.**

Speak freely, turn the result into a useful draft, and keep every original word. Mluva brings recording, editable rewrites and searchable history into a quiet native workspace that follows your desktop theme.

**[Install 1.0](#install)** · **[Watch the 55-second film ↗](https://1vecera.github.io/Mluva/#film)** · **[Choose your providers](docs/provider-selection.md)** · **[Release notes](https://github.com/1vecera/Mluva/releases/tag/v1.0.0)**

[![Watch Mluva in action — a 55-second introduction to dictation, live drafts and a workspace that follows your Omarchy theme](docs/promotion/assets/delight/opening-preview.png)](https://1vecera.github.io/Mluva/#film)

<sub>Click the image to open the film with playback controls and captions. Native app footage, synthetic narration and edited timing; see the <a href="docs/promotion/README.md">media guide</a> for sources and capture details.</sub>

## From a thought to a finished draft

| Start with… | Make it useful |
| --- | --- |
| A rough idea | Dictate with F9. Scribe streams words while you speak; completed text copies to the clipboard. |
| Text you already have | Paste into a conversation, then choose **Polish**, **Structure** or your own instruction. |
| A note that needs shape | Enable **Live rewrite** to build a task spec, structured note or custom draft as speech arrives. |
| A draft worth keeping | Edit originals and rewrites, save with **Ctrl+S**, and keep raw recognition available separately. |
| Work to return to | Find conversations through History, reuse saved prompts, and export Markdown or JSON. |

**A calm place to work.** Stable Live panes, restrained recording motion and subtle Markdown keep long notes readable. **Ctrl+P** finds actions; **Ctrl+Enter** sends a rewrite. Copy and scrolling preferences are yours to change.

**Your choice of engines.** Select speech and rewriting independently: ElevenLabs Scribe, local Voxtype/Whisper or a compatible transcription API; native Codex app-server or a LiteLLM/OpenAI-compatible service for rewriting. Availability, credentials and models are configured in Settings.

Live rewrite is opt-in and Experimental. It marks missing information, preserves manual edits and reconciles the draft with the final transcript when recording stops. [Explore the features and their limits →](docs/feature-story.md)

## Install

Mluva 1.0 is a **Linux source release with a per-user installer**. Install the [desktop dependencies](linux/README.md#supported-desktop-contract), then:

```sh
git clone --branch v1.0.0 --depth 1 https://github.com/1vecera/Mluva.git mluva
cd mluva
make linux-install
mluva
```

Prefer an archive? The [1.0 release](https://github.com/1vecera/Mluva/releases/tag/v1.0.0) includes a compact source package and SHA-256 checksum. Choose your [speech and rewrite providers](docs/provider-selection.md) and supply any required credentials through your secret manager. Local Whisper needs an installed model; cloud services need their own account or deployment.

**Upgrading from 0.x:** quit Mluva before installing. The installer migrates the old product identities and retains a private backup of settings, conversations, drafts and audio. Desktop permissions may need approval again. Read the [migration guide](docs/identity-migration.md) before upgrading a customized installation.

### On Omarchy

After installing and starting Mluva, add the [community plugin](https://github.com/1vecera/omarchy-mluva) on Omarchy Quattro:

```sh
omarchy plugin add https://github.com/1vecera/omarchy-mluva.git --enable
```

The floating widget shows five lines while you speak. When you finish, rewrite, copy or open the note in the workspace. Hover, keyboard focus and active rewrites pause its configurable four-second dismissal. **Shift+F9** reopens the latest conversation when configured.

![The compact Omarchy widget with rewrite actions, Copy, Open and a countdown ring](docs/promotion/assets/widget-review.png)

Closing the main window keeps Mluva available; quit from the shell or application menu. The [Omarchy guide](docs/omarchy-integration.md) covers dependencies, existing manual installs and removal.

## Platform support

| Platform | Release boundary |
| --- | --- |
| Fedora 44 · GNOME 50 · Wayland | Recording, transcription, recording setup, History and custom saved styles have been manually accepted. |
| Omarchy · Hyprland · Quickshell | Theme integration, the widget and workspace are tested in isolation. Full live desktop acceptance remains pending. |
| macOS 14+ | Swift source preview with Apple Speech and Google Cloud recognition. No signed, notarized public binary. |

Version 1.0 establishes the Mluva identity and Linux source distribution. Conversation rewriting, generated titles, Meeting mode, Omarchy integration and advanced providers remain **Experimental** where indicated. Automatic insertion is off by default and is not reliable in the Fedora acceptance setup; the clipboard is the dependable delivery path. See the [capability matrix](docs/feature-maturity.md).

## Your text and the cloud

- **Speech and rewriting:** your selected providers determine where processing happens. Local Whisper keeps recognition local; cloud speech sends audio to that provider. Rewriting sends the selected text to your rewrite provider.
- **Local history:** originals, completed rewrites and titles stay together in a local SQLite database. Rename, export or delete conversations. Automatic titles send up to 6,000 characters from each new conversation to the rewrite provider; disable them in Settings to use local labels.
- **Incognito:** no saved history or recovery audio, conversation rewriting or generated titles. Recognition still uses your selected speech provider; cancellation cannot recall audio already sent.

See the [product contract](docs/product-contract.md) for retention, recovery and privacy details.

## Development

Linux uses **Python, GTK 4, Libadwaita and PipeWire**, with a QML plugin for Omarchy. macOS uses Swift. Start with the [code map and focused checks](CONTRIBUTING.md#code-map), [Linux guide](linux/README.md) or [macOS source guide](docs/macos-development.md).

```sh
make linux-test linux-shortcut-test
shellcheck linux/*.sh linux/tests/*.sh scripts/*.sh linux/mluva-shell
```

Use the [isolated development runners](dev/README.md) for UI verification and reproducible media. Bug reports should include the platform, reproduction steps and expected behavior. Keep private recordings, transcripts and credentials out of public issues.

Mluva means *speech* or *manner of speaking* in Czech, pronounced roughly “MLOO-vah.” **[Apache License 2.0](LICENSE)** · [Third-party notices](THIRD_PARTY_NOTICES.md) · [Changelog](CHANGELOG.md)
