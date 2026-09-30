import { task } from "hardhat/config";
import { existsSync, readFileSync, readdirSync } from "fs";
import path from "path";

/**
 * Verify a diamond proxy and every facet it currently routes to on
 * Basescan (Etherscan v2 API), using Hardhat build-info as the
 * standard-JSON input so the submitted sources are byte-for-byte what
 * produced the artifact.
 *
 * On-chain code is matched to compiled artifacts by runtime bytecode with
 * the CBOR metadata suffix stripped, then the *exact* (metadata-inclusive)
 * candidate is preferred when one exists. Multiple artifact directories can
 * be supplied because a long-lived diamond typically mixes facets compiled
 * from several commits (the proxy from the original deploy, the diamond
 * pattern facets from the prebuilt JSONs, the app facets from the latest cut).
 *
 * Usage:
 *   BASESCAN_API_KEY=... npx hardhat verify-basescan --network base \
 *     --diamond 0x... \
 *     --artifacts /path/to/checkoutA/artifacts,/path/to/checkoutB/artifacts
 *
 * Add --dry-run to see the plan without submitting anything.
 */

const ETHERSCAN_V2 = "https://api.etherscan.io/v2/api";

interface BuildInfo {
  solcLongVersion: string;
  input: {
    language: string;
    sources: Record<string, { content: string }>;
    settings: Record<string, unknown>;
  };
  output: {
    contracts: Record<
      string,
      Record<string, { evm: { deployedBytecode: { object: string } } }>
    >;
  };
}

interface Candidate {
  sourceName: string;
  contractName: string;
  deployedBytecode: string; // 0x-prefixed, full (with metadata)
  buildInfoPath: string;
  buildInfo: BuildInfo;
}

function stripMetadata(bytecode: string): string {
  const bc = (bytecode.startsWith("0x") ? bytecode.slice(2) : bytecode).toLowerCase();
  if (bc.length < 4) return bc;
  const metadataLength = parseInt(bc.slice(-4), 16);
  const suffixHexLen = (metadataLength + 2) * 2;
  if (suffixHexLen >= bc.length) return bc;
  return bc.slice(0, bc.length - suffixHexLen);
}

function loadCandidates(artifactDirs: string[]): Candidate[] {
  const out: Candidate[] = [];
  for (const dir of artifactDirs) {
    const biDir = path.join(dir, "build-info");
    if (!existsSync(biDir)) {
      throw new Error(`No build-info directory at ${biDir} (run 'npx hardhat compile' there)`);
    }
    for (const f of readdirSync(biDir).filter((f) => f.endsWith(".json"))) {
      const p = path.join(biDir, f);
      const bi: BuildInfo = JSON.parse(readFileSync(p, "utf-8"));
      for (const [sourceName, contracts] of Object.entries(bi.output.contracts)) {
        for (const [contractName, c] of Object.entries(contracts)) {
          const obj = c.evm?.deployedBytecode?.object;
          if (!obj || obj.length < 10) continue;
          out.push({
            sourceName,
            contractName,
            deployedBytecode: "0x" + obj.toLowerCase(),
            buildInfoPath: p,
            buildInfo: bi,
          });
        }
      }
    }
  }
  return out;
}

/** All candidates reproducing the runtime bytecode; exact (metadata-inclusive) ones win. */
function pickCandidates(
  onChainCode: string,
  candidates: Candidate[]
): { candidates: Candidate[]; exact: boolean } | null {
  const code = onChainCode.toLowerCase();
  const stripped = stripMetadata(code);
  const matches = candidates.filter((c) => stripMetadata(c.deployedBytecode) === stripped);
  if (matches.length === 0) return null;
  const exact = matches.filter((c) => c.deployedBytecode === code);
  return exact.length > 0
    ? { candidates: exact, exact: true }
    : { candidates: matches, exact: false };
}

function creationBytecodeOf(c: Candidate): string {
  const out = c.buildInfo.output.contracts[c.sourceName][c.contractName] as unknown as {
    evm: { bytecode: { object: string } };
  };
  return out.evm.bytecode.object.toLowerCase();
}

async function etherscan(
  chainId: number,
  apiKey: string,
  params: Record<string, string>,
  method: "GET" | "POST" = "GET"
): Promise<{ status: string; message: string; result: unknown }> {
  // Free-tier keys are limited to 3 calls/sec; space every call out.
  await new Promise((r) => setTimeout(r, 400));
  const url = new URL(ETHERSCAN_V2);
  url.searchParams.set("chainid", String(chainId));
  const body = new URLSearchParams({ apikey: apiKey, ...params });
  let res: Response;
  if (method === "GET") {
    for (const [k, v] of body) url.searchParams.set(k, v);
    res = await fetch(url);
  } else {
    res = await fetch(url, {
      method: "POST",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body,
    });
  }
  if (!res.ok) throw new Error(`Etherscan HTTP ${res.status}: ${await res.text()}`);
  return (await res.json()) as { status: string; message: string; result: unknown };
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function isVerified(chainId: number, apiKey: string, address: string): Promise<boolean> {
  const r = await etherscan(chainId, apiKey, {
    module: "contract",
    action: "getsourcecode",
    address,
  });
  const first = Array.isArray(r.result) ? (r.result[0] as { SourceCode?: string }) : undefined;
  return !!first?.SourceCode && first.SourceCode.length > 0;
}

const BLOCKSCOUT: Record<number, string> = {
  8453: "https://base.blockscout.com",
  84532: "https://base-sepolia.blockscout.com",
};

type CreationProvider = {
  getTransaction(hash: string): Promise<{ data: string } | null>;
  getBlockNumber(): Promise<number>;
  getCode(address: string, blockTag?: number): Promise<string>;
  getLogs(filter: { address: string; fromBlock: number; toBlock: number }): Promise<{ transactionHash: string }[]>;
};

/** Find the creation tx hash: Etherscan first, then Blockscout (Etherscan's free tier rejects
 *  getcontractcreation on Base), then an RPC-only bisection. */
async function fetchCreationTxHash(
  chainId: number,
  apiKey: string,
  address: string,
  provider: CreationProvider
): Promise<string> {
  const r = await etherscan(chainId, apiKey, {
    module: "contract",
    action: "getcontractcreation",
    contractaddresses: address,
  });
  const info = Array.isArray(r.result) ? (r.result[0] as { txHash?: string }) : undefined;
  if (info?.txHash) return info.txHash;

  const bs = BLOCKSCOUT[chainId];
  if (bs) {
    const res = await fetch(`${bs}/api/v2/addresses/${address}`);
    if (res.ok) {
      const j = (await res.json()) as { creation_transaction_hash?: string; creation_tx_hash?: string };
      const hash = j.creation_transaction_hash || j.creation_tx_hash;
      if (hash) return hash;
    }
  }

  // Last resort, RPC only: bisect the block where code first appeared, then read the
  // DiamondCut/OwnershipTransferred logs the constructor emitted in that block.
  console.log(`         (no indexer has the creation tx for ${address}; bisecting via RPC)`);
  const latest = await provider.getBlockNumber();
  let lo = 0;
  let hi = latest;
  if ((await provider.getCode(address, lo)) !== "0x") {
    throw new Error(`${address} already had code at block ${lo}; cannot bisect creation`);
  }
  while (hi - lo > 1) {
    const mid = Math.floor((lo + hi) / 2);
    if ((await provider.getCode(address, mid)) === "0x") lo = mid;
    else hi = mid;
  }
  const logs = await provider.getLogs({ address, fromBlock: hi, toBlock: hi });
  const hashes = [...new Set(logs.map((l) => l.transactionHash))];
  if (hashes.length !== 1) {
    throw new Error(
      `expected exactly one tx emitting logs from ${address} in creation block ${hi}, got ${hashes.length}`
    );
  }
  return hashes[0];
}

/**
 * Constructor args = creation-tx input minus the creation bytecode prefix.
 * Several candidates can share identical runtime bytecode yet differ in constructor
 * code, so the one whose creation bytecode actually prefixes the tx input wins.
 */
async function resolveProxyCandidate(
  chainId: number,
  apiKey: string,
  address: string,
  provider: CreationProvider,
  candidates: Candidate[]
): Promise<{ candidate: Candidate; constructorArgs: string }> {
  const txHash = await fetchCreationTxHash(chainId, apiKey, address, provider);
  const tx = await provider.getTransaction(txHash);
  if (!tx) throw new Error(`creation tx ${txHash} not found via RPC`);
  const input = tx.data.toLowerCase().replace(/^0x/, "");
  for (const c of candidates) {
    const prefix = creationBytecodeOf(c);
    if (input.startsWith(prefix)) {
      return { candidate: c, constructorArgs: input.slice(prefix.length) };
    }
  }
  throw new Error(
    `creation tx ${txHash} input for ${address} does not start with any matching candidate's ` +
      `creation bytecode; pass --constructor-args manually`
  );
}

task(
  "verify-basescan",
  "Verify a diamond proxy and all of its live facets on Basescan from Hardhat build-info"
)
  .addParam("diamond", "Diamond proxy contract address")
  .addOptionalParam(
    "artifacts",
    "Comma-separated Hardhat artifacts directories to source build-info from (default: this project's)"
  )
  .addOptionalParam("apikey", "Basescan/Etherscan API key (default: env BASESCAN_API_KEY)")
  .addOptionalParam(
    "constructorArgs",
    "Hex-encoded (no 0x) constructor args for the proxy; overrides creation-tx lookup"
  )
  .addFlag("dryRun", "Only print the match plan; do not submit anything")
  .addFlag("force", "Resubmit even if Basescan already reports the address as verified")
  .setAction(async (taskArgs, hre) => {
    const diamondAddress: string = taskArgs.diamond;
    const apiKey: string = taskArgs.apikey || process.env.BASESCAN_API_KEY || "";
    const dryRun: boolean = taskArgs.dryRun;
    const force: boolean = taskArgs.force;
    const chainId = (hre.network.config as { chainId?: number }).chainId;
    if (!chainId) throw new Error("network config must set chainId");
    if (!apiKey && !dryRun) throw new Error("Set BASESCAN_API_KEY or pass --apikey");

    const artifactDirs: string[] = (taskArgs.artifacts
      ? String(taskArgs.artifacts).split(",")
      : [hre.config.paths.artifacts]
    ).map((d: string) => path.resolve(d.trim()));

    console.log(`Network: ${hre.network.name} (chainId ${chainId})`);
    console.log(`Diamond: ${diamondAddress}`);
    console.log(`Artifacts: ${artifactDirs.join(", ")}`);

    const candidates = loadCandidates(artifactDirs);
    console.log(`Loaded ${candidates.length} compiled contract candidates.\n`);

    const provider = new hre.ethers.JsonRpcProvider(
      (hre.network.config as { url?: string }).url
    );

    const loupe = new hre.ethers.Contract(
      diamondAddress,
      ["function facetAddresses() view returns (address[])"],
      provider
    );
    const facetAddresses: string[] = await loupe.facetAddresses();

    // Proxy first, then facets.
    const targets = [
      { label: "AccountConfig (proxy)", address: diamondAddress, isProxy: true },
      ...facetAddresses.map((a) => ({ label: "facet", address: a, isProxy: false })),
    ];

    interface Plan {
      label: string;
      address: string;
      isProxy: boolean;
      candidate: Candidate;
      alternatives: Candidate[];
      exact: boolean;
    }
    const plan: Plan[] = [];
    let unmatched = 0;

    for (const t of targets) {
      const code = await provider.getCode(t.address);
      const pick = pickCandidates(code, candidates);
      if (!pick) {
        console.log(`NOMATCH  ${t.address}  (${t.label}, ${code.length / 2 - 1} bytes) — no artifact reproduces this bytecode`);
        unmatched++;
        continue;
      }
      const { exact } = pick;
      const candidate = pick.candidates[0];
      const extra = pick.candidates.length > 1 ? `  (+${pick.candidates.length - 1} equivalent)` : "";
      console.log(
        `${exact ? "EXACT   " : "PARTIAL "} ${t.address}  ${candidate.sourceName}:${candidate.contractName}  <- ${candidate.buildInfoPath}${extra}`
      );
      plan.push({ ...t, candidate, alternatives: pick.candidates, exact });
    }

    if (dryRun) {
      console.log(`\nDry run: ${plan.length} verifiable, ${unmatched} unmatched.`);
      return;
    }

    console.log("");
    let failures = 0;
    for (const p of plan) {
      const already = await isVerified(chainId, apiKey, p.address);
      if (already && !force) {
        console.log(`SKIP     ${p.address}  already verified on Basescan`);
        continue;
      }

      let constructorArgs = "";
      if (p.isProxy) {
        if (taskArgs.constructorArgs) {
          constructorArgs = String(taskArgs.constructorArgs).replace(/^0x/, "");
        } else {
          const r = await resolveProxyCandidate(chainId, apiKey, p.address, provider, p.alternatives);
          p.candidate = r.candidate;
          constructorArgs = r.constructorArgs;
        }
        console.log(
          `         constructor args: ${constructorArgs.length / 2} bytes  (source: ${p.candidate.buildInfoPath})`
        );
      }

      const submit = await etherscan(
        chainId,
        apiKey,
        {
          module: "contract",
          action: "verifysourcecode",
          codeformat: "solidity-standard-json-input",
          sourceCode: JSON.stringify(p.candidate.buildInfo.input),
          contractaddress: p.address,
          contractname: `${p.candidate.sourceName}:${p.candidate.contractName}`,
          compilerversion: `v${p.candidate.buildInfo.solcLongVersion}`,
          constructorArguements: constructorArgs, // sic: Etherscan's spelling
        },
        "POST"
      );

      if (submit.status !== "1") {
        console.log(`FAIL     ${p.address}  submit: ${submit.result}`);
        failures++;
        continue;
      }
      const guid = String(submit.result);
      console.log(`SUBMIT   ${p.address}  ${p.candidate.contractName}  guid=${guid}`);

      // Poll for the result.
      let verdict = "Pending in queue";
      for (let i = 0; i < 30 && /pending/i.test(verdict); i++) {
        await sleep(4000);
        const st = await etherscan(chainId, apiKey, {
          module: "contract",
          action: "checkverifystatus",
          guid,
        });
        verdict = String(st.result);
      }
      if (/pass/i.test(verdict)) {
        console.log(`OK       ${p.address}  ${verdict}`);
      } else if (/already verified/i.test(verdict)) {
        console.log(`SKIP     ${p.address}  ${verdict}`);
      } else {
        console.log(`FAIL     ${p.address}  ${verdict}`);
        failures++;
      }
    }

    console.log("");
    if (unmatched) console.log(`${unmatched} address(es) had no matching artifact and were not submitted.`);
    if (failures) {
      console.error(`${failures} verification(s) failed.`);
      process.exit(1);
    }
    console.log("Done.");
  });
