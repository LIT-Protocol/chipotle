import { copyFile, mkdir, rm } from 'node:fs/promises';

const dist = new URL('./dist/', import.meta.url);
await rm(dist, { recursive: true, force: true });
await mkdir(dist, { recursive: true });
await copyFile(
  new URL('./lit-agent-keychain.js', import.meta.url),
  new URL('./lit-agent-keychain.js', dist),
);
