# Small Linux development and capture box

Run Mluva's native GTK app and real Omarchy QML on a private virtual desktop. The box uses Fedora 44, a pinned Omarchy source commit and locked Python dependencies, with **2 CPUs, 3 GB RAM and 256 PIDs**. It exposes no network port and mounts only this checkout. Host desktop, audio devices, clipboard, session bus, credentials and Docker socket are not mounted.

## Start and check

The host needs Docker, Git and Bash. Media export also needs `uv` and FFmpeg with `libx264`. The release assets were built in a local Colima VM; a hosted VM is not required.

```sh
bash dev/box.sh build
bash dev/box.sh up
bash dev/box.sh check
bash dev/box.sh shell
```

The container is named `mluva-dev`; set `MLUVA_DEV_NAME` for another checkout. Operations check its ownership label before using or stopping it. Build and dependency setup need network access. The Fedora base digest and Omarchy source commit are pinned; Fedora packages remain repository-resolved at build time.

```sh
bash dev/box.sh stop
bash dev/box.sh up
bash dev/box.sh remove
```

Stopping releases CPU/RAM use. Removing deletes this container's disposable home and cache; the checkout and captures remain on the host. A container belonging to another checkout is refused. Remove the image separately with `docker image rm mluva-dev:0.1.1` when finished.

## Capture promotion assets

Use a fresh output directory. Capture takes about two minutes after setup and runs dark, portrait and light scenarios sequentially to avoid Xvfb allocation races.

```sh
bash dev/box.sh capture tmp/promotion-new
uv run --python 3.13 dev/export_promotion.py tmp/promotion-new tmp/promotion-export
```

The first command records WebM videos, native PNGs and capture receipts. The exporter runs on the host and produces eight PNGs, three silent H.264 MP4s and a media/hash manifest. It refuses an existing destination. Review the result before copying it into `docs/promotion/assets/`.

`promotion-story.json` owns the synthetic transcript and rewrite. `promotion-stage.qml` composes a branded scene around the real GTK window and production `mluva.dictation` widget. `promotion_capture.py` replaces microphone, focus-tracker, clipboard and provider boundaries while retaining production views, SQLite stores, the bridge, D-Bus actions and rewrite lifecycle. The widget's Structure and Copy actions traverse a separate bridge process. A success receipt requires one completed rewrite, an unchanged original and exactly one deliberate Copy action.

The renderer configures Inter privately and raises the portrait widget above the footer. Text, timing and provider output are scripted. No API key is needed. These are UI demonstrations, not recordings of recognition speed, quality or a live Hyprland session. PNGs are preserved; MP4s use H.264/yuv420p and fast-start metadata. The close-up magnifies a crop of the same desktop recording.

`run-isolated.sh`, based on Daniel's offscreen verification helper, creates fresh XDG directories, Xvfb, private D-Bus and private AT-SPI services for each scenario. It cleans up its exact child processes. Evidence stays under `tmp/` and app data is disposable.

## Export the Omarchy distribution

The installable repository is [1vecera/omarchy-mluva](https://github.com/1vecera/omarchy-mluva). Make runtime changes in `linux/quickshell/mluva.dictation` here, then export a tagged release:

```sh
git fetch origin tag v0.1.1
uv run --python 3.13 dev/export_plugin.py tmp/plugin-export --release v0.1.1 --preview docs/promotion/assets/workflow-dark.png
bash dev/box.sh exec bash /usr/share/omarchy/bin/omarchy-plugin-validate /workspace/tmp/plugin-export
```

The exporter copies the manifest, two QML files, Apache license, README, preview and source/hash record. The destination must be new and beneath this repository's `tmp/`. It does not push or publish automatically.

## Verification boundary

For v0.1.1, 295 Linux tests, Ruff, ShellCheck, private D-Bus shortcut checks, a production GTK render and the Quickshell scrolling/countdown fixtures passed here. Promotion runs also exercised the real review action through a scripted provider, original preservation and deliberate Copy. PNG layouts and encoded video frames were inspected.

This is a Fedora container with Omarchy's production shell components. Physical microphone capture, real F9, live Hyprland focus and target-application acceptance remain separate. Hosted CI was not run; its workflow is on demand.
