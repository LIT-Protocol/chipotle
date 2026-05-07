/**
 * AccountConfig view-only ABI subset.
 *
 * Source of truth: lit-api-server/src/accounts/contracts/AccountConfig.json.
 * Includes only view functions the dashboard/SDK needs to read in sovereign mode.
 * State-mutating functions live in the full ABI bundle (added in Phase 1).
 */

export const ACCOUNT_CONFIG_VIEW_ABI = [
  {
    inputs: [{ internalType: 'uint256', name: 'apiKeyHash', type: 'uint256' }],
    name: 'accountExistsAndIsMutable',
    outputs: [{ internalType: 'bool', name: '', type: 'bool' }],
    stateMutability: 'view',
    type: 'function',
  },
  {
    inputs: [
      { internalType: 'uint256', name: 'accountApiKeyHash', type: 'uint256' },
      { internalType: 'uint256', name: 'pageNumber', type: 'uint256' },
      { internalType: 'uint256', name: 'pageSize', type: 'uint256' },
    ],
    name: 'listGroups',
    outputs: [
      {
        components: [
          { internalType: 'uint256', name: 'id', type: 'uint256' },
          { internalType: 'string', name: 'name', type: 'string' },
          { internalType: 'string', name: 'description', type: 'string' },
        ],
        internalType: 'struct AppStorage.Metadata[]',
        name: '',
        type: 'tuple[]',
      },
    ],
    stateMutability: 'view',
    type: 'function',
  },
  {
    inputs: [
      { internalType: 'uint256', name: 'accountApiKeyHash', type: 'uint256' },
      { internalType: 'uint256', name: 'pageNumber', type: 'uint256' },
      { internalType: 'uint256', name: 'pageSize', type: 'uint256' },
    ],
    name: 'listPkps',
    outputs: [
      {
        components: [
          { internalType: 'uint256', name: 'id', type: 'uint256' },
          { internalType: 'address', name: 'pkpId', type: 'address' },
          { internalType: 'string', name: 'name', type: 'string' },
          { internalType: 'string', name: 'description', type: 'string' },
        ],
        internalType: 'struct AppStorage.PkpData[]',
        name: '',
        type: 'tuple[]',
      },
    ],
    stateMutability: 'view',
    type: 'function',
  },
  {
    inputs: [
      { internalType: 'uint256', name: 'accountApiKeyHash', type: 'uint256' },
      { internalType: 'uint256', name: 'groupId', type: 'uint256' },
      { internalType: 'uint256', name: 'pageNumber', type: 'uint256' },
      { internalType: 'uint256', name: 'pageSize', type: 'uint256' },
    ],
    name: 'listWalletsInGroup',
    outputs: [
      {
        components: [
          { internalType: 'uint256', name: 'id', type: 'uint256' },
          { internalType: 'address', name: 'pkpId', type: 'address' },
          { internalType: 'string', name: 'name', type: 'string' },
          { internalType: 'string', name: 'description', type: 'string' },
        ],
        internalType: 'struct AppStorage.PkpData[]',
        name: '',
        type: 'tuple[]',
      },
    ],
    stateMutability: 'view',
    type: 'function',
  },
  {
    inputs: [
      { internalType: 'uint256', name: 'accountApiKeyHash', type: 'uint256' },
      { internalType: 'uint256', name: 'pageNumber', type: 'uint256' },
      { internalType: 'uint256', name: 'pageSize', type: 'uint256' },
    ],
    name: 'listActions',
    outputs: [
      {
        components: [
          { internalType: 'uint256', name: 'id', type: 'uint256' },
          { internalType: 'string', name: 'name', type: 'string' },
          { internalType: 'string', name: 'description', type: 'string' },
        ],
        internalType: 'struct AppStorage.Metadata[]',
        name: '',
        type: 'tuple[]',
      },
    ],
    stateMutability: 'view',
    type: 'function',
  },
  {
    inputs: [
      { internalType: 'uint256', name: 'accountApiKeyHash', type: 'uint256' },
      { internalType: 'uint256', name: 'groupId', type: 'uint256' },
      { internalType: 'uint256', name: 'pageNumber', type: 'uint256' },
      { internalType: 'uint256', name: 'pageSize', type: 'uint256' },
    ],
    name: 'listActionsInGroup',
    outputs: [
      {
        components: [
          { internalType: 'uint256', name: 'id', type: 'uint256' },
          { internalType: 'string', name: 'name', type: 'string' },
          { internalType: 'string', name: 'description', type: 'string' },
        ],
        internalType: 'struct AppStorage.Metadata[]',
        name: '',
        type: 'tuple[]',
      },
    ],
    stateMutability: 'view',
    type: 'function',
  },
  {
    inputs: [
      { internalType: 'uint256', name: 'accountApiKeyHash', type: 'uint256' },
      { internalType: 'uint256', name: 'pageNumber', type: 'uint256' },
      { internalType: 'uint256', name: 'pageSize', type: 'uint256' },
    ],
    name: 'listApiKeys',
    outputs: [
      {
        components: [
          {
            components: [
              { internalType: 'uint256', name: 'id', type: 'uint256' },
              { internalType: 'string', name: 'name', type: 'string' },
              { internalType: 'string', name: 'description', type: 'string' },
            ],
            internalType: 'struct AppStorage.Metadata',
            name: 'metadata',
            type: 'tuple',
          },
          { internalType: 'uint256', name: 'apiKeyHash', type: 'uint256' },
          { internalType: 'uint256', name: 'expiration', type: 'uint256' },
          { internalType: 'uint256', name: 'balance', type: 'uint256' },
          { internalType: 'uint256[]', name: 'executeInGroups', type: 'uint256[]' },
          { internalType: 'bool', name: 'createGroups', type: 'bool' },
          { internalType: 'bool', name: 'deleteGroups', type: 'bool' },
          { internalType: 'bool', name: 'createPKPs', type: 'bool' },
          { internalType: 'uint256[]', name: 'manageIPFSIdsInGroups', type: 'uint256[]' },
          { internalType: 'uint256[]', name: 'addPkpToGroups', type: 'uint256[]' },
          { internalType: 'uint256[]', name: 'removePkpFromGroups', type: 'uint256[]' },
        ],
        internalType: 'struct ViewsFacet.UsageApiKeyReturn[]',
        name: '',
        type: 'tuple[]',
      },
    ],
    stateMutability: 'view',
    type: 'function',
  },
  {
    inputs: [],
    name: 'api_payers',
    outputs: [{ internalType: 'address[]', name: '', type: 'address[]' }],
    stateMutability: 'view',
    type: 'function',
  },
];

export default ACCOUNT_CONFIG_VIEW_ABI;
