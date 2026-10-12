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
  await phone.page.locator("#save").click();
  await phone.page
    .getByText("Saved. Your words are available on all your devices.", {
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
  await laptop.page.locator("#save").click();
  await laptop.page
    .getByText("Saved. Your words are available on all your devices.", {
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
  await phone.page.locator("#draft").fill("Edited on the phone.");
  await laptop.page.locator("#draft").fill("Concurrent laptop edit.");
  await phone.page.locator("#save").click();
  await phone.page.waitForFunction(
    () =>
      document.getElementById("draft-state").textContent === "Saved to history",
  );
  await laptop.page.locator("#save").click();
  await laptop.page.locator("#conflict-dialog").waitFor();
  assert.equal(
    await laptop.page.locator("#conflict-text").inputValue(),
    "Edited on the phone.",
  );
  await screenshot(laptop.page, "conflict-laptop.png");
  await laptop.page.locator("#conflict-local").click();
  await laptop.page.waitForFunction(
    () =>
      document.getElementById("draft-state").textContent === "Saved to history",
  );
  let stored = (await api(laptop.page, `/history/${id}`)).body;
  assert.equal(stored.revision, 3);
  assert.equal(stored.rawText, "A phone thought, ready for the laptop.");
  assert.equal(stored.text, "Concurrent laptop edit.");
  assert.equal((await api(phone.page, "/history")).body.entries.length, 2);
  receipts.push(
    "concurrent edits require explicit resolution, originals preserved, same history ID",
  );
  await laptop.page.locator("#rewrite-panel>summary").click();
  await laptop.page.locator("#template").selectOption("notes");
  await laptop.page.locator("#rewrite").click();
  await laptop.page.locator("#rewrite-result").waitFor();
  await laptop.page.locator("#apply-rewrite").click();
  await laptop.page.waitForFunction(() =>
    document
      .querySelector("#rewrites")
      .textContent.includes("synthetic-preview"),
  );
  stored = (await api(laptop.page, `/history/${id}`)).body;
  assert.equal(stored.rewrites.length, 1);
  assert.equal(stored.rawText, "A phone thought, ready for the laptop.");
  receipts.push(
    "server rewrite preview, explicit apply, completed reply in shared history",
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
      document.getElementById("draft-state").textContent === "Saved to history",
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
      document.getElementById("draft-state").textContent === "Saved to history",
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
      document.getElementById("draft-state").textContent === "Saved to history",
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
  await phone.page.locator("#draft").fill("An offline thought.");
  preview.faults.history = true;
  await phone.page.locator("#save").click();
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
      document.getElementById("draft-state").textContent === "Saved to history",
  );
  assert.equal(
    (await api(phone.page, "/history")).body.entries.filter(
      (x) => x.text === "An offline thought.",
    ).length,
    1,
  );
  receipts.push("failed save → reload → retry without duplicated note");
  // A cancelled rewrite cannot reappear after the source changes.
  await phone.page.locator("#rewrite-panel>summary").click();
  let releaseRewrite;
  const rewriteHeld = new Promise((resolve) => {
    releaseRewrite = resolve;
  });
  await phone.page.route("**/api/rewrite**", async (route) => {
    await rewriteHeld;
    await route.continue();
  });
  await phone.page.locator("#rewrite").click();
  await phone.page.locator("#rewrite-cancel").click();
  releaseRewrite();
  await phone.page.waitForTimeout(600);
  assert.equal(await phone.page.locator("#rewrite-result").isVisible(), false);
  receipts.push("cancel invalidates a late rewrite");
  await laptop.page.locator("#new-note").click();
  await laptop.page
    .locator("#draft")
    .fill("The browser source must remain available for review.");
  await laptop.page.locator("#task-mode").selectOption("command");
  const countBeforeCommand = (await api(laptop.page, "/history")).body.entries
    .length;
  await laptop.page.locator("#record").click();
  await laptop.page.locator("#speech-start").click();
  await laptop.page.locator("#partial").waitFor();
  await screenshot(laptop.page, "recording-laptop.png");
  await laptop.page.locator("#record").click();
  await laptop.page.waitForFunction(
    () => document.getElementById("record-label").textContent === "Record",
  );
  assert.equal(
    await laptop.page.locator("#draft").inputValue(),
    "The browser source must remain available for review.",
  );
  assert.match(
    await laptop.page.locator("#instruction").inputValue(),
    /A thought from this device/,
  );
  assert.equal(
    (await api(laptop.page, "/history")).body.entries.length,
    countBeforeCommand,
  );
  await laptop.page.locator("#rewrite").click();
  await laptop.page.locator("#rewrite-result").waitFor();
  await laptop.page.locator("#apply-rewrite").click();
  await laptop.page.waitForFunction(
    () =>
      document.getElementById("draft-state").textContent === "Saved to history",
  );
  receipts.push(
    "Command keeps supplied source, captures spoken instruction, and requires explicit preview/apply",
  );
  // Memory-only mode must not call history or fan-out with its new words.
  await phone.page.locator("#new-note").click();
  await phone.page.locator("#incognito").check();
  await phone.page.locator("#draft").fill("Incognito secret stays in memory.");
  assert.equal(await phone.page.locator("#send").isDisabled(), true);
  assert.equal(await phone.page.locator("#save").isDisabled(), true);
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
