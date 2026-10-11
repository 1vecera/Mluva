import test from "node:test";
import assert from "node:assert/strict";
process.env.WEB_ORIGIN = "https://mluva.example.com";
process.env.WS_MANAGEMENT_URL = "https://socket.example.com/live";
process.env.TABLE_NAME = "not-contacted";
const { handler } = await import("../server/handler.mjs");
const event = (method, origin, claims) => ({
  requestContext: {
    http: { method },
    ...(claims ? { authorizer: { jwt: { claims } } } : {}),
  },
  headers: origin ? { origin } : {},
  rawPath: "/devices",
});
test("token-free preflight is origin bound while business routes require an access JWT", async () => {
  assert.equal(
    (await handler(event("OPTIONS", "https://mluva.example.com"))).statusCode,
    204,
  );
  assert.equal(
    (await handler(event("OPTIONS", "https://other.example.com"))).statusCode,
    403,
  );
  assert.equal(
    (await handler(event("GET", "https://mluva.example.com"))).statusCode,
    401,
  );
  assert.equal(
    (await handler(event("POST", "https://mluva.example.com"))).statusCode,
    401,
  );
  assert.equal(
    (
      await handler(
        event("GET", "https://mluva.example.com", { token_use: "id" }),
      )
    ).statusCode,
    401,
  );
});
