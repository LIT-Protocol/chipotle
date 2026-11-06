import assert from 'node:assert/strict';
import test from 'node:test';
import { setGroupsStore } from './auth.js';
import { buildGroupMultiSelect, attachGroupMultiSelectLogic, getSelectedGroupIds } from './groups.js';
import { renderPermissionSummary } from './keys.js';

test('only execution offers All Groups; management keeps explicit IDs', () => {
  setGroupsStore([{ id: '0x01', name: 'First' }, { id: '2', name: 'Second' }]);
  const management = buildGroupMultiSelect('management', false);
  assert.doesNotMatch(management, /All Groups|value="0"/);
  assert.match(management, /value="1"/);
  assert.match(management, /value="2"/);
  assert.match(buildGroupMultiSelect('execute', false, { allowAll: true }), /value="0".*All Groups/);
  setGroupsStore([]);
  assert.doesNotMatch(buildGroupMultiSelect('management', false), /type="checkbox"/);
});

test('editing preserves legacy zero and grants outside the loaded group page', () => {
  setGroupsStore([{ id: '0x01', name: 'First' }]);
  const html = buildGroupMultiSelect('management', false, { selectedIds: [0, 1, 42] });
  assert.match(html, /value="0".*0 \(no management access\)/);
  assert.match(html, /value="42".*Group 42 \(not listed\)/);
  assert.equal((html.match(/value="1"/g) || []).length, 1);
  assert.doesNotMatch(html, /All Groups/);
});

test('permission summaries distinguish execution wildcards from ineffective management grants', () => {
  assert.equal(renderPermissionSummary({
    can_execute_in_groups: ['0'],
    can_manage_ipfs_ids_in_groups: [0, 2],
    can_add_pkp_to_groups: [0],
    can_remove_pkp_from_groups: [3],
  }), 'execute: all; manage actions: 0 (no management access), 2; add PKP: 0 (no management access); remove PKP: 3');
});

test('legacy management zero coexists with explicit selections; execution zero stays exclusive', () => {
  // Minimal DOM interface exercises the same change handlers and payload reader.
  const original = globalThis.document;
  try {
    for (const allowAll of [false, true]) {
      const boxes = ['0', '2'].map(value => ({ value, checked: true, nextElementSibling: { textContent: value }, addEventListener(_, callback) { this.change = callback; } }));
      const wrap = {
        dataset: {},
        querySelector(selector) { return selector === '.ms-trigger' ? { addEventListener() {} } : selector === '.ms-summary' ? {} : boxes[0]; },
        querySelectorAll(selector) { return selector.endsWith(':checked') ? boxes.filter(b => b.checked) : boxes; },
      };
      globalThis.document = { getElementById: () => wrap };
      attachGroupMultiSelectLogic('groups', allowAll);
      boxes[1].change();
      assert.deepEqual(getSelectedGroupIds('groups'), allowAll ? [2] : [0, 2]);
      boxes[0].checked = true;
      boxes[0].change();
      assert.deepEqual(getSelectedGroupIds('groups'), allowAll ? [0] : [0, 2]);
    }
  } finally {
    if (original === undefined) delete globalThis.document;
    else globalThis.document = original;
  }
});
