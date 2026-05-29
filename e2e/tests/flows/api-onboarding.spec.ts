/**
 * Onboarding flow — API-mode auth.
 *
 * Picked up only by the `flows-api` project. Kept in its own file so the API
 * flow doesn't pay the cost of spinning up the WalletConnect fixture (which
 * also wants WC_PROJECT_ID).
 */

import { test, expect } from '../../fixtures/test';
import { HELLO_WORLD_ACTION } from '../../fixtures/api-client';

test.describe('onboarding flow (api)', () => {
  test('user can sign in and run a Lit Action', async ({
    dashboardPage,
    apiClient,
  }) => {
    await dashboardPage.goto();

    const stamp = Date.now();
    const account = await apiClient.newAccount({
      account_name: `e2e-onboard-${stamp}`,
      account_description: 'onboarding flow (api)',
    });
    await dashboardPage.loginWithApiKey(account.api_key);
    const { usage_api_key: usageApiKey } = await apiClient.addUsageApiKey(account.api_key, {
      name: `e2e-onboard-${stamp}-usage`,
      execute_in_groups: [0],
    });
    const result = await dashboardPage.runLitAction({
      usageApiKey,
      code: HELLO_WORLD_ACTION,
    });
    expect(result.response).toBe('Hello World!');
  });
});
