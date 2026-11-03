#!/usr/bin/env python3
"""Loopback stand-in for ElevenLabs Scribe inside the film VM: the unmodified app talks to it over its production routes.

The guest maps api.elevenlabs.io to 127.0.0.1 and trusts a CA made for the VM, so nothing leaves the guest. Each realtime
session plays the next entry of `sessions` in the script file: `partials` are [seconds after the first audio chunk, text]
pairs, taken from Daniel's own word timing, and `final` is committed `final_delay` seconds after Stop. The batch route
returns the same final text with even word times for annotated recordings. Every event is logged with its clock time.
"""

import asyncio
import base64
import http
import json
import ssl
import sys
import time
from pathlib import Path

from websockets.asyncio.server import serve

script_path, log_path, cert, key = sys.argv[1:5]
state = {"index": 0, "last": None}


def log(event, **fields):
    with open(log_path, "a") as stream:
        stream.write(
            json.dumps({"t": round(time.time(), 3), "event": event, **fields}) + "\n"
        )


def next_session():
    sessions = json.loads(Path(script_path).read_text())["sessions"]
    entry = sessions[min(state["index"], len(sessions) - 1)]
    state["index"] += 1
    state["last"] = entry
    return entry


async def realtime(connection):
    session = next_session()
    log("session", index=state["index"])
    await connection.send(
        json.dumps(
            {"message_type": "session_started", "session_id": f"film{state['index']}"}
        )
    )
    player = None

    async def play():
        start = time.monotonic()
        for at, text in session.get("partials", []):
            await asyncio.sleep(max(0, at - (time.monotonic() - start)))
            await connection.send(
                json.dumps({"message_type": "partial_transcript", "text": text})
            )
            log("partial", text=text)

    async for message in connection:
        event = json.loads(message)
        if event.get("message_type") != "input_audio_chunk":
            continue
        if player is None and base64.b64decode(event.get("audio_base_64", "")):
            log("first_audio")
            player = asyncio.create_task(play())
        if event.get("commit"):
            log("commit")
            await asyncio.sleep(session.get("final_delay", 0))
            if player:
                player.cancel()
            await connection.send(
                json.dumps(
                    {"message_type": "committed_transcript", "text": session["final"]}
                )
            )
            log("committed", text=session["final"])


def batch_reply():
    entry = state["last"] or {"final": ""}
    words = entry["final"].split()
    step = 0.4
    return {
        "language_code": "eng",
        "language_probability": 1,
        "text": entry["final"],
        "words": [
            {"text": word, "type": "word", "start": i * step, "end": (i + 1) * step}
            for i, word in enumerate(words)
        ],
    }


def route(connection, request):
    if request.path.startswith("/v1/speech-to-text/realtime"):
        return None
    log("http", path=request.path)
    body = json.dumps(batch_reply()).encode()
    return connection.respond(http.HTTPStatus.OK, body.decode())


async def main():
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(cert, key)
    async with serve(
        realtime, "127.0.0.1", 443, ssl=context, process_request=route, max_size=2**24
    ):
        log("ready")
        await asyncio.Future()


asyncio.run(main())
