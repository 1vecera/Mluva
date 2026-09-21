import { build } from 'esbuild';
import { createHash } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

const directory = fileURLToPath(new URL('.', import.meta.url));
const output = new URL('../../linux/resources/mermaid/mermaid.min.js', import.meta.url);
const result = await build({
  absWorkingDir: directory,
  stdin: { contents: 'import mermaid from "mermaid"; globalThis.mermaid = mermaid;', resolveDir: directory },
  bundle: true,
  format: 'iife',
  platform: 'browser',
  target: 'es2022',
  minify: true,
  legalComments: 'linked',
  metafile: true,
  outfile: fileURLToPath(output),
});
const lock = JSON.parse(await readFile(new URL('package-lock.json', import.meta.url), 'utf8'));
const components = {};
for (const input of Object.keys(result.metafile.inputs).sort()) {
  const path = input.match(/^(.*node_modules\/(?:@[^/]+\/)?[^/]+)\//)?.[1];
  if (path && lock.packages[path]) components[path] = lock.packages[path].version;
}
await writeFile(new URL('../../linux/resources/mermaid/components.json', import.meta.url), JSON.stringify({
  sha256: createHash('sha256').update(await readFile(output)).digest('hex'),
  components,
}, null, 2) + '\n');
