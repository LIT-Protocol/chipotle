/**
 * Security tests for usage API key permission enforcement.
 *
 * Tests all 7 permission dimensions (positive + negative), auth boundaries,
 * cross-account isolation, wildcard semantics, permission lifecycle,
 * key revocation, and zero-access baseline.
 *
 * Usage:
 *   k6 run k6/correctness/api-key-security.spec.ts
 *   BASE_URL=https://your-instance/core/v1 k6 run k6/correctness/api-key-security.spec.ts
 */
import { group } from "k6";
import { checkAndLog, assertOk, assertDenied, assertNon2xx } from "../helpers.ts";
import { LitApiServerClient } from "../litApiServer.ts";
import { PRECREATED_ACCOUNTS } from "../setup.ts";
import { HELLO_WORLD_CODE } from "../LitActionCode/index.ts";
import { BASE_URL, COMMON_PARAMS, K6_RUN_ID } from "../defaults.ts";

// ── Types ───────────────────────────────────────────────────────────────────

export interface SecuritySetupData {
  accountKeyA: string;
  walletAddressA: string;
  groupIdX: number;
  groupIdY: number;
  ipfsId: string;
  hashedCid: string;
  pkpWalletAddress: string;

  usageKey_executeX: string;
  usageKey_executeY: string;
  usageKey_createGroups: string;
  usageKey_deleteGroups: string;
  usageKey_createPkps: string;
  usageKey_manageIpfsX: string;
  usageKey_addPkpX: string;
  usageKey_removePkpX: string;
  usageKey_executeWildcard: string;
  usageKey_zeroAccess: string;
  usageKey_lifecycle: string;
  usageKey_revocation: string;

  accountKeyB: string;
  usageKeyB: string;
  groupIdB: number;
}

// ── Options ─────────────────────────────────────────────────────────────────

export const options = {
  vus: 1,
  iterations: 1,
  thresholds: {
    http_req_failed: ["rate<0.5"], // ~50% of requests intentionally fail
    http_req_duration: ["p(99)<30000"],
    checks: ["rate==1"],
  },
};

// ── Helpers ─────────────────────────────────────────────────────────────────

function makeClient(): LitApiServerClient {
  return new LitApiServerClient({ baseUrl: BASE_URL, commonRequestParameters: COMMON_PARAMS });
}

function authHeaders(key: string): { "X-Api-Key": string } {
  return { "X-Api-Key": key };
}

/** Create a usage API key with specific permissions. Returns the key string. */
function createUsageKey(
  client: LitApiServerClient,
  adminHeaders: { "X-Api-Key": string },
  name: string,
  perms: {
    can_create_groups?: boolean;
    can_delete_groups?: boolean;
    can_create_pkps?: boolean;
    manage_ipfs_ids_in_groups?: number[];
    add_pkp_to_groups?: number[];
    remove_pkp_from_groups?: number[];
    execute_in_groups?: number[];
  },
): string {
  const res = client.addUsageApiKey(
    {
      name,
      description: `k6-security-test-${K6_RUN_ID}`,
      can_create_groups: perms.can_create_groups ?? false,
      can_delete_groups: perms.can_delete_groups ?? false,
      can_create_pkps: perms.can_create_pkps ?? false,
      manage_ipfs_ids_in_groups: perms.manage_ipfs_ids_in_groups ?? [],
      add_pkp_to_groups: perms.add_pkp_to_groups ?? [],
      remove_pkp_from_groups: perms.remove_pkp_from_groups ?? [],
      execute_in_groups: perms.execute_in_groups ?? [],
    },
    adminHeaders,
  );
  if (!assertOk(`setup/createUsageKey(${name})`, "POST /add_usage_api_key", res)) {
    throw new Error(`setup failed: createUsageKey(${name})`);
  }
  return (res.data as { usage_api_key: string }).usage_api_key;
}

// ── Setup ───────────────────────────────────────────────────────────────────

export function setup(): SecuritySetupData {
  console.log(`k6 security test run: ${K6_RUN_ID}`);
  const client = makeClient();

  // ── Account A ───────────────────────────────────────────────────────────
  if (PRECREATED_ACCOUNTS.length === 0) {
    throw new Error(
      "No pre-created accounts found. Run accounts.seed.spec.ts first to generate k6/data/accounts.json",
    );
  }
  const accountA = PRECREATED_ACCOUNTS[Math.floor(Math.random() * PRECREATED_ACCOUNTS.length)];
  const accountKeyA = accountA.apiKey;
  const walletAddressA = accountA.walletAddress;
  const adminA = authHeaders(accountKeyA);

  // Create group X
  const addGroupXRes = client.addGroup(
    { group_name: `k6-sec-groupX-${K6_RUN_ID}`, group_description: "Security test group X", pkp_ids_permitted: [], cid_hashes_permitted: [] },
    adminA,
  );
  if (!assertOk("setup/addGroupX", "POST /add_group", addGroupXRes)) throw new Error("setup failed: addGroupX");

  // Create group Y
  const addGroupYRes = client.addGroup(
    { group_name: `k6-sec-groupY-${K6_RUN_ID}`, group_description: "Security test group Y", pkp_ids_permitted: [], cid_hashes_permitted: [] },
    adminA,
  );
  if (!assertOk("setup/addGroupY", "POST /add_group", addGroupYRes)) throw new Error("setup failed: addGroupY");

  // List groups to extract IDs
  const listGroupsRes = client.listGroups({ page_number: 0, page_size: 100 }, adminA);
  if (!assertOk("setup/listGroups", "GET /list_groups", listGroupsRes)) throw new Error("setup failed: listGroups");
  const groups = listGroupsRes.data as Array<{ id: string; name: string }>;
  const groupX = groups.find((g) => g.name === `k6-sec-groupX-${K6_RUN_ID}`);
  const groupY = groups.find((g) => g.name === `k6-sec-groupY-${K6_RUN_ID}`);
  if (!groupX || !groupY) throw new Error("setup failed: could not find created groups");
  const groupIdX = parseInt(groupX.id);
  const groupIdY = parseInt(groupY.id);

  // Get IPFS CID for hello-world action
  const ipfsRes = client.getLitActionIpfsId(HELLO_WORLD_CODE);
  if (!assertOk("setup/getLitActionIpfsId", "POST /get_lit_action_ipfs_id", ipfsRes)) throw new Error("setup failed: getLitActionIpfsId");
  const ipfsId = (ipfsRes.response.body as string).replace(/^"|"$/g, "").trim();

  // Register action
  const addActionRes = client.addAction(
    { action_ipfs_cid: ipfsId, name: `k6-sec-action-${K6_RUN_ID}`, description: "Security test action" },
    adminA,
  );
  if (!assertOk("setup/addAction", "POST /add_action", addActionRes)) throw new Error("setup failed: addAction");

  // Add action to group X
  const addActionToXRes = client.addActionToGroup({ group_id: groupIdX, action_ipfs_cid: ipfsId }, adminA);
  if (!assertOk("setup/addActionToGroupX", "POST /add_action_to_group", addActionToXRes)) throw new Error("setup failed: addActionToGroupX");

  // NOTE: action is intentionally NOT added to groupY — groupY stays empty so that
  // a key with execute_in_groups=[groupIdY] is denied (tests group-level restriction).

  // Get hashed CID from list actions — filter by name to avoid picking a stale action on reused accounts
  const actionName = `k6-sec-action-${K6_RUN_ID}`;
  const listActionsRes = client.listActions({ group_id: groupIdX, page_number: 0, page_size: 100 }, adminA);
  if (!assertOk("setup/listActions", "GET /list_actions", listActionsRes)) throw new Error("setup failed: listActions");
  const actions = listActionsRes.data as Array<{ id: string; name: string }>;
  const ourAction = actions.find((a) => a.name === actionName);
  if (!ourAction) throw new Error(`setup failed: could not find action '${actionName}' in listActions (found ${actions.length} actions)`);
  const hashedCid = ourAction.id;

  // Create wallet (PKP)
  const createWalletRes = client.createWallet(adminA);
  if (!assertOk("setup/createWallet", "GET /create_wallet", createWalletRes)) throw new Error("setup failed: createWallet");
  const pkpWalletAddress = (createWalletRes.data as { wallet_address: string }).wallet_address;

  // Add PKP to group X
  const addPkpRes = client.addPkpToGroup({ group_id: groupIdX, pkp_id: pkpWalletAddress }, adminA);
  if (!assertOk("setup/addPkpToGroupX", "POST /add_pkp_to_group", addPkpRes)) throw new Error("setup failed: addPkpToGroupX");

  // ── Create 11 usage keys ──────────────────────────────────────────────
  const usageKey_executeX = createUsageKey(client, adminA, "sec-executeX", { execute_in_groups: [groupIdX] });
  const usageKey_executeY = createUsageKey(client, adminA, "sec-executeY", { execute_in_groups: [groupIdY] });
  const usageKey_createGroups = createUsageKey(client, adminA, "sec-createGroups", { can_create_groups: true });
  const usageKey_deleteGroups = createUsageKey(client, adminA, "sec-deleteGroups", { can_delete_groups: true });
  const usageKey_createPkps = createUsageKey(client, adminA, "sec-createPkps", { can_create_pkps: true });
  const usageKey_manageIpfsX = createUsageKey(client, adminA, "sec-manageIpfsX", { manage_ipfs_ids_in_groups: [groupIdX] });
  const usageKey_addPkpX = createUsageKey(client, adminA, "sec-addPkpX", { add_pkp_to_groups: [groupIdX] });
  const usageKey_removePkpX = createUsageKey(client, adminA, "sec-removePkpX", { remove_pkp_from_groups: [groupIdX] });
  const usageKey_executeWildcard = createUsageKey(client, adminA, "sec-executeWildcard", { execute_in_groups: [0] });
  const usageKey_zeroAccess = createUsageKey(client, adminA, "sec-zeroAccess", {});
  const usageKey_lifecycle = createUsageKey(client, adminA, "sec-lifecycle", {});
  const usageKey_revocation = createUsageKey(client, adminA, "sec-revocation", { execute_in_groups: [0] });

  // ── Account B (cross-account isolation) ──────────────────────────────
  const newAccountBRes = client.newAccount({
    account_name: `k6-sec-accountB-${K6_RUN_ID}`,
    account_description: "Security test cross-account",
  });
  if (!assertOk("setup/newAccountB", "POST /new_account", newAccountBRes)) throw new Error("setup failed: newAccountB");
  const accountKeyB = (newAccountBRes.data as { api_key: string }).api_key;
  const adminB = authHeaders(accountKeyB);

  // Create group under Account B
  const addGroupBRes = client.addGroup(
    { group_name: `k6-sec-groupB-${K6_RUN_ID}`, group_description: "Account B group", pkp_ids_permitted: [], cid_hashes_permitted: [] },
    adminB,
  );
  if (!assertOk("setup/addGroupB", "POST /add_group", addGroupBRes)) throw new Error("setup failed: addGroupB");

  const listGroupsBRes = client.listGroups({ page_number: 0, page_size: 100 }, adminB);
  if (!assertOk("setup/listGroupsB", "GET /list_groups", listGroupsBRes)) throw new Error("setup failed: listGroupsB");
  const groupsB = listGroupsBRes.data as Array<{ id: string; name: string }>;
  const groupB = groupsB.find((g) => g.name === `k6-sec-groupB-${K6_RUN_ID}`);
  if (!groupB) throw new Error("setup failed: could not find Account B group");
  const groupIdB = parseInt(groupB.id);

  // Add action to Account B's group
  const addActionBRes = client.addAction(
    { action_ipfs_cid: ipfsId, name: `k6-sec-actionB-${K6_RUN_ID}`, description: "Account B action" },
    adminB,
  );
  if (!assertOk("setup/addActionB", "POST /add_action", addActionBRes)) throw new Error("setup failed: addActionB");

  const addActionToGroupBRes = client.addActionToGroup({ group_id: groupIdB, action_ipfs_cid: ipfsId }, adminB);
  if (!assertOk("setup/addActionToGroupB", "POST /add_action_to_group", addActionToGroupBRes)) throw new Error("setup failed: addActionToGroupB");

  // Create usage key for Account B with wildcard execute
  const usageKeyB = createUsageKey(client, adminB, "sec-accountB", { execute_in_groups: [0] });

  return {
    accountKeyA, walletAddressA, groupIdX, groupIdY, ipfsId, hashedCid, pkpWalletAddress,
    usageKey_executeX, usageKey_executeY, usageKey_createGroups, usageKey_deleteGroups, usageKey_createPkps,
    usageKey_manageIpfsX, usageKey_addPkpX, usageKey_removePkpX, usageKey_executeWildcard,
    usageKey_zeroAccess, usageKey_lifecycle, usageKey_revocation,
    accountKeyB, usageKeyB, groupIdB,
  };
}

// ── Tests ───────────────────────────────────────────────────────────────────

export default function (data: SecuritySetupData) {
  const client = makeClient();
  const adminA = authHeaders(data.accountKeyA);
  const adminB = authHeaders(data.accountKeyB);

  // ── Test 1: execute_in_groups ───────────────────────────────────────────
  group("1-execute-in-groups", () => {
    // Positive: key with groupX can execute action in groupX
    const posRes = client.litAction(
      { code: HELLO_WORLD_CODE, js_params: null },
      authHeaders(data.usageKey_executeX),
    );
    assertOk("1-execute-positive", "POST /lit_action", posRes);
    checkAndLog(posRes.response, {
      "execute positive: has_error is false": (r) => {
        try { return JSON.parse(r.body as string).has_error === false; } catch { return false; }
      },
    }, "1-execute-positive");

    // Negative: key scoped to groupY cannot execute action (action is in groupX and groupY,
    // but key only has execute permission for groupY — tests group-level restriction)
    const negRes = client.litAction(
      { code: HELLO_WORLD_CODE, js_params: null },
      authHeaders(data.usageKey_executeY),
    );
    assertDenied("1-execute-negative-scoped", "POST /lit_action", negRes, 403);
  });

  // ── Test 2: can_create_groups ──────────────────────────────────────────
  group("2-can-create-groups", () => {
    // Positive: key with can_create_groups=true
    const posRes = client.addGroup(
      { group_name: `k6-sec-t2-pos-${K6_RUN_ID}`, group_description: "Test 2 positive", pkp_ids_permitted: [], cid_hashes_permitted: [] },
      authHeaders(data.usageKey_createGroups),
    );
    assertOk("2-createGroup-positive", "POST /add_group", posRes);

    // Clean up: delete the created group with admin key
    const listRes = client.listGroups({ page_number: 0, page_size: 100 }, adminA);
    if (assertOk("2-listGroups-cleanup", "GET /list_groups", listRes)) {
      const gs = listRes.data as Array<{ id: string; name: string }>;
      const created = gs.find((g) => g.name === `k6-sec-t2-pos-${K6_RUN_ID}`);
      if (created) {
        client.removeGroup({ group_id: created.id }, adminA);
      }
    }

    // Negative: key with can_create_groups=false
    const negRes = client.addGroup(
      { group_name: `k6-sec-t2-neg-${K6_RUN_ID}`, group_description: "Test 2 negative", pkp_ids_permitted: [], cid_hashes_permitted: [] },
      authHeaders(data.usageKey_zeroAccess),
    );
    assertDenied("2-createGroup-negative", "POST /add_group", negRes, 403);
  });

  // ── Test 3: can_delete_groups ──────────────────────────────────────────
  group("3-can-delete-groups", () => {
    // Create a temporary group with admin key to delete
    const tempRes = client.addGroup(
      { group_name: `k6-sec-t3-temp-${K6_RUN_ID}`, group_description: "Test 3 temp", pkp_ids_permitted: [], cid_hashes_permitted: [] },
      adminA,
    );
    assertOk("3-createTempGroup", "POST /add_group", tempRes);
    const listRes = client.listGroups({ page_number: 0, page_size: 100 }, adminA);
    assertOk("3-listGroups", "GET /list_groups", listRes);
    const gs = listRes.data as Array<{ id: string; name: string }>;
    const temp = gs.find((g) => g.name === `k6-sec-t3-temp-${K6_RUN_ID}`);
    if (!temp) { console.error("3-deleteGroup: temp group not found"); return; }

    // Positive: key with can_delete_groups=true
    const posRes = client.removeGroup(
      { group_id: temp.id },
      authHeaders(data.usageKey_deleteGroups),
    );
    assertOk("3-deleteGroup-positive", "POST /remove_group", posRes);

    // Negative: key with can_delete_groups=false tries to delete groupX
    const negRes = client.removeGroup(
      { group_id: String(data.groupIdX) },
      authHeaders(data.usageKey_zeroAccess),
    );
    assertDenied("3-deleteGroup-negative", "POST /remove_group", negRes, 403);
  });

  // ── Test 4: can_create_pkps ────────────────────────────────────────────
  group("4-can-create-pkps", () => {
    // Positive: key with can_create_pkps=true
    const posRes = client.createWallet(authHeaders(data.usageKey_createPkps));
    assertOk("4-createPkps-positive", "GET /create_wallet", posRes);

    // Negative: key with can_create_pkps=false
    const negRes = client.createWallet(authHeaders(data.usageKey_zeroAccess));
    assertDenied("4-createPkps-negative", "GET /create_wallet", negRes, 403);
  });

  // ── Test 5: manage_ipfs_ids_in_groups ──────────────────────────────────
  group("5-manage-ipfs-in-groups", () => {
    // First remove action from groupX so we can re-add with usage key
    client.removeActionFromGroup({ group_id: data.groupIdX, hashed_cid: data.hashedCid }, adminA);

    // Positive: key with manage_ipfs_ids_in_groups=[groupIdX] can add action to groupX
    const posRes = client.addActionToGroup(
      { group_id: data.groupIdX, action_ipfs_cid: data.ipfsId },
      authHeaders(data.usageKey_manageIpfsX),
    );
    assertOk("5-manageIpfs-positive", "POST /add_action_to_group", posRes);

    // Negative: key with manage_ipfs_ids_in_groups=[groupIdX] cannot add action to groupY
    const negRes = client.addActionToGroup(
      { group_id: data.groupIdY, action_ipfs_cid: data.ipfsId },
      authHeaders(data.usageKey_manageIpfsX),
    );
    assertDenied("5-manageIpfs-negative", "POST /add_action_to_group", negRes, 403);
  });

  // ── Test 6: add_pkp_to_groups ──────────────────────────────────────────
  group("6-add-pkp-to-groups", () => {
    // Remove PKP from groupX first so we can re-add with usage key
    client.removePkpFromGroup({ group_id: data.groupIdX, pkp_id: data.pkpWalletAddress }, adminA);

    // Positive: key with add_pkp_to_groups=[groupIdX] can add PKP to groupX
    const posRes = client.addPkpToGroup(
      { group_id: data.groupIdX, pkp_id: data.pkpWalletAddress },
      authHeaders(data.usageKey_addPkpX),
    );
    assertOk("6-addPkp-positive", "POST /add_pkp_to_group", posRes);

    // Negative: key with add_pkp_to_groups=[groupIdX] cannot add PKP to groupY
    const negRes = client.addPkpToGroup(
      { group_id: data.groupIdY, pkp_id: data.pkpWalletAddress },
      authHeaders(data.usageKey_addPkpX),
    );
    assertDenied("6-addPkp-negative", "POST /add_pkp_to_group", negRes, 403);
  });

  // ── Test 7: remove_pkp_from_groups ─────────────────────────────────────
  group("7-remove-pkp-from-groups", () => {
    // Ensure PKP is in groupX (may already be from test 6)
    client.addPkpToGroup({ group_id: data.groupIdX, pkp_id: data.pkpWalletAddress }, adminA);

    // Positive: key with remove_pkp_from_groups=[groupIdX] can remove PKP from groupX
    const posRes = client.removePkpFromGroup(
      { group_id: data.groupIdX, pkp_id: data.pkpWalletAddress },
      authHeaders(data.usageKey_removePkpX),
    );
    assertOk("7-removePkp-positive", "POST /remove_pkp_from_group", posRes);

    // Negative: key with remove_pkp_from_groups=[groupIdX] cannot remove PKP from groupY
    // (PKP isn't in groupY, but the permission check should still deny)
    const negRes = client.removePkpFromGroup(
      { group_id: data.groupIdY, pkp_id: data.pkpWalletAddress },
      authHeaders(data.usageKey_removePkpX),
    );
    assertDenied("7-removePkp-negative", "POST /remove_pkp_from_group", negRes, 403);

    // Restore: re-add PKP to groupX for subsequent tests
    client.addPkpToGroup({ group_id: data.groupIdX, pkp_id: data.pkpWalletAddress }, adminA);
  });

  // ── Test 8: missing/invalid auth ───────────────────────────────────────
  group("8-missing-invalid-auth", () => {
    // No header at all — pass empty object as headers
    const noHeaderRes = client.litAction(
      { code: HELLO_WORLD_CODE, js_params: null },
      {} as any,
    );
    assertDenied("8-noHeader", "POST /lit_action", noHeaderRes, 401);

    // Garbage key
    const garbageRes = client.litAction(
      { code: HELLO_WORLD_CODE, js_params: null },
      authHeaders("not-a-real-key-garbage-abc123xyz"),
    );
    assertNon2xx("8-garbageKey", "POST /lit_action", garbageRes);

    // Test against management endpoint too — no header on addGroup
    const noHeaderGroupRes = client.addGroup(
      { group_name: "should-never-be-created", group_description: "", pkp_ids_permitted: [], cid_hashes_permitted: [] },
      {} as any,
    );
    assertDenied("8-noHeader-addGroup", "POST /add_group", noHeaderGroupRes, 401);
  });

  // ── Test 9: cross-account isolation ────────────────────────────────────
  group("9-cross-account-isolation", () => {
    // Positive: Account B's key works on B's resources
    const posRes = client.litAction(
      { code: HELLO_WORLD_CODE, js_params: null },
      authHeaders(data.usageKeyB),
    );
    assertOk("9-crossAccount-B-executes-own", "POST /lit_action", posRes);

    // Negative: Account B's key tries to add PKP to Account A's group
    const negPkpRes = client.addPkpToGroup(
      { group_id: data.groupIdX, pkp_id: data.pkpWalletAddress },
      adminB,
    );
    assertNon2xx("9-crossAccount-B-addPkp-A", "POST /add_pkp_to_group", negPkpRes);

    // Negative: Account B's key tries to delete Account A's group
    const negDeleteRes = client.removeGroup(
      { group_id: String(data.groupIdX) },
      adminB,
    );
    assertNon2xx("9-crossAccount-B-deleteGroup-A", "POST /remove_group", negDeleteRes);

    // Data isolation: Account B's listGroups should NOT contain Account A's groups
    const listBRes = client.listGroups({ page_number: 0, page_size: 100 }, adminB);
    if (assertOk("9-crossAccount-B-listGroups", "GET /list_groups", listBRes)) {
      const bGroups = listBRes.data as Array<{ id: string; name: string }>;
      checkAndLog(listBRes.response, {
        "B's listGroups does not contain A's groupX": () =>
          !bGroups.some((g) => parseInt(g.id) === data.groupIdX),
        "B's listGroups does not contain A's groupY": () =>
          !bGroups.some((g) => parseInt(g.id) === data.groupIdY),
      }, "9-crossAccount-isolation");
    }
  });

  // ── Test 10: wildcard vs specific group ────────────────────────────────
  group("10-wildcard-vs-specific", () => {
    // Wildcard key (execute_in_groups=[0]) can execute anywhere
    const wildcardRes = client.litAction(
      { code: HELLO_WORLD_CODE, js_params: null },
      authHeaders(data.usageKey_executeWildcard),
    );
    assertOk("10-wildcard-executes", "POST /lit_action", wildcardRes);

    // Specific key (execute_in_groups=[groupIdX]) can execute (action is in groupX)
    const specificRes = client.litAction(
      { code: HELLO_WORLD_CODE, js_params: null },
      authHeaders(data.usageKey_executeX),
    );
    assertOk("10-specific-executes-own-group", "POST /lit_action", specificRes);
  });

  // ── Test 11: permission update lifecycle ───────────────────────────────
  group("11-permission-update-lifecycle", () => {
    // Step 1: lifecycle key starts with zero perms — should be denied
    const deniedRes = client.litAction(
      { code: HELLO_WORLD_CODE, js_params: null },
      authHeaders(data.usageKey_lifecycle),
    );
    assertDenied("11-lifecycle-step1-denied", "POST /lit_action", deniedRes, 403);

    // Step 2: update lifecycle key to grant execute_in_groups=[0]
    const updateGrantRes = client.updateUsageApiKey(
      {
        usage_api_key: data.usageKey_lifecycle,
        name: "sec-lifecycle-granted",
        description: "Lifecycle test - granted",
        can_create_groups: false,
        can_delete_groups: false,
        can_create_pkps: false,
        manage_ipfs_ids_in_groups: [],
        add_pkp_to_groups: [],
        remove_pkp_from_groups: [],
        execute_in_groups: [0],
      },
      adminA,
    );
    assertOk("11-lifecycle-step2-grant", "POST /update_usage_api_key", updateGrantRes);

    // Step 3: now the lifecycle key should succeed
    const allowedRes = client.litAction(
      { code: HELLO_WORLD_CODE, js_params: null },
      authHeaders(data.usageKey_lifecycle),
    );
    assertOk("11-lifecycle-step3-allowed", "POST /lit_action", allowedRes);

    // Step 4: revoke by setting execute_in_groups back to []
    const updateRevokeRes = client.updateUsageApiKey(
      {
        usage_api_key: data.usageKey_lifecycle,
        name: "sec-lifecycle-revoked",
        description: "Lifecycle test - revoked",
        can_create_groups: false,
        can_delete_groups: false,
        can_create_pkps: false,
        manage_ipfs_ids_in_groups: [],
        add_pkp_to_groups: [],
        remove_pkp_from_groups: [],
        execute_in_groups: [],
      },
      adminA,
    );
    assertOk("11-lifecycle-step4-revoke", "POST /update_usage_api_key", updateRevokeRes);

    // Step 5: should be denied again
    const reDeniedRes = client.litAction(
      { code: HELLO_WORLD_CODE, js_params: null },
      authHeaders(data.usageKey_lifecycle),
    );
    assertDenied("11-lifecycle-step5-re-denied", "POST /lit_action", reDeniedRes, 403);
  });

  // ── Test 12: removed key revocation ────────────────────────────────────
  group("12-removed-key-revocation", () => {
    // Step 1: revocation key works (has execute_in_groups=[0])
    const worksRes = client.litAction(
      { code: HELLO_WORLD_CODE, js_params: null },
      authHeaders(data.usageKey_revocation),
    );
    assertOk("12-revocation-step1-works", "POST /lit_action", worksRes);

    // Step 2: delete the key
    const deleteRes = client.removeUsageApiKey(
      { usage_api_key: data.usageKey_revocation },
      adminA,
    );
    assertOk("12-revocation-step2-delete", "POST /remove_usage_api_key", deleteRes);

    // Step 3: deleted key should be rejected
    const rejectedRes = client.litAction(
      { code: HELLO_WORLD_CODE, js_params: null },
      authHeaders(data.usageKey_revocation),
    );
    assertNon2xx("12-revocation-step3-rejected", "POST /lit_action", rejectedRes);
  });

  // ── Test 13: empty permissions = zero access ───────────────────────────
  group("13-empty-permissions-zero-access", () => {
    const zeroHeaders = authHeaders(data.usageKey_zeroAccess);

    // litAction — should be denied (no execute_in_groups)
    const execRes = client.litAction(
      { code: HELLO_WORLD_CODE, js_params: null },
      zeroHeaders,
    );
    assertDenied("13-zero-litAction", "POST /lit_action", execRes, 403);

    // addGroup — should be denied (can_create_groups=false)
    const addGroupRes = client.addGroup(
      { group_name: "should-never-exist", group_description: "", pkp_ids_permitted: [], cid_hashes_permitted: [] },
      zeroHeaders,
    );
    assertDenied("13-zero-addGroup", "POST /add_group", addGroupRes, 403);

    // removeGroup — should be denied (can_delete_groups=false)
    const removeGroupRes = client.removeGroup(
      { group_id: String(data.groupIdX) },
      zeroHeaders,
    );
    assertDenied("13-zero-removeGroup", "POST /remove_group", removeGroupRes, 403);

    // createWallet — should be denied (can_create_pkps=false)
    const createWalletRes = client.createWallet(zeroHeaders);
    assertDenied("13-zero-createWallet", "GET /create_wallet", createWalletRes, 403);

    // addActionToGroup — should be denied (manage_ipfs_ids_in_groups=[])
    const addActionRes = client.addActionToGroup(
      { group_id: data.groupIdX, action_ipfs_cid: data.ipfsId },
      zeroHeaders,
    );
    assertDenied("13-zero-addActionToGroup", "POST /add_action_to_group", addActionRes, 403);

    // addPkpToGroup — should be denied (add_pkp_to_groups=[])
    const addPkpRes = client.addPkpToGroup(
      { group_id: data.groupIdX, pkp_id: data.pkpWalletAddress },
      zeroHeaders,
    );
    assertDenied("13-zero-addPkpToGroup", "POST /add_pkp_to_group", addPkpRes, 403);

    // removePkpFromGroup — should be denied (remove_pkp_from_groups=[])
    const removePkpRes = client.removePkpFromGroup(
      { group_id: data.groupIdX, pkp_id: data.pkpWalletAddress },
      zeroHeaders,
    );
    assertDenied("13-zero-removePkpFromGroup", "POST /remove_pkp_from_group", removePkpRes, 403);
  });

  // ── Cleanup ────────────────────────────────────────────────────────────
  group("cleanup", () => {
    // Remove action from groupX (action was never added to groupY)
    try { client.removeActionFromGroup({ group_id: data.groupIdX, hashed_cid: data.hashedCid }, adminA); } catch { /* ignore */ }

    // Remove PKP from group
    try { client.removePkpFromGroup({ group_id: data.groupIdX, pkp_id: data.pkpWalletAddress }, adminA); } catch { /* ignore */ }

    // Delete action
    try { client.deleteAction({ hashed_cid: data.hashedCid }, adminA); } catch { /* ignore */ }

    // Remove usage keys (some may already be deleted by tests)
    const keysToRemove = [
      data.usageKey_executeX, data.usageKey_executeY, data.usageKey_createGroups, data.usageKey_deleteGroups,
      data.usageKey_createPkps, data.usageKey_manageIpfsX, data.usageKey_addPkpX,
      data.usageKey_removePkpX, data.usageKey_executeWildcard, data.usageKey_zeroAccess,
      data.usageKey_lifecycle,
      // usageKey_revocation already deleted in test 12
    ];
    for (const key of keysToRemove) {
      try { client.removeUsageApiKey({ usage_api_key: key }, adminA); } catch { /* ignore */ }
    }

    // Delete groups (Account A)
    try { client.removeGroup({ group_id: String(data.groupIdX) }, adminA); } catch { /* ignore */ }
    try { client.removeGroup({ group_id: String(data.groupIdY) }, adminA); } catch { /* ignore */ }

    // Cleanup Account B resources
    try {
      const listBActions = client.listActions({ group_id: data.groupIdB, page_number: 0, page_size: 10 }, adminB);
      const bActions = listBActions.data as Array<{ id: string }>;
      if (bActions && bActions.length > 0) {
        for (const action of bActions) {
          try { client.removeActionFromGroup({ group_id: data.groupIdB, hashed_cid: action.id }, adminB); } catch { /* ignore */ }
          try { client.deleteAction({ hashed_cid: action.id }, adminB); } catch { /* ignore */ }
        }
      }
    } catch { /* ignore */ }
    try { client.removeUsageApiKey({ usage_api_key: data.usageKeyB }, adminB); } catch { /* ignore */ }
    try { client.removeGroup({ group_id: String(data.groupIdB) }, adminB); } catch { /* ignore */ }

    console.log("Cleanup complete");
  });
}
