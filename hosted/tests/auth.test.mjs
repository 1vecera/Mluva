import test from "node:test";
import assert from "node:assert/strict";
import {
  account,
  accessToken,
  signIn,
  finishSignIn,
  signOut,
} from "../web/auth.mjs";
const storage = () => {
  const values = new Map();
  return {
    getItem: (k) => values.get(k) ?? null,
    setItem: (k, v) => values.set(k, String(v)),
    removeItem: (k) => values.delete(k),
    clear: () => values.clear(),
  };
};
test("PKCE is verified, remembered login survives tab storage loss, and sign-out clears both stores", async () => {
  globalThis.sessionStorage = storage();
  globalThis.localStorage = storage();
  globalThis.location = {
    origin: "https://app.example",
    href: "",
    assign(url) {
      this.href = String(url);
    },
  };
  globalThis.history = { replaceState() {} };
  const config = {
    login: "https://login.example",
    clientId: "client",
    dev: true,
  };
  await signIn(config, true);
  const authorize = new URL(location.href);
  assert.equal(authorize.searchParams.get("code_challenge_method"), "S256");
  assert.equal(
    authorize.searchParams.get("redirect_uri"),
    "https://app.example/callback.html",
  );
  const login = JSON.parse(sessionStorage.getItem("mluva-login"));
  const token = `header.${Buffer.from(JSON.stringify({ sub: "alice" })).toString("base64url")}.signature`;
  globalThis.fetch = async () => ({
    ok: true,
    json: async () => ({
      access_token: token,
      refresh_token: "synthetic-refresh",
      expires_in: 3600,
    }),
  });
  location.href = `https://app.example/callback.html?code=one-use&state=${login.state}`;
  await finishSignIn(config);
  sessionStorage.clear();
  assert.equal(account().owner, "alice");
  assert.ok(localStorage.getItem("mluva-auth"));
  await signOut(config);
  assert.equal(account(), null);
  assert.equal(localStorage.getItem("mluva-auth"), null);
  for (const crossTab of [false, true]) {
    localStorage.setItem(
      "mluva-auth",
      JSON.stringify({
        access_token: token,
        refresh_token: "synthetic-refresh",
        expires: 0,
      }),
    );
    let release;
    globalThis.fetch = async (url) =>
      url.endsWith("/oauth2/token")
        ? new Promise((resolve) => {
            release = resolve;
          })
        : { ok: true };
    const refresh = accessToken(config);
    if (crossTab) localStorage.removeItem("mluva-auth");
    else await signOut(config);
    release({
      ok: true,
      json: async () => ({ access_token: token, expires_in: 3600 }),
    });
    await assert.rejects(refresh, /Sign-in changed/);
    assert.equal(
      account(),
      null,
      "late refresh must not resurrect a signed-out account",
    );
  }
  await signIn(config, false);
  location.href = "https://app.example/callback.html?code=code&state=wrong";
  await assert.rejects(finishSignIn(config), /could not be verified/);
  assert.equal(account(), null);
});
