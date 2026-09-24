import { test } from "node:test";
import assert from "node:assert/strict";
import { LiveKeychain, Keychain } from "../sdk/src/index.ts";
import { nowSeconds, randomId, verifyAgent } from "../protocol/crypto.ts";

// Transport-only tests: no simulated grant is ever used as cryptographic authority.
// Receipt verification and real storage lifecycle are exercised by live-discovery-api.
test("live discovery signs only a bounded service-bound challenge, never a private key", async () => {
  const identity = Keychain.generateKey();
  const url = "http://localhost:55441";
  const client = new LiveKeychain(identity.privateKey, { serviceUrl: url });
  const original = globalThis.fetch;
  let requests = 0;
  globalThis.fetch = async (input, init) => {
    requests++;
    const body = JSON.parse(String(init?.body));
    assert.ok(!String(init?.body).includes(identity.privateKey));
    if (String(input).endsWith("/challenge")) {
      assert.deepEqual(body, { agentPublicKey: identity.publicKey });
      const now = nowSeconds();
      return Response.json({
        v: 2,
        domain: "lit-keychain/discovery/v2",
        audience: url,
        agentPublicKey: identity.publicKey,
        nonce: randomId(),
        issuedAt: now,
        expiresAt: now + 60,
      });
    }
    verifyAgent(body.challenge, body.signature, identity.publicKey);
    return Response.json({ v: 2, secrets: [] });
  };
  try {
    assert.deepEqual(await client.list(), []);
    assert.deepEqual(await client.list(), []);
    assert.equal(requests, 4, "no inventory or authorization cache");
    await assert.rejects(client.get("MISSING"), /Unknown secret/);
    client.destroy();
    await assert.rejects(client.list(), /destroyed/);
  } finally {
    globalThis.fetch = original;
    client.destroy();
  }
});

for (const field of [
  "audience",
  "domain",
  "agentPublicKey",
  "expiresAt",
  "issuedAt",
  "nonce",
  "extra",
]) {
  test(`refuses malformed discovery challenge: ${field}`, async () => {
    const identity = Keychain.generateKey();
    const url = "http://localhost:55441";
    const client = new LiveKeychain(identity.privateKey, { serviceUrl: url });
    const original = globalThis.fetch;
    let calls = 0;
    globalThis.fetch = async () => {
      calls++;
      const now = nowSeconds();
      const challenge: any = {
        v: 2,
        domain: "lit-keychain/discovery/v2",
        audience: url,
        agentPublicKey: identity.publicKey,
        nonce: randomId(),
        issuedAt: now,
        expiresAt: now + 60,
      };
      challenge[field] =
        field === "expiresAt"
          ? now - 1
          : field === "issuedAt"
            ? now + 120
            : "wrong";
      return Response.json(challenge);
    };
    try {
      await assert.rejects(client.list());
      assert.equal(calls, 1, "must not sign or submit malformed challenge");
    } finally {
      globalThis.fetch = original;
      client.destroy();
    }
  });
}
