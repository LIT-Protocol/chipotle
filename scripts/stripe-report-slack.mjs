#!/usr/bin/env node
// Post a Stripe usage digest to Slack from a stripe_report CSV.
//
// Reads the CSV produced by `cargo run --bin stripe_report` (columns:
// date,customer_id,wallet_address,email,charges_count,charges_cents,credits_cents),
// aggregates per customer across the whole window, and POSTs a top-spenders
// summary to the Slack incoming webhook in $SLACK_WEBHOOK_URL.
//
// Usage:
//   SLACK_WEBHOOK_URL=https://hooks.slack.com/...  \
//   node scripts/stripe-report-slack.mjs <report.csv> [--days N] [--top N] [--dry-run]
//
// --dry-run prints the payload to stdout instead of posting (no webhook needed).

import { readFileSync } from "node:fs";

function parseArgs(argv) {
  const args = { csv: null, days: null, top: 15, dryRun: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--days") args.days = Number(argv[++i]);
    else if (a === "--top") args.top = Number(argv[++i]);
    else if (a === "--dry-run") args.dryRun = true;
    else if (!a.startsWith("--") && args.csv === null) args.csv = a;
    else throw new Error(`unexpected argument: ${a}`);
  }
  if (!args.csv) throw new Error("usage: stripe-report-slack.mjs <report.csv> [--days N] [--top N] [--dry-run]");
  if (args.days !== null && !Number.isFinite(args.days)) throw new Error("--days must be a number");
  if (!Number.isFinite(args.top) || args.top < 1) throw new Error("--top must be a positive number");
  return args;
}

// Minimal RFC-4180 CSV parser: handles quoted fields with embedded commas,
// quotes ("" escape), and newlines. Returns an array of row arrays.
function parseCsv(text) {
  const rows = [];
  let row = [];
  let field = "";
  let inQuotes = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (inQuotes) {
      if (c === '"') {
        if (text[i + 1] === '"') { field += '"'; i++; }
        else inQuotes = false;
      } else field += c;
    } else if (c === '"') {
      inQuotes = true;
    } else if (c === ",") {
      row.push(field); field = "";
    } else if (c === "\n") {
      row.push(field); field = ""; rows.push(row); row = [];
    } else if (c === "\r") {
      // swallow; \n handles the row break
    } else field += c;
  }
  // flush trailing field/row if the file didn't end on a newline
  if (field.length > 0 || row.length > 0) { row.push(field); rows.push(row); }
  return rows;
}

function centsToUsd(cents) {
  const neg = cents < 0;
  const v = Math.abs(cents);
  const s = `$${Math.floor(v / 100)}.${String(v % 100).padStart(2, "0")}`;
  return neg ? `-${s}` : s;
}

// A leading 0x wallet shown compact: 0x1234…abcd
function shortWallet(w) {
  if (!w || !w.startsWith("0x") || w.length < 12) return w || "";
  return `${w.slice(0, 6)}…${w.slice(-4)}`;
}

function aggregate(rows) {
  const header = rows[0] || [];
  const idx = Object.fromEntries(header.map((h, i) => [h.trim(), i]));
  for (const col of ["customer_id", "charges_count", "charges_cents"]) {
    if (idx[col] === undefined) throw new Error(`CSV missing expected column: ${col}`);
  }
  const byCustomer = new Map();
  const dates = new Set();
  for (const r of rows.slice(1)) {
    if (r.length === 1 && r[0] === "") continue; // blank line
    const id = r[idx.customer_id];
    if (!id) continue;
    const date = r[idx.date];
    if (date) dates.add(date);
    const cents = Number(r[idx.charges_cents] || 0);
    const count = Number(r[idx.charges_count] || 0);
    const cur = byCustomer.get(id) || {
      id,
      wallet: r[idx.wallet_address] || "",
      email: r[idx.email] || "",
      cents: 0,
      count: 0,
    };
    cur.cents += cents;
    cur.count += count;
    if (!cur.wallet && r[idx.wallet_address]) cur.wallet = r[idx.wallet_address];
    if (!cur.email && r[idx.email]) cur.email = r[idx.email];
    byCustomer.set(id, cur);
  }
  const customers = [...byCustomer.values()]
    .filter((c) => c.cents !== 0 || c.count !== 0)
    .sort((a, b) => b.cents - a.cents || b.count - a.count);
  const totalCents = customers.reduce((s, c) => s + c.cents, 0);
  const totalCount = customers.reduce((s, c) => s + c.count, 0);
  const sortedDates = [...dates].sort();
  return {
    customers,
    totalCents,
    totalCount,
    firstDate: sortedDates[0] || null,
    lastDate: sortedDates[sortedDates.length - 1] || null,
  };
}

function buildMessage(agg, { days, top }) {
  const window = days ? `last ${days} day${days === 1 ? "" : "s"}` : "window";
  const range = agg.firstDate ? ` (${agg.firstDate} → ${agg.lastDate} UTC)` : "";
  const lines = [];
  lines.push(`*📊 Stripe usage — ${window}*${range}`);

  if (agg.customers.length === 0) {
    lines.push("");
    lines.push("_No billable usage recorded in this window._");
    return lines.join("\n");
  }

  lines.push(
    `*${centsToUsd(agg.totalCents)}* across *${agg.totalCount}* call${agg.totalCount === 1 ? "" : "s"} ` +
      `from *${agg.customers.length}* customer${agg.customers.length === 1 ? "" : "s"}`,
  );
  lines.push("");

  const shown = agg.customers.slice(0, top);
  shown.forEach((c, i) => {
    const wallet = shortWallet(c.wallet);
    const label = wallet ? `\`${wallet}\`` : `\`${c.id}\``;
    const email = c.email ? ` (${c.email})` : "";
    lines.push(
      `${i + 1}. ${label}${email} — ${centsToUsd(c.cents)} · ${c.count} call${c.count === 1 ? "" : "s"}`,
    );
  });

  const remaining = agg.customers.length - shown.length;
  if (remaining > 0) {
    const restCents = agg.customers.slice(top).reduce((s, c) => s + c.cents, 0);
    lines.push(`_…and ${remaining} more (${centsToUsd(restCents)})_`);
  }
  return lines.join("\n");
}

async function post(webhook, text) {
  const res = await fetch(webhook, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ text }),
  });
  const body = await res.text();
  if (!res.ok || body !== "ok") {
    throw new Error(`Slack webhook returned ${res.status}: ${body}`);
  }
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const csv = readFileSync(args.csv, "utf8");
  const rows = parseCsv(csv);
  const agg = aggregate(rows);
  const text = buildMessage(agg, { days: args.days, top: args.top });

  if (args.dryRun) {
    console.log(text);
    return;
  }
  const webhook = process.env.SLACK_WEBHOOK_URL;
  if (!webhook) throw new Error("SLACK_WEBHOOK_URL is not set");
  await post(webhook, text);
  console.error(`Posted Stripe usage digest to Slack (${agg.customers.length} customers, ${centsToUsd(agg.totalCents)}).`);
}

main().catch((e) => {
  console.error(`error: ${e.message}`);
  process.exit(1);
});
