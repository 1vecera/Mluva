import { createHash, randomBytes } from "node:crypto";

export class Failure extends Error {
  constructor(status, message) {
    super(message);
    this.status = status;
  }
}
export class Conflict extends Error {}
const fail = (status, message) => {
  throw new Failure(status, message);
};
const hash = (value) => createHash("sha256").update(value).digest("hex");
const key = (pk, sk) => ({ pk, sk });
const uuid = (value) => {
  if (
    typeof value !== "string" ||
    !/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(
      value,
    )
  )
    fail(400, "Invalid device or recording ID.");
  return value.toLowerCase();
};
const text = (value, max = 90000) => {
  if (
    typeof value !== "string" ||
    Buffer.byteLength(value) > max ||
    value.includes("\0")
  )
    fail(400, "Text is missing or too long. Export it before continuing.");
  return value;
};
const publicDevice = ({ id, name, kind, createdAt, revoked }) => ({
  id,
  name,
  kind,
  createdAt,
  revoked: Boolean(revoked),
});
const publicHistory = ({
  id,
  title,
  rawText,
  text: content,
  createdAt,
  deviceId,
}) => ({ id, title, rawText, text: content, createdAt, deviceId });

// The service knows account identities, never login passwords or provider API keys.
export function createService({
  store,
  send,
  mintSpeechToken,
  origin,
  speechEnabled = false,
  now = () => Date.now(),
}) {
  const seconds = () => Math.floor(now() / 1000);
  const partition = (owner) => `USER#${owner}`;
  const activeDevice = async (owner, id) => {
    const item = await store.get(partition(owner), `DEVICE#${uuid(id)}`);
    if (!item || item.revoked)
      fail(
        403,
        "This device was removed. Sign out and connect it again as a new device.",
      );
    return item;
  };
  const deviceCheck = (owner, device) => ({
    check: key(partition(owner), `DEVICE#${device.id}`),
    matches: { revoked: false },
  });
  async function connections(owner) {
    return (await store.list(partition(owner), "CONN#", 100)).items.filter(
      (row) => row.expires > seconds(),
    );
  }
  async function disconnect(row) {
    await store.commit([
      { delete: key(`CONN#${row.connectionId}`, "META") },
      { delete: key(partition(row.owner), `CONN#${row.connectionId}`) },
    ]);
  }
  async function notify(owner, event, targets = []) {
    const rows = await connections(owner);
    const delivered = await Promise.all(
      rows
        .filter((row) => !targets.length || targets.includes(row.deviceId))
        .map(async (row) => {
          const device = await store.get(
            partition(owner),
            `DEVICE#${row.deviceId}`,
          );
          if (!device || device.revoked) {
            await disconnect(row);
            return;
          }
          try {
            await send(row.connectionId, event);
            return true;
          } catch (error) {
            if (
              error.statusCode === 410 ||
              error.$metadata?.httpStatusCode === 410
            )
              await disconnect(row);
            else throw error;
          }
        }),
    );
    return delivered.filter(Boolean).length;
  }
  async function http({ owner, expires, method, path, body = {}, query = {} }) {
    if (!owner || !expires || expires <= seconds())
      fail(401, "Sign in to connect your devices.");
    const pk = partition(owner);
    if (path === "/devices" && method === "GET") {
      const rows = (await store.list(pk, "DEVICE#", 100)).items.filter(
        (row) => !row.revoked,
      );
      const live = await connections(owner);
      return {
        devices: rows.map((row) => ({
          ...publicDevice(row),
          online: live.some((c) => c.deviceId === row.id),
        })),
        speechEnabled,
      };
    }
    if (path === "/devices" && method === "POST") {
      const id = uuid(body.id),
        name = text(body.name, 120).trim();
      if (
        !name ||
        name.length > 40 ||
        !["phone", "laptop", "tablet", "other"].includes(body.kind)
      )
        fail(400, "Give this device a short name and choose its type.");
      if (await store.get(pk, `REMOVED#${id}`))
        fail(403, "This device was removed. Connect a new device.");
      const existing = await store.get(pk, `DEVICE#${id}`);
      if (existing?.revoked)
        fail(403, "This device was removed. Connect a new device.");
      const row = {
        pk,
        sk: `DEVICE#${id}`,
        id,
        name,
        kind: body.kind,
        revoked: false,
        createdAt: existing?.createdAt ?? now(),
      };
      if (!existing) {
        const count = await store.get(pk, "DEVICE_COUNT");
        if ((count?.count ?? 0) >= 12)
          fail(
            409,
            "You have 12 devices. Remove an old device to connect this one.",
          );
        await store.commit([
          { put: row, absent: true },
          {
            put: { pk, sk: "DEVICE_COUNT", count: (count?.count ?? 0) + 1 },
            ...(count ? { matches: { count: count.count } } : { absent: true }),
          },
        ]);
      } else await store.commit([{ put: row, matches: { revoked: false } }]);
      return publicDevice(row);
    }
    if (path.startsWith("/devices/") && method === "DELETE") {
      const device = await activeDevice(owner, path.slice(9));
      const count = await store.get(pk, "DEVICE_COUNT");
      await store.commit([
        { delete: key(pk, `DEVICE#${device.id}`), matches: { revoked: false } },
        {
          put: { pk, sk: `REMOVED#${device.id}`, removedAt: now() },
          absent: true,
        },
        {
          put: { pk, sk: "DEVICE_COUNT", count: Math.max(0, count.count - 1) },
          matches: { count: count.count },
        },
      ]);
      const rows = await connections(owner);
      await Promise.all(
        rows
          .filter((row) => row.deviceId === device.id)
          .map(async (row) => {
            await send(row.connectionId, { type: "revoked" }).catch(() => {});
            await disconnect(row);
          }),
      );
      await notify(owner, { type: "devices.changed" });
      return { removed: true };
    }
    if (path === "/ticket" && method === "POST") {
      const device = await activeDevice(owner, body.deviceId);
      const ticket = randomBytes(32).toString("base64url");
      await store.commit([
        deviceCheck(owner, device),
        {
          put: {
            pk: `TICKET#${hash(ticket)}`,
            sk: "META",
            owner,
            deviceId: device.id,
            expires: Math.min(expires, seconds() + 60),
            authorizationExpires: expires,
          },
          absent: true,
        },
      ]);
      return { ticket };
    }
    if (path === "/speech-token" && method === "POST") {
      if (!speechEnabled)
        fail(
          503,
          "Cloud speech is not enabled on this host. You can still send text, or use your native Mluva recorder.",
        );
      const device = await activeDevice(owner, body.deviceId);
      if (body.consent !== true)
        fail(400, "Confirm that microphone audio will go to ElevenLabs.");
      const minute = Math.floor(seconds() / 60),
        counter = await store.get(pk, `SPEECH#${minute}`);
      if ((counter?.count ?? 0) >= 5)
        fail(429, "Too many speech starts. Wait a minute before trying again.");
      await store.commit([
        deviceCheck(owner, device),
        {
          put: {
            pk,
            sk: `SPEECH#${minute}`,
            count: (counter?.count ?? 0) + 1,
            expires: seconds() + 120,
          },
          ...(counter
            ? { matches: { count: counter.count } }
            : { absent: true }),
        },
      ]);
      return { token: await mintSpeechToken() };
    }
    if (path === "/history" && method === "GET") {
      const cursor = query.cursor;
      if (
        cursor &&
        (typeof cursor !== "string" ||
          !/^HISTORY#[0-9]{13}#[0-9a-f-]{36}$/.test(cursor))
      )
        fail(400, "Invalid history page.");
      const page = await store.list(pk, "HISTORY#", 30, cursor, true);
      return { entries: page.items.map(publicHistory), cursor: page.cursor };
    }
    if (path === "/history" && method === "POST") {
      const device = await activeDevice(owner, body.deviceId);
      const id = uuid(body.id),
        rawText = text(body.rawText),
        content = text(body.text);
      if (!content.trim() && !rawText.trim())
        fail(400, "There are no words to save yet.");
      const receipt = await store.get(pk, `RECORDING#${id}`);
      if (receipt) {
        const saved = await store.get(pk, receipt.historyKey);
        if (!saved) fail(410, "This recording was deleted.");
        return publicHistory(saved);
      }
      const createdAt = now(),
        sk = `HISTORY#${createdAt}#${id}`;
      const row = {
        pk,
        sk,
        id,
        title: text(
          body.title ?? content.trim().split("\n")[0].slice(0, 80),
          320,
        ),
        rawText,
        text: content,
        createdAt,
        deviceId: device.id,
      };
      await store.commit([
        deviceCheck(owner, device),
        { put: { pk, sk: `RECORDING#${id}`, historyKey: sk }, absent: true },
        { put: row, absent: true },
      ]);
      await notify(owner, { type: "history.changed", id }).catch(() => {}); // saved receipt survives fan-out failure
      return publicHistory(row);
    }
    if (path.startsWith("/history/") && method === "DELETE") {
      const id = uuid(path.slice(9)),
        receipt = await store.get(pk, `RECORDING#${id}`);
      if (receipt) {
        // Retain a content-free tombstone so a late retry cannot resurrect deleted text.
        await store.commit([{ delete: key(pk, receipt.historyKey) }]);
        await notify(owner, { type: "history.changed", id }).catch(() => {});
      }
      return { removed: true };
    }
    fail(404, "This action is unavailable.");
  }
  async function socket(event) {
    const { routeKey, connectionId } = event.requestContext;
    if (routeKey === "$connect") {
      if ((event.headers?.origin ?? event.headers?.Origin) !== origin)
        fail(403, "Open Mluva from its configured address.");
      const ticket = event.queryStringParameters?.ticket;
      if (typeof ticket !== "string" || !/^[A-Za-z0-9_-]{43}$/.test(ticket))
        fail(401, "Connect from a signed-in Mluva device.");
      const ticketKey = key(`TICKET#${hash(ticket)}`, "META"),
        row = await store.get(ticketKey.pk, ticketKey.sk);
      if (!row || row.expires <= seconds())
        fail(401, "Connection ticket expired. Reconnect.");
      const device = await activeDevice(row.owner, row.deviceId);
      if ((await connections(row.owner)).length >= 20)
        fail(429, "Too many connected windows. Close an old Mluva tab.");
      const connection = {
        owner: row.owner,
        deviceId: device.id,
        connectionId,
        expires: Math.min(row.authorizationExpires, seconds() + 7200),
      };
      await store.commit([
        deviceCheck(row.owner, device),
        { delete: ticketKey, matches: { expires: row.expires } },
        {
          put: { ...connection, pk: `CONN#${connectionId}`, sk: "META" },
          absent: true,
        },
        {
          put: {
            ...connection,
            pk: partition(row.owner),
            sk: `CONN#${connectionId}`,
          },
          absent: true,
        },
      ]);
      return;
    }
    const connection = await store.get(`CONN#${connectionId}`, "META");
    if (routeKey === "$disconnect") {
      if (connection) await disconnect(connection);
      return;
    }
    if (!connection || connection.expires <= seconds())
      fail(401, "Connection expired. Sign in again.");
    const device = await activeDevice(connection.owner, connection.deviceId);
    if (Buffer.byteLength(event.body ?? "") > 24000)
      fail(413, "Live update is too large. Save or export the text.");
    let message;
    try {
      message = JSON.parse(event.body);
    } catch {
      fail(400, "Invalid live update.");
    }
    if (!message || typeof message !== "object" || Array.isArray(message))
      fail(400, "Invalid live update.");
    if (message.action === "hello") {
      await notify(connection.owner, { type: "devices.changed" });
      return;
    }
    if (message.action === "ping") {
      await send(connectionId, { type: "pong", sentAt: message.sentAt });
      return;
    }
    if (
      message.action !== "stream" ||
      !["started", "partial", "segment", "stopped"].includes(message.phase)
    )
      fail(400, "Invalid live action.");
    const sessionId = uuid(message.sessionId),
      content = text(message.text ?? "", 16000);
    if (
      !Number.isSafeInteger(message.sequence) ||
      message.sequence < 0 ||
      message.sequence > 1000000
    )
      fail(400, "Invalid live sequence.");
    const targets = message.targets ?? [];
    if (!Array.isArray(targets) || targets.length > 12)
      fail(400, "Invalid destination devices.");
    for (const target of targets) await activeDevice(connection.owner, target);
    const pk = partition(connection.owner),
      sk = `SESSION#${sessionId}`,
      previous = await store.get(pk, sk);
    if (previous && previous.deviceId !== device.id)
      fail(403, "This recording belongs to a different device.");
    if (previous && message.sequence <= previous.sequence) {
      await send(connectionId, {
        type: "ack",
        sessionId,
        sequence: message.sequence,
      });
      return;
    }
    if (!previous && (message.phase !== "started" || message.sequence !== 0))
      fail(409, "Start a new live recording after reconnecting.");
    if (
      previous &&
      (previous.phase === "stopped" ||
        message.sequence !== previous.sequence + 1 ||
        message.phase === "started")
    )
      fail(
        409,
        "Live updates arrived out of order. Your words are kept on this device.",
      );
    await store.commit([
      deviceCheck(connection.owner, device),
      {
        put: {
          pk,
          sk,
          deviceId: device.id,
          sequence: message.sequence,
          phase: message.phase,
          expires: seconds() + 86400,
        },
        ...(previous
          ? { matches: { sequence: previous.sequence } }
          : { absent: true }),
      },
    ]);
    // Partial recognition never goes into durable history or clipboard.
    const receivers = await notify(
      connection.owner,
      {
        type: "stream",
        sessionId,
        sequence: message.sequence,
        phase: message.phase,
        text: content,
        deviceId: device.id,
        deviceName: device.name,
      },
      targets,
    );
    await send(connectionId, {
      type: "ack",
      sessionId,
      sequence: message.sequence,
      receivers,
    });
  }
  return { http, socket };
}
