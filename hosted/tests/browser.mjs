import { chromium } from "playwright";
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { createPreview } from "../scripts/dev.mjs";
if (!process.env.OFFSCREEN_SESSION_ROOT)
  throw new Error("Run this check through the offscreen Linux runner.");
const preview = await createPreview(),
  errors = [],
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
const receipts = [];
async function device(name, viewport) {
  const context = await browser.newContext({
    viewport,
    permissions: ["microphone", "clipboard-read", "clipboard-write"],
  });
  await context.route("**/*", (route) =>
    route.request().url().startsWith(preview.origin)
      ? route.continue()
      : route.abort(),
  );
  context.setDefaultTimeout(15000);
  const page = await context.newPage();
  page.on("dialog", (dialog) => dialog.accept());
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(preview.origin);
  await page.locator("#signin").click();
  await page.locator("#device-dialog").waitFor();
  await page.locator("#name").fill(name);
  await page
    .locator("#kind")
    .selectOption(name.includes("Phone") ? "phone" : "laptop");
  await page.getByRole("button", { name: "Connect this device" }).click();
  await page.waitForFunction(
    () =>
      document.getElementById("connection-label").textContent === "Connected",
  );
  return { context, page };
}
try {
  const landing = await browser.newPage({
    viewport: { width: 390, height: 844 },
  });
  await landing.goto(preview.origin);
  await landing.screenshot({
    path: new URL("welcome-phone.png", artifacts).pathname,
    fullPage: true,
  });
  await landing.close();
  const phone = await device("My Phone", { width: 390, height: 844 }),
    laptop = await device("My Laptop", { width: 1365, height: 900 }),
    third = await device("My Tablet", { width: 768, height: 1024 });
  await phone.page.locator("#refresh-devices").click();
  await laptop.page.locator("#refresh-devices").click();
  await phone.page.getByRole("checkbox", { name: "Send to My Laptop" }).check();
  await laptop.page.getByRole("checkbox", { name: "Send to My Phone" }).check();
  await phone.page.getByRole("checkbox", { name: "Send to My Tablet" }).check();
  await phone.page
    .locator("#draft")
    .fill("A phone thought, ready for the laptop.");
  await phone.page.locator("#send").click();
  await laptop.page
    .locator("#incoming")
    .getByText("A phone thought, ready for the laptop.", { exact: true })
    .waitFor();
  await phone.page.locator("#save").click();
  await laptop.page
    .locator("#history")
    .getByRole("heading", { name: "A phone thought, ready for the laptop." })
    .waitFor();
  await third.page
    .locator("#incoming")
    .getByText("A phone thought, ready for the laptop.", { exact: true })
    .waitFor();
  receipts.push(
    "phone → laptop and tablet multicast live text and shared durable history",
  );
  await laptop.page
    .locator("#draft")
    .fill("A laptop reply, ready for the phone.");
  await laptop.page.locator("#send").click();
  await phone.page
    .locator("#incoming")
    .getByText("A laptop reply, ready for the phone.", { exact: true })
    .waitFor();
  receipts.push("laptop → phone live text");
  await laptop.page.locator("#save").click();
  await phone.page
    .locator("#history")
    .getByRole("heading", { name: "A laptop reply, ready for the phone." })
    .waitFor();
  await phone.page.locator("#record").click();
  await phone.page.locator("#speech-start").click();
  await laptop.page
    .locator("#incoming")
    .getByText("A thought from this device.", { exact: true })
    .waitFor();
  await phone.page.waitForTimeout(200); // include audio after the earlier manual commit
  await phone.page.locator("#record").click();
  await phone.page.waitForFunction(
    () =>
      document.getElementById("draft-state").textContent === "Saved to history",
  );
  await laptop.page
    .locator("#history")
    .getByText("Final words are preserved.", { exact: false })
    .first()
    .waitFor();
  const entries = await phone.page.evaluate(async () => {
    const token = JSON.parse(sessionStorage.getItem("mluva-auth")).access_token;
    return (
      await (
        await fetch("/api/history", {
          headers: { authorization: `Bearer ${token}` },
        })
      ).json()
    ).entries;
  });
  assert.ok(
    entries.some(
      (entry) =>
        entry.rawText ===
        "A thought from this device.\nFinal words are preserved.",
    ),
  );
  receipts.push(
    "actual AudioWorklet PCM, partial/committed synthetic speech, final commit and auto-save",
  );
  assert.equal(
    await phone.page.evaluate(
      () => document.documentElement.scrollWidth > innerWidth,
    ),
    false,
  );
  await phone.page.screenshot({
    path: new URL("workspace-phone.png", artifacts).pathname,
    fullPage: true,
  });
  await laptop.page.screenshot({
    path: new URL("workspace-laptop.png", artifacts).pathname,
    fullPage: true,
  });
  await phone.page.locator("#draft").fill("Recovered after a failed save.");
  preview.faults.history = true;
  await phone.page.locator("#save").click();
  await phone.page.locator("#recovery").waitFor();
  await phone.page.reload();
  await phone.page.waitForFunction(
    () =>
      document.getElementById("draft").value ===
      "Recovered after a failed save.",
  );
  preview.faults.history = false;
  await phone.page.locator("#retry").click();
  await phone.page.waitForFunction(
    () =>
      document.getElementById("draft-state").textContent === "Saved to history",
  );
  receipts.push(
    "failed save → reload → recovered draft → retry without duplicate history",
  );
  await phone.page.locator("#search").fill("Recovered");
  assert.equal(await phone.page.locator(".history-card").count(), 1);
  await phone.page.locator("#search").fill("");
  const exportButton = phone.page
    .locator(".history-card")
    .filter({ hasText: "Recovered after a failed save." })
    .getByRole("button", { name: "Export", exact: true });
  const downloaded = phone.page.waitForEvent("download");
  await exportButton.click();
  assert.ok((await downloaded).suggestedFilename().endsWith(".md"));
  receipts.push("search and downloadable history export");
  const card = phone.page
    .locator(".history-card")
    .filter({ hasText: "Recovered after a failed save." });
  await card.getByRole("button", { name: "Delete", exact: true }).click();
  await phone.page.locator("#confirm-yes").click();
  await card.waitFor({ state: "detached" });
  receipts.push("history deletion propagates");
  await phone.page.locator("#install").click();
  await phone.page.locator("#install-dialog").waitFor();
  await phone.page.getByRole("button", { name: "Got it" }).click();
  await phone.page.locator("#clear").click();
  await phone.page.locator("#confirm-yes").click();
  await phone.page.locator("#retain").uncheck();
  await phone.page.locator("#confirm-yes").click();
  await phone.page.locator("#draft").fill("Memory only.");
  await phone.page.waitForTimeout(500);
  await phone.page.reload();
  await phone.page.waitForFunction(
    () =>
      document.getElementById("connection-label").textContent === "Connected",
  );
  assert.equal(await phone.page.locator("#draft").inputValue(), "");
  receipts.push("history-off draft has no persistent recovery");
  await laptop.page
    .getByRole("button", { name: "Remove My Phone", exact: true })
    .click();
  await laptop.page.locator("#confirm-yes").click();
  await phone.page.waitForFunction(
    () => document.getElementById("connection-label").textContent === "Removed",
  );
  assert.equal(preview.peers.size, 2);
  receipts.push("device revocation stops the receiver");
  assert.deepEqual(errors, []);
  await writeFile(
    new URL("browser-receipts.json", artifacts),
    JSON.stringify(
      {
        receipts,
        errors,
        display: process.env.DISPLAY,
        fakeMicrophone: true,
        physicalPhone: false,
      },
      null,
      2,
    ),
  );
  process.stdout.write(
    `${receipts.join("\n")}\nScreenshots: ${artifacts.pathname}\n`,
  );
} finally {
  await browser.close();
  await preview.close();
}
