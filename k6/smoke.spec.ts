/**
 * Smoke test - hits the public get_node_chain_config endpoint (no auth required).
 * Use: k6 run smoke.spec.ts
 */
import type { Response } from "k6/http";
import { checkAndLog } from "./check.ts";
import { LitApiServerClient } from "./litApiServer.ts";

const baseUrl =
  __ENV.BASE_URL ||
  "https://36da669c852c9bd4fdea27dd331c07ff776bd125-8000.dstack-pha-prod5.phala.network/core/v1";
const client = new LitApiServerClient({ baseUrl });

export const options = {
  vus: 2,
  duration: "10s",
};

export default function () {
  const { response } = client.getNodeChainConfig();
  checkAndLog<Response>(response, {
    "status is 200": (r) => r.status === 200,
    "has chain config": (r) => {
      try {
        const body = JSON.parse(r.body as string);
        return body && typeof body.chain_id === "number";
      } catch {
        return false;
      }
    },
  }, "getNodeChainConfig");
}
