import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { test } from "node:test";

const script = new URL("./stripe-report-slack.mjs", import.meta.url);
const csv = `date,customer_id,wallet_address,email,charges_count,charges_cents,credits_cents,window_start,window_end,identified_requests_count,unattributed_charges_count
2026-09-28,cus_test,0x1234567890abcdef,example@example.invalid,2,125,0,2026-09-23,2026-09-29,1,0
2026-09-29,cus_test,0x1234567890abcdef,example@example.invalid,3,250,0,2026-09-23,2026-09-29,1,1
`;

function run(input, mock) {
  return spawnSync(process.execPath, [
    "--import", `data:text/javascript,${encodeURIComponent(mock)}`,
    script.pathname, "-", "--days", "7",
  ], {
    input,
    encoding: "utf8",
    env: { ...process.env, SLACK_WEBHOOK_URL: "https://example.invalid/webhook" },
  });
}

test("stdin report preserves the digest and posts without logging customer data", () => {
  const result = run(csv, `
    import assert from "node:assert/strict";
    globalThis.fetch = async (url, options) => {
      assert.equal(url, "https://example.invalid/webhook");
      assert.equal(options.method, "POST");
      const { text } = JSON.parse(options.body);
      assert.match(text, /last 7 completed UTC days/);
      assert.ok(text.includes("*$3.75* across *5* billing charges from *1* customer"));
      assert.ok(text.includes("example@example.invalid"));
      assert.ok(text.includes("0x1234…cdef"));
      return { ok: true, text: async () => "ok" };
    };
  `);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stdout, "");
  assert.equal(result.stderr, "Posted Stripe usage digest to Slack.\n");
});

test("empty input from a failed producer does not post a digest", () => {
  const result = run("", `globalThis.fetch = () => { console.log("unexpected POST"); };`);
  assert.equal(result.status, 1);
  assert.equal(result.stdout, "");
  assert.match(result.stderr, /CSV missing expected column/);
});

test("Slack failure fails the command without logging the response body", () => {
  const result = run(csv, `
    globalThis.fetch = async () => ({
      ok: false, status: 500, text: async () => "example@example.invalid",
    });
  `);
  assert.equal(result.status, 1);
  assert.equal(result.stdout, "");
  assert.match(result.stderr, /HTTP 500/);
  assert.ok(!result.stderr.includes("example@example.invalid"));
});


test("window comes from metadata, including zero-activity days", () => {
  const result = run(csv, `
    import assert from "node:assert/strict";
    globalThis.fetch = async (_url, options) => {
      const { text } = JSON.parse(options.body);
      assert.ok(text.includes("2026-09-23 → 2026-09-29 UTC"));
      assert.ok(text.includes("not API requests"));
      assert.ok(text.includes("*2* distinct billed request IDs; *1* charges without request IDs"));
      assert.ok(!text.includes(" calls"));
      return { ok: true, text: async () => "ok" };
    };
  `);
  assert.equal(result.status, 0, result.stderr);
});

test("empty window still reports the complete requested range", () => {
  const input = csv.split("\n")[0] + "\n,,,,0,0,0,2026-09-23,2026-09-29,0,0\n";
  const result = run(input, `
    import assert from "node:assert/strict";
    globalThis.fetch = async (_url, options) => {
      const { text } = JSON.parse(options.body);
      assert.ok(text.includes("2026-09-23 → 2026-09-29 UTC"));
      assert.ok(text.includes("No billable usage"));
      return { ok: true, text: async () => "ok" };
    };
  `);
  assert.equal(result.status, 0, result.stderr);
});

test("malformed or inconsistent CSV never posts", () => {
  for (const input of [
    csv.replace(",2,125,", ",invalid,125,"),
    csv.replace("2026-09-28,", "2026-09-30,"),
    csv.replace("2026-09-23", "2026-09-24"),
    csv.replaceAll("2026-09-23", "2026-09-24"),
    csv.split("\n")[0] + "\n",
    csv + '\n"unterminated',
  ]) {
    const result = run(input, `globalThis.fetch = () => { console.log("unexpected POST"); };`);
    assert.equal(result.status, 1);
    assert.equal(result.stdout, "");
  }
});

test("customer text cannot inject Slack mentions or links", () => {
  const input = csv.replaceAll("example@example.invalid", "<!channel><https://example.invalid|link>");
  const result = run(input, `
    import assert from "node:assert/strict";
    globalThis.fetch = async (_url, options) => {
      const { text } = JSON.parse(options.body);
      assert.ok(!text.includes("<!channel>"));
      assert.ok(text.includes("&lt;!channel&gt;"));
      return { ok: true, text: async () => "ok" };
    };
  `);
  assert.equal(result.status, 0, result.stderr);
});
