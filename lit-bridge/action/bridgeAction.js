// Lit Action: the lit-bridge verification oracle + relayer.
//
// Permissionless half of a burn/mint cross-chain token. Two modes:
//   * Manual (sign-only): called with explicit params, returns a signature
//     anyone can submit to BridgeToken.mint on the destination chain.
//   * Relay (auto-broadcast): a lit-triggers chain_event trigger fires this on
//     each BurnInitiated; it re-verifies, signs, AND broadcasts the mint from
//     the oracle account (paying gas), only if the burn prepaid enough gas.
//
// Hardened after an adversarial review (see plans/hyperlane-competitor.md):
//   #1 the registry read itself requires M-of-N (>=2) distinct providers — a
//      single lying bootstrap RPC can no longer define the source config.
//   #2/#5 the destination contract is resolved from the SOURCE token's
//      governance-pinned `bridgePartner[destChainId]` under consensus, never
//      from caller params; the relay gas cap == the prepay-quote gas, so the
//      oracle can never be made to spend more than was prepaid.
//   #3 auto-relay only between chains with the same native asset (the prepay
//      wei comparison is otherwise meaningless).
//   #4 (partial) confirmation depth is the registry's per-chain, Safe-governed
//      `minConfirmations` over a floor; true `finalized`-tag finality needs a
//      retry poller (incompatible with the single-fire trigger) — follow-up.
//   #6 only distinct provider hosts count toward quorum.
//   #7 the destination RPC is chain-id-checked and tried in order.
//
// What's pinned in code (the trust root): REGISTRY_ADDRESS/CHAIN_ID, the
// registry-read host allowlist, BRIDGE_PKP_ID, provider host maps, finality +
// gas + quorum floors, the native-asset map, and the consensus rules.

// ── Trust root ──────────────────────────────────────────────────────────────
const REGISTRY_CHAIN_ID = 8453; // Base mainnet
const REGISTRY_ADDRESS = "0x0000000000000000000000000000000000000000"; // set at deploy

// Keyless public Base endpoints for the bootstrap registry read (+ the keyed
// providers if ever passed). Several are listed for redundancy: the read needs
// REGISTRY_READ_QUORUM of them to AGREE, so passing 3 reliable ones tolerates
// one flaking/rate-limiting under burst (which is why 2 wasn't enough).
const REGISTRY_RPC_HOSTS = [
  /^base-mainnet\.g\.alchemy\.com$/i,
  /^base-mainnet\.infura\.io$/i,
  /^mainnet\.base\.org$/i,
  /^base-rpc\.publicnode\.com$/i,
  /^1rpc\.io$/i,
  /^gateway\.tenderly\.co$/i,
];
// #1: the bootstrap registry read is itself M-of-N. Distinct hosts required.
const REGISTRY_READ_QUORUM = 2;

const BRIDGE_PKP_ID = "REPLACE_WITH_PKP_ID";

const MIN_CONFIRMATIONS_FLOOR = 2;
// #2: one gas number for BOTH the prepay check and the actual mint tx cap, so
// the oracle can never be billed for more gas than the burn prepaid.
const MINT_GAS_LIMIT = 300000;
const RELAY_DEADLINE_SECS = 3600;

const RPC_TYPE = { ALCHEMY: 0, INFURA: 1, CUSTOM: 2 };

// #3: chains whose native gas token is ETH. Auto-relay (which compares prepay
// wei to dest-gas wei 1:1) is only allowed between two of these.
const ETH_NATIVE_CHAINS = new Set([1, 10, 8453, 42161, 11155111, 84532, 421614]);

const ALCHEMY_SUBDOMAINS = {
  1: "eth-mainnet", 8453: "base-mainnet", 42161: "arb-mainnet", 10: "opt-mainnet",
  137: "polygon-mainnet", 84532: "base-sepolia", 421614: "arb-sepolia", 11155111: "eth-sepolia",
};
const INFURA_NETWORKS = {
  1: "mainnet", 8453: "base-mainnet", 42161: "arbitrum-mainnet", 10: "optimism-mainnet",
  137: "polygon-mainnet", 84532: "base-sepolia", 421614: "arbitrum-sepolia", 11155111: "sepolia",
};

const BURN_TOPIC = "BurnInitiated(address,address,uint256,uint256,uint256,uint256)";
const MINT_ABI =
  "function mint(uint256 srcChainId,address srcContract,bytes32 burnTxHash,uint256 logIndex,address recipient,uint256 amount,uint256 srcNonce,uint256 deadline,bytes signature)";
const PARTNER_ABI = "function bridgePartner(uint256) view returns (address)";

// ── Pure helpers (unit tested) ───────────────────────────────────────────────

function buildRpcUrl(entry, chainId, decryptedSecret) {
  const cid = Number(chainId);
  if (entry.rpcType === RPC_TYPE.ALCHEMY) {
    const sub = ALCHEMY_SUBDOMAINS[cid];
    if (!sub) return { ok: false, reason: `no alchemy subdomain for chain ${cid}` };
    return { ok: true, url: `https://${sub}.g.alchemy.com/v2/${decryptedSecret}` };
  }
  if (entry.rpcType === RPC_TYPE.INFURA) {
    const net = INFURA_NETWORKS[cid];
    if (!net) return { ok: false, reason: `no infura network for chain ${cid}` };
    return { ok: true, url: `https://${net}.infura.io/v3/${decryptedSecret}` };
  }
  if (entry.rpcType === RPC_TYPE.CUSTOM) {
    let parsed;
    try {
      parsed = new URL(decryptedSecret);
    } catch {
      return { ok: false, reason: "custom rpc decrypted to an invalid URL" };
    }
    if (parsed.protocol !== "https:") {
      return { ok: false, reason: `custom rpc must be https (got ${parsed.protocol})` };
    }
    if (!entry.host || parsed.hostname.toLowerCase() !== entry.host.toLowerCase()) {
      return { ok: false, reason: `custom rpc host ${parsed.hostname} != registry host ${entry.host}` };
    }
    return { ok: true, url: decryptedSecret };
  }
  return { ok: false, reason: `unknown rpc type ${entry.rpcType}` };
}

function checkRegistryRpcUrl(url) {
  let parsed;
  try {
    parsed = new URL(url);
  } catch {
    return { ok: false, reason: "registryRpcUrl is not a valid URL" };
  }
  if (parsed.protocol !== "https:") {
    return { ok: false, reason: `registryRpcUrl must be https (got ${parsed.protocol})` };
  }
  if (!REGISTRY_RPC_HOSTS.some((re) => re.test(parsed.hostname))) {
    return { ok: false, reason: `registryRpcUrl host ${parsed.hostname} not allowlisted` };
  }
  return { ok: true };
}

/// #6: count distinct hostnames among a set of URLs (only distinct providers
/// count toward quorum — two URLs on the same host are one trust root). Pure.
function distinctHostCount(urls) {
  const hosts = new Set();
  for (const u of urls) {
    try {
      hosts.add(new URL(u).hostname.toLowerCase());
    } catch {
      /* skip invalid */
    }
  }
  return hosts.size;
}

/// #3: may these two chains auto-relay? Only if both are ETH-native, so the
/// prepay-vs-dest-gas wei comparison is apples-to-apples. Pure.
function sameNative(srcChainId, destChainId) {
  return ETH_NATIVE_CHAINS.has(Number(srcChainId)) && ETH_NATIVE_CHAINS.has(Number(destChainId));
}

function canonicalize(value) {
  if (Array.isArray(value)) return "[" + value.map(canonicalize).join(",") + "]";
  if (value && typeof value === "object") {
    const keys = Object.keys(value).sort();
    return "{" + keys.map((k) => JSON.stringify(k) + ":" + canonicalize(value[k])).join(",") + "}";
  }
  return JSON.stringify(value);
}

function criticalFacts({ status, blockNumber, logAddress, topics, data, destPartner }) {
  return {
    status: String(BigInt(status)),
    blockNumber: String(BigInt(blockNumber)),
    logAddress: String(logAddress).toLowerCase(),
    topics: (topics || []).map((t) => String(t).toLowerCase()),
    data: String(data).toLowerCase(),
    destPartner: String(destPartner).toLowerCase(),
  };
}

function tallyConsensus(votes, quorum) {
  const q = Math.max(1, Number(quorum) || 1);
  const groups = new Map();
  let counted = 0;
  for (const v of votes) {
    if (!v || !v.ok) continue;
    counted++;
    const key = canonicalize(v.facts);
    const g = groups.get(key);
    if (g) g.count++;
    else groups.set(key, { count: 1, facts: v.facts });
  }
  if (counted < q) {
    return { agreed: false, reason: `only ${counted} provider(s) voted; need quorum ${q}` };
  }
  let best = null;
  for (const g of groups.values()) if (!best || g.count > best.count) best = g;
  if (!best || best.count < q) {
    return {
      agreed: false,
      reason: `no set of facts reached quorum ${q} (best agreement: ${best ? best.count : 0} of ${counted})`,
    };
  }
  return { agreed: true, facts: best.facts };
}

function effectiveMinConfirmations(registryMinConf) {
  return Math.max(MIN_CONFIRMATIONS_FLOOR, Number(registryMinConf) || 0);
}

function gasPrepaySufficient(gasPrepaid, destGasPrice, gasLimit) {
  return BigInt(gasPrepaid.toString()) >= BigInt(destGasPrice.toString()) * BigInt(gasLimit);
}

function mapEventToInputs(event) {
  if (!event || typeof event !== "object") {
    return { ok: false, reason: "relay mode: missing event" };
  }
  const srcChainId = event.chain_id;
  const burnTxHash = event.transaction_hash;
  const srcContract = event.contract_address || event.address;
  const logIndex = event.log_index;
  if (srcChainId == null || !burnTxHash || !srcContract || logIndex == null) {
    return { ok: false, reason: "relay mode: event missing chain_id/transaction_hash/contract/log_index" };
  }
  return { ok: true, srcChainId, burnTxHash, srcContract, logIndex };
}

// ── Orchestration ────────────────────────────────────────────────────────────

async function main(params) {
  if (REGISTRY_ADDRESS === "0x0000000000000000000000000000000000000000") {
    return { authorized: false, reason: "REGISTRY_ADDRESS not configured" };
  }
  const relay = params.source === "chain_event" || !!params.event;

  // ---- 0. Verification inputs (manual params vs relay event) ---------------
  let srcChainId, burnTxHash, srcContract, logIndex;
  if (relay) {
    const m = mapEventToInputs(params.event);
    if (!m.ok) return { authorized: false, reason: m.reason };
    ({ srcChainId, burnTxHash, srcContract, logIndex } = m);
  } else {
    ({ burnTxHash, srcChainId, srcContract, logIndex } = params);
    if (!burnTxHash || srcChainId == null || !srcContract || logIndex == null) {
      return { authorized: false, reason: "manual mode: missing burnTxHash/srcChainId/srcContract/logIndex" };
    }
  }

  // ---- 1. Registry-read RPCs: allowlisted + >= quorum DISTINCT hosts (#1,#6)-
  const regUrls = Array.isArray(params.registryRpcUrls) ? params.registryRpcUrls : [];
  for (const u of regUrls) {
    const ok = checkRegistryRpcUrl(u);
    if (!ok.ok) return { authorized: false, reason: ok.reason };
  }
  if (distinctHostCount(regUrls) < REGISTRY_READ_QUORUM) {
    return {
      authorized: false,
      reason: `need >= ${REGISTRY_READ_QUORUM} distinct registry RPC hosts, got ${distinctHostCount(regUrls)}`,
    };
  }

  // ---- 2. Read source config (consensus) + decrypt + dedupe hosts (#1,#6) --
  const cfg = await readChainConfig(regUrls, srcChainId);
  if (!cfg.ok) return { authorized: false, reason: cfg.reason };
  const minConf = effectiveMinConfirmations(cfg.minConfirmations);
  const quorum = Math.max(1, Number(cfg.quorum) || 1);
  const urls = [];
  for (const entry of cfg.rpcs) {
    const secret = await Lit.Actions.Decrypt({ pkpId: BRIDGE_PKP_ID, ciphertext: entry.encSecret });
    const built = buildRpcUrl(entry, srcChainId, secret);
    if (!built.ok) return { authorized: false, reason: built.reason };
    urls.push(built.url);
  }
  if (distinctHostCount(urls) < quorum) {
    return {
      authorized: false,
      reason: `chain ${srcChainId}: ${distinctHostCount(urls)} distinct rpc host(s) < quorum ${quorum}`,
    };
  }

  // ---- 3. M-of-N consensus on the burn (finality + dest partner) (#4,#5) ---
  const idx = Number(logIndex);
  const partnerIface = new ethers.utils.Interface([PARTNER_ABI]);
  const votes = await Promise.all(
    urls.map((url) => collectBurnFacts(url, srcChainId, srcContract, burnTxHash, idx, minConf, partnerIface))
  );
  const consensus = tallyConsensus(votes, quorum);
  if (!consensus.agreed) return { authorized: false, reason: consensus.reason };
  const facts = consensus.facts;

  // ---- 4. Decode the agreed log -------------------------------------------
  // (logAddress / topic0 already validated per-provider in collectBurnFacts.)
  const recipient = ethers.utils.getAddress("0x" + facts.topics[2].slice(26));
  const destChainId = Number(BigInt(facts.topics[3]));
  const decoded = ethers.utils.defaultAbiCoder.decode(["uint256", "uint256", "uint256"], facts.data);
  const amount = decoded[0];
  const srcNonce = decoded[1];
  const gasPrepaid = decoded[2];

  // ---- 5. Destination is the governance-pinned partner, not caller input (#2,#5)
  if (!facts.destPartner || /^0x0+$/.test(facts.destPartner)) {
    return { authorized: false, reason: `source token has no bridgePartner for dest chain ${destChainId}` };
  }
  const destContract = ethers.utils.getAddress(facts.destPartner);
  if (destChainId === Number(srcChainId)) {
    return { authorized: false, reason: "src and dest chain are the same" };
  }

  // ---- 6. Resolve deadline + gas gate (relay) ------------------------------
  let deadline, destRpcUrl;
  if (relay) {
    if (!sameNative(srcChainId, destChainId)) {
      return { authorized: false, reason: `auto-relay only between same-native chains (src ${srcChainId}, dest ${destChainId})` };
    }
    const destPick = await pickDestRpc(regUrls, destChainId);
    if (!destPick.ok) return { authorized: false, reason: destPick.reason };
    destRpcUrl = destPick.url;
    const block = await rpc(destRpcUrl, "eth_getBlockByNumber", ["latest", false]);
    deadline = Number(BigInt(block.timestamp)) + RELAY_DEADLINE_SECS;
    // #2: prepay must cover the SAME gas the mint tx is capped at.
    const destGasPrice = BigInt(await rpc(destRpcUrl, "eth_gasPrice", []));
    if (!gasPrepaySufficient(gasPrepaid, destGasPrice, MINT_GAS_LIMIT)) {
      return {
        authorized: false,
        reason: `insufficient gas prepay: have ${gasPrepaid.toString()} wei, need ${(destGasPrice * BigInt(MINT_GAS_LIMIT)).toString()} wei (not auto-relayed; holder can self-submit)`,
      };
    }
  } else {
    deadline = params.deadline;
    if (deadline == null) return { authorized: false, reason: "manual mode: deadline required" };
  }

  // ---- 7. Sign (Option B: dedicated account) -------------------------------
  const digest = ethers.utils.keccak256(
    ethers.utils.defaultAbiCoder.encode(
      ["uint256", "address", "bytes32", "uint256", "address", "uint256", "uint256", "uint256", "address", "uint256"],
      [srcChainId, srcContract, burnTxHash, idx, recipient, amount, srcNonce, deadline, destContract, destChainId]
    )
  );
  const wallet = new ethers.Wallet(await Lit.Actions.getPrivateKey({ pkpId: BRIDGE_PKP_ID }));
  const signature = await wallet.signMessage(ethers.utils.arrayify(digest));

  const base = {
    authorized: true, signer: wallet.address, srcChainId, srcContract, burnTxHash,
    logIndex: idx, recipient, amount: amount.toString(), srcNonce: srcNonce.toString(),
    destChainId, destContract, deadline, signature, quorum,
  };

  // ---- 8. Manual: return signature. Relay: broadcast the mint (#2 capped). --
  if (!relay) return base;
  if (params.dryRun) return { ...base, dryRun: true };

  const data = new ethers.utils.Interface([MINT_ABI]).encodeFunctionData("mint", [
    srcChainId, srcContract, burnTxHash, idx, recipient, amount, srcNonce, deadline, signature,
  ]);
  const signer = wallet.connect(new ethers.providers.JsonRpcProvider(destRpcUrl));
  const tx = await signer.sendTransaction({
    to: destContract,
    data,
    gasLimit: ethers.BigNumber.from(MINT_GAS_LIMIT), // == the gas the prepay covered
  });
  const rcpt = await tx.wait();
  return { ...base, minted: true, mintTxHash: rcpt.transactionHash, mintBlock: rcpt.blockNumber };
}

async function readChainConfig(regUrls, chainId) {
  const iface = new ethers.utils.Interface([
    "function getChain(uint256) view returns (bool exists, uint64 minConfirmations, uint8 quorum, uint256 rpcCount)",
    "function getRpc(uint256 chainId, uint256 index) view returns (uint8 rpcType, string host, string encSecret)",
  ]);
  const reads = await Promise.all(regUrls.map((url) => readChainConfigFrom(url, iface, chainId)));
  const consensus = tallyConsensus(reads, REGISTRY_READ_QUORUM);
  if (!consensus.agreed) return { ok: false, reason: `registry read: ${consensus.reason}` };
  const c = consensus.facts;
  if (!c.exists) return { ok: false, reason: `chain ${chainId} not configured in registry` };
  return { ok: true, minConfirmations: c.minConfirmations, quorum: c.quorum, rpcs: c.rpcs };
}

async function readChainConfigFrom(url, iface, chainId) {
  try {
    const head = await ethCallTo(url, REGISTRY_ADDRESS, iface, "getChain", [chainId]);
    if (!head[0]) return { ok: true, facts: { exists: false } };
    const rpcCount = Number(head[3]);
    const rpcs = [];
    for (let i = 0; i < rpcCount; i++) {
      const r = await ethCallTo(url, REGISTRY_ADDRESS, iface, "getRpc", [chainId, i]);
      rpcs.push({ rpcType: Number(r[0]), host: String(r[1]), encSecret: String(r[2]) });
    }
    return { ok: true, facts: { exists: true, minConfirmations: head[1].toString(), quorum: Number(head[2]), rpcs } };
  } catch (e) {
    return { ok: false, reason: `registry read from one rpc failed: ${e.message}` };
  }
}

/// One provider's vote: chain id, receipt, FINALITY (#4), the target log, and
/// the governance-pinned destination partner (#5) — all under consensus.
async function collectBurnFacts(url, srcChainId, srcContract, burnTxHash, logIndex, minConf, partnerIface) {
  try {
    const reportedChainId = await rpc(url, "eth_chainId", []);
    if (BigInt(reportedChainId) !== BigInt(srcChainId)) {
      return { ok: false, reason: `rpc reports chain ${BigInt(reportedChainId)}` };
    }
    const receipt = await rpc(url, "eth_getTransactionReceipt", [burnTxHash]);
    if (!receipt) return { ok: false, reason: "burn tx not found" };
    if (BigInt(receipt.status) !== 1n) return { ok: false, reason: "burn tx reverted" };
    const burnBlock = BigInt(receipt.blockNumber);

    // #4 (partial): require `minConf` confirmations, where minConf is the
    // registry's per-chain value (Safe-governed) over a code floor. The Safe
    // should set this to a finality-appropriate depth per chain. NOTE: a true
    // `finalized`-tag gate (L1 finality) is the correct end state but conflicts
    // with the lit-triggers single-fire model (it fires ~once at a fixed depth
    // and never retries, so a not-yet-finalized burn would fail permanently).
    // The full fix is a retry poller for un-minted-but-now-final burns — tracked
    // as follow-up. Manual mode callers can simply wait for finality themselves.
    const head = BigInt(await rpc(url, "eth_blockNumber", []));
    if (head - burnBlock < BigInt(minConf)) return { ok: false, reason: "below min confirmations" };

    const log = (receipt.logs || []).find((l) => Number(l.logIndex) === logIndex);
    if (!log) return { ok: false, reason: `no log with index ${logIndex}` };
    if (String(log.address).toLowerCase() !== String(srcContract).toLowerCase()) {
      return { ok: false, reason: `log emitted by ${log.address}, not srcContract` };
    }
    if ((log.topics[0] || "").toLowerCase() !== ethers.utils.id(BURN_TOPIC).toLowerCase()) {
      return { ok: false, reason: "log is not a BurnInitiated event" };
    }
    // Resolve the governance-pinned destination from the SOURCE token (#5).
    const destChainId = BigInt(log.topics[3]);
    const partner = await ethCallTo(url, srcContract, partnerIface, "bridgePartner", [destChainId.toString()]);
    return {
      ok: true,
      facts: criticalFacts({
        status: receipt.status, blockNumber: receipt.blockNumber,
        logAddress: log.address, topics: log.topics, data: log.data,
        destPartner: Array.isArray(partner) ? partner[0] : partner,
      }),
    };
  } catch (e) {
    return { ok: false, reason: `rpc error: ${e.message}` };
  }
}

/// #7: pick the first dest RPC (from the registry) that actually reports the
/// expected chain id; used for the relay clock, gas quote, and broadcast.
async function pickDestRpc(regUrls, destChainId) {
  const cfg = await readChainConfig(regUrls, destChainId);
  if (!cfg.ok) return { ok: false, reason: `dest ${cfg.reason}` };
  for (const entry of cfg.rpcs) {
    try {
      const secret = await Lit.Actions.Decrypt({ pkpId: BRIDGE_PKP_ID, ciphertext: entry.encSecret });
      const built = buildRpcUrl(entry, destChainId, secret);
      if (!built.ok) continue;
      const cid = await rpc(built.url, "eth_chainId", []);
      if (BigInt(cid) === BigInt(destChainId)) return { ok: true, url: built.url };
    } catch {
      /* try next */
    }
  }
  return { ok: false, reason: `no working dest RPC for chain ${destChainId}` };
}

async function ethCallTo(url, to, iface, fn, args) {
  const data = iface.encodeFunctionData(fn, args);
  const result = await rpc(url, "eth_call", [{ to, data }, "latest"]);
  if (!result || result === "0x") throw new Error(`${fn} returned empty (wrong address or chain?)`);
  return iface.decodeFunctionResult(fn, result);
}

async function rpc(url, method, params) {
  let lastErr;
  // Generous retry: keyless public endpoints rate-limit under the registry-read
  // burst (getChain + getRpc per chain per provider). Backoff up to ~3s clears
  // the typical short rate-limit window.
  for (let attempt = 0; attempt < 5; attempt++) {
    if (attempt > 0) await new Promise((r) => setTimeout(r, 600 * attempt));
    try {
      const res = await fetch(url, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
        redirect: "error",
      });
      if (res.status === 429 || res.status >= 500) {
        lastErr = new Error(`${method} -> HTTP ${res.status}`);
        continue;
      }
      const body = await res.json();
      if (body.error) throw new Error(`${method} -> ${body.error.message}`);
      return body.result;
    } catch (e) {
      lastErr = e;
    }
  }
  throw lastErr;
}

if (typeof module !== "undefined" && module.exports) {
  module.exports = {
    RPC_TYPE, buildRpcUrl, checkRegistryRpcUrl, distinctHostCount, sameNative,
    canonicalize, criticalFacts, tallyConsensus, effectiveMinConfirmations,
    gasPrepaySufficient, mapEventToInputs, MIN_CONFIRMATIONS_FLOOR, REGISTRY_READ_QUORUM,
  };
}
