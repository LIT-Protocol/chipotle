/**
 * Group edit modal — saved permissions pre-fill (regression for the bug where
 * editing a group opened the "PKP IDs permitted" / "CID hashes permitted"
 * multi-selects empty instead of showing the group's saved members).
 *
 * Provisions wallets + actions + groups with different permitted subsets via
 * the API, then opens each group's edit modal in the dashboard and asserts the
 * right checkboxes come back checked. Captures screenshots for each set.
 */

import { mkdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { test, expect } from '../../fixtures/test';
import { API_BASE_URL } from '../../playwright.config';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const SHOT_DIR = path.resolve(HERE, '../../artifacts/group-prefill');

// ── tiny HTTP helpers (the shared apiClient doesn't cover these routes) ──────
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

async function getJson<T>(p: string, key: string): Promise<T> {
  const res = await fetch(api(p), { headers: { 'x-api-key': key } });
  const text = await res.text();
  if (!res.ok) throw new Error(`GET ${p} → ${res.status}: ${text.slice(0, 300)}`);
  return JSON.parse(text) as T;
}

const ZERO_ADDR = '0x0000000000000000000000000000000000000000'; // "All" sentinel

test.describe('group edit modal pre-fills saved permissions', () => {
  test('various permitted subsets load back into the edit modal', async ({ dashboardPage, apiClient }) => {
    mkdirSync(SHOT_DIR, { recursive: true });
    const stamp = Date.now();

    // 1) account
    const { api_key: key } = await apiClient.newAccount({
      account_name: `grp-prefill-${stamp}`,
      account_description: 'group edit pre-fill screenshots',
    });

    // 2) three PKP wallets (plus the Account Master Wallet that already exists)
    const wallets: Record<string, string> = {};
    for (const label of ['W1', 'W2', 'W3']) {
      const { wallet_address } = await getJson<{ wallet_address: string }>('/create_wallet', key);
      wallets[label] = wallet_address;
    }

    // 3) three actions — get a CID per snippet, register it, then read its hashed id
    const actionHash: Record<string, string> = {};
    for (const [label, code] of [
      ['A1', 'async function main(){return "a1";}'],
      ['A2', 'async function main(){return "a2";}'],
      ['A3', 'async function main(){return "a3";}'],
    ] as const) {
      const cid = await apiClient.getLitActionIpfsId(code);
      await post('/add_action', key, { action_ipfs_cid: cid, name: label, description: '' });
    }
    const actions = await getJson<Array<{ id: string; name: string }>>(
      '/list_actions?page_number=0&page_size=50',
      key,
    );
    for (const a of actions) actionHash[a.name] = a.id;

    // 4) groups with distinct permitted subsets
    const groups = [
      { name: 'Two PKPs + one action', pkps: [wallets.W1, wallets.W2], cids: [actionHash.A1] },
      { name: 'One PKP + two actions', pkps: [wallets.W3], cids: [actionHash.A2, actionHash.A3] },
      { name: 'All PKPs + one action', pkps: [ZERO_ADDR], cids: [actionHash.A1] },
    ];
    for (const g of groups) {
      await post('/add_group', key, {
        group_name: g.name,
        group_description: '',
        pkp_ids_permitted: g.pkps,
        cid_hashes_permitted: g.cids,
      });
    }

    // 5) log in — preloadAllTables() populates wallets/actions/groups stores
    await dashboardPage.goto();
    await dashboardPage.loginWithApiKey(key);
    const page = dashboardPage.page;
    // make sure the multi-select option sources are loaded before opening modals
    await expect(page.locator('#wallets-tbody tr')).toHaveCount(4); // AMW + W1..W3
    await expect(page.locator('#actions-tbody tr')).toHaveCount(3);
    await page.locator('#btn-load-groups').click();
    await expect(page.locator('#groups-tbody tr')).toHaveCount(groups.length);

    // 6) open each group's edit modal and capture the pre-filled checkboxes
    for (const g of groups) {
      const row = page.locator('#groups-tbody tr', { hasText: g.name });
      await row.locator('button[title="Edit"]').click();

      const overlay = page.locator('#modal-overlay');
      await expect(page.locator('#modal-title')).toHaveText('Edit group');

      // preselect is async (fetches listWalletsInGroup + listActions) — wait for
      // the boxes to actually be checked before asserting / screenshotting.
      const pkpWrap = page.locator('#modal-group-pkp-ids');
      const cidWrap = page.locator('#modal-group-cid-hashes');
      await expect(pkpWrap.locator('input:checked')).toHaveCount(g.pkps.length);
      await expect(cidWrap.locator('input:checked')).toHaveCount(g.cids.length);

      const slug = g.name.toLowerCase().replace(/[^a-z0-9]+/g, '-');
      await overlay.screenshot({ path: path.join(SHOT_DIR, `${slug}--summary.png`) });

      // expand the PKP dropdown to show the checked boxes
      await pkpWrap.locator('.ms-trigger').click();
      await overlay.screenshot({ path: path.join(SHOT_DIR, `${slug}--pkps-open.png`) });

      // expand the CID dropdown (clicking it auto-closes the PKP one)
      await cidWrap.locator('.ms-trigger').click();
      await overlay.screenshot({ path: path.join(SHOT_DIR, `${slug}--actions-open.png`) });

      await page.locator('#modal-cancel-btn').click();
      await expect(overlay).not.toHaveClass(/is-open/);
    }
  });
});
