// Exercise the same pure revision helper loaded by Quickshell, including UTF-16 boundaries.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';

const context = vm.createContext({});
vm.runInContext(readFileSync(new URL('../quickshell/mluva.dictation/TextMotion.js', import.meta.url), 'utf8')
  .replace('.pragma library', ''), context);
const cases = [
  ['Hello 😀.', 'Hello 😎.'], ['🦊', '🐊'], ['A😀', 'B😀'],
  ['Ahoj, jak se máš? 👩‍💻', 'Ahoj, mám se dobře. 👨‍💻'],
  ['Tuesday. Keep this middle sentence. Thanks.', 'Wednesday. Keep this middle sentence. Cheers.'],
  ['', '<b>Exact source & Markdown.</b>'], ['Goodbye.', ''],
  ['old '.repeat(400), 'new '.repeat(400)],
];
const ink = {r: 1, g: 0.5, b: 0};
for (const [before, after] of cases) {
  const changes = context.changes(before, after);
  let reconstructed = before;
  for (const [a, b, c, d] of [...changes].reverse())
    reconstructed = reconstructed.slice(0, a) + after.slice(c, d) + reconstructed.slice(b);
  assert.equal(reconstructed, after);
  for (const old of [true, false]) {
    const html = context.styled(old ? before : after, changes, old, 0.5, ink);
    assert.ok(html.isWellFormed(), `Surrogate pair split in ${html}`);
    assert.ok(!html.includes('<b>Exact'), 'Recognition source must remain escaped');
  }
}
assert.equal(context.changes(cases[4][0], cases[4][1]).length, 2, 'Unchanged middle words should stay stable');
assert.ok(context.forecast([[0,0],[1000,60]], 60, 0.7, 1, 2)
  > context.forecast([[0,0],[1000,8]], 60, 0.7, 1, 2));
console.log('Quickshell word revisions: Unicode, escaping, stable middles, bounded fallback and pace passed');
