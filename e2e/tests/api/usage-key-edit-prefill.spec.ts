/**
 * Usage API key edit modal — confirm ALL options pre-fill when editing.
 *
 * The usage-key modal already had preselect logic (keys.js openUsageKeyModal),
 * and unlike list_groups, list_api_keys returns the full permission set. This
 * test proves every option round-trips: the three capability checkboxes plus
 * all four group multi-selects (execute / manage-actions / add-PKP / remove-PKP).
 *
 * Captures screenshots of each set.
 */

import { mkdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { test, expect } from '../../fixtures/test';
import { API_BASE_URL } from '../../playwright.config';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const SHOT_DIR = path.resolve(HERE, '../../artifacts/usage-key-prefill');

const api = (p: string) => `${API_BASE_URL.replace(/\/$/, '')}/core/v1${p}`;
async function post<T>(p: string, key: string | null, body: unknown): Promise<T> {
  const res = await fetch(api(p), {
    method: 'POST',
    headers: { 'content-type': 'application/json', ...(key ? { 'x-api-key': key } : {}) },
    body: JSON.stringify(body),
  });
  const text = await res.text();
  if (!res.ok) throw new Error(`POST ${p} → ${res.status}: ${text.slice(0, 300)}`);
  try { return JSON.parse(text) as T; } catch { return text as unknown as T; }
}

test.describe('usage API key edit modal pre-fills every option', () => {
  test('capabilities + all four group multi-selects load back', async ({ dashboardPage, apiClient }) => {
    mkdirSync(SHOT_DIR, { recursive: true });
    const stamp = Date.now();

    // account
    const { api_key: key } = await apiClient.newAccount({
      account_name: `uk-prefill-${stamp}`,
      account_description: 'usage key edit pre-fill screenshots',
    });

    // three groups → capture their on-chain ids
    const gid: number[] = [];
    for (const name of ['G1', 'G2', 'G3']) {
      const r = await post<{ group_id: string }>('/add_group', key, {
        group_name: name, group_description: '', pkp_ids_permitted: [], cid_hashes_permitted: [],
      });
      gid.push(Number(r.group_id));
    }
    const [g1, g2, g3] = gid;

    // one usage key with a deliberately mixed permission set across every option
    const perms = {
      can_create_groups: true,
      can_delete_groups: false,
      can_create_pkps: true,
      execute_in_groups: [g1, g2],
      manage_ipfs_ids_in_groups: [g1],
      add_pkp_to_groups: [g2, g3],
      remove_pkp_from_groups: [g3],
    };
    await apiClient.addUsageApiKey(key, { name: `uk-${stamp}`, description: 'all options', ...perms });

    // log in — preloadAllTables() loads groups (multi-select options) + usage keys
    await dashboardPage.goto();
    await dashboardPage.loginWithApiKey(key);
    const page = dashboardPage.page;
    await page.locator('#btn-load-usage-keys').click();
    await expect(page.locator('#usage-keys-tbody tr')).toHaveCount(1);
    await expect(page.locator('#groups-tbody tr')).toHaveCount(3);

    // open the edit modal
    await page.locator('#usage-keys-tbody tr button[title="Edit"]').click();
    await expect(page.locator('#modal-title')).toHaveText('Edit usage API key');
    const overlay = page.locator('#modal-overlay');

    // ── capability checkboxes (disabled in edit, but must reflect saved state) ──
    await expect(page.locator('#modal-usage-can-create-groups')).toBeChecked();
    await expect(page.locator('#modal-usage-can-delete-groups')).not.toBeChecked();
    await expect(page.locator('#modal-usage-can-create-pkps')).toBeChecked();

    // ── four group multi-selects: each pre-checks exactly its saved subset ──
    const groups: Array<[string, number[]]> = [
      ['#modal-usage-execute-groups', perms.execute_in_groups],
      ['#modal-usage-manage-ipfs-groups', perms.manage_ipfs_ids_in_groups],
      ['#modal-usage-add-pkp-groups', perms.add_pkp_to_groups],
      ['#modal-usage-remove-pkp-groups', perms.remove_pkp_from_groups],
    ];
    // Assert the EXACT checked group-id values, not just the count — the bug
    // was mismatched IDs, so a same-cardinality wrong set must fail here.
    for (const [sel, expected] of groups) {
      await expect
        .poll(() =>
          page.locator(`${sel} input:checked`).evaluateAll((els) =>
            (els as HTMLInputElement[]).map((e) => e.value).sort(),
          ),
        )
        .toEqual(expected.map(String).sort());
    }

    // screenshot: collapsed (summaries + capability boxes visible)
    await overlay.screenshot({ path: path.join(SHOT_DIR, 'usage-key--summary.png') });

    // screenshot each multi-select expanded so the checked boxes are visible
    const shots: Array<[string, string]> = [
      ['#modal-usage-execute-groups', 'execute-groups'],
      ['#modal-usage-manage-ipfs-groups', 'manage-actions'],
      ['#modal-usage-add-pkp-groups', 'add-pkp'],
      ['#modal-usage-remove-pkp-groups', 'remove-pkp'],
    ];
    for (const [sel, slug] of shots) {
      await page.locator(`${sel} .ms-trigger`).click();
      await overlay.screenshot({ path: path.join(SHOT_DIR, `usage-key--${slug}-open.png`) });
    }
  });
});
