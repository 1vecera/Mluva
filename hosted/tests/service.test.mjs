import { test } from "node:test";
import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { createService, Failure, Conflict } from "../server/service.mjs";
import { memoryStore } from "./memory-store.mjs";
function fixture(enabled = true) {
  const store = memoryStore(),
    sent = [],
    clock = { time: 1791650000000 },
    origin = "https://test.example";
  let paid = 0;
  const service = createService({
    store,
    origin,
    speechEnabled: enabled,
    now: () => clock.time,
    send: async (id, payload) => {
      sent.push({ id, ...payload });
    },
    mintSpeechToken: async () => {
      paid++;
      return "temporary";
    },
  });
  const http = (owner, method, path, body = {}, query = {}) =>
    service.http({
      owner,
      expires: Math.floor(clock.time / 1000) + 3600,
      method,
      path,
      body,
      query,
    });
  const device = async (owner, name = "Phone") => {
    const id = randomUUID();
    await http(owner, "POST", "/devices", { id, name, kind: "phone" });
    return id;
  };
  const connect = async (owner, deviceId, id = randomUUID()) => {
    const { ticket } = await http(owner, "POST", "/ticket", { deviceId });
    await service.socket({
      requestContext: { routeKey: "$connect", connectionId: id },
      headers: { origin },
      queryStringParameters: { ticket },
    });
    return id;
  };
  const stream = (connectionId, message) =>
    service.socket({
      requestContext: { routeKey: "$default", connectionId },
      body: JSON.stringify({ action: "stream", ...message }),
    });
  return {
    store,
    sent,
    clock,
    origin,
    service,
    http,
    device,
    connect,
    stream,
    paid: () => paid,
  };
}
const status = (code) => (error) =>
  error instanceof Failure && error.status === code;
test("bidirectional streams isolate accounts and partial text never enters durable history", async () => {
  const f = fixture(),
    phone = await f.device("alice"),
    laptop = await f.device("alice", "Laptop"),
    stranger = await f.device("bob");
  const source = await f.connect("alice", phone),
    destination = await f.connect("alice", laptop),
    other = await f.connect("bob", stranger),
    sessionId = randomUUID();
  await f.stream(source, {
    sessionId,
    sequence: 0,
    phase: "started",
    targets: [laptop],
  });
  await f.stream(source, {
    sessionId,
    sequence: 1,
    phase: "partial",
    text: "still changing",
    targets: [laptop],
  });
  await f.stream(source, {
    sessionId,
    sequence: 2,
    phase: "segment",
    text: "settled words",
    targets: [laptop],
  });
  assert.equal(
    f.sent.filter((e) => e.id === destination && e.type === "stream").length,
    3,
  );
  assert.equal(f.sent.filter((e) => e.id === other).length, 0);
  assert.deepEqual((await f.http("alice", "GET", "/history")).entries, []);
  assert.ok(
    !JSON.stringify([...f.store.rows.values()]).includes("still changing"),
  );
  await assert.rejects(
    f.stream(source, {
      sessionId,
      sequence: 3,
      phase: "segment",
      targets: [stranger],
    }),
    status(403),
  );
  await assert.rejects(
    f.stream(destination, { sessionId, sequence: 3, phase: "stopped" }),
    status(403),
  );
  const reverse = randomUUID();
  await f.stream(destination, {
    sessionId: reverse,
    sequence: 0,
    phase: "started",
    targets: [phone],
  });
  assert.ok(f.sent.some((e) => e.id === source && e.sessionId === reverse));
});
test("connection tickets are one-use, short-lived, origin checked and account bound", async () => {
  const f = fixture(),
    id = await f.device("alice");
  await assert.rejects(
    f.http("bob", "POST", "/ticket", { deviceId: id }),
    status(403),
  );
  const { ticket } = await f.http("alice", "POST", "/ticket", { deviceId: id });
  const event = {
    requestContext: { routeKey: "$connect", connectionId: "first" },
    headers: { origin: "https://wrong.example" },
    queryStringParameters: { ticket },
  };
  await assert.rejects(f.service.socket(event), status(403));
  event.headers.origin = f.origin;
  await f.service.socket(event);
  event.requestContext.connectionId = "replay";
  await assert.rejects(f.service.socket(event), status(401));
  const next = await f.http("alice", "POST", "/ticket", { deviceId: id });
  f.clock.time += 61000;
  await assert.rejects(
    f.service.socket({ ...event, queryStringParameters: next }),
    status(401),
  );
});
test("device removal revokes existing connections and unused tickets", async () => {
  const f = fixture(),
    phone = await f.device("alice"),
    id = await f.connect("alice", phone),
    ticket = await f.http("alice", "POST", "/ticket", { deviceId: phone });
  await f.http("alice", "DELETE", `/devices/${phone}`);
  assert.ok(f.sent.some((e) => e.id === id && e.type === "revoked"));
  await assert.rejects(
    f.stream(id, { sessionId: randomUUID(), sequence: 0, phase: "started" }),
    status(401),
  );
  await assert.rejects(
    f.service.socket({
      requestContext: { routeKey: "$connect", connectionId: "late" },
      headers: { origin: f.origin },
      queryStringParameters: ticket,
    }),
    status(403),
  );
  assert.equal((await f.http("alice", "GET", "/devices")).devices.length, 0);
});
test("ordered updates suppress duplicates and reject missing, post-stop and expired updates", async () => {
  const f = fixture(),
    phone = await f.device("alice"),
    source = await f.connect("alice", phone),
    sessionId = randomUUID(),
    first = { sessionId, sequence: 0, phase: "started" };
  await f.stream(source, first);
  const count = f.sent.filter((e) => e.type === "stream").length;
  await f.stream(source, first);
  assert.equal(f.sent.filter((e) => e.type === "stream").length, count);
  await assert.rejects(
    f.stream(source, { sessionId, sequence: 2, phase: "segment" }),
    status(409),
  );
  await f.stream(source, { sessionId, sequence: 1, phase: "stopped" });
  await assert.rejects(
    f.stream(source, { sessionId, sequence: 2, phase: "segment" }),
    status(409),
  );
  f.clock.time += 3600001;
  await assert.rejects(f.stream(source, first), status(401));
});
test("history originals survive retries and deletion cannot resurrect or cross accounts", async () => {
  const f = fixture(),
    deviceId = await f.device("alice"),
    id = randomUUID(),
    body = { deviceId, id, rawText: "rough original", text: "edited text" };
  await f.http("alice", "POST", "/history", body);
  await f.http("alice", "POST", "/history", { ...body, rawText: "changed" });
  const history = await f.http("alice", "GET", "/history");
  assert.equal(history.entries.length, 1);
  assert.equal(history.entries[0].rawText, "rough original");
  assert.deepEqual((await f.http("bob", "GET", "/history")).entries, []);
  await f.http("bob", "DELETE", `/history/${id}`);
  assert.equal((await f.http("alice", "GET", "/history")).entries.length, 1);
  await f.http("alice", "DELETE", `/history/${id}`);
  assert.equal((await f.http("alice", "GET", "/history")).entries.length, 0);
  await assert.rejects(f.http("alice", "POST", "/history", body), status(410));
  assert.ok(
    !JSON.stringify([...f.store.rows.values()]).includes("rough original"),
  );
});
test("pagination stays account-scoped and oversized Unicode is rejected before persistence", async () => {
  const f = fixture(),
    deviceId = await f.device("alice");
  for (let i = 0; i < 33; i++) {
    f.clock.time++;
    await f.http("alice", "POST", "/history", {
      deviceId,
      id: randomUUID(),
      rawText: `original ${i}`,
      text: `draft ${i}`,
    });
  }
  const first = await f.http("alice", "GET", "/history");
  assert.equal(first.entries.length, 30);
  assert.equal(
    (await f.http("alice", "GET", "/history", {}, { cursor: first.cursor }))
      .entries.length,
    3,
  );
  assert.deepEqual(
    (await f.http("bob", "GET", "/history", {}, { cursor: first.cursor }))
      .entries,
    [],
  );
  await assert.rejects(
    f.http("alice", "POST", "/history", {
      deviceId,
      id: randomUUID(),
      rawText: "😀".repeat(23000),
      text: "x",
    }),
    status(400),
  );
});
test("speech is opt-in, requires consent and caps paid token creation", async () => {
  const disabled = fixture(false),
    deviceId = await disabled.device("alice");
  await assert.rejects(
    disabled.http("alice", "POST", "/speech-token", {
      deviceId,
      consent: true,
    }),
    status(503),
  );
  assert.equal(disabled.paid(), 0);
  const f = fixture(),
    phone = await f.device("alice");
  await assert.rejects(
    f.http("alice", "POST", "/speech-token", { deviceId: phone }),
    status(400),
  );
  for (let i = 0; i < 5; i++)
    assert.equal(
      (
        await f.http("alice", "POST", "/speech-token", {
          deviceId: phone,
          consent: true,
        })
      ).token,
      "temporary",
    );
  await assert.rejects(
    f.http("alice", "POST", "/speech-token", {
      deviceId: phone,
      consent: true,
    }),
    status(429),
  );
  assert.equal(f.paid(), 5);
});
test("atomic admission bounds active devices and rejects stale writes", async () => {
  const f = fixture();
  for (let i = 0; i < 12; i++) await f.device("alice");
  await assert.rejects(f.device("alice"), status(409));
  const first = (await f.http("alice", "GET", "/devices")).devices[0];
  await f.http("alice", "DELETE", `/devices/${first.id}`);
  await f.device("alice");
  assert.equal((await f.http("alice", "GET", "/devices")).devices.length, 12);
  await assert.rejects(
    f.store.commit([
      { put: { pk: "x", sk: "y", count: 1 }, matches: { count: 0 } },
    ]),
    Conflict,
  );
});
