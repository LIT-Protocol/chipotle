import { test } from "node:test";
import assert from "node:assert/strict";
import { p256 } from "@noble/curves/nist.js";
import { sha256 } from "@noble/hashes/sha2.js";
import { generateKeyPair, exportJWK, SignJWT } from "jose";
import { Harness, json, pubFor, keyFor } from "./harness.ts";
import { actionCid } from "../protocol/actions.ts";
import {
  randomBytes,
  randomId,
  hex,
  digest,
  utf8,
  b64u,
  unhex,
  agentPublicKey,
  signAgent,
  nowSeconds,
  makeReceipt,
} from "../protocol/crypto.ts";
import {
  DOMAIN,
  V,
  type Authority,
  type Owner,
  type Challenge,
  type GoogleSession,
} from "../protocol/schema.ts";
function objects(owner: Owner) {
  const h = new Harness();
  const now = nowSeconds();
  const authority: Authority = {
    v: V,
    network: "test",
    registry: "https://keychain.test",
    owner,
  };
  const vaultId = digest(authority);
  const document = {
    v: V,
    domain: DOMAIN,
    kind: "login" as const,
    vaultId,
    challenge: randomId(),
    expiresAt: now + 120,
  };
  const challenge: Challenge = {
    v: V,
    domain: "lit-keychain/authorize/v2",
    vaultId,
    objectHash: digest(document),
    operation: "login",
    nonce: randomId(),
    issuedAt: now,
    expiresAt: now + 120,
  };
  return { h, authority, vaultId, document, challenge, now };
}
test("WebAuthn verifies P-256, origin, RP hash, operation challenge, UP and UV inside action", async () => {
  const privateKey = p256.utils.randomSecretKey();
  const owner: Extract<Owner, { kind: "passkey" }> = {
    kind: "passkey",
    publicKey: hex(p256.getPublicKey(privateKey, false)),
    credentialId: b64u(randomBytes()),
    rpId: "keychain.test",
    origin: "https://keychain.test",
  };
  const f = objects(owner);
  function proof(overrides: any = {}) {
    const client = utf8(
      JSON.stringify({
        type: "webauthn.get",
        challenge: b64u(unhex(digest(f.challenge))),
        origin: owner.origin,
        crossOrigin: false,
        ...overrides.client,
      }),
    );
    const auth = new Uint8Array(37);
    auth.set(sha256(utf8(overrides.rp || owner.rpId)));
    auth[32] = overrides.flags ?? 5;
    const signed = new Uint8Array(auth.length + 32);
    signed.set(auth);
    signed.set(sha256(client), auth.length);
    return {
      kind: "passkey",
      owner,
      challenge: f.challenge,
      clientDataJSON: b64u(client),
      authenticatorData: b64u(auth),
      signature: b64u(p256.sign(signed, privateKey, { format: "der" })),
    };
  }
  assert.equal(
    (await f.h.run(f.authority, { document: f.document, proof: proof() })).ok,
    true,
  );
  for (const override of [
    { client: { origin: "https://evil.test" } },
    { client: { challenge: b64u(randomBytes()) } },
    { client: { type: "webauthn.create" } },
    { client: { crossOrigin: true } },
    { client: { topOrigin: "https://evil.test" } },
    { rp: "evil.test" },
    { flags: 1 },
    { flags: 4 },
    { flags: 0 },
  ]) {
    assert.equal(
      (
        await f.h.run(f.authority, {
          document: f.document,
          proof: proof(override),
        })
      ).ok,
      false,
      JSON.stringify(override),
    );
  }
});
test("Google-only login binds a locally held session key; fresh-device sessions keep the same owner", async () => {
  const { publicKey, privateKey } = await generateKeyPair("RS256", {
    modulusLength: 2048,
  });
  const jwk = {
    ...(await exportJWK(publicKey)),
    kid: "test-key",
    alg: "RS256",
    use: "sig",
  };
  const owner: Extract<Owner, { kind: "google" }> = {
    kind: "google",
    subject: "123456789",
    clientId: "test.apps.googleusercontent.com",
  };
  const f = objects(owner);
  f.h.extraFetch = async (url) => {
    assert.equal(String(url), "https://www.googleapis.com/oauth2/v3/certs");
    return json({ keys: [jwk] });
  };
  async function proof(overrides: any = {}) {
    const key = randomBytes();
    const session: GoogleSession = {
      v: V,
      domain: "lit-keychain/google-session/v2",
      network: "test",
      registry: f.authority.registry,
      publicKey: agentPublicKey(key),
      nonce: randomId(),
      issuedAt: f.now,
      expiresAt: f.now + 600,
      scope: "authorize",
      ...overrides.session,
    };
    const token = await new SignJWT({
      iss: "https://accounts.google.com",
      aud: owner.clientId,
      sub: owner.subject,
      iat: f.now,
      exp: f.now + 3600,
      nonce: b64u(unhex(digest(session))),
      ...overrides.claims,
    })
      .setProtectedHeader({ alg: "RS256", kid: "test-key" })
      .sign(privateKey);
    return {
      kind: "google",
      owner,
      challenge: f.challenge,
      token,
      session,
      signature: signAgent(f.challenge, key),
    };
  }
  for (let i = 0; i < 2; i++)
    assert.equal(
      (
        await f.h.run(f.authority, {
          document: f.document,
          proof: await proof(),
        })
      ).ok,
      true,
    );
  for (const overrides of [
    { claims: { iss: "https://evil.test" } },
    { claims: { aud: "other.apps.googleusercontent.com" } },
    { claims: { sub: "another-user" } },
    { claims: { exp: f.now - 1 } },
    { claims: { iat: f.now + 100 } },
    { claims: { nonce: "replayed-login-token" } },
    { claims: { azp: "wrong" } },
    { session: { expiresAt: f.now + 10000 } },
    { session: { network: "other" } },
    { session: { registry: "https://evil.test" } },
  ]) {
    assert.equal(
      (
        await f.h.run(f.authority, {
          document: f.document,
          proof: await proof(overrides),
        })
      ).ok,
      false,
      JSON.stringify(overrides),
    );
  }
  const stolen = await proof();
  stolen.session.publicKey = agentPublicKey(randomBytes());
  assert.equal(
    (await f.h.run(f.authority, { document: f.document, proof: stolen })).ok,
    false,
  );
  const substituted = await proof();
  substituted.signature = signAgent(f.challenge, randomBytes());
  assert.equal(
    (await f.h.run(f.authority, { document: f.document, proof: substituted }))
      .ok,
    false,
  );
});
test("credential replacement is owner-receipted; forged registry owners fail", async () => {
  const { fixture } = await import("./harness.ts");
  const f = await fixture();
  const credentials = {
    v: V,
    domain: DOMAIN,
    kind: "credentials" as const,
    vaultId: f.manifest.vaultId,
    epoch: 1,
    previousHash: null,
    owners: [{ kind: "wallet" as const, address: "0x" + "1".repeat(40) }],
    notBefore: f.now - 1,
    expiresAt: f.now + 3600,
  };
  const signed = {
    document: credentials,
    receipt: makeReceipt(credentials, keyFor(f.authorityCid), f.now),
  };
  f.h.registry.set(
    `${f.authority.registry}/api/registry/credentials/${f.manifest.vaultId}`,
    signed,
  );
  assert.equal(
    (
      await f.h.run(f.authority, {
        document: f.policy,
        proof: await f.ownerProof(f.policy),
      })
    ).ok,
    false,
  );
  // Tampering with the signed document breaks its receipt. An unverifiable
  // receipt is treated exactly like a null credential state (the operator could
  // serve null anyway): the root owner is accepted again, nothing else is.
  signed.document.owners = [f.authority.owner as any];
  assert.equal(
    (
      await f.h.run(f.authority, {
        document: f.policy,
        proof: await f.ownerProof(f.policy),
      })
    ).ok,
    true,
  );
  const forged = await f.ownerProof(f.policy);
  forged.owner = { kind: "wallet", address: "0x" + "1".repeat(40) };
  assert.equal(
    (await f.h.run(f.authority, { document: f.policy, proof: forged })).ok,
    false,
  );
});
