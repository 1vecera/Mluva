#!/usr/bin/env python3
"""Write the film's edit plan from Daniel's word map: captions, key tokens, camera, presenter moves and the take anchors.

    edit_plan.py EDL.json OUT_PLAN.json OUT_ANCHORS.json

Anchors pair a word's film time with the first frame where the app visibly does that thing. The frames were found with
measure.py, in capture seconds. Between anchors a take plays at real speed; rest frames are held or trimmed to meet the next one.
"""
import json, re, sys

edl = json.load(open(sys.argv[1]))
words = [w for w in edl["words"] if w["start"] is not None]
DURATION = edl["duration"]


def clean(text):
    return text.strip(",.?!-").lower()


def at(text, nth=1, key="start"):
    return [w for w in words if clean(w["text"]) == text.lower()][nth - 1][key]


def after(text, t, key="start"):
    return [w for w in words if clean(w["text"]) == text.lower() and w["start"] >= t][0][key]


# Captions: ** marks a highlighted word. Their words consume the kept words of the take in order.
PHRASES = """Hello, everyone.
I'm **Daniel,**
and I'd like to introduce to you
the **all-new** **Mluva.**
It's **simple.**
You press **F9,** you **talk,**
and then you press **F9**
once you're done,
and **poof,**
the words appear
in your **app.**
But what changed
is that most people
don't talk to their apps,
they talk to their **agents.**
Thus, you press **F12,**
you pick a **Claude** or **Codex**
session you want to talk to,
you **record** your text,
and once you're done,
you press **F12** again,
and the text gets
**automatically** routed
to the **correct** session
and **sent.**
You don't need to **screenshot**
things anymore.
You just hold **Shift,**
**highlight** what you want
your agents to know,
and the **screenshot**
will get included
on the transcription
**automatically.**
In my previous version,
there was a bunch of other features
like live rewrite,
polishes,
AI cleanups and stuff.
I **dropped** all that
to keep it **simple**
because that's the job
of your **agent.**
I've built Mluva
to be **beautiful.**
It **tiles,**
it **moves** with you,
it adjusts to your **themes.**
It's like **water.**
You can choose between
a **local** or a **cloud** model,
I built it
to just give you
a little bit more
of **delight** every day,
you know?
It's **free,**
it's **simple,**
it's **beautiful.**
So **try** it right now.
**Cheers.**""".split("\n")

phrases, i = [], 0
for line in PHRASES:
    tokens = line.split()
    out = []
    for token in tokens:
        emphasis = token.startswith("**")
        text = token.strip("*")
        w = words[i]
        # "AI cleanups and stuff" and "agents" keep Daniel's wording from his transcript.
        assert clean(w["text"]).replace("all-new", "all-new")[:3] == clean(text)[:3] or clean(text) == "agents", (w["text"], text)
        out.append({"text": text, "start": w["start"], "end": w["end"], "emphasis": emphasis})
        i += 1
    phrases.append({"start": out[0]["start"] - 0.02, "end": out[-1]["end"] + 0.12, "words": out})
assert i == len(words), (i, len(words))
for a, b in zip(phrases, phrases[1:]):
    a["end"] = min(a["end"] + 0.2, b["start"])
phrases[-1]["end"] = min(DURATION - 0.3, phrases[-1]["end"] + 0.4)

f12 = at("f12", 1)
keys = [
    dict(start=at("f9", 1), end=at("and", 2) + 0.05, key="F9", label="Talk"),
    dict(start=at("f9", 2), end=at("poof") + 0.25, key="F9", label="Done"),
    dict(start=f12, end=at("pick") + 0.1, key="F12", label="Your sessions"),
    dict(start=at("record"), end=at("and", 4) + 0.1, key="F12", label="Talk"),
    dict(start=at("f12", 2), end=at("f12", 2) + 1.3, key="F12", label="Send"),
    dict(start=at("shift"), end=at("know", 1, "end") + 0.1, key="Shift", label="Hold and draw"),
    dict(start=at("screenshot", 2) - 0.2, end=at("screenshot", 2) + 1.1, key="F12", label="Send"),
    dict(start=at("tiles"), end=at("moves") - 0.03, key="Super + T", label="Tile"),
    dict(start=at("moves"), end=at("adjusts") - 0.05, key="Super + 2", label="Next desktop"),
    dict(start=at("adjusts"), end=at("themes", 1, "end") + 0.1, key="Theme", label="Osaka Jade"),
]

CORNER = dict(x=1500, bottom=0, height=430)
LARGE = dict(x=1010, bottom=0, height=860)
presenter = [
    dict(t=0, **LARGE),
    dict(t=at("simple", 1) + 0.28, **CORNER),
    dict(t=at("built", 2) + 0.2, **LARGE),
]
camera = [
    dict(t=0, scale=1.0, x=50, y=50),
    dict(t=at("press", 2), scale=1.0, x=50, y=50),
    dict(t=at("the", 2) + 0.2, scale=1.3, x=14, y=42),  # the words in the note, on "the words appear"
    dict(t=at("they") + 0.2, scale=1.0, x=50, y=50),
    dict(t=at("the", 4) + 0.1, scale=1.34, x=92, y=12),  # Codex's pane as the text is routed there
    dict(t=at("sent") + 0.75, scale=1.0, x=50, y=50),
    dict(t=at("hold") + 0.25, scale=1.28, x=18, y=62),  # the Save button and the ink
    dict(t=at("screenshot", 2) + 0.05, scale=1.0, x=50, y=50),
    dict(t=at("automatically", 2) + 0.3, scale=1.25, x=92, y=12),  # Claude's pane with the screenshot
    dict(t=at("in", 2) - 0.1, scale=1.0, x=50, y=50),
    dict(t=at("choose") - 0.05, scale=1.0, x=50, y=50),
    dict(t=at("local") + 0.05, scale=1.34, x=56, y=60),  # the provider row
    dict(t=at("built", 2) + 0.2, scale=1.0, x=50, y=50),
]
plan = {
    "duration": DURATION,
    "poppy_captions": phrases,
    "keys": keys,
    "camera": camera,
    "presenter": presenter,
    "name": {"start": at("i'm") , "end": at("mluva", 1, "start") - 0.1, "text": "Daniel Vecera"},
    "logo": {"start": at("all-new") + 0.1, "end": at("simple", 1) + 0.25},
    "dropped": {
        "items": [
            {"text": "Live rewrite", "start": at("live")},
            {"text": "Polishes", "start": at("polishes")},
            {"text": "AI cleanups", "start": at("ai")},
        ],
        "strike": at("dropped"),
        "end": at("simple", 2) + 0.2,
        "dim_start": at("in", 2) - 0.2,
        "dim_end": at("agent") + 0.9,
    },
    "still": {"start": at("automatically", 2) + 0.05, "end": at("in", 2) + 0.1, "src": "film/keyframe.png", "x": 60, "y": 170, "width": 800},
    "closing": {
        "start": at("built", 2) + 0.2,
        "words": [{"text": "Free.", "start": at("free")}, {"text": "Simple.", "start": at("simple", 3)}, {"text": "Beautiful.", "start": at("beautiful", 2)}],
        "url_start": at("try"),
        "fade_start": DURATION - 0.55,
    },
}
json.dump(plan, open(sys.argv[2], "w"), indent=1)

# (film seconds, capture seconds) per take; `until` is where the next shot's crossfade ends.
shots = [
    dict(take="ab", start=0.0, until=at("you", 9) if False else 29.15, anchors=[
        (0.0, 0.0), (at("f9", 1), 3.150), (at("f9", 2), 5.200), (at("poof"), 6.433), (14.76, 12.10), (f12, 14.417),
        (at("claude"), 15.45), (at("codex"), 15.85), (at("record"), 18.117), (at("f12", 2), 20.317), (at("sent"), 24.517)]),
    dict(take="c", start=29.05, until=49.75, anchors=[
        (29.05, 29.05 - 24.14), (at("highlight"), 8.40), (at("screenshot", 2) - 0.25, 11.78), (at("screenshot", 2) + 1.05, 13.91),
        (at("automatically", 2), 16.45)]),
    dict(take="e", start=49.6, until=56.3, anchors=[
        (49.6, 49.6 - 47.12), (at("tiles"), 4.767), (at("moves"), 5.300), (at("themes") - 0.2, 6.35)]),
    dict(take="f", start=56.15, until=59.45, anchors=[(56.15, 56.15 - 54.22), (at("local"), 3.267), (58.47, 4.067)]),
    dict(take="empty", start=59.2, until=DURATION, anchors=[(59.2, 0.0)]),
]
json.dump({"shots": shots}, open(sys.argv[3], "w"), indent=1)
print("phrases", len(phrases), "keys", len(keys), "words", len(words))
for k in keys: print(k)
