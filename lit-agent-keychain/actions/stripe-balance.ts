import { execute } from "./secret-common.ts";
import type { Manifest } from "../protocol/schema.ts";
import { decode, utf8, requireThat } from "../protocol/crypto.ts";
import { jsonFetch } from "../protocol/http.ts";
// Deliberately no get/export/rewrap route, arbitrary destination, body, headers or code.
export const run = (manifest: Manifest, params: unknown) =>
  execute(
    manifest,
    params,
    "stripe_balance",
    "stripe.balance",
    async (value) => {
      const credential = decode(value);
      requireThat(
        /^(?:sk|rk)_(?:test|live)_[A-Za-z0-9]{12,256}$/.test(credential),
      );
      const result = await jsonFetch(
        "https://api.stripe.com/v1/balance",
        {
          headers: {
            Authorization: `Bearer ${credential}`,
            "Stripe-Version": "2024-06-20",
          },
        },
        8000,
        65536,
      );
      // Project onto bounded numeric balances; never return upstream errors, headers or arbitrary strings.
      const balances = (items: any) => {
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
      return utf8(
        JSON.stringify({
          available: balances(result.available),
          pending: balances(result.pending),
          livemode: result.livemode === true,
        }),
      );
    },
  );
