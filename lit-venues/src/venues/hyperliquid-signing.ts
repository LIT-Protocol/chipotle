/**
 * Hyperliquid's two signing schemes (plan D8), pinned against the official
 * Python SDK's test vectors in test/hyperliquid-signing.test.ts:
 *
 *  1. L1 (trading) actions — msgpack(action) ‖ nonce_be64 ‖ vault marker
 *     [‖ 0x00 ‖ expiresAfter_be64] → keccak → "phantom agent"
 *     {source: "a"|"b", connectionId} signed as EIP-712 under domain
 *     {name: "Exchange", chainId: 1337}.
 *  2. User-signed actions (approveAgent, usdSend, withdraw) — plain EIP-712
 *     under {name: "HyperliquidSignTransaction", chainId: signatureChainId}.
 *
 * Byte-exact msgpack fidelity is the #1 correctness risk here (plan D8); do
 * not reorder any object literal that feeds actionHash.
 */

import { keccak_256 } from '@noble/hashes/sha3';
import { bytesToHex, concatBytes, hexToBytes } from '@noble/hashes/utils';
import { msgpackEncode } from '../msgpack';
import {
  ZERO_ADDRESS,
  typedDataDigest,
  type Eip712Field,
  type RsvSignature,
  type SignFn,
} from '../eip712';

export const HYPERLIQUID_SIGNATURE_CHAIN_ID = '0x66eee'; // Arbitrum Sepolia, fixed by the venue for user-signed actions

function be64(v: number | bigint): Uint8Array {
  const out = new Uint8Array(8);
  let rest = BigInt(v);
  for (let i = 7; i >= 0; i--) {
    out[i] = Number(rest & 0xffn);
    rest >>= 8n;
  }
  return out;
}

/**
 * keccak over msgpack(action) + nonce + vault/expiry markers — the exact byte
 * layout of the SDK's `action_hash(action, vault_address, nonce, expires_after)`.
 */
export function actionHash(
  action: unknown,
  nonce: number,
  vaultAddress?: string,
  expiresAfter?: number,
): Uint8Array {
  const parts: Uint8Array[] = [msgpackEncode(action), be64(nonce)];
  if (vaultAddress === undefined) {
    parts.push(Uint8Array.from([0x00]));
  } else {
    const hex = vaultAddress.startsWith('0x') ? vaultAddress.slice(2) : vaultAddress;
    parts.push(Uint8Array.from([0x01]), hexToBytes(hex.toLowerCase()));
  }
  if (expiresAfter !== undefined) {
    parts.push(Uint8Array.from([0x00]), be64(expiresAfter));
  }
  return keccak_256(concatBytes(...parts));
}

const AGENT_FIELDS: Eip712Field[] = [
  { name: 'source', type: 'string' },
  { name: 'connectionId', type: 'bytes32' },
];

export interface L1SignOptions {
  isMainnet: boolean;
  vaultAddress?: string;
  expiresAfter?: number;
}

/** The phantom-agent struct the L1 digest commits to. Exposed for tests (the SDK publishes a connectionId vector). */
export function phantomAgent(hash: Uint8Array, isMainnet: boolean): { source: string; connectionId: string } {
  return { source: isMainnet ? 'a' : 'b', connectionId: `0x${bytesToHex(hash)}` };
}

export async function signL1Action(
  sign: SignFn,
  action: unknown,
  nonce: number,
  opts: L1SignOptions,
): Promise<RsvSignature> {
  const agent = phantomAgent(actionHash(action, nonce, opts.vaultAddress, opts.expiresAfter), opts.isMainnet);
  const digest = typedDataDigest(
    { name: 'Exchange', version: '1', chainId: 1337, verifyingContract: ZERO_ADDRESS },
    'Agent',
    AGENT_FIELDS,
    agent,
  );
  return await sign(digest);
}

export const APPROVE_AGENT_FIELDS: Eip712Field[] = [
  { name: 'hyperliquidChain', type: 'string' },
  { name: 'agentAddress', type: 'address' },
  { name: 'agentName', type: 'string' },
  { name: 'nonce', type: 'uint64' },
];

export const USD_SEND_FIELDS: Eip712Field[] = [
  { name: 'hyperliquidChain', type: 'string' },
  { name: 'destination', type: 'string' },
  { name: 'amount', type: 'string' },
  { name: 'time', type: 'uint64' },
];

export const WITHDRAW_FIELDS: Eip712Field[] = [
  { name: 'hyperliquidChain', type: 'string' },
  { name: 'destination', type: 'string' },
  { name: 'amount', type: 'string' },
  { name: 'time', type: 'uint64' },
];

/**
 * Sign a user-signed action. `message` must already carry `hyperliquidChain`
 * ("Mainnet"/"Testnet") — only the listed fields are hashed; extra keys on the
 * POSTed action (type, signatureChainId) are domain/server concerns.
 */
export async function signUserSignedAction(
  sign: SignFn,
  message: Record<string, unknown>,
  fields: Eip712Field[],
  primaryType: string,
  signatureChainId: string = HYPERLIQUID_SIGNATURE_CHAIN_ID,
): Promise<RsvSignature> {
  const digest = typedDataDigest(
    {
      name: 'HyperliquidSignTransaction',
      version: '1',
      chainId: Number.parseInt(signatureChainId, 16),
      verifyingContract: ZERO_ADDRESS,
    },
    primaryType,
    fields,
    message,
  );
  return await sign(digest);
}
