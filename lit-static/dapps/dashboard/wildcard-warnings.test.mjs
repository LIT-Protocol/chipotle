import assert from 'node:assert/strict';
import test from 'node:test';
import { populateWildcardWarnings } from './wildcard-warnings.js';
import { usageKeyHasAllGroupsWildcard } from './keys.js';

const wildcard = { wallet_address: '0x0000000000000000000000000000000000000000' };
const scoped = { wallet_address: '0x0000000000000000000000000000000000000001' };
function options(listWalletsInGroup, extra = {}) {
  return {
    rows: [{ groupId: '1' }], getClient: async () => ({ listWalletsInGroup }),
    apiKey: 'test', pageSize: 20, isCurrent: () => true,
    onWarning: () => assert.fail('unexpected warning'), onError: (e) => { throw e; },
    ...extra,
  };
}

test('only explicit execution wildcard permissions trigger the key warning', () => {
  for (const id of [0, '0', 0n, '0x0000']) {
    assert.equal(usageKeyHasAllGroupsWildcard({ can_execute_in_groups: [12, id] }), true);
  }
  for (const item of [null, {}, { can_execute_in_groups: [] },
    { can_execute_in_groups: [1, 2] }, { can_execute_in_groups: [null, '', false] },
    { can_add_pkp_to_groups: [0] }]) assert.equal(usageKeyHasAllGroupsWildcard(item), false);
});

test('finds a wildcard after the first 20 wallets', async () => {
  const pages = [], warnings = [];
  await populateWildcardWarnings(options(async ({ pageNumber }) => {
    pages.push(pageNumber);
    return pageNumber === '0' ? Array(20).fill(scoped) : [wildcard];
  }, { onWarning: (row) => warnings.push(row.groupId) }));
  assert.deepEqual(pages, ['0', '1']);
  assert.deepEqual(warnings, ['1']);
});

test('fully scoped exact-size pages terminate on the empty next page', async () => {
  const pages = [];
  await populateWildcardWarnings(options(async ({ pageNumber }) => {
    pages.push(pageNumber);
    return pageNumber === '0' ? Array(20).fill(scoped) : [];
  }));
  assert.deepEqual(pages, ['0', '1']);
});

test('stale in-flight responses neither warn nor request more pages/groups', async () => {
  for (const response of [[wildcard], Array(20).fill(scoped)]) {
    let current = true, calls = 0;
    await populateWildcardWarnings(options(async () => {
      calls++;
      current = false;
      return response;
    }, { isCurrent: () => current }));
    assert.equal(calls, 1);
  }
});

test('session change during client initialization prevents requests', async () => {
  let current = true;
  await populateWildcardWarnings(options(() => assert.fail('stale request'), {
    isCurrent: () => current,
    getClient: async () => { current = false; return {}; },
  }));
});

test('bounds concurrency and continues after a failed group lookup', async () => {
  let active = 0, peak = 0;
  const errors = [], warnings = [];
  await populateWildcardWarnings(options(async ({ groupId }) => {
    peak = Math.max(peak, ++active);
    await new Promise((resolve) => setImmediate(resolve));
    active--;
    if (groupId === '0') throw new Error('lookup failed');
    return [wildcard];
  }, {
    rows: Array.from({ length: 10 }, (_, i) => ({ groupId: String(i) })),
    onWarning: (row) => warnings.push(row.groupId), onError: (e) => errors.push(e),
  }));
  assert.equal(peak, 4);
  assert.equal(errors.length, 1);
  assert.equal(warnings.length, 9);
});
