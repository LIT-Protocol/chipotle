// Lit Action — phase 1 of a CEX → self-custody sweep (plan D6 two-phase pattern).
//
// This file is NOT submitted alone: scripts/_lit.js concatenates the prebuilt
// lit-venues IIFE bundle (../../lit-venues/dist/lit-venues.iife.js) above it,
// which defines the global `LitVenues`. `Lit.Actions` comes from the runtime.
// The action's identity (its IPFS CID) covers bundle + this source together.
//
// What this phase does:
//   1. Connects to the venue (Binance spot testnet) with caller-supplied
//      credentials and reads balances — one signed fetch.
//   2. POLICY: refuses outright if the free balance doesn't cover the sweep.
//   3. Requests a human approval at assurance L2 (link click + OTP) with a
//      summary describing the exact intent, then EXITS. Actions are
//      request-scoped, so approval is two-phase by design: a later invocation
//      of completeSweep.js checks the attestation and proceeds.
//
// The assurance level and TTL live in POLICY below — part of the hashed
// source, so a caller cannot downgrade "L2" to a bare link click without
// producing a different action CID.
//
// js_params:
//   probe          true → return { ready } without side effects (setup poll)
//   venueApiKey    Binance spot-testnet API key   (demo: js_params; prod: sealed)
//   venueSecret    Binance spot-testnet secret
//   proxyUrl       optional egress proxy (Binance 451s US egress; plan D4)
//   approverEmail  who must approve
//   asset          e.g. "USDT"
//   amount         decimal string, e.g. "100"
//   destination    self-custody address the sweep is destined for

const POLICY = {
  venueId: "binance",
  sandbox: true, // spot testnet — this demo never touches a live venue
  assurance: "L2", // email is the notification channel; the OTP authenticates
  ttlSec: 900, // approval expires after 15 minutes
};

// Canonical hash of the EXACT operation being approved. Both phases compute it
// identically; phase 1 binds it into the approval, phase 2 proves it matches.
// This is what stops a valid "100 USDT to X" approval from being replayed to
// authorize a different asset/amount/destination (codex P1). Field order is
// fixed — it's hashed, so any drift changes the hash and fails the binding.
function sweepRequestHash({ venueId, sandbox, asset, amount, destination }) {
  const canonical = JSON.stringify({
    v: "cex-sweep-v1",
    venueId,
    sandbox,
    asset,
    amount,
    destination,
  });
  return LitVenues.sha256Hex(canonical);
}

async function main(params) {
  if (params && params.probe) {
    // Side-effect-free readiness probe: proves the bundle parsed and runs.
    return { ready: true, litVenuesVersion: LitVenues.VERSION };
  }

  const { venueApiKey, venueSecret, proxyUrl, approverEmail, asset, amount, destination } =
    params || {};
  for (const [name, value] of [
    ["venueApiKey", venueApiKey],
    ["venueSecret", venueSecret],
    ["approverEmail", approverEmail],
    ["asset", asset],
    ["destination", destination],
  ]) {
    if (!value || typeof value !== "string") {
      return { requested: false, reason: `missing required param "${name}"` };
    }
  }
  if (typeof amount !== "string" || !/^\d+(\.\d+)?$/.test(amount) || Number(amount) <= 0) {
    return { requested: false, reason: "amount must be a positive decimal string" };
  }

  // ---- 1. Read venue balances (1 fetch, well inside the action quota) -----
  const venue = LitVenues.createVenue({
    venueId: POLICY.venueId,
    sandbox: POLICY.sandbox,
    credentials: { apiKey: venueApiKey, secret: venueSecret, keyType: "hmac" },
    proxy: proxyUrl || undefined, // routes via Lit.Actions.proxiedFetch when set
  });
  const balances = await venue.fetchBalances();
  const bal = balances.find((b) => b.asset === asset);
  const free = bal ? bal.free : "0";

  // ---- 2. The policy decision (exact decimal math — no float drift) -------
  if (LitVenues.subDec(free, amount).startsWith("-")) {
    return {
      requested: false,
      reason: `free ${asset} balance (${free}) does not cover the sweep of ${amount}`,
      free,
    };
  }

  // ---- 3. Ask the human, then exit -----------------------------------------
  // lit-venues exposes no withdrawal endpoints by design (plan non-goal):
  // the value-moving step is what phase 2 gates behind the verified approval.
  const summary = `Sweep ${amount} ${asset} from ${POLICY.venueId} (spot testnet) to ${destination}`;
  const requestHash = sweepRequestHash({
    venueId: POLICY.venueId,
    sandbox: POLICY.sandbox,
    asset,
    amount,
    destination,
  });
  const approval = await Lit.Actions.requestEmailApproval({
    to: approverEmail,
    summary,
    assurance: POLICY.assurance,
    ttlSec: POLICY.ttlSec,
    // Binds this approval to THIS exact sweep; phase 2 re-derives and the
    // runtime verifies the match in-TEE.
    requestHash,
  });

  return {
    requested: true,
    approvalId: approval.approvalId,
    requestHash,
    // L2: the OTP must reach the approver OUT-OF-BAND via the requesting app
    // (the script prints it). It is never included in the approval email.
    otp: approval.otp ?? null,
    // Only present on dev servers running with LIT_APPROVAL_EXPOSE_LINK;
    // otherwise the link travels exclusively in the approver's email.
    approvalUrl: approval.approvalUrl ?? null,
    summary,
    free,
    ttlSec: POLICY.ttlSec,
  };
}
