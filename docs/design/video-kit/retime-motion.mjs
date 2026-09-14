/** Apply the Motion collection to the existing native Figma tracks.
 * Load figma-use and figma-use-motion; run the exported function through use_figma.
 * Dry-run by default. Existing keyframe identities, values and easing are retained.
 */
export async function retimeMluvaMotion(figma, { dryRun = true } = {}) {
  const page = figma.root.children.find((n) => n.name === "Motion");
  if (!page) throw new Error("Missing Motion page");
  await figma.setCurrentPageAsync(page);
  const variables = await figma.variables.getLocalVariablesAsync();
  const seconds = (name) => {
    const v = variables.find((n) => n.name === "motion/" + name + "-seconds");
    const value = v && Object.values(v.valuesByMode)[0];
    if (typeof value !== "number" || !Number.isFinite(value) || value <= 0) {
      throw new Error("Expected positive seconds: " + name);
    }
    return Math.round(value * 1e6) / 1e6;
  };
  const t = Object.fromEntries(["remove", "reflow", "insert", "accent-decay", "overflow",
    "window-move", "theme", "recording-cycle", "background-cycle", "brand-mark", "brand-type",
    "word-cadence"].map((name) => [name, seconds(name)]));
  if (t.theme >= 2 || t.overflow >= 9 * t["word-cadence"] || t["word-cadence"] < .02) {
    throw new Error("Transitions exceed the study phase slots; adjust the editorial pacing first");
  }
  const plans = [];
  const durations = new Map();
  const root = (name) => {
    const matches = page.children.filter((n) => n.name === name);
    if (matches.length !== 1) throw new Error("Expected one study: " + name);
    return matches[0];
  };
  const child = (owner, name) => {
    const matches = owner.findAll((n) => n.name === name);
    if (matches.length !== 1) throw new Error("Expected one layer: " + name);
    return matches[0];
  };
  const plan = (owner, name, field, times, values) => {
    const node = child(owner, name);
    const track = node.manualKeyframeTracks[field];
    if (!track || track.keyframes.length !== times.length) {
      throw new Error("Changed track structure: " + name + " / " + field);
    }
    times = times.map((n) => Math.round(n * 1e6) / 1e6);
    if (times.some((n, i) => !Number.isFinite(n) || n < 0 || (i && n < times[i - 1]))) {
      throw new Error("Invalid timing order: " + name);
    }
    const keyframes = track.keyframes.map((k, i) => ({ ...k, timelinePosition: times[i],
      ...(values ? { value: { ...k.value, value: values[i] } } : {}) }));
    const changed = track.keyframes.some((k, i) => Math.abs(k.timelinePosition - times[i]) > 1e-5 ||
      (values && Math.abs(k.value.value - values[i]) > 1e-4));
    plans.push({ node, field, track: { ...track, keyframes }, changed });
    durations.set(owner, Math.max(durations.get(owner) || 0, ...times));
  };
  const reveal = (owner, name, start) => plan(owner, name, "OPACITY", [0, start, start + t.insert]);
  const recorder = root("Recorder / Empty to overflow");
  const words = recorder.findAll((n) => /^Word [0-9]+$/.test(n.name)).sort((a, b) => a.name.localeCompare(b.name));
  if (words.length !== 66) throw new Error("Review the recorder choreography after changing its sample text");
  words.forEach((n, i) => reveal(recorder, n.name, .6 + i * t["word-cadence"]));
  const rowStarts = words.map((n, i) => ({ n, i })).filter(({ n }, i) => !i || n.y !== words[i - 1].y);
  if (rowStarts.length !== 7) throw new Error("Expected seven wrapped rows in the recorder study");
  const first = .6 + rowStarts[5].i * t["word-cadence"] - .04;
  const second = .6 + rowStarts[6].i * t["word-cadence"] - .04;
  const line = rowStarts[1].n.y - rowStarts[0].n.y;
  plan(recorder, "Transcript / forward only", "TRANSLATION_Y",
    [0, first - t.overflow, first, second - t.overflow, second], [0, 0, -line, -line, -2 * line]);
  const pulse = Array.from({ length: 5 }, (_, i) => i * t["recording-cycle"] / 2);
  for (const field of ["ROTATION", "SCALE_X", "SCALE_Y"]) {
    plan(recorder, "Fluid light / expanded breath", field, pulse);
  }
  const patch = root("Rewrite / Changed words");
  const removalEnd = 1 + t.remove;
  const insertionStart = removalEnd + t.reflow;
  const insertionEnd = insertionStart + t.insert;
  plan(patch, "Removed / Tuesday", "OPACITY", [0, 1, removalEnd]);
  reveal(patch, "Inserted / Wednesday", insertionStart);
  plan(patch, "Stable punctuation", "TRANSLATION_X", [0, removalEnd, insertionStart]);
  plan(patch, "Change underline", "OPACITY", [0, insertionStart, insertionEnd, insertionEnd + t["accent-decay"]]);
  const grill = root("Grilling / Answer and advance");
  reveal(grill, "Answer arrives", 1);
  plan(grill, "Answered question", "OPACITY", [0, 2, 2 + t.remove]);
  const questionStart = 2 + t.remove + t.reflow;
  reveal(grill, "Next question", questionStart);
  plan(grill, "Next question", "TRANSLATION_Y", [0, questionStart, questionStart + t.insert]);
  reveal(grill, "New requirement", questionStart + t.insert + .28);
  plan(root("Desktop / Float to tile"), "Runtime / Tiled", "OPACITY", [0, 2, 2 + t["window-move"]]);
  const water = root("Background / Water loop");
  for (const field of ["SCALE_X", "SCALE_Y"]) {
    plan(water, "Background / Water", field, [0, t["background-cycle"] / 2, t["background-cycle"]]);
  }
  const brand = root("Brand / Title reveal");
  for (const field of ["OPACITY", "SCALE_X", "SCALE_Y"]) plan(brand, "Mark", field, [0, t["brand-mark"]]);
  for (const field of ["OPACITY", "TRANSLATION_Y"]) {
    plan(brand, "Wordmark / approved baseline", field, [0, .3, .3 + t["brand-type"]]);
  }
  plan(brand, "Descriptor", "OPACITY", [0, .8, .8 + t["brand-type"]]);
  const theme = root("Theme / Palette change");
  for (const [name, start] of [["Tokyo Night", 2], ["Rosé Pine", 4]]) {
    plan(theme, "Theme reveal / " + name, "WIDTH", [0, start, start + t.theme]);
    plan(theme, "Theme reveal / " + name, "OPACITY", [0, start]);
  }
  const breath = root("Breathing light / Blender study");
  for (const frame of [1, 26, 52, 78]) {
    plan(breath, "Blender / Frame " + frame, "OPACITY",
      Array.from({ length: 9 }, (_, i) => i * t["recording-cycle"] / 4));
  }
  // Validate every required track before changing any of them.
  const changed = plans.filter((p) => p.changed);
  const extended = [...durations].filter(([owner, duration]) => owner.timelines[0].duration < duration - 1e-5);
  if (!dryRun) {
    for (const p of changed) {
      p.node.manualKeyframeTracks = { ...p.node.manualKeyframeTracks, [p.field]: p.track };
    }
    for (const [owner, duration] of extended) owner.setTimelineDuration(owner.timelines[0].id, duration);
  }
  return { dryRun, inspectedTracks: plans.length, changedTracks: changed.length,
    changes: changed.map((p) => ({ nodeId: p.node.id, name: p.node.name, field: p.field })),
    timelineExtensions: extended.map(([owner, duration]) => ({ nodeId: owner.id, duration })),
    createdNodeIds: [], mutatedNodeIds: dryRun ? [] : [...new Set([...changed.map((p) => p.node.id), ...extended.map(([n]) => n.id)])] };
}
