/**
 * `POST /core/v1/prepare_wallet` — the unsigned derived-wallet-address endpoint.
 *
 * This is the no-signature equivalent of `create_wallet_with_signature`: it lets
 * a ChainSecured client obtain a PKP address before registering it on-chain, so
 * the owner ceremony collapses into a single signed bind UserOp (flows#532).
 *
 * Covers:
 *   - No auth / no body required; returns a well-formed address + derivation path.
 *   - NOT idempotent: two calls return two different wallets (no server-side dedup).
 */

import { test, expect } from '../../fixtures/test';

const ADDRESS_RE = /^0x[0-9a-fA-F]{40}$/;
// 0x-prefixed lowercase hex uint256; server formats via `format!("0x{:x}", ..)`
// so leading zeros are trimmed — accept 1..=64 hex digits.
const DERIVATION_PATH_RE = /^0x[0-9a-f]{1,64}$/;

test.describe('prepare_wallet (unsigned derived address)', () => {
  test('returns a well-formed address and derivation path with no auth', async ({ apiClient }) => {
    const res = await apiClient.prepareWallet();
    expect(res.wallet_address).toMatch(ADDRESS_RE);
    expect(res.derivation_path).toMatch(DERIVATION_PATH_RE);
    expect(res.derivation_path).not.toBe('0x0');
  });

  test('is NOT idempotent — each call mints a distinct wallet', async ({ apiClient }) => {
    const a = await apiClient.prepareWallet();
    const b = await apiClient.prepareWallet();
    expect(a.wallet_address.toLowerCase()).not.toBe(b.wallet_address.toLowerCase());
    expect(a.derivation_path).not.toBe(b.derivation_path);
  });
});
