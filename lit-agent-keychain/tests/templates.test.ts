import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { json } from "./harness.ts";
import { TemplateStore, actionCid, templateHash } from "../protocol/actions.ts";
import archiveIndex from "../generated/archive-index.ts";
import type { Authority } from "../protocol/schema.ts";

const authority: Authority = {
  v: 2,
  network: "test",
  registry: "https://keychain.test",
  owner: { kind: "wallet", address: "0x" + "1".repeat(40) },
};
const [current, previous] = archiveIndex.authority;

test("template store resolves the current release offline and older releases by verified hash", async () => {
  const store = new TemplateStore();
  const now = store.current(authority);
  assert.equal(now.hash, current);
  assert.equal(
    await actionCid(authority),
    await actionCid(authority, now.code),
  );
  const oldCode = await readFile(
    `actions/archive/authority/${previous}.js`,
    "utf8",
  );
  const oldCid = await actionCid(authority, oldCode);
  assert.notEqual(oldCid, await actionCid(authority));
  const served: string[] = [];
  const realFetch = globalThis.fetch;
  globalThis.fetch = (async (input: any) => {
    served.push(String(input));
    if (String(input).endsWith(previous)) return new Response(oldCode);
    return json({ error: "not_found" }, 404);
  }) as any;
  try {
    const resolved = await store.resolve(authority, oldCid);
    assert.equal(resolved.hash, previous);
    assert.deepEqual(served, [
      `https://keychain.test/api/templates/${previous}`,
    ]);
    // Cached: no second fetch.
    await store.resolve(authority, oldCid);
    assert.equal(served.length, 1);
    // Unknown CIDs never resolve, whatever the registry serves.
    await assert.rejects(
      store.resolve(authority, "Qm" + "a".repeat(44)),
      /known release/,
    );
    // A registry serving different bytes for a hash is rejected.
    const tampering = new TemplateStore();
    globalThis.fetch = (async () => new Response(oldCode + "\n//x")) as any;
    await assert.rejects(
      tampering.byHash("authority", previous, authority.registry),
      /does not match its hash/,
    );
    // Hashes outside the bundled index are refused before any fetch.
    await assert.rejects(
      tampering.byHash("authority", templateHash("bogus"), authority.registry),
      /Unknown authority release/,
    );
  } finally {
    globalThis.fetch = realFetch;
  }
});
