# Product film and capture tooling

The completed [55-second launch film](../docs/promotion/README.md) uses the frozen installed Linux runtime on claw-mini. The launch workflow reserves Xvfb **:203**, explicit guest identity, private D-Bus/AT-SPI/XDG state and fresh output directories under this checkout’s `tmp/`. The historical Lenovo **:193** real-provider and **:195** fixture routes remain below.

## Short launch film

The delivered film used runtime `6022b05e4258768e7aa70305febf7dad0835b060`; its exact historical harnesses remain with the capture assets. Current tooling follows the Mluva 1.0 identities. For a new take, commit and install the selected runtime and pass that exact revision to the verifier; do not reuse a historical installation receipt after an upgrade. The Mac route reuses the prepared `mluva-film` ARM64 container on Docker context `colima`, with hostname `claw-mini-capture`, label `dev.mluva.capture-profile=claw-mini-film` and volume `mluva-film-home`. Both repository case spellings are mounted, so resolve the prepared worktree with `pwd -P` and always pass it as the guest working directory. The handoff under `/Users/openclaw/code/Mluva/tmp/mluva-launch-handoff/` owns local configuration and exact asset provenance; it is not a portable full-Omarchy image.

The guest has GTK/libadwaita, Omarchy 4.0.2 QML/theme assets, Quickshell 0.2.1, private X11/PipeWire dependencies, xcompmgr 1.1.10, Adwaita Sans, librsvg and FFmpeg with OpenH264/AAC. Codex 0.154.0 is available as the native ARM64 binary. The native preflight and actual encoding passed at 1920 × 1080, 60 fps requested, pixel ratio 1. No full VM or Quickshell upgrade was needed. Other containers and the host app-server remain running.

For a separately authorized new capture, install the selected committed runtime into the guest’s own home, then probe its launcher from outside the source checkout:

```sh
mluva_worktree="$(pwd -P)"
mluva_revision="$(git rev-parse HEAD)"
git diff --exit-code "$mluva_revision" -- linux

docker --context colima exec --workdir "$mluva_worktree" mluva-film \
  git config --global --add safe.directory "$mluva_worktree"
docker --context colima exec --workdir "$mluva_worktree" mluva-film \
  bash linux/install.sh
docker --context colima exec --workdir "$mluva_worktree" mluva-film \
  uv run --no-project dev/verify_capture_install.py tmp/delight-film/installation-next \
  --runtime-revision "$mluva_revision"
```

`verify_capture_install.py` validates the guest profile, source tree, launcher template, installed interpreter/module, 60 payload files and icon. It records the actual launcher command, private working directory and startup screenshot in `installation.json`. This credential-free startup has the expected missing-credential setup banner. Both capture routes independently recheck the supplied guest receipt, installed files, dependencies and icon before/after recording; documentation and media commits may differ while the Linux source tree stays identical. They execute the installed interpreter directly without installing or syncing dependencies during a take.

The Mac adapter verifies container ownership and supplies an explicit guest path and encoder. Preflight and feature capture receive no credentials. For the real take, the managed Scribe key and existing Codex authentication travel through stdin into a one-use, same-user Linux broker. Only the application/provider process receives the provider input. Docker subprocesses use a clean environment, desktop services start without keys, and child environments remove Scribe credentials. Codex reads an anonymous memory descriptor through its private auth link; its private provider directory is removed after the run. No image, shared container environment, CLI argument or published receipt contains credential values.

Use fresh output names, run the following sequentially from a managed Mac shell with `DAS_ITEM_ELEVEN_LABS_API_KEY__CREDENTIAL` available, and review the real recognition/replies before capturing features:

```sh
uv run --no-project dev/mac_capture.py preflight tmp/delight-film/layout-next \
  --installation-receipt tmp/delight-film/installation-next/installation.json

uv run --no-project dev/mac_capture.py real tmp/delight-film/real-next \
  --installation-receipt tmp/delight-film/installation-next/installation.json

uv run --no-project dev/mac_capture.py features tmp/delight-film/features-next \
  --installation-receipt tmp/delight-film/installation-next/installation.json \
  --source-take tmp/delight-film/real-next

uv run --no-project dev/package_delight.py real \
  tmp/delight-film/real-next tmp/delight-film/real-export-next
uv run --no-project dev/package_delight.py features \
  tmp/delight-film/features-next tmp/delight-film/features-export-next
```

The supplied synthetic WAV enters production Scribe/Codex clients through a private PipeWire graph without hardware devices. The real take retains recorder PCM, provisional recognition, actual replies, provider events and the measured audio/video offset. An explicit 12-second hold after playback leaves recording open for a readable Live shot; it is disclosed editorial timing. The feature take seeds its original and first draft from the exact real result, then uses local deterministic Polish/Rewrite replies. Native assertions cover Copy, Save, private XTest Ctrl+P/search/Enter, theme changes and unchanged raw input. Automatic copy/paste, global shortcuts and live target tracking are disabled.

`package_delight.py` uses the Mac’s FFmpeg with libx264 for continuous H.264 exports, preserving source duration without editorial cuts or speed changes. Its allowlist copies only media, text, receipts and the exact harness; private sessions and provider state stay local. The real export’s `export.json` records PCM alignment. Both packages retain hashes, runtime verification and full-decode results. New takes need semantic review and newly bound offsets; do not reuse final-film offsets against another capture.

The committed plan has `status: ready` and resolves its complete sources relative to `docs/promotion/assets/delight`. Reconstruct the delivered edit without provider access using the guest FFmpeg, which includes the required `drawtext` filter. The current Mac Homebrew FFmpeg lacks that filter; its working libx264 export path does not imply it can compose this film.

```sh
mluva_worktree="$(pwd -P)"
docker --context colima exec --workdir "$mluva_worktree" mluva-film \
  uv run --no-project dev/compose_delight.py \
  docs/promotion/assets/delight/launch.plan.json \
  tmp/delight-film/mluva-delight-launch.mp4 --encoder libopenh264

docker --context colima exec --workdir "$mluva_worktree" mluva-film \
  uv run --no-project dev/verify_delight.py \
  docs/promotion/assets/delight/launch.plan.json \
  tmp/delight-film/mluva-delight-launch.mp4 \
  --images tmp/delight-film/review-frames
```

The composer supports an explicit `--font` when another machine needs the original font file. The delivered font SHA-256 is `8381c33b9a44f066f2b99dba3d416a2342891e28c956a35dfd8d16ee2987e6d4`, matching the prepared opening. Source speech, logo and scenery need no regeneration. Per-shot `fade_in: 0` gives direct cuts; other durations and eased camera movement are explicit in the plan. Receipts bind the sources, font, plan, composer, FFmpeg version, encoder and output.

The adapter’s credential/ownership boundary can be checked without credentials or provider calls: run `uv run --project linux --locked pytest dev/test_mac_capture.py --basetemp tmp/delight-film/adapter-tests -q` inside the guest. It covers clean inspection/exec environments, stdin-only real provider input, credential-free feature routes and rejection of the wrong home volume. Keep the worktree’s Linux virtual environment in the guest when running locked project tools.

The verifier checks complete decoding, exactly 3,300 frames at 1080p60, 55-second duration, audio loudness/headroom and whole-word narration cuts. It writes optional SubRip captions, a poster and representative frames. Inspect those frames and every transition before delivery; the [final visual review](../docs/promotion/assets/delight/qa/review.json) records the delivered film’s ten cuts and two internal theme changes. Automatic media checks do not establish auditory listening or provider accuracy/latency. The [media guide](../docs/promotion/README.md) and [release verification](../docs/verification/delight-launch.md) preserve the installed-source and environment boundaries.

## Historical Lenovo workflow

The commands and measured results below reproduce the [earlier 83.6-second film](../docs/promotion/archive-product-film.md). Their Lenovo dependency paths and old source offsets do not describe the claw-mini launch capture.

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
