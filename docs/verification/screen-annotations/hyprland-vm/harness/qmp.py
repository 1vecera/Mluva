#!/usr/bin/env python3
"""Drive the test VM's virtual keyboard and tablet through QMP, and take screendumps.

    qmp.py SOCKET COMMAND...

Commands run in order:
  key NAME...            press and release a chord, e.g. `key shift a`, `key ctrl alt esc`
  down NAME | up NAME    press or release one key
  type TEXT              type ASCII text, using Shift for capitals and symbols
  move X Y               absolute tablet position in guest layout pixels (needs --size)
  press [left|right|middle] | release [...] | click [...]
  path FILE STEP_MS      tablet moves through `x y` lines, one every STEP_MS
  line X0 Y0 X1 Y1 N STEP_MS
  sleep MS
  screendump FILE [HEAD]  PPM or PNG (by extension) of the guest's console HEAD
  raw JSON               any QMP command
Every sent input event is logged as one JSON line with a monotonic timestamp when --log is given.
"""

import json
import socket
import sys
import time

SHIFTED = {
    "!": "1",
    "@": "2",
    "#": "3",
    "$": "4",
    "%": "5",
    "^": "6",
    "&": "7",
    "*": "8",
    "(": "9",
    ")": "0",
    "_": "minus",
    "+": "equal",
    "{": "bracket_left",
    "}": "bracket_right",
    ":": "semicolon",
    '"': "apostrophe",
    "<": "comma",
    ">": "dot",
    "?": "slash",
    "~": "grave_accent",
    "|": "backslash",
}
PLAIN = {
    " ": "spc",
    "-": "minus",
    "=": "equal",
    "[": "bracket_left",
    "]": "bracket_right",
    ";": "semicolon",
    "'": "apostrophe",
    ",": "comma",
    ".": "dot",
    "/": "slash",
    "`": "grave_accent",
    "\\": "backslash",
    "\n": "ret",
}
ALIASES = {
    "shift": "shift",
    "lshift": "shift",
    "rshift": "shift_r",
    "ctrl": "ctrl",
    "alt": "alt",
    "super": "meta_l",
    "enter": "ret",
    "return": "ret",
    "esc": "esc",
    "escape": "esc",
    "space": "spc",
}


class Qmp:
    def __init__(self, path, size=None, log=False):
        self.sock = socket.socket(socket.AF_UNIX)
        self.sock.connect(path)
        self.file = self.sock.makefile("rwb")
        self.size = size
        self.log = log
        self._read()
        self.call("qmp_capabilities")

    def _read(self):
        while True:
            message = json.loads(self.file.readline())
            if "event" not in message:
                return message

    def call(self, command, **arguments):
        payload = {"execute": command}
        if arguments:
            payload["arguments"] = arguments
        self.file.write(json.dumps(payload).encode() + b"\n")
        self.file.flush()
        reply = self._read()
        if "error" in reply:
            raise SystemExit(f"QMP {command}: {reply['error']}")
        return reply.get("return")

    def events(self, events):
        # QMP's optional `device` names a display console, not an input device: leave it out so
        # events reach the active USB keyboard and tablet handlers.
        self.call("input-send-event", events=events)
        if self.log:
            print(json.dumps({"t": time.monotonic(), "events": events}), flush=True)

    def key(self, name, down):
        name = ALIASES.get(name.lower(), name.lower())
        self.events(
            [
                {
                    "type": "key",
                    "data": {"down": down, "key": {"type": "qcode", "data": name}},
                }
            ]
        )

    def chord(self, names, gap=0.02):
        for name in names:
            self.key(name, True)
            time.sleep(gap)
        for name in reversed(names):
            self.key(name, False)
            time.sleep(gap)

    def type(self, text):
        for char in text:
            if char.isalpha() and char.isupper():
                self.chord(["shift", char.lower()])
            elif char in SHIFTED:
                self.chord(["shift", SHIFTED[char]])
            elif char in PLAIN:
                self.chord([PLAIN[char]])
            else:
                self.chord([char.lower()])
            time.sleep(0.03)

    def move(self, x, y):
        width, height = self.size
        ax = max(0, min(0x7FFF, round(float(x) * 0x7FFF / (width - 1))))
        ay = max(0, min(0x7FFF, round(float(y) * 0x7FFF / (height - 1))))
        self.events(
            [
                {"type": "abs", "data": {"axis": "x", "value": ax}},
                {"type": "abs", "data": {"axis": "y", "value": ay}},
            ]
        )

    def button(self, name, down):
        self.events([{"type": "btn", "data": {"down": down, "button": name}}])


def main(argv):
    args = argv[1:]
    size, log, path = (1920, 1080), False, None
    while args and (args[0].startswith("--") or path is None):
        flag = args.pop(0)
        if flag == "--size":
            size = tuple(int(v) for v in args.pop(0).split("x"))
        elif flag == "--log":
            log = True
        else:
            path = flag
    qmp = Qmp(path, size, log)
    while args:
        command = args.pop(0)
        if command == "key":
            names = []
            while args and args[0] not in COMMANDS:
                names.append(args.pop(0))
            qmp.chord(names)
        elif command in ("down", "up"):
            qmp.key(args.pop(0), command == "down")
        elif command == "type":
            qmp.type(args.pop(0))
        elif command == "move":
            qmp.move(args.pop(0), args.pop(0))
        elif command in ("press", "release", "click"):
            name = (
                args.pop(0)
                if args and args[0] in ("left", "right", "middle")
                else "left"
            )
            if command in ("press", "click"):
                qmp.button(name, True)
            if command == "click":
                time.sleep(0.05)
            if command in ("release", "click"):
                qmp.button(name, False)
        elif command == "path":
            path, step = args.pop(0), float(args.pop(0)) / 1000
            with open(path) as points:
                for line in points:
                    if line.strip():
                        qmp.move(*line.split()[:2])
                        time.sleep(step)
        elif command == "line":
            x0, y0, x1, y1 = (float(args.pop(0)) for _ in range(4))
            count, step = int(args.pop(0)), float(args.pop(0)) / 1000
            for i in range(count + 1):
                qmp.move(x0 + (x1 - x0) * i / count, y0 + (y1 - y0) * i / count)
                time.sleep(step)
        elif command == "sleep":
            time.sleep(float(args.pop(0)) / 1000)
        elif command == "screendump":
            target = args.pop(0)
            arguments = {"filename": target}
            if target.endswith(".png"):
                arguments["format"] = "png"
            if args and args[0].isdigit():
                arguments["device"] = "gpu0"
                arguments["head"] = int(args.pop(0))
            qmp.call("screendump", **arguments)
        elif command == "raw":
            request = json.loads(args.pop(0))
            print(
                json.dumps(qmp.call(request["execute"], **request.get("arguments", {})))
            )
        else:
            raise SystemExit(f"unknown command {command}")


COMMANDS = {
    "key",
    "down",
    "up",
    "type",
    "move",
    "press",
    "release",
    "click",
    "path",
    "line",
    "sleep",
    "screendump",
    "raw",
}

if __name__ == "__main__":
    main(sys.argv)
