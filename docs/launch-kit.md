# Mluva launch kit

Copy for v0.1.1, prepared on 2026-09-08. Social and community posts are drafts for Daniel to review. The app release and plugin repository are public; marketplace approval is tracked separately.

## Positioning

**Headline:** Speak freely. Stay in flow.

**One line:** Mluva brings live dictation, quick rewrites and recoverable originals to your Omarchy workflow.

**Short description:** Talk through a coding task, a note or an idea. Mluva shows your words in a five-line floating preview, copies completed dictation, and offers Polish, Structure and saved prompts. The native app follows your desktop palette and keeps the original with every completed rewrite.

**Developer promise:** Make talking through a task feel natural on Omarchy. Win on a readable preview, coherent desktop behavior and a clear path from a rough thought to usable text. “Best dictation for Omarchy” is the ambition; no measured accuracy, speed or compatibility lead is claimed.

## LinkedIn draft

I've been building Mluva into the dictation app I want on #Omarchy.

Talk through a coding task. Keep the rough version. Turn it into a clear note. Copy it into your editor or agent.

The bit I like most now is the little floating widget: five lines of live words, a translucent surface that follows the desktop theme, then Polish or Structure when you're done. The original stays there. So does every completed rewrite.

Native GTK app, Quickshell plugin, ElevenLabs Scribe for recognition and Codex for optional rewrites. Open source.

The ambition is the best dictation experience on Omarchy. It's an early release: the integration is still experimental, and the demo uses scripted content. Real recognition uses your ElevenLabs account.

Code and install: https://github.com/1vecera/Mluva

Plugin: https://github.com/1vecera/omarchy-mluva

## Short announcement draft

Mluva for Omarchy: five lines of live dictation, quick rewrites and your original kept intact. Native GTK + Quickshell. Open source; early experimental integration. ElevenLabs recognition, optional Codex rewrites. https://github.com/1vecera/Mluva

## Community post draft

**Title:** Mluva: native dictation and a five-line Quickshell preview for Omarchy

I've released Mluva v0.1.1 and published its Omarchy plugin. The workflow is dictation → clipboard → an optional rewrite, with the original and every completed version kept together. The widget follows the desktop palette, shows five live lines with eased scrolling, and lets you run Polish or Structure without opening the main window.

It uses ElevenLabs Scribe for speech recognition and an authenticated Codex app-server for optional rewrites. You need your own provider access. Linux recognition is cloud-based.

This is an early release. Isolated GTK, Quickshell and D-Bus checks pass, but live Hyprland acceptance remains pending. Automatic insertion is disabled by default; use the clipboard. The video shows the real UI with a scripted transcript and rewrite provider.

Install the app first, start Mluva, then add the plugin:

```sh
omarchy plugin add https://github.com/1vecera/omarchy-mluva.git --enable
```

App and setup: https://github.com/1vecera/Mluva

Plugin source and removal: https://github.com/1vecera/omarchy-mluva

Useful feedback would name the Omarchy/Hyprland version, keyboard layout and target app, then describe start, stop, rewrite and Copy. Keep real transcripts and recordings out of public issues.

## Assets and release links

- [Three videos and eight screenshots](promotion/README.md), with dimensions, alt text and reproduction instructions.
- [Mluva v0.1.1](https://github.com/1vecera/Mluva/releases/tag/v0.1.1), with source archive and checksum.
- [Installable plugin](https://github.com/1vecera/omarchy-mluva) and [marketplace submission](https://github.com/omacom/omarchy-plugin-marketplace/issues/5533).
- [Feature maturity](feature-maturity.md) and [Omarchy verification boundary](omarchy-integration.md#verification).

## Next evidence to earn

- Record live Omarchy acceptance for F9 on US/Czech layouts, microphone stop/cancel, focus retention, theme changes and a session restart.
- Check clipboard delivery in named browser, editor and terminal applications. Keep automatic-insertion claims separate.
- Compare a declared task set using consented, reusable audio. Publish the corpus and actual recognition errors, correction effort and timing before a comparative “best” claim.
- Supplement the scripted showcase with an identified real-audio demonstration after that acceptance.

## Claim boundaries

Keep Omarchy integration, rewriting and generated titles labeled Experimental until their own acceptance is recorded. Fedora 44 / GNOME 50 / Wayland remains the manually accepted recording baseline. macOS remains a source preview without a signed public binary. Do not present the plugin as official Omarchy software, an approved marketplace listing before approval, offline recognition, universal auto-paste or a measured performance winner.
