# Real audio capture on Lenovo

`capture_campaign.py` runs real Mluva audio, recognition and rewrite workflows on reserved Xvfb **:193**. It is intentionally restricted to this Lenovo and fresh output directories below this checkout's `tmp/`. It never installs into or drives the live desktop. [Assets and claim boundaries](../docs/promotion/README.md) distinguish tagged v0.3.0 speech footage, the unreleased Live workflow, generated transitions and legacy fixtures.

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

The coordinator's `compose_campaign_intro.py` reproduces the reviewed introduction from the portable plan. Source paths resolve relative to that plan, with original-speed cuts and disclosed crossfades. Reproduction needs no inference:

```sh
uv run --no-project dev/compose_campaign_intro.py \
  docs/promotion/assets/mluva-product-intro.plan.json tmp/campaign/intro/mluva-product-intro.mp4
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

## Validation and older tooling

`make linux-test` passed 318 tests plus the feature-maturity check and Ruff on the tagged base. `make linux-text-target-test` passed private AT-SPI focus, Unicode insertion and exact-target checks, with its Xvfb launcher restricted to :193. These checks do not establish live Hyprland or physical-microphone behavior. The new capture/export helpers are also linted and their produced PNG/MP4/WAV files are decoded and visually/audio checked. Swift/macOS build and smoke checks are unavailable on this Linux host; no hosted CI spend is authorized.

The [legacy Fedora container guide](legacy-container.md) and `promotion_capture.py`/`export_promotion.py` preserve the v0.1.1 scripted-fixture workflow. They are not the capture path for this campaign. `plugin-README.md` remains the distribution README template; its review timer and independent speech/rewrite provider descriptions reflect v0.3.0. Distribution publication is owned by the coordinator.
