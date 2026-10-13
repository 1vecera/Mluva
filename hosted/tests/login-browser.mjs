import { chromium } from "playwright";
import { createPreview } from "../scripts/dev.mjs";
import { readFile, mkdir, writeFile } from "node:fs/promises";
import { createHash, randomBytes } from "node:crypto";
import assert from "node:assert/strict";

if (!process.env.OFFSCREEN_SESSION_ROOT)
  throw new Error("Use the isolated runner.");
let input = "";
for await (const chunk of process.stdin) input += chunk;
const credentials = JSON.parse(input);
const output = "tmp/login-evidence";
const resource = JSON.parse(await readFile(`${output}/resources.json`, "utf8"));
const login = `https://${resource.domain}.auth.${resource.region}.amazoncognito.com`;
const preview = await createPreview(8792);
const evidence = `${output}/screenshots`;
await mkdir(evidence, { recursive: true });
const checks = [],
  errors = [],
  metrics = [];
const browser = await chromium.launch({
  executablePath: "/usr/bin/chromium",
  headless: true,
  args: ["--no-sandbox", "--disable-gpu"],
});
let phase = "start";
const pass = (name) => {
  checks.push(name);
  console.log(`PASS ${name}`);
};
async function context(
  viewport = { width: 1365, height: 900 },
  colorScheme = "light",
) {
  const ctx = await browser.newContext({
    viewport,
    colorScheme,
    serviceWorkers: "block",
  });
  await ctx.route(`${preview.origin}/config.json`, (route) =>
    route.fulfill({
      json: {
        api: `${preview.origin}/api`,
        websocket: preview.origin.replace("http:", "ws:") + "/live",
        login,
        clientId: resource.client,
        origin: preview.origin,
      },
    }),
  );
  await ctx.route(`${login}/oauth2/token`, async (route) => {
    const response = await route.fetch();
    if (response.ok()) {
      const body = await response.json();
      const claims = JSON.parse(
        Buffer.from(body.access_token.split(".")[1], "base64url"),
      );
      preview.sessions.set(body.access_token, {
        owner: claims.sub,
        session: claims.origin_jti,
        expires: claims.exp,
      });
    }
    await route.fulfill({ response });
  });
  ctx.on("page", (page) => {
    page.setDefaultTimeout(20000);
    page.on("pageerror", () => errors.push("uncaught page error"));
  });
  return ctx;
}
async function shot(page, name) {
  await page.screenshot({
    path: `${evidence}/${name}.png`,
    fullPage: true,
    animations: "disabled",
  });
  assert.equal(
    await page.evaluate(
      () => document.documentElement.scrollWidth > innerWidth,
    ),
    false,
    name + " overflows",
  );
}
try {
  for (const [device, viewport] of [
    ["laptop", { width: 1365, height: 900 }],
    ["phone", { width: 390, height: 844 }],
  ]) {
    for (const theme of ["light", "dark"]) {
      phase = `${device}-${theme}`;
      const ctx = await context(viewport, theme),
        page = await ctx.newPage();
      await page.goto(preview.origin);
      await page.locator("#signin").waitFor({ state: "visible" });
      await shot(page, `welcome-${device}-${theme}`);
      metrics.push({
        surface: `welcome-${device}-${theme}`,
        controls: await page
          .locator("#signin, #create-account, .remember, .signin-description")
          .evaluateAll((elements) =>
            elements.map((e) => {
              const style = getComputedStyle(e);
              return {
                element: e.id || e.className,
                height: e.getBoundingClientRect().height,
                fontSize: style.fontSize,
                color: style.color,
                background: style.backgroundColor,
              };
            }),
          ),
      });
      const button = await page.locator("#signin").boundingBox();
      assert.ok(button.height >= 44);
      await page.keyboard.press("Tab");
      assert.ok(
        await page.evaluate(() =>
          document.activeElement.matches(":focus-visible"),
        ),
      );
      await page.locator("#signin").click();
      const emailField = page.getByRole("textbox", {
        name: "Email address",
        exact: true,
      });
      // A freshly created Cognito prefix can precede its managed-login assets.
      // Retry only this initial, unauthenticated page; never replay a form submit.
      if (device === "laptop" && theme === "light") {
        for (let attempt = 0; attempt < 24; attempt++) {
          try {
            await emailField.waitFor({ timeout: 5000 });
            break;
          } catch (error) {
            if (attempt === 23) {
              await shot(page, "provider-startup-failure");
              throw error;
            }
            await page.reload();
          }
        }
      } else await emailField.waitFor();
      phase = `provider-assets-${device}-${theme}`;
      // The first page can have the provider's default theme during propagation.
      if (device === "laptop" && theme === "light") {
        for (let attempt = 0; attempt < 12; attempt++) {
          if (await page.getByRole("img", { name: "Form logo" }).count()) break;
          await new Promise((resolve) => setTimeout(resolve, 5000));
          await page.reload();
        }
      }
      await page
        .getByRole("img", { name: "Form logo" })
        .evaluate((img) => img.decode());
      await shot(page, `credentials-${device}-${theme}`);
      metrics.push({
        surface: `credentials-${device}-${theme}`,
        controls: await page
          .locator(
            "input[type=email], input[type=password], button[type=submit]",
          )
          .evaluateAll((elements) =>
            elements.map((e) => {
              const style = getComputedStyle(e);
              return {
                element: e.type,
                height: e.getBoundingClientRect().height,
                fontSize: style.fontSize,
                color: style.color,
                background: style.backgroundColor,
              };
            }),
          ),
      });
      pass(
        `${device} ${theme}: real Cognito credentials, welcome, keyboard focus and no overflow`,
      );
      if (device === "phone" && theme === "light") {
        await page
          .getByRole("link", { name: "Create an account", exact: true })
          .click();
        await page
          .getByRole("button", { name: "Sign up", exact: true })
          .waitFor();
        await shot(page, "signup-phone");
        await page.goBack();
        await page
          .getByRole("link", { name: "Forgot your password?", exact: true })
          .click();
        await page
          .getByRole("heading", { name: "Forgot your password?", exact: true })
          .waitFor();
        await shot(page, "recovery-phone");
        pass("Real sign-up and recovery pages render; no messages sent");
      }
      await ctx.close();
    }
  }
  phase = "callback-offline";
  const recovery = await context({ width: 320, height: 640 }),
    recoveryPage = await recovery.newPage();
  await recoveryPage.goto(
    `${preview.origin}/callback.html?code=invalid&state=invalid`,
  );
  await recoveryPage
    .getByRole("heading", { name: "Sign-in didn’t finish" })
    .waitFor();
  await shot(recoveryPage, "callback-error-320");
  await recoveryPage.getByRole("link", { name: "Back to sign in" }).click();
  await recoveryPage.locator("#signin").waitFor({ state: "visible" });
  await recovery.setOffline(true);
  await recoveryPage.locator("#signin").click();
  await recoveryPage.locator("#signin-error").waitFor({ state: "visible" });
  assert.equal(await recoveryPage.locator("#signin").isEnabled(), true);
  await shot(recoveryPage, "offline-320");
  await recovery.setOffline(false);
  await recoveryPage.locator("#create-account").click();
  await recoveryPage
    .getByRole("button", { name: "Sign up", exact: true })
    .waitFor();
  pass(
    "320px callback failure, visible offline error, re-enabled controls and successful retry to account creation",
  );
  await recovery.close();
  phase = "text-scaling";
  const scaled = await context({ width: 640, height: 900 }),
    scaledPage = await scaled.newPage();
  await scaledPage.goto(preview.origin);
  await scaledPage.locator("#signin").waitFor({ state: "visible" });
  await scaledPage.addStyleTag({
    content:
      ".auth-page { --signin-body: 32px; } .signin-tagline { font-size: 72px; } .signin-form h1 { font-size: 48px; } .signin .remember, .signin-local { font-size: 28px; } .signin-hint { font-size: 26px; }",
  });
  assert.equal(
    await scaledPage.evaluate(
      () => document.documentElement.scrollWidth > innerWidth,
    ),
    false,
  );
  assert.ok((await scaledPage.locator("#signin").boundingBox()).height >= 44);
  await scaledPage.emulateMedia({ reducedMotion: "reduce" });
  await scaledPage.goto(`${preview.origin}/callback.html`);
  await scaledPage
    .getByRole("heading", { name: "Sign-in didn’t finish" })
    .waitFor();
  assert.equal(
    await scaledPage
      .locator(".callback-progress")
      .evaluate((e) => getComputedStyle(e).animationName),
    "none",
  );
  const back = scaledPage.locator("#callback-return");
  assert.equal(
    await back.evaluate((e) => getComputedStyle(e).color),
    await scaledPage.locator("body").evaluate((e) => getComputedStyle(e).color),
  );
  pass(
    "200% app text scaling at 640px, reduced-motion callback and themed retry control",
  );
  await scaled.close();
  phase = "real-login";
  const ctx = await context(),
    page = await ctx.newPage();
  await page.goto(preview.origin);
  await page.locator("#remember-login").uncheck();
  await page.locator("#signin").click();
  await page
    .getByRole("textbox", { name: "Email address", exact: true })
    .fill(credentials.email);
  await page
    .getByRole("textbox", { name: "Password", exact: true })
    .fill(credentials.password);
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  phase = "oauth-return";
  await page.locator("#name").waitFor({ state: "visible" });
  await page.locator("#name").fill("Sign-in verification");
  await page.getByRole("button", { name: "Connect this device" }).click();
  await page.waitForFunction(
    () =>
      document.getElementById("connection-label").textContent === "Connected",
  );
  assert.equal(
    await page.evaluate(
      () =>
        !!sessionStorage.getItem("mluva-auth") &&
        !localStorage.getItem("mluva-auth"),
    ),
    true,
  );
  pass(
    "Real Cognito email/password + S256 PKCE callback → named device → local API workspace; session-only choice honored",
  );
  phase = "native-callback";
  const verifier = randomBytes(32).toString("base64url"),
    state = randomBytes(24).toString("base64url");
  const authorize = new URL(`${login}/oauth2/authorize`);
  authorize.search = new URLSearchParams({
    response_type: "code",
    client_id: resource.client,
    redirect_uri: "mluva://account",
    scope: "openid email",
    state,
    code_challenge: createHash("sha256").update(verifier).digest("base64url"),
    code_challenge_method: "S256",
  });
  let destination = authorize.href;
  for (let i = 0; i < 8 && !destination.startsWith("mluva:"); i++) {
    const response = await ctx.request.get(destination, { maxRedirects: 0 });
    assert.ok(
      response.status() >= 300 && response.status() < 400,
      "Expected authenticated native redirect",
    );
    destination = new URL(response.headers().location, destination).href;
  }
  const native = new URL(destination);
  assert.equal(native.protocol, "mluva:");
  assert.equal(native.searchParams.get("state"), state);
  const token = await ctx.request.post(`${login}/oauth2/token`, {
    form: {
      grant_type: "authorization_code",
      client_id: resource.client,
      redirect_uri: "mluva://account",
      code: native.searchParams.get("code"),
      code_verifier: verifier,
    },
  });
  assert.equal(token.status(), 200);
  const nativeTokens = await token.json();
  await ctx.request.post(`${login}/oauth2/revoke`, {
    form: { client_id: resource.client, token: nativeTokens.refresh_token },
  });
  pass(
    "Native mluva://account redirect preserves state, exchanges S256 PKCE code and revokes temporary token",
  );
  phase = "logout";
  await page.locator("#signout").click();
  await page.locator("#signin").waitFor({ state: "visible" });
  assert.equal(
    await page.evaluate(
      () =>
        sessionStorage.getItem("mluva-auth") ||
        localStorage.getItem("mluva-auth"),
    ),
    null,
  );
  pass("Real logout returns to branded welcome with credentials removed");
  await ctx.close();
  assert.deepEqual(errors, []);
  await writeFile(
    `${output}/browser-receipt.json`,
    JSON.stringify(
      {
        checks,
        metrics,
        uncaughtErrors: errors.length,
        scope:
          "Real isolated Cognito pool, real browser OAuth, local application API peer. Synthetic account; no email, audio or production writes.",
      },
      null,
      2,
    ),
  );
} catch (error) {
  // Never print Playwright call logs: form-fill arguments contain the synthetic password.
  console.error(`FAIL phase=${phase} type=${error.name}`);
  const failedPage = browser
    .contexts()
    .flatMap((c) => c.pages())
    .at(-1);
  if (failedPage) {
    console.error(`Location: ${new URL(failedPage.url()).pathname}`);
    const alerts = await failedPage
      .locator("[role=alert], #callback-status")
      .allTextContents();
    console.error(
      JSON.stringify(alerts)
        .replaceAll(credentials.password, "[redacted]")
        .replaceAll(credentials.email, "[synthetic account]"),
    );
    await failedPage.screenshot({
      path: `${evidence}/failure.png`,
      mask: [failedPage.locator("input, textarea")],
    });
  }
  if (phase.startsWith("provider-") || phase === "laptop-light") {
    const page = browser
      .contexts()
      .flatMap((c) => c.pages())
      .at(-1);
    if (page)
      await page.screenshot({
        path: `${evidence}/provider-startup-failure.png`,
      });
  }
  process.exitCode = 1;
} finally {
  await browser.close();
  await preview.close();
}
