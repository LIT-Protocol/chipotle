import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
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
      const signedOperations: string[] = [];
      const c = new OwnerClient(
        authority,
        async (challenge) => {
          signedOperations.push(challenge.operation);
          return {
            kind: "wallet",
            owner,
            challenge,
            signature: await account.signTypedData(
              authorizationTypedData(challenge),
            ),
          };
        },
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
      signedOperations.length = 0;
      let bundle = await c.create("API_TEST", "local-only-secret-7f9ba");
      // Manifest, ciphertext and policy are approved with one owner signature.
      assert.deepEqual(signedOperations, ["batch"]);
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
      // `keychain run` injects the value into a child's environment and prints
      // nothing itself.
      {
        const dir = mkdtempSync(path.join(tmpdir(), "keychain-run-"));
        try {
          const identityFile = path.join(dir, "identity.json");
          const configFile = path.join(dir, "API_TEST.keychain.json");
          writeFileSync(identityFile, JSON.stringify({ v: 2, ...keys }), {
            mode: 0o600,
          });
          writeFileSync(
            configFile,
            JSON.stringify({
              v: 2,
              litApiUrl: lit,
              usageApiKey: c.lit.usageApiKey,
              secrets: {
                API_TEST: {
                  manifest: bundle.manifest.document.manifest,
                  actionCid: bundle.manifest.document.actionCid,
                },
              },
            }),
          );
          const stdout = execFileSync(
            process.execPath,
            [
              "sdk/cli.mjs",
              "run",
              identityFile,
              configFile,
              "--env",
              "API_TEST=INJECTED",
              "--",
              process.execPath,
              "-e",
              'process.stdout.write(JSON.stringify([process.env.INJECTED, "API_TEST" in process.env]))',
            ],
            {
              encoding: "utf8",
              stdio: ["ignore", "pipe", "pipe"],
              env: { ...process.env, KEYCHAIN_SKIP_ATTESTATION: "1" },
            },
          );
          assert.deepEqual(JSON.parse(stdout), [
            "local-only-secret-7f9ba",
            false,
          ]);
          // --file: private file for the child's lifetime, gone afterwards.
          const secretFile = path.join(dir, "api-test.txt");
          const fromFile = execFileSync(
            process.execPath,
            [
              "sdk/cli.mjs",
              "run",
              identityFile,
              configFile,
              "--file",
              `API_TEST=${secretFile}`,
              "--",
              process.execPath,
              "-e",
              `const fs=require("node:fs");process.stdout.write(JSON.stringify([fs.readFileSync(${JSON.stringify(secretFile)},"utf8"),(fs.statSync(${JSON.stringify(secretFile)}).mode&0o777).toString(8),"API_TEST" in process.env]))`,
            ],
            {
              encoding: "utf8",
              stdio: ["ignore", "pipe", "pipe"],
              env: { ...process.env, KEYCHAIN_SKIP_ATTESTATION: "1" },
            },
          );
          assert.deepEqual(JSON.parse(fromFile), [
            "local-only-secret-7f9ba",
            "600",
            false,
          ]);
          assert.equal(existsSync(secretFile), false);
          assert.throws(
            () =>
              execFileSync(
                process.execPath,
                [
                  "sdk/cli.mjs",
                  "run",
                  identityFile,
                  configFile,
                  "--",
                  process.execPath,
                  "-e",
                  "process.exit(7)",
                ],
                {
                  stdio: "pipe",
                  env: { ...process.env, KEYCHAIN_SKIP_ATTESTATION: "1" },
                },
              ),
            (error: any) => error.status === 7,
          );
        } finally {
          rmSync(dir, { recursive: true, force: true });
        }
      }
      const stale = bundle;
      const earlyBackup = await c.backup();
      signedOperations.length = 0;
      bundle = await c.rotate(bundle, "rotated-only-in-browser");
      assert.deepEqual(signedOperations, ["batch"]);
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
      const listing = (await c.api("/api/secrets")).secrets;
      assert.equal(listing.length, 3);
      // The Agents page groups by these; they are the policy's grants verbatim.
      const listed = listing.find(
        (s: any) => s.secretId === bundle.manifest.document.manifest.secretId,
      );
      assert.deepEqual(listed.agents, bundle.policy.document.grants);
      assert.equal(listed.agentCount, listed.agents.length);
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
test(
  "authority release transition: old vaults keep working, sign-in moves them forward, old secrets stay manageable",
  { skip: !api },
  async () => {
    const { default: archiveIndex } =
      await import("../generated/archive-index.ts");
    const { actionCid, templateStore } = await import("../protocol/actions.ts");
    const [newest, previous] = archiveIndex.authority;
    assert.ok(previous, "the archive must hold an earlier authority release");
    const original = globalThis.fetch;
    const jars = new Map<string, string>();
    let active = "old";
    globalThis.fetch = async (input, init) => {
      if (String(input).startsWith(api!)) {
        const headers = new Headers(init?.headers);
        const cookie = jars.get(active);
        if (cookie) headers.set("Cookie", cookie);
        const res = await original(input, { ...init, headers });
        const set = res.headers.get("set-cookie");
        if (set) jars.set(active, set.split(";")[0]);
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
      const signer = async (challenge: any) => ({
        kind: "wallet" as const,
        owner,
        challenge,
        signature: await account.signTypedData(
          authorizationTypedData(challenge),
        ),
      });
      // A vault created by a client that shipped the previous authority release.
      const oldClient = new OwnerClient(
        authority,
        signer,
        new LitConnection(lit),
        120000,
        { authorityRelease: previous },
      );
      await oldClient.login();
      await subscribe(oldClient);
      const oldCid = await oldClient.currentAuthorityCid();
      const oldTemplate = await templateStore.byHash(
        "authority",
        previous,
        api!,
      );
      assert.equal(oldCid, await actionCid(authority, oldTemplate.code));
      assert.notEqual(oldCid, await actionCid(authority));
      assert.deepEqual((await oldClient.api("/api/me")).authorities, [oldCid]);
      let s1 = await oldClient.create("LEGACY", "created-under-old-release");
      assert.equal(s1.manifest.document.manifest.authorityCid, oldCid);
      const agentKeys = Keychain.generateKey();
      const agentFor = (bundle: SecretBundle, usageApiKey?: string) =>
        new Keychain(agentKeys.privateKey, {
          v: 2,
          litApiUrl: lit,
          usageApiKey,
          secrets: {
            [bundle.envelope.document.metadata.name]: {
              manifest: bundle.manifest.document.manifest,
              actionCid: bundle.manifest.document.actionCid,
            },
          },
        });
      s1 = await oldClient.delegate(s1, agentKeys.publicKey, "agent");
      assert.equal(
        await agentFor(s1, oldClient.lit.usageApiKey).get("LEGACY"),
        "created-under-old-release",
      );

      // The owner upgrades their client: sign-in runs the newest release and the
      // vault records it; the old release stays granted.
      active = "new";
      const newClient = new OwnerClient(
        authority,
        signer,
        new LitConnection(lit),
      );
      await newClient.login();
      const newCid = await newClient.currentAuthorityCid();
      assert.equal(newCid, await actionCid(authority));
      const me = await newClient.api("/api/me");
      assert.deepEqual(me.authorities, [oldCid, newCid]);
      const audit = await newClient.api("/api/audit");
      assert.ok(
        audit.events.some(
          (e: any) =>
            e.event === "authority_upgraded" && e.objectHash === newCid,
        ),
      );
      // The old secret is still manageable: approvals run the release it pins,
      // fetched by hash from the registry and verified locally.
      const fresh = await newClient.bundle(
        s1.manifest.document.manifest.secretId,
      );
      assert.equal(fresh.manifest.document.manifest.authorityCid, oldCid);
      const disabled = await newClient.setPolicy(fresh, { disabled: true });
      assert.equal(disabled.policy.document.disabled, true);
      await assert.rejects(
        agentFor(disabled, newClient.lit.usageApiKey).get("LEGACY"),
        /denied/i,
      );
      const enabled = await newClient.setPolicy(disabled, { disabled: false });
      assert.equal(
        await agentFor(enabled, newClient.lit.usageApiKey).get("LEGACY"),
        "created-under-old-release",
      );
      const rotated = await newClient.rotate(enabled, "rotated-by-new-client");
      assert.equal(
        await agentFor(rotated, newClient.lit.usageApiKey).get("LEGACY"),
        "rotated-by-new-client",
      );
      // New secrets pin the newest release.
      const s2 = await newClient.create("MODERN", "created-under-new-release");
      assert.equal(s2.manifest.document.manifest.authorityCid, newCid);
      // Backups spanning both releases restore.
      const backup = await newClient.backup();
      assert.equal(backup.bundles.length, 2);
      await newClient.restore(backup);
      // A client still on the previous release can sign in; the vault is not
      // moved backwards.
      active = "old";
      await oldClient.login();
      assert.deepEqual((await oldClient.api("/api/me")).authorities, [
        oldCid,
        newCid,
      ]);
      // Templates are content-addressed and public.
      const index = await (await fetch(`${api}/api/templates`)).json();
      assert.deepEqual(index.templates.authority, archiveIndex.authority);
      const served = await (
        await fetch(`${api}/api/templates/${previous}`)
      ).text();
      assert.equal(served, oldTemplate.code);
      assert.equal(
        (await fetch(`${api}/api/templates/${"0".repeat(64)}`)).status,
        404,
      );
      void newest;
    } finally {
      globalThis.fetch = original;
    }
  },
);
