/**
 * ECDSA sign — parity with `k6/correctness/lit-action-ecdsa-sign.spec.ts`.
 *
 * Runs the ecdsa-sign Lit Action via the dashboard, asserts no error, and
 * verifies the returned signature recovers to the wallet address the action
 * derived from the action-private-key (the TODO in the k6 spec). This gives
 * the e2e suite a bit more bite than k6 currently has.
 */

import { test, expect } from '../../fixtures/test';
import { ECDSA_SIGN_ACTION } from '../../fixtures/api-client';
import { verifyMessage, getAddress } from 'ethers';

test.describe('lit action — ecdsa-sign', () => {
  test('signature recovers to the address the action returned', async ({
    apiClient,
    dashboardPage,
  }) => {
    const stamp = Date.now();
    const account = await apiClient.newAccount({
      account_name: `e2e-ecdsa-${stamp}`,
      account_description: 'e2e ecdsa-sign test',
    });
    const { usage_api_key: usageApiKey } = await apiClient.addUsageApiKey(account.api_key, {
      name: `e2e-ecdsa-${stamp}-usage`,
      execute_in_groups: [0],
    });

    await dashboardPage.goto();
    await dashboardPage.loginWithApiKey(account.api_key);

    const result = await dashboardPage.runLitAction({
      usageApiKey,
      code: ECDSA_SIGN_ACTION,
    });
    expect(result.has_error).toBe(false);
    const payload = result.response as {
      wallet_address?: string;
      signature?: string;
      publicKey?: string;
    };
    expect(payload?.wallet_address).toMatch(/^0x[0-9a-fA-F]{40}$/);
    expect(payload?.signature).toMatch(/^0x[0-9a-fA-F]+$/);

    const recovered = verifyMessage('Chipotle Rocks!', payload!.signature!);
    expect(getAddress(recovered)).toBe(getAddress(payload!.wallet_address!));
  });
});
