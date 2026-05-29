// Shared Lit/Chipotle REST helpers used by the setup scripts: compute an
// action CID, create a permission group, mint a scoped usage key, derive an
// action's wallet address, register an action, add it to a group.
//
// (setup.js predates this module and inlines its own copies; the Across setup
// uses these. Same endpoints, same semantics.)

const ADDRESS_DERIVER_CODE = `
  async function main({ ipfsId }) {
    const walletAddress = await Lit.Actions.getLitActionWalletAddress({ ipfsId });
    return { walletAddress };
  }
`;

async function call(base, apiKey, path, init = {}) {
  const res = await fetch(`${base}/core/v1/${path}`, {
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
    const err = new Error(`${path} -> ${res.status}: ${msg}`);
    err.status = res.status;
    err.body = body;
    throw err;
  }
  return body;
}

async function getActionCid(base, apiKey, code) {
  return call(base, apiKey, "get_lit_action_ipfs_id", {
    method: "POST",
    body: JSON.stringify(code),
  });
}

async function addGroup(base, apiKey, name, description) {
  const body = await call(base, apiKey, "add_group", {
    method: "POST",
    body: JSON.stringify({
      group_name: name,
      group_description: description,
      pkp_ids_permitted: [],
      cid_hashes_permitted: ["0"], // wildcard; bounded by the scoped usage key
    }),
  });
  return body.group_id;
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

async function deriveActionWalletAddress(base, usageKey, cid) {
  const body = await call(base, usageKey, "lit_action", {
    method: "POST",
    body: JSON.stringify({ code: ADDRESS_DERIVER_CODE, js_params: { ipfsId: cid } }),
  });
  if (body.has_error) {
    throw new Error(`address derivation failed: ${body.logs || JSON.stringify(body)}`);
  }
  if (!body.response || !body.response.walletAddress) {
    throw new Error(`address derivation returned: ${JSON.stringify(body)}`);
  }
  return body.response.walletAddress;
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

module.exports = {
  getActionCid,
  addGroup,
  createUsageApiKey,
  deriveActionWalletAddress,
  addAction,
  addActionToGroup,
};
