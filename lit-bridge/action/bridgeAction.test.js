// Unit tests for the pure logic of bridgeAction.js.
// Run: node --test   (from lit-bridge/action/)
//
// These cover the trust-critical decisions that don't need a network or the
// Lit runtime: provider URL construction, the registry-host allowlist, the
// canonicalization that drives consensus, and the quorum tally itself.

const test = require("node:test");
const assert = require("node:assert/strict");

const {
  RPC_TYPE,
  buildRpcUrl,
  checkRegistryRpcUrl,
  canonicalize,
  criticalFacts,
  tallyConsensus,
  effectiveMinConfirmations,
  gasPrepaySufficient,
  mapEventToInputs,
  distinctHostCount,
  sameNative,
  MIN_CONFIRMATIONS_FLOOR,
  REGISTRY_READ_QUORUM,
} = require("./bridgeAction.js");

const KEY = "abcdef123456";

test("buildRpcUrl: alchemy constructs host from code map (config can't redirect)", () => {
  const r = buildRpcUrl({ rpcType: RPC_TYPE.ALCHEMY }, 84532, KEY);
  assert.equal(r.ok, true);
  assert.equal(r.url, `https://base-sepolia.g.alchemy.com/v2/${KEY}`);
});

test("buildRpcUrl: infura constructs host from code map", () => {
  const r = buildRpcUrl({ rpcType: RPC_TYPE.INFURA }, 421614, KEY);
  assert.equal(r.ok, true);
  assert.equal(r.url, `https://arbitrum-sepolia.infura.io/v3/${KEY}`);
});

test("buildRpcUrl: alchemy/infura reject chains with no code-resident host", () => {
  assert.equal(buildRpcUrl({ rpcType: RPC_TYPE.ALCHEMY }, 999999, KEY).ok, false);
  assert.equal(buildRpcUrl({ rpcType: RPC_TYPE.INFURA }, 999999, KEY).ok, false);
});

test("buildRpcUrl: custom must match the registry's plaintext host", () => {
  const good = buildRpcUrl(
    { rpcType: RPC_TYPE.CUSTOM, host: "rpc.my-chain.io" },
    1234,
    "https://rpc.my-chain.io/key/xyz"
  );
  assert.equal(good.ok, true);
  assert.equal(good.url, "https://rpc.my-chain.io/key/xyz");

  const mismatch = buildRpcUrl(
    { rpcType: RPC_TYPE.CUSTOM, host: "rpc.my-chain.io" },
    1234,
    "https://evil.example/key/xyz"
  );
  assert.equal(mismatch.ok, false);
});

test("buildRpcUrl: custom rejects http and garbage", () => {
  assert.equal(
    buildRpcUrl({ rpcType: RPC_TYPE.CUSTOM, host: "rpc.x.io" }, 1, "http://rpc.x.io").ok,
    false
  );
  assert.equal(
    buildRpcUrl({ rpcType: RPC_TYPE.CUSTOM, host: "rpc.x.io" }, 1, "not-a-url").ok,
    false
  );
});

test("buildRpcUrl: unknown provider type fails closed", () => {
  assert.equal(buildRpcUrl({ rpcType: 7 }, 1, KEY).ok, false);
});

test("checkRegistryRpcUrl: allowlist + https enforced (Base mainnet registry)", () => {
  assert.equal(checkRegistryRpcUrl("https://mainnet.base.org").ok, true);
  assert.equal(checkRegistryRpcUrl("https://base-mainnet.g.alchemy.com/v2/x").ok, true);
  assert.equal(checkRegistryRpcUrl("http://mainnet.base.org").ok, false); // not https
  assert.equal(checkRegistryRpcUrl("https://evil.example").ok, false); // not allowlisted
  assert.equal(checkRegistryRpcUrl("https://sepolia.base.org").ok, false); // testnet no longer allowed
  assert.equal(checkRegistryRpcUrl("garbage").ok, false);
});

test("canonicalize: key order independent, value sensitive", () => {
  assert.equal(
    canonicalize({ a: 1, b: [2, 3] }),
    canonicalize({ b: [2, 3], a: 1 })
  );
  assert.notEqual(canonicalize({ a: 1 }), canonicalize({ a: 2 }));
  assert.notEqual(canonicalize([1, 2]), canonicalize([2, 1])); // arrays are ordered
});

test("criticalFacts: normalizes case + numeric form, excludes head", () => {
  const a = criticalFacts({
    status: "0x1",
    blockNumber: "0x10",
    logAddress: "0xAbCd",
    topics: ["0xTOP", "0xFROM"],
    data: "0xDEAD",
  });
  const b = criticalFacts({
    status: 1,
    blockNumber: 16,
    logAddress: "0xabcd",
    topics: ["0xtop", "0xfrom"],
    data: "0xdead",
  });
  assert.equal(canonicalize(a), canonicalize(b));
  assert.ok(!("head" in a) && !("confirmations" in a));
});

test("tallyConsensus: 2-of-3 agree, 1 disagrees -> agreed", () => {
  const f = { status: "1", blockNumber: "100", logAddress: "0xa", topics: ["0xt"], data: "0x" };
  const other = { ...f, blockNumber: "999" };
  const r = tallyConsensus(
    [{ ok: true, facts: f }, { ok: true, facts: f }, { ok: true, facts: other }],
    2
  );
  assert.equal(r.agreed, true);
  assert.equal(r.facts.blockNumber, "100");
});

test("tallyConsensus: no majority reaches quorum -> fail closed", () => {
  const a = { status: "1", blockNumber: "1", logAddress: "0xa", topics: [], data: "0x" };
  const b = { ...a, blockNumber: "2" };
  const c = { ...a, blockNumber: "3" };
  const r = tallyConsensus(
    [{ ok: true, facts: a }, { ok: true, facts: b }, { ok: true, facts: c }],
    2
  );
  assert.equal(r.agreed, false);
});

test("tallyConsensus: abstaining providers don't count toward quorum", () => {
  const f = { status: "1", blockNumber: "5", logAddress: "0xa", topics: [], data: "0x" };
  // one good vote + two abstentions, quorum 2 -> not enough voters
  const r = tallyConsensus(
    [{ ok: true, facts: f }, { ok: false, reason: "below finality" }, { ok: false, reason: "rpc error" }],
    2
  );
  assert.equal(r.agreed, false);
  assert.match(r.reason, /only 1 provider/);
});

test("tallyConsensus: quorum 1 single provider (escape hatch) works", () => {
  const f = { status: "1", blockNumber: "5", logAddress: "0xa", topics: [], data: "0x" };
  const r = tallyConsensus([{ ok: true, facts: f }], 1);
  assert.equal(r.agreed, true);
});

test("tallyConsensus: a single lying RPC cannot forge at quorum 2", () => {
  // Two honest providers agree; the attacker-controlled one fabricates a
  // different receipt. With quorum 2 the fabrication is outvoted.
  const honest = { status: "1", blockNumber: "100", logAddress: "0xtoken", topics: ["0xburn"], data: "0x01" };
  const forged = { status: "1", blockNumber: "100", logAddress: "0xtoken", topics: ["0xburn"], data: "0xFF" };
  const r = tallyConsensus(
    [{ ok: true, facts: honest }, { ok: true, facts: honest }, { ok: true, facts: forged }],
    2
  );
  assert.equal(r.agreed, true);
  assert.equal(r.facts.data, "0x01"); // honest data wins
});

test("effectiveMinConfirmations: floor wins over a too-low registry value", () => {
  assert.equal(effectiveMinConfirmations(0), MIN_CONFIRMATIONS_FLOOR);
  assert.equal(effectiveMinConfirmations(50), 50);
});

test("distinctHostCount: only distinct hosts count toward quorum (#6)", () => {
  assert.equal(distinctHostCount(["https://a.com/x", "https://b.com/y"]), 2);
  assert.equal(distinctHostCount(["https://a.com/x", "https://a.com/z"]), 1); // same host, one trust root
  assert.equal(distinctHostCount([]), 0);
  assert.equal(distinctHostCount(["garbage", "https://a.com"]), 1); // invalid skipped
});

test("REGISTRY_READ_QUORUM is >= 2 (#1 — bootstrap read is M-of-N)", () => {
  assert.ok(REGISTRY_READ_QUORUM >= 2);
});

test("sameNative: auto-relay only between ETH-native chains (#3)", () => {
  assert.equal(sameNative(8453, 42161), true); // Base + Arbitrum, both ETH
  assert.equal(sameNative(84532, 421614), true); // testnets, both ETH
  assert.equal(sameNative(8453, 137), false); // Polygon (MATIC) — wei compare invalid
  assert.equal(sameNative(137, 8453), false);
});

test("gasPrepaySufficient: covers dest gas iff prepay >= gasPrice*limit (same native)", () => {
  const gasPrice = 100000000n; // 0.1 gwei
  const limit = 250000;
  const required = gasPrice * BigInt(limit); // 25_000_000_000_000 wei
  // exact and over are sufficient; under is not. Accepts string/bigint inputs.
  assert.equal(gasPrepaySufficient(required.toString(), gasPrice, limit), true);
  assert.equal(gasPrepaySufficient(required * 2n, gasPrice, limit), true);
  assert.equal(gasPrepaySufficient(required - 1n, gasPrice, limit), false);
  assert.equal(gasPrepaySufficient(0n, gasPrice, limit), false); // un-prepaid burn -> not relayed
});

test("mapEventToInputs: pulls only identifiers from a chain_event (ignores decoded)", () => {
  const r = mapEventToInputs({
    chain_id: 84532,
    transaction_hash: "0xabc",
    contract_address: "0xToken",
    log_index: 3,
    decoded: { arg1: "0xattacker" }, // deliberately ignored — we re-derive from the log
  });
  assert.equal(r.ok, true);
  assert.equal(r.srcChainId, 84532);
  assert.equal(r.burnTxHash, "0xabc");
  assert.equal(r.srcContract, "0xToken");
  assert.equal(r.logIndex, 3);
});

test("mapEventToInputs: falls back to event.address and rejects incomplete events", () => {
  assert.equal(mapEventToInputs({ chain_id: 1, transaction_hash: "0x1", address: "0xT", log_index: 0 }).ok, true);
  assert.equal(mapEventToInputs(null).ok, false);
  assert.equal(mapEventToInputs({ chain_id: 1, transaction_hash: "0x1" }).ok, false); // no contract/log_index
});
