// Run with: node --test test/backfill-path-owners.test.cjs
const { test } = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const path = require('node:path');
const ts = require('typescript');
const { ethers } = require('ethers');
function harness(options = {}) {
  let action;
  const args = { diamond: ethers.toBeHex(1, 20) }, reads = [], files = [];
  const builder = {
    addParam() { return this; },
    addOptionalParam(n, _d, v) { args[n] = v; return this; },
    addFlag(n) { args[n] = false; return this; },
    setAction(fn) { action = fn; return this; },
  };
  const count = options.count ?? 1;
  const contract = {
    filters: { WalletDerivationRegistered() { return {}; } },
    async queryFilter() { return options.logs || []; },
    async pkpCount(s) { reads.push(['count', s]); return BigInt(count); },
    async allPkpIdsAt(i, s) {
      reads.push(['index', i, s]);
      assert.ok(i >= 1 && i <= count, 'global PKP ledger is one-based');
      return ethers.toBeHex(i, 20);
    },
    async getPkpOwnerMaster(_pkp, s) { reads.push(['owner', s]); return options.unbound ? 0n : 99n; },
    async getWalletDerivation(_owner, pkp, s) {
      reads.push(['path', s]);
      if (options.unresolved) throw new Error('unresolved');
      if (options.denied) {
        const error = new Error('ownership denied');
        error.data = new ethers.Interface(['error InvalidRequest(string)']).encodeErrorResult('InvalidRequest', [options.denied]);
        throw error;
      }
      return BigInt(pkp) + 1000n;
    },
    async getPathOwnerMaster(p) { return p === 1n ? 0n : (options.bound ?? 0n); },
  };
  class Provider {
    async getNetwork() { return { chainId: options.chainId ?? 8453n }; }
    async getBlock() { return { number: 100 }; }
  }
  const source = fs.readFileSync(path.join(__dirname, '../tasks/backfill-path-owners.ts'), 'utf8');
  const js = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
  vm.runInNewContext(js, {
    exports: {},
    require(name) {
      if (name === 'hardhat/config') return { task: () => builder };
      if (name === 'ethers') return { ethers: { ...ethers, JsonRpcProvider: Provider, Contract: function () { return contract; } } };
      if (name === './rpc-retry') return { withRetry: (_label, fn) => fn() };
      if (name === 'fs') return { mkdirSync() {}, writeFileSync(file, data) { files.push({ file, data: JSON.parse(data) }); } };
      return require(name);
    },
    console: { log() {} }, process: { stdout: { write() {} }, env: {} },
  });
  return { reads, files, run(overrides = {}) {
    return action({ ...args, ...overrides }, { network: { name: 'base', config: { chainId: 8453, url: 'mock' } } });
  } };
}
test('includes last storage-only PKP and pins reads to finalized snapshot', async () => {
  const h = harness({ count: 2 }); await h.run({ safeOut: 'out' });
  assert.deepEqual(h.reads.filter(r => r[0] === 'index').map(r => r[1]), [1, 2]);
  for (const r of h.reads) assert.equal(r.at(-1).blockTag, 100);
  const iface = new ethers.Interface(['function backfillPathOwners(uint256[],uint256[])']);
  const decoded = iface.decodeFunctionData('backfillPathOwners', h.files[0].data.transactions[0].data);
  assert.deepEqual(Array.from(decoded[0]), [1001n, 1002n]);
  assert.deepEqual(Array.from(decoded[1]), [99n, 99n]);
  assert.equal(h.files[0].data.chainId, '8453');
});
test('wrong binding blocks completion even with allowConflicts', async () => {
  const h = harness({ bound: 123n });
  await assert.rejects(h.run({ safeOut: 'out', allowConflicts: true }), /1 incorrectly bound/);
  assert.equal(h.files.length, 0);
});
for (const [name, options, message] of [
  ['unbound PKP', { unbound: true }, /1 PKP\(s\) without owners/],
  ['unresolved PKP', { unresolved: true }, /1 unresolved PKP/],
]) test(`${name} prevents incomplete output`, async () => {
  const h = harness(options); await assert.rejects(h.run({ safeOut: 'out' }), message);
  assert.equal(h.files.length, 0);
});
test('correctly bound paths are skipped', async () => {
  const h = harness({ bound: 99n }); await h.run({ safeOut: 'out' }); assert.equal(h.files.length, 0);
});
test('Safe defaults split 401 paths into files of 400 and 1', async () => {
  const h = harness({ count: 401 }); await h.run({ safeOut: 'out' });
  assert.deepEqual(h.files.map(f => f.data.transactions.length), [2, 1]);
});
test('rejects oversized Safe batches and invalid integers before reads', async () => {
  const h = harness();
  await assert.rejects(h.run({ safeOut: 'out', callsPerFile: '10' }), /at most 400 paths/);
  for (const o of [{ batchSize: '0' }, { fromBlock: '-1' }, { concurrency: '0' }, { chunkSize: '100oops' }, { callsPerFile: '1.5' }]) await assert.rejects(h.run(o), /must be an integer/);
  assert.equal(h.reads.length, 0);
});
test('rejects wrong RPC chain', async () => { await assert.rejects(harness({ chainId: 1n }).run(), /does not match/); });
test('event/storage conflict blocks Safe output by default', async () => {
  const h = harness({ logs: [{ args: [88n, ethers.toBeHex(2, 20), 1001n], blockNumber: 1, transactionIndex: 0, index: 0, transactionHash: '0x1234' }] });
  await assert.rejects(h.run({ safeOut: 'out' }), /unresolved conflict/); assert.equal(h.files.length, 0);
});

function aliasHistory() {
  return [88n, 99n].map((master, i) => ({ args: [master, ethers.toBeHex(1, 20), 1001n], blockNumber: i + 1, transactionIndex: 0, index: 0, transactionHash: '0x1234' }));
}
test('post-backfill denial of a corroborated historical alias is expected', async () => {
  const h = harness({ logs: aliasHistory(), bound: 88n, denied: 'derivation path owned by another account' });
  await h.run();
  assert.equal(h.files.length, 0);
});
test('alias denial without the canonical binding remains unresolved', async () => {
  const h = harness({ logs: aliasHistory(), bound: 0n, denied: 'derivation path owned by another account' });
  await assert.rejects(h.run(), /1 unresolved PKP/);
});
test('unrelated errors on historical aliases remain unresolved', async () => {
  const h = harness({ logs: aliasHistory(), bound: 88n, denied: 'PKP owned by another account' });
  await assert.rejects(h.run(), /1 unresolved PKP/);
});
