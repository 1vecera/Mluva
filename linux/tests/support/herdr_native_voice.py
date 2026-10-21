#!/usr/bin/env python3
"""Disposable real Codex/Claude TUIs, guarded Herdr and loopback inference only."""

import http.server
import json
import os
import pathlib
import shlex
import socket
import sqlite3
import subprocess
import threading
import time
import uuid

root = pathlib.Path(os.environ["VOICE_PROOF_ROOT"])
exe = os.environ["VOICE_PROOF_HERDR"]
manual_codex = os.environ.get("VOICE_PROOF_CODEX_LAUNCH") == "--manual-codex"
mluva = os.environ.get("VOICE_PROOF_MLUVA")
assert str(root) == "/tmp/herdr-voice", "run through a private native verification wrapper"
if mluva:
    subprocess.run([os.environ["MLUVA_VERIFY_CLI"], "doctor"], check=True, stdout=subprocess.DEVNULL)
assert not any(
    pathlib.Path(p).exists() for p in ["/dev/snd", "/dev/input", "/dev/uinput"]
)
assert mluva or not any(
    os.environ.get(k)
    for k in [
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "DBUS_SESSION_BUS_ADDRESS",
        "WAYLAND_DISPLAY",
        "DISPLAY",
    ]
)
(root / "config/herdr-dev").mkdir(parents=True, exist_ok=True)
(root / "config/herdr-dev/config.toml").write_text(
    "onboarding = false\n[update]\nversion_check = false\nmanifest_check = false\n"
)
for name in ["home/.codex", "home/.claude", "project", "runtime", "data", "state"]:
    (root / name).mkdir(parents=True, exist_ok=True)
requests = []


class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def do_GET(self):
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(b'{"data":[],"models":[]}')

    def do_POST(self):
        data = json.loads(
            self.rfile.read(int(self.headers.get("Content-Length", 0))) or b"{}"
        )
        requests.append({"path": self.path, "body": data})
        answer = "SYNTHETIC_ACK: received by the selected test agent."
        if "messages" in self.path:
            if "count_tokens" in self.path:
                self.send_response(200)
                self.end_headers()
                self.wfile.write(b'{"input_tokens":100}')
                return
            events = [
                (
                    "message_start",
                    {
                        "type": "message_start",
                        "message": {
                            "id": "msg_synthetic",
                            "type": "message",
                            "role": "assistant",
                            "model": "claude-sonnet-4-6",
                            "content": [],
                            "stop_reason": None,
                            "usage": {"input_tokens": 100, "output_tokens": 0},
                        },
                    },
                ),
                (
                    "content_block_start",
                    {
                        "type": "content_block_start",
                        "index": 0,
                        "content_block": {"type": "text", "text": ""},
                    },
                ),
                (
                    "content_block_delta",
                    {
                        "type": "content_block_delta",
                        "index": 0,
                        "delta": {"type": "text_delta", "text": answer},
                    },
                ),
                ("content_block_stop", {"type": "content_block_stop", "index": 0}),
                (
                    "message_delta",
                    {
                        "type": "message_delta",
                        "delta": {"stop_reason": "end_turn", "stop_sequence": None},
                        "usage": {"output_tokens": 20},
                    },
                ),
                ("message_stop", {"type": "message_stop"}),
            ]
        else:
            item = {
                "id": "msg_synthetic",
                "type": "message",
                "status": "completed",
                "role": "assistant",
                "content": [{"type": "output_text", "text": answer, "annotations": []}],
            }
            events = [
                (
                    "response.created",
                    {
                        "type": "response.created",
                        "response": {
                            "id": "resp_synthetic",
                            "status": "in_progress",
                            "output": [],
                        },
                    },
                ),
                (
                    "response.output_item.added",
                    {
                        "type": "response.output_item.added",
                        "output_index": 0,
                        "item": {
                            "id": "msg_synthetic",
                            "type": "message",
                            "status": "in_progress",
                            "role": "assistant",
                            "content": [],
                        },
                    },
                ),
                (
                    "response.content_part.added",
                    {
                        "type": "response.content_part.added",
                        "item_id": "msg_synthetic",
                        "output_index": 0,
                        "content_index": 0,
                        "part": {"type": "output_text", "text": "", "annotations": []},
                    },
                ),
                (
                    "response.output_text.delta",
                    {
                        "type": "response.output_text.delta",
                        "item_id": "msg_synthetic",
                        "output_index": 0,
                        "content_index": 0,
                        "delta": answer,
                    },
                ),
                (
                    "response.output_item.done",
                    {
                        "type": "response.output_item.done",
                        "output_index": 0,
                        "item": item,
                    },
                ),
                (
                    "response.completed",
                    {
                        "type": "response.completed",
                        "response": {
                            "id": "resp_synthetic",
                            "status": "completed",
                            "output": [item],
                            "usage": {
                                "input_tokens": 100,
                                "output_tokens": 20,
                                "total_tokens": 120,
                            },
                        },
                    },
                ),
            ]
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.end_headers()
        for name, event in events:
            self.wfile.write(
                ("event: " + name + "\ndata: " + json.dumps(event) + "\n\n").encode()
            )
        self.wfile.flush()


server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
threading.Thread(target=server.serve_forever, daemon=True).start()
endpoint = "http://127.0.0.1:" + str(server.server_port)
(root / "home/.codex/config.toml").write_text(
    'model="gpt-5.4"\nmodel_provider="fixture"\napproval_policy="never"\nsandbox_mode="read-only"\n[model_providers.fixture]\nname="Local synthetic inference"\nbase_url="'
    + endpoint
    + '/v1"\nwire_api="responses"\n[features]\nresponses_websockets=false\nresponses_websockets_v2=false\nremote_models=false\nenable_request_compression=false\n[projects."'
    + str(root / "project")
    + '"]\ntrust_level="trusted"\n'
)
(root / "home/.claude.json").write_text(
    json.dumps(
        {
            "hasCompletedOnboarding": True,
            "theme": "dark",
            "projects": {
                str(root / "project"): {
                    "hasTrustDialogAccepted": True,
                    "hasCompletedProjectOnboarding": True,
                }
            },
            "customApiKeyResponses": {"approved": ["synthetic-key"], "rejected": []},
        }
    )
)
env = dict(
    os.environ,
    ANTHROPIC_BASE_URL=endpoint,
    ANTHROPIC_API_KEY="synthetic-key",
    CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC="1",
    DISABLE_AUTOUPDATER="1",
    HERDR_SOCKET_PATH=str(root / "runtime/h.sock"),
    SHELL="/bin/bash",
    HOME=str(root / "home"),
    XDG_CONFIG_HOME=str(root / "config"),
    XDG_DATA_HOME=str(root / "data"),
    XDG_STATE_HOME=str(root / "state"),
    XDG_RUNTIME_DIR=str(root / "runtime"),
)
for key in ["DISPLAY", "WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS", "AT_SPI_BUS_ADDRESS"]:
    env.pop(key, None)
for kind in ["codex", "claude"]:
    p = subprocess.run(
        [exe, "integration", "install", kind], env=env, capture_output=True, text=True
    )
    print("integration", kind, p.returncode, flush=True)
log = open(root / "server.log", "wb")
child = subprocess.Popen(
    [exe, "server"],
    env=env,
    stdin=subprocess.DEVNULL,
    stdout=log,
    stderr=log,
    start_new_session=True,
)


def rpc(method, params=None):
    with socket.socket(socket.AF_UNIX) as s:
        s.settimeout(8)
        s.connect(env["HERDR_SOCKET_PATH"])
        s.sendall(
            (
                json.dumps(
                    {"id": str(uuid.uuid4()), "method": method, "params": params or {}}
                )
                + "\n"
            ).encode()
        )
        return json.loads(s.makefile().readline())


try:
    for _ in range(150):
        if pathlib.Path(env["HERDR_SOCKET_PATH"]).exists():
            break
        time.sleep(0.05)
    workspace = rpc(
        "workspace.create",
        {
            "cwd": str(root / "project"),
            "label": "Synthetic voice proof",
            "focus": False,
        },
    )
    assert "result" in workspace, workspace
    pane = workspace["result"]["root_pane"]["pane_id"]
    time.sleep(0.3)
    codex_args = [
        "--no-alt-screen", "--no-daemon", "--disable", "hooks",
        "Bootstrap the synthetic transport fixture; reply briefly without tools.",
    ]
    if manual_codex:
        started = rpc("pane.send_text", {
            "pane_id": pane,
            "text": shlex.join(["codex", *codex_args]) + "\n",
        })
        assert "result" in started, started
    else:
        started = rpc("agent.start", {
            "pane_id": pane,
            "kind": "codex",
            "name": "voice-codex",
            "args": codex_args,
            "timeout_ms": 15000,
        })
    assert "result" in started, started
    time.sleep(6)
    database = sqlite3.connect(
        "file:" + str(root / "home/.codex/state_5.sqlite") + "?mode=ro", uri=True
    )
    native = database.execute("select id,rollout_path from threads").fetchone()
    assert native, "Codex did not create its native session"
    hook = root / "home/.codex/herdr-agent-state.sh"
    subprocess.run(
        [str(hook), "session"],
        input=json.dumps(
            {
                "hook_event_name": "SessionStart",
                "session_id": native[0],
                "transcript_path": native[1],
                "source": "startup",
            }
        ),
        text=True,
        env=dict(env, HERDR_ENV="1", HERDR_PANE_ID=pane),
        check=True,
    )
    print("codex-fixture-identity-reported", flush=True)
    if manual_codex:
        renamed = rpc("agent.rename", {"target": pane, "name": "voice-codex"})
        assert "result" in renamed, renamed
    split = rpc(
        "pane.split",
        {
            "target_pane_id": pane,
            "direction": "right",
            "focus": False,
            "cwd": str(root / "project"),
        },
    )
    assert "result" in split, split
    other = split["result"]["pane"]["pane_id"]
    time.sleep(0.3)
    started = rpc(
        "agent.start",
        {
            "pane_id": other,
            "kind": "claude",
            "name": "voice-claude",
            "args": ["--permission-mode", "plan", "--tools", "", "--model", "sonnet"],
            "timeout_ms": 15000,
        },
    )
    assert "result" in started, started
    time.sleep(7)
    listing = rpc("agent.guarded_list")["result"]
    print("guarded-count", len(listing["agents"]), flush=True)
    assert len(listing["agents"]) == 2
    if manual_codex:
        assert "result" in rpc("pane.send_text", {"pane_id": pane, "text": "UNSENT_DRAFT"})
        time.sleep(0.4)
        assert all(
            entry["agent"]["pane_id"] != pane
            for entry in rpc("agent.guarded_list")["result"]["agents"]
        ), "A directly launched Codex with a nonempty composer must be excluded"
        assert "result" in rpc("pane.send_keys", {"pane_id": pane, "keys": ["ctrl+u"]})
        time.sleep(0.4)
        listing = rpc("agent.guarded_list")["result"]
        assert len(listing["agents"]) == 2
    if mluva:
        helper = os.environ["MLUVA_VERIFY_CLI"]
        expected_text = os.environ["VOICE_PROOF_EXPECTED_TEXT"]
        codex = next(e for e in listing["agents"] if e["agent"]["agent"] == "codex")

        def app_cli(*arguments):
            return json.loads(
                subprocess.check_output([mluva, "herdr", *arguments], text=True)
            )

        def portal(action):
            subprocess.run(
                [
                    "gdbus",
                    "call",
                    "--session",
                    "--dest",
                    "org.freedesktop.portal.Desktop",
                    "--object-path",
                    "/org/freedesktop/portal/desktop",
                    "--method",
                    "com.mluva.TestPortal.Drive",
                    json.dumps(json.dumps(action)),
                ],
                check=True,
                stdout=subprocess.DEVNULL,
            )

        assert app_cli(
            "select", codex["agent"]["pane_id"], "--socket", env["HERDR_SOCKET_PATH"]
        )["connected"]
        for _ in range(100):
            if app_cli("status")["agent_shortcut"]["ready"]:
                break
            time.sleep(0.1)
        else:
            raise AssertionError("Private global shortcut was not approved")
        approved = app_cli("status")
        assert approved["shortcuts"]["recording_trigger"] == "F9"
        assert approved["hold_key"] == "F12"
        assert approved["shortcuts"]["shortcut_service_available"]
        portal({"op": "Activated", "id": "agent-hold"})
        for _ in range(50):
            discovery = json.loads(subprocess.check_output([helper, "discover"], text=True))
            if "Choose a Herdr session" in json.dumps(discovery):
                break
            time.sleep(0.1)
        else:
            raise AssertionError("First F12 did not display the real session wheel")
        assert app_cli("status")["delivery"] != "recording"
        assert not pathlib.Path(
            os.environ["OFFSCREEN_SESSION_ROOT"], "tools/raw.ready.json"
        ).exists(), "First F12 opened the microphone"
        portal({"op": "Activated", "id": "agent-hold"})
        assert app_cli("status")["delivery"] != "recording", "Held-key repeat started capture"
        # Release rearms the real portal action; the second press starts capture.
        portal({"op": "Deactivated", "id": "agent-hold"})
        portal({"op": "Activated", "id": "agent-hold"})
        for _ in range(100):
            if pathlib.Path(
                os.environ["OFFSCREEN_SESSION_ROOT"], "tools/raw.ready.json"
            ).exists():
                break
            time.sleep(0.1)
        else:
            raise AssertionError("Global shortcut did not start actual capture")
        portal({"op": "Deactivated", "id": "agent-hold"})
        assert app_cli("status")["delivery"] == "recording", (
            "Releasing the key stopped toggle recording"
        )
        subprocess.run([helper, "click", "Stop and send"], check=True)
        for _ in range(150):
            state = app_cli("status")
            if state["delivery"] == "sent":
                break
            assert state["delivery"] not in ["draft", "unconfirmed"], state
            time.sleep(0.1)
        else:
            raise AssertionError(
                "Native Mluva did not receive a guarded delivery receipt"
            )
        # The actual Codex must consume the speech result through its model API.
        for _ in range(100):
            routed = [
                r
                for r in requests
                if any(
                    m.get("role") == "user"
                    and any(
                        isinstance(p, dict) and p.get("text") == expected_text
                        for p in m.get("content", [])
                    )
                    for m in r["body"].get("input", [])
                    if isinstance(m, dict)
                )
            ]
            if routed:
                break
            time.sleep(0.1)
        assert len(routed) == 1, (
            "Expected one actual Codex inference request",
            len(routed),
        )
        subprocess.run([helper, "assert", "raw", expected_text], check=True)
        subprocess.run([helper, "assert", "count", "1"], check=True)
        subprocess.run([helper, "screenshot", "agent-end-to-end"], check=True)
        result = {
            "passed": True,
            "pipeline": [
                "private global shortcut activation",
                "actual PCM capture",
                "native Stop and send button",
                "speech protocol peer",
                "Mluva guarded submission",
                "actual Herdr server",
                "actual Codex",
                "loopback Responses inference",
            ],
            "native_messages": len(routed),
            "key_release_keeps_recording": True,
            "first_f12_only_selects": True,
            "second_f12_starts_recording": True,
            "receipt": "accepted",
            "hold_required": False,
            "extra_send_confirmation": False,
            "inference": "synthetic private loopback; no external provider",
        }
        (
            pathlib.Path(os.environ["OFFSCREEN_SESSION_ROOT"]) / "agent-end-to-end.json"
        ).write_text(json.dumps(result, indent=2))
        print("MLUVA_END_TO_END", json.dumps(result), flush=True)
        raise SystemExit(0)
    receipts = []
    for entry in listing["agents"]:
        kind = entry["agent"]["agent"]
        marker = "ROUTED_SYNTHETIC_" + kind.upper()
        request = {
            "target": entry["agent"]["pane_id"],
            "server_epoch": listing["server_epoch"],
            "guard": entry["guard"],
            "operation_id": str(uuid.uuid4()),
            "expires_at_unix_ms": int(time.time() * 1000) + 5000,
            "text": marker + " Reply only with acknowledgement, without tools.",
        }
        start = time.monotonic()
        first = rpc("agent.prompt_guarded", request)
        for _ in range(60):
            receipt = rpc(
                "agent.prompt_receipt",
                {
                    "server_epoch": listing["server_epoch"],
                    "operation_id": request["operation_id"],
                },
            )
            if receipt.get("result", {}).get("state") != "pending":
                break
            time.sleep(0.1)
        assert receipt.get("result", {}).get("state") == "accepted", receipt
        elapsed = round((time.monotonic() - start) * 1000)
        assert rpc("agent.prompt_guarded", request)["result"]["state"] == "accepted"
        for _ in range(80):
            routed = [
                r
                for r in requests
                if "count_tokens" not in r["path"]
                and any(
                    m.get("role") == "user"
                    and (
                        m.get("content", "").startswith(marker)
                        if isinstance(m.get("content"), str)
                        else any(
                            isinstance(p, dict) and p.get("text", "").startswith(marker)
                            for p in m.get("content", [])
                        )
                    )
                    for m in r["body"].get("input", r["body"].get("messages", []))
                    if isinstance(m, dict)
                )
            ]
            if routed:
                break
            time.sleep(0.1)
        assert len(routed) == 1, (kind, len(routed))
        assert ("/responses" in routed[0]["path"]) == (kind == "codex")
        other_marker = "ROUTED_SYNTHETIC_" + ("CLAUDE" if kind == "codex" else "CODEX")
        assert other_marker not in json.dumps(routed[0]["body"])
        receipts.append(
            {
                "agent": kind,
                "receipt": "accepted",
                "native_inference_requests_for_message": 1,
                "duplicate_reconciled": True,
                "submit_to_receipt_ms": elapsed,
            }
        )
        print("accepted", kind, elapsed, flush=True)
        time.sleep(0.5)
    # A rename changes the opaque identity without stopping the recipient.
    assert "result" in rpc(
        "agent.rename", {"target": request["target"], "name": "voice-renamed"}
    )
    request.update(
        operation_id=str(uuid.uuid4()),
        expires_at_unix_ms=int(time.time() * 1000) + 5000,
    )
    assert rpc("agent.prompt_guarded", request)["result"]["state"] == "rejected"
    assert len(rpc("agent.guarded_list")["result"]["agents"]) == 2
    assert (
        rpc(
            "agent.prompt_receipt",
            {"server_epoch": "stale", "operation_id": request["operation_id"]},
        )["error"]["code"]
        == "stale_server"
    )
    print("rename-rejected-recipient-still-running", flush=True)
    print(
        "PROOF_RESULT",
        json.dumps(
            {
                "receipts": receipts,
                "codex_launch": "direct shell command" if manual_codex else "managed",
                "codex_identity": "native session metadata replayed through reviewed Herdr hook; automatic hook trust not bypassed",
                "inference": "private loopback synthetic responses; no paid requests",
            }
        ),
        flush=True,
    )
    (root / "result.json").write_text(
        json.dumps(
            {
                "receipts": receipts,
                "rename_rejected_recipient_running": True,
                "codex_launch": "direct shell command" if manual_codex else "managed",
                "manual_draft_excluded": manual_codex,
                "stale_epoch_rejected": True,
                "codex_identity": "Actual session reported through Herdr hook by fixture; automatic trust approval remains manual",
                "inference": "loopback synthetic; network namespace has no external connection",
            },
            indent=2,
        )
    )

finally:
    try:
        rpc("server.stop")
    except Exception:
        child.terminate()
    try:
        child.wait(timeout=8)
    except subprocess.TimeoutExpired:
        child.kill()
        child.wait()
    log.close()
    server.shutdown()
