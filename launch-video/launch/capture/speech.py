"""Loopback speech endpoint: each transcription request returns the next scripted dictation."""

import http.server
import json
import sys

port, log_path = int(sys.argv[1]), sys.argv[2]
DICTATIONS = [
    "Add a changelog entry for the agent routing release, then run the tests.",
    "Check why the shortcut portal test is flaky on a cold start.",
]
served = 0


class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def do_POST(self):
        global served
        self.rfile.read(int(self.headers.get("Content-Length", 0) or 0))
        text = DICTATIONS[min(served, len(DICTATIONS) - 1)]
        served += 1
        with open(log_path, "a") as log:
            log.write(json.dumps({"path": self.path, "text": text}) + "\n")
        payload = json.dumps({"text": text, "language_code": "eng"}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)


http.server.ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()
