const encoder = new TextEncoder();
const base64url = (value) =>
  btoa(String.fromCharCode(...new Uint8Array(value)))
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replaceAll("=", "");
export async function signIn(config) {
  const verifier = base64url(crypto.getRandomValues(new Uint8Array(32))),
    state = base64url(crypto.getRandomValues(new Uint8Array(24)));
  const challenge = base64url(
    await crypto.subtle.digest("SHA-256", encoder.encode(verifier)),
  );
  sessionStorage.setItem(
    "mluva-login",
    JSON.stringify({ verifier, state, createdAt: Date.now() }),
  );
  const url = new URL("/oauth2/authorize", config.login);
  url.search = new URLSearchParams({
    response_type: "code",
    client_id: config.clientId,
    redirect_uri: `${location.origin}/callback.html`,
    scope: "openid email",
    state,
    code_challenge: challenge,
    code_challenge_method: "S256",
  });
  location.assign(url);
}
export async function finishSignIn(config) {
  const url = new URL(location.href),
    login = JSON.parse(sessionStorage.getItem("mluva-login") ?? "null");
  history.replaceState({}, "", "/callback.html"); // remove authorization code from the address immediately
  sessionStorage.removeItem("mluva-login");
  if (
    !login ||
    login.state !== url.searchParams.get("state") ||
    Date.now() - login.createdAt > 600000 ||
    !url.searchParams.get("code")
  )
    throw new Error(
      "Sign-in could not be verified. Return to Mluva and try again.",
    );
  const reply = await fetch(`${config.login}/oauth2/token`, {
    method: "POST",
    headers: { "content-type": "application/x-www-form-urlencoded" },
    body: new URLSearchParams({
      grant_type: "authorization_code",
      client_id: config.clientId,
      redirect_uri: `${location.origin}/callback.html`,
      code: url.searchParams.get("code"),
      code_verifier: login.verifier,
    }),
    signal: AbortSignal.timeout(15000),
  });
  if (!reply.ok)
    throw new Error("Sign-in did not finish. Return to Mluva and try again.");
  const payload = await reply.json();
  sessionStorage.setItem(
    "mluva-auth",
    JSON.stringify({
      ...payload,
      expires: Date.now() + payload.expires_in * 1000,
    }),
  );
}
export function account() {
  const auth = JSON.parse(sessionStorage.getItem("mluva-auth") ?? "null");
  if (!auth) return null;
  const value = auth.access_token.split(".")[1];
  try {
    return {
      ...auth,
      owner: JSON.parse(
        new TextDecoder().decode(
          Uint8Array.from(
            atob(value.replaceAll("-", "+").replaceAll("_", "/")),
            (c) => c.charCodeAt(0),
          ),
        ),
      ).sub,
    };
  } catch {
    return null;
  }
}
let refreshing;
export async function accessToken(config) {
  const auth = account();
  if (!auth) throw new Error("Sign in to connect your devices.");
  if (auth.expires > Date.now() + 60000) return auth.access_token;
  if (!refreshing)
    refreshing = (async () => {
      if (!auth.refresh_token)
        throw new Error(
          "Sign-in expired. Sign in again; your unsaved draft stays on this device.",
        );
      const reply = await fetch(`${config.login}/oauth2/token`, {
        method: "POST",
        headers: { "content-type": "application/x-www-form-urlencoded" },
        body: new URLSearchParams({
          grant_type: "refresh_token",
          client_id: config.clientId,
          refresh_token: auth.refresh_token,
        }),
        signal: AbortSignal.timeout(15000),
      });
      if (!reply.ok)
        throw new Error(
          "Sign-in expired. Sign in again; your unsaved draft stays on this device.",
        );
      const next = await reply.json();
      sessionStorage.setItem(
        "mluva-auth",
        JSON.stringify({
          ...auth,
          ...next,
          expires: Date.now() + next.expires_in * 1000,
        }),
      );
      return next.access_token;
    })().finally(() => {
      refreshing = undefined;
    });
  return refreshing;
}
export async function signOut(config) {
  const auth = account();
  sessionStorage.removeItem("mluva-auth");
  if (auth?.refresh_token)
    await fetch(`${config.login}/oauth2/revoke`, {
      method: "POST",
      headers: { "content-type": "application/x-www-form-urlencoded" },
      body: new URLSearchParams({
        token: auth.refresh_token,
        client_id: config.clientId,
      }),
      signal: AbortSignal.timeout(5000),
    }).catch(() => {});
  if (config.dev) location.assign("/");
  else {
    const url = new URL("/logout", config.login);
    url.search = new URLSearchParams({
      client_id: config.clientId,
      logout_uri: `${location.origin}/`,
    });
    location.assign(url);
  }
}
