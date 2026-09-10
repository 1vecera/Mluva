# Product film and capture tooling

The [55-second launch plan](../docs/promotion/README.md) is prepared; installed product capture and the final film are pending. Its new workflow reserves Xvfb **:203**. The capture controllers are restricted to this Lenovo and fresh output directories below this checkout's `tmp/`. They execute the installed interpreter directly with isolated app state; they never install, sync its environment or drive the live desktop. The earlier **:193** real-provider and **:195** feature workflows remain documented below for their historical assets.

## Short launch film

Complete runtime verification, commit the Linux source and install that exact build before capturing. `tmp/delight-launch/live-installation.json` must identify `source_revision`, `installed_runtime`, `installed_at_utc`, `entrypoint`, `installed_files`, `plugin_files` and `verified: true`. Both capture routes verify the installed Python, QML, dependency locks, helper files and icon against the clean source tree before and after capture. The receipt’s runtime directory must match, and its source revision must contain the same Linux tree as the current checkout; later documentation/media commits are allowed. Select that exact receipt revision for the real take. A staged-install receipt does not authorize final footage.

Use the system dependencies and private desktop helper described below, with **:203** free. Supply an existing xcompmgr binary; no compositor installation is required. Optional `--pixel-ratio 2` renders the native 1920 × 1080 logical desktop at 3840 × 2160 for sharper camera crops. Verify its layout and recording performance with a credential-free native take before requesting the real providers.

```sh
mluva_runtime="$(uv run --no-project python -c 'import json; print(json.load(open("tmp/delight-launch/live-installation.json"))["installed_runtime"])')"
mluva_revision="$(uv run --no-project python -c 'import json; print(json.load(open("tmp/delight-launch/live-installation.json"))["source_revision"])')"
mluva_compositor=/absolute/path/to/xcompmgr

uv run --no-project dev/capture_delight.py tmp/delight-film/layout-take \
  --runtime "$mluva_runtime" \
  --installation-receipt tmp/delight-launch/live-installation.json \
  --compositor "$mluva_compositor" --pixel-ratio 2

~/.config/daniel-ai-skills/bin/das-agent-snapshot launch \
  --only DAS_ITEM_ELEVEN_LABS_API_KEY__CREDENTIAL -- \
  uv run --no-project dev/capture_campaign.py task tmp/delight-film/real-take \
  --runtime . --runtime-revision "$mluva_revision" \
  --installed-payload "$mluva_runtime" \
  --installation-receipt tmp/delight-launch/live-installation.json \
  --compositor "$mluva_compositor" --launch-film --pixel-ratio 2 \
  --audio docs/promotion/assets/delight/source/dictation-input.wav \
  --rewrite-provider codex

uv run --no-project dev/capture_delight.py tmp/delight-film/feature-take \
  --runtime "$mluva_runtime" \
  --installation-receipt tmp/delight-launch/live-installation.json \
  --compositor "$mluva_compositor" --pixel-ratio 2 \
  --source-take tmp/delight-film/real-take

uv run --no-project dev/package_delight.py real \
  tmp/delight-film/real-take tmp/delight-film/real-export
uv run --no-project dev/package_delight.py features \
  tmp/delight-film/feature-take tmp/delight-film/feature-export
```

The real take feeds the retained synthetic WAV through a private PipeWire graph into production Scribe and Codex clients. It retains the complete recorder audio, provisional recognition, actual replies, provider events and measured audio/video offset. The later feature take seeds its original and first draft from those exact results, then uses local deterministic Polish and custom Rewrite replies. It asserts native Copy, Save edits, private XTest Ctrl+P/search/Enter, theme changes and unchanged raw input. Local timing and fixture replies establish no provider quality or latency result.

`package_delight.py` publishes only explicit media, text, receipts and exact harness files; private desktop directories and provider state stay outside the package. Its continuous exports preserve elapsed source time without editorial cuts or speed changes. The real export’s `export.json` documents PCM alignment; both packages include hashes, before/after runtime identity and full-decode results.

The final edit needs no provider access once its native footage exists. Its portable plan resolves sources relative to `docs/promotion/assets/delight`, will bind every native source offset, camera move and speed factor, and places individually timed narration phrases over the original synth bed. `compose_delight.py` renders the vector logo and text in software, checks the exact 3,300-frame 1080p60 4:2:0 result, and records source, font, plan, composer and output hashes. The committed plan currently has `status: awaiting-installed-capture`; it supports only `--preview`. Source speech and scenery already exist and need no regeneration. Populate the native clips from the completed captures and set `status: ready` before running the full edit and verifier below.

```sh
uv run --no-project dev/compose_delight.py \
  docs/promotion/assets/delight/launch.plan.json \
  tmp/delight-film/mluva-delight-launch.mp4

uv run --no-project dev/verify_delight.py \
  docs/promotion/assets/delight/launch.plan.json \
  tmp/delight-film/mluva-delight-launch.mp4 \
  --images tmp/delight-film/review-frames
```

The verifier checks source/plan/output hashes, the encoded media format and complete decoding, narration loudness/headroom and word-aligned phrase cuts. It exports optional SubRip captions with canonical spelling, a poster, an opening frame and a labeled contact sheet. Inspect those frames and the action/source evidence before delivering; automatic media checks are not an auditory listening review.

The installed app and continuous evidence determine the claims. Read the [current media guide](../docs/promotion/README.md) and [release verification](../docs/verification/delight-launch.md) for source boundaries, native checks and final review. The older commands and measured results below reproduce the [historical 83.6-second film](../docs/promotion/archive-product-film.md).

## Prepare

Use a fresh worktree from the remote default branch and retain the v0.3.0 tag. Run `make linux-setup`. The host needs GTK 4, libadwaita, Quickshell, Omarchy shell modules, Xvfb, D-Bus/AT-SPI, PipeWire CLI tools, FFmpeg, ImageMagick, Fontconfig and `uv`. The capture uses Daniel's installed `run-offscreen-linux-verification` helper at `~/.agents/skills/run-offscreen-linux-verification-daniel/scripts/run_isolated_x11.sh`.

The tested compositor binary is xcompmgr 1.1.10, extracted from the official Arch package into `tmp/campaign/tools/usr/bin/xcompmgr`; the helper expects that path. Do not install a compositor or change the live desktop for this capture. Leave :193 free and run capture/UI checks sequentially. The helper copies the actual active Nord theme; no live config is edited.

Managed provider credentials reach only the application through a one-use same-user broker after private desktop services have started. Child environments remove those keys. Native Codex gets a private configuration directory, existing authenticated access and no inherited MCP/plugin setup; that temporary provider directory is removed after the run. No key is passed in a CLI argument or written to a capture receipt. Do not launch the private desktop directly beneath a secret-loaded environment.

## Capture and export

The repository includes the exact final audio inputs, so regeneration or a fresh speech-generation call is unnecessary. The Fish source, speech provenance and transforms are documented with the assets.

```sh
make linux-setup
uv run --no-project dev/capture_campaign.py layout tmp/campaign/layout

~/.config/daniel-ai-skills/bin/das-agent-launch --only DAS_ITEM_ELEVEN_LABS_API_KEY__CREDENTIAL \
  uv run --project linux --locked python dev/capture_campaign.py speech tmp/campaign/speech \
  --audio docs/promotion/assets/sources/jfk-iconic-input.wav

uv run --no-project dev/export_campaign.py tmp/campaign/speech tmp/campaign/speech-export

uv run --no-project dev/capture_campaign.py saved-speech tmp/campaign/portrait \
  --saved-run tmp/campaign/speech --portrait
uv run --no-project dev/capture_campaign.py layout tmp/campaign/portrait-stage --portrait
uv run --no-project dev/export_campaign.py tmp/campaign/speech tmp/campaign/speech-export \
  --vertical-background tmp/campaign/portrait-stage/stage.png
```

The landscape MP4 keeps the complete screen recording and measured PCM offset. The portrait MP4 is a disclosed crop of the same recording widget over the captured editorial stage; its AAC stream is copied. `fc-match` resolves the caption font instead of assuming Inter is installed. The portrait PNG reopens the real saved history in the native UI without another provider call.

`compose_campaign_intro.py` reproduces the 83.6-second product film from its version 2 portable plan. Source paths resolve relative to the plan, and the replaceable brand JSON resolves its own vector input. FFmpeg renders all typography, the opening identity/app motion, JFK source card, original-speed cuts and 0.2-second edge fades without overlaps. Fontconfig resolves the requested caption font, while `rsvg-convert` renders the outlined SVG lockup. No inference is needed:

```sh
uv run --no-project dev/compose_campaign_intro.py \
  docs/promotion/assets/mluva-product-intro.plan.json tmp/film/mluva-product-intro.mp4

uv run --no-project dev/verify_product_film.py \
  docs/promotion/assets/mluva-product-intro.plan.json tmp/film/mluva-product-intro.mp4 \
  --images tmp/film/assets
```

For the separately authorized unreleased Live workflow, supply the exact tested runtime checkout and commit. Runtime files must match the revision; the explicit `--experimental-dirty` option instead preserves a frozen, authorized uncommitted patch. Neither route is labeled v0.3.0. The application and production QML are imported/copied from that runtime, while capture automation remains in this worktree.

```sh
uv run --no-project dev/capture_campaign.py catalog tmp/campaign/catalog --rewrite-provider codex

~/.config/daniel-ai-skills/bin/das-agent-launch --only DAS_ITEM_ELEVEN_LABS_API_KEY__CREDENTIAL \
  uv run --project linux --locked python dev/capture_campaign.py task tmp/campaign/live \
  --runtime /absolute/path/to/tested-runtime \
  --runtime-revision cdc00237595f8cfb29e75cd87a980311ea3bcc98 \
  --audio docs/promotion/assets/sources/task-input.wav \
  --rewrite-provider codex --rewrite-model gpt-5.6-luna --live-interval 4 --min-characters 160

uv run --no-project dev/export_campaign.py tmp/campaign/live tmp/campaign/live-export
```

Catalog discovery starts no generation turn. Use a model actually advertised by the installed server; see the [official app-server protocol](https://developers.openai.com/codex/app-server). Fast mode remains off. Provider observations preserve actual setup, turn dispatch, completion, GTK after-paint, speech commits and Stop separately. Exact initial-template echoes are excluded from the helper's meaningful-draft gate; human review further distinguishes an incomplete Intent from substantial Task/Intent/Requirements and active voiced audio from trailing silence.

Receipts include immutable recognition/replies, retained recorder WAV, frame/video captures and provider events. Recent captures also retain the exact capture harness, runtime patch, file hashes and post-run verification. Raw values are never replaced by model output. Editorial history titles are separate from recognition. Automatic clipboard delivery, paste, titles, spoken commands and live focus tracking are disabled for this demonstration; the real recording and provider transports remain intact.

## Offline feature walkthroughs

The six `capture_features.py` scenarios use reserved Xvfb **:195**, separate D-Bus/AT-SPI/XDG state and a private clipboard. Audio, hardware devices, providers, portals, global shortcuts and target tracking are disabled. It accepts no credentials. `feature_session.py` imports production GTK views, callbacks and stores and the production Quickshell widget, with local example text and a scripted streaming rewrite. The current Linux runtime must be committed and unchanged; before/after file hashes and the exact harness are retained. Capture requires the same isolated X11 helper and compositor described above.

```sh
uv run --no-project dev/capture_features.py tmp/film/features \
  --compositor tmp/campaign/tools/usr/bin/xcompmgr

uv run --no-project dev/export_features.py tmp/film/features tmp/film/features-export
```

Use `--scenario polish`, `floating`, `expanded`, `history`, `export` or `providers` for a bounded rerun. The exporter copies the full H.264 stream into silent MP4 without cuts or retiming, decodes it, and packages screenshots, immutable example input/replies, actual Markdown/JSON exports, runtime hashes, exact harness and an export receipt. The selected captures in `docs/promotion/assets/polished/features` predate comment-only helper edits and preserve the exact harness that produced them. Before screenshots use the setup clock; action and after-screenshot timestamps use a separate clock reset at FFmpeg launch. Export receipts make this distinction explicit; no synthetic status or timing establishes provider latency.

The composition can replace identity with `--brand path/to/brand.json`; fields are `name`, `wordmark`, `font_family`, `hook`, `link`, `provenance` and `derivation`. Keep referenced paths relative to their plan or brand JSON. The edit receipt fingerprints the plan, composer, actual font, brand and input files. `verify_product_film.py` uses its inline NumPy dependency with `uv run --no-project`; it checks all source/film hashes, the 2508-frame 1080p/30 fps output, complete decoding, fixture silence, and zero-shift waveform correlation and lag against both real continuous sources. It exports a poster at film 3.4 s and eight feature screenshots with hashes/timestamps. Visual review and source-build defects are documented separately in the promotion guide.

## Validation and older tooling

For S27-465, `make linux-test` passed **358 tests**, the feature-maturity check and Ruff on runtime `18004cc98cd73f1b038883e59db6274ee9f0be4b`. `OFFSCREEN_DISPLAY_NUMBER=195 make linux-text-target-test` passed private AT-SPI focus, Unicode insertion and exact-target assertions, with one retained AT-SPI cache warning; the warning is a known private-harness limitation and is unrelated to this media change. A follow-up under a different installed wrapper did not pass its AT-SPI-enabled precondition and is retained separately; it is not accepted as validation. The coordinator independently reports the combined tree passing 361 tests, Live workspace, native focus/Unicode/exact-target assertions and staged install, with the same known cache warning. No further runtime testing or harness changes are part of this handoff. These checks do not establish live Hyprland, physical F9 or microphone behavior. The six feature captures pass their action/raw-input assertions. Changed dev helpers pass Ruff check and format with `--config linux/pyproject.toml`. Final film decode, source hashes, silent sections, audio alignment, poster/screenshots and opening/transition visual review are retained in `docs/promotion/evidence/polished` and the adjacent media QA. Swift/macOS build and smoke checks are unavailable on this Linux host; no hosted CI spend is authorized.

The [legacy Fedora container guide](legacy-container.md) and `promotion_capture.py`/`export_promotion.py` preserve the v0.1.1 scripted-fixture workflow. They are not the capture path for this campaign. `plugin-README.md` remains the distribution README template; its review timer and independent speech/rewrite provider descriptions reflect v0.3.0. Distribution publication is owned by the coordinator.
