import type { OwnerClient } from "../sdk/src/index.ts";
export async function subscribe(client: OwnerClient) {
  const { url } = await client.api("/api/billing/checkout", { method: "POST" });
  if (new URL(url).origin !== "http://127.0.0.1:55442")
    throw new Error("Only mock checkout is allowed");
  const response = await fetch(url + "/complete", { method: "POST" });
  if (!response.ok) throw new Error("Mock checkout failed");
  const { customer } = await response.json();
  await client.api("/api/billing/refresh", { method: "POST" });
  return customer as string;
}
