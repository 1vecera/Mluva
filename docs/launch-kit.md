# Mluva launch kit

Copy for v0.3.0, prepared on 2026-09-09. Social and community posts are drafts for Daniel to review. The release and plugin are public; the asset manifest records the version, audio sources and capture method for each demonstration.

## Positioning

**Direction:** Build the cleanest, smartest dictation app for Omarchy.

**Headline:** Speak freely. Watch your thoughts take shape.

**One line:** Mluva turns spoken thoughts into editable notes, clear drafts and structured tasks in a minimal Omarchy workspace.

**Short description:** Talk through an idea. Keep the original, refine the wording, or see a structured draft emerge while you speak. Mluva pairs a small translucent recording preview with a native editing workspace. Choose your speech and rewrite engines, save the useful version, and carry it into your next task.

**Product character:** As direct as a terminal interface, with the comforts of a small native GUI. A readable five-line preview, restrained motion, your Omarchy palette, and advanced controls revealed when needed. The ambition is a clean, smart everyday interface for computing; comparative accuracy and speed claims need a reproducible benchmark.

## LinkedIn draft

I'm building the cleanest, smartest dictation app for #Omarchy. It's called Mluva, and I'm using it all the time as an interface for computing.

Talk through a task, including the bits you haven't figured out. Live rewrite starts putting it into a spec while you're still speaking. Missing owner? Missing deadline? Those stay visible instead of quietly becoming invented requirements.

That's the part I care about: getting useful structure out of a thought before I've finished tidying it up in my head.

The UI stays small. Five lines in a translucent recording widget, a native GTK workspace for the original and rewritten text, and room to edit either. It follows the Omarchy theme and feels close to a terminal interface.

v0.3.0 adds editable documents, automatic copy, live task specs and notes, plus separate speech and rewrite engines. ElevenLabs Scribe, local Voxtype/Whisper, Codex or a LiteLLM-compatible service. Bring the provider access you use.

It's early. Live output arrives in updates, with latency depending on the speech engine and model. I'm making that faster and making provider selection simpler next.

[Source and install](https://github.com/1vecera/Mluva) · [v0.3.0](https://github.com/1vecera/Mluva/releases/tag/v0.3.0)

## Short announcement draft

Mluva v0.3.0 for Omarchy: speak, rewrite and shape a task while you're still thinking through it. Editable originals and drafts, live structure with missing details marked, a small translucent preview, and your choice of speech and rewrite engines. Native GTK + Quickshell. Open source; early experimental integration. [Try Mluva](https://github.com/1vecera/Mluva/releases/tag/v0.3.0).

## Community post draft

**Title:** Mluva v0.3.0: live structured dictation in a small Omarchy workspace

I've released Mluva v0.3.0 and updated its Omarchy plugin. Dictate into a five-line preview, polish a note, or enable Live and watch a task spec take shape while you speak. Both the original document and the rewrite are editable. Raw recognition stays available, and completed text copies automatically by default.

The app is native GTK, with a Quickshell widget that follows the desktop palette. Speech and rewriting are separate choices: ElevenLabs Scribe or a compatible speech endpoint, local Voxtype/Whisper, and native Codex or a LiteLLM-compatible rewrite service. Local recognition is available; whether a rewrite leaves your machine depends on the engine you select.

Live rewrite produces periodic drafts rather than an update for every word. Batch speech engines also wait for audio chunks. The integration and advanced features remain Experimental, with exact verification recorded in the repository.

Install and start the app, then add the plugin:

```sh
omarchy plugin add https://github.com/1vecera/omarchy-mluva.git --enable
```

[Release and source archive](https://github.com/1vecera/Mluva/releases/tag/v0.3.0) · [Provider and Live setup](providers-and-live-rewrite.md) · [Plugin](https://github.com/1vecera/omarchy-mluva)

## Product intro

Use real v0.3.0 frames from the Lenovo/Omarchy capture. Keep product text and controls readable; use the H3 Max animation for motion and transitions around those frames. Show the app's actual output in workflow shots. Keep any accelerated segment identifiable in the accompanying manifest.

| Beat | Picture | Copy |
| --- | --- | --- |
| Open | Omarchy theme and a quiet recording surface | Mluva. Speak freely. |
| Dictate | Public-domain speech drives the real transcription preview | Your words, ready to use. |
| Structure | Synthetic messy task speech drives an actual live draft | See the task take shape. |
| Refine | Editable source and completed draft, including a missing-detail marker | Keep the thought. Make it clear. |
| Close | Real app frame, held still long enough to read | Built for Omarchy. Open source. |

The full demonstration videos carry the original public or synthetic audio. The short intro can select and cut those moments. Publish each asset with its source audio, release commit, provider/model, editing notes and hashes in the manifest. The H3 Max experiment has a $5 total budget; record the actual request charge before claiming a cost.

## Assets and evidence

- [Promotion assets and capture provenance](promotion/README.md).
- [Mluva v0.3.0](https://github.com/1vecera/Mluva/releases/tag/v0.3.0), with source archive and checksum.
- [Installable plugin](https://github.com/1vecera/omarchy-mluva) and [marketplace submission](https://github.com/omacom/omarchy-plugin-marketplace/issues/5533). A submission is separate from marketplace approval.
- [Feature maturity](feature-maturity.md), [provider behavior](providers-and-live-rewrite.md) and [Omarchy verification](omarchy-integration.md#verification).

## Future story

The longer-term vision is an interface that helps Daniel think while he works. Agents may eventually talk back or ask a useful clarifying question. That is future direction. The current product story is structured dictation, editable notes and reliable handoff into the next task.

Next proof should show actual microphone and physical-shortcut behavior on named Omarchy setups, then a declared reusable audio corpus with recognition errors, correction effort and timing. Keep those claims separate from scripted visual checks and short public-audio demonstrations.
