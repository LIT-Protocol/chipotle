import { test } from "node:test";
import assert from "node:assert/strict";
import { privateKeyToAccount } from "viem/accounts";
import {
  OwnerClient,
  LitConnection,
  Keychain,
  actionSource,
  authorizationTypedData,
  hex,
  randomBytes,
} from "../sdk/src/index.ts";
import discoverySource from "../generated/discovery.ts";
import { subscribe } from "./billing-fixture.ts";
import { fixtureSql } from "./storage-fixture.ts";
const api = process.env.KEYCHAIN_TEST_API;
const lit = process.env.KEYCHAIN_TEST_LIT || "http://127.0.0.1:55440";
const stripe = "http://127.0.0.1:55442";
function client() {
  const account = privateKeyToAccount(`0x${hex(randomBytes())}`);
  const owner = {
    kind: "wallet" as const,
    address: account.address.toLowerCase(),
  };
  const c = new OwnerClient(
    { v: 2, network: "test", registry: api!, owner },
    async (challenge) => ({
      kind: "wallet",
      owner,
      challenge,
      signature: await account.signTypedData(authorizationTypedData(challenge)),
    }),
    new LitConnection(lit),
  );
  let cookie = "";
  c.api = async (path, init = {}) => {
    const headers = new Headers(init.headers);
    if (cookie) headers.set("Cookie", cookie);
    const response = await fetch(api + path, { ...init, headers });
    const set = response.headers.get("set-cookie");
    if (set) cookie = set.split(";")[0];
    if (!response.ok) throw new Error(`Request failed (${response.status})`);
    return response.status === 204 ? null : response.json();
  };
  return c;
}
const post = (body: any) => ({
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify(body),
});
async function execute(key: string, code: string, params: any = {}) {
  return fetch(lit + "/core/v1/lit_action", {
    ...post({ code, js_params: params }),
    headers: {
      "Content-Type": "application/json",
      "X-Api-Key": key,
      "X-Privacy-Mode": "true",
    },
  });
}
test(
  "subscriptions, scoped direct execution, cancellation, immutable backups, and adversarial billing requests",
  { skip: !api },
  async () => {
    assert.ok(["localhost", "127.0.0.1"].includes(new URL(api!).hostname));
    const a = client();
    const b = client();
    await a.login();
    await b.login();
    const initial = await a.api("/api/billing");
    assert.equal(initial.subscription.active, false);
    assert.equal(initial.subscription.plan, "free");
    assert.equal(initial.subscription.secretLimit, 5);
    // Free includes five secrets with sponsored execution; the sixth needs a card.
    for (let i = 0; i < 5; i++) await a.create(`FREE_${i}`, `free-value-${i}`);
    const freeBundle = await a.api(
      `/api/secrets/${(await a.listSecrets()).find((s: any) => s.name === "FREE_0")!.secretId}/bundle`,
    );
    assert.equal(
      (
        await execute(
          a.lit.usageApiKey!,
          actionSource(freeBundle.manifest.document.manifest),
          { operation: "publicKey", challenge: hex(randomBytes()) },
        )
      ).status,
      200,
    );
    await assert.rejects(a.create("UNPAID", "must-stay-local"), /402/);
    assert.equal((await a.api("/api/billing")).secretCount, 5);
    assert.equal(
      (
        await execute(
          a.lit.usageApiKey!,
          "async function main(){return 'arbitrary code'}",
        )
      ).status,
      403,
    );
    assert.equal(
      (await execute(a.lit.usageApiKey!, actionSource(b.authority))).status,
      403,
    );
    assert.equal(
      (
        await fetch(lit + "/core/v1/update_group", {
          ...post({
            group_id: 1,
            cid_hashes_permitted: ["0x0"],
            pkp_ids_permitted: [],
          }),
          headers: {
            "Content-Type": "application/json",
            "X-Api-Key": a.lit.usageApiKey!,
            "X-Privacy-Mode": "true",
          },
        })
      ).status,
      403,
    );
    assert.equal(
      (await fetch(api + "/api/execution-key", { method: "POST" })).status,
      401,
    );
    assert.equal(
      (
        await fetch(api + "/api/billing/webhook", {
          ...post({ type: "invoice.paid" }),
          headers: {
            "Content-Type": "application/json",
            "Stripe-Signature": "t=1,v1=00",
          },
        })
      ).status,
      400,
    );
    const checkouts = await Promise.all(
      Array.from({ length: 4 }, () =>
        a.api(
          "/api/billing/checkout",
          post({
            price: "price_attacker",
            customer: "cus_attacker",
            quantity: 100,
          }),
        ),
      ),
    );
    assert.equal(new Set(checkouts.map((c) => c.url)).size, 1);
    assert.equal(
      (await a.api("/api/billing?checkout=success")).subscription.active,
      false,
    );
    const customer = await subscribe(a);
    const active = await a.api("/api/billing");
    assert.equal(active.subscription.active, true);
    assert.equal(active.subscription.plan, "standard");
    assert.equal(active.subscription.secretLimit, 1000);
    await assert.rejects(
      a.api("/api/billing/checkout", { method: "POST" }),
      /409/,
    );
    const portal = await a.api(
      "/api/billing/portal",
      post({ customer: "cus_attacker" }),
    );
    assert.equal(portal.url, stripe + "/portal/" + customer);
    let bundle = await a.create("BILLING_SECRET", "protected-test-credential");
    // Exercise the actual 1,000-secret boundary without 998 redundant crypto runs.
    fixtureSql(
      a.vaultId,
      `BEGIN;
      CREATE TEMP TABLE quota_fixture AS SELECT md5(:'vault'||i::text)||md5(i::text||:'vault') AS id, i FROM generate_series(1,993) i;
      INSERT INTO kc_secrets(id,vault_id,name,manifest,action_cid,current_version) SELECT id,:'vault','QUOTA_FIXTURE_'||i,'{"document":{"manifest":{"release":"export"}}}', 'fixture',1 FROM quota_fixture;
      INSERT INTO kc_policies(hash,vault_id,scope,epoch,signed) SELECT 'fixture-'||id,:'vault','secret:'||id,1,'{"document":{"disabled":false,"expiresAt":2000000000,"grants":[]}}' FROM quota_fixture;
      INSERT INTO kc_registry(scope,vault_id,policy_hash,epoch) SELECT 'secret:'||id,:'vault','fixture-'||id,1 FROM quota_fixture;
      COMMIT;`,
    );
    const lastSlots = await Promise.allSettled([
      a.create("LIMIT_RACE_A", "race"),
      a.create("LIMIT_RACE_B", "race"),
    ]);
    assert.equal(lastSlots.filter((r) => r.status === "fulfilled").length, 1);
    assert.equal((await a.api("/api/billing")).secretCount, 1000);
    assert.equal((await a.listSecrets()).length, 1000);
    bundle = await a.rotate(bundle, "protected-test-credential");
    assert.equal((await a.api("/api/billing")).secretCount, 1000);
    fixtureSql(
      a.vaultId,
      `BEGIN;
      DELETE FROM kc_registry WHERE vault_id=:'vault' AND policy_hash LIKE 'fixture-%';
      DELETE FROM kc_policies WHERE vault_id=:'vault' AND hash LIKE 'fixture-%';
      DELETE FROM kc_secrets WHERE vault_id=:'vault' AND name LIKE 'QUOTA_FIXTURE_%';
      COMMIT;`,
    );
    // The contract caps bulk group updates at 10 CIDs, but incremental grants
    // must support a full Standard vault. Real source/CID enrollment crosses it.
    for (let i = 0; i < 12; i++)
      await a.create(`GROUP_CAP_${i}`, `group-value-${i}`);
    const identity = Keychain.generateKey();
    bundle = await a.delegate(bundle, identity.publicKey, "Paid agent");
    const config = {
      v: 2 as const,
      litApiUrl: lit,
      usageApiKey: a.lit.usageApiKey,
      secrets: {
        BILLING_SECRET: {
          manifest: bundle.manifest.document.manifest,
          actionCid: bundle.manifest.document.actionCid,
        },
      },
    };
    const agent = new Keychain(identity.privateKey, config);
    assert.equal(
      await agent.get("BILLING_SECRET"),
      "protected-test-credential",
    );
    const otherPayer = new Keychain(identity.privateKey, {
      ...config,
      usageApiKey: b.lit.usageApiKey,
    });
    await assert.rejects(otherPayer.get("BILLING_SECRET"), /403/);
    // Another vault's manifest is rejected on Free and Standard alike.
    await assert.rejects(b.api("/api/actions", post(bundle.manifest)), /403/);
    // Unsigned preparation is bound to the session's vault the same way.
    await assert.rejects(
      b.api(
        "/api/actions/prepare",
        post({
          manifest: bundle.manifest.document.manifest,
          actionCid: bundle.manifest.document.actionCid,
        }),
      ),
      /403/,
    );
    await subscribe(b);
    await assert.rejects(b.api("/api/actions", post(bundle.manifest)), /403/);
    const oldKey = a.lit.usageApiKey!;
    await fetch(lit + "/test/fail-removal", { method: "POST" });
    const rotated = await a.api("/api/execution-key/rotate", {
      method: "POST",
    });
    a.lit.usageApiKey = rotated.usageApiKey;
    assert.notEqual(oldKey, rotated.usageApiKey);
    assert.equal(
      (
        await execute(oldKey, discoverySource, {
          cid: bundle.manifest.document.actionCid,
        })
      ).status,
      403,
    );
    assert.equal(
      (
        await execute(rotated.usageApiKey, discoverySource, {
          cid: bundle.manifest.document.actionCid,
        })
      ).status,
      200,
    );
    const update = async (body: any) => {
      const response = await fetch(
        stripe + "/test/update",
        post({ customer, ...body }),
      );
      assert.equal(response.status, 200);
      assert.equal((await response.json()).webhookStatus, 200);
    };
    await update({ cancel: true });
    assert.equal((await a.api("/api/billing")).subscription.active, true);
    const backup = await a.backup();
    assert.ok(!JSON.stringify(backup).includes("protected-test-credential"));
    await update({ expired: true });
    assert.equal((await a.api("/api/billing")).subscription.active, false);
    assert.equal((await a.api("/api/billing")).subscription.secretLimit, 5);
    // Over the Free limit after expiry: nothing is deleted, but writes need a subscription.
    await assert.rejects(a.create("AFTER_EXPIRY", "secret"), /402/);
    await assert.rejects(a.rotate(bundle, "new-secret"), /402|403/);
    assert.deepEqual(await a.backup(), backup);
    await a.api("/api/execution-key", { method: "POST" }); // immediate scope reconciliation, also done by the worker
    const currentAgent = new Keychain(identity.privateKey, {
      ...config,
      usageApiKey: rotated.usageApiKey,
    });
    // Lapsing is not revocation: enrolled secrets keep executing on Free.
    assert.equal(
      await currentAgent.get("BILLING_SECRET"),
      "protected-test-credential",
    );
    await a.setPolicy(bundle, { disabled: true }); // revocation remains available after cancellation
    await assert.rejects(currentAgent.get("BILLING_SECRET"), /denied/i);
    await update({ expired: false, status: "active", paid: false });
    assert.equal((await a.api("/api/billing")).subscription.active, false);
    await update({ paid: true, price: "price_wrong" });
    assert.equal((await a.api("/api/billing")).subscription.active, false);
    await update({ price: "price_test_standard", quantity: 2 });
    assert.equal((await a.api("/api/billing")).subscription.active, false);
    await update({ quantity: 1 });
    assert.equal((await a.api("/api/billing")).subscription.active, true);
    await a.api("/api/execution-key", { method: "POST" });
    // Renewal restores the existing populated group with one key update.
    const groupSecret = (await a.listSecrets()).find(
      (s: any) => s.name === "GROUP_CAP_11",
    )!;
    const groupBundle = await a.api(
      `/api/secrets/${groupSecret.secretId}/bundle`,
    );
    assert.equal(
      (
        await execute(
          rotated.usageApiKey,
          actionSource(groupBundle.manifest.document.manifest),
          { operation: "publicKey", challenge: hex(randomBytes()) },
        )
      ).status,
      200,
    );
    // An older event delivered later re-reads current Stripe state instead of restoring stale access.
    await update({ status: "canceled", eventId: "evt_test_old_delivery" });
    assert.equal((await a.api("/api/billing")).subscription.active, false);
    await update({ status: "canceled", eventId: "evt_test_old_delivery" });
    assert.equal((await a.api("/api/billing")).subscription.active, false);
    const saved = await a.api("/api/secrets");
    assert.equal(saved.secrets.length, 19);
  },
);
