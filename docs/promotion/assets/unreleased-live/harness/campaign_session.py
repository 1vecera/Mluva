"""Capture released GTK/QML and actual audio/provider lifecycles on reserved Xvfb :193."""

import base64
import ctypes
import hashlib
import json
import os
import shutil
import socket
import sqlite3
import subprocess
import time
import traceback
from dataclasses import asdict, replace
from pathlib import Path
from unittest.mock import patch

import gi
from voice_scribe_linux import realtime
from voice_scribe_linux.app import MluvaApplication
from voice_scribe_linux.audio import PipeWireRecorder
from voice_scribe_linux.codex_client import CodexAppServerClient, CodexAppServerError
from voice_scribe_linux.config import AudioRetentionPolicy
from voice_scribe_linux.live_rewrite import initial_draft
from voice_scribe_linux.providers import LiteLLMClient, ProviderError
from voice_scribe_linux.realtime import RealtimeTranscriptionSession

gi.require_version("GdkX11", "4.0")
from gi.repository import GLib


def settle(predicate, timeout=10):
    """Wait for observed production state while dispatching GTK's real callbacks."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        while GLib.MainContext.default().pending():
            GLib.MainContext.default().iteration(False)
        if predicate():
            return
        time.sleep(0.01)
    raise TimeoutError("Production state did not settle")


def main():
    """Run the real recorder, recognition and draft lifecycle; automate only private controls."""
    root = Path(__file__).resolve().parents[1]
    runtime = Path(os.environ["CAMPAIGN_RUNTIME_ROOT"])
    output = Path(os.environ["OFFSCREEN_ARTIFACT_DIR"])
    session = Path(os.environ["OFFSCREEN_SESSION_ROOT"])
    assert os.environ["DISPLAY"] == ":193" and os.environ["GDK_BACKEND"] == "x11"
    assert not os.environ.get("WAYLAND_DISPLAY") and not os.environ.get(
        "HYPRLAND_INSTANCE_SIGNATURE"
    )
    for name in ("CONFIG", "DATA", "STATE", "CACHE", "RUNTIME"):
        Path(
            os.environ[f"XDG_{name}_HOME" if name != "RUNTIME" else "XDG_RUNTIME_DIR"]
        ).resolve().relative_to(session)
    assert not any(
        "TOKEN" in key or "API_KEY" in key or key.startswith("DAS_")
        for key in os.environ
    )
    os.environ.update(
        PIPEWIRE_REMOTE="pipewire-campaign",
        PIPEWIRE_RUNTIME_DIR=os.environ["XDG_RUNTIME_DIR"],
        PULSE_SERVER="unix:" + os.environ["XDG_RUNTIME_DIR"] + "/disabled-pulse",
        DBUS_SYSTEM_BUS_ADDRESS="unix:path="
        + os.environ["XDG_RUNTIME_DIR"]
        + "/disabled-system-bus",
    )
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
        connection.connect("\0" + os.environ.pop("CAMPAIGN_BROKER"))
        with connection.makefile("rb") as stream:
            credentials = json.load(stream)
    os.environ.update(credentials)
    original_popen = subprocess.Popen
    codex_home = session / "codex-provider"
    if os.environ["CAMPAIGN_REWRITE_PROVIDER"] == "codex":
        codex_home.mkdir(mode=0o700)
        (codex_home / "auth.json").symlink_to(Path.home() / ".codex/auth.json")
        (codex_home / "config.toml").write_text(
            'approval_policy = "never"\nsandbox_mode = "read-only"\n[features]\napps = false\nplugins = false\n'
        )

    class PrivateChild(original_popen):
        """Prevent provider credentials from reaching any helper or desktop service."""

        def __init__(self, *args, **kwargs):
            environment = dict(kwargs.pop("env", os.environ))
            for key in credentials:
                environment.pop(key, None)
            if args and args[0] and args[0][0] == "codex":
                environment["CODEX_HOME"] = str(codex_home)
            super().__init__(*args, env=environment, **kwargs)

    subprocess.Popen = PrivateChild
    portrait = os.environ["CAMPAIGN_PORTRAIT"] == "1"
    scenario = os.environ["CAMPAIGN_SCENARIO"]
    saved_run = (
        Path(os.environ["CAMPAIGN_SAVED_RUN"])
        if os.environ.get("CAMPAIGN_SAVED_RUN")
        else None
    )
    if scenario.startswith("saved"):
        assert saved_run
        saved_run.resolve().relative_to(root / "tmp")
        databases = list(saved_run.glob("session.*/data/voice-scribe/history.sqlite3"))
        assert len(databases) == 1
        destination = Path(os.environ["XDG_DATA_HOME"]) / "voice-scribe/history.sqlite3"
        destination.parent.mkdir(parents=True, exist_ok=True)
        with (
            sqlite3.connect(f"file:{databases[0]}?mode=ro", uri=True) as source_db,
            sqlite3.connect(destination) as target_db,
        ):
            source_db.backup(target_db)
    width, height = (1080, 1920) if portrait else (1920, 1080)
    app_width, app_height = (952, 1030) if portrait else (1288, 772)
    app_x, app_y = (64, 600) if portrait else (568, 146)
    started = time.monotonic()
    events = []
    children = []
    logs = []
    errors = []
    recorder = None
    shell = None
    audio_first_epoch = []

    def event(name, **details):
        events.append(
            {
                "event": name,
                "seconds": round(time.monotonic() - started, 4),
                "epoch": time.time(),
                **details,
            }
        )

    def launch(command, name):
        log = (output / name).open("w")
        logs.append(log)
        child = subprocess.Popen(
            command, stdout=log, stderr=subprocess.STDOUT, stdin=subprocess.PIPE
        )
        children.append(child)
        return child

    def ipc(*arguments):
        return subprocess.check_output(
            [
                "quickshell",
                "ipc",
                "--pid",
                str(shell.pid),
                "call",
                "campaign",
                *arguments,
            ],
            text=True,
            timeout=5,
        ).strip()

    def screenshot(name):
        frames = []
        app.window.add_tick_callback(
            lambda *_args: frames.append(True) is None and len(frames) < 4
        )
        app.window.queue_draw()
        settle(lambda: len(frames) >= 4)
        subprocess.run(
            [
                "magick",
                "import",
                "-silent",
                "-window",
                "root",
                "-define",
                "png:compression-level=1",
                str(output / f"{name}.png"),
            ],
            check=True,
            timeout=10,
        )
        event("screenshot", file=f"{name}.png", shell=json.loads(ipc("state")))

    theme = Path.home() / ".local/state/omarchy/current/theme"
    for destination in (
        Path(os.environ["XDG_STATE_HOME"]) / "omarchy/current/theme",
        session / ".local/state/omarchy/current/theme",
    ):
        destination.mkdir(parents=True)
        for name in ("colors.toml", "shell.toml"):
            shutil.copy2(theme / name, destination / name)
    for module in ("Commons", "Ui"):
        shutil.copytree(Path("/usr/share/omarchy/shell") / module, output / module)
        for path in (output / module).glob("*.qml"):
            path.write_text(
                path.read_text().replace(
                    'Quickshell.env("HOME")', 'Quickshell.env("OFFSCREEN_SESSION_ROOT")'
                )
            )
    shutil.copytree(
        runtime / "linux/quickshell/mluva.dictation", output / "mluva.dictation"
    )
    shutil.copy2(root / "dev/campaign-stage.qml", output / "shell.qml")
    shutil.copy2(
        root / "linux/resources/com.voicescribe.Linux.svg", output / "mark.svg"
    )
    shutil.copy2(
        Path.home() / ".local/state/omarchy/current/background",
        output / "wallpaper.jpg",
    )
    binaries = output / "bin"
    binaries.mkdir()
    if os.environ["CAMPAIGN_REWRITE_PROVIDER"] == "codex":
        (binaries / "codex").symlink_to(os.environ["CAMPAIGN_CODEX_BINARY"])
    (binaries / "hyprctl").write_text("#!/bin/sh\nprintf '{\"int\":0}\\n'\n")
    (binaries / "hyprctl").chmod(0o700)
    os.environ.update(
        PATH=str(binaries) + os.pathsep + os.environ["PATH"],
        MLUVA_SHELL_COMMAND=str(runtime / "linux/mluva-shell"),
    )
    if os.environ["CAMPAIGN_STT"] == "voxtype":
        models = Path(os.environ["XDG_DATA_HOME"]) / "voxtype/models"
        models.mkdir(parents=True)
        (models / "ggml-base.en.bin").symlink_to(
            Path.home() / ".local/share/voxtype/models/ggml-base.en.bin"
        )
        config_dir = Path(os.environ["XDG_CONFIG_HOME"]) / "voxtype"
        config_dir.mkdir()
        (config_dir / "config.toml").write_text(
            '[whisper]\nmodel = "base.en"\nlanguage = "en"\n'
        )
    pipewire_config = session / "config/pipewire-campaign.conf"
    pipewire_config.write_text("""context.properties = {
        core.daemon = true
        core.name = pipewire-campaign
        default.clock.rate = 16000
        default.clock.quantum = 160
        default.clock.min-quantum = 160
    }
    context.spa-libs = { audio.convert.* = audioconvert/libspa-audioconvert support.* = support/libspa-support }
    context.modules = [
        { name = libpipewire-module-protocol-native }
        { name = libpipewire-module-metadata }
        { name = libpipewire-module-spa-node-factory }
        { name = libpipewire-module-client-node }
        { name = libpipewire-module-adapter }
        { name = libpipewire-module-link-factory }
        { name = libpipewire-module-access }
    ]
    context.objects = [
        { factory = spa-node-factory args = {
            factory.name = support.node.driver node.name = Campaign-Driver priority.driver = 20000
        } }
    ]
    """)

    class CampaignApplication(MluvaApplication):
        """Use isolated stores and real provider transports, without live desktop targets."""

        def _initialize_local_services(self):
            super()._initialize_local_services()
            self.config = replace(
                self.config,
                automatic_titles=False,
                auto_copy_dictation=False,
                auto_copy_rewrite=False,
                auto_paste=False,
                spoken_commands_enabled=False,
                microphone_target="0",
                audio_retention_policy=AudioRetentionPolicy.ALWAYS,
                live_rewrite_enabled="task" in scenario,
                live_rewrite_min_characters=int(os.environ["CAMPAIGN_MIN_CHARACTERS"]),
                live_rewrite_interval_seconds=int(os.environ["CAMPAIGN_LIVE_INTERVAL"]),
                review_timeout_seconds=60,
                transcription_provider=os.environ["CAMPAIGN_STT"],
                voxtype_model="base.en",
                rewrite_provider=os.environ["CAMPAIGN_REWRITE_PROVIDER"],
                litellm_base_url="https://api.z.ai/api/paas/v4",
                litellm_model=os.environ["CAMPAIGN_REWRITE_MODEL"] or None,
                litellm_api_key_env="CAMPAIGN_REWRITE_KEY",
                codex_model=None,
                rewrite_model=os.environ["CAMPAIGN_REWRITE_MODEL"] or None,
                rewrite_fast_mode=False,
            )

        def _initialize_capture_services(self):
            if scenario in {"task", "speech"}:
                super()._initialize_capture_services()

        def _live_rewrite_finished(self, session_id, client, revision, result, model):
            event(
                "live_rewrite_result",
                text=result,
                model=model,
                during_recording=self.capture_started_at is not None,
            )
            value = super()._live_rewrite_finished(
                session_id, client, revision, result, model
            )
            if result and self.conversation_workspace.live_draft() == result:
                clock = self.window.get_frame_clock()
                handler = None

                def painted(_clock):
                    clock.disconnect(handler)
                    event(
                        "gtk_after_paint",
                        text=self.conversation_workspace.live_draft(),
                        during_recording=self.capture_started_at is not None,
                    )

                handler = clock.connect("after-paint", painted)
                self.window.queue_draw()
            return value

        def _new_rewrite_client(self, config=None, **kwargs):
            config = config or self.config
            event(
                "rewrite_setup_start",
                provider=config.rewrite_provider,
                model=config.rewrite_model or config.litellm_model,
            )
            client = super()._new_rewrite_client(config, **kwargs)
            event(
                "rewrite_setup_complete",
                provider=config.rewrite_provider,
                model=config.rewrite_model or config.litellm_model,
            )
            return client

    original_publish = PipeWireRecorder._write_and_publish_audio
    original_handle_event = RealtimeTranscriptionSession._handle_event
    original_connect = realtime.websocket_connect
    original_transform = LiteLLMClient.transform
    original_open = LiteLLMClient._open
    original_codex_request = CodexAppServerClient._request
    original_codex_transform = CodexAppServerClient.transform

    def observed_codex_request(client, method, parameters):
        if method in {"model/list", "thread/start", "turn/start"}:
            event(
                "codex_request",
                method=method,
                model=parameters.get("model") or client.last_model_identifier,
                effort=parameters.get("effort"),
                service_tier=parameters.get("serviceTier"),
            )
        result = original_codex_request(client, method, parameters)
        if method in {"model/list", "thread/start", "turn/start"}:
            event("codex_request_accepted", method=method, model=result.get("model"))
        return result

    def observed_codex_transform(client, *args, **kwargs):
        try:
            result = original_codex_transform(client, *args, **kwargs)
            event(
                "rewrite_provider_complete",
                text=result,
                model=client.last_model_identifier,
            )
            return result
        except CodexAppServerError as error:
            event(
                "rewrite_provider_error",
                reason=str(error),
                model=client.last_model_identifier,
            )
            raise

    def observed_open(client, path, body=None, content_type="application/json"):
        if path == "/chat/completions":
            payload = json.loads(body)
            event(
                "rewrite_http_request",
                model=payload["model"],
                prompt=payload["messages"][0]["content"],
            )
        return original_open(client, path, body, content_type)

    def observed_transform(client, *args, **kwargs):
        try:
            result = original_transform(client, *args, **kwargs)
            event("rewrite_provider_complete", text=result, model=client.model)
            return result
        except ProviderError as error:
            event("rewrite_provider_error", reason=str(error), model=client.model)
            raise

    def observed_connect(*args, **kwargs):
        connection = original_connect(*args, **kwargs)
        event("websocket_open")

        class ObservedConnection:
            """Observe successful sends without changing the released wire messages."""

            sent_bytes = 0

            def send(self, message):
                connection.send(message)
                payload = json.loads(message)
                self.sent_bytes += len(
                    base64.b64decode(payload.get("audio_base_64", ""))
                )
                if payload.get("commit"):
                    event(
                        "outbound_commit", audio_offset_seconds=self.sent_bytes / 32000
                    )

            def recv(self, *args, **kwargs):
                return connection.recv(*args, **kwargs)

            def close(self):
                return connection.close()

        return ObservedConnection()

    def handle_provider_event(provider_session, payload):
        event(
            "provider_event",
            message_type=payload.get("message_type"),
            text=payload.get("text"),
        )
        original_handle_event(provider_session, payload)
        if not provider_session.is_healthy:
            event("provider_failure", reason=provider_session._current_failure())

    def publish(recording_self, recording, frames):
        if not audio_first_epoch:
            audio_first_epoch.append(time.time() - len(frames) / 32000)
            event("first_pcm", epoch_of_first_sample=audio_first_epoch[0])
        return original_publish(recording_self, recording, frames)

    xlib = ctypes.CDLL("libX11.so.6")
    xlib.XOpenDisplay.restype = ctypes.c_void_p
    xlib.XOpenDisplay.argtypes = [ctypes.c_char_p]
    display = xlib.XOpenDisplay(b":193")
    assert display
    xlib.XMoveWindow.argtypes = [
        ctypes.c_void_p,
        ctypes.c_ulong,
        ctypes.c_int,
        ctypes.c_int,
    ]
    xlib.XRaiseWindow.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
    xlib.XFlush.argtypes = [ctypes.c_void_p]
    xlib.XInternAtom.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int]
    xlib.XInternAtom.restype = ctypes.c_ulong
    xlib.XGetSelectionOwner.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
    xlib.XGetSelectionOwner.restype = ctypes.c_ulong
    app = CampaignApplication()

    def exercise():
        nonlocal recorder
        try:
            initial_template = initial_draft(app.config)
            app.window.set_default_size(app_width, app_height)
            try:
                settle(
                    lambda: (
                        app.window.get_width() == app_width
                        and app.window.get_height() == app_height
                    )
                )
            except TimeoutError:
                event(
                    "allocation_failed",
                    actual=[app.window.get_width(), app.window.get_height()],
                )
                screenshot("allocation-failed")
                raise
            xid = app.window.get_surface().get_xid()
            xlib.XMoveWindow(display, xid, app_x, app_y)
            xlib.XRaiseWindow(display, xid)
            xlib.XFlush(display)
            settle(lambda: json.loads(ipc("state"))["width"] == width)
            ipc("layout")
            assert xlib.XGetSelectionOwner(
                display, xlib.XInternAtom(display, b"_NET_WM_CM_S0", 0)
            )
            screenshot("ready")
            if scenario.startswith("layout"):
                ipc("stageImage", str(output / "stage.png"))
                settle(lambda: (output / "stage.png").is_file())
            if scenario == "catalog":
                client = app._new_rewrite_client()
                try:
                    catalog = [asdict(model) for model in client.list_models()]
                    (output / "catalog.json").write_text(
                        json.dumps(catalog, indent=2) + "\n"
                    )
                finally:
                    client.close()
                app.quit()
                return GLib.SOURCE_REMOVE
            if scenario.startswith("saved"):
                entry = app.history_store.recent(limit=1)[0]
                replies = app.conversation_store.replies(entry.identifier)
                title = (
                    "A clearer brief for the sales team"
                    if "task" in scenario
                    else "A voice from Rice University · 1962"
                )
                app.history_store.update_title(entry.identifier, title)
                entry = app.history_store.find(entry.identifier)
                app.conversation_workspace.show_conversation(entry, replies)
                app.conversation_workspace.refresh_history()
                screenshot("hero")
                (output / "recognition.json").write_text(
                    json.dumps(asdict(entry), indent=2) + "\n"
                )
                (output / "replies.json").write_text(
                    json.dumps([asdict(reply) for reply in replies], indent=2) + "\n"
                )
                event(
                    "saved_render",
                    source=str(saved_run),
                    raw_sha256=hashlib.sha256(entry.raw_text.encode()).hexdigest(),
                )
                app.quit()
                return GLib.SOURCE_REMOVE
            if scenario.startswith("layout"):
                app.quit()
                return GLib.SOURCE_REMOVE
            assert app.recorder and app.workflow and app.realtime_client
            app.cleanup_switch.set_active(False)
            recorder = launch(
                [
                    "ffmpeg",
                    "-hide_banner",
                    "-loglevel",
                    "verbose",
                    "-debug_ts",
                    "-f",
                    "x11grab",
                    "-draw_mouse",
                    "0",
                    "-framerate",
                    "30",
                    "-video_size",
                    f"{width}x{height}",
                    "-i",
                    ":193",
                    "-c:v",
                    "libx264",
                    "-preset",
                    "ultrafast",
                    "-crf",
                    "18",
                    "-pix_fmt",
                    "yuv420p",
                    "-an",
                    str(output / "screen.mkv"),
                ],
                "ffmpeg.log",
            )
            settle(lambda: (output / "screen.mkv").exists())
            event("record_control")
            app.record_button.emit("clicked")
            settle(lambda: app.capture_started_at is not None, timeout=20)
            if app.pending_realtime_fallback_reason:
                raise RuntimeError(
                    "Realtime provider did not open; no batch fallback recording is accepted"
                )
            playback = launch(
                [
                    "pw-play",
                    "--target=0",
                    "--properties={ node.name = campaign-playback }",
                    os.environ["CAMPAIGN_AUDIO"],
                ],
                "playback.log",
            )
            audio_nodes = []

            def audio_nodes_ready():
                graph = json.loads(subprocess.check_output(["pw-dump"], text=True))
                audio_nodes[:] = [
                    node
                    for node in graph
                    if node["type"] == "PipeWire:Interface:Node"
                    and node["info"]["props"]["node.name"]
                    in {"campaign-playback", "pw-record"}
                ]
                return len(audio_nodes) == 2

            settle(audio_nodes_ready)
            for node in audio_nodes:
                direction = (
                    "Input"
                    if node["info"]["props"]["node.name"] == "pw-record"
                    else "Output"
                )
                configuration = (
                    "{ direction = "
                    + direction
                    + " mode = dsp format = { mediaType = audio mediaSubtype = raw "
                    "format = F32P rate = 16000 channels = 1 position = [ MONO ] } }"
                )
                subprocess.run(
                    [
                        "pw-cli",
                        "set-param",
                        str(node["id"]),
                        "PortConfig",
                        configuration,
                    ],
                    check=True,
                    stdout=subprocess.DEVNULL,
                )
            ports = []

            def ports_ready():
                ports[:] = subprocess.check_output(
                    ["pw-link", "-o"], text=True
                ).splitlines()
                inputs = subprocess.check_output(
                    ["pw-link", "-i"], text=True
                ).splitlines()
                return any("campaign-playback:" in port for port in ports) and any(
                    "pw-record:" in port for port in inputs
                )

            settle(ports_ready)
            output_port = next(
                port.strip() for port in ports if "campaign-playback:" in port
            )
            input_port = next(
                port.strip()
                for port in subprocess.check_output(
                    ["pw-link", "-i"], text=True
                ).splitlines()
                if "pw-record:" in port
            )
            event("audio_link", output_port=output_port, input_port=input_port)
            subprocess.run(["pw-link", output_port, input_port], check=True)
            screenshots = set()
            last_preview = ""
            while playback.poll() is None:
                settle(lambda: True)
                snapshot = app.realtime_session.snapshot()
                if snapshot.display_text != last_preview:
                    last_preview = snapshot.display_text
                    event(
                        "recognition_preview",
                        text=last_preview,
                        committed=snapshot.committed_text,
                    )
                elapsed = time.monotonic() - app.capture_started_at
                if elapsed > 9 and "recording" not in screenshots:
                    screenshot("recording")
                    ipc("overlayImage", str(output / "widget-recording.png"))
                    screenshots.add("recording")
                if (
                    any(
                        item["event"] == "live_rewrite_result"
                        and item["text"]
                        and item["text"].strip() != initial_template.strip()
                        for item in events
                    )
                    and "live" not in screenshots
                ):
                    screenshot("live")
                    screenshots.add("live")
                if elapsed > 90:
                    raise TimeoutError("Audio did not finish")
                time.sleep(0.04)
            event("audio_complete")
            assert playback.returncode == 0
            event(
                "stop_control", committed=app.realtime_session.snapshot().committed_text
            )
            app.record_button.emit("clicked")
            settle(
                lambda: not app.capture_processing and app.recorder.process is None,
                timeout=30,
            )
            settle(lambda: app.live_schedule is None, timeout=65)
            entry = app.conversation_workspace.entry
            assert entry and entry.raw_text.strip()
            replies = app.conversation_store.replies(entry.identifier)
            # An editorial title is separate from immutable recognition and provider output.
            title = (
                "A clearer brief for the sales team"
                if scenario == "task"
                else "A voice from Rice University · 1962"
            )
            app.history_store.update_title(entry.identifier, title)
            entry = app.history_store.find(entry.identifier)
            app.conversation_workspace.show_conversation(entry, replies)
            app.conversation_workspace.refresh_history()
            screenshot("hero")
            app._publish_review(entry.identifier)
            settle(lambda: json.loads(ipc("state"))["phase"] == "ready")
            screenshot("review")
            ipc("overlayImage", str(output / "widget-review.png"))
            settle(lambda: (output / "widget-review.png").is_file())
            shutil.copy2(entry.retained_audio_path, output / "captured.wav")
            (output / "recognition.json").write_text(
                json.dumps(asdict(entry), indent=2) + "\n"
            )
            (output / "replies.json").write_text(
                json.dumps([asdict(reply) for reply in replies], indent=2) + "\n"
            )
            event(
                "complete",
                raw_sha256=hashlib.sha256(entry.raw_text.encode()).hexdigest(),
            )
            if scenario == "task":
                event(
                    "task_outcome",
                    missing_fields_visible=bool(
                        replies and "[Missing:" in replies[-1].text
                    ),
                    final_update_complete=bool(
                        replies and "partial draft" not in replies[-1].instruction
                    ),
                    draft_during_recording=any(
                        item["event"] == "gtk_after_paint"
                        and item["during_recording"]
                        and item["text"].strip() != initial_template.strip()
                        for item in events
                    ),
                )
            end = time.monotonic() + 3
            settle(lambda: time.monotonic() > end, timeout=5)
        except Exception:  # noqa: BLE001 - retain a failure receipt and shut down the private desktop.
            errors.append(traceback.format_exc())
        app.quit()
        return GLib.SOURCE_REMOVE

    try:
        launch(
            [str(root / "tmp/campaign/tools/usr/bin/xcompmgr"), "-n", "-d", ":193"],
            "compositor.log",
        )
        launch(["pipewire", "-c", str(pipewire_config)], "pipewire.log")
        settle(
            lambda: (Path(os.environ["XDG_RUNTIME_DIR"]) / "pipewire-campaign").exists()
        )
        shell = launch(
            ["quickshell", "--no-color", "-p", str(output / "shell.qml")],
            "quickshell.log",
        )
        with (
            patch("voice_scribe_linux.app.FocusedTextTargetTracker", return_value=None),
            patch.object(PipeWireRecorder, "_write_and_publish_audio", publish),
            patch.object(
                RealtimeTranscriptionSession, "_handle_event", handle_provider_event
            ),
            patch.object(realtime, "websocket_connect", observed_connect),
            patch.object(LiteLLMClient, "transform", observed_transform),
            patch.object(LiteLLMClient, "_open", observed_open),
            patch.object(CodexAppServerClient, "_request", observed_codex_request),
            patch.object(CodexAppServerClient, "transform", observed_codex_transform),
        ):
            app.connect("activate", lambda _app: GLib.timeout_add(750, exercise))
            app.run([])
    finally:
        if recorder is not None and recorder.poll() is None:
            recorder.communicate(b"q", timeout=20)
        service_environments = []
        for child in children:
            if child.poll() is None:
                keys = {
                    item.split(b"=", 1)[0].decode()
                    for item in Path(f"/proc/{child.pid}/environ")
                    .read_bytes()
                    .split(b"\0")
                    if item
                }
                assert not (set(credentials) & keys)
                service_environments.append(
                    {"pid": child.pid, "provider_credentials_present": False}
                )
        for child in reversed(children):
            if child.poll() is None:
                child.terminate()
                child.wait(timeout=10)
        for log in logs:
            log.close()
        if codex_home.exists():
            shutil.rmtree(codex_home)
        receipt = {
            "scenario": scenario,
            "release": os.environ["CAMPAIGN_BUILD_LABEL"],
            "release_commit": os.environ["CAMPAIGN_RELEASE_COMMIT"],
            "runtime_commit": os.environ["CAMPAIGN_RUNTIME_COMMIT"],
            "runtime_root": str(runtime),
            "live_rewrite_interval_seconds": int(os.environ["CAMPAIGN_LIVE_INTERVAL"]),
            "live_rewrite_min_characters": int(os.environ["CAMPAIGN_MIN_CHARACTERS"]),
            "transcription_provider": os.environ["CAMPAIGN_STT"],
            "rewrite_model": os.environ["CAMPAIGN_REWRITE_MODEL"],
            "rewrite_provider": os.environ["CAMPAIGN_REWRITE_PROVIDER"],
            "rewrite_fast_mode": False,
            "host": socket.gethostname(),
            "display": os.environ["DISPLAY"],
            "screen": [width, height],
            "application": [app_x, app_y, app_width, app_height],
            "theme": "Nord",
            "first_pcm_epoch": audio_first_epoch[0] if audio_first_epoch else None,
            "events": events,
            "services": service_environments,
            "errors": errors,
        }
        (output / "capture.json").write_text(json.dumps(receipt, indent=2) + "\n")
    if errors:
        raise RuntimeError("\n".join(errors))


if __name__ == "__main__":
    main()
