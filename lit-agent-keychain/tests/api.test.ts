import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { subscribe } from "./billing-fixture.ts";
import { privateKeyToAccount } from "viem/accounts";
import {
  OwnerClient,
  Keychain,
  LitConnection,
  authorizationTypedData,
  type Authority,
  type SecretBundle,
} from "../sdk/src/index.ts";
import { hex, randomBytes, digest, nowSeconds } from "../protocol/crypto.ts";
import { handleMessage } from "../sdk/mcp.mjs";
const api = process.env.KEYCHAIN_TEST_API;
const lit = process.env.KEYCHAIN_TEST_LIT || "http://127.0.0.1:55440";
test(
  "Postgres API + bundled actions + agent SDK: import, grant, rotate, revoke, concurrency, recovery",
  { skip: !api },
  async () => {
    assert.ok(
      ["127.0.0.1", "localhost"].includes(new URL(api!).hostname),
      "integration test must use loopback",
    );
    const original = globalThis.fetch;
    let cookie = "";
    const uploads: string[] = [];
    globalThis.fetch = async (input, init) => {
      if (String(input).startsWith(api!)) {
        const headers = new Headers(init?.headers);
        if (cookie) headers.set("Cookie", cookie);
        if (typeof init?.body === "string") uploads.push(init.body);
        const res = await original(input, { ...init, headers });
        const set = res.headers.get("set-cookie");
        if (set) cookie = set.split(";")[0];
        return res;
      }
      return original(input, init);
    };
    try {
      const account = privateKeyToAccount(`0x${hex(randomBytes())}`);
      const owner = {
        kind: "wallet" as const,
        address: account.address.toLowerCase(),
      };
      const authority: Authority = {
        v: 2,
        network: "test",
        registry: api!,
        owner,
      };
      const c = new OwnerClient(
        authority,
        async (challenge) => ({
          kind: "wallet",
          owner,
          challenge,
          signature: await account.signTypedData(
            authorizationTypedData(challenge),
          ),
        }),
        new LitConnection(lit),
      );
      await c.login();
      await subscribe(c);
      execFileSync(
        "target/debug/keychain-plan",
        [c.vaultId, "3", new Date(Date.now() + 86400000).toISOString()],
        { env: process.env },
      );
      assert.equal((await c.api("/api/me")).vaultId, c.vaultId);
      let bundle = await c.create("API_TEST", "local-only-secret-7f9ba");
      const keys = Keychain.generateKey();
      const agent = new Keychain(keys.privateKey, {
        v: 2,
        litApiUrl: lit,
        usageApiKey: c.lit.usageApiKey,
        secrets: {
          API_TEST: {
            manifest: bundle.manifest.document.manifest,
            actionCid: bundle.manifest.document.actionCid,
          },
        },
      });
      await assert.rejects(agent.get("API_TEST"), /denied/i);
      bundle = await c.delegate(bundle, keys.publicKey, "Integration agent");
      assert.equal(await agent.get("API_TEST"), "local-only-secret-7f9ba");
      // Every API response must be strict JSON (RFC 8259): no raw control
      // characters inside strings, so jq and other strict parsers accept it.
      const rawBundle = await (
        await fetch(
          `${api}/api/secrets/${bundle.manifest.document.manifest.secretId}/bundle`,
        )
      ).text();
      assert.doesNotMatch(rawBundle, /[\u0000-\u001f]/);
      JSON.parse(rawBundle);
      // The local MCP server reads through the same agent identity.
      const mcpGet = await handleMessage(agent, {
        jsonrpc: "2.0",
        id: 7,
        method: "tools/call",
        params: { name: "get_secret", arguments: { name: "API_TEST" } },
      });
      assert.deepEqual(mcpGet, {
        jsonrpc: "2.0",
        id: 7,
        result: {
          content: [{ type: "text", text: "local-only-secret-7f9ba" }],
        },
      });
      assert.ok(
        uploads.every(
          (body) =>
            !body.includes("local-only-secret-7f9ba") &&
            !body.includes(keys.privateKey),
        ),
      );
      const stale = bundle;
      const earlyBackup = await c.backup();
      bundle = await c.rotate(bundle, "rotated-only-in-browser");
      assert.equal(await agent.get("API_TEST"), "rotated-only-in-browser");
      await c.restore(earlyBackup);
      assert.equal(await agent.get("API_TEST"), "rotated-only-in-browser");
      await assert.rejects(c.setPolicy(stale, { disabled: true }), /409/);
      const forged = structuredClone(bundle.policy);
      forged.document.grants[0].label = "forged";
      await assert.rejects(
        c.api(
          `/api/secrets/${bundle.manifest.document.manifest.secretId}/policy`,
          {
            method: "PUT",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(forged),
          },
        ),
        /403/,
      );
      bundle = await c.setPolicy(bundle, { disabled: true });
      await assert.rejects(agent.get("API_TEST"), /denied/i);
      const attempts = await Promise.allSettled(
        Array.from({ length: 5 }, (_, n) =>
          c.create(`RACE_${n}`, "concurrent-secret"),
        ),
      );
      assert.equal(attempts.filter((r) => r.status === "fulfilled").length, 2);
      assert.equal((await c.api("/api/secrets")).secrets.length, 3);
      const backup = await c.backup();
      assert.equal(backup.bundles.length, 3);
      assert.ok(!JSON.stringify(backup).includes("rotated-only-in-browser"));
      await c.restore(backup);
      await c.restore(backup);
      for (const path of [
        "/api/grants",
        "/api/reference/API_TEST",
        "/agent/authorize",
        "/auth/request",
      ]) {
        const response = await original(api + path, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: "{}",
        });
        assert.equal(response.status, 404);
      }
      const crossSite = await original(api + "/api/secrets", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Cookie: cookie,
          Origin: "https://evil.test",
        },
        body: JSON.stringify(bundle),
      });
      assert.equal(crossSite.status, 403);
      const events = (await c.api("/api/audit")).events.map(
        (e: any) => e.event,
      );
      assert.ok(events.includes("secret_rotated"));
      assert.ok(events.includes("policy_updated"));
      const recoveryAccount = privateKeyToAccount(`0x${hex(randomBytes())}`);
      const recoveryOwner = {
        kind: "wallet" as const,
        address: recoveryAccount.address.toLowerCase(),
      };
      await c.updateCredentials([recoveryOwner]);
      await assert.rejects(c.api("/api/me"), /401/);
      await assert.rejects(c.login(), /denied/i);
      const recovered = new OwnerClient(
        authority,
        async (challenge) => ({
          kind: "wallet",
          owner: recoveryOwner,
          challenge,
          signature: await recoveryAccount.signTypedData(
            authorizationTypedData(challenge),
          ),
        }),
        new LitConnection(lit),
      );
      await recovered.login();
      assert.equal((await recovered.api("/api/secrets")).secrets.length, 3);
      // Reconstitute a missing vault from a signed credential backup, without the root key.
      const freshAccount = privateKeyToAccount(`0x${hex(randomBytes())}`);
      const freshOwner = {
        kind: "wallet" as const,
        address: freshAccount.address.toLowerCase(),
      };
      const freshAuthority = { ...authority, owner: freshOwner };
      const freshClient = new OwnerClient(
        freshAuthority,
        async (challenge) => ({
          kind: "wallet",
          owner: freshOwner,
          challenge,
          signature: await freshAccount.signTypedData(
            authorizationTypedData(challenge),
          ),
        }),
        new LitConnection(lit),
      );
      const now = nowSeconds();
      const credentials = await freshClient.authorize({
        v: 2,
        domain: "lit-keychain/v2",
        kind: "credentials",
        vaultId: freshClient.vaultId,
        epoch: 3,
        previousHash: hex(randomBytes()),
        owners: [recoveryOwner],
        notBefore: now,
        expiresAt: null,
      });
      // Authorization now signs in first. Remove this test-only vault to model
      // a lost database before exercising signed recovery initialization.
      const database = new URL(process.env.KEYCHAIN_TEST_DATABASE_URL!);
      assert.ok(["127.0.0.1", "localhost"].includes(database.hostname));
      assert.match(database.pathname, /test|_ci$/);
      execFileSync(
        "psql",
        [
          database.toString(),
          "-X",
          "-v",
          "ON_ERROR_STOP=1",
          "-v",
          `vault=${freshClient.vaultId}`,
        ],
        {
          input:
            "BEGIN; DELETE FROM kc_execution_accounts WHERE vault_id=:'vault'; DELETE FROM kc_subscriptions WHERE vault_id=:'vault'; DELETE FROM kc_audit WHERE vault_id=:'vault'; DELETE FROM kc_sessions WHERE vault_id=:'vault'; DELETE FROM kc_vaults WHERE id=:'vault'; COMMIT;",
        },
      );
      await OwnerClient.restoreCredentials(
        { authority: freshAuthority, credentials },
        new LitConnection(lit),
      );
      const restoredClient = new OwnerClient(
        freshAuthority,
        async (challenge) => ({
          kind: "wallet",
          owner: recoveryOwner,
          challenge,
          signature: await recoveryAccount.signTypedData(
            authorizationTypedData(challenge),
          ),
        }),
        new LitConnection(lit),
      );
      await restoredClient.login();
      await restoredClient.updateCredentials([freshOwner]);
      await OwnerClient.restoreCredentials(
        { authority: freshAuthority, credentials },
        new LitConnection(lit),
      );
      await assert.rejects(restoredClient.login(), /denied/i);
      await freshClient.login();
      const badCredentials = structuredClone(credentials);
      badCredentials.document.owners = [freshOwner];
      await assert.rejects(
        OwnerClient.restoreCredentials(
          { authority: freshAuthority, credentials: badCredentials },
          new LitConnection(lit),
        ),
      );
      agent.destroy();
    } finally {
      globalThis.fetch = original;
    }
  },
);
