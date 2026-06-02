/**
 * Exercises the dashboard's "Get Lit Action IPFS CID" button.
 *
 * The CID is deterministic on the action source, so we assert format + that
 * two calls on the same source return the same value. This also covers the
 * `POST /get_lit_action_ipfs_id` route (k6's litApiServer.ts wraps it but no
 * existing k6 spec calls it).
 */

import { test, expect } from '../../fixtures/test';
import { HELLO_WORLD_ACTION } from '../../fixtures/api-client';

test.describe('lit action IPFS CID', () => {
  test('CID is non-empty, formatted, and stable on the same source', async ({
    apiClient,
    dashboardPage,
  }) => {
    const stamp = Date.now();
    const account = await apiClient.newAccount({
      account_name: `e2e-cid-${stamp}`,
      account_description: 'e2e cid test',
    });

    await dashboardPage.goto();
    await dashboardPage.loginWithApiKey(account.api_key);

    const cid = await dashboardPage.getLitActionIpfsCid(HELLO_WORLD_ACTION);
    expect(cid).toMatch(/^Qm[1-9A-HJ-NP-Za-km-z]{44}$|^bafy[a-z2-7]+$/);

    const cidAgain = await apiClient.getLitActionIpfsId(HELLO_WORLD_ACTION);
    expect(cidAgain).toBe(cid);
  });
});
