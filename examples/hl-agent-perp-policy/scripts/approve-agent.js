// Venue-side connect step, run by the USER with their testnet master key:
// approve the action's CID-bound agent address for trade-only powers.
//
//   1. Derive the agent address by RUNNING the action's "address" branch —
//      the address comes from the TEE, live, not from local bookkeeping.
//   2. Sign `approveAgent` with the master key (HL_MASTER_PRIVATE_KEY in
//      .env) from Node, using lit-venues' ESM build directly. This is the
//      ONLY thing the master key is ever used for here.
//
// After this, the action can sign orders and cancels for the master account,
// and Hyperliquid structurally bars it from withdrawals and transfers —
// the venue enforces the custody boundary, not just our policy. The master
// can revoke the named agent ("lit-policy") venue-side at any time.
//
// Requires a FUNDED testnet master (https://app.hyperliquid-testnet.xyz/drip):
// Hyperliquid rejects actions from accounts it has never seen funds for.
//
// Usage: npm run approve-agent

const path = require("path");
const { pathToFileURL } = require("url");
const env = require("./_env");
const { runAction } = require("./_lit");
env.load();

// The scripts run in Node, so they may import the ESM build directly —
// unlike action code, which gets the IIFE bundle concatenated by _lit.js.
const MJS_PATH = path.join(
  __dirname,
  "..",
  "..",
  "..",
  "lit-venues",
  "dist",
  "lit-venues.mjs"
);

const AGENT_NAME = "lit-policy"; // named agents are auditable + individually revocable

async function main() {
  const masterKey = process.env.HL_MASTER_PRIVATE_KEY;
  if (!masterKey || !/^(0x)?[0-9a-fA-F]{64}$/.test(masterKey)) {
    throw new Error(
      "HL_MASTER_PRIVATE_KEY is required in .env (32-byte hex). TESTNET ONLY — see README."
    );
  }
  const LitVenues = await import(pathToFileURL(MJS_PATH).href);

  // ---- 1. The agent address, fresh from the TEE ---------------------------
  const { agentAddress } = await runAction({ action: "address" });
  if (!agentAddress) throw new Error("action did not return an agent address");
  const recorded = process.env.AGENT_ADDRESS;
  if (recorded && recorded.toLowerCase() !== agentAddress.toLowerCase()) {
    console.warn(
      `  WARNING: agent address changed (${recorded} -> ${agentAddress}).\n` +
        "  The action source or the lit-venues bundle changed, so the CID — and\n" +
        "  the CID-bound key — changed with it. Approving the NEW address; the\n" +
        "  old approval now matches nothing that can sign, but revoke it\n" +
        "  venue-side anyway to keep the agent list clean."
    );
  }
  env.upsert("AGENT_ADDRESS", agentAddress);
  console.log(`Agent address (CID-bound, key lives only in the TEE): ${agentAddress}`);

  // ---- 2. Master signs the approval (testnet) ------------------------------
  const masterAddress = LitVenues.privateKeyToAddress(masterKey);
  env.upsert("HL_MASTER_ADDRESS", masterAddress);
  console.log(`Master account: ${masterAddress}`);

  const master = LitVenues.createVenue({
    venueId: "hyperliquid",
    sandbox: true,
    credentials: { keyType: "pkp-eip712", privateKey: masterKey },
  });
  console.log(`Approving agent "${AGENT_NAME}" on the Hyperliquid testnet...`);
  await master.approveAgent({ agentAddress, agentName: AGENT_NAME });

  console.log("\n✓ Agent approved.");
  console.log("  The action can now sign orders/cancels for the master account.");
  console.log("  It cannot withdraw or transfer — Hyperliquid bars agents from that");
  console.log("  by construction. Revoke anytime from the master (testnet app -> API).");
  console.log("\nTry it out:");
  console.log("  npm run trade -- buy 0.01 ETH            # market, 1x");
  console.log("  npm run trade -- buy 0.01 ETH 2000 3x    # GTC limit @ 2000, 3x");
  console.log("  npm run trade -- buy 1 BTC               # watch the notional fence refuse");
}

main().catch((err) => {
  console.error(err.message || err);
  if (/deposit|exist|account/i.test(String(err.message))) {
    console.error(
      "\nHint: the master must be FUNDED on the testnet before it can sign actions —\n" +
        "claim mock-USDC at https://app.hyperliquid-testnet.xyz/drip first."
    );
  }
  process.exit(1);
});
