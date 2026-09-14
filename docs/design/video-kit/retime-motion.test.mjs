import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { retimeMluvaMotion } from "./retime-motion.mjs";

function fixture() {
  const snapshot = JSON.parse(readFileSync(new URL("./figma-snapshot.json", import.meta.url)));
  const native = snapshot.motion.find((m) => m.id === "155:16");
  const variables = snapshot.collections.flatMap((c) => c.variables);
  const cycle = variables.find((v) => v.name === "motion/recording-cycle-seconds");
  const owner = {
    id: native.id, name: native.name,
    timelines: [{ id: "loop", duration: native.duration }],
    children: native.animated.map((n) => ({
      id: n.id, name: n.name,
      manualKeyframeTracks: Object.fromEntries(Object.entries(n.tracks).map(([field, track]) => [field, {
        keyframes: track.times.map((time, i) => ({
          id: n.id + "/" + field + "/" + i, timelinePosition: time,
          value: { value: track.values[i] }, easing: { type: track.easing[i] },
        })),
      }])),
    })),
    findAll(predicate) { return this.children.filter(predicate); },
    setTimelineDuration(id, duration) {
      assert.equal(id, "loop");
      this.timelines[0].duration = duration;
    },
  };
  const page = { name: "Breathing Motion", children: [owner] };
  const figma = {
    root: { children: [page] },
    async setCurrentPageAsync(selected) { assert.equal(selected, page); },
    variables: { async getLocalVariablesAsync() { return variables; } },
  };
  return {
    owner,
    setPeriod(seconds) { cycle.valuesByMode[Object.keys(cycle.valuesByMode)[0]] = seconds; },
    run(dryRun = false) { return retimeMluvaMotion(figma, { pageName: page.name, dryRun }); },
    capture() { return JSON.stringify({ children: owner.children, timelines: owner.timelines }); },
  };
}

function assertUninterruptedLoop(owner) {
  const tracks = owner.children.map((n) => n.manualKeyframeTracks.OPACITY.keyframes);
  const boundaries = [...new Set(tracks.flatMap((keys) => keys.map((k) => k.timelinePosition)))].sort((a, b) => a - b);
  assert.equal(boundaries.at(-1), owner.timelines[0].duration, "No frozen tail after the last pose");
  const probes = [...boundaries, ...boundaries.slice(1).map((time, i) => (time + boundaries[i]) / 2)];
  for (const time of probes) {
    const visible = tracks.map((keys) => keys.findLast((k) => k.timelinePosition <= time).value.value);
    assert.equal(visible.filter((value) => value === 1).length, 1, "Exactly one opaque pose at " + time);
    assert.ok(visible.every((value) => value === 0 || value === 1));
  }
  assert.ok(tracks.every((keys) => keys.every((k) => k.easing.type === "HOLD")));
}

test("increasing, decreasing and restoring the period restores the complete loop", async () => {
  const f = fixture(), original = f.capture();
  for (const period of [4.2, 2, 3.4]) {
    f.setPeriod(period);
    const before = f.capture(), preview = await f.run(true);
    assert.equal(f.capture(), before, "Dry run must not mutate tracks or duration");
    assert.equal(preview.timelineAdjustments[0].duration, 2 * period);
    await f.run();
    assert.equal(f.owner.timelines[0].duration, 2 * period);
    assertUninterruptedLoop(f.owner);
    const repeat = await f.run(true);
    assert.equal(repeat.changedTracks, 0);
    assert.deepEqual(repeat.timelineAdjustments, []);
  }
  assert.equal(f.capture(), original, "Restoration includes keyframe identities, values, easing and duration");
});

for (const extra of ["manual track", "child style", "root style"]) {
  test("shortening refuses an unmanaged " + extra + " before any mutation", async () => {
    const f = fixture();
    f.setPeriod(2);
    if (extra === "manual track") {
      f.owner.children[0].manualKeyframeTracks.TRANSLATION_X = {
        keyframes: [{ timelinePosition: 6, value: { value: 10 } }],
      };
    } else (extra === "root style" ? f.owner : f.owner.children[0]).animationStyles = [{ id: "unmanaged" }];
    const before = f.capture();
    await assert.rejects(f.run(), /Cannot shorten a loop with unmanaged animation/);
    assert.equal(f.capture(), before);
  });
}
