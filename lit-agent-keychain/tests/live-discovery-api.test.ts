import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { privateKeyToAccount } from "viem/accounts";
import {
  OwnerClient,
  LiveKeychain,
  Keychain,
  LitConnection,
  authorizationTypedData,
  type SecretBundle,
} from "../sdk/src/index.ts";
import { hex, randomBytes, signAgent, unhex } from "../protocol/crypto.ts";
import { handleMessage } from "../sdk/mcp.mjs";
const api = process.env.KEYCHAIN_TEST_API;
const lit = process.env.KEYCHAIN_TEST_LIT || "http://127.0.0.1:55440";
test(
  "live authenticated discovery: replay isolation and same-client owner lifecycle",
  { skip: !api },
  async () => {
    assert.ok(["localhost", "127.0.0.1"].includes(new URL(api!).hostname));
    const original = globalThis.fetch;
    let cookie = "";
    const uploads: string[] = [];
    globalThis.fetch = async (url, init) => {
      if (!String(url).startsWith(api!)) return original(url, init);
      const headers = new Headers(init?.headers);
      if (cookie) headers.set("Cookie", cookie);
      if (typeof init?.body === "string") uploads.push(init.body);
      const response = await original(url, { ...init, headers });
      if (response.headers.has("set-cookie"))
        cookie = response.headers.get("set-cookie")!.split(";")[0];
      return response;
    };
    const post = (path: string, body: unknown) =>
      original(api + path, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });
    const a = Keychain.generateKey(),
      b = Keychain.generateKey();
    const client = new LiveKeychain(a.privateKey, {
      serviceUrl: api!,
      litApiUrl: lit,
    });
    const other = new LiveKeychain(b.privateKey, {
      serviceUrl: api!,
      litApiUrl: lit,
    });
    const owner = async () => {
      const account = privateKeyToAccount(`0x${hex(randomBytes())}`);
      const descriptor = {
        kind: "wallet" as const,
        address: account.address.toLowerCase(),
      };
      const c = new OwnerClient(
        { v: 2, network: "test", registry: api!, owner: descriptor },
        async (challenge) => ({
          kind: "wallet",
          owner: descriptor,
          challenge,
          signature: await account.signTypedData(
            authorizationTypedData(challenge),
          ),
        }),
        new LitConnection(lit),
      );
      await c.login();
      return c;
    };
    try {
      const c = await owner();
      let bundle = await c.create("LIVE_TEST", "local-live-secret");
      assert.deepEqual(await client.list(), []);
      await assert.rejects(client.get("LIVE_TEST"), /Unknown secret/);
      const challenge = await (
        await post("/api/agents/challenge", { agentPublicKey: a.publicKey })
      ).json();
      const proof = {
        challenge,
        signature: signAgent(challenge, unhex(a.privateKey)),
      };
      assert.equal(
        (
          await post("/api/agents/discover", {
            ...proof,
            signature: signAgent(challenge, unhex(b.privateKey)),
          })
        ).status,
        403,
      );
      assert.equal(
        (
          await post("/api/agents/discover", {
            ...proof,
            challenge: { ...challenge, audience: "https://wrong.example" },
          })
        ).status,
        403,
      );
      assert.equal((await post("/api/agents/discover", proof)).status, 200);
      assert.equal((await post("/api/agents/discover", proof)).status, 403);
      const expiringChallenge = await (
        await post("/api/agents/challenge", { agentPublicKey: a.publicKey })
      ).json();
      assert.match(expiringChallenge.nonce, /^[0-9a-f]{64}$/);
      // Move only this dedicated fixture nonce past its stored deadline. Signature
      // still claims a valid window, proving storage expiry is independently checked.
      if (process.env.KEYCHAIN_TEST_DATABASE_URL) {
        execFileSync("psql", [
          process.env.KEYCHAIN_TEST_DATABASE_URL,
          "-v",
          "ON_ERROR_STOP=1",
          "-c",
          `UPDATE kc_agent_challenges SET expires_at=now()-interval '1 second' WHERE nonce='${expiringChallenge.nonce}'`,
        ]);
        assert.equal(
          (
            await post("/api/agents/discover", {
              challenge: expiringChallenge,
              signature: signAgent(expiringChallenge, unhex(a.privateKey)),
            })
          ).status,
          403,
        );
      }
      const expired = { ...challenge, expiresAt: 1 };
      assert.equal(
        (
          await post("/api/agents/discover", {
            challenge: expired,
            signature: signAgent(expired, unhex(a.privateKey)),
          })
        ).status,
        403,
      );
      assert.equal((await post("/api/agents/discover", {})).status, 422);
      bundle = await c.delegate(bundle, a.publicKey, "agent A");
      const allowedChallenge = await (
        await post("/api/agents/challenge", { agentPublicKey: a.publicKey })
      ).json();
      const allowedProof = {
        challenge: allowedChallenge,
        signature: signAgent(allowedChallenge, unhex(a.privateKey)),
      };
      const concurrent = await Promise.all([
        post("/api/agents/discover", allowedProof),
        post("/api/agents/discover", allowedProof),
      ]);
      assert.deepEqual(concurrent.map((r) => r.status).sort(), [200, 403]);
      const metadata = await concurrent.find((r) => r.status === 200)!.json();
      assert.deepEqual(Object.keys(metadata.secrets[0]).sort(), [
        "actionCid",
        "manifest",
        "name",
        "usageApiKey",
      ]);
      assert.equal(metadata.secrets[0].usageApiKey, c.lit.usageApiKey);
      assert.doesNotMatch(
        JSON.stringify(metadata),
        /local-live-secret|local-master-only|local-test-only|privateKey|grants|owners/,
      );
      assert.equal((await client.list())[0].name, "LIVE_TEST");
      assert.equal(await client.get("LIVE_TEST"), "local-live-secret");
      assert.deepEqual(await other.list(), []);
      bundle = await c.delegate(bundle, b.publicKey, "agent B");
      assert.equal(await other.get("LIVE_TEST"), "local-live-secret");
      bundle = await c.setPolicy(bundle, {
        grants: bundle.policy.document.grants.filter(
          (g) => g.agentPublicKey !== a.publicKey,
        ),
      });
      assert.deepEqual(await client.list(), []);
      await assert.rejects(client.get("LIVE_TEST"), /Unknown secret/);
      assert.equal(await other.get("LIVE_TEST"), "local-live-secret");
      // Rotation replaces only the billing credential; existing live clients
      // discover it without a config refresh or restart.
      c.lit.usageApiKey = (
        await c.api("/api/execution-key/rotate", { method: "POST" })
      ).usageApiKey;
      assert.equal(await other.get("LIVE_TEST"), "local-live-secret");
      bundle = await c.delegate(bundle, a.publicKey, "agent A");
      const list = await handleMessage(client, {
        jsonrpc: "2.0",
        id: 1,
        method: "tools/call",
        params: { name: "list_secrets" },
      });
      assert.match(JSON.stringify(list), /LIVE_TEST/);
      const dir = mkdtempSync(`${tmpdir()}/keychain-live-`);
      try {
        const file = `${dir}/identity.json`;
        writeFileSync(file, JSON.stringify({ v: 2, ...a }), { mode: 0o600 });
        const env = {
          ...process.env,
          KEYCHAIN_SERVICE_URL: api!,
          KEYCHAIN_LIT_API_URL: lit,
          KEYCHAIN_SKIP_ATTESTATION: "1",
        };
        const run = (...args: string[]) =>
          execFileSync(process.execPath, ["sdk/cli.mjs", ...args], {
            encoding: "utf8",
            env,
          });
        assert.match(run("list", file), /LIVE_TEST/);
        assert.equal(run("get", file, "LIVE_TEST").trim(), "local-live-secret");
        assert.equal(
          run(
            "run",
            file,
            "--only",
            "LIVE_TEST",
            "--",
            process.execPath,
            "-e",
            "process.stdout.write(process.env.LIVE_TEST)",
          ),
          "local-live-secret",
        );
      } finally {
        rmSync(dir, { recursive: true, force: true });
      }
      // Each case starts from valid grants, so failures are not masked by a
      // previously disabled policy. Use actual owner receipts and API writes.
      const valid = bundle;
      for (const alter of [
        (p: any) => {
          p.disabled = true;
        },
        (p: any) => {
          p.grants.forEach((g: any) => (g.operations = ["stripe.balance"]));
        },
        (p: any) => {
          p.grants.forEach(
            (g: any) => (g.versions[0].envelopeHash = "00".repeat(32)),
          );
        },
        (p: any) => {
          p.grants.forEach((g: any) => (g.versions[0].version = 2));
        },
      ]) {
        const p = structuredClone(valid.policy.document);
        alter(p);
        p.epoch = bundle.policy.document.epoch + 1;
        p.previousHash = (await import("../protocol/crypto.ts")).digest(
          bundle.policy.document,
        );
        const signed = await c.authorize(
          p,
          bundle.manifest.document.manifest.authorityCid,
        );
        await c.api(`/api/secrets/${p.secretId}/policy`, {
          method: "PUT",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(signed),
        });
        bundle = { ...bundle, policy: signed } as SecretBundle;
        assert.deepEqual(await client.list(), []);
        await assert.rejects(client.get("LIVE_TEST"), /Unknown secret/);
      }
      // Real clock expiry, not a mocked policy check.
      const short = {
        ...valid.policy.document,
        epoch: bundle.policy.document.epoch + 1,
        previousHash: (await import("../protocol/crypto.ts")).digest(
          bundle.policy.document,
        ),
        expiresAt: Math.floor(Date.now() / 1000) + 3,
      };
      const expiring = await c.authorize(
        short,
        bundle.manifest.document.manifest.authorityCid,
      );
      await c.api(`/api/secrets/${short.secretId}/policy`, {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(expiring),
      });
      bundle = { ...bundle, policy: expiring };
      await new Promise((resolve) => setTimeout(resolve, 3100));
      assert.deepEqual(await client.list(), []);
      await assert.rejects(client.get("LIVE_TEST"), /Unknown secret/);
      // Restore the valid grant through a fresh owner approval, not DB rewriting.
      const p = {
        ...valid.policy.document,
        epoch: bundle.policy.document.epoch + 1,
        previousHash: (await import("../protocol/crypto.ts")).digest(
          bundle.policy.document,
        ),
      };
      const signed = await c.authorize(
        p,
        bundle.manifest.document.manifest.authorityCid,
      );
      await c.api(`/api/secrets/${p.secretId}/policy`, {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(signed),
      });
      // Same name in another vault must never silently select a different secret.
      const secondOwner = await owner();
      let second = await secondOwner.create("LIVE_TEST", "other-vault-value");
      second = await secondOwner.delegate(second, a.publicKey, "agent A");
      assert.equal((await client.list()).length, 2);
      const otherInventory = await other.list();
      assert.equal(otherInventory.length, 1);
      assert.equal(otherInventory[0].vaultId, c.vaultId);
      await assert.rejects(client.get("LIVE_TEST"), /ambiguous/i);
      assert.equal(
        await client.get(
          `${secondOwner.vaultId}/${second.manifest.document.manifest.secretId}`,
        ),
        "other-vault-value",
      );
      assert.ok(
        uploads.every(
          (body) =>
            !body.includes(a.privateKey) &&
            !body.includes(b.privateKey) &&
            !body.includes("local-live-secret"),
        ),
      );
    } finally {
      client.destroy();
      other.destroy();
      globalThis.fetch = original;
    }
  },
);
