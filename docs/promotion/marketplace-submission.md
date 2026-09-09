### Repository URL

https://github.com/1vecera/omarchy-mluva

### Category

Productivity

### Tags

ai, bar, quickshell

### Suggest a missing tag

_No response_

### Maintainer notes

Mluva dictation 0.2.1, exported from Mluva v0.1.1. The root manifest and both QML files are byte-for-byte copies of the tagged application source; SOURCE.json records the source commit and SHA-256 hashes. A fresh public clone passes omarchy-plugin-validate from Omarchy commit e848f1df97bbbe23db42fc2b1fb04937d7a0eae0. The README now describes the separate application setup directly; this resolves a scanner match on a negated privilege statement without changing runtime code.

Requires the separately installed and running Mluva Linux app (v0.1.1 or compatible), its mluva-shell command, and Omarchy Quattro's Quickshell shell. The plugin has no install hooks, sudo calls or configuration-writing code. It invokes a configurable local bridge executable with explicit argument arrays and uses the app's existing D-Bus actions. The bounded preview is volatile and is not logged or persisted by the plugin.

The app uses ElevenLabs cloud recognition with the user's API key and optional authenticated Codex rewriting. Those external dependencies, data flows, installation, upgrade, duplicate-ID migration and removal are documented in the README. The plugin itself holds no provider credential and makes no direct network request.

Automated checks exercised the real production widget and bridge in a private X11/D-Bus session: five-line scrolling, countdown interactions, original preservation, a real rewrite action through a scripted local provider, and deliberate Copy. Live Hyprland, physical F9 and microphone acceptance remain pending; the integration is explicitly Experimental. The preview is a composed capture of real GTK and Quickshell UI using scripted content, not a speech-recognition benchmark. Community integration; no official Omarchy affiliation is claimed.

### Submission checklist

- [x] The repository is public and contains installation and removal instructions.
- [x] I have documented the plugin license and any external dependencies.
- [x] I confirm that I own or have permission to submit this plugin and its preview assets.
- [x] The plugin does not overwrite user configuration without explicit consent.
- [x] I understand that approval is for listing and is not a security review.
