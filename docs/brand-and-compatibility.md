# Mluva brand and compatibility contract

The public product name is **Mluva** (roughly “MLOO-vah”), the Czech noun for speech or manner of speaking. The category descriptor is **Open-source AI dictation for Linux, with a source preview for macOS.** Public copy, application titles, launchers, distributable filenames, screenshots, and release notes use Mluva.

## Upgrade continuity

The first Mluva release intentionally retains several Voice Scribe-era technical identifiers. Changing them during a visual rename would reset desktop approvals, split local history, or disconnect an already reviewed secret reference. They are compatibility contracts, not alternate public names.

- macOS keeps the `VoiceScribeMac` Swift target and executable, the `com.voicescribe.mac` bundle identifier, the `VoiceScribe` Application Support directory, and the existing local signing and notarization profile names. The bundle and installed application are displayed as `Mluva.app`.
- Linux keeps the `voice_scribe_linux` Python module, `com.voicescribe.Linux` application and D-Bus identity, `voice-scribe` XDG storage roots, `recording-status@voicescribe.local` GNOME extension UUID, `voice-scribe-input@.service` systemd unit, and the owner-only `voice-scribe` managed-secret profile.
- Linux installs `mluva`, `mluva-input-helper`, and `mluva-overlay` as the canonical commands. The previous `voice-scribe`, `voice-scribe-input-helper`, and `voice-scribe-overlay` commands remain compatibility aliases so existing shortcuts and instructions do not fail abruptly.

Any later identifier migration must copy or adopt existing data atomically, preserve permissions, remove stale launch entries, and verify upgraded installations before the legacy identifiers are retired.

## Naming boundaries

Mluva is the project’s brand, not a claim that transcription runs locally. Linux recognition currently crosses the ElevenLabs boundary; optional cleanup and Command mode cross the locally authenticated Codex app-server boundary. macOS can use on-device Apple Speech or configured Google Cloud recognition. Product copy must preserve those distinctions.

Preliminary exact-name research found no current software or dictation product named Mluva. It did find the separate Czech communication platform `mluvii` and the phrase and registered mark `Nová mluva`. This research is a collision screen, not legal or trademark clearance.
