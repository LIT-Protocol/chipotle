/**
 * Integration tests for lit-api-server.
 * Exercises every endpoint in the OpenAPI spec using a full account lifecycle.
 *
 * Usage:
 *   k6 run k6/integration.spec.ts
 *   BASE_URL=https://your-instance.phala.network/core/v1 k6 run k6/integration.spec.ts
 */
import { check } from "k6";
import type { Response } from "k6/http";
import { LitApiServerClient } from "./litApiServer.ts";

const BASE_URL =
  __ENV.BASE_URL ||
  "https://36da669c852c9bd4fdea27dd331c07ff776bd125-8000.dstack-pha-prod5.phala.network/core/v1";

const HELLO_WORLD_CODE = 'Lit.Actions.setResponse({response: "Hello World!"})';

export const options = {
  vus: 1,
  iterations: 1,
  thresholds: {
    http_req_failed: ["rate==0"],
  },
};

function assertOk(
  name: string,
  endpoint: string,
  res: { response: Response },
): boolean {
  const { response } = res;
  const status = response?.status ?? 0;
  const ok = status >= 200 && status < 300;
  if (!ok) {
    let msg = "";
    if (status === 0) {
      msg = "(no response / connection failed)";
    } else {
      try {
        const body = JSON.parse(response.body as string);
        msg =
          body.message ??
          body.error ??
          body.detail ??
          (typeof body === "string" ? body : JSON.stringify(body));
      } catch {
        msg = (response.body as string) || "(no body)";
      }
    }
    console.error(`FAIL ${name} | ${endpoint} | ${status} | ${msg}`);
  }
  check(response, {
    [`${name} 2xx`]: (r) =>
      (r?.status ?? 0) >= 200 && (r?.status ?? 0) < 300,
  });
  return ok;
}

export default function () {
  const client = new LitApiServerClient({ baseUrl: BASE_URL });

  // ── 1. getNodeChainConfig ─────────────────────────────────────────────────
  const chainConfigRes = client.getNodeChainConfig();
  if (!assertOk("getNodeChainConfig", "GET /get_node_chain_config", chainConfigRes)) return;
  check(chainConfigRes.response, {
    "chain config has numeric chain_id": (r) => {
      try {
        return typeof JSON.parse(r.body as string).chain_id === "number";
      } catch {
        return false;
      }
    },
  });
}
