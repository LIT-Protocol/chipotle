import { createHash } from 'node:crypto';
import { readFile, writeFile, copyFile, mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';

const sdk = fileURLToPath(new URL('../', import.meta.url));
const registry = 'https://registry.npmjs.org/';

export function nextVersion(floor, versions) {
  const parse = v => {
    if (!/^\d+\.\d+\.\d+$/.test(v)) throw new Error(`Invalid stable version: ${v}`);
    return v.split('.').map(Number);
  };
  const compare = (a, b) => a[0] - b[0] || a[1] - b[1] || a[2] - b[2];
  const minimum = parse(floor);
  const stable = versions.filter(v => /^\d+\.\d+\.\d+$/.test(v)).map(parse).sort(compare);
  const highest = stable.at(-1);
  if (!highest || compare(minimum, highest) > 0) return floor;
  return `${highest[0]}.${highest[1]}.${highest[2] + 1}`;
}

export async function release(publish = false) {
  const manifest = JSON.parse(await readFile(join(sdk, 'package.json'), 'utf8'));
  const stage = await mkdtemp(join(tmpdir(), 'lit-core-sdk-'));
  try {
    const hash = createHash('sha256').update(JSON.stringify(manifest));
    for (const file of [...manifest.files, 'README.md']) {
      const source = file === 'README.md' ? join(sdk, file) : join(sdk, '..', file);
      hash.update(file).update(await readFile(source));
      await copyFile(source, join(stage, file));
    }
    const digest = hash.digest('hex');
    if (publish) {
      const response = await fetch(`${registry}${encodeURIComponent(manifest.name)}`, {
        signal: AbortSignal.timeout(30000), headers: { 'Cache-Control': 'no-cache' },
      });
      if (!response.ok && response.status !== 404) throw new Error(`Registry lookup failed: ${response.status}`);
      const metadata = response.status === 404 ? { versions: {} } : await response.json();
      // Compare only the newest stable release so intentional reverts publish too.
      const newest = Object.keys(metadata.versions).filter(v => /^\d+\.\d+\.\d+$/.test(v))
        .sort((a, b) => {
          const x = a.split('.').map(Number), y = b.split('.').map(Number);
          return x[0] - y[0] || x[1] - y[1] || x[2] - y[2];
        }).at(-1);
      if (newest && metadata.versions[newest].litSdkDigest === digest) {
        console.log('These SDK contents have already been published.');
        return;
      }
      manifest.version = nextVersion(manifest.version, Object.keys(metadata.versions));
    }
    manifest.litSdkDigest = digest;
    await writeFile(join(stage, 'package.json'), JSON.stringify(manifest, null, 2) + '\n');
    const packed = JSON.parse(execFileSync('npm', ['pack', '--json', '--ignore-scripts'], { cwd: stage, encoding: 'utf8' }))[0];
    const allowed = new Set(['package.json', 'README.md', ...manifest.files]);
    if (packed.files.length !== allowed.size || packed.files.some(f => !allowed.has(f.path))) {
      throw new Error('Unexpected package contents');
    }
    // Exercise the actual tarball as an installed package, including its relative imports.
    const consumer = join(stage, 'consumer');
    execFileSync('npm', ['install', '--prefix', consumer, '--ignore-scripts', '--no-audit', '--no-fund', join(stage, packed.filename)], { stdio: 'pipe' });
    execFileSync(process.execPath, ['--input-type=module', '-e', `
      import assert from 'node:assert/strict';
      import Client, { createClient } from '@lit-protocol/core-sdk';
      import { verifyAttestation, LIT_API_URL } from '@lit-protocol/core-sdk/attestation';
      assert.equal(typeof verifyAttestation, 'function');
      assert.ok(LIT_API_URL.startsWith('https://'));
      const client = createClient('https://example.invalid');
      assert.ok(client instanceof Client);
      globalThis.fetch = async (url, options) => {
        assert.equal(url, 'https://example.invalid/core/v1/account_exists');
        assert.equal(options.headers['X-Api-Key'], 'test-key');
        return new Response(JSON.stringify({ exists: true }));
      };
      assert.deepEqual(await client.accountExists('test-key'), { exists: true });
    `], { cwd: consumer, stdio: 'inherit' });
    if (publish) {
      execFileSync('npm', ['publish', join(stage, packed.filename), '--access', 'public', '--ignore-scripts', '--registry', registry], { cwd: stage, stdio: 'inherit' });
    }
    console.log(`${publish ? 'Published' : 'Validated'} ${manifest.name}@${manifest.version}`);
  } finally {
    await rm(stage, { recursive: true, force: true });
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  await release(process.argv.includes('--publish'));
}
