import { build } from 'esbuild';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';

// IIFE for inline embedding in Lit Action code (actions are plain scripts, not
// modules) — exposes `LitVenues` as a global. ESM build is the future
// integrity-pinned CDN import path (plan D1).
const targets = [
  { format: 'iife', globalName: 'LitVenues', outfile: 'dist/lit-venues.iife.js' },
  { format: 'esm', outfile: 'dist/lit-venues.mjs' },
];

for (const t of targets) {
  await build({
    entryPoints: ['src/index.ts'],
    bundle: true,
    target: 'es2022',
    minify: false,
    legalComments: 'inline',
    ...t,
  });
  const buf = readFileSync(t.outfile);
  const sha384 = createHash('sha384').update(buf).digest('base64');
  console.log(`${t.outfile}: ${buf.length} bytes  integrity=sha384-${sha384}`);
}
