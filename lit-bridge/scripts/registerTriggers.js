// Register the production relayer: two lit-triggers chain_event triggers, one
// per direction. Each watches BurnInitiated on a BridgeToken and runs the
// bridge action in relay mode — the action re-verifies (M-of-N consensus),
// signs, and broadcasts the mint itself.
//
// This is the ONLY part of the relayer that needs a running lit-triggers
// instance + agent authorization. The relay LOGIC is already proven by
// relay.js (which invokes the same action directly). lit-triggers just calls
// this action automatically when a burn event lands.
//
// Required in .env:
//   LIT_USAGE_API_KEY        (scoped key from setup — sent to lit-triggers)
//   BRIDGE_TOKEN_BASE_SEPOLIA, BRIDGE_TOKEN_ARB_SEPOLIA
//   TRIGGERS_BASE            (e.g. https://triggers.litprotocol.com or http://localhost:8000)
//   TRIGGERS_AGENT_TOKEN     (Bearer token from the lit-triggers agent-auth flow)
//
// Prereqs for the lit-triggers instance:
//   * BASE_SEPOLIA_RPC_URL and ARBITRUM_SEPOLIA_RPC_URL set in its env (the
//     poller resolves RPCs per chain from these — see config.rs CHAIN_SPECS,
//     which now include base-sepolia / arbitrum-sepolia).

const fs = require("fs");
const path = require("path");
const env = require("./_env");

const BUILT_ACTION = path.join(__dirname, "..", "action", "bridgeAction.built.js");
const BURN_EVENT = "BurnInitiated(address,address,uint256,uint256,uint256,uint256)";

async function createTrigger(base, token, body) {
  const res = await fetch(`${base}/api/triggers`, {
    method: "POST",
    headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  const out = await res.json();
  if (!res.ok) throw new Error(`POST /api/triggers -> ${res.status}: ${JSON.stringify(out)}`);
  return out;
}

async function main() {
  env.load();
  for (const k of ["LIT_USAGE_API_KEY", "BRIDGE_TOKEN_BASE_MAINNET", "BRIDGE_TOKEN_ARB_MAINNET", "TRIGGERS_BASE", "TRIGGERS_AGENT_TOKEN"]) {
    if (!process.env[k]) {
      throw new Error(
        `${k} missing. Set TRIGGERS_BASE + TRIGGERS_AGENT_TOKEN after authorizing this ` +
        `machine with lit-triggers (magic-link sign-in + "Authorize agent"). The rest come from setup.`
      );
    }
  }
  const actionCode = fs.readFileSync(BUILT_ACTION, "utf8");
  const base = process.env.TRIGGERS_BASE.replace(/\/$/, "");
  const token = process.env.TRIGGERS_AGENT_TOKEN;

  // default_params carry the relay config; the dispatcher overlays the event.
  // The action resolves the destination from the source token's bridgePartner
  // and caps gas itself, so we no longer pass a tokens map or gasLimit (the
  // action would ignore them). >= 2 distinct registry hosts (action requires).
  const defaultParams = {
    registryRpcUrls: ["https://base-rpc.publicnode.com", "https://1rpc.io/base", "https://gateway.tenderly.co/public/base"],
  };

  const directions = [
    { name: "lit-bridge: Base burns", chain: "base", contract: process.env.BRIDGE_TOKEN_BASE_MAINNET },
    { name: "lit-bridge: Arbitrum burns", chain: "arbitrum", contract: process.env.BRIDGE_TOKEN_ARB_MAINNET },
  ];

  for (const d of directions) {
    const trigger = await createTrigger(base, token, {
      name: d.name,
      kind: "chain_event",
      action_code: actionCode,
      default_params: defaultParams,
      usage_api_key: process.env.LIT_USAGE_API_KEY,
      config: {
        chain: d.chain,
        contract_address: d.contract,
        event_signature: BURN_EVENT,
      },
    });
    console.log(`  ${d.name}: trigger ${trigger.id} (cid ${trigger.action_cid})`);
  }

  console.log("\n✓ Relayer triggers registered. Burns on either chain now auto-mint on the other.");
}

main().catch((err) => {
  console.error("\nregisterTriggers failed:", err.message);
  process.exit(1);
});
