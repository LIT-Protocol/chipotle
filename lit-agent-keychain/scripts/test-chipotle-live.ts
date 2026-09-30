// Opt-in live compatibility test. Creates and removes only temporary groups/keys.
// Never prints key values. Uses no real owner credentials or protected secrets.
import assert from "node:assert/strict";
import { keccak256, toHex } from "viem";
import {
  LitConnection,
  actionCid,
  hex,
  randomBytes,
} from "../sdk/src/index.ts";
import { cidForCode } from "../protocol/actions.ts";
import discovery from "../generated/discovery.ts";
import type { Manifest } from "../protocol/schema.ts";
const base = process.env.LIT_API_URL || "https://api.chipotle.litprotocol.com";
const master = process.env.CHIPOTLE_MASTER_API_KEY;
assert.ok(master, "CHIPOTLE_MASTER_API_KEY is required");
assert.ok(
  new URL(base).protocol === "https:",
  "Use a trusted HTTPS Chipotle origin",
);
const groups: string[] = [];
let usage: string | undefined;
async function request(path: string, body: unknown, key = master!) {
  const response = await fetch(`${base}/core/v1/${path}`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "X-Api-Key": key,
      "X-Privacy-Mode": "true",
    },
    body: JSON.stringify(body),
    redirect: "error",
    signal: AbortSignal.timeout(45000),
  });
  assert.ok(response.ok, `${path}: HTTP ${response.status}`);
  return response.json();
}
async function manage(path: string, body: unknown) {
  const result = await request(path, body);
  assert.equal(result.success, true, `${path}: unsuccessful`);
  return result;
}
const permission = (ids: number[]) => ({
  name: "Keychain temporary live check",
  description: "Automatically removed after validation",
  can_create_groups: false,
  can_delete_groups: false,
  can_create_pkps: false,
  manage_ipfs_ids_in_groups: [],
  add_pkp_to_groups: [],
  remove_pkp_from_groups: [],
  execute_in_groups: ids,
});
try {
  const helper = await cidForCode(discovery);
  for (const cids of [[helper], []]) {
    const result = await manage("add_group", {
      group_name: `Keychain live check ${Date.now()}`,
      group_description: "Temporary compatibility test",
      pkp_ids_permitted: [],
      cid_hashes_permitted: cids.map((cid) => keccak256(toHex(cid))),
    });
    groups.push(result.group_id);
  }
  usage = (await manage("add_usage_api_key", permission(groups.map(Number))))
    .usage_api_key;
  assert.equal(typeof usage, "string");
  const lit = new LitConnection(base, 45000, usage);
  const root = await lit.publicKey(helper);
  assert.ok(root.length >= 66);
  console.log(
    "PASS: scoped usage key runs public discovery on existing Chipotle endpoint",
  );
  const manifest: Manifest = {
    v: 2,
    network: "chipotle-v1",
    registry: "https://keychain.litprotocol.com",
    vaultId: hex(randomBytes()),
    secretId: hex(randomBytes()),
    authorityCid: helper,
    release: "export",
  };
  const cid = await actionCid(manifest);
  await manage("add_action_to_group", {
    group_id: Number(groups[1]),
    action_ipfs_cid: cid,
  });
  const pubkey = await lit.encryptionPublicKey(manifest);
  assert.match(pubkey, /^[0-9a-f]{64}$/);
  console.log(
    "PASS: full secret action runs, derived encryption-key binding verifies",
  );
  assert.equal(await lit.encryptionPublicKey(manifest), pubkey);
  console.log(
    "PASS: derived action encryption identity is stable across executions",
  );
  await assert.rejects(
    request(
      "lit_action",
      { code: "async function main(){return 'unauthorized'}", js_params: {} },
      usage,
    ),
    /HTTP 403/,
  );
  console.log("PASS: scoped key cannot execute arbitrary code");
  await manage("update_usage_api_key", {
    ...permission([Number(groups[0])]),
    usage_api_key: usage,
  });
  await assert.rejects(lit.encryptionPublicKey(manifest), /403/);
  assert.ok(await new LitConnection(base, 45000, usage).publicKey(cid));
  console.log(
    "PASS: removing secret-group permission stops secret action; discovery remains available",
  );
  await manage("update_usage_api_key", {
    ...permission(groups.map(Number)),
    usage_api_key: usage,
  });
  assert.equal(await lit.encryptionPublicKey(manifest), pubkey);
  console.log("PASS: restoring permission re-enables the same action identity");
} finally {
  let failed = false;
  if (usage) {
    try {
      await manage("remove_usage_api_key", { usage_api_key: usage });
      console.log("CLEANUP: temporary usage key removed");
    } catch {
      failed = true;
      console.error(
        "Cleanup could not confirm temporary key removal; inspect temporary groups on the test account",
      );
    }
  }
  for (const group of groups) {
    try {
      await manage("remove_group", { group_id: group });
      console.log("CLEANUP: temporary group removed");
    } catch {
      failed = true;
      console.error(
        `Cleanup could not confirm temporary group removal (${group})`,
      );
    }
  }
  if (failed) throw new Error("Live validation cleanup incomplete");
}
