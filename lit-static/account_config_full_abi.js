/**
 * AccountConfig full ABI bundle (view + admin writes + custom errors).
 *
 * Source of truth: lit-api-server/src/accounts/contracts/AccountConfig.json.
 * This is the hand-maintained subset used for sovereign-mode direct contract
 * calls.
 *
 * When to update what:
 * - Facet code changes (function added/removed/signature changed in any
 *   AccountConfigFacets/*.sol): regenerate the WRITE_FUNCTIONS / VIEW /
 *   CUSTOM_ERRORS entries from the updated JSON, and bump
 *   ACCOUNT_CONFIG_ABI_VERSION. The pinned bytecode hashes do NOT need to
 *   change — facet upgrades go through diamondCut and don't shift the proxy
 *   runtime bytecode that ACCOUNT_CONFIG_DEPLOYMENTS pins.
 * - Diamond proxy itself redeployed (new address, or re-deployed at the same
 *   address with new proxy code): update ACCOUNT_CONFIG_DEPLOYMENTS with the
 *   new (chainId, address) → runtimeBytecodeKeccak pin.
 *
 * Drift detection:
 * - `runtimeBytecodeKeccak` is the keccak256 of the deployed runtime bytecode
 *   returned by eth_getCode for (chainId, address). Dashboard hard-blocks if
 *   the live hash does not match the pinned value. This catches proxy-level
 *   swaps; it does NOT catch facet ABI drift — keep the JS ABI subset in sync
 *   with AccountConfig.json by hand whenever facets change.
 * - If a (chainId, address) pair has `runtimeBytecodeKeccak: null`, drift is
 *   skipped (dev-only override; never ship null values to production).
 */

import { ACCOUNT_CONFIG_VIEW_ABI } from './account_config_view_abi.js';

export const ACCOUNT_CONFIG_ABI_VERSION = '2026-04-28.1';

const WRITE_FUNCTIONS = [
  {
    inputs: [
      { internalType: 'uint256', name: 'apiKeyHash', type: 'uint256' },
      { internalType: 'string', name: 'name', type: 'string' },
      { internalType: 'string', name: 'description', type: 'string' },
      { internalType: 'uint256[]', name: 'cidHashes', type: 'uint256[]' },
      { internalType: 'address[]', name: 'pkpIds', type: 'address[]' },
    ],
    name: 'addGroup',
    outputs: [{ internalType: 'uint256', name: '', type: 'uint256' }],
    stateMutability: 'nonpayable',
    type: 'function',
  },
  {
    inputs: [
      { internalType: 'uint256', name: 'apiKeyHash', type: 'uint256' },
      { internalType: 'uint256', name: 'groupId', type: 'uint256' },
    ],
    name: 'removeGroup',
    outputs: [],
    stateMutability: 'nonpayable',
    type: 'function',
  },
  {
    inputs: [
      { internalType: 'uint256', name: 'accountApiKeyHash', type: 'uint256' },
      { internalType: 'uint256', name: 'groupId', type: 'uint256' },
      { internalType: 'string', name: 'name', type: 'string' },
      { internalType: 'string', name: 'description', type: 'string' },
      { internalType: 'uint256[]', name: 'cidHashes', type: 'uint256[]' },
      { internalType: 'address[]', name: 'pkpIds', type: 'address[]' },
    ],
    name: 'updateGroup',
    outputs: [],
    stateMutability: 'nonpayable',
    type: 'function',
  },
  {
    inputs: [
      { internalType: 'uint256', name: 'accountApiKeyHash', type: 'uint256' },
      { internalType: 'uint256', name: 'groupId', type: 'uint256' },
      { internalType: 'string', name: 'name', type: 'string' },
      { internalType: 'string', name: 'description', type: 'string' },
    ],
    name: 'updateGroupMetadata',
    outputs: [],
    stateMutability: 'nonpayable',
    type: 'function',
  },
  {
    inputs: [
      { internalType: 'uint256', name: 'accountApiKeyHash', type: 'uint256' },
      { internalType: 'string', name: 'name', type: 'string' },
      { internalType: 'string', name: 'description', type: 'string' },
      { internalType: 'uint256', name: 'actionHash', type: 'uint256' },
    ],
    name: 'addAction',
    outputs: [],
    stateMutability: 'nonpayable',
    type: 'function',
  },
  {
    inputs: [
      { internalType: 'uint256', name: 'accountApiKeyHash', type: 'uint256' },
      { internalType: 'uint256', name: 'actionHash', type: 'uint256' },
    ],
    name: 'removeAction',
    outputs: [],
    stateMutability: 'nonpayable',
    type: 'function',
  },
  {
    inputs: [
      { internalType: 'uint256', name: 'apiKeyHash', type: 'uint256' },
      { internalType: 'uint256', name: 'groupId', type: 'uint256' },
      { internalType: 'uint256', name: 'action', type: 'uint256' },
    ],
    name: 'addActionToGroup',
    outputs: [],
    stateMutability: 'nonpayable',
    type: 'function',
  },
  {
    inputs: [
      { internalType: 'uint256', name: 'apiKeyHash', type: 'uint256' },
      { internalType: 'uint256', name: 'groupId', type: 'uint256' },
      { internalType: 'uint256', name: 'action', type: 'uint256' },
    ],
    name: 'removeActionFromGroup',
    outputs: [],
    stateMutability: 'nonpayable',
    type: 'function',
  },
  {
    inputs: [
      { internalType: 'uint256', name: 'accountApiKeyHash', type: 'uint256' },
      { internalType: 'uint256', name: 'actionHash', type: 'uint256' },
      { internalType: 'uint256', name: 'groupId', type: 'uint256' },
      { internalType: 'string', name: 'name', type: 'string' },
      { internalType: 'string', name: 'description', type: 'string' },
    ],
    name: 'updateActionMetadata',
    outputs: [],
    stateMutability: 'nonpayable',
    type: 'function',
  },
  {
    inputs: [
      { internalType: 'uint256', name: 'apiKeyHash', type: 'uint256' },
      { internalType: 'uint256', name: 'groupId', type: 'uint256' },
      { internalType: 'address', name: 'pkpId', type: 'address' },
    ],
    name: 'addPkpToGroup',
    outputs: [],
    stateMutability: 'nonpayable',
    type: 'function',
  },
  {
    inputs: [
      { internalType: 'uint256', name: 'apiKeyHash', type: 'uint256' },
      { internalType: 'uint256', name: 'groupId', type: 'uint256' },
      { internalType: 'address', name: 'pkpId', type: 'address' },
    ],
    name: 'removePkpFromGroup',
    outputs: [],
    stateMutability: 'nonpayable',
    type: 'function',
  },
  {
    inputs: [
      { internalType: 'uint256', name: 'accountApiKeyHash', type: 'uint256' },
      { internalType: 'uint256', name: 'usageApiKeyHash', type: 'uint256' },
      { internalType: 'uint256', name: 'expiration', type: 'uint256' },
      { internalType: 'uint256', name: 'balance', type: 'uint256' },
      { internalType: 'string', name: 'name', type: 'string' },
      { internalType: 'string', name: 'description', type: 'string' },
      { internalType: 'bool', name: 'createGroups', type: 'bool' },
      { internalType: 'bool', name: 'deleteGroups', type: 'bool' },
      { internalType: 'bool', name: 'createPKPs', type: 'bool' },
      { internalType: 'uint256[]', name: 'manageIPFSIdsInGroups', type: 'uint256[]' },
      { internalType: 'uint256[]', name: 'addPkpToGroups', type: 'uint256[]' },
      { internalType: 'uint256[]', name: 'removePkpFromGroups', type: 'uint256[]' },
      { internalType: 'uint256[]', name: 'executeInGroups', type: 'uint256[]' },
    ],
    name: 'setUsageApiKey',
    outputs: [],
    stateMutability: 'nonpayable',
    type: 'function',
  },
  {
    inputs: [
      { internalType: 'uint256', name: 'accountApiKeyHash', type: 'uint256' },
      { internalType: 'uint256', name: 'usageApiKeyHash', type: 'uint256' },
    ],
    name: 'removeUsageApiKey',
    outputs: [],
    stateMutability: 'nonpayable',
    type: 'function',
  },
  {
    inputs: [
      { internalType: 'uint256', name: 'accountApiKeyHash', type: 'uint256' },
      { internalType: 'uint256', name: 'usageApiKeyHash', type: 'uint256' },
      { internalType: 'string', name: 'name', type: 'string' },
      { internalType: 'string', name: 'description', type: 'string' },
    ],
    name: 'updateUsageApiKeyMetadata',
    outputs: [],
    stateMutability: 'nonpayable',
    type: 'function',
  },
  {
    inputs: [
      { internalType: 'uint256', name: 'apiKeyHash', type: 'uint256' },
      { internalType: 'bool', name: 'managed', type: 'bool' },
      { internalType: 'string', name: 'accountName', type: 'string' },
      { internalType: 'string', name: 'accountDescription', type: 'string' },
      { internalType: 'address', name: 'adminWalletAddress', type: 'address' },
    ],
    name: 'newAccount',
    outputs: [],
    stateMutability: 'nonpayable',
    type: 'function',
  },
  {
    inputs: [
      { internalType: 'string', name: 'accountName', type: 'string' },
      { internalType: 'string', name: 'accountDescription', type: 'string' },
    ],
    name: 'newChainSecuredAccount',
    outputs: [],
    stateMutability: 'nonpayable',
    type: 'function',
  },
  {
    inputs: [
      { internalType: 'uint256', name: 'apiKeyHash', type: 'uint256' },
      { internalType: 'address', name: 'pkpId', type: 'address' },
      { internalType: 'uint256', name: 'derivationPath', type: 'uint256' },
      { internalType: 'string', name: 'name', type: 'string' },
      { internalType: 'string', name: 'description', type: 'string' },
    ],
    name: 'registerWalletDerivation',
    outputs: [],
    stateMutability: 'nonpayable',
    type: 'function',
  },
];

const CUSTOM_ERRORS = [
  { inputs: [{ internalType: 'uint256', name: 'apiKeyHash', type: 'uint256' }], name: 'AccountAlreadyExists', type: 'error' },
  { inputs: [{ internalType: 'uint256', name: 'apiKeyHash', type: 'uint256' }], name: 'AccountDoesNotExist', type: 'error' },
  {
    inputs: [
      { internalType: 'uint256', name: 'apiKeyHash', type: 'uint256' },
      { internalType: 'uint256', name: 'groupId', type: 'uint256' },
      { internalType: 'uint256', name: 'cidHash', type: 'uint256' },
    ],
    name: 'ActionDoesNotExist',
    type: 'error',
  },
  {
    inputs: [
      { internalType: 'uint256', name: 'apiKeyHash', type: 'uint256' },
      { internalType: 'uint256', name: 'groupId', type: 'uint256' },
    ],
    name: 'GroupDoesNotExist',
    type: 'error',
  },
  { inputs: [{ internalType: 'string', name: 'message', type: 'string' }], name: 'InvalidRequest', type: 'error' },
  {
    inputs: [
      { internalType: 'uint256', name: 'apiKeyHash', type: 'uint256' },
      { internalType: 'address', name: 'sender', type: 'address' },
    ],
    name: 'NoAccountAccess',
    type: 'error',
  },
  {
    inputs: [
      { internalType: 'uint256', name: 'apiKeyHash', type: 'uint256' },
      { internalType: 'uint256', name: 'groupId', type: 'uint256' },
    ],
    name: 'NotAllowedToAddPkpToGroup',
    type: 'error',
  },
  { inputs: [{ internalType: 'uint256', name: 'apiKeyHash', type: 'uint256' }], name: 'NotAllowedToCreateGroup', type: 'error' },
  { inputs: [{ internalType: 'uint256', name: 'apiKeyHash', type: 'uint256' }], name: 'NotAllowedToCreatePkp', type: 'error' },
  { inputs: [{ internalType: 'uint256', name: 'apiKeyHash', type: 'uint256' }], name: 'NotAllowedToDeleteGroup', type: 'error' },
  {
    inputs: [
      { internalType: 'uint256', name: 'apiKeyHash', type: 'uint256' },
      { internalType: 'uint256', name: 'groupId', type: 'uint256' },
    ],
    name: 'NotAllowedToManageIPFSIdsInGroup',
    type: 'error',
  },
  {
    inputs: [
      { internalType: 'uint256', name: 'apiKeyHash', type: 'uint256' },
      { internalType: 'uint256', name: 'groupId', type: 'uint256' },
    ],
    name: 'NotAllowedToRemovePkpFromGroup',
    type: 'error',
  },
  { inputs: [{ internalType: 'uint256', name: 'apiKeyHash', type: 'uint256' }], name: 'NotMasterAccount', type: 'error' },
  { inputs: [{ internalType: 'address', name: 'caller', type: 'address' }], name: 'OnlyApiPayerOrOwner', type: 'error' },
  {
    inputs: [
      { internalType: 'uint256', name: 'apiKeyHash', type: 'uint256' },
      { internalType: 'uint256', name: 'groupId', type: 'uint256' },
      { internalType: 'address', name: 'pkpId', type: 'address' },
    ],
    name: 'PkpDoesNotExist',
    type: 'error',
  },
  {
    inputs: [
      { internalType: 'uint256', name: 'apiKeyHash', type: 'uint256' },
      { internalType: 'uint256', name: 'usageApiKeyHash', type: 'uint256' },
    ],
    name: 'UsageApiKeyDoesNotExist',
    type: 'error',
  },
  {
    inputs: [
      { internalType: 'uint256', name: 'apiKeyHash', type: 'uint256' },
      { internalType: 'uint256', name: 'amount', type: 'uint256' },
    ],
    name: 'InsufficientBalance',
    type: 'error',
  },
];

export const ACCOUNT_CONFIG_WRITE_ABI = WRITE_FUNCTIONS;
export const ACCOUNT_CONFIG_ERROR_ABI = CUSTOM_ERRORS;

export const ACCOUNT_CONFIG_FULL_ABI = [
  ...ACCOUNT_CONFIG_VIEW_ABI,
  ...WRITE_FUNCTIONS,
  ...CUSTOM_ERRORS,
];

/**
 * Pinned per-deployment bytecode hashes. Hard-block on mismatch.
 *
 * Key format: `${chainId}:${contractAddress.toLowerCase()}`.
 * Value.runtimeBytecodeKeccak: lowercase 0x-prefixed keccak256 of eth_getCode.
 * Value.runtimeBytecodeKeccak === null: drift check DISABLED for this target
 *   (dev/local override; do not ship null values to production dashboards).
 *
 * Operators: regenerate hashes after a proxy redeploy with
 *   `cast keccak $(cast code <address> --rpc-url <rpc>)`.
 */
export const ACCOUNT_CONFIG_DEPLOYMENTS = Object.freeze({
  // Base mainnet — `next` (staging) deployment, NodeConfig.next.toml
  '8453:0x98e501fab2d60a5119a185e1563f10cb54bc6068': {
    runtimeBytecodeKeccak: '0x3e21f437bf1c1f75d60cdcf90aafef49ef2869aa18e58e37e282152c5c702254',
  },
  // Base mainnet — `main` deployment, NodeConfig.main.toml
  '8453:0x4c8eb9f329ebfdb369f0c90954875ef8f568ad24': {
    runtimeBytecodeKeccak: '0x3e21f437bf1c1f75d60cdcf90aafef49ef2869aa18e58e37e282152c5c702254',
  },
  // Base mainnet — `prod` deployment, NodeConfig.prod.toml
  '8453:0xaaaaa9120fe271f653cfdb6bf400db93d2dea7aa': {
    runtimeBytecodeKeccak: '0xa7c20a6750b5847265c831def8c009d92524ea0a83921261d065da58b366f074',
  },
});

/**
 * Dev-only escape hatch for running sovereign mode against a deployment that
 * has no pinned bytecode hash yet (local anvil, a fresh testnet deploy, etc.).
 * Browser-only: checks `window` / `globalThis` for a truthy flag.
 *
 * Auto-enables on `localhost`, `127.0.0.1`, and `[::1]` so a contributor doing
 * `python -m http.server` against a local anvil isn't blocked by drift checks.
 * On any other hostname (including LAN IPs and *.local), the override is
 * strictly opt-in via `window.LIT_ACCOUNT_CONFIG_ALLOW_UNPINNED_DEPLOYMENTS`.
 *
 * Shipping dashboards MUST either populate ACCOUNT_CONFIG_DEPLOYMENTS with the
 * target (chainId, address) entries or pass them via `deployments` option;
 * otherwise sovereign writes hard-fail. See _verifyAbiIntegrity.
 */
export function isAbiDriftDevOverrideEnabled() {
  const v = (typeof globalThis !== 'undefined' && globalThis.LIT_ACCOUNT_CONFIG_ALLOW_UNPINNED_DEPLOYMENTS)
    ?? (typeof window !== 'undefined' && window.LIT_ACCOUNT_CONFIG_ALLOW_UNPINNED_DEPLOYMENTS);
  if (typeof v === 'boolean') return v;
  if (typeof v === 'string') {
    const s = v.trim().toLowerCase();
    return s === '1' || s === 'true' || s === 'yes' || s === 'on';
  }
  if (typeof location !== 'undefined' && location.hostname) {
    const h = location.hostname;
    if (h === 'localhost' || h === '127.0.0.1' || h === '[::1]') return true;
  }
  return false;
}

/**
 * Merge operator-provided deployments (e.g. from a build-time config or
 * `window.ACCOUNT_CONFIG_DEPLOYMENTS`) with the pinned map.
 */
export function mergeDeployments(overrides) {
  if (!overrides || typeof overrides !== 'object') return { ...ACCOUNT_CONFIG_DEPLOYMENTS };
  return { ...ACCOUNT_CONFIG_DEPLOYMENTS, ...overrides };
}

export default ACCOUNT_CONFIG_FULL_ABI;
