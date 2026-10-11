// Reads a CloudFormation describe-stacks JSON file and prepares local static assets.
// This command never uploads or deploys them.
import { readFile, writeFile, mkdir, cp } from "node:fs/promises";
const input = process.argv[2],
  destination = process.argv[3];
if (!input || !destination)
  throw new Error(
    "Usage: node scripts/configure.mjs stack-outputs.json /absolute/path/to/prepared-site",
  );
const stack = JSON.parse(await readFile(input, "utf8")).Stacks[0];
const outputs = Object.fromEntries(
  stack.Outputs.map(({ OutputKey, OutputValue }) => [OutputKey, OutputValue]),
);
for (const name of [
  "WebOrigin",
  "HttpUrl",
  "WebSocketUrl",
  "CognitoClientId",
  "LoginUrl",
])
  if (!outputs[name]) throw new Error(`Missing stack output: ${name}`);
await mkdir(destination, { recursive: true });
await cp(new URL("../web/", import.meta.url), destination, { recursive: true });
await mkdir(`${destination}/icons`, { recursive: true });
for (const size of [192, 512])
  await cp(
    new URL(`../../docs/brand/png/mluva-mark-${size}.png`, import.meta.url),
    `${destination}/icons/mluva-${size}.png`,
  );
await writeFile(
  `${destination}/config.json`,
  JSON.stringify(
    {
      api: outputs.HttpUrl,
      websocket: outputs.WebSocketUrl,
      clientId: outputs.CognitoClientId,
      login: outputs.LoginUrl,
      origin: outputs.WebOrigin,
    },
    null,
    2,
  ),
);
process.stdout.write(
  `Prepared static site in ${destination}. No resources were published.\n`,
);
