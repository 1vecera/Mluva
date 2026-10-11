// Disposable loopback preview. Production Lambda has no development login route.
import http from "node:http";
import { randomUUID, randomBytes } from "node:crypto";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { resolve, extname } from "node:path";
import { WebSocketServer } from "ws";
import { createService, Failure, Conflict } from "../server/service.mjs";
import { memoryStore } from "../tests/memory-store.mjs";
export async function createPreview(port = 0) {
  const faults = {},
    store = memoryStore(),
    sessions = new Map(),
    peers = new Map(),
    speechTokens = new Set();
  const server = http.createServer(),
    sockets = new WebSocketServer({ noServer: true }),
    speech = new WebSocketServer({ noServer: true });
  await new Promise((done) => server.listen(port, "127.0.0.1", done));
  const origin = `http://127.0.0.1:${server.address().port}`;
  const service = createService({
    store,
    origin,
    speechEnabled: true,
    send: async (id, payload) => {
      const peer = peers.get(id);
      if (!peer || peer.readyState !== 1) {
        const error = new Error();
        error.statusCode = 410;
        throw error;
      }
      peer.send(JSON.stringify(payload));
    },
    mintSpeechToken: async () => {
      const token = randomBytes(32).toString("base64url");
      speechTokens.add(token);
      return token;
    },
  });
  const root = fileURLToPath(new URL("../web/", import.meta.url));
  server.on("request", async (request, response) => {
    const url = new URL(request.url, origin);
    const json = (status, body) => {
      response.writeHead(status, {
        "content-type": "application/json",
        "cache-control": "no-store",
      });
      response.end(JSON.stringify(body));
    };
    try {
      if (url.pathname === "/config.json")
        return json(200, {
          api: `${origin}/api`,
          websocket: origin.replace("http:", "ws:") + "/live",
          speechSocket: origin.replace("http:", "ws:") + "/speech",
          login: origin,
          origin,
          clientId: "local-preview",
          dev: true,
        });
      if (url.pathname === "/dev/session" && request.method === "POST") {
        if (request.headers.origin !== origin)
          throw new Failure(403, "Use the loopback preview page.");
        const token = `local.${Buffer.from(JSON.stringify({ sub: "preview-alice" })).toString("base64url")}.${randomBytes(32).toString("base64url")}`;
        sessions.set(token, {
          owner: "preview-alice",
          expires: Math.floor(Date.now() / 1000) + 3600,
        });
        return json(200, {
          access_token: token,
          expires: Date.now() + 3600000,
        });
      }
      if (url.pathname.startsWith("/api/")) {
        if (
          faults.history &&
          url.pathname === "/api/history" &&
          request.method === "POST"
        )
          throw new Failure(503, "Synthetic save interruption.");
        const session = sessions.get(request.headers.authorization?.slice(7));
        let body = "";
        for await (const chunk of request) {
          body += chunk;
          if (Buffer.byteLength(body) > 200000)
            throw new Failure(413, "Too large.");
        }
        if (request.method !== "GET" && request.headers.origin !== origin)
          throw new Failure(403, "Use the preview page.");
        return json(
          200,
          await service.http({
            ...session,
            method: request.method,
            path: url.pathname.slice(4),
            query: Object.fromEntries(url.searchParams),
            body: body ? JSON.parse(body) : {},
          }),
        );
      }
      if (request.method !== "GET")
        return json(404, { message: "Unavailable." });
      const path =
        url.pathname === "/"
          ? "index.html"
          : decodeURIComponent(url.pathname.slice(1));
      if (path.startsWith("icons/")) {
        const size =
          path === "icons/mluva-192.png"
            ? 192
            : path === "icons/mluva-512.png"
              ? 512
              : null;
        if (!size) throw new Failure(404, "Missing icon.");
        response.writeHead(200, { "content-type": "image/png" });
        response.end(
          await readFile(
            new URL(
              `../../docs/brand/png/mluva-mark-${size}.png`,
              import.meta.url,
            ),
          ),
        );
        return;
      }
      const file = resolve(root, path);
      if (!file.startsWith(root)) throw new Failure(404, "Missing file.");
      const body = await readFile(file);
      response.writeHead(200, {
        "content-type":
          {
            ".html": "text/html",
            ".css": "text/css",
            ".js": "text/javascript",
            ".mjs": "text/javascript",
            ".json": "application/json",
            ".webmanifest": "application/manifest+json",
          }[extname(file)] ?? "text/plain",
        "cache-control": "no-store",
        "x-content-type-options": "nosniff",
      });
      response.end(body);
    } catch (error) {
      json(error.status ?? (error instanceof Conflict ? 409 : 500), {
        message: error.message ?? "Preview failed.",
      });
    }
  });
  server.on("upgrade", async (request, socket, head) => {
    const url = new URL(request.url, origin),
      id = randomUUID();
    try {
      if (url.pathname === "/speech") {
        if (
          request.headers.origin !== origin ||
          !speechTokens.delete(url.searchParams.get("token"))
        )
          throw new Failure(401, "Invalid speech token.");
        speech.handleUpgrade(request, socket, head, (peer) => {
          let count = 0,
            committed = 0;
          peer.send(JSON.stringify({ message_type: "session_started" }));
          peer.on("message", (data) => {
            const payload = JSON.parse(data.toString());
            count++;
            if (payload.commit) {
              committed++;
              peer.send(
                JSON.stringify({
                  message_type: "committed_transcript",
                  text:
                    committed === 1
                      ? "A thought from this device."
                      : "Final words are preserved.",
                }),
              );
            } else if (count === 2)
              peer.send(
                JSON.stringify({
                  message_type: "partial_transcript",
                  text: "A thought from",
                }),
              );
          });
        });
        return;
      }
      if (url.pathname !== "/live")
        throw new Failure(404, "Missing live socket.");
      await service.socket({
        requestContext: { routeKey: "$connect", connectionId: id },
        headers: request.headers,
        queryStringParameters: Object.fromEntries(url.searchParams),
      });
      sockets.handleUpgrade(request, socket, head, (peer) => {
        peers.set(id, peer);
        peer.on("message", async (data) => {
          try {
            await service.socket({
              requestContext: { routeKey: "$default", connectionId: id },
              body: data.toString(),
            });
          } catch (error) {
            if (peer.readyState === 1)
              peer.send(
                JSON.stringify({
                  type: "error",
                  status:
                    error.status ?? (error instanceof Conflict ? 409 : 503),
                  message: error.message,
                }),
              );
          }
        });
        peer.on("close", () => {
          peers.delete(id);
          service
            .socket({
              requestContext: { routeKey: "$disconnect", connectionId: id },
            })
            .catch(() => {});
        });
      });
    } catch (error) {
      socket.end(`HTTP/1.1 ${error.status ?? 401} Unauthorized\r\n\r\n`);
    }
  });
  return {
    origin,
    store,
    service,
    sessions,
    peers,
    faults,
    close: async () => {
      peers.forEach((peer) => peer.terminate());
      speech.clients.forEach((peer) => peer.terminate());
      sockets.close();
      speech.close();
      await new Promise((resolve) => server.close(resolve));
    },
  };
}
if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const preview = await createPreview(
    Number(process.env.MLUVA_PREVIEW_PORT ?? 8788),
  );
  process.stdout.write(
    `Local synthetic preview: ${preview.origin}\nNo AWS resources, provider requests or production accounts are used.\n`,
  );
  process.on("SIGINT", async () => {
    await preview.close();
    process.exit(0);
  });
}
