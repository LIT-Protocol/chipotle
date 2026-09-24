import { test } from "node:test";
import assert from "node:assert/strict";
import { LiveKeychain } from "../sdk/src/index.ts";
import { fixture, pubFor } from "./harness.ts";
import { hex, nowSeconds, randomId } from "../protocol/crypto.ts";

for (const release of ["export", "stripe_balance"])
  test(`live ${release} verifies bundles, rejects substitution, and respects Lit revocation`, async () => {
    const f = await fixture(release);
    const balances = {
      available: [{ amount: 12, currency: "usd" }],
      pending: [],
      livemode: false,
    };
    f.h.extraFetch = async (url) => {
      assert.equal(String(url), "https://api.stripe.com/v1/balance");
      return Response.json(balances);
    };
    const client = new LiveKeychain(hex(f.agentKey), {
      serviceUrl: f.manifest.registry,
      litApiUrl: "https://trusted-lit.test",
    });
    const read = (name: string) =>
      release === "export" ? client.get(name) : client.use(name);
    const bundle = {
      manifest: f.sign({
        v: 2,
        domain: "lit-keychain/v2",
        kind: "manifest",
        vaultId: f.manifest.vaultId,
        manifest: f.manifest,
        actionCid: f.cid,
      }),
      envelope: f.sign(f.envelope),
      policy: f.sign(f.policy),
    };
    let name = "TEST_SECRET";
    let tamper = false;
    const original = globalThis.fetch;
    let executions = 0;
    globalThis.fetch = async (input, init) => {
      const url = String(input);
      if (url.endsWith("/challenge")) {
        const now = nowSeconds();
        return Response.json({
          v: 2,
          domain: "lit-keychain/discovery/v2",
          audience: f.manifest.registry,
          agentPublicKey: client.publicKey,
          nonce: randomId(),
          issuedAt: now,
          expiresAt: now + 60,
        });
      }
      if (url.endsWith("/discover"))
        return Response.json({
          v: 2,
          litApiUrl: "https://evil.test",
          secrets: [
            {
              name,
              manifest: f.manifest,
              actionCid: f.cid,
              usageApiKey: "test-only-billing",
            },
          ],
        });
      if (url.endsWith("/bundle")) {
        const b = structuredClone(bundle);
        if (tamper) b.policy.receipt.signature = "00".repeat(64);
        return Response.json(b);
      }
      assert.equal(url, "https://trusted-lit.test/core/v1/lit_action");
      const body = JSON.parse(String(init?.body));
      if (body.js_params.cid)
        return Response.json({
          has_error: false,
          response: { ok: true, public_key: pubFor(body.js_params.cid) },
        });
      executions++;
      return Response.json({
        has_error: false,
        response: await f.h.run(f.manifest, body.js_params),
      });
    };
    try {
      assert.equal((await client.list())[0].name, "TEST_SECRET");
      assert.deepEqual(
        await read("TEST_SECRET"),
        release === "export" ? f.secret : balances,
      );
      assert.equal(executions, 1);
      name = "SUBSTITUTED_NAME";
      await assert.rejects(client.list(), /substitution/i);
      await assert.rejects(read(name), /substitution/i);
      assert.equal(
        executions,
        1,
        "must not execute a different secret under a service-supplied alias",
      );
      name = "TEST_SECRET";
      tamper = true;
      await assert.rejects(client.list());
      await assert.rejects(read(name));
      assert.equal(executions, 1);
      tamper = false;
      // Discovery and the fetched bundle can be stale; Lit still independently
      // fetches the current owner-signed policy and denies this same live client.
      f.h.registry.set(f.registryUrl, f.sign({ ...f.policy, grants: [] }));
      await assert.rejects(read(name), /denied/i);
      assert.equal(executions, 2);
    } finally {
      client.destroy();
      globalThis.fetch = original;
    }
  });
