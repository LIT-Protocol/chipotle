// The type declarations are written by hand. These tests fail when the
// JavaScript and the declarations stop naming the same things.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const root = new URL('../../', import.meta.url);

function declaredValues(source) {
  const names = new Set();
  for (const m of source.matchAll(/^export (?:declare )?(?:const|function|class) (\w+)/gm)) names.add(m[1]);
  for (const m of source.matchAll(/^export \{([^}]*)\} from/gm)) {
    for (const name of m[1].split(',').map(s => s.trim()).filter(Boolean)) names.add(name);
  }
  if (/^export default /m.test(source)) names.add('default');
  return [...names].sort();
}

for (const file of ['core_sdk', 'attestation']) {
  test(`${file}.d.ts declares exactly what ${file}.js exports`, async () => {
    const runtime = Object.keys(await import(new URL(`${file}.js`, root))).sort();
    const declared = declaredValues(await readFile(new URL(`${file}.d.ts`, root), 'utf8'));
    assert.deepEqual(declared, runtime);
  });
}

test('core_sdk.d.ts declares exactly the public client methods', async () => {
  const { LitNodeSimpleApiClient } = await import(new URL('core_sdk.js', root));
  const descriptors = Object.getOwnPropertyDescriptors(LitNodeSimpleApiClient.prototype);
  const runtime = Object.keys(descriptors)
    .filter(name => name !== 'constructor' && !name.startsWith('_') && typeof descriptors[name].value === 'function')
    .sort();
  const source = await readFile(new URL('core_sdk.d.ts', root), 'utf8');
  const body = /^export class LitNodeSimpleApiClient \{\n([\s\S]*?)^\}/m.exec(source)[1];
  const declared = [...body.matchAll(/^  (\w+)[<(]/gm)].map(m => m[1]).filter(name => name !== 'constructor').sort();
  assert.deepEqual(declared, runtime);
});
