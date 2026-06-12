// Lit Action — phase 2 of the CEX → self-custody sweep (plan D6).
//
// Like phase 1, this runs with the lit-venues IIFE bundle concatenated above
// it by scripts/_lit.js (global `LitVenues`); `Lit.Actions` is the runtime.
//
// checkEmailApproval is the heart of the example. The attestation produced by
// the approval server is verified by the action runtime IN-TEE before
// `approved: true` is reported, against TWO things:
//   1. the pinned network attestation key (signature) — so a compromised
//      *Flows* layer or transport cannot forge an approval; and
//   2. the `requestHash` of THIS exact sweep — so a valid approval for one
//      operation cannot be replayed to authorize a different asset/amount/
//      destination (this binding is what makes the gate meaningful).
// The approval is also single-use: checkEmailApproval consumes it, so the same
// approval can't drive two sweeps. (A compromise of the approval *service*
// itself, which holds the signing key, is out of scope for in-TEE verification
// — see the trust-boundary note in lit-api-server/src/approvals.rs.)
//
// On `approved: true`, the demo performs the swept-funds step it can honestly
// perform: it re-verifies the venue balance still covers the intent and
// returns the attestation as the auditable record of the human approval.
// In production THIS is the spot where the action signs the venue's
// withdrawal request or an on-chain transfer through the same policy.
// lit-venues v1 deliberately exposes no withdrawal endpoints (plan non-goal:
// policy-gated sweeps go through this approval primitive first).
//
// On `pending`, it returns { swept: false, status: "pending" } — re-invoke
// later; in production a lit-triggers approval-completed webhook re-invokes
// this phase automatically.
//
// js_params:
//   approvalId    from phase 1 (apr_<32 hex>)
//   venueApiKey   Binance spot-testnet API key
//   venueSecret   Binance spot-testnet secret
//   proxyUrl      optional egress proxy (plan D4)
//   asset         the intent being completed — same values phase 1 put in
//   amount        the human-approved summary
//   destination

// Identical to phase 1's hash — same fields, same order. Re-derived here from
// the params phase 2 is about to act on, so the approval is bound to exactly
// what THIS invocation would do, not to whatever was merely displayed earlier.
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
  const { approvalId, venueApiKey, venueSecret, proxyUrl, asset, amount, destination } =
    params || {};
  if (typeof approvalId !== "string" || !/^apr_[0-9a-f]{32}$/.test(approvalId)) {
    return { swept: false, status: "invalid", reason: "approvalId must look like apr_<32 hex>" };
  }
  if (typeof amount !== "string" || !/^\d+(\.\d+)?$/.test(amount) || Number(amount) <= 0) {
    return { swept: false, status: "invalid", reason: "amount must be a positive decimal string" };
  }
  for (const [name, value] of [["asset", asset], ["destination", destination]]) {
    if (!value || typeof value !== "string") {
      return { swept: false, status: "invalid", reason: `missing required param "${name}"` };
    }
  }

  // ---- 1. Check the approval. The runtime verifies the attestation IN-TEE
  // against the pinned key AND binds it to the requestHash of THIS exact sweep
  // (recomputed from the params we're about to act on). A mismatch — a replay
  // of an approval for a different operation — fails closed right here.
  const requestHash = sweepRequestHash({
    venueId: "binance",
    sandbox: true,
    asset,
    amount,
    destination,
  });
  let approval;
  try {
    approval = await Lit.Actions.checkEmailApproval({ approvalId, requestHash });
  } catch (e) {
    // Binding/verification failure (wrong operation, bad signature, expired).
    return { swept: false, status: "rejected", reason: String((e && e.message) || e).slice(0, 200) };
  }
  if (!approval.approved) {
    // pending → try again later (or let a lit-triggers webhook resume);
    // denied / expired / consumed → the sweep is dead, a fresh phase 1 is required.
    return { swept: false, status: approval.status };
  }

  // ---- 2. Approved. Perform the gated step. --------------------------------
  // Demo: confirm the balance still covers the human-approved intent and put
  // the attestation on record. Production: sign the withdrawal / transfer here.
  const venue = LitVenues.createVenue({
    venueId: "binance",
    sandbox: true,
    credentials: { apiKey: venueApiKey, secret: venueSecret, keyType: "hmac" },
    proxy: proxyUrl || undefined,
  });
  const balances = await venue.fetchBalances();
  const bal = balances.find((b) => b.asset === asset);
  const free = bal ? bal.free : "0";
  if (LitVenues.subDec(free, amount).startsWith("-")) {
    return {
      swept: false,
      status: approval.status,
      reason: `approved, but free ${asset} balance (${free}) no longer covers ${amount}`,
      free,
    };
  }

  return {
    swept: true,
    status: approval.status,
    intent: { venueId: "binance", sandbox: true, asset, amount, destination },
    free,
    approver: approval.approver ?? null,
    assurance: approval.assurance ?? null,
    approvedAtMs: approval.approvedAtMs ?? null,
    // The in-TEE-verified attestation — export it as the audit record of who
    // approved what, at which assurance level, and when.
    attestation: approval.attestation ?? null,
  };
}
