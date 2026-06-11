// Lit Action — phase 2 of the CEX → self-custody sweep (plan D6).
//
// Like phase 1, this runs with the lit-venues IIFE bundle concatenated above
// it by scripts/_lit.js (global `LitVenues`); `Lit.Actions` is the runtime.
//
// checkEmailApproval is the heart of the example: the attestation produced by
// the approval server is verified by the action runtime IN-TEE against the
// pinned network attestation key before `approved: true` is ever reported.
// That means a compromised approval server (or a compromised caller of this
// action) cannot forge an approval — the worst either can do is fail to
// deliver one. The action only has to branch on `approved`.
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

async function main(params) {
  const { approvalId, venueApiKey, venueSecret, proxyUrl, asset, amount, destination } =
    params || {};
  if (typeof approvalId !== "string" || !/^apr_[0-9a-f]{32}$/.test(approvalId)) {
    return { swept: false, status: "invalid", reason: "approvalId must look like apr_<32 hex>" };
  }

  // ---- 1. Check the approval; the runtime verifies the attestation in-TEE --
  const approval = await Lit.Actions.checkEmailApproval({ approvalId });
  if (!approval.approved) {
    // pending → try again later (or let a lit-triggers webhook resume);
    // denied / expired → the sweep is dead, a fresh phase 1 is required.
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
