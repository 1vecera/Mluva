"""Verify the installed Codex against loopback inference and synthetic configuration only."""

import argparse
import json
import os
import shlex
import tempfile
from http.server import BaseHTTPRequestHandler
from pathlib import Path

from http_fixture import local_http_server

from mluva_linux.codex_client import CodexAppServerClient, CodexAppServerError

requests = []
inject_tool = False
command_marker = None


class Handler(BaseHTTPRequestHandler):
    """Return text or a deliberately unsolicited tool call from a loopback model fixture."""

    def log_message(self, *_args):
        """Keep synthetic provider traffic out of the test log."""
        pass

    def do_POST(self):
        """Capture the actual tool catalog and emit Responses API streaming events."""
        request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        requests.append(request)
        item = {
            "id": "msg_fixture",
            "type": "message",
            "role": "assistant",
            "status": "completed",
            "content": [{"type": "output_text", "text": "Synthetic transformed text.", "annotations": []}],
        }
        events = [
            {
                "type": "response.created",
                "response": {"id": "resp_fixture", "object": "response", "status": "in_progress"},
            },
            {
                "type": "response.output_item.added",
                "output_index": 0,
                "item": {**item, "status": "in_progress", "content": []},
            },
            {
                "type": "response.content_part.added",
                "item_id": "msg_fixture",
                "output_index": 0,
                "content_index": 0,
                "part": {"type": "output_text", "text": "", "annotations": []},
            },
            {
                "type": "response.output_text.delta",
                "item_id": "msg_fixture",
                "output_index": 0,
                "content_index": 0,
                "delta": "Synthetic transformed text.",
            },
            {"type": "response.output_item.done", "output_index": 0, "item": item},
            {
                "type": "response.completed",
                "response": {
                    "id": "resp_fixture",
                    "object": "response",
                    "status": "completed",
                    "output": [item],
                    "usage": {"input_tokens": 10, "output_tokens": 4, "total_tokens": 14},
                },
            },
        ]
        if inject_tool and len(requests) == 1:
            tool = {
                "id": "tool_fixture",
                "type": "function_call",
                "name": "exec_command",
                "call_id": "call_fixture",
                "arguments": json.dumps({"cmd": f"touch {shlex.quote(str(command_marker))}"}),
            }
            events = [
                events[0],
                {"type": "response.output_item.done", "output_index": 0, "item": tool},
                {
                    "type": "response.completed",
                    "response": {"id": "resp_fixture", "object": "response", "status": "completed", "output": [tool]},
                },
            ]
        payload = "".join(
            "event: " + event["type"] + "\ndata: " + json.dumps(event) + "\n\n" for event in events
        ).encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("evidence_directory", type=Path)
args = parser.parse_args()
args.evidence_directory = args.evidence_directory.resolve()
args.evidence_directory.mkdir(parents=True, mode=0o700, exist_ok=True)
summaries = []
for inject_tool in (False, True):
    with tempfile.TemporaryDirectory(dir=args.evidence_directory) as location, local_http_server(Handler) as server:
        requests.clear()
        root = Path(location)
        os.environ["CODEX_HOME"] = str(root)
        os.environ.pop("OPENAI_API_KEY", None)
        marker = root / "mcp-started"
        command_marker = root / "command-started"
        (root / "config.toml").write_text(
            '[mcp_servers.canary]\ncommand="/usr/bin/touch"\nargs=["' + str(marker) + '"]\n'
        )
        (root / "AGENTS.md").write_text("PRIVATE_INSTRUCTION_CANARY")
        (root / "AGENTS.override.md").write_text("PRIVATE_OVERRIDE_CANARY")
        command = (
            "codex",
            "app-server",
            "--listen",
            "stdio://",
            "-c",
            "features.remote_models=false",
            "-c",
            "features.responses_websockets=false",
            "-c",
            "features.responses_websockets_v2=false",
            "-c",
            "features.enable_request_compression=false",
            "-c",
            'model_provider="mock"',
            "-c",
            'model_providers.mock={name="Mock",base_url="http://127.0.0.1:'
            + str(server.server_port)
            + '",wire_api="responses"}',
        )
        client = CodexAppServerClient(command=command, request_timeout_seconds=10, turn_timeout_seconds=10)
        try:
            assert client.transform("Synthetic source only.", root, model="gpt-5.4") == "Synthetic transformed text."
        except CodexAppServerError as error:
            assert inject_tool and "outside text-only" in str(error), str(error)
        finally:
            client.close()
        assert not marker.exists(), "Inherited MCP server was started"
        assert not command_marker.exists(), "An unsolicited command was executed"
        assert requests and all(request["tools"] == [] for request in requests)
        assert "PRIVATE_INSTRUCTION_CANARY" not in json.dumps(requests)
        assert "PRIVATE_OVERRIDE_CANARY" not in json.dumps(requests)
        assert (root / "AGENTS.md").read_text() == "PRIVATE_INSTRUCTION_CANARY"
        assert (root / "AGENTS.override.md").read_text() == "PRIVATE_OVERRIDE_CANARY"
        summaries.append(
            {
                "unsolicited_tool_call": inject_tool,
                "model_requests": len(requests),
                "tools": [],
                "mcp_started": False,
                "command_executed": False,
                "instructions_disclosed": False,
            }
        )
args.evidence_directory.joinpath("results.json").write_text(json.dumps(summaries, indent=2) + "\n")
print("Installed Codex: tool-free inference and unsolicited-command rejection passed.")
