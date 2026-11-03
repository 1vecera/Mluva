# Launch film, November 2026

`MluvaFilm` (`src/film/MluvaFilm.tsx`) is the launch film cut to Daniel's iPhone take: 69.3 seconds, 1920×1080 at 60 fps. Every frame of Mluva, Codex, Claude Code and Omarchy is the running software, filmed in a VM. Each key press was fired on the film time of the word that names it, so the shots follow his delivery instead of being laid under it. Media stays out of Git, under `~/Videos/Mluva/launch-2026-recording/`.

## How it is made

| Step | Script | What it does |
| --- | --- | --- |
| Cut the take | `build_edl.py` | Picks 18 matched picture/audio cuts on 30 fps frames inside silence, at the frame pair that looks closest, and maps every Scribe word onto film time (`edl.json`). Cuts remove "If--", both "uh"s and the "and, uh" before "I built it" |
| Voice | `voice.py` | DeepFilterNet3 locally (18 dB limit), then EQ for a lavalier on a shirt, de-essing and 2.6:1 compression, to −18 LUFS. No pitch or rate change |
| Presenter | `presenter.py` | RVM MobileNetV3 matte over the whole take, cut with the voice's boundaries; a 4–6 frame dissolve after each cut blends the skipped pause frames into the joined ones, streaming frames |
| Takes | `make_takes.py`, `director.py` | Writes each take from the word map and plays it in the VM: QMP key and tablet events at word times while `wf-recorder` records the guest losslessly at 2560×1440 |
| Anchors | `measure.py`, `edit_plan.py` | Finds the first frame where each action shows, pairs it with its word, and writes captions, key tokens, camera and presenter moves |
| Desktop | `assemble.py` | Plays each take at real speed between anchors, holding or trimming rest frames to meet the next one; crossfades between takes |
| Sound | `music.py`, `mix.py` | A synthesized bed (no third-party music), 21.5 dB under the voice over speech; master at −16 LUFS |
| Picture and master | `npm run film:render`, `master.sh` | Silent Remotion render, then the mix attached without re-encoding the picture |
| Checks | `verify.py` | Full decode, black frames, loudness, voice alignment and a fresh Scribe pass over the encoded film |

Run each heavy step alone and capped, for example `systemd-run --user --scope -p MemoryMax=12G -p MemorySwapMax=0 -- npm run film:render`.

## The VM

`guest/vm.sh` boots the screen-annotation harness's Arch image (`docs/verification/screen-annotations/hyprland-vm/`) with 14 vCPUs and 20 GB, headless on `egl-headless`, with user networking restricted to the forwarded SSH port during takes. The guest runs Omarchy 4.0.2 and the Arch packages of 22 September 2026, matching the workstation (Qt 6.11.2, Quickshell 0.3.1, GTK 4.22). `guest/` holds the guest side: the desktop, the synthetic set (`set.sh`, `fixtures/`), loopback models for the Codex and Claude Code sessions (`models.py`) and `scribe_peer.py`.

The app is the unmodified `origin/main` build. Its ElevenLabs route reaches `scribe_peer.py` because the guest maps `api.elevenlabs.io` to 127.0.0.1 and trusts a CA made for the VM. The peer plays Daniel's own words as live partials at his word times and commits the final text after a delay chosen so delivery lands on his word. The recorder reads a synthetic PCM fixture. Nothing leaves the guest.

The model-choice shot runs a capture-only build of the in-progress provider work (Microsoft MAI, xAI Grok, Mistral) so the real Settings list shows them; it is not part of this branch, and no shot shows them transcribing.

## Limits

Under motion the guest compositor draws about 30–45 distinct frames a second, so busy motion repeats frames inside the 60 fps take. Retiming holds or trims only frames at rest; it never changes playback speed. Provider replies and agent answers are scripted loopback responses; the delivered texts and annotation bundles are the app's own output.
