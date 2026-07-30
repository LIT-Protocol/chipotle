/**
 * gVisor smoke test - runs a "hello world" any-language (Python) action through
 * the gVisor runner via POST /lit_binary_action.
 *
 * The bundle installs a pinned pip package and runs a recent Python that greets
 * and reports its version, proving the any-language sandbox is live in the
 * target environment. gVisor is off by default (CPL-359) and only guaranteed on
 * in the Phala test/next staging environment, so the deploy pipeline runs this
 * against staging only — not prod.
 *
 * Use: k6 run gvisor-smoke.spec.ts
 */
import { checkAndLog, warnOnHttpFailures, assertOk } from "./helpers.ts";
import { LitApiServerClient } from "./litApiServer.ts";
import { PRECREATED_ACCOUNTS } from "./setup.ts";
import { BASE_URL, COMMON_PARAMS } from "./defaults.ts";
import { ensureAccountCredits } from "./stripe.ts";
import { buildBundleBase64 } from "./gvisorBundle.ts";

// Bundle source lives as real files (linted, reviewable) and is packed into the
// tar at init time. `startup.sh` is the sandbox entrypoint; `main.py` is the action.
const STARTUP_SCRIPT = open(
  import.meta.resolve("./LitActionCode/gvisor/startup.sh"),
) as unknown as string;
const MAIN_PY = open(
  import.meta.resolve("./LitActionCode/gvisor/main.py"),
) as unknown as string;

const BUNDLE_BASE64 = buildBundleBase64([
  { name: "startup.sh", content: STARTUP_SCRIPT, mode: 0o755 },
  { name: "main.py", content: MAIN_PY },
]);

export const options = {
  vus: 1,
  iterations: 1,
  thresholds: {
    http_reqs: ["count>=1"],
    checks: ["rate==1"],
  },
};

export interface GvisorSmokeSetupData {
  usageApiKey: string;
}

export function setup(): GvisorSmokeSetupData {
  if (PRECREATED_ACCOUNTS.length === 0) {
    throw new Error(
      "No pre-created accounts found. Run accounts.seed.spec.ts first to generate k6/data/accounts.json",
    );
  }
  const account =
    PRECREATED_ACCOUNTS[Math.floor(Math.random() * PRECREATED_ACCOUNTS.length)];

  const client = new LitApiServerClient({ baseUrl: BASE_URL, commonRequestParameters: COMMON_PARAMS });
  ensureAccountCredits(client, { "X-Api-Key": account.apiKey });

  return { usageApiKey: account.usageApiKey };
}

export default function (data: GvisorSmokeSetupData) {
  const client = new LitApiServerClient({ baseUrl: BASE_URL, commonRequestParameters: COMMON_PARAMS });
  const usageKeyHeaders = { "X-Api-Key": data.usageApiKey };

  const res = client.litBinaryAction(
    { bundle: BUNDLE_BASE64, js_params: { name: "chipotle" } },
    usageKeyHeaders,
  );
  if (!assertOk("litBinaryAction", "POST /lit_binary_action", res)) return;

  checkAndLog(res.response, {
    "binary action has no error": (r) => {
      try {
        return JSON.parse(r.body as string).has_error === false;
      } catch {
        return false;
      }
    },
    "python hello-world ran and pip install worked": (r) => {
      try {
        // The action set-response with a JSON string; the API returns it in
        // `response` (as a string, or already parsed — accept both).
        const body = JSON.parse(r.body as string);
        const payload =
          typeof body.response === "string"
            ? JSON.parse(body.response)
            : body.response;
        return (
          payload.ok === true &&
          typeof payload.python === "string" &&
          payload.python.startsWith("3.") &&
          payload.greeting === `hello chipotle from python ${payload.python}`
        );
      } catch {
        return false;
      }
    },
  }, "litBinaryAction");
}

export const handleSummary = warnOnHttpFailures;
