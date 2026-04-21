/**
 * AccountConfig full ABI bundle (view + admin writes + custom errors).
 *
 * Source of truth: lit-api-server/src/accounts/contracts/AccountConfig.json.
 * This is the hand-maintained subset used for sovereign-mode direct contract
 * calls. If the on-chain contract changes, (a) regenerate entries from the
 * updated JSON, (b) bump ACCOUNT_CONFIG_ABI_VERSION, (c) update the pinned
 * bytecode hash in ACCOUNT_CONFIG_DEPLOYMENTS for each affected chain.
 *
 * Drift detection:
 * - `runtimeBytecodeKeccak` is the keccak256 of the deployed runtime bytecode
 *   returned by eth_getCode for (chainId, address). Dashboard hard-blocks if
 *   the live hash does not match the pinned value.
 * - If a (chainId, address) pair has `runtimeBytecodeKeccak: null`, drift is
 *   skipped (dev-only override; never ship null values to production).
 */

import { ACCOUNT_CONFIG_VIEW_ABI } from './account_config_view_abi.js';

export const ACCOUNT_CONFIG_ABI_VERSION = '2026-04-21.1';

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
      { internalType: 'address', name: 'creatorWalletAddress', type: 'address' },
    ],
    name: 'newAccount',
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
 * Operators: regenerate hashes after any deployment with
 *   `cast keccak $(cast code <address> --rpc-url <rpc>)`.
 */
export const ACCOUNT_CONFIG_DEPLOYMENTS = {};

/**
 * Merge operator-provided deployments (e.g. from a build-time config or
 * `window.ACCOUNT_CONFIG_DEPLOYMENTS`) with the pinned map.
 */
export function mergeDeployments(overrides) {
  if (!overrides || typeof overrides !== 'object') return { ...ACCOUNT_CONFIG_DEPLOYMENTS };
  return { ...ACCOUNT_CONFIG_DEPLOYMENTS, ...overrides };
}

export default ACCOUNT_CONFIG_FULL_ABI;
