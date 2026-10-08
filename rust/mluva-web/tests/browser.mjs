import { chromium } from "../../../tmp/browser/node_modules/playwright/index.mjs";
import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";

await mkdir("tmp/browser-evidence", { recursive: true });
let page;
const browser = await chromium.launch({ executablePath: "/usr/bin/chromium", headless: true,
  args: ["--use-fake-device-for-media-stream", "--use-fake-ui-for-media-stream", "--disable-dev-shm-usage"] });
try {
  const context = await browser.newContext({ viewport: { width: 390, height: 844 },
    permissions: ["microphone"], extraHTTPHeaders: {
      "cf-access-jwt-assertion": process.env.MLUVA_TEST_JWT,
      "origin": process.env.MLUVA_TEST_ORIGIN,
    } });
  page = await context.newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(process.env.MLUVA_TEST_URL);
  await page.getByRole("button", { name: "Record", exact: true }).waitFor();
  await page.waitForFunction(() => !document.getElementById("record").disabled);
  await page.screenshot({ path: "tmp/browser-evidence/mobile-ready.png", fullPage: true });
  await page.getByRole("button", { name: "Record", exact: true }).click();
  await page.getByRole("button", { name: "Stop & transfer" }).waitFor();
  await page.waitForTimeout(1200);
  await page.getByRole("button", { name: "Stop & transfer" }).click();
  await page.waitForFunction(() => document.getElementById("text").value === "Synthetic browser test.");
  await page.waitForFunction(() => document.getElementById("status").textContent.includes("clipboard"));
  assert.equal(await page.locator("#history article").count(), 1);
  await page.screenshot({ path: "tmp/browser-evidence/mobile-completed.png", fullPage: true });
  // A second device sees the completed recording without receiving desktop history.
  const second = await context.newPage();
  await second.goto(process.env.MLUVA_TEST_URL);
  await second.waitForFunction(() => document.querySelectorAll("#history article").length === 1);
  await second.close();
  // Connection failure preserves audio across reload, and Retry completes that same ID.
  await page.route("**/api/recordings/*", (route) => route.request().method() === "POST" ? route.abort() : route.continue());
  await page.getByRole("button", { name: "Record", exact: true }).click();
  await page.getByRole("button", { name: "Stop & transfer" }).waitFor();
  await page.waitForTimeout(1200);
  await page.getByRole("button", { name: "Stop & transfer" }).click();
  await page.getByRole("button", { name: "Retry transfer" }).waitFor();
  await page.reload();
  await page.getByRole("button", { name: "Retry transfer" }).waitFor();
  assert(await page.getByRole("link", { name: "Download audio" }).isVisible());
  await page.unroute("**/api/recordings/*");
  await page.getByRole("button", { name: "Retry transfer" }).click();
  await page.waitForFunction(() => document.querySelectorAll("#history article").length === 2);
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
  assert.deepEqual(errors, []);
  console.log("PASS: browser recording, Scribe result, two-device history, mobile layout, offline audio recovery, and retry.");
} catch (error) {
  if (page) console.log(await page.locator("body").innerText());
  throw error;
} finally { await browser.close(); }
