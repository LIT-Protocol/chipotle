// Loopback fixture only. No real cards, credentials, or Stripe network calls.
import http from "node:http";
import { createHmac, randomBytes } from "node:crypto";
const port = Number(process.env.KEYCHAIN_MOCK_STRIPE_PORT || 55442);
const base = `http://127.0.0.1:${port}`;
const api = process.env.KEYCHAIN_TEST_API || "http://localhost:55441";
if (!["localhost", "127.0.0.1"].includes(new URL(api).hostname))
  throw new Error("Test API must be loopback");
const customers = new Map<string, any>();
const checkouts = new Map<string, any>();
const subscriptions = new Map<string, any>();
const idempotent = new Map<string, any>();
const runId = randomBytes(8).toString("hex");
let sequence = 0;
const now = () => Math.floor(Date.now() / 1000);
async function webhook(
  customer: string,
  type = "customer.subscription.updated",
  id = `evt_test_${++sequence}`,
) {
  const body = JSON.stringify({
    id: `${id}_${runId}`,
    type,
    livemode: false,
    data: { object: { customer } },
  });
  const timestamp = now();
  const signature = createHmac("sha256", "whsec_local_test")
    .update(`${timestamp}.${body}`)
    .digest("hex");
  return fetch(api + "/api/billing/webhook", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Stripe-Signature": `t=${timestamp},v1=${signature}`,
    },
    body,
  });
}
const server = http.createServer(async (req, res) => {
  res.setHeader("Content-Type", "application/json");
  res.setHeader("Access-Control-Allow-Origin", "*");
  try {
    const url = new URL(req.url || "/", base);
    let raw = "";
    for await (const chunk of req) {
      raw += chunk;
      if (raw.length > 65536) throw new Error("large request");
    }
    const form = new URLSearchParams(raw);
    const send = (body: any) => res.end(JSON.stringify(body));
    const complete = async (id: string) => {
      const checkout = checkouts.get(id);
      if (!checkout) throw new Error("Unknown checkout");
      if (checkout.status === "open") {
        checkout.status = "complete";
        const subscription = {
          id: `sub_test_${runId}_${++sequence}`,
          customer: checkout.customer,
          created: now(),
          status: "active",
          cancel_at_period_end: false,
          metadata: customers.get(checkout.customer).metadata,
          items: {
            data: [
              {
                price: { id: "price_test_standard" },
                quantity: 1,
                current_period_end: now() + 30 * 86400,
              },
            ],
          },
          latest_invoice: { status: "paid", paid: true },
        };
        checkout.subscription = subscription.id;
        subscriptions.set(subscription.id, subscription);
      }
      const response = await webhook(
        checkout.customer,
        "checkout.session.completed",
      );
      if (!response.ok) throw new Error("Webhook failed");
      return checkout;
    };
    if (url.pathname.startsWith("/checkout/")) {
      const id = url.pathname.split("/")[2];
      if (req.method === "POST") {
        const checkout = await complete(id);
        if (url.pathname.endsWith("/complete"))
          return send({ ok: true, customer: checkout.customer });
        res.writeHead(303, { Location: checkout.success_url });
        res.end();
        return;
      }
      if (!checkouts.has(id)) throw new Error("Unknown checkout");
      res.setHeader("Content-Type", "text/html");
      res.end(
        `<h1>Test Stripe Checkout</h1><p>Keychain Standard: $10/month, 1,000 secrets</p><form method="POST"><button>Pay $10</button></form>`,
      );
      return;
    }
    if (url.pathname === "/test/update") {
      const {
        customer,
        status,
        cancel,
        expired,
        paid,
        eventId,
        price,
        quantity,
      } = JSON.parse(raw);
      const sub = [...subscriptions.values()].find(
        (s) => s.customer === customer,
      );
      if (!sub) throw new Error("Unknown customer");
      if (status !== undefined) sub.status = status;
      if (cancel !== undefined) sub.cancel_at_period_end = cancel;
      if (expired !== undefined)
        sub.items.data[0].current_period_end = expired
          ? now() - 1
          : now() + 30 * 86400;
      if (paid !== undefined)
        sub.latest_invoice = { status: paid ? "paid" : "open", paid };
      if (price !== undefined) sub.items.data[0].price.id = price;
      if (quantity !== undefined) sub.items.data[0].quantity = quantity;
      const response = await webhook(
        customer,
        "customer.subscription.updated",
        eventId,
      );
      return send({ webhookStatus: response.status });
    }
    if (url.pathname === "/test/state")
      return send({
        customers: [...customers.values()],
        checkouts: [...checkouts.values()],
        subscriptions: [...subscriptions.values()],
      });
    if (req.headers.authorization !== "Bearer sk_test_local") {
      res.writeHead(401);
      return send({ error: "unauthorized" });
    }
    const idem = req.headers["idempotency-key"] as string;
    if (req.method === "POST" && idem && idempotent.has(idem))
      return send(idempotent.get(idem));
    let result: any;
    if (url.pathname === "/v1/prices/price_test_standard")
      result = {
        active: true,
        unit_amount: 1000,
        currency: "usd",
        billing_scheme: "per_unit",
        recurring: {
          interval: "month",
          interval_count: 1,
          usage_type: "licensed",
        },
      };
    else if (
      url.pathname === "/v1/billing_portal/configurations/bpc_test_keychain"
    )
      result = {
        active: true,
        features: {
          subscription_cancel: { enabled: true, mode: "at_period_end" },
          subscription_update: { enabled: false },
        },
      };
    else if (url.pathname === "/v1/customers" && req.method === "POST") {
      result = {
        id: `cus_test_${runId}_${++sequence}`,
        metadata: {
          keychain_vault_id: form.get("metadata[keychain_vault_id]"),
          app: form.get("metadata[app]"),
        },
      };
      customers.set(result.id, result);
    } else if (url.pathname === "/v1/subscriptions")
      result = {
        has_more: false,
        data: [...subscriptions.values()].filter(
          (s) => s.customer === url.searchParams.get("customer"),
        ),
      };
    else if (
      url.pathname === "/v1/checkout/sessions" &&
      req.method === "POST"
    ) {
      if (
        form.get("line_items[0][price]") !== "price_test_standard" ||
        form.get("line_items[0][quantity]") !== "1" ||
        form.get("mode") !== "subscription"
      )
        throw new Error("Wrong checkout price");
      const id = `cs_test_${runId}_${++sequence}`;
      result = {
        id,
        status: "open",
        url: `${base}/checkout/${id}`,
        customer: form.get("customer"),
        success_url: form.get("success_url"),
      };
      checkouts.set(id, result);
    } else if (url.pathname.startsWith("/v1/checkout/sessions/"))
      result = checkouts.get(url.pathname.split("/").at(-1)!);
    else if (url.pathname === "/v1/billing_portal/sessions")
      result = { url: `${base}/portal/${form.get("customer")}` };
    else {
      res.writeHead(404);
      return send({ error: "not_found" });
    }
    if (!result) throw new Error("Missing fixture");
    if (req.method === "POST" && idem) idempotent.set(idem, result);
    send(result);
  } catch {
    res.writeHead(500);
    res.end(JSON.stringify({ error: "mock_stripe_failed" }));
  }
});
server.listen(port, "127.0.0.1", () =>
  process.stdout.write(`Test Stripe adapter listening on ${port}\n`),
);
