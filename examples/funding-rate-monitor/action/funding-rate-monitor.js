// Lit Action: cross-venue funding/basis monitor with an email alert.
//
// The lit-venues IIFE bundle is concatenated ABOVE this file by
// scripts/_lit.js (global `LitVenues`). Zero credentials anywhere: both legs
// are public data, and the alert goes out via the server-mediated
// Lit.Actions.sendEmail op (fixed from-domain, per-account quota, plain text).
//
// Per coin:
//   - Hyperliquid perp funding (one leg): fetchFundingRate -> hourly rate.
//     Funding accrues hourly, so annualized % = rate x 24 x 365 x 100.
//   - Coinbase spot (basis reference): fetchTicker COIN/USD.
//   - basis % = (HL mark - Coinbase spot) / spot x 100.
// If |annualized funding| > thresholdPct for any coin, ONE email with the
// full plain-text table goes to alertTo.
//
// Quota discipline: 2 outbound fetches per coin; coins are capped at 4
// (<= 8 fetches, inside this repo's <=10-per-action example budget and far
// inside the runtime's 50-fetch cap).
//
// Float math is fine HERE: this action only displays rates and never places
// an order (lit-venues keeps every order path in exact decimal strings).
//
// js_params (all optional except alertTo if you want the email):
//   coins         array of Hyperliquid coin names, default ["BTC","ETH"];
//                 case-sensitive (e.g. "kPEPE"), capped at 4
//   thresholdPct  alert when |annualized funding %| exceeds this, default 20
//   alertTo       email recipient; without it the action only reports
//   sandbox       true -> Hyperliquid testnet (funding exists there too)
//
// Returns { ts, thresholdPct, rows, alerted }.

async function main(params) {
  params = params || {};
  const coins = (Array.isArray(params.coins) && params.coins.length ? params.coins : ["BTC", "ETH"])
    .filter((c) => typeof c === "string" && c.trim().length > 0)
    .map((c) => c.trim())
    .slice(0, 4); // quota cap — see header
  const thresholdPct = Number.isFinite(Number(params.thresholdPct)) ? Number(params.thresholdPct) : 20;
  const alertTo =
    typeof params.alertTo === "string" && params.alertTo.includes("@") ? params.alertTo : null;

  const hl = LitVenues.createVenue({ venueId: "hyperliquid", sandbox: params.sandbox === true });
  const coinbase = LitVenues.createVenue({ venueId: "coinbase" });

  const rows = [];
  for (const coin of coins) {
    const row = {
      coin,
      fundingHourly: null,
      fundingAnnualizedPct: null,
      hlMark: null,
      spotUsd: null,
      basisPct: null,
      alert: false,
    };

    try {
      const fr = await hl.fetchFundingRate(coin); // 1 fetch — hourly rate as a decimal string
      row.fundingHourly = fr.fundingRate;
      row.fundingAnnualizedPct = round2(Number(fr.fundingRate) * 24 * 365 * 100);
      row.hlMark = fr.markPrice != null ? Number(fr.markPrice) : null;
    } catch (e) {
      row.hlError = errInfo(e);
    }

    try {
      const t = await coinbase.fetchTicker(`${coin}/USD`); // 1 fetch — spot basis reference
      row.spotUsd = t.last;
    } catch (e) {
      row.spotError = errInfo(e); // e.g. bad_symbol: no Coinbase USD product for this coin
    }

    if (row.hlMark != null && row.spotUsd != null && row.spotUsd > 0) {
      row.basisPct = round2(((row.hlMark - row.spotUsd) / row.spotUsd) * 100);
    }
    row.alert = row.fundingAnnualizedPct != null && Math.abs(row.fundingAnnualizedPct) > thresholdPct;
    rows.push(row);
  }

  const alerting = rows.filter((r) => r.alert);
  let alerted = false;
  if (alerting.length > 0 && alertTo) {
    const subject = `funding-rate-monitor: ${alerting.map((r) => r.coin).join(", ")} annualized funding beyond ${thresholdPct}%`;
    const { accepted } = await Lit.Actions.sendEmail({
      to: alertTo,
      subject,
      text: renderTable(rows, thresholdPct),
    });
    alerted = accepted === true;
  }

  return { ts: Date.now(), thresholdPct, rows, alerted };
}

function round2(x) {
  return Math.round(x * 100) / 100;
}

function errInfo(e) {
  return {
    code: (e && e.code) || "unknown",
    message: String((e && e.message) || e).slice(0, 160),
  };
}

// Plain-text table (sendEmail is deliberately text-only — no HTML to spoof).
function renderTable(rows, thresholdPct) {
  const pad = (s, n) => String(s == null ? "-" : s).padEnd(n);
  const lines = [
    `Funding monitor @ ${new Date().toISOString()}`,
    `Alert threshold: |annualized| > ${thresholdPct}%`,
    "",
    pad("COIN", 8) + pad("FUNDING/HR", 13) + pad("ANNUALIZED%", 13) + pad("HL MARK", 12) + pad("CB SPOT", 12) + "BASIS%",
  ];
  for (const r of rows) {
    lines.push(
      pad(r.coin + (r.alert ? "*" : ""), 8) +
        pad(r.fundingHourly, 13) +
        pad(r.fundingAnnualizedPct, 13) +
        pad(r.hlMark, 12) +
        pad(r.spotUsd, 12) +
        (r.basisPct == null ? "-" : r.basisPct)
    );
    if (r.hlError) lines.push(`  ${r.coin} hyperliquid error [${r.hlError.code}]: ${r.hlError.message}`);
    if (r.spotError) lines.push(`  ${r.coin} coinbase error [${r.spotError.code}]: ${r.spotError.message}`);
  }
  lines.push("", "* beyond threshold");
  return lines.join("\n");
}
