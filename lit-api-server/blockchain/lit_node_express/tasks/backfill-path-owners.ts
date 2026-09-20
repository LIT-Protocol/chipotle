import { task } from "hardhat/config";
import { ethers } from "ethers";

// Minimal ABI: the event we scan plus the migration entry points.
const DIAMOND_ABI = [
  "event WalletDerivationRegistered(uint256 indexed apiKeyHash, address indexed pkpId, uint256 derivationPath)",
  "function backfillPathOwners(uint256[] derivationPaths, uint256[] masterHashes)",
  "function getPathOwnerMaster(uint256 derivationPath) view returns (uint256)",
];

// Public Base RPCs cap eth_getLogs ranges (2,000 blocks on mainnet.base.org)
// and rate-limit bursts. Retry transient failures with backoff so a long scan
// does not die halfway; range-cap errors are not transient and surface at once
// with a hint to lower --chunk-size.
async function withRetry<T>(label: string, fn: () => Promise<T>, attempts = 6): Promise<T> {
  let delay = 500;
  for (let i = 1; ; i++) {
    try {
      return await fn();
    } catch (err) {
      const msg = (err as Error).message || String(err);
      if (/limited to a [\d,]+ range|block range/i.test(msg)) {
        throw new Error(`${label}: ${msg} — lower --chunk-size`);
      }
      if (i >= attempts) throw err;
      process.stderr.write(`\n  ${label} failed (${msg.slice(0, 80)}); retry ${i}/${attempts - 1} in ${delay}ms`);
      await new Promise((r) => setTimeout(r, delay));
      delay = Math.min(delay * 2, 10_000);
    }
  }
}

interface FirstRegistration {
  derivationPath: bigint;
  masterHash: bigint;
  pkpId: string;
  blockNumber: number;
  txHash: string;
  // Other master accounts that later registered the same derivationPath
  // (under the same or a different pkpId label). Either way the later account
  // can drive the node onto this path's key, so both are conflicts here.
  conflicts: { masterHash: bigint; pkpId: string }[];
}

/**
 * One-time migration for the derivation-path aliasing fix (#690).
 *
 * A PKP private key is a stateless function of its derivationPath, and paths
 * are public (emitted by WalletDerivationRegistered). The #575 fix bound
 * ownership on pkpId only, so an attacker could register a FRESH self-owned
 * pkpId carrying a victim's path. The upgraded diamond adds pathToOwnerMaster:
 * a path's first registrant owns it, enforced in registerWalletDerivation and
 * at resolve time in getWalletDerivation.
 *
 * Paths registered before the upgrade have no binding. Until they are
 * backfilled, ANY account can claim an unbound historical path — and because
 * backfillPathOwners never overwrites an existing binding, such a claim cannot
 * be repaired by this task. Run this immediately after the diamondCut (same
 * maintenance window), dry-run first, and treat reported conflicts as
 * incidents to investigate before --execute.
 *
 * This task rebuilds the binding from history: it scans every
 * WalletDerivationRegistered event, takes the FIRST registration per
 * derivationPath (the same rule the contract now enforces), and submits
 * backfillPathOwners in batches. Already-bound paths are skipped on-chain, so
 * the task is idempotent and safe to re-run until it reports nothing left.
 *
 * Companion tasks: `backfill-pkp-owners` (the #575 pkpId binding) and
 * `scan-path-aliases` (read-only detector for cross-pkpId path aliasing).
 */
task(
  "backfill-path-owners",
  "Backfill pathToOwnerMaster for derivation paths registered before the #690 path-aliasing fix"
)
  .addParam("diamond", "Diamond proxy contract address")
  .addOptionalParam("fromBlock", "Block to start scanning events from", "0")
  .addOptionalParam("chunkSize", "getLogs block range per request", "10000")
  .addOptionalParam("batchSize", "derivationPaths per backfill transaction", "200")
  .addOptionalParam(
    "confirmations",
    "Blocks to stay behind chain head to avoid reorgs (0 = use the 'finalized' tag)",
    "0"
  )
  .addFlag("execute", "Send the backfill transactions (default is dry-run)")
  .addFlag(
    "allowConflicts",
    "Proceed even if paths were registered by multiple master accounts (pre-fix hijacks/aliases). Off by default: conflicts are a hard stop under --execute."
  )
  .setAction(async (taskArgs, hre) => {
    const { diamond: diamondAddress } = taskArgs;
    const fromBlock = parseInt(taskArgs.fromBlock, 10);
    const chunkSize = parseInt(taskArgs.chunkSize, 10);
    const batchSize = parseInt(taskArgs.batchSize, 10);
    const confirmations = parseInt(taskArgs.confirmations, 10);

    const rpcUrl =
      (hre.network.config as { url?: string }).url || "https://mainnet.base.org";
    const provider = new ethers.JsonRpcProvider(rpcUrl);
    const readOnly = new ethers.Contract(diamondAddress, DIAMOND_ABI, provider);

    console.log(`Network: ${hre.network.name}`);
    console.log(`Diamond: ${diamondAddress}`);

    // Fail fast if the diamond hasn't been upgraded yet: getPathOwnerMaster is
    // only present after the #690 diamondCut, and backfillPathOwners would
    // revert with an unrecognised selector.
    try {
      await readOnly.getPathOwnerMaster(1n);
    } catch (err) {
      throw new Error(
        `getPathOwnerMaster is not callable on ${diamondAddress} — run the contract upgrade (diamondCut) first. (${
          (err as Error).message
        })`
      );
    }

    // 1. Scan all WalletDerivationRegistered events. The first indexed arg is
    //    the master apiKeyHash (WritesFacet emits masterHash, never a usage-key
    //    hash), so it is exactly the value pathToOwnerMaster needs. Scan to a
    //    finalized/confirmed head, not `latest`: a reorg that reranks the first
    //    registrant would otherwise bind the wrong owner.
    let latestBlock: number;
    if (confirmations > 0) {
      latestBlock = (await provider.getBlockNumber()) - confirmations;
    } else {
      const finalized = await provider.getBlock("finalized");
      if (!finalized) {
        throw new Error(
          "Provider does not support the 'finalized' block tag; pass --confirmations N instead"
        );
      }
      latestBlock = finalized.number;
    }
    if (latestBlock < fromBlock) {
      console.log("No finalized blocks in range yet. Nothing to do.");
      return;
    }
    console.log(
      `Scanning events from block ${fromBlock} to ${latestBlock} (finalized head)...`
    );

    const filter = readOnly.filters.WalletDerivationRegistered();
    const allLogs: ethers.EventLog[] = [];
    for (let start = fromBlock; start <= latestBlock; start += chunkSize) {
      const end = Math.min(start + chunkSize - 1, latestBlock);
      const logs = await withRetry(`getLogs ${start}-${end}`, () =>
        readOnly.queryFilter(filter, start, end)
      );
      for (const log of logs) allLogs.push(log as ethers.EventLog);
      if (end < latestBlock) {
        process.stdout.write(
          `\r  scanned up to block ${end} (${allLogs.length} events)`
        );
      }
    }

    // Chain order so "first registration wins" is unambiguous; getLogs
    // ordering is not guaranteed across or within chunks.
    allLogs.sort(
      (a, b) =>
        a.blockNumber - b.blockNumber ||
        a.transactionIndex - b.transactionIndex ||
        a.index - b.index
    );

    const firstByPath = new Map<string, FirstRegistration>();
    for (const log of allLogs) {
      const masterHash = log.args[0] as bigint;
      const pkpId = (log.args[1] as string).toLowerCase();
      const derivationPath = log.args[2] as bigint;
      if (derivationPath === 0n) continue; // contract rejects path 0; nothing to bind
      const key = derivationPath.toString();
      const existing = firstByPath.get(key);
      if (!existing) {
        firstByPath.set(key, {
          derivationPath,
          masterHash,
          pkpId,
          blockNumber: log.blockNumber,
          txHash: log.transactionHash,
          conflicts: [],
        });
      } else if (
        existing.masterHash !== masterHash &&
        !existing.conflicts.some(
          (c) => c.masterHash === masterHash && c.pkpId === pkpId
        )
      ) {
        existing.conflicts.push({ masterHash, pkpId });
      }
    }
    console.log(
      `\nFound ${allLogs.length} registration events across ${firstByPath.size} distinct derivationPaths.`
    );

    // 2. Surface paths registered by more than one master account. Each is a
    //    key another account also claimed pre-fix — a probable hijack (same
    //    pkpId label) or alias (different label). The backfill binds the FIRST
    //    registrant and the hardened getWalletDerivation then refuses to serve
    //    the later registrant. But a conflict still means: (a) verify the first
    //    registrant is genuinely the rightful owner (an attacker who registered
    //    BEFORE the victim would be bound as owner here), and (b) the later
    //    account's pkpData row should be removed. Hard stop under --execute
    //    unless the operator has reviewed them and passes --allow-conflicts.
    const conflicted = [...firstByPath.values()].filter(
      (r) => r.conflicts.length > 0
    );
    if (conflicted.length > 0) {
      console.log(
        `\n⚠️  ${conflicted.length} derivationPath(s) were registered by MULTIPLE master accounts (probable pre-fix hijack/alias):`
      );
      for (const r of conflicted) {
        console.log(
          `  path 0x${r.derivationPath.toString(16)} first master=0x${r.masterHash.toString(
            16
          )} pkpId=${r.pkpId} (block ${r.blockNumber}, ${r.txHash})`
        );
        for (const c of r.conflicts) {
          const kind = c.pkpId === r.pkpId ? "same pkpId (label hijack)" : `pkpId=${c.pkpId} (alias)`;
          console.log(
            `    also registered by master 0x${c.masterHash.toString(16)} — ${kind}`
          );
        }
      }
      console.log(
        "  Backfill binds the FIRST registrant; the later registrant's stale pkpData row must be removed separately."
      );
      if (taskArgs.execute && !taskArgs.allowConflicts) {
        throw new Error(
          `Refusing to --execute with ${conflicted.length} unresolved conflict(s). Review them, remediate the later registrants, then re-run with --allow-conflicts.`
        );
      }
    }

    // 3. Drop paths that are already bound (post-upgrade registrations, or a
    //    previous run of this task).
    console.log("\nChecking current on-chain bindings...");
    const toBind: FirstRegistration[] = [];
    for (const r of firstByPath.values()) {
      const owner: bigint = await withRetry("getPathOwnerMaster", () =>
        readOnly.getPathOwnerMaster(r.derivationPath)
      );
      if (owner === 0n) {
        toBind.push(r);
      } else if (owner !== r.masterHash) {
        console.log(
          `  ⚠️  path 0x${r.derivationPath.toString(
            16
          )} already bound to 0x${owner.toString(
            16
          )} which is NOT its first registrant 0x${r.masterHash.toString(
            16
          )} — a post-upgrade claim of an unbound historical path; investigate`
        );
      }
    }
    console.log(`${toBind.length} derivationPath(s) need backfilling.`);
    if (toBind.length === 0) {
      console.log("Nothing to do.");
      return;
    }

    if (!taskArgs.execute) {
      console.log("\nDry run (pass --execute to send transactions):");
      for (const r of toBind) {
        console.log(
          `  path 0x${r.derivationPath.toString(16)} -> 0x${r.masterHash.toString(16)}`
        );
      }
      return;
    }

    // 4. Send backfillPathOwners in batches. Caller must be the diamond owner
    //    or config operator.
    const signerKey =
      process.env.CONFIG_OPERATOR_PRIVATE_KEY || process.env.OWNER_PRIVATE_KEY;
    if (!signerKey) {
      throw new Error(
        "CONFIG_OPERATOR_PRIVATE_KEY or OWNER_PRIVATE_KEY environment variable is required with --execute"
      );
    }
    const wallet = new ethers.Wallet(signerKey, provider);
    const diamond = new ethers.Contract(diamondAddress, DIAMOND_ABI, wallet);
    console.log(`\nSending backfill as ${wallet.address}...`);

    for (let i = 0; i < toBind.length; i += batchSize) {
      const batch = toBind.slice(i, i + batchSize);
      const tx = await diamond.backfillPathOwners(
        batch.map((r) => r.derivationPath),
        batch.map((r) => r.masterHash)
      );
      console.log(
        `  batch ${i / batchSize + 1} (${batch.length} paths): ${tx.hash}`
      );
      const receipt = await tx.wait();
      console.log(`    confirmed in block ${receipt.blockNumber}`);
    }

    // 5. Verify every pair landed.
    console.log("\nVerifying...");
    let failures = 0;
    for (const r of toBind) {
      const owner: bigint = await readOnly.getPathOwnerMaster(r.derivationPath);
      if (owner !== r.masterHash) {
        failures++;
        console.log(
          `  ❌ path 0x${r.derivationPath.toString(
            16
          )}: expected 0x${r.masterHash.toString(16)}, got 0x${owner.toString(16)}`
        );
      }
    }
    if (failures > 0) {
      throw new Error(`${failures} binding(s) failed verification`);
    }
    console.log(`All ${toBind.length} bindings verified. Backfill complete.`);
  });
