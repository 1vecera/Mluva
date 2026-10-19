import { chromium } from "playwright";
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { createPreview } from "../scripts/dev.mjs";
if (!process.env.OFFSCREEN_SESSION_ROOT)
  throw new Error("Use the offscreen runner.");
const preview = await createPreview(),
  errors = [],
  receipts = [],
  artifacts = new URL("../../tmp/hosted-evidence/", import.meta.url);
await mkdir(artifacts, { recursive: true });
const browser = await chromium.launch({
  executablePath: "/usr/bin/chromium",
  headless: true,
  args: [
    "--use-fake-device-for-media-stream",
    "--use-fake-ui-for-media-stream",
    "--disable-gpu",
    "--no-sandbox",
  ],
});
async function screenshot(page, name) {
  await page.evaluate(() => window.scrollTo(0, 0));
  await page.screenshot({
    path: new URL(name, artifacts).pathname,
    fullPage: false,
  });
  assert.equal(
    await page.evaluate(
      () => document.documentElement.scrollWidth > innerWidth,
    ),
    false,
    `overflow: ${name}`,
  );
}
async function device(name, viewport) {
  const context = await browser.newContext({
    viewport,
    serviceWorkers: "block",
    permissions: ["microphone", "clipboard-read", "clipboard-write"],
  });
  await context.route("**/*", (route) =>
    route.request().url().startsWith(preview.origin)
      ? route.continue()
      : route.abort(),
  );
  const page = await context.newPage();
  page.setDefaultTimeout(15000);
  page.on("dialog", (d) => d.accept());
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(preview.origin);
  await page.locator("#signin").click();
  await page.locator("#name").fill(name);
  await page.getByRole("button", { name: "Connect this device" }).click();
  await page.waitForFunction(
    () =>
      document.getElementById("connection-label").textContent === "Connected",
  );
  return { page, context };
}
async function settings(page, fn) {
  await page.locator("#show-settings").click();
  await fn();
  await page.getByRole("button", { name: "Back to workspace" }).click();
}
async function history(page, fn) {
  const mobile = page.viewportSize().width <= 650;
  if (mobile) await page.locator("#show-history").click();
  await fn();
  if (
    mobile &&
    (await page
      .locator("body")
      .evaluate((e) => e.classList.contains("history-open")))
  )
    await page.locator("#show-history").click();
}
async function api(page, path, method = "GET", body, deviceOverride) {
  return page.evaluate(
    async ({ path, method, body, deviceOverride }) => {
      const auth = JSON.parse(sessionStorage.getItem("mluva-auth"));
      const owner = JSON.parse(atob(auth.access_token.split(".")[1])).sub;
      const id =
        deviceOverride ?? localStorage.getItem(`mluva-device:${owner}`);
      const reply = await fetch(
        `/api${path}${path.includes("?") ? "&" : "?"}deviceId=${id}`,
        {
          method,
          headers: {
            authorization: `Bearer ${auth.access_token}`,
            "content-type": "application/json",
          },
          ...(body ? { body: JSON.stringify({ deviceId: id, ...body }) } : {}),
        },
      );
      return { status: reply.status, body: await reply.json() };
    },
    { path, method, body, deviceOverride },
  );
}
try {
  const landing = await browser.newPage({
    viewport: { width: 390, height: 844 },
  });
  await landing.goto(preview.origin);
  await screenshot(landing, "login-phone.png");
  await landing.setViewportSize({ width: 1365, height: 900 });
  await screenshot(landing, "login-laptop.png");
  await landing.close();
  const phone = await device("My Phone", { width: 390, height: 844 }),
    laptop = await device("My Laptop", { width: 1365, height: 900 }),
    third = await device("My Tablet", { width: 768, height: 1024 });
  await settings(phone.page, async () => {
    await phone.page.locator("#refresh-devices").click();
    await phone.page
      .getByRole("checkbox", { name: "Send to My Laptop" })
      .check();
    await phone.page
      .getByRole("checkbox", { name: "Send to My Tablet" })
      .check();
    await screenshot(phone.page, "settings-phone.png");
  });
  await settings(laptop.page, async () => {
    await laptop.page.locator("#refresh-devices").click();
    await laptop.page
      .getByRole("checkbox", { name: "Send to My Phone" })
      .check();
    await screenshot(laptop.page, "settings-laptop.png");
  });
  await phone.page
    .locator("#draft")
    .fill("A phone thought, ready for the laptop.");
  preview.faults.dropAckOnce = true;
  const fanoutStarted = performance.now();
  await phone.page.locator("#send").click();
  for (const receiver of [laptop, third])
    await receiver.page
      .locator("#incoming")
      .getByText("A phone thought, ready for the laptop.", { exact: true })
      .waitFor();
  const localFanoutUiMs = performance.now() - fanoutStarted;
  await phone.page.waitForFunction(
    () => !document.getElementById("send").disabled,
  );
  assert.equal(
    await laptop.page
      .locator("#incoming")
      .getByText("A phone thought, ready for the laptop.", { exact: true })
      .count(),
    1,
  );
  receipts.push(
    "lost live acknowledgement → reconnect → identical retry, one receiver segment",
  );
  await phone.page
    .getByText("Saved automatically", {
      exact: true,
    })
    .waitFor();
  let entries = (await api(laptop.page, "/history")).body.entries;
  assert.equal(entries.length, 1);
  const id = entries[0].id;
  receipts.push("phone → laptop and third receiver live text; one saved note");
  await laptop.page
    .locator("#draft")
    .fill("A laptop reply, ready for the phone.");
  await laptop.page.locator("#send").click();
  await phone.page
    .locator("#incoming")
    .getByText("A laptop reply, ready for the phone.", { exact: true })
    .waitFor();
  await laptop.page
    .getByText("Saved automatically", {
      exact: true,
    })
    .waitFor();
  receipts.push("laptop → phone live text");
  // Same note opened on two clients, then edited concurrently.
  await history(phone.page, async () => {
    await phone.page
      .locator(".history-card")
      .filter({ hasText: "A phone thought, ready for the laptop." })
      .getByRole("button", { name: "Open", exact: true })
      .click();
  });
  await history(laptop.page, async () => {
    await laptop.page
      .locator(".history-card")
      .filter({ hasText: "A phone thought, ready for the laptop." })
      .getByRole("button", { name: "Open", exact: true })
      .click();
  });
  let releaseConflict;
  const conflictGate = new Promise((resolve) => {
    releaseConflict = resolve;
  });
  const conflictRoute = `${preview.origin}/api/history/${id}?*`;
  await laptop.page.route(conflictRoute, async (route) => {
    if (route.request().method() === "PUT") await conflictGate;
    await route.continue();
  });
  await phone.page.locator("#draft").fill("Edited on the phone.");
  await laptop.page.locator("#draft").fill("Concurrent laptop edit.");
  await phone.page.waitForFunction(
    () =>
      document.getElementById("draft-state").textContent ===
      "Saved automatically",
  );
  releaseConflict();
  await laptop.page.locator("#conflict-dialog").waitFor();
  await laptop.page.unroute(conflictRoute);
  assert.equal(
    await laptop.page.locator("#conflict-text").inputValue(),
    "Edited on the phone.",
  );
  await screenshot(laptop.page, "conflict-laptop.png");
  await laptop.page.locator("#conflict-local").click();
  await laptop.page.waitForFunction(
    () =>
      document.getElementById("draft-state").textContent ===
      "Saved automatically",
  );
  let stored = (await api(laptop.page, `/history/${id}`)).body;
  assert.equal(stored.revision, 3);
  assert.equal(stored.rawText, "A phone thought, ready for the laptop.");
  assert.equal(stored.text, "Concurrent laptop edit.");
  assert.equal((await api(phone.page, "/history")).body.entries.length, 2);
  receipts.push(
    "concurrent edits require explicit resolution, originals preserved, same history ID",
  );
  // Upgrade compatibility: a previously saved reply survives editing and export.
  stored.rewrites = [
    {
      id: crypto.randomUUID(),
      instruction: "Old polish",
      text: "A previously saved rewrite.",
      model: "legacy-model",
      createdAt: new Date().toISOString(),
    },
  ];
  delete stored.deviceId;
  const legacySave = await api(laptop.page, `/history/${id}`, "PUT", {
    ...stored,
    baseRevision: stored.revision,
    operationId: crypto.randomUUID(),
  });
  assert.equal(legacySave.status, 200, JSON.stringify(legacySave.body));
  await laptop.page.reload();
  await history(laptop.page, async () =>
    laptop.page
      .locator(".history-card")
      .filter({ hasText: "A phone thought, ready for the laptop." })
      .getByRole("button", { name: "Open", exact: true })
      .click(),
  );
  await laptop.page
    .locator("#rewrites")
    .getByText("legacy-model", { exact: false })
    .waitFor();
  stored = (await api(laptop.page, `/history/${id}`)).body;
  assert.equal(stored.rewrites[0].text, "A previously saved rewrite.");
  assert.equal(stored.rawText, "A phone thought, ready for the laptop.");
  assert.equal(
    await laptop.page.locator("#rewrite-panel,#task-mode,#rewrite").count(),
    0,
  );
  assert.equal(
    (
      await api(laptop.page, "/rewrite", "POST", {
        source: "private words",
        instruction: "Polish",
      })
    ).status,
    404,
  );
  receipts.push(
    "saved legacy replies preserved; generation route and mode controls removed",
  );
  await screenshot(laptop.page, "editing-laptop.png");
  // Native-like source note remains independently edited while synthetic speech records a fresh note.
  await phone.page.locator("#new-note").click();
  await phone.page.locator("#record").click();
  await phone.page.locator("#speech-start").click();
  await phone.page
    .locator("#partial")
    .filter({ hasText: "A thought from" })
    .waitFor();
  await screenshot(phone.page, "recording-phone.png");
  await phone.page.locator("#record").click();
  await phone.page.waitForFunction(
    () =>
      document.getElementById("draft-state").textContent ===
      "Saved automatically",
  );
  assert.match(
    await phone.page.locator("#draft").inputValue(),
    /A thought from this device/,
  );
  await screenshot(phone.page, "editing-phone.png");
  receipts.push(
    "synthetic microphone → server Scribe protocol, partial words, Stop finalization and saved final words",
  );
  await history(phone.page, async () =>
    screenshot(phone.page, "history-phone.png"),
  );
  await screenshot(laptop.page, "history-laptop.png");
  const recordedId = (await api(phone.page, "/history")).body.entries.find(
    (e) => e.text.includes("A thought from this device"),
  ).id;
  await phone.page.locator("#continue-recording").click();
  await phone.page.locator("#speech-start").click();
  await phone.page.locator("#partial").waitFor();
  await phone.page.locator("#record").click();
  await phone.page.waitForFunction(
    () =>
      document.getElementById("draft-state").textContent ===
      "Saved automatically",
  );
  const continued = (await api(phone.page, `/history/${recordedId}`)).body;
  assert.equal(continued.segments.length, 1);
  assert.match(continued.rawText, /A thought from this device/);
  await phone.page.locator("#conversation-controls>summary").click();
  await phone.page.locator("#merge-with").selectOption(id);
  await phone.page.locator("#merge-note").click();
  await phone.page.locator("#confirm-yes").click();
  await phone.page.waitForFunction(
    () =>
      document.getElementById("draft-state").textContent ===
      "Saved automatically",
  );
  const merged = (await api(phone.page, `/history/${recordedId}`)).body;
  assert.equal(merged.segments.length, 2);
  assert.equal(merged.rewrites.length, 1);
  assert.equal((await api(phone.page, `/history/${id}`)).status, 410);
  receipts.push(
    "continued recording and atomic conversation merge preserve original segments and completed rewrites",
  );
  // Interrupted save survives a reload and is retried with the same ID.
  await phone.page.locator("#new-note").click();
  preview.faults.history = true;
  await phone.page.locator("#draft").fill("An offline thought.");
  await phone.page.locator("#recovery").waitFor();
  await screenshot(phone.page, "recovery-phone.png");
  await phone.page.setViewportSize({ width: 1365, height: 900 });
  await screenshot(phone.page, "recovery-laptop.png");
  await phone.page.setViewportSize({ width: 390, height: 844 });
  await phone.page.reload();
  await phone.page.locator("#recovery").waitFor();
  assert.equal(
    await phone.page.locator("#draft").inputValue(),
    "An offline thought.",
  );
  preview.faults.history = false;
  await phone.page.locator("#retry").click();
  await phone.page.waitForFunction(
    () =>
      document.getElementById("draft-state").textContent ===
      "Saved automatically",
  );
  assert.equal(
    (await api(phone.page, "/history")).body.entries.filter(
      (x) => x.text === "An offline thought.",
    ).length,
    1,
  );
  receipts.push("failed save → reload → retry without duplicated note");
  await laptop.page.locator("#new-note").click();
  await laptop.page.locator("#record").click();
  await laptop.page.locator("#speech-start").click();
  await laptop.page.locator("#partial").waitFor();
  await screenshot(laptop.page, "recording-laptop.png");
  await laptop.page.locator("#record").click();
  await laptop.page.waitForFunction(
    () =>
      document.getElementById("draft-state").textContent ===
      "Saved automatically",
  );
  assert.match(
    await laptop.page.locator("#draft").inputValue(),
    /A thought from this device/,
  );
  receipts.push(
    "normal laptop recording finalizes directly to an editable saved note",
  );
  // A slow first acknowledgement must not lock typing or overwrite later edits.
  await laptop.page.locator("#new-note").click();
  let releaseSave, requestSeen;
  const saveGate = new Promise((resolve) => {
    releaseSave = resolve;
  });
  const seen = new Promise((resolve) => {
    requestSeen = resolve;
  });
  let firstWrite = true;
  const saveRoute = `${preview.origin}/api/history?*`;
  await laptop.page.route(saveRoute, async (route) => {
    if (route.request().method() !== "POST" || !firstWrite)
      return route.continue();
    firstWrite = false;
    const response = await route.fetch();
    requestSeen();
    await saveGate;
    await route.fulfill({ response });
  });
  await laptop.page.locator("#draft").fill("First immutable typed words.");
  await seen;
  assert.equal(await laptop.page.locator("#draft").isEditable(), true);
  await laptop.page.locator("#draft").fill("Newer text typed during saving.");
  await laptop.page.locator("#note-title").fill("New title during saving");
  releaseSave();
  await laptop.page.waitForFunction(
    () =>
      document.getElementById("draft-state").textContent ===
      "Saved automatically",
  );
  await laptop.page.unroute(saveRoute);
  const raceNote = (await api(laptop.page, "/history")).body.entries.find(
    (note) => note.title === "New title during saving",
  );
  assert.equal(raceNote.text, "Newer text typed during saving.");
  assert.equal(raceNote.rawText, "First immutable typed words.");
  assert.equal(raceNote.revision, 2);
  await laptop.page.locator("#draft").fill("");
  await laptop.page.waitForFunction(
    () =>
      document.getElementById("draft-state").textContent ===
      "Saved automatically",
  );
  assert.equal(
    (await api(laptop.page, `/history/${raceNote.id}`)).body.text,
    "",
  );
  receipts.push(
    "autosave preserves typing and title changes during a request; blank working text preserves its original",
  );
  // History off also keeps ordinary edits in memory, without requiring Incognito.
  await laptop.page.locator("#retain").uncheck();
  await laptop.page.locator("#confirm-yes").click();
  await laptop.page.locator("#draft").fill("Later words stay in memory.");
  await laptop.page.waitForTimeout(800);
  assert.equal(
    await laptop.page.locator("#draft-state").textContent(),
    "Kept in memory",
  );
  assert.equal(
    (await api(laptop.page, `/history/${raceNote.id}`)).body.text,
    "",
  );
  receipts.push("history disabled keeps subsequent edits in memory");
  // Memory-only mode must not call history or fan-out with its new words.
  await phone.page.locator("#new-note").click();
  await phone.page.locator("#incognito").check();
  await phone.page.locator("#draft").fill("Incognito secret stays in memory.");
  assert.equal(await phone.page.locator("#send").isDisabled(), true);
  assert.equal(await phone.page.locator("#save").count(), 0);
  await phone.page.waitForTimeout(800);
  assert.equal(
    JSON.stringify([...preview.store.rows.values()]).includes(
      "Incognito secret",
    ),
    false,
  );
  receipts.push("Incognito has no saved history or text fan-out");
  await phone.page.setViewportSize({ width: 320, height: 740 });
  await screenshot(phone.page, "minimum-phone.png");
  await settings(laptop.page, async () => {
    await laptop.page.locator("#theme").selectOption("dark");
  });
  await screenshot(laptop.page, "dark-laptop.png");
  // Device revocation blocks stale HTTP credentials as well as the open socket.
  const current = await api(third.page, "/devices");
  const tablet = current.body.devices.find((d) => d.name === "My Tablet");
  await api(laptop.page, `/devices/${tablet.id}`, "DELETE");
  await third.page.waitForFunction(
    () => document.getElementById("connection-label").textContent === "Removed",
  );
  assert.equal(
    (await api(third.page, "/history", "GET", undefined, tablet.id)).status,
    403,
  );
  receipts.push("revocation closes live access and rejects history reads");
  const retainedToken = await laptop.page.evaluate(
    () => JSON.parse(sessionStorage.getItem("mluva-auth")).access_token,
  );
  await laptop.page.locator("#signout").click();
  await laptop.page.locator("#signin").waitFor();
  const afterSignOut = await laptop.page.evaluate(
    async (token) => ({
      localAuth: localStorage.getItem("mluva-auth"),
      tabAuth: sessionStorage.getItem("mluva-auth"),
      status: (
        await fetch("/api/devices", {
          headers: { authorization: `Bearer ${token}` },
        })
      ).status,
    }),
    retainedToken,
  );
  assert.deepEqual(afterSignOut, {
    localAuth: null,
    tabAuth: null,
    status: 403,
  });
  receipts.push(
    "sign-out clears browser login and revokes retained account access",
  );
  assert.deepEqual(errors, []);
  await writeFile(
    new URL("checks.json", artifacts),
    JSON.stringify(
      {
        environment:
          "isolated Chromium, synthetic providers, private display/network/devices",
        receipts,
        timing: {
          localFanoutUiMs,
          scope:
            "loopback preview, Playwright click through both receiver renders; not AWS or one-way network latency",
        },
        errors,
      },
      null,
      2,
    ),
  );
  console.log(receipts.map((r) => `PASS ${r}`).join("\n"));
} finally {
  await browser.close();
  await preview.close();
}
