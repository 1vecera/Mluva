/** Arrange one Figma page from canvas/stage variables and shared frame notes.
 * Call once per page (in separate use_figma invocations). Dry-run by default.
 * Export frame contents and native timelines retain their own coordinate systems.
 */
export async function reflowMluvaCanvas(figma, { pageId, dryRun = true } = {}) {
  const page = figma.root.children.find((n) => n.id === pageId || n.name === pageId);
  if (!page) throw new Error("Choose an existing page ID or name");
  await figma.setCurrentPageAsync(page);
  const variables = await figma.variables.getLocalVariablesAsync();
  const value = (name) => {
    const v = variables.find((n) => n.name === name);
    const number = v && Object.values(v.valuesByMode)[0];
    if (typeof number !== "number" || !Number.isFinite(number) || number < 0) throw new Error("Invalid " + name);
    return number;
  };
  const margin = value("canvas/margin"), gap = value("canvas/gap");
  const stagePadding = value("stage/padding"), stageGap = value("stage/gap"), noteGap = value("spacing/xl");
  const changes = [], touched = new Set(), virtual = new Map();
  const fonts = new Map();
  for (const text of page.findAllWithCriteria({ types: ["TEXT"] })) {
    for (const segment of text.getStyledTextSegments(["fontName"])) fonts.set(JSON.stringify(segment.fontName), segment.fontName);
  }
  await Promise.all([...fonts.values()].map((font) => figma.loadFontAsync(font)));
  const size = (n) => virtual.get(n.id) || { width: n.width, height: n.height };
  const resize = (n, width, height) => {
    if (Math.abs(n.width - width) < .01 && Math.abs(n.height - height) < .01) return;
    changes.push({ nodeId: n.id, size: [width, height] }); touched.add(n.id);
    if (!dryRun) n.resizeWithoutConstraints(width, height);
    virtual.set(n.id, { width, height: dryRun ? height : n.height });
  };
  const position = (n, x, y) => {
    if (Math.abs(n.x - x) < .01 && Math.abs(n.y - y) < .01) return;
    changes.push({ nodeId: n.id, position: [x, y] }); touched.add(n.id);
    if (!dryRun) { n.x = x; n.y = y; }
  };
  const notes = (parent) => new Map(parent.children.filter((n) => n.name.startsWith("Frame note / "))
    .map((n) => [n.name.slice("Frame note / ".length), n]));
  const pack = (parent, targets, left, top, spacing) => {
    const byId = notes(parent);
    for (const target of targets) {
      const note = byId.get(target.id);
      if (note) resize(note, size(target).width, note.height);
    }
    const column = Math.max(0, ...targets.map((n) => size(n).width));
    let y = top;
    for (let row = 0; row < targets.length; row += 2) {
      let height = 0;
      for (const [i, target] of targets.slice(row, row + 2).entries()) {
        const x = left + i * (column + spacing), s = size(target), note = byId.get(target.id);
        position(target, x, y);
        if (note) position(note, x, y + s.height + noteGap);
        height = Math.max(height, s.height + (note ? noteGap + size(note).height : 0));
      }
      y += height + spacing;
    }
    return { width: left * 2 + Math.min(2, targets.length) * column + (targets.length > 1 ? spacing : 0),
      height: targets.length ? y - spacing + left : top + left };
  };
  if (page.name === "Recorder") {
    const board = page.children.find((n) => n.name === "Recorder / States");
    const set = board?.children.find((n) => n.type === "COMPONENT_SET" && n.name === "Recorder");
    if (!set) throw new Error("Missing recorder state library");
    const padding = noteGap, spacing = value("spacing/scene");
    const variants = [...set.children];
    const width = Math.max(...variants.map((n) => n.width));
    resize(board, width * 2 + spacing + 2 * padding + 2 * spacing, board.height);
    if (!dryRun) {
      for (const n of board.children.filter((n) => n.type === "TEXT" && n.layoutPositioning !== "ABSOLUTE")) {
        n.layoutSizingHorizontal = "FILL"; touched.add(n.id);
      }
    }
    let y = padding;
    for (let row = 0; row < variants.length; row += 2) {
      let height = 0;
      for (const [i, variant] of variants.slice(row, row + 2).entries()) {
        const x = padding + i * (width + spacing);
        const note = board.children.find((n) => n.name === "Purpose / " + variant.name);
        if (!note) throw new Error("Missing purpose note for " + variant.name);
        resize(note, variant.width, note.height);
        position(variant, x, y);
        position(note, set.x + x, set.y + y + variant.height + noteGap);
        height = Math.max(height, variant.height + noteGap + note.height);
      }
      y += height + spacing;
    }
    resize(set, width * 2 + spacing + 2 * padding, y - spacing + padding);
    // Hug-content height may have changed after the component set resized.
    virtual.set(board.id, { width: size(board).width, height: board.height });
  }
  for (const section of page.children.filter((n) => n.type === "SECTION")) {
    if (section.name === "App / Complete screens") {
      const targets = section.children.filter((n) => n.type === "COMPONENT");
      const s = pack(section, targets, stagePadding, stagePadding, gap);
      resize(section, s.width, s.height);
    } else if (/^S[0-9]{2} /.test(section.name)) {
      const frame = section.children.find((n) => n.type === "FRAME" && !n.name.startsWith("Edit notes"));
      const note = section.children.find((n) => n.name.startsWith("Edit notes"));
      if (!frame || !note) throw new Error("Shot frame or edit notes missing: " + section.name);
      position(frame, stagePadding, stagePadding);
      position(note, stagePadding, stagePadding + frame.height + stageGap);
      resize(section, frame.width + 2 * stagePadding, frame.height + note.height + stageGap + 2 * stagePadding);
    }
  }
  const guides = page.children.filter((n) => n.name.startsWith("Guide / ") || n.name === "Kit / Page guide");
  let top = margin;
  for (const guide of guides) { position(guide, margin, top); top += guide.height + gap; }
  const targets = page.children.filter((n) => !guides.includes(n) && !n.name.startsWith("Frame note / "));
  pack(page, targets, margin, top, gap);
  return { pageId: page.id, dryRun, changes, createdNodeIds: [], mutatedNodeIds: dryRun ? [] : [...touched] };
}
