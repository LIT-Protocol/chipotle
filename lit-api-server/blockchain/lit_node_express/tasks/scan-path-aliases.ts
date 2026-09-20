import { task } from "hardhat/config";
import { ethers } from "ethers";
import { withRetry } from "./rpc-retry";

// Minimal ABI: the registration event plus the owner-binding getters used to
// annotate findings. getPathOwnerMaster only exists on diamonds upgraded with
// the path-aliasing fix, so it is called defensively.
const DIAMOND_ABI = [
  "event WalletDerivationRegistered(uint256 indexed apiKeyHash, address indexed pkpId, uint256 derivationPath)",
  "function getPkpOwnerMaster(address pkpId) view returns (uint256)",
  "function getPathOwnerMaster(uint256 derivationPath) view returns (uint256)",
];

interface Registration {
  pkpId: string;
  masterHash: bigint;
  derivationPath: bigint;
  blockNumber: number;
  txHash: string;
}

/**
 * Detect PKP path-aliasing across account boundaries.
 *
 * A PKP private key is a stateless function of its derivationPath only, and
 * derivationPaths are public (emitted by WalletDerivationRegistered). Every
 * legitimate mint generates a FRESH random path whose address becomes the pkpId,
 * so each path should appear under exactly one pkpId. If the same derivationPath
 * is registered under two DIFFERENT pkpIds — or under two different master
 * accounts — then someone registered an observed path under a new label to
 * resolve the node onto another wallet's key. That is the exact attack this
 * scan surfaces, using only event history (no key material / TEE required).
 *
 * The companion `backfill-pkp-owners` task groups by pkpId and so catches the
 * #575 same-label hijack; this task groups by derivationPath and catches the
 * distinct-label alias that pkpId grouping misses.
 *
 * NOTE: This on-chain scan is the detector for the attack's signature. The
 * AUTHORITATIVE check — address(secp256k1(get_client_key(path))) == pkpId for
 * every registered wallet — requires the TEE key oracle and must run on a node
 * (mirrors `core::get_verified_client_key`). This task needs neither keys nor a
 * signer; it is a safe read-only forensic pass.
 */
task(
  "scan-path-aliases",
  "Scan WalletDerivationRegistered history for cross-account derivationPath aliasing"
)
  .addParam("diamond", "Diamond proxy contract address")
  .addOptionalParam("fromBlock", "Block to start scanning events from", "0")
  .addOptionalParam("chunkSize", "getLogs block range per request", "10000")
  .addOptionalParam(
    "confirmations",
    "Blocks to stay behind chain head to avoid reorgs (0 = use the 'finalized' tag)",
    "0"
  )
  .setAction(async (taskArgs, hre) => {
    const { diamond: diamondAddress } = taskArgs;
    const fromBlock = parseInt(taskArgs.fromBlock, 10);
    const chunkSize = parseInt(taskArgs.chunkSize, 10);
    const confirmations = parseInt(taskArgs.confirmations, 10);

    const rpcUrl =
      (hre.network.config as { url?: string }).url || "https://mainnet.base.org";
    const provider = new ethers.JsonRpcProvider(rpcUrl);
    const readOnly = new ethers.Contract(diamondAddress, DIAMOND_ABI, provider);

    console.log(`Network: ${hre.network.name}`);
    console.log(`Diamond: ${diamondAddress}`);

    // Fix the upper bound to a finalized/confirmed head so a reorg can't reorder
    // "first registration wins" (mirrors backfill-pkp-owners).
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

    // Chain order so the earliest registration of each path is unambiguous.
    allLogs.sort(
      (a, b) =>
        a.blockNumber - b.blockNumber ||
        a.transactionIndex - b.transactionIndex ||
        a.index - b.index
    );

    // Group every registration by derivationPath.
    const byPath = new Map<string, Registration[]>();
    for (const log of allLogs) {
      const reg: Registration = {
        masterHash: log.args[0] as bigint,
        pkpId: (log.args[1] as string).toLowerCase(),
        derivationPath: log.args[2] as bigint,
        blockNumber: log.blockNumber,
        txHash: log.transactionHash,
      };
      const key = reg.derivationPath.toString();
      const list = byPath.get(key);
      if (list) list.push(reg);
      else byPath.set(key, [reg]);
    }
    console.log(
      `\nFound ${allLogs.length} registration events across ${byPath.size} distinct derivationPaths.`
    );

    // A path is aliased if it was registered under more than one distinct pkpId.
    // (Same pkpId re-registered by the same master is a benign recovery/re-add
    // after removeWalletDerivation; same pkpId under different masters is a
    // separate #575 label hijack the backfill task already reports.)
    const aliased: { path: bigint; regs: Registration[] }[] = [];
    for (const [key, regs] of byPath) {
      const distinctPkps = new Set(regs.map((r) => r.pkpId));
      if (distinctPkps.size > 1) {
        aliased.push({ path: BigInt(key), regs });
      }
    }

    if (aliased.length === 0) {
      console.log(
        "\n✅ No derivationPath was registered under more than one pkpId. No path aliasing detected."
      );
      return;
    }

    console.log(
      `\n🚨 ${aliased.length} derivationPath(s) registered under MULTIPLE distinct pkpIds — cross-account key aliasing:`
    );
    for (const { path, regs } of aliased) {
      // Earliest registrant is the presumed rightful owner; every later distinct
      // pkpId is an alias pointing at the same underlying key.
      const [first, ...rest] = regs;
      console.log(`\n  derivationPath 0x${path.toString(16)}`);
      console.log(
        `    first   pkpId=${first.pkpId} master=0x${first.masterHash.toString(
          16
        )} (block ${first.blockNumber}, ${first.txHash})`
      );
      const seen = new Set([first.pkpId]);
      for (const r of rest) {
        if (seen.has(r.pkpId)) continue; // ignore benign same-pkpId re-registers
        seen.add(r.pkpId);
        console.log(
          `    ALIAS   pkpId=${r.pkpId} master=0x${r.masterHash.toString(
            16
          )} (block ${r.blockNumber}, ${r.txHash})`
        );
      }

      // Best-effort: annotate with the current on-chain owner binding. Wrapped
      // in try/catch so the scan still works against a diamond not yet upgraded
      // with getPathOwnerMaster.
      try {
        const pathOwner: bigint = await readOnly.getPathOwnerMaster(path);
        console.log(
          `    current pathToOwnerMaster = 0x${pathOwner.toString(16)}`
        );
      } catch {
        console.log(
          "    (getPathOwnerMaster unavailable — diamond not yet upgraded with the path-aliasing fix)"
        );
      }
    }

    console.log(
      "\nEach ALIAS line is an account that registered a pkpId label aliased to another\n" +
        "wallet's derivation path. Remediate: remove the alias registrant's pkpData row and\n" +
        "confirm the earliest registrant is the genuine owner (an attacker who registered\n" +
        "BEFORE the victim would appear as 'first'). Then run backfill-path-owners."
    );
    // Non-zero exit so this can gate an ops/CI check.
    process.exitCode = 1;
  });
