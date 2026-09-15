import { defineAction, requireThat } from "../../lib.ts";
// Deliberately no get/export/rewrap route, arbitrary destination, body, headers or code.
export default defineAction(async ({ credential, fetchJson }) => {
  const result = await fetchJson("https://api.stripe.com/v1/balance", {
    headers: {
      Authorization: `Bearer ${credential}`,
      "Stripe-Version": "2024-06-20",
    },
  });
  // Project onto bounded numeric balances; never return upstream errors, headers or arbitrary strings.
  const balances = (items: unknown) => {
    requireThat(Array.isArray(items) && items.length <= 100);
    return items.map((entry: any) => {
      requireThat(
        Number.isSafeInteger(entry.amount) &&
          typeof entry.currency === "string" &&
          /^[a-z]{3}$/.test(entry.currency),
      );
      return { amount: entry.amount, currency: entry.currency };
    });
  };
  return {
    available: balances(result.available),
    pending: balances(result.pending),
    livemode: result.livemode === true,
  };
});
