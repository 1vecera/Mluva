# Mluva launch film — capture pending

[Watch the opening preview](assets/delight/opening-preview.mp4) · [Reviewed opening frame](assets/delight/opening-preview.png) · [55-second storyboard and voice map](assets/delight/launch.plan.json) · [Narration](assets/delight/source/narration.txt) · [Audio sources](assets/delight/source/README.md) · [Preparation QA](assets/delight/preproduction-qa.json) · [Capture instructions](../../dev/README.md#short-launch-film)

![Reviewed Mluva opening with the new positioning](assets/delight/opening-preview.png)

**The final product film has not been captured or rendered.** The completed media is a reviewed **5.5-second silent opening**, prepared narration and a **55-second storyboard**. Native footage must follow a verified update of the existing per-user installation. Staged installation and earlier-build recordings do not satisfy that requirement.

The opening uses the selected Mluva lockup and **“The most delightful dictation app for Omarchy.”** The original clean animated scenery returns behind crisp software type. Its [preview receipt](assets/delight/opening-preview.edit.json) identifies the source artwork, font and rendered file. It contains no app footage.

## Planned 55-second film

| Film seconds | Intended beat | Capture state |
| --- | --- | --- |
| 0–5.5 | Mluva and the new positioning | Opening preview ready |
| 5.5–10.5 | Record outside the app | Installed real-provider capture pending |
| 10.5–15 | Open the workspace while speaking | Same real take, pending |
| 15–24 | Read both sides of a useful Live draft | Same real take, pending |
| 24–27 | Copy the finished note | Native control capture pending |
| 27–32 | Polish, then give a custom rewrite instruction | Native control capture pending |
| 32–37 | Edit and save | Native control capture pending |
| 37–42 | Press Ctrl+P, search and choose Copy | Native keyboard capture pending |
| 42–50 | Change the desktop theme | Native theme capture pending |
| 50–55 | Return to Mluva | Closing design reviewed; final assembly pending |

The [plan](assets/delight/launch.plan.json) has the full storyboard and exact narration cuts. Its native source offsets remain unbound until the installed app has been recorded. `status: awaiting-installed-capture` prevents the compositor from treating this partial source map as the final film; only `--preview` is currently available for the committed plan. The target deliverable is 55 seconds, 1920 × 1080, 60 fps.

## Prepared sources and capture boundary

The [54-word dictation source](assets/delight/source/dictation-script.txt) asks for a Friday shop-demo checklist: preview uploads before saving, flag missing order IDs, reconcile totals, retain source rows for audit, and keep Maya’s Thursday preparation and the speaker’s Friday review. The generated [input WAV](assets/delight/source/dictation-input.wav) is ready for the actual app’s private PipeWire input, Scribe recognition and native Codex rewriting. No newly installed recognition or rewrite result is claimed yet.

The planned feature take will begin with that real take’s exact original and final draft. Later Polish and custom Rewrite replies will be declared local examples while the native controls, storage, editing, clipboard, in-app Ctrl+P and palette changes run normally. Those fixtures establish interface behavior, not provider quality or speed. Both continuous sources and the exact source map will accompany the final edit.

The capture tools reserve Xvfb **:203**, separate D-Bus/AT-SPI/XDG state and a private clipboard. The real take uses a private PipeWire graph without hardware devices; the feature take disables audio and authenticated providers. Automatic copy/paste and global shortcuts are disabled. The desktop combines stock Omarchy wallpapers, an editorial bar and the production Mluva widget. This boundary does not establish physical-microphone, global F9 or live Hyprland behavior. In-app Ctrl+P will use private XTest key events.

## Voice, design and remaining review

The narration and dictation are synthetic Fish Audio speech, with scripts, source hashes and generation receipts retained under [source](assets/delight/source). Two targeted pronunciation phrases replace their original narration counterparts. Word alignment preserves whole words and canonical subtitle spelling. The quiet stereo pulse is an original oscillator composition with a retained [recipe](assets/delight/source/sound-bed.recipe.json); it uses no sampled music.

The [outlined lockup](assets/polished/brand/mluva-lockup.svg) and its [wordmark provenance](assets/polished/brand/wordmark-provenance.json) preserve the selected identity. The [existing H3 scenery](assets/polished/motion/h3-max-clean-background.mp4) retains its original [generation provenance](assets/polished/motion/provenance.json). Its generated audio is muted, and all text is rendered in software. The wallpapers and generated assets are not claimed as public domain or Apache-2.0.

The renderer has passed a separate 55-second technical smoke using scenery in place of product footage: exact 3,300-frame 1080p60 H.264 4:2:0, stereo AAC, complete decoding and a measured −16.12 LUFS narration mix. That smoke is not a product film. Final source capture, content review, edit assembly, visual review and media QA remain outstanding. The [current product direction](../delight-launch-direction.md) supplies the brief; the positioning implies no comparative accuracy result or Omarchy bundling commitment.

## Earlier films

The [83.6-second S27-465 product film and guide](archive-product-film.md) remain available with their original 30 fps edit, provider recordings, local feature fixtures and historical limitations. Older Fedora fixtures remain under [legacy-v0.1.1](assets/legacy-v0.1.1/manifest.json).
