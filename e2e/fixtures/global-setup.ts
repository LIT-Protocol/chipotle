/**
 * Playwright `globalSetup` — runs once before any tests, registers the
 * lit-api-server's TEE-derived api_payer wallet(s) on the AccountConfig
 * diamond, and funds them with gas.
 *
 * Why: `local_test.sh` deploys the contracts but doesn't call
 * `setApiPayers(...)`, so out of the box the api_payer set is empty. The
 * dashboard's API-mode `/new_account` then fails with
 * "ChainSecured accounts must be unmanaged" because the contract refuses to
 * mint a managed account from an unauthorized caller.
 *
 * Two-pass because lit-api-server only reveals its dstack-derived api_payer
 * addresses via `/get_api_payers`, which itself reads the contract's payer
 * count and asks dstack for that many keys. So we:
 *   1. Bump the on-chain count to 1 (registering a placeholder).
 *   2. Read /get_api_payers to learn what addresses lit-api-server expects.
 *   3. Set api_payers to that list and fund each address.
 *
 * Idempotent — re-running with the list already in sync is a no-op write.
 */

import {
  createPublicClient,
  createWalletClient,
  http,
  encodeFunctionData,
  parseAbi,
  parseEther,
  type Address,
} from 'viem';
import { foundry } from 'viem/chains';
import { privateKeyToAccount } from 'viem/accounts';
import { ANVIL_PRIVATE_KEYS, ANVIL_RPC_URL } from './anvil';
import { accountConfigAddress } from './contracts';
import { API_BASE_URL } from '../playwright.config';

const ABI = parseAbi([
  'function api_payers() view returns (address[])',
  'function setApiPayers(address[] newApiPayers)',
]);

const PLACEHOLDER_ADDR = '0x000000000000000000000000000000000000dEaD' as Address;
const FUND_AMOUNT = parseEther('10');

async function fetchApiPayers(): Promise<Address[]> {
  const res = await fetch(`${API_BASE_URL}/core/v1/get_api_payers`);
  if (!res.ok) {
    throw new Error(`GET /get_api_payers → ${res.status} ${res.statusText}`);
  }
  return (await res.json()) as Address[];
}

export default async function globalSetup(): Promise<void> {
  // Remote environments (staging/dev CVMs) already have api_payers registered
  // and funded — this bootstrap is only for the local_test.sh Anvil stack.
  if (!/localhost|127\.0\.0\.1/.test(API_BASE_URL)) {
    // eslint-disable-next-line no-console
    console.log(`[global-setup] remote API_BASE_URL (${API_BASE_URL}) — skipping local Anvil bootstrap`);
    return;
  }

  const proxy = accountConfigAddress();

  const publicClient = createPublicClient({ chain: foundry, transport: http(ANVIL_RPC_URL) });

  // Anvil account #0 deployed the diamond in local_test.sh, so it owns the
  // setApiPayers gate.
  const deployer = privateKeyToAccount(ANVIL_PRIVATE_KEYS[0]);
  const wallet = createWalletClient({
    account: deployer,
    chain: foundry,
    transport: http(ANVIL_RPC_URL),
  });

  async function setApiPayers(addrs: Address[]): Promise<void> {
    const data = encodeFunctionData({
      abi: ABI,
      functionName: 'setApiPayers',
      args: [addrs],
    });
    const txHash = await wallet.sendTransaction({ to: proxy, data });
    await publicClient.waitForTransactionReceipt({ hash: txHash });
  }

  // Pass 1: ensure the count is non-zero so /get_api_payers can derive keys.
  const onChain = (await publicClient.readContract({
    address: proxy,
    abi: ABI,
    functionName: 'api_payers',
  })) as Address[];
  if (onChain.length === 0) {
    await setApiPayers([PLACEHOLDER_ADDR]);
  }

  // Pass 2: ask the server which addresses it would sign as, and sync.
  const expected = await fetchApiPayers();
  if (expected.length === 0) {
    throw new Error(
      '[global-setup] /get_api_payers still empty after bumping the count — is lit-api-server up?',
    );
  }

  const sameAsExpected =
    onChain.length === expected.length &&
    onChain.every((a, i) => a.toLowerCase() === expected[i]!.toLowerCase());
  if (!sameAsExpected) {
    await setApiPayers(expected);
    // eslint-disable-next-line no-console
    console.log(`[global-setup] registered api_payers ${expected.join(', ')} on ${proxy}`);
  }

  // Fund each api_payer so it can submit transactions.
  for (const addr of expected) {
    const balance = await publicClient.getBalance({ address: addr });
    if (balance < parseEther('1')) {
      const txHash = await wallet.sendTransaction({ to: addr, value: FUND_AMOUNT });
      await publicClient.waitForTransactionReceipt({ hash: txHash });
      // eslint-disable-next-line no-console
      console.log(`[global-setup] funded api_payer ${addr} with 10 ETH`);
    }
  }
}
