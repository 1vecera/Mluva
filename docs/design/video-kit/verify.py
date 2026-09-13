"""Verify every source token and asset against the captured native Figma read-back."""

import json
import math
from itertools import pairwise
from pathlib import Path

BASE = Path(__file__).resolve().parent


def read(name):
    """Load an adjacent artifact without account access."""
    return json.loads((BASE / name).read_text())


def require(condition, message):
    """Fail the handoff check with the mismatched contract."""
    if not condition:
        raise ValueError(message)


def close(actual, expected, name):
    """Allow Figma floating-point round trips without hiding material changes."""
    if isinstance(expected, (int, float)) and not isinstance(expected, bool):
        require(
            math.isclose(actual, expected, abs_tol=1e-5),
            f"{name}: {actual} != {expected}",
        )
    else:
        require(actual == expected, f"{name}: {actual!r} != {expected!r}")


def main():
    """Check aliases, values, units, inventory, shot continuity, motion and native stress evidence."""
    source, manifest, snapshot = (
        read(n) for n in ("tokens.json", "asset-manifest.json", "figma-snapshot.json")
    )
    collections = {c["collection"]["name"]: c for c in snapshot["collections"]}
    variables = {v["name"]: v for c in collections.values() for v in c["variables"]}
    by_id = {v["id"]: v for v in variables.values()}
    groups = ("primitives", "theme", "layout", "typography", "motion", "content")
    require(len(collections) == 6, "Collection count drift")
    require(
        set(variables) == set().union(*(set(source[g]) for g in groups)),
        "Missing or obsolete source tokens",
    )
    require(
        len(variables)
        == manifest["tokens"]["total"]
        == snapshot["counts"]["variables"],
        "Variable count drift",
    )
    for name, spec in source["primitives"].items():
        actual = next(iter(variables[name]["valuesByMode"].values()))
        color = "#" + "".join(f"{round(actual[c] * 255):02x}" for c in ("r", "g", "b"))
        require(
            color == spec["sourceHex"].lower() and len(spec["oklch"]) == 3,
            name + ": palette drift",
        )
        close(actual["a"], 1, name + " alpha")
    modes = {
        m["modeId"]: m["name"] for m in collections["Theme"]["collection"]["modes"]
    }
    aliases = 0
    for name, spec in source["theme"].items():
        actual = variables[name]
        require(
            sorted(actual["scopes"]) == sorted(spec["scopes"]), name + ": scope drift"
        )
        for mode, value in actual["valuesByMode"].items():
            require(value["type"] == "VARIABLE_ALIAS", name + ": not an alias")
            require(
                by_id[value["id"]]["name"] == spec["aliases"][modes[mode]],
                name + ": alias drift",
            )
            aliases += 1
    require(aliases == manifest["tokens"]["semanticAliases"] == 57, "Alias count drift")
    for group in groups[2:]:
        for name, spec in source[group].items():
            actual = variables[name]
            close(
                next(iter(actual["valuesByMode"].values())),
                spec.get("figmaValue", spec["value"]),
                name,
            )
            require(
                sorted(actual["scopes"]) == sorted(spec["scopes"]),
                name + ": scope drift",
            )
            if "figmaValue" in spec:
                close(
                    spec["figmaValue"] / 100,
                    spec["value"],
                    name + " opacity conversion",
                )
    for name in ("font/ui", "font/display"):
        require(
            source["typography"][name]["value"] == "JetBrains Mono",
            "Type contract drift",
        )
    components = {c["nodeId"] for c in manifest["components"]}
    require(
        len(components)
        == manifest["componentCount"]
        == snapshot["counts"]["components"],
        "Component drift",
    )
    require(
        len(manifest["screens"]) == 12 and len(manifest["runtimeCaptures"]) == 6,
        "Screen inventory drift",
    )
    require(
        all(
            c["nodeId"] in components
            for c in manifest["screens"] + manifest["runtimeCaptures"]
        ),
        "Missing master",
    )
    require(
        len(manifest["componentFamilies"]) == snapshot["counts"]["componentSets"] == 8,
        "Variant family drift",
    )
    for family in manifest["componentFamilies"]:
        require(
            all(v["id"] in components for v in family["variants"]), "Missing variant"
        )
    require(
        len(manifest["figma"]["pages"]) == len(snapshot["pages"]) == 12,
        "Page inventory drift",
    )
    require(len(manifest["storyboard"]) == 11, "Shot inventory drift")
    cursor = 0
    for shot in manifest["storyboard"]:
        require(shot["start"] == cursor and shot["end"] > cursor, "Shot gap or overlap")
        cursor = shot["end"]
        require(
            all(shot[f] for f in ("voice", "action", "motion", "frameId", "sectionId")),
            "Incomplete shot",
        )
    require(cursor == manifest["film"]["durationSeconds"] == 59, "Film duration drift")
    require("Tuesday" in manifest["examples"]["polished"], "Polish changes a fact")
    native = {m["id"]: m for m in snapshot["motion"]}
    tracks = 0
    for spec in manifest["motions"]:
        motion = native[spec["nodeId"]]
        close(motion["duration"], spec["durationSeconds"], spec["name"])
        count = sum(len(n["tracks"]) for n in motion["animated"])
        require(count == spec["tracks"], "Track inventory drift")
        tracks += count
        for node in motion["animated"]:
            for track in node["tracks"].values():
                times = track["times"]
                require(
                    times == sorted(times) and len(times) == len(track["values"]),
                    "Invalid keyframe order",
                )
                require(
                    0 <= times[0] <= times[-1] <= motion["duration"] + 1e-5,
                    "Keyframe outside timeline",
                )
    require(len(native) == 8 and tracks == 96, "Motion inventory drift")
    scroll = next(
        n
        for n in native["30:27"]["animated"]
        if n["name"] == "Transcript / forward only"
    )
    values = scroll["tracks"]["TRANSLATION_Y"]["values"]
    require(all(b <= a for a, b in pairwise(values)), "Recorder scroll reverses")
    require(
        sum(n["name"].startswith("Word ") for n in native["30:27"]["animated"]) == 66,
        "Word flow drift",
    )
    answer = next(n for n in native["34:11"]["animated"] if n["id"] == "34:17")
    require(
        answer["tracks"]["OPACITY"]["values"][-1] == 1, "Grilling erases the answer"
    )
    tests = snapshot["tests"]
    for key, count in (("freshInstances", 11), ("themeAliases", 57)):
        require(
            len(tests[key]) == count and all(c["pass"] for c in tests[key]),
            key + ": propagation failed",
        )
    require(
        not tests["missingFonts"] and not tests["fontViolations"],
        "Missing or inconsistent fonts",
    )
    require(not tests["overlaps"], "Canvas/section asset overlap")
    require(
        all(n["id"] in tests["intentionalOverflow"] for n in tests["textOverruns"]),
        "Unexplained clipping",
    )
    require(
        tests["retiming"]["changedTracks"] == 0
        and tests["retiming"]["inspectedTracks"] == tracks,
        "Timing drift",
    )
    require(
        tests["retimingStress"]["changedTracks"]
        == tests["retimingRestored"]["changedTracks"]
        == 90,
        "Stress drift",
    )
    require(
        all(not p["changes"] for p in tests["canvas"]),
        "Canvas refresh is not idempotent",
    )
    print(
        f"PASS: {len(variables)} variables / 57 aliases; {len(components)} components; 12 screens + 6 captures; "
        "11 contiguous shots; 8 timelines / 96 tracks; fonts, layout and native propagation evidence."
    )
    print(
        "This verifies the captured snapshot; it does not fetch the current Figma file."
    )


if __name__ == "__main__":
    main()
