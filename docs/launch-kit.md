# Mluva launch kit

Copy for the Omarchy launch direction, updated on 2026-09-09. Social and community posts remain drafts for Daniel to review. The v0.3.0 release and plugin are public; the asset manifest records the version, audio sources and capture method for each demonstration. The app name remains Mluva.

## Positioning

**Direction:** The most delightful dictation app for Omarchy.

**Headline:** The most delightful dictation app for Omarchy.

**One line:** Speak a rough idea. Shape it into useful text. Keep the original.

**Short description:** Talk through an idea, or paste text you already have. Polish it, structure it, or see a draft emerge while you speak. Mluva pairs a small translucent preview with a native editing workspace. Keep completed versions in searchable history, then copy the result or export the conversation as Markdown or JSON. Choose speech and rewriting independently.

**Product character:** As direct as a terminal interface, with the comforts of a small native GUI. A readable five-line preview, restrained motion, your Omarchy palette, and advanced controls revealed when needed. The ambition is a clean, smart everyday interface for computing; comparative accuracy and speed claims need a reproducible benchmark.

Delight is the design ambition. The [source-backed capability matrix](feature-story.md) records actual behavior, unfinished features, acceptance and deployment limits. Provider adapters do not establish universal cloud/account compatibility. Bundling with Omarchy and a dedicated default shortcut are future aspirations.

## LinkedIn draft

I'm building the most delightful dictation app I can for #Omarchy. It's called Mluva, and I'm using it all the time as an interface for computing.

Talk through a task, including the bits you haven't figured out. Live rewrite starts putting it into a spec while you're still speaking. In the recorded demo, the owner and deadline stay open because I haven't chosen them yet.

That's the part I care about: getting useful structure out of a thought before I've finished tidying it up in my head.

The UI stays small. Five lines in a translucent recording widget, a native GTK workspace for the original and rewritten text, and room to edit either. It follows the Omarchy theme and feels close to a terminal interface.

Already have the text? Paste it in and polish it. Open the full note from the widget, search the earlier versions, or export the conversation as Markdown or JSON.

Speech and rewriting are separate choices: ElevenLabs Scribe, local Voxtype/Whisper or a compatible transcription deployment for speech; Codex or a compatible chat deployment for rewrites. Bring the provider access you use.

It's early. Live output arrives in updates, with latency depending on the speech engine and model. The feature walkthrough uses example text; the separate provider recordings show actual recognition and model output. The repository records which source build each capture uses and which features still need acceptance.

[Source and install](https://github.com/1vecera/Mluva) · [v0.3.0](https://github.com/1vecera/Mluva/releases/tag/v0.3.0)

## Short announcement draft

Mluva for Omarchy: speak, rewrite and shape a task while you're still thinking through it. Paste existing text to polish, open the full draft, find it in history and export it. Native GTK + Quickshell, independent speech and rewrite providers. Open source; early experimental integration. [Try Mluva](https://github.com/1vecera/Mluva).

## Community post draft

**Title:** Mluva: dictation, rewriting and searchable notes in a small Omarchy workspace

I've released Mluva v0.3.0 and updated its Omarchy plugin. Dictate into a five-line preview, polish a note, or enable Live and watch a task spec take shape while you speak. Both the original document and the rewrite are editable. Raw recognition stays available, and completed text copies automatically by default.

You can also start with pasted text, open the full note from the floating widget, search your earlier versions, and export a saved conversation as Markdown or JSON. The feature walkthrough uses example text and a local provider fixture; the full provider recordings show actual speech recognition and model output with their source versions labeled.

The app is native GTK, with a Quickshell widget that follows the desktop palette. Speech and rewriting are separate choices: ElevenLabs Scribe, local Voxtype/Whisper or a compatible transcription deployment for speech; Codex or a compatible chat deployment for rewriting. Your endpoint and account need to support the chosen task. Local recognition is available; whether a rewrite leaves your machine depends on the engine you select.

Live rewrite produces periodic drafts rather than an update for every word. Batch speech engines also wait for audio chunks. The integration and advanced features remain Experimental, with exact verification recorded in the repository.

Install and start the app, then add the plugin:

```sh
omarchy plugin add https://github.com/1vecera/omarchy-mluva.git --enable
```

[Release and source archive](https://github.com/1vecera/Mluva/releases/tag/v0.3.0) · [Provider and Live setup](providers-and-live-rewrite.md) · [Plugin](https://github.com/1vecera/omarchy-mluva)

## Product intro

**Film caption:** Smart dictation. Built for Omarchy.

The new feature walkthrough uses **“Feature walkthrough · example text”** as its small on-screen label. Its media README and manifest disclose synthetic text and a local provider fixture. Keep it separate from the real-provider source clips below. Use the [transparent production lockup](assets/mluva-lockup.svg) on dark scenes, or the [solid ink lockup](assets/mluva-lockup-on-light.svg) on light scenes; PNG counterparts are beside them. The [brand contract](brand-and-compatibility.md) preserves the selected model source and software-refinement provenance.

The final film opens on a clean scenery-only H3 background, then software-composites real native UI with the crisp SVG logo and wordmark. A labeled cut moves forward to the actual saved Live result, while the historical source-build provenance remains preserved in the media manifest. Keep product text and controls readable; source excerpts play at their recorded speed and the manifest records cuts, compositing and crossfades.

| Beat | Picture | Copy |
| --- | --- | --- |
| Open | Omarchy theme and a quiet recording surface | Mluva. Speak freely. |
| Dictate | Public-domain speech drives the real transcription preview | Your words, ready to use. |
| Structure | Synthetic messy task speech drives an actual live draft | See the task take shape. |
| Refine | Editable source and completed draft, including a missing-detail marker | Keep the thought. Make it clear. |
| Existing text | Explicit paste, then Polish | Already written? Give it a polish. |
| Floating note | Hover pauses dismissal, then Open enters the workspace | A small note, with room to grow. |
| History / export | Search earlier versions; export Markdown or JSON | Find it again. Take it with you. |
| Providers | Independent speech and rewrite settings | Your speech engine. Your rewrite model. |
| Close | Real app frame, held still long enough to read | Built for Omarchy. Open source. |

The full demonstration videos carry the original public or synthetic audio. The short intro selects and cuts those moments. Each asset records its source audio, release commit or unreleased source hashes, provider/model, editing notes and hashes in the manifest. The coordinator's latest campaign ledger reserves $3.40 of the $5 cap: $1.20 for the original H3 Max clip, $1.00 for the GPT Image 2.5 Sunburst logo source, and $1.20 for the new 15-second H3 Max background with no UI or lettering. Actual bills are unavailable because the credential lacks Admin billing scope; reservations are estimates, not receipts. No further generations are planned.

The selected Live capture uses unreleased commit `cdc0023`, ElevenLabs Scribe v2 Realtime and native Codex `gpt-5.6-luna`, low effort, standard tier. An incomplete Intent field first painted 11.06 seconds after the first captured sample; a task with requirements appeared at 22.73 seconds, while the source speech continued. Stop occurred at 40.74 seconds and final reconciliation painted at 54.91 seconds. The final task includes the missing-ID, preview and totals requirements, with owner and deadline unresolved. These are distinct stages from one observed run, not a general latency or accuracy claim; source, recognition and draft wording remain available for review. A four-second speech-commit experiment dropped requirements and was rejected. The selected implementation keeps longer recognition commits and uses provisional speech only for the live draft before final reconciliation.

## Assets and evidence

- [Product intro](promotion/assets/mluva-product-intro.mp4), with its current duration, source clips and composition recorded in the media manifest.
- [Complete released speech demo](promotion/assets/v0.3.0/speech/workflow.mp4) and [complete unreleased Live demo](promotion/assets/unreleased-live/workflow.mp4), with source audio at recorded speed.
- [Promotion assets and capture provenance](promotion/README.md).
- [Mluva v0.3.0](https://github.com/1vecera/Mluva/releases/tag/v0.3.0), with source archive and checksum.
- [Installable plugin](https://github.com/1vecera/omarchy-mluva) and [marketplace submission](https://github.com/omacom/omarchy-plugin-marketplace/issues/5533). A submission is separate from marketplace approval.
- [Feature maturity](feature-maturity.md), [provider behavior](providers-and-live-rewrite.md) and [Omarchy verification](omarchy-integration.md#verification).
- [Source-backed capability matrix](feature-story.md) and [production logo/wordmark assets](brand-and-compatibility.md#production-artwork).

## Launch checklist

- Match every screenshot/video to its manifest's release or source commit. A merged feature is not automatically in an older recording or installed release.
- Check that the small example-text label is readable, and that README/manifest distinguish local synthetic fixtures from actual provider recordings.
- Use the production SVG/PNG lockups with the correct light/dark variant; preserve historical artwork in unchanged source recordings.
- Keep Live rewrite, provider choice and Omarchy integration labeled Experimental. Describe the expanded workspace without implying a dedicated full-screen editor or arbitrary selected-text hover toolbar.
- Confirm that provider copy describes working deployments rather than promising every vendor/account. Automatic paste remains Experimental and disabled by default; provisional text is never delivered.
- Have Daniel review the name and final posts before publishing. The current brand and media changes stay in draft PRs; release/integration belongs to the coordinator.

## Future story

The longer-term vision is an interface that helps Daniel think while he works. Agents may eventually talk back or ask a useful clarifying question. That is future direction. The current product story is structured dictation, editable notes and reliable handoff into the next task.

Next proof should show actual microphone and physical-shortcut behavior on named Omarchy setups, then a declared reusable audio corpus with recognition errors, correction effort and timing. Keep those claims separate from scripted visual checks and short public-audio demonstrations.
