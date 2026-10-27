"""Loopback models for disposable agent sessions: one scripted reply per product, nothing leaves the host.

Codex speaks the Responses API, Claude Code the Messages API. Title requests get a fixed fixture
title, so the sessions carry a readable name. Every request body is appended to requests.jsonl.
"""

import http.server
import json
import sys
import time

port, log_path = int(sys.argv[1]), sys.argv[2]
# Codex replies by what the person last asked; the first exchange only opens the thread.
CODEX_REPLIES = [
    (
        "changelog entry",
        "On it: adding the entry under Unreleased, then running the tests.",
    ),
    ("CHANGELOG.md", "Read it. Ready for your next request."),
]
CODEX_REPLY = "Done."


def last_user_text(body: dict) -> str:
    for item in reversed(
        body.get("input", []) if isinstance(body.get("input"), list) else []
    ):
        if isinstance(item, dict) and item.get("role") == "user":
            return json.dumps(item.get("content", ""))
    return ""


CLAUDE_REPLY = "Looking at the portal test now."
# A request typed into Claude Code before the dictation, answered slowly so the session is still
# working when the dictation arrives and Claude Code queues it.
CLAUDE_SLOW_TRIGGER = "Run the portal tests"
CLAUDE_SLOW_REPLY = [
    "Running the portal tests.",
    " 14 of 15 pass.",
    " shortcut_portal_cold_start",
    " fails on a cold start:",
    " the portal answered",
    " after the 5 s timeout.",
]
CLAUDE_SLOW_SECONDS = 10.0


def last_claude_user_text(body: dict) -> str:
    for message in reversed(body.get("messages", []) or []):
        if isinstance(message, dict) and message.get("role") == "user":
            return json.dumps(message.get("content", ""))
    return ""


CODEX_TITLE = "Release notes"


def is_title_request(body: dict) -> bool:
    text = json.dumps(body).lower()
    return (
        "title" in text
        and ("concise" in text or "short title" in text or "thread title" in text)
        and len(text) < 20000
    )


class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def do_GET(self):
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(b'{"data":[],"models":[]}')

    def do_POST(self):
        raw = self.rfile.read(int(self.headers.get("Content-Length", 0) or 0))
        try:
            body = json.loads(raw or b"{}")
        except ValueError:
            body = {}
        with open(log_path, "a") as log:
            log.write(json.dumps({"path": self.path, "body": body}) + "\n")
        if "count_tokens" in self.path:
            self.send_response(200)
            self.end_headers()
            self.wfile.write(b'{"input_tokens":100}')
            return
        slow = False
        if "messages" in self.path:
            slow = CLAUDE_SLOW_TRIGGER in last_claude_user_text(body)
            reply = CLAUDE_REPLY
            events = [
                (
                    "message_start",
                    {
                        "type": "message_start",
                        "message": {
                            "id": "msg_fixture",
                            "type": "message",
                            "role": "assistant",
                            "model": "loopback-model",
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
                *[
                    (
                        "content_block_delta",
                        {
                            "type": "content_block_delta",
                            "index": 0,
                            "delta": {"type": "text_delta", "text": part},
                        },
                    )
                    for part in (CLAUDE_SLOW_REPLY if slow else [reply])
                ],
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
            asked = last_user_text(body)
            reply = next((r for key, r in CODEX_REPLIES if key in asked), CODEX_REPLY)
            if is_title_request(body):
                reply = CODEX_TITLE
            item = {
                "id": "msg_fixture",
                "type": "message",
                "status": "completed",
                "role": "assistant",
                "content": [{"type": "output_text", "text": reply, "annotations": []}],
            }
            events = [
                (
                    "response.created",
                    {
                        "type": "response.created",
                        "response": {
                            "id": "resp_fixture",
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
                        "item": {**item, "status": "in_progress", "content": []},
                    },
                ),
                (
                    "response.content_part.added",
                    {
                        "type": "response.content_part.added",
                        "item_id": "msg_fixture",
                        "output_index": 0,
                        "content_index": 0,
                        "part": {"type": "output_text", "text": "", "annotations": []},
                    },
                ),
                (
                    "response.output_text.delta",
                    {
                        "type": "response.output_text.delta",
                        "item_id": "msg_fixture",
                        "output_index": 0,
                        "content_index": 0,
                        "delta": reply,
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
                            "id": "resp_fixture",
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
        pause = (
            CLAUDE_SLOW_SECONDS / len(CLAUDE_SLOW_REPLY)
            if "messages" in self.path and slow
            else 0
        )
        for name, event in events:
            self.wfile.write(f"event: {name}\ndata: {json.dumps(event)}\n\n".encode())
            self.wfile.flush()
            if pause and name == "content_block_delta":
                time.sleep(pause)


http.server.ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()
