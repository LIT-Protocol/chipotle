// Lit Chipotle REST helpers, shared by setup.js and bridge.js.
// Mirrors the patterns in examples/dark-pool/scripts/setup.js.

async function call(base, apiKey, p, init = {}) {
  const res = await fetch(`${base}/core/v1/${p}`, {
    ...init,
    headers: {
      "X-Api-Key": apiKey,
      "Content-Type": "application/json",
      ...(init.headers || {}),
    },
  });
  const body = await res.json();
  if (!res.ok) {
    const msg = body.message || body.error || JSON.stringify(body);
    const err = new Error(`${p} -> ${res.status}: ${msg}`);
    err.body = body;
    throw err;
  }
  return body;
}

// Compute an action's IPFS CID without publishing it.
async function getActionCid(base, apiKey, code) {
  return call(base, apiKey, "get_lit_action_ipfs_id", {
    method: "POST",
    body: JSON.stringify(code),
  });
}

// Execute an action by inline code. The server computes the code's CID and (for
// a scoped usage key) checks it against the key's group allowlist before
// running. Returns the action's `response` (throws on action error).
async function runAction(base, apiKey, code, jsParams) {
  const body = await call(base, apiKey, "lit_action", {
    method: "POST",
    body: JSON.stringify({ code, js_params: jsParams }),
  });
  if (body.has_error) {
    throw new Error(`action error: ${body.logs || JSON.stringify(body)}`);
  }
  return body.response;
}

// Create a managed PKP wallet. Returns its address (used as the pkpId).
async function createWallet(base, apiKey) {
  const body = await call(base, apiKey, "create_wallet", { method: "GET" });
  if (!body.wallet_address) {
    throw new Error(`create_wallet returned no address: ${JSON.stringify(body)}`);
  }
  return body.wallet_address;
}

async function addGroup(base, apiKey, name, description) {
  const body = await call(base, apiKey, "add_group", {
    method: "POST",
    body: JSON.stringify({
      group_name: name,
      group_description: description,
      pkp_ids_permitted: [],
      // No wildcard — specific CIDs are pinned via add_action_to_group, so the
      // scoped usage key can only run audited code against the PKP.
      cid_hashes_permitted: [],
    }),
  });
  return body.group_id;
}

async function addPkpToGroup(base, apiKey, groupId, pkpId) {
  return call(base, apiKey, "add_pkp_to_group", {
    method: "POST",
    body: JSON.stringify({ group_id: Number(groupId), pkp_id: pkpId }),
  });
}

async function addAction(base, apiKey, cid, name, description) {
  return call(base, apiKey, "add_action", {
    method: "POST",
    body: JSON.stringify({ action_ipfs_cid: cid, name, description }),
  });
}

async function addActionToGroup(base, apiKey, groupId, cid) {
  return call(base, apiKey, "add_action_to_group", {
    method: "POST",
    body: JSON.stringify({ group_id: Number(groupId), action_ipfs_cid: cid }),
  });
}

async function createUsageApiKey(base, apiKey, groupId, name, description) {
  const body = await call(base, apiKey, "add_usage_api_key", {
    method: "POST",
    body: JSON.stringify({
      name,
      description,
      can_create_groups: false,
      can_delete_groups: false,
      can_create_pkps: false,
      manage_ipfs_ids_in_groups: [],
      add_pkp_to_groups: [],
      remove_pkp_from_groups: [],
      execute_in_groups: [Number(groupId)],
    }),
  });
  if (!body.usage_api_key) {
    throw new Error(`add_usage_api_key returned no key: ${JSON.stringify(body)}`);
  }
  return body.usage_api_key;
}

// Inline setup-only helper actions (auditable here).
const ENCRYPT_SECRET_CODE = `
  async function main({ pkpId, message }) {
    const ciphertext = await Lit.Actions.Encrypt({ pkpId, message });
    return { ciphertext };
  }
`;

// Derives the address of the key the bridge action will SIGN with (Option B),
// so the oracle pinned in BridgeToken matches exactly what signs the mint.
const SIGNER_DERIVER_CODE = `
  async function main({ pkpId }) {
    const wallet = new ethers.Wallet(await Lit.Actions.getPrivateKey({ pkpId }));
    return { address: wallet.address };
  }
`;

module.exports = {
  call,
  getActionCid,
  runAction,
  createWallet,
  addGroup,
  addPkpToGroup,
  addAction,
  addActionToGroup,
  createUsageApiKey,
  ENCRYPT_SECRET_CODE,
  SIGNER_DERIVER_CODE,
};
