import { task } from "hardhat/config";
import { ethers } from "ethers";
import * as fs from "fs";
import * as path from "path";
import {
  DIAMOND_ABI,
  buildOwnershipFromAccounts,
  buildSafeBatch,
  buildSafeTx,
  chunk,
  decodeTransactionsToPairs,
  encodeBackfillCall,
  estimateBackfillGas,
  mapLimit,
  OwnedPkp,
  Pair,
  resolveScanHead,
  scanFirstOwners,
} from "./lib/pkp-owners";

const DEFAULT_DIAMOND = "0xaAaAA9120fE271F653cfDb6bf400dB93D2DEa7Aa";
const BASE_CHAIN_ID = 8453n;

/**
 * Resolve a Base RPC URL. Priority: explicit --rpc-url, then BASE_RPC_URL, then
 * ALCHEMY_API_KEY (env or .context/.env), then the public endpoint (rate-limited
 * for a full account enumeration).
 */
function resolveRpcUrl(explicit: string | undefined, repoRoot: string): string {
  if (explicit) return explicit;
  if (process.env.BASE_RPC_URL) return process.env.BASE_RPC_URL;
  let alchemy = process.env.ALCHEMY_API_KEY;
  if (!alchemy) {
    const envPath = path.join(repoRoot, ".context", ".env");
    if (fs.existsSync(envPath)) {
      const m = fs
        .readFileSync(envPath, "utf8")
        .match(/^\s*ALCHEMY_API_KEY\s*=\s*(.+?)\s*$/m);
      if (m) alchemy = m[1].trim();
    }
  }
  if (alchemy) return `https://base-mainnet.g.alchemy.com/v2/${alchemy}`;
  return "https://mainnet.base.org";
}

function findRepoRoot(): string {
  let dir = process.cwd();
  for (let i = 0; i < 8; i++) {
    if (fs.existsSync(path.join(dir, ".context"))) return dir;
    const parent = path.dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  return process.cwd();
}

/**
 * Resolve one master per pkpId. Single-owner pkpIds are unambiguous. Multi-owner
 * pkpIds are on-chain hijacks: pick the first WalletDerivationRegistered
 * registrant among the holders (the rule the contract enforces). Returns the
 * chosen pairs plus the conflicts that could not be resolved.
 */
async function resolveOwners(
  provider: ethers.Provider,
  diamond: string,
  owned: OwnedPkp[],
  allowConflicts: boolean,
  fromBlock: number,
  chunkSize: number,
  toBlock: number
): Promise<{ pairs: Pair[]; unresolved: OwnedPkp[] }> {
  const single = owned.filter((o) => o.masters.length === 1);
  const multi = owned.filter((o) => o.masters.length > 1);
  const pairs: Pair[] = single.map((o) => ({
    pkpId: o.pkpId,
    masterHash: o.masters[0],
  }));
  if (multi.length === 0 || !allowConflicts) {
    return { pairs, unresolved: multi };
  }
  // Disambiguate conflicts via events.
  const scan = await scanFirstOwners(provider, diamond, {
    fromBlock,
    toBlock,
    chunkSize,
  });
  const unresolved: OwnedPkp[] = [];
  for (const o of multi) {
    const first = scan.firstByPkp.get(o.pkpId.toLowerCase());
    if (first && o.masters.some((m) => m === first.masterHash)) {
      pairs.push({ pkpId: o.pkpId, masterHash: first.masterHash });
    } else {
      unresolved.push(o); // no event, or first registrant no longer holds it
    }
  }
  return { pairs, unresolved };
}

task(
  "pkp-backfill:gen-safe",
  "Generate Safe Transaction Builder JSON to backfill pkpIdToOwnerMaster (#575)"
)
  .addOptionalParam("diamond", "Diamond proxy address", DEFAULT_DIAMOND)
  .addOptionalParam("rpcUrl", "Base RPC URL (else BASE_RPC_URL / ALCHEMY_API_KEY)")
  .addOptionalParam("batchSize", "pkpIds per backfillPkpOwners call", "1000")
  .addOptionalParam(
    "callsPerFile",
    "backfillPkpOwners calls bundled into one Safe tx / file",
    "2"
  )
  .addOptionalParam("outDir", "Directory to write Safe JSON files", ".context/pkp-backfill")
  .addOptionalParam("fromBlock", "Event scan start (conflict disambiguation only)", "0")
  .addOptionalParam("chunkSize", "getLogs block range (conflict disambiguation)", "20000")
  .addFlag(
    "allowConflicts",
    "Resolve multi-account pkpIds via first-registrant events instead of excluding them"
  )
  .setAction(async (args, hre) => {
    const repoRoot = findRepoRoot();
    const provider = new ethers.JsonRpcProvider(
      resolveRpcUrl(args.rpcUrl, repoRoot)
    );
    const diamond = ethers.getAddress(args.diamond);
    const batchSize = parseInt(args.batchSize, 10);
    const callsPerFile = parseInt(args.callsPerFile, 10);

    const net = await provider.getNetwork();
    console.log(`RPC chainId: ${net.chainId}  diamond: ${diamond}`);
    if (net.chainId !== BASE_CHAIN_ID) {
      console.log(`⚠️  Not Base mainnet (${BASE_CHAIN_ID}). Continuing anyway.`);
    }

    // Authoritative ownership from account enumeration (covers migrated PKPs).
    console.log("Enumerating accounts + pkpData (authoritative ownership)...");
    const own = await buildOwnershipFromAccounts(provider, diamond, {
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
      console.log(`\n⚠️  ${own.conflicted.length} pkpId(s) held by MULTIPLE accounts (on-chain hijack):`);
      for (const o of own.conflicted.slice(0, 20)) {
        console.log(`  ${o.pkpId}: ${o.masters.map((m) => "0x" + m.toString(16)).join(", ")}`);
      }
      if (!args.allowConflicts)
        console.log("  Excluded from batch. Investigate; re-run with --allow-conflicts to bind first-registrant.");
    }

    const toBlock = await resolveScanHead(provider, 0);
    const { pairs: resolved, unresolved } = await resolveOwners(
      provider,
      diamond,
      [...own.byPkp.values()],
      args.allowConflicts,
      parseInt(args.fromBlock, 10),
      parseInt(args.chunkSize, 10),
      toBlock
    );
    if (unresolved.length > 0) {
      console.log(`⚠️  ${unresolved.length} conflict(s) could not be auto-resolved and are excluded.`);
    }

    // Drop already-bound pkpIds (post-fix registrations or a prior run).
    console.log("Checking current on-chain bindings...");
    const readOnly = new ethers.Contract(diamond, DIAMOND_ABI, provider);
    const owners = (await mapLimit(resolved, 25, (r) =>
      readOnly.getPkpOwnerMaster(r.pkpId)
    )) as bigint[];
    const toBind: Pair[] = [];
    let alreadyCorrect = 0;
    let boundWrong = 0;
    resolved.forEach((r, i) => {
      const o = owners[i];
      if (o === 0n) toBind.push(r);
      else if (o === r.masterHash) alreadyCorrect++;
      else {
        boundWrong++;
        console.log(`  ⚠️  ${r.pkpId} bound to 0x${o.toString(16)}, expected 0x${r.masterHash.toString(16)}`);
      }
    });
    console.log(`${toBind.length} to bind, ${alreadyCorrect} already correct, ${boundWrong} bound elsewhere.`);
    if (toBind.length === 0) {
      console.log("Nothing to generate.");
      return;
    }

    // Batch into calls, bundle calls into per-file Safe transactions.
    const calls = chunk(toBind, batchSize);
    const files = chunk(calls, callsPerFile);
    const outDir = path.isAbsolute(args.outDir)
      ? args.outDir
      : path.join(repoRoot, args.outDir);
    fs.mkdirSync(outDir, { recursive: true });

    const createdAt = Date.now();
    const manifest: any = {
      diamond,
      chainId: net.chainId.toString(),
      scannedToBlock: toBlock,
      accountCount: own.accountCount,
      pkpCount: own.pkpCount,
      totalToBind: toBind.length,
      batchSize,
      callsPerFile,
      files: [] as any[],
    };

    files.forEach((fileCalls, fi) => {
      const txs = fileCalls.map((pairs) =>
        buildSafeTx(diamond, encodeBackfillCall(pairs))
      );
      const pairsInFile = fileCalls.reduce((n, c) => n + c.length, 0);
      const gas = fileCalls.reduce((g, c) => g + estimateBackfillGas(c.length), 0);
      const batch = buildSafeBatch(
        net.chainId,
        createdAt,
        `PKP owner backfill (#575) part ${fi + 1}/${files.length}`,
        `${pairsInFile} pkpIds in ${txs.length} call(s). Diamond ${diamond}.`,
        txs
      );
      const fname = `safe-backfill-${String(fi + 1).padStart(2, "0")}.json`;
      fs.writeFileSync(path.join(outDir, fname), JSON.stringify(batch, null, 2));
      manifest.files.push({ file: fname, calls: txs.length, pkpIds: pairsInFile, estGas: gas });
      console.log(`  ${fname}: ${txs.length} call(s), ${pairsInFile} pkpIds, ~${(gas / 1e6).toFixed(1)}M gas`);
    });

    fs.writeFileSync(path.join(outDir, "manifest.json"), JSON.stringify(manifest, null, 2));
    console.log(`\nWrote ${files.length} Safe batch file(s) + manifest.json to ${outDir}`);
    console.log(`Verify before signing:\n  npx hardhat pkp-backfill:verify-safe --dir ${args.outDir}`);
  });

task(
  "pkp-backfill:verify-safe",
  "Verify Safe backfill JSON against authoritative on-chain ownership (#575)"
)
  .addParam("dir", "Directory of safe-backfill-*.json files (or a single --file)")
  .addOptionalParam("file", "Verify a single JSON file instead of a directory")
  .addOptionalParam("diamond", "Diamond proxy address", DEFAULT_DIAMOND)
  .addOptionalParam("rpcUrl", "Base RPC URL (else BASE_RPC_URL / ALCHEMY_API_KEY)")
  .addFlag("allowConflicts", "Do not fail if conflicted (multi-account) pkpIds are included")
  .setAction(async (args, hre) => {
    const repoRoot = findRepoRoot();
    const provider = new ethers.JsonRpcProvider(
      resolveRpcUrl(args.rpcUrl, repoRoot)
    );
    const diamond = ethers.getAddress(args.diamond);
    const net = await provider.getNetwork();

    // 1. Load + decode every Safe transaction into (pkpId -> master) pairs.
    const resolveIn = (p: string) =>
      path.isAbsolute(p) ? p : path.join(repoRoot, p);
    let jsonFiles: string[];
    if (args.file) {
      jsonFiles = [resolveIn(args.file)];
    } else {
      const dir = resolveIn(args.dir);
      jsonFiles = fs
        .readdirSync(dir)
        .filter((f) => f.startsWith("safe-backfill-") && f.endsWith(".json"))
        .sort()
        .map((f) => path.join(dir, f));
    }
    if (jsonFiles.length === 0) throw new Error("No safe-backfill-*.json found");
    console.log(`Verifying ${jsonFiles.length} file(s).`);

    const errors: string[] = [];
    const jsonPairs = new Map<string, bigint>();
    let totalCalls = 0;
    for (const f of jsonFiles) {
      const batch = JSON.parse(fs.readFileSync(f, "utf8"));
      if (batch.chainId && batch.chainId !== net.chainId.toString()) {
        errors.push(`${path.basename(f)}: chainId ${batch.chainId} != RPC ${net.chainId}`);
      }
      const txs = (batch.transactions || []).map((t: any) => ({ to: t.to, data: t.data }));
      totalCalls += txs.length;
      const { pairs, targets } = decodeTransactionsToPairs(txs);
      for (const t of targets)
        if (t !== diamond.toLowerCase())
          errors.push(`${path.basename(f)}: call targets ${t}, not diamond ${diamond}`);
      for (const p of pairs) {
        const key = p.pkpId.toLowerCase();
        if (p.masterHash === 0n) errors.push(`${p.pkpId}: masterHash 0`);
        const prev = jsonPairs.get(key);
        if (prev !== undefined && prev !== p.masterHash)
          errors.push(`${p.pkpId}: two different masters in JSON`);
        jsonPairs.set(key, p.masterHash);
      }
    }
    console.log(`Decoded ${jsonPairs.size} distinct bindings across ${totalCalls} call(s).`);

    // 2. Authoritative ownership from account enumeration.
    console.log("Enumerating accounts + pkpData to re-derive truth...");
    const own = await buildOwnershipFromAccounts(provider, diamond, {
      log: (m) => process.stdout.write("\r" + m),
    });
    console.log(`\naccounts=${own.accountCount} distinct=${own.byPkp.size} conflicts=${own.conflicted.length}`);

    // 3. Every JSON pair must match an actual account owner (and be single-owner).
    for (const [key, master] of jsonPairs) {
      const truth = own.byPkp.get(key);
      if (!truth) {
        errors.push(`${key}: in JSON but not owned by any account`);
        continue;
      }
      if (!truth.masters.includes(master))
        errors.push(
          `${truth.pkpId}: JSON binds 0x${master.toString(16)} but no account holds it (holders: ${truth.masters
            .map((m) => "0x" + m.toString(16))
            .join(",")})`
        );
      if (!args.allowConflicts && truth.masters.length > 1)
        errors.push(`${truth.pkpId}: conflicted (multi-account) without --allow-conflicts`);
    }

    // 4. On-chain current binding: unbound (will write) or already correct.
    console.log("Checking current on-chain bindings...");
    const readOnly = new ethers.Contract(diamond, DIAMOND_ABI, provider);
    const keys = [...jsonPairs.keys()];
    const owners = (await mapLimit(keys, 25, (k) => readOnly.getPkpOwnerMaster(k))) as bigint[];
    let willWrite = 0;
    let noop = 0;
    keys.forEach((k, i) => {
      const o = owners[i];
      const m = jsonPairs.get(k)!;
      if (o === 0n) willWrite++;
      else if (o === m) noop++;
      else errors.push(`${k}: on-chain owner 0x${o.toString(16)} != JSON 0x${m.toString(16)}`);
    });
    console.log(`  ${willWrite} will be written, ${noop} already correct.`);

    // 5. Coverage: every unbound, single-owner pkpId must be in the JSON.
    console.log("Coverage: every unbound owned pkpId present in JSON...");
    const ownedKeys = [...own.byPkp.keys()].filter(
      (k) => args.allowConflicts || own.byPkp.get(k)!.masters.length === 1
    );
    const ownedBindings = (await mapLimit(ownedKeys, 25, (k) =>
      readOnly.getPkpOwnerMaster(k)
    )) as bigint[];
    let missing = 0;
    ownedKeys.forEach((k, i) => {
      if (ownedBindings[i] === 0n && !jsonPairs.has(k)) {
        missing++;
        if (missing <= 20) errors.push(`${own.byPkp.get(k)!.pkpId}: unbound but MISSING from JSON`);
      }
    });
    if (missing > 20) errors.push(`... and ${missing - 20} more missing`);
    console.log(`  ${missing} unbound owned pkpId(s) missing from JSON.`);

    console.log("\n" + "=".repeat(60));
    if (errors.length === 0) {
      console.log("VERIFY: PASS — the Safe JSON matches authoritative on-chain ownership.");
    } else {
      console.log(`VERIFY: FAIL — ${errors.length} problem(s):`);
      for (const e of errors.slice(0, 50)) console.log(`  ❌ ${e}`);
      if (errors.length > 50) console.log(`  ... and ${errors.length - 50} more`);
      process.exitCode = 1;
    }
  });
