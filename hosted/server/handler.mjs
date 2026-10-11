import {
  ApiGatewayManagementApiClient,
  PostToConnectionCommand,
} from "@aws-sdk/client-apigatewaymanagementapi";
import {
  SecretsManagerClient,
  GetSecretValueCommand,
} from "@aws-sdk/client-secrets-manager";
import { dynamoStore } from "./aws-store.mjs";
import { createService, Failure, Conflict } from "./service.mjs";

const secrets = new SecretsManagerClient({});
let speechKey;
async function mintSpeechToken() {
  speechKey ??= (
    await secrets.send(
      new GetSecretValueCommand({ SecretId: process.env.SPEECH_SECRET_ARN }),
    )
  ).SecretString;
  const reply = await fetch(
    "https://api.elevenlabs.io/v1/single-use-token/realtime_scribe",
    {
      method: "POST",
      headers: { "xi-api-key": speechKey },
      signal: AbortSignal.timeout(8000),
    },
  );
  if (!reply.ok)
    throw new Failure(
      503,
      "Speech is unavailable. Keep your words and try later.",
    );
  const payload = await reply.json();
  if (typeof payload.token !== "string")
    throw new Failure(503, "Speech did not return a connection token.");
  return payload.token;
}
export async function handler(event) {
  const websocket = Boolean(event.requestContext?.connectionId);
  const gateway = websocket
    ? new ApiGatewayManagementApiClient({
        endpoint: `https://${event.requestContext.domainName}/${event.requestContext.stage}`,
      })
    : null;
  // HTTP fan-out uses the fixed stack WebSocket endpoint, never client-supplied URLs.
  const sender =
    gateway ??
    new ApiGatewayManagementApiClient({
      endpoint: process.env.WS_MANAGEMENT_URL,
    });
  const service = createService({
    store: dynamoStore(process.env.TABLE_NAME),
    origin: process.env.WEB_ORIGIN,
    speechEnabled: process.env.SPEECH_ENABLED === "true",
    mintSpeechToken,
    send: (connectionId, payload) =>
      sender.send(
        new PostToConnectionCommand({
          ConnectionId: connectionId,
          Data: Buffer.from(JSON.stringify(payload)),
        }),
      ),
  });
  const headers = {
    "content-type": "application/json",
    "cache-control": "no-store",
    "x-content-type-options": "nosniff",
  };
  try {
    if (websocket) {
      await service.socket(event);
      return { statusCode: 200, body: "" };
    }
    const claims = event.requestContext?.authorizer?.jwt?.claims;
    if (!claims || claims.token_use !== "access")
      throw new Failure(401, "Sign in to Mluva.");
    const method = event.requestContext.http.method;
    if (
      method !== "GET" &&
      (event.headers?.origin ?? "") !== process.env.WEB_ORIGIN
    )
      throw new Failure(403, "Open Mluva from its configured address.");
    const encoded = event.body ?? "";
    if (event.isBase64Encoded || Buffer.byteLength(encoded) > 200000)
      throw new Failure(413, "This recording is too large. Export the text.");
    let body = {};
    try {
      body = encoded ? JSON.parse(encoded) : {};
    } catch {
      throw new Failure(400, "Invalid request.");
    }
    if (!body || typeof body !== "object" || Array.isArray(body))
      throw new Failure(400, "Invalid request.");
    const result = await service.http({
      owner: claims.sub,
      expires: Number(claims.exp),
      method,
      path: event.rawPath,
      body,
      query: event.queryStringParameters,
    });
    return { statusCode: 200, headers, body: JSON.stringify(result) };
  } catch (error) {
    const status =
      error instanceof Failure
        ? error.status
        : error instanceof Conflict
          ? 409
          : 503;
    const message =
      error instanceof Failure
        ? error.message
        : error instanceof Conflict
          ? "Another update arrived first. Retry this action."
          : "Connection failed. Keep your words and retry.";
    // No request body, transcript, ticket, provider token or secret is logged.
    if (websocket && event.requestContext.routeKey !== "$connect") {
      await sender
        .send(
          new PostToConnectionCommand({
            ConnectionId: event.requestContext.connectionId,
            Data: Buffer.from(
              JSON.stringify({ type: "error", status, message }),
            ),
          }),
        )
        .catch(() => {});
    }
    return { statusCode: status, headers, body: JSON.stringify({ message }) };
  }
}
