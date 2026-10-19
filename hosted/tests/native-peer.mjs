// A separate browser process and loopback protocol peer for the real GTK application.
import { chromium } from "playwright";
import { createInterface } from "node:readline";
import { createPreview } from "../scripts/dev.mjs";
if (!process.env.OFFSCREEN_SESSION_ROOT)
  throw new Error("Private session required");
const preview = await createPreview(
  Number(new URL(process.env.MLUVA_ACCOUNT_ORIGIN).port),
);
const browser = await chromium.launch({
  executablePath: "/usr/bin/chromium",
  headless: true,
  args: ["--disable-gpu", "--no-sandbox"],
});
const pages = [];
for (const name of ["Browser phone", "Third receiver"]) {
  const context = await browser.newContext({
    viewport: { width: name === "Browser phone" ? 390 : 1200, height: 844 },
    serviceWorkers: "block",
  });
  await context.route("**/*", (route) =>
    route.request().url().startsWith(preview.origin)
      ? route.continue()
      : route.abort(),
  );
  const page = await context.newPage();
  page.setDefaultTimeout(15000);
  await page.goto(preview.origin);
  await page.locator("#signin").click();
  await page.locator("#name").fill(name);
  await page.getByRole("button", { name: "Connect this device" }).click();
  await page.waitForFunction(
    () =>
      document.getElementById("connection-label").textContent === "Connected",
  );
  pages.push(page);
}
const peerSession = JSON.parse(
  Buffer.from(
    (
      await pages[0].evaluate(
        () => JSON.parse(sessionStorage.getItem("mluva-auth")).access_token,
      )
    ).split(".")[1],
    "base64url",
  ),
).origin_jti;
const api = (method, path, body = {}) =>
  preview.service.http({
    owner: "preview-alice",
    session: peerSession,
    expires: Math.floor(Date.now() / 1000) + 3600,
    method,
    path,
    body,
    query: body,
  });
function reply(value) {
  process.stdout.write(JSON.stringify(value) + "\n");
}
reply({ origin: preview.origin });
for await (const line of createInterface({ input: process.stdin })) {
  try {
    const command = JSON.parse(line);
    let result;
    switch (command.action) {
      case "authorize": {
        const url = new URL(command.url);
        if (
          url.origin !== preview.origin ||
          url.pathname !== "/oauth2/authorize"
        )
          throw new Error("Expected the isolated native authorization request");
        const response = await pages[0].context().request.get(url.href, {
          maxRedirects: 0,
        });
        if (response.status() !== 302)
          throw new Error("Missing native callback");
        result = { callback: response.headers().location };
        break;
      }
      case "send": {
        const page = pages[0];
        await page.locator("#show-settings").click();
        await page.locator("#refresh-devices").click();
        await page
          .getByRole("checkbox", { name: "Send to Native Lenovo" })
          .check();
        await page.getByRole("button", { name: "Back to workspace" }).click();
        await page.locator("#draft").fill(command.text);
        await page.locator("#send").click();
        await page.waitForFunction(
          () =>
            document.getElementById("draft-state").textContent ===
            "Saved automatically",
        );
        result = { ok: true };
        break;
      }
      case "live":
        for (const page of pages)
          await page
            .locator("#incoming")
            .getByText(command.text, { exact: true })
            .waitFor();
        result = { ok: true };
        break;
      case "documents": {
        const device = (await api("GET", "/devices")).devices.find(
          (d) => d.currentSession,
        );
        result = await api("GET", "/history", { deviceId: device.id });
        break;
      }
      case "edit": {
        const device = (await api("GET", "/devices")).devices.find(
          (d) => d.currentSession,
        );
        const note = await api("GET", `/history/${command.id}`, {
          deviceId: device.id,
        });
        result = await api("PUT", `/history/${command.id}`, {
          ...note,
          deviceId: device.id,
          text: command.text,
          baseRevision: note.revision,
          operationId: crypto.randomUUID(),
          rewrites: command.rewrites ?? note.rewrites,
        });
        break;
      }
      case "delete": {
        const device = (await api("GET", "/devices")).devices.find(
          (d) => d.currentSession,
        );
        result = await api("DELETE", `/history/${command.id}`, {
          deviceId: device.id,
        });
        break;
      }
      case "offline":
        preview.faults.history = command.enabled;
        for (const peer of preview.peers.values()) peer.terminate();
        result = { ok: true };
        break;
      case "revoke": {
        const native = (await api("GET", "/devices")).devices.find(
          (d) => d.name === "Native Lenovo",
        );
        result = await api("DELETE", `/devices/${native.id}`);
        break;
      }
      case "audit":
        result = { requests: preview.requests };
        break;
      case "close":
        reply({ ok: true });
        await browser.close();
        await preview.close();
        process.exit(0);
        break;
      default:
        throw new Error("Unknown test command");
    }
    reply(result);
  } catch (error) {
    reply({ error: error.message });
  }
}
await browser.close();
await preview.close();
