import { task } from "hardhat/config";
import { ethers } from "ethers";
import {
  DIAMOND_ABI,
  buildOwnershipFromAccounts,
  chunk,
  estimateBackfillGas,
  mapLimit,
  Pair,
} from "./lib/pkp-owners";

/**
 * One-time migration for issue #575: PKPs registered before the global
 * `pkpIdToOwnerMaster` binding existed have no owner entry, so any account could
 * still claim them via registerWalletDerivation.
 *
 * Ownership is reconstructed from account enumeration (listPkps) rather than
 * WalletDerivationRegistered events. Events miss PKPs that were migrated into
 * this diamond's storage without emitting (there are ~hundreds), whereas
 * listPkps reads the exact pkpData mapping the node uses to sign. A pkpId held
 * by more than one account is an on-chain hijack and is excluded unless
 * --allow-conflicts.
 *
 * backfillPkpOwners skips already-bound pkpIds on-chain, so this is idempotent
 * and safe to re-run until it reports nothing left to bind.
 *
 * NOTE: this path signs with a raw key (config-operator or owner). If the admin
 * is a Safe, use `pkp-backfill:gen-safe` / `pkp-backfill:verify-safe` instead.
 */
task(
  "backfill-pkp-owners",
  "Backfill pkpIdToOwnerMaster for PKPs registered before the #575 fix"
)
  .addParam("diamond", "Diamond proxy contract address")
  .addOptionalParam("batchSize", "pkpIds per backfill transaction", "1000")
  .addFlag("execute", "Send the backfill transactions (default is dry-run)")
  .addFlag(
    "allowConflicts",
    "Include pkpIds held by multiple accounts (on-chain hijacks). Off by default: a hard stop under --execute."
  )
  .setAction(async (taskArgs, hre) => {
    const diamondAddress = ethers.getAddress(taskArgs.diamond);
    const batchSize = parseInt(taskArgs.batchSize, 10);

    const rpcUrl =
      (hre.network.config as { url?: string }).url || "https://mainnet.base.org";
    const provider = new ethers.JsonRpcProvider(rpcUrl);

    console.log(`Network: ${hre.network.name}`);
    console.log(`Diamond: ${diamondAddress}`);

    // 1. Authoritative ownership from account enumeration.
    console.log("Enumerating accounts + pkpData...");
    const own = await buildOwnershipFromAccounts(provider, diamondAddress, {
      log: (m) => process.stdout.write("\r" + m),
    });
    console.log(
      `\naccounts=${own.accountCount} pkpCount=${own.pkpCount} distinct=${own.byPkp.size} conflicts=${own.conflicted.length}`
    );
    if (own.byPkp.size !== own.pkpCount) {
      console.log(
        `⚠️  distinct owned (${own.byPkp.size}) != pkpCount (${own.pkpCount}). Investigate before proceeding.`
      );
    }

    if (own.conflicted.length > 0) {
      console.log(
        `\n⚠️  ${own.conflicted.length} pkpId(s) held by MULTIPLE accounts (on-chain hijack):`
      );
      for (const o of own.conflicted.slice(0, 20)) {
        console.log(
          `  ${o.pkpId}: ${o.masters.map((m) => "0x" + m.toString(16)).join(", ")}`
        );
      }
      if (taskArgs.execute && !taskArgs.allowConflicts) {
        throw new Error(
          `Refusing to --execute with ${own.conflicted.length} conflict(s). Review, remediate, then re-run with --allow-conflicts.`
        );
      }
    }

    // 2. Single-owner pkpIds are unambiguous; multi-owner excluded (or handled
    //    manually) — conflict disambiguation via events belongs in gen-safe.
    const candidates: Pair[] = [...own.byPkp.values()]
      .filter((o) => o.masters.length === 1)
      .map((o) => ({ pkpId: o.pkpId, masterHash: o.masters[0] }));

    // 3. Drop already-bound pkpIds.
    console.log("Checking current on-chain bindings...");
    const readOnly = new ethers.Contract(diamondAddress, DIAMOND_ABI, provider);
    const owners = (await mapLimit(candidates, 25, (r) =>
      readOnly.getPkpOwnerMaster(r.pkpId)
    )) as bigint[];
    const toBind: Pair[] = [];
    candidates.forEach((r, i) => {
      const o = owners[i];
      if (o === 0n) toBind.push(r);
      else if (o !== r.masterHash)
        console.log(
          `  ⚠️  ${r.pkpId} bound to 0x${o.toString(16)}, expected 0x${r.masterHash.toString(16)}`
        );
    });
    console.log(`${toBind.length} pkpId(s) need backfilling.`);
    if (toBind.length === 0) {
      console.log("Nothing to do.");
      return;
    }

    if (!taskArgs.execute) {
      console.log("\nDry run (pass --execute to send transactions):");
      for (const r of toBind.slice(0, 10))
        console.log(`  ${r.pkpId} -> 0x${r.masterHash.toString(16)}`);
      if (toBind.length > 10) console.log(`  ... and ${toBind.length - 10} more`);
      return;
    }

    // 4. Send backfillPkpOwners in batches (config-operator or owner key).
    const signerKey =
      process.env.CONFIG_OPERATOR_PRIVATE_KEY || process.env.OWNER_PRIVATE_KEY;
    if (!signerKey) {
      throw new Error(
        "CONFIG_OPERATOR_PRIVATE_KEY or OWNER_PRIVATE_KEY required with --execute"
      );
    }
    const wallet = new ethers.Wallet(signerKey, provider);
    const diamond = new ethers.Contract(diamondAddress, DIAMOND_ABI, wallet);
    console.log(`\nSending backfill as ${wallet.address}...`);

    const batches = chunk(toBind, batchSize);
    for (let i = 0; i < batches.length; i++) {
      const batch = batches[i];
      // Pass an explicit gasLimit instead of relying on eth_estimateGas: some
      // providers (e.g. Alchemy) cap estimateGas simulation well below the
      // block limit (~13-20M here), so a >~500-pair batch makes estimateGas
      // fail with "missing revert data" even though the tx itself is valid.
      // estimateBackfillGas is ~26.5k/pair; add 25% headroom.
      const gasLimit = BigInt(
        Math.ceil(estimateBackfillGas(batch.length) * 1.25)
      );
      const tx = await diamond.backfillPkpOwners(
        batch.map((r) => r.pkpId),
        batch.map((r) => r.masterHash),
        { gasLimit }
      );
      console.log(
        `  batch ${i + 1}/${batches.length} (${batch.length} pkpIds, gasLimit ${(Number(gasLimit) / 1e6).toFixed(1)}M): ${tx.hash}`
      );
      const receipt = await tx.wait();
      console.log(`    confirmed in block ${receipt.blockNumber} (gas used ${receipt.gasUsed})`);
    }

    // 5. Verify.
    console.log("\nVerifying...");
    const after = (await mapLimit(toBind, 25, (r) =>
      readOnly.getPkpOwnerMaster(r.pkpId)
    )) as bigint[];
    let failures = 0;
    toBind.forEach((r, i) => {
      if (after[i] !== r.masterHash) {
        failures++;
        console.log(`  ❌ ${r.pkpId}: expected 0x${r.masterHash.toString(16)}, got 0x${after[i].toString(16)}`);
      }
    });
    if (failures > 0) throw new Error(`${failures} binding(s) failed verification`);
    console.log(`All ${toBind.length} bindings verified. Backfill complete.`);
  });
