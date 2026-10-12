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
const publicHistory = (row) => ({
  id: row.id,
  title: row.title,
  rawText: row.rawText,
  text: row.text,
  createdAt: row.createdAt,
  deviceId: row.deviceId,
  revision: row.revision ?? 1,
  deliveredText: row.deliveredText ?? row.text,
  mode: row.mode ?? "dictation",
  rewrites: row.rewrites ?? [],
  segments: row.segments ?? [],
  updatedAt: row.updatedAt ?? row.createdAt,
});
function documentFields(body) {
  const rewrites = body.rewrites ?? [],
    segments = body.segments ?? [];
  if (
    !Array.isArray(rewrites) ||
    rewrites.length > 100 ||
    !Array.isArray(segments) ||
    segments.length > 100 ||
    [...rewrites, ...segments].some((r) => !r || typeof r !== "object")
  )
    fail(400, "This conversation is full. Export it before continuing.");
  const fields = {
    title: text(
      body.title ?? body.text.trim().split("\n")[0].slice(0, 80),
      320,
    ),
    text: text(body.text),
    deliveredText: text(body.deliveredText ?? body.text),
    mode: body.mode ?? "dictation",
    rewrites: rewrites.map((r) => ({
      id: text(r.id, 80),
      instruction: text(r.instruction, 12000),
      text: text(r.text),
      model: text(r.model, 200),
      createdAt: text(r.createdAt, 80),
    })),
    segments: segments.map((r) => ({
      id: uuid(r.id),
      rawText: text(r.rawText),
    })),
  };
  if (!["dictation", "command", "scratchpad", "meeting"].includes(fields.mode))
    fail(400, "Invalid task mode.");
  if (
    new Set(fields.segments.map((r) => r.id)).size !== fields.segments.length ||
    fields.segments.some((r) => r.id === body.id)
  )
    fail(400, "Recording segments must have distinct IDs.");
  if (Buffer.byteLength(JSON.stringify(fields)) > 220000)
    fail(413, "This conversation is full. Export it before continuing.");
  return fields;
}

// The service knows account identities, never login passwords or provider API keys.
export function createService({
  store,
  send,
  mintSpeechToken,
  origin,
  origins = [origin],
  speechEnabled = false,
  rewriteText,
  now = () => Date.now(),
}) {
  const seconds = () => Math.floor(now() / 1000);
  const partition = (owner) => `USER#${owner}`;
  const activeDevice = async (owner, id, session) => {
    const item = await store.get(partition(owner), `DEVICE#${uuid(id)}`);
    if (
      !item ||
      item.revoked ||
      !item.authSession ||
      (session && item.authSession !== session) ||
      (item.authSession &&
        (await store.get(partition(owner), `REVOKED_AUTH#${item.authSession}`)))
    )
      fail(
        403,
        "This device was removed. Sign out and connect it again as a new device.",
      );
    return item;
  };
  const authCheck = (owner, session) => ({
    check: key(partition(owner), `REVOKED_AUTH#${session}`),
    absent: true,
  });
  const deviceChecks = (owner, device) => [
    {
      check: key(partition(owner), `DEVICE#${device.id}`),
      matches: { revoked: false, authSession: device.authSession },
    },
    authCheck(owner, device.authSession),
  ];
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
  async function notify(owner, event, targets = null) {
    const rows = await connections(owner);
    const delivered = await Promise.all(
      rows
        .filter((row) => targets === null || targets.includes(row.deviceId))
        .map(async (row) => {
          const device = await activeDevice(owner, row.deviceId).catch(
            () => null,
          );
          if (!device) {
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
  async function mergeRecords(pk, id, fields, sources) {
    const mergedSources = sources ?? [];
    const originals = new Map(fields.segments.map((s) => [s.id, s.rawText]));
    if (
      !Array.isArray(mergedSources) ||
      mergedSources.length > 20 ||
      new Set(mergedSources.map((r) => r?.id)).size !== mergedSources.length
    )
      fail(400, "Choose up to 20 distinct conversations to merge.");
    const merges = [];
    for (const source of mergedSources) {
      const sourceId = uuid(source?.id);
      if (sourceId === id) fail(400, "Choose a different conversation.");
      const receipt = await store.get(pk, `RECORDING#${sourceId}`);
      const original =
        receipt?.historyKey && (await store.get(pk, receipt.historyKey));
      if (
        !original ||
        receipt.deleted ||
        source.baseRevision !== (original.revision ?? 1)
      )
        fail(
          409,
          "A conversation selected for merging changed. Reopen both notes before merging.",
        );
      if (
        originals.get(sourceId) !== original.rawText ||
        (original.segments ?? []).some((s) => originals.get(s.id) !== s.rawText)
      )
        fail(409, "A merge must preserve every original recording.");
      merges.push({ original, receipt });
    }
    return merges;
  }
  const absorb = (pk, id, merges) =>
    merges.flatMap(({ original, receipt }) => [
      {
        put: {
          pk,
          sk: `RECORDING#${original.id}`,
          historyKey: receipt.historyKey,
          deleted: true,
          mergedInto: id,
        },
        matches: { historyKey: receipt.historyKey },
      },
      {
        delete: key(pk, original.sk),
        matches: original.revision
          ? { revision: original.revision }
          : { rawText: original.rawText, text: original.text },
      },
    ]);
  async function http({
    owner,
    expires,
    session,
    method,
    path,
    body = {},
    query = {},
  }) {
    if (!owner || !expires || expires <= seconds())
      fail(401, "Sign in to connect your devices.");
    const pk = partition(owner);
    if (typeof session !== "string" || !session || session.length > 200)
      fail(401, "Sign in again to bind this device to your login.");
    const authSession = hash(session);
    if (await store.get(pk, `REVOKED_AUTH#${authSession}`))
      fail(403, "This login was revoked. Sign in again.");
    const signedDevice = (id) => activeDevice(owner, id, authSession);
    if (path.startsWith("/history"))
      await signedDevice(body.deviceId ?? query.deviceId);
    if (path === "/devices" && method === "GET") {
      const rows = (await store.list(pk, "DEVICE#", 100)).items.filter(
        (row) => !row.revoked,
      );
      const live = await connections(owner);
      return {
        devices: rows.map((row) => ({
          ...publicDevice(row),
          currentSession: row.authSession === authSession,
          online: live.some((c) => c.deviceId === row.id),
        })),
        speechEnabled,
        rewriteEnabled: Boolean(rewriteText),
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
      if (
        existing &&
        (existing.revoked || existing.authSession !== authSession)
      )
        fail(403, "This device was removed. Connect a new device.");
      const row = {
        pk,
        sk: `DEVICE#${id}`,
        id,
        name,
        kind: body.kind,
        revoked: false,
        authSession,
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
          authCheck(owner, authSession),
          { check: key(pk, `REMOVED#${id}`), absent: true },
          { put: row, absent: true },
          {
            put: { pk, sk: "DEVICE_COUNT", count: (count?.count ?? 0) + 1 },
            ...(count ? { matches: { count: count.count } } : { absent: true }),
          },
        ]);
      } else
        await store.commit([
          authCheck(owner, authSession),
          { put: row, matches: { revoked: false, authSession } },
        ]);
      return publicDevice(row);
    }
    if (path.startsWith("/devices/") && method === "DELETE") {
      const device = await store.get(pk, `DEVICE#${uuid(path.slice(9))}`);
      if (!device || device.revoked)
        fail(403, "This device was already removed.");
      const count = await store.get(pk, "DEVICE_COUNT");
      await store.commit([
        ...(device.authSession === authSession
          ? []
          : [authCheck(owner, authSession)]),
        { delete: key(pk, `DEVICE#${device.id}`), matches: { revoked: false } },
        {
          put: { pk, sk: `REMOVED#${device.id}`, removedAt: now() },
          absent: true,
        },
        {
          put: { pk, sk: "DEVICE_COUNT", count: Math.max(0, count.count - 1) },
          matches: { count: count.count },
        },
        ...(device.authSession
          ? [
              {
                put: {
                  pk,
                  sk: `REVOKED_AUTH#${device.authSession}`,
                  removedAt: now(),
                },
              },
            ]
          : []),
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
      const device = await signedDevice(body.deviceId);
      const ticket = randomBytes(32).toString("base64url");
      await store.commit([
        ...deviceChecks(owner, device),
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
      const device = await signedDevice(body.deviceId);
      if (body.consent !== true)
        fail(400, "Confirm that microphone audio will go to ElevenLabs.");
      const minute = Math.floor(seconds() / 60),
        counter = await store.get(pk, `SPEECH#${minute}`);
      if ((counter?.count ?? 0) >= 5)
        fail(429, "Too many speech starts. Wait a minute before trying again.");
      await store.commit([
        ...deviceChecks(owner, device),
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
      const token = await mintSpeechToken();
      await signedDevice(body.deviceId);
      return { token };
    }
    if (path === "/rewrite" && method === "POST") {
      if (!rewriteText)
        fail(
          503,
          "Rewriting is not enabled on this host. Your text is ready to edit and export.",
        );
      const device = await signedDevice(body.deviceId);
      if (body.consent !== true)
        fail(400, "Confirm that text will go to the host’s rewrite provider.");
      const source = text(body.text, 60000),
        instruction = text(body.instruction, 12000);
      if (!source.trim() || !instruction.trim())
        fail(400, "Add text and a rewrite instruction first.");
      const minute = Math.floor(seconds() / 60),
        sk = `REWRITE#${minute}`,
        count = await store.get(pk, sk);
      if ((count?.count ?? 0) >= 5)
        fail(429, "Wait a minute before rewriting again.");
      await store.commit([
        ...deviceChecks(owner, device),
        {
          put: {
            pk,
            sk,
            count: (count?.count ?? 0) + 1,
            expires: seconds() + 120,
          },
          ...(count ? { matches: { count: count.count } } : { absent: true }),
        },
      ]);
      const result = await rewriteText({ source, instruction });
      await signedDevice(body.deviceId);
      return { text: text(result.text), model: text(result.model, 200) };
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
      const device = await signedDevice(body.deviceId);
      const id = uuid(body.id),
        rawText = text(body.rawText),
        fields = documentFields(body);
      if (!fields.text.trim() && !rawText.trim())
        fail(400, "There are no words to save yet.");
      const receipt = await store.get(pk, `RECORDING#${id}`);
      if (receipt) {
        const saved =
          receipt.historyKey && (await store.get(pk, receipt.historyKey));
        if (!saved || receipt.deleted) fail(410, "This recording was deleted.");
        if (saved.rawText !== rawText)
          fail(409, "Original recognition is immutable.");
        if ((saved.revision ?? 1) !== 1)
          fail(
            409,
            "This recording was saved and then changed. Review the account version.",
          );
        if (JSON.stringify(documentFields(saved)) !== JSON.stringify(fields))
          fail(409, "This recording ID was already saved with different text.");
        return publicHistory(saved);
      }
      const createdAt =
          Number.isSafeInteger(body.createdAt) &&
          body.createdAt >= 1000000000000 &&
          body.createdAt <= now() + 60000
            ? body.createdAt
            : now(),
        sk = `HISTORY#${createdAt}#${id}`;
      const row = {
        pk,
        sk,
        id,
        ...fields,
        rawText,
        createdAt,
        updatedAt: createdAt,
        deviceId: device.id,
        revision: 1,
      };
      const merges = await mergeRecords(pk, id, fields, body.mergedSources);
      await store.commit([
        ...deviceChecks(owner, device),
        { put: { pk, sk: `RECORDING#${id}`, historyKey: sk }, absent: true },
        { put: row, absent: true },
        ...absorb(pk, id, merges),
      ]);
      await notify(owner, { type: "history.changed", id }).catch(() => {});
      for (const { original } of merges)
        await notify(owner, {
          type: "history.changed",
          id: original.id,
          deleted: true,
          mergedInto: id,
        }).catch(() => {});
      return publicHistory(row);
    }
    if (path.startsWith("/history/")) {
      const id = uuid(path.slice(9)),
        receipt = await store.get(pk, `RECORDING#${id}`);
      const saved =
        receipt?.historyKey && (await store.get(pk, receipt.historyKey));
      if (method === "DELETE") {
        const device = await signedDevice(body.deviceId ?? query.deviceId);
        // Tombstone even an unknown ID: a delayed first save must not resurrect it.
        await store.commit([
          ...deviceChecks(owner, device),
          {
            put: {
              pk,
              sk: `RECORDING#${id}`,
              deleted: true,
              ...(receipt?.historyKey
                ? { historyKey: receipt.historyKey }
                : {}),
            },
          },
          ...(receipt?.historyKey
            ? [{ delete: key(pk, receipt.historyKey) }]
            : []),
        ]);
        await notify(owner, {
          type: "history.changed",
          id,
          deleted: true,
        }).catch(() => {});
        return { removed: true };
      }
      if (!saved || receipt?.deleted)
        fail(
          receipt ? 410 : 404,
          "This recording was deleted or is unavailable.",
        );
      if (method === "GET") return publicHistory(saved);
      if (method === "PUT") {
        if (body.id !== id)
          fail(400, "The document ID must match its address.");
        const device = await signedDevice(body.deviceId),
          operationId = uuid(body.operationId);
        const operationKey = `EDIT#${id}#${operationId}`;
        const fingerprint = hash(JSON.stringify(body));
        const applied = await store.get(pk, operationKey);
        if (applied) {
          if (applied.fingerprint !== fingerprint)
            fail(409, "This edit ID was already used.");
          if (applied.revision !== (saved.revision ?? 1))
            fail(
              409,
              "This edit was saved, then changed on another device. Review both versions.",
            );
          return { ...publicHistory(saved), appliedRevision: applied.revision };
        }
        if (body.baseRevision !== (saved.revision ?? 1))
          fail(
            409,
            "This note changed on another device. Review both versions before saving.",
          );
        if (body.rawText !== saved.rawText)
          fail(409, "Original recognition is immutable.");
        const fields = documentFields(body);
        const originals = new Map(
          fields.segments.map((r) => [r.id, r.rawText]),
        );
        if (
          (saved.segments ?? []).some((r) => originals.get(r.id) !== r.rawText)
        )
          fail(409, "Original recording segments are immutable.");
        const revision = (saved.revision ?? 1) + 1;
        const row = { ...saved, ...fields, revision, updatedAt: now() };
        const merges = await mergeRecords(pk, id, fields, body.mergedSources);
        await store.commit([
          ...deviceChecks(owner, device),
          {
            check: key(pk, `RECORDING#${id}`),
            matches: { historyKey: saved.sk },
          },
          {
            put: row,
            matches: saved.revision
              ? { revision: saved.revision }
              : { rawText: saved.rawText, text: saved.text },
          },
          {
            put: {
              pk,
              sk: operationKey,
              fingerprint,
              revision,
              expires: seconds() + 604800,
            },
            absent: true,
          },
          ...absorb(pk, id, merges),
        ]);
        // Refresh the target first; receivers retain absorbed originals as segments.
        await notify(owner, { type: "history.changed", id }).catch(() => {});
        for (const { original } of merges)
          await notify(owner, {
            type: "history.changed",
            id: original.id,
            deleted: true,
            mergedInto: id,
          }).catch(() => {});
        return publicHistory(row);
      }
    }
    fail(404, "This action is unavailable.");
  }
  async function socket(event) {
    const { routeKey, connectionId } = event.requestContext;
    if (routeKey === "$connect") {
      if (!origins.includes(event.headers?.origin ?? event.headers?.Origin))
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
        ...deviceChecks(row.owner, device),
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
    const fingerprint = hash(
      JSON.stringify({ phase: message.phase, text: content, targets }),
    );
    if (previous && message.sequence <= previous.sequence) {
      if (
        message.sequence === previous.sequence &&
        previous.fingerprint !== fingerprint
      )
        fail(409, "A live sequence cannot be reused with different words.");
      if (message.sequence === previous.sequence)
        await notify(
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
      ...deviceChecks(connection.owner, device),
      {
        put: {
          pk,
          sk,
          deviceId: device.id,
          sequence: message.sequence,
          fingerprint,
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
