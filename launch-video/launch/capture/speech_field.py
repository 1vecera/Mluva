"""Loopback speech peer for take A: a scripted, synthetic sentence that grows as it is "spoken".

Mluva's compatible provider uploads each preview chunk (3 s of audio) on its own and the whole
recording on Stop. A preview chunk receives the words whose scripted time falls inside it, so the
widget's live words grow chunk by chunk as speech would; the final upload receives the sentence.
Every request is logged as a JSON line with its audio length. Nothing leaves 127.0.0.1.
"""

import http.server
import json
import sys

port, log_path = int(sys.argv[1]), sys.argv[2]
SENTENCE = (
    "Thanks for the notes. Let's move the launch review to Thursday at ten,"
    " and I'll bring the new storyboard."
)
# Words spread over 7.5 s of speech from 0.4 s, about 150 words a minute.
WORDS = SENTENCE.split()
TIMES = [0.4 + i * 7.5 / len(WORDS) for i in range(len(WORDS))]
state = {"offset": 0.0}


def audio_seconds(body: bytes) -> float:
    start = body.find(b"RIFF")
    return max(0.0, (len(body) - start - 44) / 32000.0) if start >= 0 else 0.0


class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, format, *args):
        pass

    def do_GET(self):
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(b'{"data":[]}')

    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("Content-Length", 0) or 0))
        seconds = audio_seconds(body)
        if seconds > 3.5:
            text, kind = SENTENCE, "final"
        else:
            start, end = state["offset"], state["offset"] + seconds
            state["offset"] = end
            text = " ".join(w for w, t in zip(WORDS, TIMES) if start <= t < end)
            kind = "preview"
        with open(log_path, "a") as log:
            log.write(
                json.dumps(
                    {
                        "path": self.path,
                        "kind": kind,
                        "audio_seconds": round(seconds, 2),
                        "text": text,
                    }
                )
                + "\n"
            )
        payload = json.dumps({"text": text, "language_code": "eng"}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)


http.server.ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()
