import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createClient } from './core_sdk.js';
import { normalizeUsageKeyItem, usageKeyRowFromCreated, usageKeyRowFromListItem, usageKeyUpdateArgs } from './dapps/dashboard/keys.js';

// Sovereign writes load ethers from globalThis when it is set. This stand-in
// keccak256 returns the string it hashed, so the tests see exactly what the SDK
// hashed and what it sent to the contract.
globalThis.ethers = {
  toUtf8Bytes: (s) => new TextEncoder().encode(s),
  keccak256: (bytes) => `keccak256(${new TextDecoder().decode(bytes)})`,
};

const ADMIN_HASH = '0x' + '11'.repeat(32);
// listApiKeys returns only this hash, so it is all the dashboard has for a key
// it loaded from chain (for example, one a previous admin issued).
const LISTED_HASH = '0x' + 'fd'.repeat(32);

/** Sovereign client whose contract writes are recorded instead of sent. */
function sovereignClient() {
  const client = createClient({
    mode: 'sovereign',
    contractAddress: '0x' + '22'.repeat(20),
    rpcUrl: 'http://127.0.0.1:1',
    adminHashOverride: ADMIN_HASH,
  });
  const sent = [];
  const write = (method) => {
    const send = async (...args) => {
      sent.push({ method, args: args.slice(0, -1) }); // drop tx overrides
      return { hash: '0x' + 'ab'.repeat(32), wait: async () => ({ status: 1 }) };
    };
    send.staticCall = async () => undefined;
    return send;
  };
  client._getWriteContract = async () => ({
    removeUsageApiKey: write('removeUsageApiKey'),
    setUsageApiKey: write('setUsageApiKey'),
    updateUsageApiKeyMetadata: write('updateUsageApiKeyMetadata'),
  });
  return { client, sent };
}

async function usageKeyWrites(usageApiKey) {
  const { client, sent } = sovereignClient();
  await client.removeUsageApiKey({ usageApiKey });
  await client.updateUsageApiKey({ usageApiKey, name: 'n', description: 'd' });
  await client.updateUsageApiKeyMetadata({ usageApiKey, name: 'n', description: 'd' });
  return sent.map(({ method, args }) => [method, args[0], args[1]]);
}

test('sovereign usage-key writes send a listed api_key_hash as is', async () => {
  for (const hash of [LISTED_HASH, '0x' + 'FD'.repeat(32)]) {
    assert.deepEqual(await usageKeyWrites(hash), [
      ['removeUsageApiKey', ADMIN_HASH, hash],
      ['setUsageApiKey', ADMIN_HASH, hash],
      ['updateUsageApiKeyMetadata', ADMIN_HASH, hash],
    ]);
  }
});

test('sovereign usage-key writes hash anything that is not a 32-byte 0x hash', async () => {
  const keys = [
    'lk_' + 'ab'.repeat(32), // minted by addUsageApiKey in sovereign mode
    Buffer.alloc(32, 0xfd).toString('base64'), // issued by the server
    '0X' + 'fd'.repeat(32), // the server also hashes an uppercase prefix
    '0x' + 'fd'.repeat(31),
    '0x' + 'fd'.repeat(32) + 'f',
    '0x' + 'zz'.repeat(32),
  ];
  for (const key of keys) {
    const hashed = `keccak256(${key})`;
    assert.deepEqual(await usageKeyWrites(key), [
      ['removeUsageApiKey', ADMIN_HASH, hashed],
      ['setUsageApiKey', ADMIN_HASH, hashed],
      ['updateUsageApiKeyMetadata', ADMIN_HASH, hashed],
    ]);
  }
});

test('sovereign updateUsageApiKey writes the expiration and balance it is given', async () => {
  // The dashboard edit passes the loaded values because setUsageApiKey
  // overwrites both. listApiKeys returns expiration as a decimal string and
  // balance as a decimal string. Explicit numeric inputs also remain supported.
  const { client, sent } = sovereignClient();
  await client.updateUsageApiKey({
    usageApiKey: LISTED_HASH,
    name: 'n',
    description: 'd',
    expiration: '1893456000',
    balance: 7,
  });
  assert.deepEqual(sent[0].args.slice(0, 4), [ADMIN_HASH, LISTED_HASH, 1893456000n, 7n]);
});

test('a listed usage key keeps its expiration, exact balance and permissions through a dashboard edit', async () => {
  // Every hop from the listing to the write: the SDK's listApiKeys item, the
  // row loadUsageKeys stores, the normalized row the Edit button opens the
  // modal with, and the options the modal's Save passes to the SDK.
  for (const balance of [7n, 9007199254740993n, (1n << 256n) - 1n]) {
    const { client, sent } = sovereignClient();
    client._getViewContract = async () => ({
      listApiKeys: async () => [{
        metadata: { id: BigInt(LISTED_HASH), name: 'original', description: 'description' },
        apiKeyHash: BigInt(LISTED_HASH),
        expiration: 1893456000n,
        balance,
        createGroups: true,
        deleteGroups: false,
        createPKPs: true,
        manageIPFSIdsInGroups: [],
        addPkpToGroups: [],
        removePkpFromGroups: [],
        executeInGroups: [3n],
      }],
    });
    const [listed] = await client.listApiKeys({});
    assert.equal(BigInt(listed.balance), balance, 'listing must not round the balance');
    const item = normalizeUsageKeyItem(usageKeyRowFromListItem(listed));
    await client.updateUsageApiKey(usageKeyUpdateArgs(item, {
      name: 'edited',
      description: 'description',
      manageIpfsIdsInGroups: [],
      addPkpToGroups: [],
      removePkpFromGroups: [],
      executeInGroups: [3, 4],
    }));
    assert.deepEqual(sent, [{
      method: 'setUsageApiKey',
      args: [
        ADMIN_HASH, LISTED_HASH, 1893456000n, balance, 'edited', 'description',
        true, false, true, [], [], [], [3n, 4n],
      ],
    }]);
  }
});

test('a usage key added in this session keeps its permissions through an edit before Load', async () => {
  // The addUsageApiKey options the dashboard's Add sends: a key scoped to two
  // groups, and an All Options key (every flag, "All Groups" = 0 everywhere).
  for (const addOptions of [
    {
      name: 'added', description: '', canCreateGroups: false, canDeleteGroups: false, canCreatePkps: false,
      manageIpfsIdsInGroups: [], addPkpToGroups: [2], removePkpFromGroups: [], executeInGroups: [1, 2],
    },
    {
      name: 'added', description: 'all', canCreateGroups: true, canDeleteGroups: true, canCreatePkps: true,
      manageIpfsIdsInGroups: [0], addPkpToGroups: [0], removePkpFromGroups: [0], executeInGroups: [0],
    },
  ]) {
    const { client, sent } = sovereignClient();
    const { usage_api_key: usageKey } = await client.addUsageApiKey(addOptions);
    const item = normalizeUsageKeyItem(usageKeyRowFromCreated(usageKey, addOptions));
    // The Edit dialog ticks the row's groups (checkbox values are decimal
    // strings) and Save reads the ticked ones back as numbers.
    const ticked = (ids) => ids.map(String).map(Number);
    await client.updateUsageApiKey(usageKeyUpdateArgs(item, {
      name: 'renamed',
      description: item.description,
      manageIpfsIdsInGroups: ticked(item.can_manage_ipfs_ids_in_groups),
      addPkpToGroups: ticked(item.can_add_pkp_to_groups),
      removePkpFromGroups: ticked(item.can_remove_pkp_from_groups),
      executeInGroups: ticked(item.can_execute_in_groups),
    }));
    assert.deepEqual(sent.map(({ method }) => method), ['setUsageApiKey', 'setUsageApiKey']);
    const [added, edited] = sent.map(({ args }) => args);
    // The edit writes back everything the add wrote (same key hash, expiration,
    // balance, flags and groups); only the name changes.
    assert.deepEqual(edited, [...added.slice(0, 4), 'renamed', ...added.slice(5)]);
  }
});
