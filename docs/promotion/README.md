# Mluva launch film

[Watch the 55-second film](assets/delight/mluva-delight-launch.mp4) · [Captions](assets/delight/mluva-delight-launch.srt) · [Edit plan](assets/delight/launch.plan.json) · [Media QA](assets/delight/mluva-delight-launch.qa.json) · [Visual review](assets/delight/qa/review.json) · [Capture instructions](../../dev/README.md#short-launch-film)

[![Mluva: the most delightful dictation app for Omarchy](assets/delight/opening-preview.png)](assets/delight/mluva-delight-launch.mp4)

The completed film is **55 seconds, 1920 × 1080, 60 fps**, with stereo AAC audio and optional word-aligned captions. It uses the selected Mluva lockup, original animated mountain/eclipse scenery and **“The most delightful dictation app for Omarchy.”** The desktop opens in Daniel’s exact Nord palette and black-moon background.

| Film seconds | Beat | Source |
| --- | --- | --- |
| 0–5.5 | Mluva and the positioning | Original scenery, software-rendered logo/type |
| 5.5–10.5 | Record outside the app | Installed app, real Scribe/Codex take |
| 10.5–15 | Open the workspace while speaking | Same real take |
| 15–24 | Useful Live draft with steady panes | Same real take, recording still active |
| 24–27 | Copy the finished note | Native feature take seeded from the real result |
| 27–32 | Polish and custom rewrite | Native controls, disclosed local replies |
| 32–37 | Edit and save | Native editor and persistence |
| 37–42 | Ctrl+P, search and Copy | Private XTest keys and native command palette |
| 42–50 | Nord, Tokyo Night and Rosé Pine | Production theme watcher and theme assets |
| 50–55 | Return to Mluva | Original scenery, software-rendered logo/type |

The [plan](assets/delight/launch.plan.json) binds every source offset, speed factor, camera move, reveal and narration cut. The [edit receipt](assets/delight/mluva-delight-launch.edit.json) binds the plan, inputs, font, composer and output by SHA-256. Most action boundaries use direct cuts; the opening and closing use short reveals.

## Installed capture and source boundaries

The actual Linux payload was installed in the owned Fedora 44 ARM64 guest on claw-mini, frozen at merged main `6022b05e4258768e7aa70305febf7dad0835b060`. Its [guest installation receipt](assets/delight/capture/environment/installation.json) verifies the installed launcher from outside the source checkout, 60 payload files and the icon. The exact installed files were checked before and after both final takes. No product runtime was changed for this film.

The [continuous real take](assets/delight/capture/real/workflow.mp4) uses production Scribe v2 realtime and Codex 0.154.0 with catalog-selected `gpt-6-astra`. The [54-word synthetic input](assets/delight/source/dictation-script.txt), [literal recognition](assets/delight/capture/real/recognition.json), [actual replies](assets/delight/capture/real/replies.json), [retained PCM](assets/delight/capture/real/captured.wav) and [provider events](assets/delight/capture/real/capture.json) support the [semantic/timing review](assets/delight/capture/environment/source-review.json). The source word sequence and intended responsibilities survive recognition. The final reply retains preview-before-save, missing IDs, original-file totals, source rows for audit, Maya’s Thursday preparation and the speaker’s Friday review; it explicitly marks unspecified owners and calendar dates.

The early incomplete provisional draft is excluded from the useful Live shot. That shot uses source seconds 23.3–32.3 while recording remains active, after the voiced input has ended. The input includes trailing silence, and the harness then holds recording open for another 12 seconds for reading. Source gaps and later speed changes are editorial choices, not latency measurements.

The [continuous feature take](assets/delight/capture/features/workflow.mp4) starts with that real take’s exact recognized original and final reply, bound by its [seed receipt](assets/delight/capture/features/seed-take.json). Later Polish and custom Rewrite replies are deterministic local examples. The actual GTK callbacks, clipboard, editor, storage, in-app Ctrl+P and theme watcher run normally. [Native assertions](assets/delight/capture/features/capture.json) verify every action and the byte-for-byte unchanged original. Fixture reply timings establish no provider performance result.

The [theme provenance](assets/delight/capture/environment/theme-provenance.json) and recorded session hashes identify Daniel’s Nord `colors.toml`, generated `shell.toml` and black-moon wallpaper. This is actual Mluva with production Omarchy 4.0.2 QML/theme assets inside a themed Linux capture environment. Quickshell 0.2.1 passed the native preflight. The desktop bar is editorial framing; this does not establish bare-metal Omarchy, real Hyprland portals, physical global-keyboard integration or hardware-microphone quality. Omarchy bundling and a future dedicated shortcut remain aspirations.

Xvfb :203, D-Bus, AT-SPI, XDG state, clipboard and PipeWire are private to the take. The supplied WAV enters a private graph without hardware devices. Provider credentials pass through a one-use broker into the provider process; capture services were audited without those keys. Codex authentication is supplied through stdin and anonymous memory, with its private directory removed afterward. [Real](assets/delight/capture/real/package.json) and [feature](assets/delight/capture/features/package.json) package manifests retain explicit media, text, receipts and exact harnesses; private session directories, databases and auth material are excluded.

## Final review and limits

The [media verifier](assets/delight/mluva-delight-launch.qa.json) confirms complete decoding, exactly 3,300 frames, 55-second duration, H.264 4:2:0 at 1080p60, and 48 kHz stereo AAC. The mix measures **−16.12 LUFS**, **−4.91 dBTP**, with 6.7 LU loudness range. All 11 narration cuts preserve whole words and canonical subtitle spelling.

The [contact sheet](assets/delight/qa/contact-sheet.jpg), all ten editorial boundaries and both internal theme changes were visually inspected. The [review receipt](assets/delight/qa/review.json) links three transition sheets and records close-frame checks for Live, Polish, Rewrite, editing, commands and light/dark themes. The short captions leave the content legible; fixture timing labels stay outside the close crops. No secret/private session payload was found in the publication scan. Auditory listening was not supported by this runner; loudness, decoding and word alignment are **not** a pronunciation or listening review.

[Repository checks](assets/delight/qa/checks.json) record 381 passing Linux tests, four passing Mac adapter boundary tests, feature-maturity drift validation, lint/format checks and private shortcut assertions. The macOS suite has one unrelated frozen-baseline failure: a protected-vocabulary assertion expects `[Voi]` while the request contains `[Mlu]` (291 of 292 pass). The optional GNOME overlay smoke is unavailable in this capture guest. The separate native text-target assertions pass with an AT-SPI cache warning, so they are not pristine GNOME acceptance. Hosted CI is on demand and was not purchased or triggered.

## Retained design and audio provenance

The [audio guide](assets/delight/source/README.md) retains synthetic Fish narration, two targeted pronunciation replacements, dictation, scripts, word alignment and generation receipts. The final edit mutes captured source audio beneath narration. Its quiet stereo pulse is an original oscillator composition with a retained [recipe](assets/delight/source/sound-bed.recipe.json), without sampled music.

The [outlined lockup](assets/polished/brand/mluva-lockup.svg) and [wordmark provenance](assets/polished/brand/wordmark-provenance.json) preserve the selected identity. The [animated H3 scenery](assets/polished/motion/h3-max-clean-background.mp4) retains its original [generation provenance](assets/polished/motion/provenance.json); its generated audio is muted. Wallpapers and generated assets are not claimed as public domain or Apache-2.0. The positioning implies no comparative accuracy result.

The [reviewed 5.5-second opening](assets/delight/opening-preview.mp4), [preproduction QA](assets/delight/preproduction-qa.json), [historical 83.6-second film](archive-product-film.md) and [older Fedora fixtures](assets/legacy-v0.1.1/manifest.json) remain separately available.
