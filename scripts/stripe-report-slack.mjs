#!/usr/bin/env node
// Post a Stripe usage digest to Slack from a stripe_report CSV.
//
// Reads the CSV produced by `cargo run --bin stripe_report` (columns:
// date,customer_id,wallet_address,email,charges_count,charges_cents,credits_cents,window_start,window_end,identified_requests_count,unattributed_charges_count),
// aggregates per customer across the whole window, and POSTs a top-spenders
// summary to the Slack incoming webhook in $SLACK_WEBHOOK_URL.
//
// Usage:
//   SLACK_WEBHOOK_URL=https://hooks.slack.com/...  \
//   node scripts/stripe-report-slack.mjs <report.csv|-> [--days N] [--top N] [--dry-run]
//
// Use - to read CSV from stdin without saving a report file.
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
  if (!args.csv) throw new Error("usage: stripe-report-slack.mjs <report.csv|-> [--days N] [--top N] [--dry-run]");
  if (args.days !== null && (!Number.isSafeInteger(args.days) || args.days < 1 || args.days > 3660)) throw new Error("--days must be an integer between 1 and 3660");
  if (!Number.isSafeInteger(args.top) || args.top < 1) throw new Error("--top must be a positive integer");
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
  if (inQuotes) throw new Error("CSV contains an unterminated quoted field");
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

function validDate(value) {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) return false;
  const timestamp = Date.parse(value);
  return Number.isFinite(timestamp) && new Date(timestamp).toISOString().slice(0, 10) === value;
}

function unsignedInteger(value) {
  if (!/^\d+$/.test(value) || !Number.isSafeInteger(Number(value))) throw new Error("CSV contains invalid charge data");
  return Number(value);
}

function escapeSlack(value) {
  return value.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;").replaceAll("`", "'").replace(/[\r\n]/g, " ");
}

function aggregate(rows) {
  const header = rows[0] || [];
  const idx = Object.fromEntries(header.map((h, i) => [h.trim(), i]));
  for (const col of ["date", "customer_id", "charges_count", "charges_cents", "window_start", "window_end", "identified_requests_count", "unattributed_charges_count"]) {
    if (idx[col] === undefined) throw new Error(`CSV missing expected column: ${col}`);
  }
  const byCustomer = new Map();
  let windowStart;
  let windowEnd;
  for (const r of rows.slice(1)) {
    if (r.length === 1 && r[0] === "") continue; // blank line
    if (r.length !== header.length) throw new Error("CSV row has the wrong number of fields");
    const start = r[idx.window_start];
    const end = r[idx.window_end];
    if (!validDate(start) || !validDate(end) || start > end) throw new Error("CSV has invalid window bounds");
    if (windowStart && (windowStart !== start || windowEnd !== end)) throw new Error("CSV has inconsistent window bounds");
    windowStart = start;
    windowEnd = end;
    const cents = unsignedInteger(r[idx.charges_cents]);
    const count = unsignedInteger(r[idx.charges_count]);
    const requests = unsignedInteger(r[idx.identified_requests_count]);
    const unattributed = unsignedInteger(r[idx.unattributed_charges_count]);
    if (requests + unattributed > count) throw new Error("CSV request counts exceed charge counts");
    const id = r[idx.customer_id];
    if (!id) {
      if (cents !== 0 || count !== 0 || r[idx.date]) throw new Error("CSV has invalid empty-window row");
      continue;
    }
    const date = r[idx.date];
    if (!validDate(date) || date < start || date > end) throw new Error("CSV activity is outside the window");
    const cur = byCustomer.get(id) || {
      id,
      wallet: r[idx.wallet_address] || "",
      email: r[idx.email] || "",
      cents: 0,
      count: 0,
      requests: 0,
      unattributed: 0,
    };
    cur.cents += cents;
    cur.count += count;
    cur.requests += requests;
    cur.unattributed += unattributed;
    if (!Number.isSafeInteger(cur.cents) || !Number.isSafeInteger(cur.count)) throw new Error("CSV totals exceed safe integer range");
    if (!cur.wallet && r[idx.wallet_address]) cur.wallet = r[idx.wallet_address];
    if (!cur.email && r[idx.email]) cur.email = r[idx.email];
    byCustomer.set(id, cur);
  }
  const customers = [...byCustomer.values()]
    .filter((c) => c.cents !== 0 || c.count !== 0)
    .sort((a, b) => b.cents - a.cents || b.count - a.count);
  const totalCents = customers.reduce((s, c) => s + c.cents, 0);
  const totalCount = customers.reduce((s, c) => s + c.count, 0);
  if (!windowStart) throw new Error("CSV has no window metadata");
  if (!Number.isSafeInteger(totalCents) || !Number.isSafeInteger(totalCount)) throw new Error("CSV totals exceed safe integer range");
  return {
    customers,
    totalCents,
    totalCount,
    totalRequests: customers.reduce((s, c) => s + c.requests, 0),
    totalUnattributed: customers.reduce((s, c) => s + c.unattributed, 0),
    firstDate: windowStart,
    lastDate: windowEnd,
  };
}

function buildMessage(agg, { days, top }) {
  const actualDays = (Date.parse(agg.lastDate) - Date.parse(agg.firstDate)) / 86_400_000 + 1;
  if (days !== null && days !== actualDays) throw new Error("--days does not match the CSV window");
  const window = `last ${actualDays} completed UTC day${actualDays === 1 ? "" : "s"}`;
  const range = ` (${agg.firstDate} → ${agg.lastDate} UTC)`;
  const lines = [];
  lines.push(`*📊 Stripe usage — ${window}*${range}`);

  if (agg.customers.length === 0) {
    lines.push("");
    lines.push("_No billable usage recorded in this window._");
    return lines.join("\n");
  }

  lines.push(
    `*${centsToUsd(agg.totalCents)}* across *${agg.totalCount}* billing charge${agg.totalCount === 1 ? "" : "s"} ` +
      `from *${agg.customers.length}* customer${agg.customers.length === 1 ? "" : "s"}`,
  );
  lines.push(`*${agg.totalRequests}* distinct billed request IDs; *${agg.totalUnattributed}* charges without request IDs.`);
  lines.push("_Counts are settled billing charges, not API requests; one execution can create several charges._");
  lines.push("");

  const shown = agg.customers.slice(0, top);
  shown.forEach((c, i) => {
    const wallet = escapeSlack(shortWallet(c.wallet));
    const label = wallet ? `\`${wallet}\`` : `\`${escapeSlack(c.id)}\``;
    const email = c.email ? ` (${escapeSlack(c.email)})` : "";
    lines.push(
      `${i + 1}. ${label}${email} — ${centsToUsd(c.cents)} · ${c.count} billing charge${c.count === 1 ? "" : "s"} · ${c.requests} identified billed requests · ${c.unattributed} charges without IDs`,
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
    throw new Error(`Slack webhook returned an unsuccessful response (HTTP ${res.status})`);
  }
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const csv = readFileSync(args.csv === "-" ? 0 : args.csv, "utf8");
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
  console.error("Posted Stripe usage digest to Slack.");
}

main().catch((e) => {
  console.error(`error: ${e.message}`);
  process.exit(1);
});
