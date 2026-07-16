// Shared logic for the #575 PKP owner backfill: reconstruct the first-owner
// binding from on-chain history, build Safe Transaction Builder batches, and
// decode/verify those batches back against the chain. Used by both
// `pkp-backfill:gen-safe` and `pkp-backfill:verify-safe` so the generator and
// verifier agree on the source-of-truth rule (first WalletDerivationRegistered
// event per pkpId wins — the exact rule registerWalletDerivation now enforces).

import { ethers } from "ethers";

export const DIAMOND_ABI = [
  "event WalletDerivationRegistered(uint256 indexed apiKeyHash, address indexed pkpId, uint256 derivationPath)",
  "function backfillPkpOwners(address[] pkpIds, uint256[] masterHashes)",
  "function getPkpOwnerMaster(address pkpId) view returns (uint256)",
  "function allPkpIdsAt(uint256 index) view returns (address)",
  "function pkpCount() view returns (uint256)",
  "function accountCount() view returns (uint256)",
  "function indexToAccountHashAt(uint256 index) view returns (uint256)",
  "function listPkps(uint256 apiKeyHash, uint256 pageNumber, uint256 pageSize) view returns (tuple(uint256 id, address pkpId, string name, string description)[])",
];

export const diamondIface = new ethers.Interface(DIAMOND_ABI);
export const BACKFILL_SELECTOR =
  diamondIface.getFunction("backfillPkpOwners")!.selector;

// Safe MultiSend / MultiSendCallOnly. Only needed to decode a bundled tx if the
// operator hands us a raw multiSend blob instead of the Transaction Builder JSON.
export const MULTISEND_ABI = ["function multiSend(bytes transactions)"];
export const multiSendIface = new ethers.Interface(MULTISEND_ABI);
export const MULTISEND_SELECTOR =
  multiSendIface.getFunction("multiSend")!.selector;

export interface FirstOwner {
  pkpId: string; // checksummed address
  masterHash: bigint;
  blockNumber: number;
  txHash: string;
  conflicts: bigint[]; // other masters that later registered the same pkpId
}

export interface ScanResult {
  firstByPkp: Map<string, FirstOwner>; // key = lowercase pkpId
  conflicted: FirstOwner[];
  scannedTo: number;
  totalEvents: number;
}

/** Resolve the upper scan bound: a finalized head (or N confirmations behind). */
export async function resolveScanHead(
  provider: ethers.Provider,
  confirmations: number
): Promise<number> {
  if (confirmations > 0) {
    return (await provider.getBlockNumber()) - confirmations;
  }
  const finalized = await provider.getBlock("finalized");
  if (!finalized) {
    throw new Error(
      "Provider does not support the 'finalized' block tag; pass --confirmations N instead"
    );
  }
  return finalized.number;
}

/**
 * Scan WalletDerivationRegistered events and reconstruct the first owner per
 * pkpId. Logs are collected across chunks then sorted by
 * (blockNumber, transactionIndex, logIndex) so "first registration wins" is
 * enforced regardless of provider ordering.
 */
export async function scanFirstOwners(
  provider: ethers.Provider,
  diamondAddress: string,
  opts: {
    fromBlock: number;
    toBlock: number;
    chunkSize: number;
    log?: (msg: string) => void;
  }
): Promise<ScanResult> {
  const { fromBlock, toBlock, chunkSize } = opts;
  const readOnly = new ethers.Contract(diamondAddress, DIAMOND_ABI, provider);
  const filter = readOnly.filters.WalletDerivationRegistered();

  const allLogs: ethers.EventLog[] = [];
  for (let start = fromBlock; start <= toBlock; start += chunkSize) {
    const end = Math.min(start + chunkSize - 1, toBlock);
    const logs = await readOnly.queryFilter(filter, start, end);
    for (const log of logs) allLogs.push(log as ethers.EventLog);
    if (opts.log && end < toBlock) {
      opts.log(`  scanned up to block ${end} (${allLogs.length} events)`);
    }
  }

  allLogs.sort(
    (a, b) =>
      a.blockNumber - b.blockNumber ||
      a.transactionIndex - b.transactionIndex ||
      a.index - b.index
  );

  const firstByPkp = new Map<string, FirstOwner>();
  for (const log of allLogs) {
    const masterHash = log.args[0] as bigint;
    const pkpChecksummed = ethers.getAddress(log.args[1] as string);
    const key = pkpChecksummed.toLowerCase();
    const existing = firstByPkp.get(key);
    if (!existing) {
      firstByPkp.set(key, {
        pkpId: pkpChecksummed,
        masterHash,
        blockNumber: log.blockNumber,
        txHash: log.transactionHash,
        conflicts: [],
      });
    } else if (
      existing.masterHash !== masterHash &&
      !existing.conflicts.includes(masterHash)
    ) {
      existing.conflicts.push(masterHash);
    }
  }

  const conflicted = [...firstByPkp.values()].filter(
    (r) => r.conflicts.length > 0
  );
  return { firstByPkp, conflicted, scannedTo: toBlock, totalEvents: allLogs.length };
}

export interface OwnedPkp {
  pkpId: string; // checksummed
  masters: bigint[]; // account master hashes whose pkpData contains this pkpId
}

export interface OwnershipResult {
  byPkp: Map<string, OwnedPkp>; // key = lowercase pkpId
  conflicted: OwnedPkp[]; // held by >1 account (on-chain hijack evidence)
  accountCount: number;
  pkpCount: number;
  totalRows: number;
}

/**
 * Authoritative ownership: enumerate every account and read its pkpData via
 * listPkps. This is the exact mapping the node reads to resolve a signing key,
 * and unlike WalletDerivationRegistered events it covers PKPs migrated into
 * storage without emitting an event. A pkpId held by more than one account is a
 * hijack that already landed on-chain.
 */
export async function buildOwnershipFromAccounts(
  provider: ethers.Provider,
  diamondAddress: string,
  opts: { concurrency?: number; pageSize?: number; log?: (m: string) => void } = {}
): Promise<OwnershipResult> {
  const concurrency = opts.concurrency ?? 20;
  const pageSize = opts.pageSize ?? 500;
  const c = new ethers.Contract(diamondAddress, DIAMOND_ABI, provider);
  const accountCount = Number(await c.accountCount());
  const pkpCount = Number(await c.pkpCount());
  const idx = Array.from({ length: accountCount }, (_, i) => i + 1);
  const hashes = (await mapLimit(idx, concurrency, (i) =>
    c.indexToAccountHashAt(i)
  )) as bigint[];

  const byPkp = new Map<string, OwnedPkp>();
  let totalRows = 0;
  let done = 0;
  await mapLimit(hashes, concurrency, async (h) => {
    let page = 0;
    while (true) {
      const rows = await c.listPkps(h, page, pageSize);
      for (const r of rows) {
        const pkpChecksummed = ethers.getAddress(r.pkpId as string);
        const key = pkpChecksummed.toLowerCase();
        let entry = byPkp.get(key);
        if (!entry) {
          entry = { pkpId: pkpChecksummed, masters: [] };
          byPkp.set(key, entry);
        }
        if (!entry.masters.includes(h)) entry.masters.push(h);
        totalRows++;
      }
      if (rows.length < pageSize) break;
      page++;
    }
    if (opts.log) opts.log(`  accounts read: ${++done}/${accountCount}`);
  });

  const conflicted = [...byPkp.values()].filter((o) => o.masters.length > 1);
  return { byPkp, conflicted, accountCount, pkpCount, totalRows };
}

export function chunk<T>(arr: T[], size: number): T[][] {
  const out: T[][] = [];
  for (let i = 0; i < arr.length; i += size) out.push(arr.slice(i, i + size));
  return out;
}

export interface Pair {
  pkpId: string;
  masterHash: bigint;
}

/** ABI-encode a single backfillPkpOwners(address[],uint256[]) call. */
export function encodeBackfillCall(pairs: Pair[]): string {
  return diamondIface.encodeFunctionData("backfillPkpOwners", [
    pairs.map((p) => p.pkpId),
    pairs.map((p) => p.masterHash),
  ]);
}

export interface SafeTx {
  to: string;
  value: string;
  data: string;
  contractMethod: null;
  contractInputsValues: null;
}

export interface SafeBatch {
  version: string;
  chainId: string;
  createdAt: number;
  meta: { name: string; description: string; txBuilderVersion: string };
  transactions: SafeTx[];
}

export function buildSafeTx(diamondAddress: string, data: string): SafeTx {
  return {
    to: ethers.getAddress(diamondAddress),
    value: "0",
    data,
    contractMethod: null,
    contractInputsValues: null,
  };
}

export function buildSafeBatch(
  chainId: number | bigint,
  createdAt: number,
  name: string,
  description: string,
  transactions: SafeTx[]
): SafeBatch {
  return {
    version: "1.0",
    chainId: chainId.toString(),
    createdAt,
    meta: {
      name,
      description,
      txBuilderVersion: "1.16.5",
    },
    transactions,
  };
}

/**
 * Rough gas estimate for a backfillPkpOwners call: one cold zero->nonzero
 * SSTORE (~22.1k) + event (~1.9k) per pair, plus per-pair calldata (~2 words *
 * ~40 gas amortized) and a fixed base. Deliberately conservative so operators
 * don't under-size a Safe tx.
 */
export function estimateBackfillGas(pairCount: number): number {
  return 50_000 + pairCount * 26_500;
}

/** Decode Safe MultiSend packed bytes into individual inner calls. */
export function decodeMultiSend(
  multiSendData: string
): { to: string; value: bigint; data: string }[] {
  const [packed] = multiSendIface.decodeFunctionData(
    "multiSend",
    multiSendData
  );
  const bytes = ethers.getBytes(packed);
  const calls: { to: string; value: bigint; data: string }[] = [];
  let i = 0;
  while (i < bytes.length) {
    // operation(1) + to(20) + value(32) + dataLength(32) + data(dataLength)
    i += 1; // skip operation
    const to = ethers.getAddress(ethers.hexlify(bytes.slice(i, i + 20)));
    i += 20;
    const value = BigInt(ethers.hexlify(bytes.slice(i, i + 32)));
    i += 32;
    const len = Number(BigInt(ethers.hexlify(bytes.slice(i, i + 32))));
    i += 32;
    const data = ethers.hexlify(bytes.slice(i, i + len));
    i += len;
    calls.push({ to, value, data });
  }
  return calls;
}

/**
 * Decode a list of Safe transactions (Transaction Builder `transactions[]`, or
 * raw {to,data} calls) into backfill pairs. Handles direct backfillPkpOwners
 * calls and MultiSend-wrapped bundles. Returns pairs plus the set of target
 * addresses seen (so the caller can assert everything targets the diamond).
 */
export function decodeTransactionsToPairs(
  txs: { to: string; data: string }[]
): { pairs: Pair[]; targets: Set<string> } {
  const pairs: Pair[] = [];
  const targets = new Set<string>();

  const handleCall = (to: string, data: string) => {
    const selector = data.slice(0, 10).toLowerCase();
    if (selector === MULTISEND_SELECTOR.toLowerCase()) {
      for (const inner of decodeMultiSend(data)) {
        handleCall(inner.to, inner.data);
      }
      return;
    }
    if (selector !== BACKFILL_SELECTOR.toLowerCase()) {
      throw new Error(
        `Unexpected function selector ${selector} for target ${to} (expected backfillPkpOwners ${BACKFILL_SELECTOR} or multiSend)`
      );
    }
    targets.add(ethers.getAddress(to).toLowerCase());
    const [pkpIds, masterHashes] = diamondIface.decodeFunctionData(
      "backfillPkpOwners",
      data
    );
    if (pkpIds.length !== masterHashes.length) {
      throw new Error(
        `backfillPkpOwners call has mismatched array lengths (${pkpIds.length} vs ${masterHashes.length})`
      );
    }
    for (let k = 0; k < pkpIds.length; k++) {
      pairs.push({
        pkpId: ethers.getAddress(pkpIds[k] as string),
        masterHash: masterHashes[k] as bigint,
      });
    }
  };

  for (const tx of txs) handleCall(tx.to, tx.data);
  return { pairs, targets };
}

/** Simple concurrency-limited map for on-chain reads. */
export async function mapLimit<T, R>(
  items: T[],
  limit: number,
  fn: (item: T, index: number) => Promise<R>
): Promise<R[]> {
  const results: R[] = new Array(items.length);
  let next = 0;
  async function worker() {
    while (true) {
      const i = next++;
      if (i >= items.length) return;
      results[i] = await fn(items[i], i);
    }
  }
  await Promise.all(
    Array.from({ length: Math.min(limit, items.length) }, () => worker())
  );
  return results;
}
