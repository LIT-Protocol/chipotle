import { task } from "hardhat/config";
import { ethers } from "ethers";

// Minimal ABI: the event we scan plus the migration entry points.
const DIAMOND_ABI = [
  "event WalletDerivationRegistered(uint256 indexed apiKeyHash, address indexed pkpId, uint256 derivationPath)",
  "function backfillPkpOwners(address[] pkpIds, uint256[] masterHashes)",
  "function getPkpOwnerMaster(address pkpId) view returns (uint256)",
];

interface FirstRegistration {
  pkpId: string;
  masterHash: bigint;
  blockNumber: number;
  txHash: string;
  conflicts: bigint[]; // other masters that later registered the same pkpId
}

/**
 * One-time migration for issue #575: wallets registered before the global
 * `pkpIdToOwnerMaster` binding existed have no owner entry, so any account
 * could still claim them via registerWalletDerivation. This task rebuilds the
 * binding from history: it scans every WalletDerivationRegistered event, takes
 * the FIRST registration per pkpId (the same rule the contract now enforces),
 * and submits backfillPkpOwners in batches. Already-bound pkpIds are skipped
 * on-chain, so the task is idempotent and safe to re-run until it reports
 * nothing left to bind.
 */
task(
  "backfill-pkp-owners",
  "Backfill pkpIdToOwnerMaster for wallets registered before the #575 fix"
)
  .addParam("diamond", "Diamond proxy contract address")
  .addOptionalParam("fromBlock", "Block to start scanning events from", "0")
  .addOptionalParam("chunkSize", "getLogs block range per request", "10000")
  .addOptionalParam("batchSize", "pkpIds per backfill transaction", "200")
  .addFlag("execute", "Send the backfill transactions (default is dry-run)")
  .setAction(async (taskArgs, hre) => {
    const { diamond: diamondAddress } = taskArgs;
    const fromBlock = parseInt(taskArgs.fromBlock, 10);
    const chunkSize = parseInt(taskArgs.chunkSize, 10);
    const batchSize = parseInt(taskArgs.batchSize, 10);

    const rpcUrl =
      (hre.network.config as { url?: string }).url || "https://mainnet.base.org";
    const provider = new ethers.JsonRpcProvider(rpcUrl);
    const readOnly = new ethers.Contract(diamondAddress, DIAMOND_ABI, provider);

    console.log(`Network: ${hre.network.name}`);
    console.log(`Diamond: ${diamondAddress}`);

    // 1. Scan all WalletDerivationRegistered events. The event's first indexed
    //    arg is the master apiKeyHash (WritesFacet emits masterHash, never a
    //    usage-key hash), so it is exactly the value pkpIdToOwnerMaster needs.
    const latestBlock = await provider.getBlockNumber();
    console.log(`Scanning events from block ${fromBlock} to ${latestBlock}...`);

    const filter = readOnly.filters.WalletDerivationRegistered();
    const firstByPkp = new Map<string, FirstRegistration>();
    let totalEvents = 0;

    for (let start = fromBlock; start <= latestBlock; start += chunkSize) {
      const end = Math.min(start + chunkSize - 1, latestBlock);
      const logs = await readOnly.queryFilter(filter, start, end);
      totalEvents += logs.length;
      // queryFilter returns logs in chain order (block, then log index), so the
      // first occurrence per pkpId within and across chunks is the first ever.
      for (const log of logs) {
        const parsed = log as ethers.EventLog;
        const masterHash = parsed.args[0] as bigint;
        const pkpId = (parsed.args[1] as string).toLowerCase();
        const existing = firstByPkp.get(pkpId);
        if (!existing) {
          firstByPkp.set(pkpId, {
            pkpId: parsed.args[1] as string,
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
      if (end < latestBlock) {
        process.stdout.write(
          `\r  scanned up to block ${end} (${totalEvents} events, ${firstByPkp.size} wallets)`
        );
      }
    }
    console.log(
      `\nFound ${totalEvents} registration events across ${firstByPkp.size} distinct pkpIds.`
    );

    // 2. Surface pkpIds registered by more than one master account. First
    //    registration wins (matching the contract rule), but each conflict is a
    //    wallet another account also claimed pre-fix — review them manually,
    //    since the later registrant could sign with the first owner's key
    //    until this backfill lands.
    const conflicted = [...firstByPkp.values()].filter(
      (r) => r.conflicts.length > 0
    );
    if (conflicted.length > 0) {
      console.log(
        `\n⚠️  ${conflicted.length} pkpId(s) were registered by MULTIPLE master accounts (possible pre-fix hijack):`
      );
      for (const r of conflicted) {
        console.log(
          `  ${r.pkpId} first=0x${r.masterHash.toString(16)} (block ${r.blockNumber}, ${r.txHash})`
        );
        for (const other of r.conflicts) {
          console.log(`    also registered by 0x${other.toString(16)}`);
        }
      }
      console.log(
        "  First registration wins in this backfill; investigate the later registrants."
      );
    }

    // 3. Drop pkpIds that are already bound (post-fix registrations, or a
    //    previous run of this task).
    console.log("\nChecking current on-chain bindings...");
    const toBind: FirstRegistration[] = [];
    for (const r of firstByPkp.values()) {
      const owner: bigint = await readOnly.getPkpOwnerMaster(r.pkpId);
      if (owner === 0n) {
        toBind.push(r);
      } else if (owner !== r.masterHash) {
        console.log(
          `  ⚠️  ${r.pkpId} already bound to 0x${owner.toString(16)} which is NOT its first registrant 0x${r.masterHash.toString(16)} — investigate`
        );
      }
    }
    console.log(`${toBind.length} pkpId(s) need backfilling.`);
    if (toBind.length === 0) {
      console.log("Nothing to do.");
      return;
    }

    if (!taskArgs.execute) {
      console.log("\nDry run (pass --execute to send transactions):");
      for (const r of toBind) {
        console.log(`  ${r.pkpId} -> 0x${r.masterHash.toString(16)}`);
      }
      return;
    }

    // 4. Send backfillPkpOwners in batches. Caller must be the diamond owner
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
      const tx = await diamond.backfillPkpOwners(
        batch.map((r) => r.pkpId),
        batch.map((r) => r.masterHash)
      );
      console.log(
        `  batch ${i / batchSize + 1} (${batch.length} pkpIds): ${tx.hash}`
      );
      const receipt = await tx.wait();
      console.log(`    confirmed in block ${receipt.blockNumber}`);
    }

    // 5. Verify every pair landed.
    console.log("\nVerifying...");
    let failures = 0;
    for (const r of toBind) {
      const owner: bigint = await readOnly.getPkpOwnerMaster(r.pkpId);
      if (owner !== r.masterHash) {
        failures++;
        console.log(
          `  ❌ ${r.pkpId}: expected 0x${r.masterHash.toString(16)}, got 0x${owner.toString(16)}`
        );
      }
    }
    if (failures > 0) {
      throw new Error(`${failures} binding(s) failed verification`);
    }
    console.log(`All ${toBind.length} bindings verified. Backfill complete.`);
  });
