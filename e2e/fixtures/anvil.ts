import { createPublicClient, createWalletClient, http, type Address, type Hex } from 'viem';
import { foundry } from 'viem/chains';
import { privateKeyToAccount } from 'viem/accounts';

export const ANVIL_RPC_URL = process.env.ANVIL_RPC_URL ?? 'http://127.0.0.1:8545';

// Anvil's first 10 deterministic private keys. Index matches `anvil` startup logs.
export const ANVIL_PRIVATE_KEYS: readonly Hex[] = [
  '0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80',
  '0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d',
  '0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a',
  '0x7c852118294e51e653712a81e05800f419141751be58f605c371e15141b007a6',
  '0x47e179ec197488593b187f80a00eb0da91f1b9d0b13f8733639f19c30a34926a',
  '0x8b3a350cf5c34c9194ca85829a2df0ec3153be0318b5e2d3348e872092edffba',
  '0x92db14e403b83dfe3df233f83dfa3a0d7096f21ca9b0d6d6b8d88b2b4ec1564e',
  '0x4bbbf85ce3377467afe5d46f804f221813b2bb87f24d81f60f1fcdbf7cbf4356',
  '0xdbda1821b80551c9d65939329250298aa3472ba22feea921c0cf5d620ea67b97',
  '0x2a871d0798f97d79848a013d4936a73bf4cc922c825d33c1cf7073dff6d409c6',
] as const;

export const publicClient = createPublicClient({
  chain: foundry,
  transport: http(ANVIL_RPC_URL),
});

export function walletFor(index: number) {
  const pk = ANVIL_PRIVATE_KEYS[index];
  if (!pk) throw new Error(`No Anvil account at index ${index}`);
  const account = privateKeyToAccount(pk);
  const client = createWalletClient({ account, chain: foundry, transport: http(ANVIL_RPC_URL) });
  return { account, client, privateKey: pk };
}

// Raw Anvil cheat RPC. Add to this as you need more.
async function rpc<T = unknown>(method: string, params: unknown[] = []): Promise<T> {
  const res = await fetch(ANVIL_RPC_URL, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }),
  });
  const json = (await res.json()) as { result?: T; error?: { message: string } };
  if (json.error) throw new Error(`${method} failed: ${json.error.message}`);
  return json.result as T;
}

export const anvil = {
  snapshot: () => rpc<Hex>('evm_snapshot'),
  revert: (id: Hex) => rpc<boolean>('evm_revert', [id]),
  mine: (blocks = 1) => rpc<Hex>('anvil_mine', [`0x${blocks.toString(16)}`]),
  setBalance: (addr: Address, weiHex: Hex) =>
    rpc<boolean>('anvil_setBalance', [addr, weiHex]),
  setStorageAt: (addr: Address, slot: Hex, value: Hex) =>
    rpc<boolean>('anvil_setStorageAt', [addr, slot, value]),
  impersonate: (addr: Address) => rpc<null>('anvil_impersonateAccount', [addr]),
  stopImpersonating: (addr: Address) =>
    rpc<null>('anvil_stopImpersonatingAccount', [addr]),
  setNextBlockTimestamp: (ts: number) =>
    rpc<null>('evm_setNextBlockTimestamp', [ts]),
};
