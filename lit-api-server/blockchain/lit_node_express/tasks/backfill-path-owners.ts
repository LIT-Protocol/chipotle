import { task } from "hardhat/config";
import { ethers } from "ethers";
import * as fs from "fs";
import * as path from "path";
import { withRetry } from "./rpc-retry";

// Minimal ABI: the event we scan, the storage views used to reconstruct
// ownership for registrations that never emitted an event, and the migration
// entry points.
const DIAMOND_ABI = [
  "error InvalidRequest(string message)",
  "event WalletDerivationRegistered(uint256 indexed apiKeyHash, address indexed pkpId, uint256 derivationPath)",
  "function backfillPathOwners(uint256[] derivationPaths, uint256[] masterHashes)",
  "function getPathOwnerMaster(uint256 derivationPath) view returns (uint256)",
  "function getPkpOwnerMaster(address pkpId) view returns (uint256)",
  "function getWalletDerivation(uint256 apiKeyHash, address walletAddress) view returns (uint256)",
  "function pkpCount() view returns (uint256)",
  "function allPkpIdsAt(uint256 index) view returns (address)",
];

const diamondIface = new ethers.Interface(DIAMOND_ABI);

interface FirstRegistration {
  derivationPath: bigint;
  masterHash: bigint;
  pkpId: string;
  // Block/tx of the first WalletDerivationRegistered event, or 0 / "storage"
  // when the binding was reconstructed from diamond storage only.
  blockNumber: number;
  txHash: string;
  source: "event" | "storage" | "both";
  // Other master accounts that later registered the same derivationPath
  // (under the same or a different pkpId label). Either way the later account
  // can drive the node onto this path's key, so both are conflicts here.
  conflicts: { masterHash: bigint; pkpId: string; via: "event" | "storage" }[];
}

/** Run `fn` over `items` with at most `limit` in flight. */
async function mapLimit<T, R>(
  items: T[],
  limit: number,
  fn: (item: T, index: number) => Promise<R>
): Promise<R[]> {
  const out: R[] = new Array(items.length);
  let next = 0;
  async function worker() {
    for (;;) {
      const i = next++;
      if (i >= items.length) return;
      out[i] = await fn(items[i], i);
    }
  }
  await Promise.all(Array.from({ length: Math.min(limit, items.length) }, worker));
  return out;
}

/** Safe Transaction Builder JSON (same shape the #575 Safe backfill used). */
function buildSafeBatch(
  chainId: bigint,
  diamondAddress: string,
  name: string,
  description: string,
  calls: string[]
) {
  return {
    version: "1.0",
    chainId: chainId.toString(),
    createdAt: Date.now(),
    meta: { name, description, txBuilderVersion: "1.16.5" },
    transactions: calls.map((data) => ({
      to: ethers.getAddress(diamondAddress),
      value: "0",
      data,
      contractMethod: null,
      contractInputsValues: null,
    })),
  };
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
 * Ownership is rebuilt from TWO sources and cross-checked:
 *  1. Event history: every WalletDerivationRegistered event, FIRST registration
 *     per derivationPath (the rule the contract now enforces).
 *  2. Diamond storage: every pkpId in the global list -> its #575 owner
 *     (getPkpOwnerMaster) -> that account's derivation path
 *     (getWalletDerivation). This is the authoritative pkpData the node reads,
 *     and it covers wallets migrated into the diamond without an event — the
 *     #575 event-only backfill missed 436 such PKPs on prod.
 * Disagreements between the two are reported as conflicts.
 *
 * Output modes:
 *  - default: dry run, prints the plan.
 *  - --execute: send backfillPathOwners from CONFIG_OPERATOR_PRIVATE_KEY /
 *    OWNER_PRIVATE_KEY (EOA owner or config operator, e.g. `next`).
 *  - --safe-out <dir>: write Safe Transaction Builder JSON batches for a
 *    Safe-owned diamond (`prod`). Import each file in the Safe UI, execute,
 *    then re-run this task (dry run) until it reports nothing left.
 * Already-bound paths are skipped on-chain, so every mode is idempotent.
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
  .addOptionalParam("concurrency", "Parallel RPC reads during storage reconstruction", "8")
  .addOptionalParam(
    "safeOut",
    "Write Safe Transaction Builder JSON batches to this directory instead of sending (for a Safe-owned diamond)"
  )
  .addOptionalParam(
    "callsPerFile",
    "backfillPathOwners calls bundled per Safe JSON file (--safe-out only)",
    "2"
  )
  .addFlag("eventsOnly", "Skip the diamond-storage reconstruction (events only)")
  .addFlag("execute", "Send the backfill transactions (default is dry-run)")
  .addFlag(
    "allowConflicts",
    "Proceed even if paths were registered by multiple master accounts (pre-fix hijacks/aliases). Off by default: conflicts are a hard stop under --execute / --safe-out."
  )
  .setAction(async (taskArgs, hre) => {
    const { diamond: diamondAddress } = taskArgs;
    const fromBlock = Number(taskArgs.fromBlock);
    const chunkSize = Number(taskArgs.chunkSize);
    const batchSize = Number(taskArgs.batchSize);
    const confirmations = Number(taskArgs.confirmations);
    const concurrency = Number(taskArgs.concurrency);
    const callsPerFile = Number(taskArgs.callsPerFile);
    const safeOut: string | undefined = taskArgs.safeOut;
    for (const [name, value, minimum] of [
      ["fromBlock", fromBlock, 0], ["chunkSize", chunkSize, 1],
      ["batchSize", batchSize, 1], ["confirmations", confirmations, 0],
      ["concurrency", concurrency, 1], ["callsPerFile", callsPerFile, 1],
    ] as const) {
      if (!Number.isSafeInteger(value) || value < minimum) {
        throw new Error(`${name} must be an integer >= ${minimum}`);
      }
    }
    if (taskArgs.execute && safeOut) {
      throw new Error("--execute and --safe-out are mutually exclusive");
    }
    // Leave room for Safe/MultiSend overhead under the 2^24 transaction gas cap.
    if (safeOut && batchSize * callsPerFile > 400) {
      throw new Error("Safe files must contain at most 400 paths; lower batchSize or callsPerFile");
    }

    const rpcUrl =
      (hre.network.config as { url?: string }).url || "https://mainnet.base.org";
    const provider = new ethers.JsonRpcProvider(rpcUrl);
    const network = await provider.getNetwork();
    if (hre.network.config.chainId === undefined ||
        network.chainId !== BigInt(hre.network.config.chainId)) {
      throw new Error(`RPC chain ID ${network.chainId} does not match configured network`);
    }
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
    const lastByWallet = new Map<string, bigint>();
    for (const log of allLogs) {
      const masterHash = log.args[0] as bigint;
      const pkpId = (log.args[1] as string).toLowerCase();
      const derivationPath = log.args[2] as bigint;
      lastByWallet.set(`${masterHash}:${pkpId}`, derivationPath);
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
          source: "event",
          conflicts: [],
        });
      } else if (
        existing.masterHash !== masterHash &&
        !existing.conflicts.some(
          (c) => c.masterHash === masterHash && c.pkpId === pkpId
        )
      ) {
        existing.conflicts.push({ masterHash, pkpId, via: "event" });
      }
    }
    console.log(
      `\nFound ${allLogs.length} registration events across ${firstByPath.size} distinct derivationPaths.`
    );

    // 2. Reconstruct from diamond storage. For every pkpId the diamond knows,
    //    its #575 owner (pkpIdToOwnerMaster, populated by backfill-pkp-owners /
    //    registration) tells us which account's pkpData row is authoritative,
    //    and getWalletDerivation on that account yields the path. This is what
    //    the node actually reads, so it is the ground truth for wallets that
    //    were migrated into the diamond without a WalletDerivationRegistered
    //    event. Cross-check against the event view: a path whose storage owner
    //    differs from its first event registrant is a conflict.
    let storageOnly = 0;
    let storageAgree = 0;
    let unboundPkps = 0;
    let unresolved = 0;
    let blockedAliases = 0;
    if (!taskArgs.eventsOnly) {
      const snapshot = { blockTag: latestBlock };
      const total = Number(await withRetry("pkpCount", () => readOnly.pkpCount(snapshot)));
      console.log(`\nReconstructing from storage: ${total} pkpIds on the diamond...`);
      // registerWalletDerivation increments pkpCount BEFORE storing the ID.
      const indices = Array.from({ length: total }, (_, i) => i + 1);
      let done = 0;
      const rows = await mapLimit(indices, concurrency, async (i) => {
        const pkpId: string = (
          await withRetry(`allPkpIdsAt ${i}`, () => readOnly.allPkpIdsAt(i, snapshot))
        ).toLowerCase();
        const owner: bigint = await withRetry(`getPkpOwnerMaster ${pkpId}`, () =>
          readOnly.getPkpOwnerMaster(pkpId, snapshot)
        );
        let derivationPath = 0n;
        let error: string | undefined;
        let pathOwnershipDenied = false;
        if (owner !== 0n) {
          try {
            derivationPath = await withRetry(`getWalletDerivation ${pkpId}`, () =>
              readOnly.getWalletDerivation(owner, pkpId, snapshot)
            );
          } catch (err) {
            error = (err as Error).message.slice(0, 120);
            const data = (err as { data?: string }).data;
            if (data) {
              try {
                const decoded = diamondIface.parseError(data);
                pathOwnershipDenied = decoded?.name === "InvalidRequest" &&
                  decoded.args[0] === "derivation path owned by another account";
              } catch { /* Unknown errors remain unresolved. */ }
            }
          }
        }
        done++;
        if (done % 250 === 0 || done === total) {
          process.stdout.write(`\r  resolved ${done}/${total} pkpIds`);
        }
        return { pkpId, owner, derivationPath, error, pathOwnershipDenied };
      });
      console.log("");
      for (const r of rows) {
        if (r.owner === 0n) {
          unboundPkps++;
          continue;
        }
        if (r.error) {
          // A completed backfill intentionally makes historical aliases revert.
          // Accept only that precise contract error, corroborated by this
          // wallet's last event and the canonical path binding at the snapshot.
          const lastPath = lastByWallet.get(`${r.owner}:${r.pkpId}`);
          const first = lastPath === undefined ? undefined : firstByPath.get(lastPath.toString());
          if (r.pathOwnershipDenied && first && first.masterHash !== r.owner) {
            const bound = await withRetry("verify blocked alias", () =>
              readOnly.getPathOwnerMaster(first.derivationPath, snapshot) as Promise<bigint>
            );
            if (bound === first.masterHash) {
              blockedAliases++;
              continue;
            }
          }
          unresolved++;
          console.log(
            `  ⚠️  pkpId ${r.pkpId} (owner 0x${r.owner.toString(16)}): getWalletDerivation reverted — ${r.error}`
          );
          continue;
        }
        if (r.derivationPath === 0n) continue; // derivation removed; nothing to bind
        const key = r.derivationPath.toString();
        const existing = firstByPath.get(key);
        if (!existing) {
          storageOnly++;
          firstByPath.set(key, {
            derivationPath: r.derivationPath,
            masterHash: r.owner,
            pkpId: r.pkpId,
            blockNumber: 0,
            txHash: "storage",
            source: "storage",
            conflicts: [],
          });
        } else if (existing.masterHash === r.owner) {
          storageAgree++;
          existing.source = "both";
        } else if (
          !existing.conflicts.some(
            (c) => c.masterHash === r.owner && c.pkpId === r.pkpId
          )
        ) {
          existing.conflicts.push({ masterHash: r.owner, pkpId: r.pkpId, via: "storage" });
        }
      }
      console.log(
        `  storage agrees with events on ${storageAgree} path(s); ${storageOnly} path(s) exist ONLY in storage (no event); ` +
          `${unboundPkps} pkpId(s) have no #575 owner binding; ${unresolved} could not be resolved; ` +
          `${blockedAliases} historical alias(es) correctly blocked by path ownership.`
      );
      if (unboundPkps > 0) {
        console.log(
          "  ⚠️  pkpIds without a pkpIdToOwnerMaster binding cannot be attributed from storage — run backfill-pkp-owners first, or rely on their events."
        );
      }
    }

    // 3. Surface paths registered by more than one master account. Each is a
    //    key another account also claimed pre-fix — a probable hijack (same
    //    pkpId label) or alias (different label). The backfill binds the FIRST
    //    registrant and the hardened getWalletDerivation then refuses to serve
    //    the later registrant. But a conflict still means: (a) verify the first
    //    registrant is genuinely the rightful owner (an attacker who registered
    //    BEFORE the victim would be bound as owner here), and (b) the later
    //    account's pkpData row should be removed. Hard stop under --execute /
    //    --safe-out unless the operator has reviewed them and passes
    //    --allow-conflicts.
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
          )} pkpId=${r.pkpId} (${r.source === "storage" ? "storage" : `block ${r.blockNumber}, ${r.txHash}`})`
        );
        for (const c of r.conflicts) {
          const kind = c.pkpId === r.pkpId ? "same pkpId (label hijack)" : `pkpId=${c.pkpId} (alias)`;
          console.log(
            `    also registered by master 0x${c.masterHash.toString(16)} — ${kind} [via ${c.via}]`
          );
        }
      }
      console.log(
        "  Backfill binds the FIRST registrant; the later registrant's stale pkpData row must be removed separately."
      );
      if ((taskArgs.execute || safeOut) && !taskArgs.allowConflicts) {
        throw new Error(
          `Refusing to proceed with ${conflicted.length} unresolved conflict(s). Review them, remediate the later registrants, then re-run with --allow-conflicts.`
        );
      }
    }

    // 4. Drop paths that are already bound (post-upgrade registrations, or a
    //    previous run of this task).
    console.log("\nChecking current on-chain bindings...");
    const toBind: FirstRegistration[] = [];
    let alreadyBound = 0;
    let incorrectlyBound = 0;
    const candidates = [...firstByPath.values()];
    const owners = await mapLimit(candidates, concurrency, (r) =>
      withRetry("getPathOwnerMaster", () =>
        readOnly.getPathOwnerMaster(r.derivationPath) as Promise<bigint>
      )
    );
    candidates.forEach((r, i) => {
      const owner = owners[i];
      if (owner === 0n) {
        toBind.push(r);
      } else if (owner !== r.masterHash) {
        incorrectlyBound++;
        console.log(
          `  ⚠️  path 0x${r.derivationPath.toString(
            16
          )} already bound to 0x${owner.toString(
            16
          )} which is NOT its first registrant 0x${r.masterHash.toString(
            16
          )} — a post-upgrade claim of an unbound historical path; investigate`
        );
      } else {
        alreadyBound++;
      }
    });
    console.log(
      `${toBind.length} derivationPath(s) need backfilling (${alreadyBound} already bound correctly).`
    );
    if (incorrectlyBound > 0 || unboundPkps > 0 || unresolved > 0) {
      throw new Error(
        `Incomplete ownership verification: ${incorrectlyBound} incorrectly bound path(s), ` +
        `${unboundPkps} PKP(s) without owners, ${unresolved} unresolved PKP(s). Investigate before proceeding.`
      );
    }
    if (toBind.length === 0) {
      console.log("Nothing to do.");
      return;
    }

    const batches: FirstRegistration[][] = [];
    for (let i = 0; i < toBind.length; i += batchSize) {
      batches.push(toBind.slice(i, i + batchSize));
    }
    const encodeBatch = (batch: FirstRegistration[]) =>
      diamondIface.encodeFunctionData("backfillPathOwners", [
        batch.map((r) => r.derivationPath),
        batch.map((r) => r.masterHash),
      ]);

    // 5a. Safe mode: write Transaction Builder JSON and stop. Idempotent on
    //     chain, so files may be executed in any order and re-generated later.
    if (safeOut) {
      const chainId = (await provider.getNetwork()).chainId;
      fs.mkdirSync(safeOut, { recursive: true });
      const files: string[] = [];
      for (let f = 0; f * callsPerFile < batches.length; f++) {
        const slice = batches.slice(f * callsPerFile, (f + 1) * callsPerFile);
        const pathCount = slice.reduce((n, b) => n + b.length, 0);
        const json = buildSafeBatch(
          chainId,
          diamondAddress,
          `backfillPathOwners ${String(f + 1).padStart(2, "0")}`,
          `#690 pathToOwnerMaster backfill on ${diamondAddress}: ${slice.length} call(s), ${pathCount} derivationPath(s). Idempotent; already-bound paths are skipped.`,
          slice.map(encodeBatch)
        );
        const file = path.join(safeOut, `safe-backfill-paths-${String(f + 1).padStart(2, "0")}.json`);
        fs.writeFileSync(file, JSON.stringify(json, null, 2));
        files.push(file);
        console.log(
          `  wrote ${file}: ${slice.length} call(s), ${pathCount} path(s), ~${(50_000 + pathCount * 26_500).toLocaleString()} gas`
        );
      }
      console.log(
        `\nWrote ${files.length} Safe Transaction Builder file(s). Import each in the Safe UI (Transaction Builder → Load batch), execute, then re-run this task without --safe-out to verify 0 remaining.`
      );
      return;
    }

    if (!taskArgs.execute) {
      console.log("\nDry run (pass --execute to send, or --safe-out <dir> for Safe JSON):");
      for (const r of toBind) {
        console.log(
          `  path 0x${r.derivationPath.toString(16)} -> 0x${r.masterHash.toString(16)} [${r.source}]`
        );
      }
      return;
    }

    // 5b. Send backfillPathOwners in batches. Caller must be the diamond owner
    //     or config operator.
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

    for (let i = 0; i < batches.length; i++) {
      const batch = batches[i];
      const tx = await diamond.backfillPathOwners(
        batch.map((r) => r.derivationPath),
        batch.map((r) => r.masterHash)
      );
      console.log(`  batch ${i + 1}/${batches.length} (${batch.length} paths): ${tx.hash}`);
      const receipt = await tx.wait();
      console.log(`    confirmed in block ${receipt.blockNumber}`);
    }

    // 6. Verify every pair landed.
    console.log("\nVerifying...");
    let failures = 0;
    const after = await mapLimit(toBind, concurrency, (r) =>
      withRetry("getPathOwnerMaster", () =>
        readOnly.getPathOwnerMaster(r.derivationPath) as Promise<bigint>
      )
    );
    toBind.forEach((r, i) => {
      if (after[i] !== r.masterHash) {
        failures++;
        console.log(
          `  ❌ path 0x${r.derivationPath.toString(
            16
          )}: expected 0x${r.masterHash.toString(16)}, got 0x${after[i].toString(16)}`
        );
      }
    });
    if (failures > 0) {
      throw new Error(`${failures} binding(s) failed verification`);
    }
    console.log(`All ${toBind.length} bindings verified. Backfill complete.`);
  });
