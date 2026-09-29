import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { test } from "node:test";

const script = new URL("./stripe-report-slack.mjs", import.meta.url);
const csv = `date,customer_id,wallet_address,email,charges_count,charges_cents,credits_cents
2026-09-28,cus_test,0x1234567890abcdef,example@example.invalid,2,125,0
2026-09-29,cus_test,0x1234567890abcdef,example@example.invalid,3,250,0
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
      assert.match(text, /last 7 days/);
      assert.ok(text.includes("*$3.75* across *5* calls from *1* customer"));
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
