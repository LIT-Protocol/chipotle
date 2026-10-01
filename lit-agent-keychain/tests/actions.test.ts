import { test } from "node:test";
import assert from "node:assert/strict";
import { fixture, json, keyFor, pubFor } from "./harness.ts";
import {
  digest,
  hex,
  randomBytes,
  signAgent,
  open,
  decode,
  responseContext,
  verifyAction,
  encryptionPublicKey,
  makeReceipt,
  encryptEnvelope,
  encryptionKey,
  verifyReceipt,
  nowSeconds,
} from "../protocol/crypto.ts";
import { actionCid } from "../protocol/actions.ts";
import { V, DOMAIN } from "../protocol/schema.ts";

test("owner EIP-712 authorization issues a verifiable exact-object receipt", async () => {
  const f = await fixture();
  const proof = await f.ownerProof(f.policy);
  const out = await f.h.run(f.authority, { document: f.policy, proof });
  assert.equal(out.ok, true);
  verifyAction(
    out.receipt.payload,
    out.receipt.signature,
    pubFor(f.authorityCid),
  );
  assert.equal(out.receipt.payload.objectHash, digest(f.policy));
  for (const change of [
    (p: any) => (p.challenge.objectHash = "0".repeat(64)),
    (p: any) => (p.challenge.expiresAt = f.now - 1),
    (p: any) => (p.owner.address = "0x" + "1".repeat(40)),
    (p: any) => (p.challenge.operation = "envelope"),
  ]) {
    const altered = structuredClone(proof);
    change(altered);
    assert.equal(
      (await f.h.run(f.authority, { document: f.policy, proof: altered })).ok,
      false,
    );
  }
});
test("one owner signature approves a batch and yields one exact-object receipt per document", async () => {
  const f = await fixture();
  const manifestDocument = {
    v: V,
    domain: DOMAIN,
    kind: "manifest" as const,
    vaultId: f.manifest.vaultId,
    manifest: f.manifest,
    actionCid: f.cid,
  };
  const documents = [manifestDocument, f.envelope, f.policy];
  const batch = { kind: "batch", vaultId: f.manifest.vaultId, documents };
  const proof = await f.ownerProof(batch as any);
  const out = await f.h.run(f.authority, { documents, proof });
  assert.equal(out.ok, true, JSON.stringify(out));
  assert.equal(out.receipts.length, 3);
  assert.equal(out.receipt, undefined);
  documents.forEach((document, i) => {
    verifyAction(
      out.receipts[i].payload,
      out.receipts[i].signature,
      pubFor(f.authorityCid),
    );
    assert.equal(out.receipts[i].payload.objectHash, digest(document));
    // Receipts are indistinguishable from single-document ones.
    verifyReceipt(
      { document, receipt: out.receipts[i] },
      pubFor(f.authorityCid),
      f.manifest.vaultId,
    );
  });
  // Only the exact batch is approved: reordering, dropping, adding or altering a
  // document, or presenting the batch proof for a single document, is refused.
  const denied = async (params: unknown) =>
    assert.equal((await f.h.run(f.authority, params as any)).ok, false);
  await denied({ documents: [f.envelope, manifestDocument, f.policy], proof });
  await denied({ documents: [manifestDocument, f.envelope], proof });
  await denied({ documents: [...documents, f.policy], proof });
  await denied({
    documents: [
      manifestDocument,
      f.envelope,
      { ...f.policy, expiresAt: f.policy.expiresAt! + 1 },
    ],
    proof,
  });
  await denied({ document: f.policy, proof });
  await denied({ document: f.policy, documents, proof });
  // A single-document proof cannot be replayed as a batch of one.
  const single = await f.ownerProof(f.policy);
  await denied({ documents: [f.policy], proof: single });
  // Sign-in and owner-credential documents are never batched.
  const login = {
    v: V,
    domain: DOMAIN,
    kind: "login" as const,
    vaultId: f.manifest.vaultId,
    challenge: digest({ any: 1 }),
    expiresAt: f.now + 60,
  };
  const withLogin = [manifestDocument, login];
  await denied({
    documents: withLogin,
    proof: await f.ownerProof({
      kind: "batch",
      vaultId: f.manifest.vaultId,
      documents: withLogin,
    } as any),
  });
  // Per-document invariants still apply inside a batch.
  const badPolicy = { ...f.policy, expiresAt: f.policy.notBefore };
  const bad = [manifestDocument, badPolicy];
  await denied({
    documents: bad,
    proof: await f.ownerProof({
      kind: "batch",
      vaultId: f.manifest.vaultId,
      documents: bad,
    } as any),
  });
});
test("policy lifetime is the owner's choice: multi-year and never-expiring policies are approved and honored", async () => {
  for (const expiresAt of [null, nowSeconds() + 10 * 366 * 86400]) {
    const f = await fixture();
    f.policy.expiresAt = expiresAt;
    const proof = await f.ownerProof(f.policy);
    const approved = await f.h.run(f.authority, { document: f.policy, proof });
    assert.equal(
      approved.ok,
      true,
      `authority approves expiresAt=${expiresAt}`,
    );
    f.request.policyHash = digest(f.policy);
    f.params.signedRequest = {
      request: f.request,
      signature: signAgent(f.request, f.agentKey),
    };
    f.h.registry.set(f.registryUrl, f.sign(f.policy));
    const out = await f.h.run(f.manifest, f.params);
    assert.equal(out.ok, true, `secret action honors expiresAt=${expiresAt}`);
    // A never-expiring policy still fails closed on every other check.
    f.policy.disabled = true;
    f.h.registry.set(f.registryUrl, f.sign(f.policy));
    assert.equal((await f.h.run(f.manifest, f.params)).ok, false);
  }
});
test("local HPKE import and signed recipient-encrypted release round trip", async () => {
  const f = await fixture();
  const out = await f.h.run(f.manifest, f.params);
  assert.equal(out.ok, true);
  verifyAction(out.result.payload, out.result.signature, pubFor(f.cid));
  assert.equal(
    decode(
      await open(
        f.responseKey,
        out.result.payload.sealed,
        responseContext(f.request),
      ),
    ),
    f.secret,
  );
  assert.ok(!JSON.stringify(out).includes(f.secret));
});
test("public encryption key is bound to the challenge and immutable manifest", async () => {
  const f = await fixture();
  const challenge = "1".repeat(64);
  const out = await f.h.run(f.manifest, { operation: "publicKey", challenge });
  assert.equal(out.ok, true);
  verifyAction(out.binding.payload, out.binding.signature, pubFor(f.cid));
  assert.equal(out.binding.payload.manifestHash, digest(f.manifest));
  assert.equal(out.binding.payload.challenge, challenge);
  assert.throws(() =>
    verifyAction(
      { ...out.binding.payload, encryptionPublicKey: "0".repeat(64) },
      out.binding.signature,
      pubFor(f.cid),
    ),
  );
});
const attacks: Record<string, (f: any) => void> = {
  "recipient substitution": (f) => {
    f.params.signedRequest.request.responsePublicKey =
      encryptionPublicKey(randomBytes());
  },
  "agent substitution": (f) => {
    f.params.signedRequest.request.agentPublicKey = "1".repeat(64);
  },
  "signature forgery": (f) => {
    f.params.signedRequest.signature = "0".repeat(128);
  },
  "wrong request CID": (f) => {
    f.params.signedRequest.request.actionCid = f.authorityCid;
  },
  "wrong request secret": (f) => {
    f.params.signedRequest.request.secretId = "0".repeat(64);
  },
  "wrong request version": (f) => {
    f.params.signedRequest.request.version = 2;
  },
  "expired request": (f) => {
    f.params.signedRequest.request.expiresAt = f.now - 1;
  },
  "far future request": (f) => {
    f.params.signedRequest.request.issuedAt = f.now + 1000;
  },
  "unbounded request lifetime": (f) => {
    f.params.signedRequest.request.expiresAt = f.now + 86400;
  },
  "policy forgery": (f) => {
    const p = f.sign(f.policy);
    p.document.grants[0].agentPublicKey = "0".repeat(64);
    f.h.registry.set(f.registryUrl, p);
  },
  "disabled policy": (f) => {
    f.policy.disabled = true;
    f.h.registry.set(f.registryUrl, f.sign(f.policy));
  },
  "expired policy": (f) => {
    f.policy.expiresAt = f.now - 1;
    f.h.registry.set(f.registryUrl, f.sign(f.policy));
  },
  "future policy": (f) => {
    f.policy.notBefore = f.now + 1;
    f.h.registry.set(f.registryUrl, f.sign(f.policy));
  },
  "ciphertext substitution": (f) => {
    f.params.envelope.document.ciphertext = "AAAA";
  },
  "envelope owner forgery": (f) => {
    f.params.envelope.receipt.signature = "0".repeat(128);
  },
  "registry unavailable": (f) => {
    f.h.registry.clear();
  },
  "capability epoch mismatch": (f) => {
    f.policy.epoch++;
    f.h.registry.set(f.registryUrl, f.sign(f.policy));
  },
  "operation substitution": (f) => {
    f.params.operation = "stripe.balance";
  },
  "malformed signature": (f) => {
    f.params.signedRequest.signature = "g".repeat(128);
  },
};
for (const [name, attack] of Object.entries(attacks))
  test(`deny ${name} before deriving the secret key`, async () => {
    const f = await fixture();
    attack(f);
    const out = await f.h.run(f.manifest, f.params);
    assert.deepEqual(out, { ok: false, error: "access_denied" });
    assert.equal(f.h.privateKeyCalls, 0);
  });
test("even a correctly signed request cannot exceed the owner grant", async () => {
  const f = await fixture();
  const another = randomBytes();
  f.request.agentPublicKey = hex(
    (await import("@noble/curves/ed25519.js")).ed25519.getPublicKey(another),
  );
  f.params.signedRequest.signature = signAgent(f.request, another);
  assert.equal((await f.h.run(f.manifest, f.params)).ok, false);
});
test("honest revocation denies; DB rollback deliberately restores still-valid old permissions", async () => {
  const f = await fixture();
  const old = f.sign(structuredClone(f.policy));
  f.h.registry.set(
    f.registryUrl,
    f.sign({
      ...f.policy,
      epoch: 2,
      previousHash: digest(f.policy),
      disabled: true,
    }),
  );
  assert.equal((await f.h.run(f.manifest, f.params)).ok, false);
  f.h.registry.set(f.registryUrl, old);
  assert.equal((await f.h.run(f.manifest, f.params)).ok, true);
});
test("identical signed request can repeat; response remains encrypted to the same agent", async () => {
  const f = await fixture();
  for (let i = 0; i < 2; i++) {
    const out = await f.h.run(f.manifest, f.params);
    assert.equal(out.ok, true);
    assert.equal(
      decode(
        await open(
          f.responseKey,
          out.result.payload.sealed,
          responseContext(f.request),
        ),
      ),
      f.secret,
    );
  }
});
test("changing action code changes the encryption identity", async () => {
  const f = await fixture();
  const altered = { ...f.manifest, registry: "https://attacker.test" };
  assert.notEqual(await actionCid(altered), f.cid);
  f.h.registry.set(
    `https://attacker.test/api/registry/secrets/${f.manifest.secretId}`,
    f.sign(f.policy),
  );
  assert.equal((await f.h.run(altered, f.params)).ok, false);
});
test("Stripe action permits only fixed balance request and a bounded response projection", async () => {
  const f = await fixture("stripe_balance");
  f.h.extraFetch = async (input, init) => {
    assert.equal(String(input), "https://api.stripe.com/v1/balance");
    assert.equal(init?.redirect, "error");
    assert.equal((init?.headers as any).Authorization, `Bearer ${f.secret}`);
    return json({
      available: [{ amount: 12, currency: "usd", secret: f.secret }],
      pending: [],
      livemode: false,
      secret: f.secret,
    });
  };
  const out = await f.h.run(f.manifest, f.params);
  assert.equal(out.ok, true);
  const result = decode(
    await open(
      f.responseKey,
      out.result.payload.sealed,
      responseContext(f.request),
    ),
  );
  assert.deepEqual(JSON.parse(result), {
    available: [{ amount: 12, currency: "usd" }],
    pending: [],
    livemode: false,
  });
  assert.ok(!result.includes(f.secret));
  for (const operation of ["get", "export", "rewrap", "sign", "fetch"])
    assert.equal(
      (await f.h.run(f.manifest, { ...f.params, operation })).ok,
      false,
    );
});
test("Stripe redirects, upstream errors and reflected strings never expose a credential", async () => {
  for (const response of [
    () => json({ error: "secret" }, 401),
    () =>
      new Response(null, {
        status: 302,
        headers: { location: "https://evil.test" },
      }),
    () =>
      json({
        available: [{ amount: 1, currency: "sk_test_secret" }],
        pending: [],
      }),
  ]) {
    const f = await fixture("stripe_balance");
    f.h.extraFetch = async () => response();
    assert.deepEqual(await f.h.run(f.manifest, f.params), {
      ok: false,
      error: "access_denied",
    });
  }
});
test("authority treats a credentials receipt from another release like the original credential", async () => {
  // A vault whose owner set was approved under a different authority release:
  // this release cannot verify that receipt, so it sees only the root owner.
  const f = await fixture();
  const otherOwner = {
    kind: "wallet" as const,
    address: "0x" + "2".repeat(40),
  };
  const credentials = {
    v: 2 as const,
    domain: "lit-keychain/v2" as const,
    kind: "credentials" as const,
    vaultId: f.manifest.vaultId,
    epoch: 1,
    previousHash: null,
    owners: [otherOwner],
    notBefore: f.now - 10,
    expiresAt: null,
  };
  const foreignKey = keyFor("Qm" + "z".repeat(44));
  f.h.registry.set(
    `${f.authority.registry}/api/registry/credentials/${f.manifest.vaultId}`,
    {
      document: credentials,
      receipt: makeReceipt(credentials, foreignKey, f.now),
    },
  );
  // Root owner still authorizes (equivalent to the operator serving null)…
  const proof = await f.ownerProof(f.policy);
  const out = await f.h.run(f.authority, { document: f.policy, proof });
  assert.equal(out.ok, true);
  // …but the unverifiable owner set grants nothing: a forged owner is denied.
  const forged = structuredClone(proof);
  forged.owner = otherOwner;
  assert.equal(
    (await f.h.run(f.authority, { document: f.policy, proof: forged })).ok,
    false,
  );
  // A receipt from this release with the root owner removed is honoured (fail closed for root).
  const replaced = { ...credentials, owners: [otherOwner] };
  f.h.registry.set(
    `${f.authority.registry}/api/registry/credentials/${f.manifest.vaultId}`,
    {
      document: replaced,
      receipt: makeReceipt(replaced, keyFor(f.authorityCid), f.now),
    },
  );
  assert.equal(
    (await f.h.run(f.authority, { document: f.policy, proof })).ok,
    false,
  );
});
