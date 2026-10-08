import { chromium } from "../../../tmp/browser/node_modules/playwright/index.mjs";
import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm } from "node:fs/promises";

await mkdir("tmp/browser-evidence", { recursive: true });
let page;
const profile = await mkdtemp("tmp/browser-evidence/profile-");
// Installation is unavailable in incognito contexts. Use a disposable profile.
const context = await chromium.launchPersistentContext(profile, {
  executablePath: "/usr/bin/chromium", headless: true,
  args: ["--use-fake-device-for-media-stream", "--use-fake-ui-for-media-stream", "--disable-dev-shm-usage"],
  viewport: { width: 390, height: 844 }, permissions: ["microphone"],
});
try {
  await context.addCookies([{ name: "mluva-test-access", value: "authorized", url: process.env.MLUVA_TEST_URL, httpOnly: true, sameSite: "Lax" }]);
  page = await context.newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(process.env.MLUVA_TEST_URL);
  await page.getByRole("button", { name: "Record", exact: true }).waitFor();
  await page.waitForFunction(() => !document.getElementById("record").disabled);
  // Authenticated manifest/icon loading is essential behind Cloudflare Access.
  const manifestLink = page.locator('link[rel="manifest"]');
  assert.equal(await manifestLink.getAttribute("crossorigin"), "use-credentials");
  const cdp = await context.newCDPSession(page);
  const manifest = await cdp.send("Page.getAppManifest");
  assert.deepEqual(manifest.errors, []);
  const parsed = JSON.parse(manifest.data);
  assert.equal(parsed.display, "standalone");
  assert.equal(parsed.start_url, "/");
  for (const icon of parsed.icons) {
    const response = await context.request.get(new URL(icon.src, process.env.MLUVA_TEST_URL).href);
    assert.equal(response.status(), 200);
    assert.match(response.headers()["content-type"], /image\/png/);
  }
  await page.waitForFunction(() => navigator.serviceWorker.controller !== null);
  const installability = await cdp.send("Page.getInstallabilityErrors");
  assert.deepEqual(installability.installabilityErrors, []);
  assert.equal(await page.evaluate(async () => (await caches.keys()).length), 0);
  // Check iPhone instructions in a context without native install prompts.
  // Chromium with this UA tests our guide, not physical Safari acceptance.
  const guideContext = await context.browser().newContext({
    storageState: await context.storageState(),
    viewport: { width: 390, height: 844 },
    userAgent: "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 Version/18.0 Mobile/15E148 Safari/604.1",
  });
  try {
    const guide = await guideContext.newPage();
    await guide.goto(process.env.MLUVA_TEST_URL);
    await guide.getByRole("button", { name: "Install app", exact: true }).click();
    assert(await guide.getByRole("dialog").isVisible());
    assert.match(await guide.locator("#install-instructions").innerText(), /Safari.*Add to Home Screen/);
    await guide.getByRole("button", { name: "Got it" }).click();
  } finally { await guideContext.close(); }
  assert.equal(await page.locator("#recent").getAttribute("open"), null);
  const bounds = await page.locator("#record").boundingBox();
  assert(bounds.width >= 160 && bounds.height >= 160);
  await page.screenshot({ path: "tmp/browser-evidence/mobile-ready.png", fullPage: true });
  await page.getByRole("button", { name: "Record", exact: true }).click();
  await page.getByRole("button", { name: "Stop", exact: true }).waitFor();
  await page.waitForTimeout(1200);
  await page.getByRole("button", { name: "Stop", exact: true }).click();
  await page.waitForFunction(() => document.getElementById("text").value === "Synthetic browser test.");
  await page.waitForFunction(() => document.getElementById("status").textContent.includes("clipboard"));
  await page.waitForFunction(() => document.querySelectorAll("#history article").length === 1 && !document.getElementById("record").disabled);
  assert.equal(await page.locator("#history article").count(), 1);
  assert.equal(await page.locator("#download").isVisible(), false);
  await page.locator("#recent summary").click();
  await page.screenshot({ path: "tmp/browser-evidence/mobile-completed.png", fullPage: true });
  // A second device sees the completed recording without receiving desktop history.
  const second = await context.newPage();
  await second.goto(process.env.MLUVA_TEST_URL);
  await second.waitForFunction(() => document.querySelectorAll("#history article").length === 1);
  await second.close();
  // Connection failure preserves audio across reload, and Retry completes that same ID.
  await context.setOffline(true);
  await page.getByRole("button", { name: "Record", exact: true }).click();
  await page.getByRole("button", { name: "Stop", exact: true }).waitFor();
  await page.waitForTimeout(1200);
  await page.getByRole("button", { name: "Stop", exact: true }).click();
  await page.getByRole("button", { name: "Retry transfer" }).waitFor();
  await context.setOffline(false);
  await page.reload();
  await page.getByRole("button", { name: "Retry transfer" }).waitFor();
  assert(await page.getByRole("link", { name: "Download audio" }).isVisible());
  await page.getByRole("button", { name: "Retry transfer" }).click();
  await page.waitForFunction(() => document.querySelectorAll("#history article").length === 2);
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
  // A controlling worker must not turn expired access into a cached response.
  await context.clearCookies();
  assert.equal(await page.evaluate(async () => (await fetch("/api/recordings")).status), 401);
  assert.equal(await page.evaluate(async () => (await caches.keys()).length), 0);
  assert.deepEqual(errors, []);
  console.log("PASS: browser recording, Scribe result, two-device history, PWA manifest/icons/service worker, mobile layout, offline audio recovery, and retry.");
} catch (error) {
  if (page) console.log(await page.locator("body").innerText());
  throw error;
} finally { await context.close(); await rm(profile, { recursive: true, force: true }); }
