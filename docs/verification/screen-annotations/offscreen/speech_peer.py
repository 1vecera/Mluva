#!/usr/bin/env python3
"""Loopback OpenAI-compatible transcription peer with a scripted, synthetic narration.

Preview chunks (short uploads) receive the sentence that belongs to their time, so the live
preview grows as speech would; the final upload (the whole recording) receives all of it.
Every request is logged as JSON lines with its audio length. Nothing leaves 127.0.0.1.
"""
import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

SENTENCES = [
    (0.5, "Here is the synthetic settings page we are fixing."),
    (4.5, "Look at this Save button, it sits outside the card."),
    (9.5, "And this sync folder label runs past the right edge."),
    (13.0, "Now an error banner covers the title, so pin one marks where it should sit instead."),
]
LOG = open(sys.argv[1], "a", buffering=1)
state = {"offset": 0.0}


def audio_seconds(body: bytes) -> float:
    start = body.find(b"RIFF")
    return max(0.0, (len(body) - start - 44) / 32000.0) if start >= 0 else 0.0


class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
        seconds = audio_seconds(body)
        if seconds > 6.0:
            text = " ".join(sentence for _, sentence in SENTENCES)
            kind = "final"
        else:
            start, end = state["offset"], state["offset"] + seconds
            state["offset"] = end
            text = " ".join(s for at, s in SENTENCES if start <= at < end)
            kind = "preview"
        LOG.write(json.dumps({"path": self.path, "kind": kind, "audio_seconds": round(seconds, 2), "text": text}) + "\n")
        payload = json.dumps({"text": text}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def log_message(self, *args):
        pass


server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
print(server.server_address[1], flush=True)
server.serve_forever()
