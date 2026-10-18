import test from "node:test";
import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { createService, Conflict } from "../server/service.mjs";
import { memoryStore } from "./memory-store.mjs";
async function fixture() {
  const store = memoryStore(),
    sent = [];
  const service = createService({
    store,
    origin: "https://example.test",
    send: async (id, event) => sent.push({ id, ...event }),
    mintSpeechToken: async () => "unused",
  });
  const ids = { alice: randomUUID(), bob: randomUUID() };
  const api = (owner, method, path, body = {}) =>
    service.http({
      owner,
      session: ids[owner],
      expires: Math.floor(Date.now() / 1000) + 3600,
      method,
      path,
      body: { deviceId: ids[owner], ...body },
      query: { deviceId: ids[owner] },
    });
  for (const owner of Object.keys(ids))
    await api(owner, "POST", "/devices", {
      id: ids[owner],
      name: owner,
      kind: "laptop",
    });
  return { store, service, api, ids, sent };
}
const note = () => ({
  id: randomUUID(),
  rawText: "Immutable Czech original: Příliš žluťoučký kůň.",
  text: "Working source",
  deliveredText: "Delivered once",
  rewrites: [],
  segments: [],
  title: "Shared note",
  mode: "dictation",
});
const status = (n) => (e) => e.status === n;
test("revocation blocks the token family, guessed active IDs and re-enrollment while another login stays usable", async () => {
  const f = await fixture(),
    second = randomUUID();
  const login = (session, method, path, body = {}) =>
    f.service.http({
      owner: "alice",
      session,
      expires: Math.floor(Date.now() / 1000) + 3600,
      method,
      path,
      body,
      query: { deviceId: body.deviceId },
    });
  await login(second, "POST", "/devices", {
    id: second,
    name: "Second login",
    kind: "phone",
  });
  await assert.rejects(
    f.api("alice", "GET", "/history", { deviceId: second }),
    status(403),
  );
  await login(second, "DELETE", `/devices/${f.ids.alice}`);
  for (const [method, path, body] of [
    ["GET", "/devices", {}],
    ["GET", "/history", { deviceId: second }],
    ["POST", "/ticket", { deviceId: second }],
    [
      "POST",
      "/devices",
      { id: randomUUID(), name: "Replacement", kind: "phone" },
    ],
    ["DELETE", `/devices/${second}`, {}],
  ])
    await assert.rejects(login(f.ids.alice, method, path, body), status(403));
  // A refreshed access JWT has a new expiry but the same origin_jti.
  await assert.rejects(
    f.service.http({
      owner: "alice",
      session: f.ids.alice,
      expires: Math.floor(Date.now() / 1000) + 7200,
      method: "GET",
      path: "/devices",
    }),
    status(403),
  );
  assert.equal((await login(second, "GET", "/devices")).devices.length, 1);
  assert.deepEqual(
    (await login(second, "GET", "/history", { deviceId: second })).entries,
    [],
  );
  assert.equal((await f.api("bob", "GET", "/devices")).devices.length, 1);
});
test("revocation wins a concurrent new-device registration transaction", async () => {
  const f = await fixture(),
    replacement = randomUUID(),
    otherLogin = randomUUID();
  const other = (method, path, body = {}) =>
    f.service.http({
      owner: "alice",
      session: otherLogin,
      expires: Math.floor(Date.now() / 1000) + 3600,
      method,
      path,
      body,
    });
  await other("POST", "/devices", {
    id: otherLogin,
    name: "Trusted laptop",
    kind: "laptop",
  });
  const commit = f.store.commit;
  let injected = false;
  f.store.commit = async (ops) => {
    if (!injected && ops.some((op) => op.put?.sk === `DEVICE#${replacement}`)) {
      injected = true;
      await other("DELETE", `/devices/${f.ids.alice}`);
    }
    return commit(ops);
  };
  await assert.rejects(
    f.api("alice", "POST", "/devices", {
      id: replacement,
      name: "Racing registration",
      kind: "phone",
    }),
    Conflict,
  );
  assert.ok(injected);
  assert.equal(
    await f.store.get("USER#alice", `DEVICE#${replacement}`),
    undefined,
  );
});
test("legacy unbound devices cannot access account data or websocket tickets", async () => {
  const f = await fixture(),
    pk = "USER#alice",
    sk = `DEVICE#${f.ids.alice}`;
  const old = await f.store.get(pk, sk);
  delete old.authSession;
  await f.store.commit([{ put: old }]);
  await assert.rejects(f.api("alice", "GET", "/history"), status(403));
  await assert.rejects(f.api("alice", "POST", "/ticket"), status(403));
  await assert.rejects(
    f.api("alice", "POST", "/devices", {
      id: f.ids.alice,
      name: "Old device",
      kind: "phone",
    }),
    status(403),
  );
  assert.equal(
    (await f.api("alice", "GET", "/devices")).devices[0].currentSession,
    false,
  );
});
test("revision CAS rejects simultaneous edits; retries keep one note and immutable originals", async () => {
  const f = await fixture(),
    original = note();
  const first = await f.api("alice", "POST", "/history", original);
  const edit = {
    ...first,
    text: "Edited on native",
    baseRevision: 1,
    operationId: randomUUID(),
    rewrites: [
      {
        id: "2026-10-12T18:00:00Z",
        createdAt: "2026-10-12T18:00:00Z",
        instruction: "Polish",
        text: "Completed reply",
        model: "native-provider",
      },
    ],
  };
  const saved = await f.api("alice", "PUT", `/history/${original.id}`, edit);
  assert.equal(saved.revision, 2);
  assert.equal(saved.rawText, original.rawText);
  assert.equal(saved.deliveredText, original.deliveredText);
  const retry = await f.api("alice", "PUT", `/history/${original.id}`, edit);
  assert.equal(retry.revision, 2);
  assert.equal(retry.appliedRevision, 2);
  await assert.rejects(
    f.api("alice", "PUT", `/history/${original.id}`, {
      ...edit,
      text: "Web stale edit",
      operationId: randomUUID(),
    }),
    status(409),
  );
  await assert.rejects(
    f.api("alice", "PUT", `/history/${original.id}`, {
      ...edit,
      baseRevision: 2,
      rawText: "overwrite",
      operationId: randomUUID(),
    }),
    status(409),
  );
  assert.equal((await f.api("alice", "GET", "/history")).entries.length, 1);
  assert.equal(
    (await f.api("alice", "GET", `/history/${original.id}`)).rewrites[0].text,
    "Completed reply",
  );
  assert.deepEqual((await f.api("bob", "GET", "/history")).entries, []);
  await assert.rejects(
    f.api("bob", "GET", `/history/${original.id}`),
    status(404),
  );
});
test("deletion wins against delayed first saves, updates, retries and other account IDs", async () => {
  const f = await fixture(),
    original = note();
  await f.api("alice", "DELETE", `/history/${original.id}`);
  await assert.rejects(
    f.api("alice", "POST", "/history", original),
    status(410),
  );
  const saved = await f.api("bob", "POST", "/history", original);
  const edit = {
    ...saved,
    text: "Saved edit",
    baseRevision: 1,
    operationId: randomUUID(),
  };
  await f.api("bob", "PUT", `/history/${original.id}`, edit);
  await f.api("bob", "DELETE", `/history/${original.id}`);
  await assert.rejects(
    f.api("bob", "PUT", `/history/${original.id}`, edit),
    status(410),
  );
  assert.equal(
    JSON.stringify([...f.store.rows.values()]).includes("Working source"),
    false,
  );
  assert.equal(
    JSON.stringify([...f.store.rows.values()]).includes(original.rawText),
    false,
  );
});
test("removed devices cannot read, save or mint tickets with an otherwise live JWT", async () => {
  const f = await fixture(),
    original = note();
  await f.api("alice", "POST", "/history", original);
  await f.api("alice", "DELETE", `/devices/${f.ids.alice}`);
  for (const [method, path, body] of [
    ["GET", "/history", {}],
    ["GET", `/history/${original.id}`, {}],
    ["POST", "/history", original],
    ["POST", "/ticket", {}],
    ["DELETE", `/history/${original.id}`, {}],
  ]) {
    await assert.rejects(f.api("alice", method, path, body), status(403));
  }
});
test("explicit empty receiver list delivers nowhere; duplicate retries retain the original payload", async () => {
  const f = await fixture(),
    deviceId = f.ids.alice;
  const ticket = await f.api("alice", "POST", "/ticket");
  await f.service.socket({
    requestContext: { routeKey: "$connect", connectionId: "native" },
    headers: { origin: "https://example.test" },
    queryStringParameters: ticket,
  });
  const id = randomUUID(),
    stream = (body) =>
      f.service.socket({
        requestContext: { routeKey: "$default", connectionId: "native" },
        body: JSON.stringify({
          action: "stream",
          sessionId: id,
          sequence: 0,
          phase: "started",
          text: "",
          targets: [],
          ...body,
        }),
      });
  await stream({});
  assert.equal(f.sent.filter((x) => x.type === "stream").length, 0);
  await stream({});
  await assert.rejects(stream({ text: "different retry" }), status(409));
  assert.equal(
    JSON.stringify([...f.store.rows.values()]).includes("different retry"),
    false,
  );
  assert.equal(deviceId.length, 36);
});

test("a late acknowledged retry cannot hide a newer edit; merge is atomic and preserves sources", async () => {
  const f = await fixture();
  const a = await f.api("alice", "POST", "/history", note());
  const b = await f.api("alice", "POST", "/history", note());
  const edit = {
    ...a,
    text: "first edit",
    baseRevision: 1,
    operationId: randomUUID(),
  };
  const revision2 = await f.api("alice", "PUT", `/history/${a.id}`, edit);
  const revision3 = await f.api("alice", "PUT", `/history/${a.id}`, {
    ...revision2,
    text: "newer edit",
    baseRevision: 2,
    operationId: randomUUID(),
  });
  await assert.rejects(
    f.api("alice", "PUT", `/history/${a.id}`, edit),
    status(409),
  );
  const merge = {
    ...revision3,
    text: `${revision3.text}\n\n${b.text}`,
    segments: [{ id: b.id, rawText: b.rawText }],
    baseRevision: 3,
    operationId: randomUUID(),
    mergedSources: [{ id: b.id, baseRevision: 1 }],
  };
  const merged = await f.api("alice", "PUT", `/history/${a.id}`, merge);
  assert.equal(merged.revision, 4);
  assert.deepEqual(merged.segments, [{ id: b.id, rawText: b.rawText }]);
  assert.equal((await f.api("alice", "GET", "/history")).entries.length, 1);
  await assert.rejects(f.api("alice", "GET", `/history/${b.id}`), status(410));
  assert.equal(
    (await f.api("alice", "PUT", `/history/${a.id}`, merge)).revision,
    4,
  );
  await assert.rejects(f.api("alice", "POST", "/history", b), status(410));
  const c = await f.api("alice", "POST", "/history", note());
  const staleMerge = {
    ...merged,
    text: "merge",
    segments: [...merged.segments, { id: c.id, rawText: c.rawText }],
    baseRevision: 4,
    operationId: randomUUID(),
    mergedSources: [{ id: c.id, baseRevision: 0 }],
  };
  await assert.rejects(
    f.api("alice", "PUT", `/history/${a.id}`, staleMerge),
    status(409),
  );
  assert.equal((await f.api("alice", "GET", `/history/${a.id}`)).revision, 4);
  assert.equal((await f.api("alice", "GET", `/history/${c.id}`)).revision, 1);
});
