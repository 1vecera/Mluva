#!/usr/bin/env python3
"""A stub terminal agent: shows each guarded prompt the Herdr fixture peer receives.

It never runs a model. It follows the peer's audit file and prints prompt texts verbatim.
"""
import json
import sys
import time

path = sys.argv[1]
print("stub agent · waiting for a prompt from Mluva", flush=True)
seen = 0
while True:
    try:
        with open(path) as audit:
            lines = audit.readlines()
    except FileNotFoundError:
        lines = []
    for line in lines[seen:]:
        request = json.loads(line)
        if request.get("method") == "agent.prompt_guarded":
            print("\n── prompt received ──", flush=True)
            print(request["params"]["text"], flush=True)
    seen = len(lines)
    time.sleep(0.2)
