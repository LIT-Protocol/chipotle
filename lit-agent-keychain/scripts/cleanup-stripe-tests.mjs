// Recover fixtures left by interrupted CI, using their creation-event idempotency
// keys as provenance. Test mode only; never touch another product's objects.
const key = process.env.STRIPE_SECRET_KEY;
if (!/^(sk|rk)_test_/.test(key || ""))
  throw new Error("Stripe test key required");
async function call(method, path, fields = {}) {
  const data = new URLSearchParams(fields);
  const res = await fetch(
    `https://api.stripe.com/v1${path}${method === "GET" ? "?" + data : ""}`,
    {
      method,
      headers: {
        Authorization: `Bearer ${key}`,
        "Stripe-Version": "2025-03-31.basil",
        ...(method === "POST"
          ? { "Content-Type": "application/x-www-form-urlencoded" }
          : {}),
      },
      ...(method === "POST" ? { body: data } : {}),
      redirect: "error",
      signal: AbortSignal.timeout(20000),
    },
  );
  const body = await res.json();
  if (res.status === 404) return null;
  if (!res.ok)
    throw new Error(
      `${method} ${path}: HTTP ${res.status}; code=${body.error?.code || "unknown"}`,
    );
  return body;
}
const now = Math.floor(Date.now() / 1000);
const events = [];
let after;
for (let page = 0; page < 20; page++) {
  const result = await call("GET", "/events", {
    limit: "100",
    "created[gte]": String(now - 86400),
    "created[lte]": String(now - 600),
    ...(after ? { starting_after: after } : {}),
  });
  events.push(...result.data);
  if (!result.has_more) break;
  after = result.data.at(-1)?.id;
  if (page === 19)
    throw new Error("Too many Stripe events for bounded fixture recovery");
}
const prefixes = new Set(
  events
    .filter(
      (e) =>
        e.type === "product.created" &&
        e.livemode === false &&
        e.data.object.name === "Keychain temporary contract test" &&
        /^[0-9a-f]{64}-product$/.test(e.request?.idempotency_key || ""),
    )
    .map((e) => e.request.idempotency_key.slice(0, -8)),
);
const owned = events.filter(
  (e) =>
    e.livemode === false &&
    prefixes.has((e.request?.idempotency_key || "").slice(0, 64)),
);
const resources = new Map();
for (const e of owned) {
  const o = e.data.object;
  if (
    ["product", "price", "customer", "billing_portal.configuration"].includes(
      o.object,
    )
  )
    resources.set(o.id, o.object);
}
// Customers first: expire unpaid checkouts and delete only these synthetic customers
// (Stripe also cancels their subscriptions). Already-deleted objects are harmless.
const priority = {
  customer: 0,
  price: 1,
  product: 2,
  "billing_portal.configuration": 3,
};
const ordered = [...resources].sort((a, b) => priority[a[1]] - priority[b[1]]);
for (const [id, type] of ordered) {
  const kind = {
    product: "products",
    price: "prices",
    customer: "customers",
    "billing_portal.configuration": "billing_portal/configurations",
  }[type];
  const current = await call("GET", `/${kind}/${id}`);
  if (!current || current.deleted || current.active === false) continue;
  if (type === "customer") {
    const sessions = await call("GET", "/checkout/sessions", {
      customer: id,
      limit: "100",
    });
    for (const session of sessions.data)
      if (session.status === "open")
        await call("POST", `/checkout/sessions/${session.id}/expire`);
    await call("DELETE", `/customers/${id}`);
  } else if (type === "billing_portal.configuration" && current.is_default) {
    // Preserve the shared sandbox default and label the known test fixture.
    // A portal configuration holds no customer, card or secret data.
    if (current.metadata?.keychain_contract_test_default !== "true")
      await call("POST", `/${kind}/${id}`, {
        "metadata[keychain_contract_test_default]": "true",
      });
    console.log(`Retained labelled default TEST portal configuration: ${id}`);
    continue;
  } else {
    await call("POST", `/${kind}/${id}`, { active: "false" });
  }
  console.log(`Recovered stale Keychain TEST fixture: ${type} ${id}`);
}
console.log(
  "Stripe fixture recovery complete; only proven Keychain test objects considered",
);
