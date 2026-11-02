#!/usr/bin/env python3
"""Write the takes from Daniel's word timing, so every key press is filmed on the word that names it.

    make_takes.py EDL.json OUT_DIR

`w("F9", 2)` is the film time of the second "F9" he says. Offsets are the measured lag between a key press and its first
visible frame, plus any calibration from the previous take (see calibrate.py), so the first visible change lands on the word.
"""
import json, math, pathlib, sys

edl = json.load(open(sys.argv[1]))
out = pathlib.Path(sys.argv[2]); out.mkdir(parents=True, exist_ok=True)
words = edl["words"]


def w(text, nth=1, key="start"):
    hits = [x for x in words if x["text"].strip(",.?!-").lower() == text.lower() and x["start"] is not None]
    return hits[nth - 1][key]


def after(text, t, key="start"):
    """The first time he says `text` at or after film time `t`."""
    hits = [x for x in words if x["text"].strip(",.?!-").lower() == text.lower() and x["start"] is not None and x["start"] >= t]
    return hits[0][key]


def grow(sentence_words, press):
    """Live partials at the spoken time of each word: [(film time of the word's end, words so far)]."""
    parts = []
    text = ""
    for word, at in sentence_words:
        text = (text + " " + word).strip()
        parts.append([round(max(0.02, at - press), 3), text])
    return parts


def focus_ws(n):
    return f'hyprctl dispatch "hl.dsp.focus({{ workspace = {n} }})"; sleep 0.9'


FF = 'hyprctl dispatch "hl.dsp.focus({ window = \\"class:firefox\\" })"; sleep 0.3'
RESET_NOTE = "wtype -M ctrl a -m ctrl; wtype -k BackSpace"

# ---- A+B: F9, then F12, one continuous run from the same desktop ----
f9a = w("F9", 1) - 0.05
f9b = w("F9", 2) - 0.03
poof = w("poof")
talk = w("talk")
said = [("You", talk - 0.02), ("talk,", w("talk", 1, "end")), ("and", after("and", talk, "end")), ("then", after("then", talk, "end")), ("you", after("you", talk, "end")),
        ("press", after("press", talk, "end")), ("F9", w("F9", 2, "end") - 0.08)]
ws2 = w("they") + 0.18  # the slide settles on "agents"
f12a = w("F12", 1) - 0.05
f12b = w("record") - 0.05
f12c = w("F12", 2) - 0.03
sent = w("sent")
ask = "Add a changelog entry for the routing release, then run the tests."
ask_words = ask.split()
span = f12c - f12b - 0.1
ask_grow = [(word, f12b + 0.18 + (i + 1) * span / len(ask_words)) for i, word in enumerate(ask_words)]
take_ab = {
    "name": "ab-f9-f12", "start": w("press", 1) - 1.0, "end": sent + 2.6, "lead": 0.6,
    "before": [focus_ws(1), FF, RESET_NOTE, "sleep 0.3"],
    "qmp_before": [{"type": "move", "x": 1150, "y": 690}],
    "script": {"sessions": [
        {"partials": grow(said, f9a + 0.1), "final": "You talk, and then you press F9 once you're done.", "final_delay": round(poof - f9b - 0.56, 3)},
        {"partials": grow(ask_grow, f12b + 0.1), "final": ask, "final_delay": 1.8},
    ]},
    "events": [
        {"t": f9a, "type": "key", "keys": ["f9"], "cue": "F9 start, word 'F9'"},
        {"t": f9b, "type": "key", "keys": ["f9"], "cue": "F9 stop, second 'F9'"},
        {"t": ws2, "type": "key", "keys": ["super", "2"], "cue": "desktop 2, 'they talk to their agents'"},
        {"t": f12a, "type": "key", "keys": ["f12"], "cue": "dial opens, word 'F12'"},
        {"t": w("Claude"), "type": "key", "keys": ["a"], "cue": "hand on a Claude session, word 'Claude'"},
        {"t": w("Codex"), "type": "key", "keys": ["f"], "cue": "hand on a Codex session, word 'Codex'"},
        {"t": f12b, "type": "key", "keys": ["f12"], "cue": "record, word 'record'"},
        {"t": f12c, "type": "key", "keys": ["f12"], "cue": "stop, second 'F12'"},
    ],
}

# ---- C: Shift. Recording is already running when the shot starts ----
shift_t = w("Shift")
hi = w("highlight")
know = w("know", 1, "end")
stop_c = w("screenshot", 2) - 0.25
auto = w("automatically", 2)
dont = w("don't", 2)
you_c = dont - 0.1
fix = "Move this Save button inside the card, and stop the sync path running past the edge."
fix_words = fix.split()
press_c = you_c - 0.4 - 0.2 - 0.9  # the prelude's last F12: take start, less the lead and its hold
fix_grow = [(word, press_c + 0.5 + (i + 1) * (stop_c - press_c - 0.7) / len(fix_words)) for i, word in enumerate(fix_words)]
# Save button and sync row in the guest's logical pixels (workspace 3, Firefox on the left).
cx, cy, rx, ry = 516, 432, 86, 46
circle = [(cx + rx * math.cos(math.radians(a)), cy + ry * math.sin(math.radians(a))) for a in range(-130, 275, 15)]
under = [(300, 378), (380, 380), (460, 378), (540, 380), (612, 379)]
what = after("what", hi)
take_c = {
    "name": "c-shift", "start": you_c - 0.4, "end": auto + 2.6, "lead": 0.2,
    "before": [focus_ws(3)],
    "qmp_before": [{"type": "move", "x": 620, "y": 180}],
    "prelude": [{"after": 0.0, "type": "key", "keys": ["f12"]}, {"after": 1.1, "type": "key", "keys": ["d"]}, {"after": 1.9, "type": "key", "keys": ["f12"]}],
    "script": {"sessions": [{"partials": grow(fix_grow, press_c + 0.1), "final": fix, "final_delay": 1.2}]},
    "events": [
        {"t": after("just", you_c) - 0.1, "type": "path", "points": [(620, 180), (570, 290), (cx + rx * math.cos(math.radians(-130)), cy + ry * math.sin(math.radians(-130)))], "duration": 0.5, "cue": "pointer towards Save"},
        {"t": shift_t - 0.03, "type": "down", "key": "shift", "cue": "hold Shift, word 'Shift'"},
        {"t": hi - 0.02, "type": "path", "points": circle, "duration": what + 0.42 - hi, "cue": "circle Save, from word 'highlight'"},
        {"t": what + 0.44, "type": "up", "key": "shift", "cue": "lift between marks"},
        {"t": what + 0.46, "type": "path", "points": [circle[-1], (380, 400), under[0]], "duration": 0.2, "cue": "to the sync path, lifted"},
        {"t": what + 0.70, "type": "down", "key": "shift", "cue": "hold again"},
        {"t": what + 0.74, "type": "path", "points": under, "duration": know - what - 0.74, "cue": "underline, to 'know'"},
        {"t": know + 0.02, "type": "up", "key": "shift", "cue": "release Shift, end of 'know'"},
        {"t": stop_c, "type": "key", "keys": ["f12"], "cue": "stop, before 'screenshot'"},
    ],
}

# ---- E: tiles, moves with you, themes, water ----
built = w("I've")
press_e = built - 0.4 - 0.2 - 0.9  # the prelude's F9
choose = w("choose")
you_f = choose - 0.26
said_e = [(x["text"], x["end"]) for x in words if x["start"] is not None and x["start"] >= w("I've") - 0.01 and x["start"] <= w("water", 1)]
take_e = {
    "name": "e-beautiful", "start": w("I've") - 0.4, "end": you_f + 0.1, "lead": 0.2,
    "before": [focus_ws(1), FF, "omarchy-theme-set tokyo-night; sleep 2.2"],
    "qmp_before": [{"type": "move", "x": 1200, "y": 60}],
    "prelude": [{"after": 0.0, "type": "key", "keys": ["f9"]}],
    "script": {"sessions": [{"partials": grow(said_e, press_e + 0.1), "final": " ".join(x[0] for x in said_e), "final_delay": 0.1}]},
    "events": [
        {"t": w("tiles") - 0.06, "type": "key", "keys": ["super", "t"], "cue": "tile, word 'tiles'"},
        {"t": w("moves") - 0.2, "type": "key", "keys": ["super", "t"], "cue": "float again, before 'moves'"},
        {"t": w("moves") - 0.03, "type": "key", "keys": ["super", "2"], "cue": "desktop 2, word 'moves'"},
        {"t": w("themes") - 2.45, "type": "guest", "command": "omarchy-theme-set osaka-jade", "cue": "theme script takes about 2.5 s, so its swap lands on 'themes'"},
        {"t": w("water") - 0.35, "type": "guest", "command": "true", "cue": "water"},
    ],
    "after": ["omarchy-theme-set tokyo-night; sleep 1.5", "gdbus call --session --dest com.mluva.Linux --object-path /com/mluva/Linux --method org.gtk.Actions.Activate cancel '[]' '{}' >/dev/null; true"],
}

# ---- F: local or cloud, the real Providers tab ----
take_f = {
    "name": "f-models", "start": you_f - 0.5, "end": w("model", 1, "end") + 1.0, "lead": 0.2,
    "before": ["systemctl --user stop mluva; sleep 1; ~/start.sh provider; sleep 1.5", focus_ws(9), "gdbus call --session --dest com.mluva.Linux --object-path /com/mluva/Linux --method org.gtk.Actions.Activate settings '[]' '{}' >/dev/null; sleep 0.8"],
    "qmp_before": [{"type": "move", "x": 792, "y": 171}, {"type": "sleep", "seconds": 0.2}, {"type": "click"}, {"type": "sleep", "seconds": 0.5}, {"type": "move", "x": 441, "y": 404}, {"type": "sleep", "seconds": 0.2}, {"type": "click"}, {"type": "sleep", "seconds": 0.5}, {"type": "move", "x": 640, "y": 500}],
    "events": [
        {"t": w("between") - 0.1, "type": "path", "points": [(640, 500), (760, 450), (837, 404)], "duration": w("local") - w("between") + 0.06, "cue": "towards Local model"},
        {"t": w("local") - 0.02, "type": "click", "cue": "Local model, word 'local'"},
        {"t": w("or", 2) - 0.02, "type": "path", "points": [(837, 404), (705, 408), (573, 404), (441, 408), (309, 404)], "duration": w("cloud") - w("or", 2) + 0.16, "cue": "across Mistral, Grok, ElevenLabs to MAI"},
        {"t": w("cloud") + 0.16, "type": "click", "cue": "a cloud provider, word 'cloud'"},
    ],
    "after": ["systemctl --user stop mluva; sleep 0.5; ~/start.sh mluva"],
}

for take in (take_ab, take_c, take_e, take_f):
    json.dump(take, open(out / f"{take['name']}.json", "w"), indent=1)
    print(take["name"], round(take["start"], 2), round(take["end"], 2), [round(e["t"], 2) for e in take["events"]])
