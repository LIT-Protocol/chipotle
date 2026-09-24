import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { approveAgentSecrets } from "../web/src/agent-onboarding.ts";
import type { SecretBundle } from "../sdk/src/index.ts";

const key = "ab".repeat(32); // Synthetic, never a user's key.
const bundle = (id: string, changes = {}) =>
  ({
    manifest: {
      document: {
        manifest: { secretId: id, release: "export" },
        actionCid: "test",
      },
    },
    envelope: { document: { metadata: { name: id } } },
    policy: {
      document: { disabled: false, expiresAt: null, grants: [], ...changes },
    },
  }) as unknown as SecretBundle;
function fixture(values = [bundle("ONE"), bundle("TWO")]) {
  const calls: string[] = [];
  return {
    calls,
    client: {
      bundle: async (id: string) =>
        values.find((b) => b.manifest.document.manifest.secretId === id)!,
      delegate: async (b: SecretBundle, publicKey: string, label: string) => {
        calls.push(b.manifest.document.manifest.secretId);
        return {
          ...b,
          policy: {
            ...b.policy,
            document: {
              ...b.policy.document,
              grants: [{ agentPublicKey: publicKey, label }],
            },
          },
        } as SecretBundle;
      },
    },
  };
}
test("explicit selection grants only chosen secrets with canonical public key", async () => {
  const { client, calls } = fixture();
  const result = await approveAgentSecrets(client, key.toUpperCase(), "Agent", [
    "TWO",
  ]);
  assert.deepEqual(calls, ["TWO"]);
  assert.equal(
    result.approved[0].policy.document.grants[0].agentPublicKey,
    key,
  );
  assert.equal(result.error, undefined);
});
test("reject malformed keys, blank names and empty selection before writes", async () => {
  const { client, calls } = fixture();
  for (const bad of [
    "",
    "0x" + key,
    key + " ",
    "private-key",
    '{"publicKey":"' + key + '"}',
  ])
    await assert.rejects(
      approveAgentSecrets(client, bad, "Agent", ["ONE"]),
      /64 hex/,
    );
  await assert.rejects(approveAgentSecrets(client, key, " ", ["ONE"]), /name/);
  await assert.rejects(approveAgentSecrets(client, key, "Agent", []), /Select/);
  assert.deepEqual(calls, []);
});
test("preflight rejects disabled or expired secrets without changing any policy", async () => {
  for (const policy of [{ disabled: true }, { expiresAt: 1 }]) {
    const { client, calls } = fixture([bundle("ONE"), bundle("TWO", policy)]);
    await assert.rejects(
      approveAgentSecrets(client, key, "Agent", ["ONE", "TWO"]),
      /disabled or expired/,
    );
    assert.deepEqual(calls, []);
  }
});
test("duplicate selection is written once; existing approval is not rewritten", async () => {
  const { client, calls } = fixture([
    bundle("ONE", { grants: [{ agentPublicKey: key, label: "Existing" }] }),
  ]);
  const result = await approveAgentSecrets(client, key, "Agent", [
    "ONE",
    "ONE",
  ]);
  assert.deepEqual(calls, []);
  assert.equal(result.approved.length, 1);
});
test("partial failure reports completed approvals and stops, allowing safe retry", async () => {
  const { client, calls } = fixture();
  const delegate = client.delegate;
  client.delegate = async (b, k, n) => {
    if (b.manifest.document.manifest.secretId === "TWO")
      throw new Error("Owner declined");
    return delegate(b, k, n);
  };
  const result = await approveAgentSecrets(client, key, "Agent", [
    "ONE",
    "TWO",
  ]);
  assert.deepEqual(calls, ["ONE"]);
  assert.equal(result.approved.length, 1);
  assert.match(result.error!, /TWO.*Owner declined/);
});
test("onboarding preserves exact expiry and unrelated grants through the real SDK policy path", async () => {
  const { OwnerClient } = await import("../sdk/src/index.ts");
  const { fixture: actionFixture } = await import("./harness.ts");
  const f = await actionFixture();
  for (const expiresAt of [null, f.now + 3600, f.now + 200 * 86400]) {
    const original = { ...f.policy, expiresAt };
    const b = {
      manifest: { document: { manifest: f.manifest, actionCid: f.cid } },
      envelope: f.sign(f.envelope),
      policy: f.sign(original),
    } as SecretBundle;
    const written: unknown[] = [];
    const owner = Object.create(OwnerClient.prototype);
    owner.bundle = async () => b;
    owner.policyLifetimeCapDays = async () => null;
    // Only the external signer/storage boundary is simulated; delegate/setPolicy are real.
    owner.authorize = async (document: any) => f.sign(document);
    owner.api = async (_path: string, init: any) => {
      written.push(init);
    };
    const result = await approveAgentSecrets(owner, key, "New agent", [
      f.manifest.secretId,
    ]);
    assert.equal(result.error, undefined);
    assert.equal(result.approved[0].policy.document.expiresAt, expiresAt);
    assert.deepEqual(
      result.approved[0].policy.document.grants[0],
      original.grants[0],
    );
    assert.equal(written.length, 1);
  }
});

test("onboarding uses the real SDK without renewing existing grants or expiry", async () => {
  const { OwnerClient, LitConnection } = await import("../sdk/src/index.ts");
  const { fixture: signedFixture } = await import("./harness.ts");
  const f = await signedFixture();
  const c = new OwnerClient(
    f.authority,
    async () => {
      throw new Error("unused fixture signer");
    },
    new LitConnection("https://lit.invalid"),
  );
  // Simulate only transport and owner signing; exercise real delegation/policy construction.
  c.authorize = async (document: any) => f.sign(document) as any;
  c.api = async () => ({});
  c.policyLifetimeCapDays = async () => null;
  for (const expiry of [f.now + 60, f.now + 86400 * 180, null]) {
    const b = {
      manifest: { document: { manifest: f.manifest, actionCid: f.cid } },
      envelope: f.sign(f.envelope),
      policy: f.sign({ ...f.policy, expiresAt: expiry }),
    } as SecretBundle;
    c.bundle = async () => b;
    const result = await approveAgentSecrets(c, key, "New agent", [
      f.manifest.secretId,
    ]);
    assert.equal(result.error, undefined);
    assert.equal(result.approved[0].policy.document.expiresAt, expiry);
    assert.deepEqual(
      result.approved[0].policy.document.grants[0],
      f.policy.grants[0],
    );
    assert.equal(result.approved[0].policy.document.disabled, false);
  }
});

test("SDK expiry preservation rejects conflicting renewal and respects legacy caps", async () => {
  const { OwnerClient } = await import("../sdk/src/index.ts");
  const { fixture: signedFixture } = await import("./harness.ts");
  const f = await signedFixture();
  const b = {
    manifest: { document: { manifest: f.manifest, actionCid: f.cid } },
    envelope: f.sign(f.envelope),
    policy: f.sign({ ...f.policy, expiresAt: null }),
  } as SecretBundle;
  const c = Object.create(OwnerClient.prototype);
  let signatures = 0;
  c.authorize = async (document: any) => {
    signatures++;
    return f.sign(document);
  };
  c.api = async () => ({});
  c.policyLifetimeCapDays = async () => 90;
  await assert.rejects(
    c.setPolicy(b, { preserveExpiry: true }),
    /limits permissions to 90 days/,
  );
  await assert.rejects(c.setPolicy(b, { preserveExpiry: true, days: 1 }));
  assert.equal(signatures, 0);
  const renewed = await c.setPolicy(b, { days: 1 });
  assert.ok(renewed.policy.document.expiresAt >= f.now + 86400);
  assert.equal(signatures, 1);
});

test("onboarding public instructions use discoverable labels, not removed controls", async () => {
  for (const path of [
    "SKILL.md",
    "web/public/llms.txt",
    "../docs/keychain/quickstart.mdx",
    "../docs/keychain/agents.mdx",
  ]) {
    const text = await readFile(new URL(`../${path}`, import.meta.url), "utf8");
    assert.match(text, /Add agent/);
    assert.match(text, /Approve selected secrets/);
    assert.match(text, /Download agent config/);
    assert.doesNotMatch(
      text,
      /Config · all secrets|Execution and account access/,
    );
  }
});

test("website removes managed execution controls and exposes Add agent", async () => {
  const source = await readFile(
    new URL("../web/src/main.tsx", import.meta.url),
    "utf8",
  );
  assert.doesNotMatch(
    source,
    /Execution and account access|Replace execution key|\/api\/execution-key\/rotate/,
  );
  assert.match(source, /Add agent/);
  assert.match(source, /<AgentOnboarding/);
});
