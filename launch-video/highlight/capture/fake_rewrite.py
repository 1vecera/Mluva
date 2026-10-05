#!/usr/bin/env python3
"""Loopback OpenAI-compatible peer that streams one prepared rewrite (demo only, no network)."""
import json, sys, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
TEXT = sys.argv[2]
class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        body = json.dumps({"object": "list", "data": [{"id": "demo-rewrite", "object": "model"}]}).encode()
        self.send_response(200); self.send_header("content-type", "application/json"); self.send_header("content-length", str(len(body))); self.end_headers(); self.wfile.write(body)
    def do_POST(self):
        self.rfile.read(int(self.headers.get("content-length", 0)))
        self.send_response(200); self.send_header("content-type", "text/event-stream"); self.end_headers()
        words = TEXT.split(" ")
        for i in range(0, len(words), 2):
            chunk = " ".join(words[i:i+2]) + ("" if i + 2 >= len(words) else " ")
            self.wfile.write(("data: " + json.dumps({"choices": [{"index": 0, "delta": {"content": chunk}}]}) + "\n\n").encode()); self.wfile.flush(); time.sleep(0.12)
        self.wfile.write(b'data: {"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}\n\ndata: [DONE]\n\n'); self.wfile.flush()
ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
