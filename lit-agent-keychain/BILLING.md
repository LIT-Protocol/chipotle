# Subscriptions and sponsored execution

Standard costs **USD $10 per month per vault/account for up to 1,000 stored secrets**.
Every retained secret counts once, including disabled secrets. Rotation adds a version,
not a slot. Fair use execution is included; there is no metered invoice or automatic
overage charge. More storage or high-volume usage goes through **Contact us**.

## Stripe configuration

Use a dedicated Stripe product for Keychain subscriptions. This is independent of the
optional Stripe balance credential that a user can store in their vault.

1. Create an active recurring price: USD 1000 cents, monthly, interval count 1,
   licensed usage, per-unit billing. Set `STRIPE_PRICE_ID` to its `price_…` ID.
2. Create a dedicated customer portal configuration with cancellation enabled **at
   period end**, subscription/quantity changes disabled, payment-method updates and
   invoice history enabled. Set `STRIPE_PORTAL_CONFIGURATION_ID` to its `bpc_…` ID.
3. Set `STRIPE_SECRET_KEY` to the corresponding test/live secret key. Register
   `https://YOUR-KEYCHAIN-ORIGIN/api/billing/webhook` and set its signing secret as
   `STRIPE_WEBHOOK_SECRET`. Subscribe to `checkout.session.completed`,
   `checkout.session.async_payment_succeeded`, `checkout.session.async_payment_failed`,
   `customer.subscription.created`, `customer.subscription.updated`,
   `customer.subscription.deleted`, `invoice.paid`, and `invoice.payment_failed`.
4. Set `PUBLIC_BASE_URL` to the served HTTPS origin and `CONTACT_EMAIL` to the sales
   contact. The service validates price and portal settings at startup and refuses
   to start on an incorrect price or unsafe portal configuration.

The client uses hosted Stripe Checkout and the hosted customer portal; no publishable
Stripe key or card input is needed in Keychain. The API pins Stripe's
`2025-03-31.basil` version. Webhooks are verified over the original request bytes,
with five-minute timestamp tolerance and matching test/live mode. Event payloads and
checkout redirect parameters never grant access: the API re-fetches current Stripe
subscriptions under the vault lock, validates account metadata, exact price and
quantity, and requires an active subscription with a paid latest invoice.

Checkout requests are serialized per vault, use Stripe idempotency keys, and reuse
an open session. Existing active/pending subscriptions block duplicate checkout.
Refresh handles delayed events; background synchronization retries every five minutes.
Keep hosts' clocks synchronized. Portal settings must remain dedicated to this product.

## Chipotle configuration

Set `CHIPOTLE_MASTER_API_KEY` for a dedicated managed parent account and
`LIT_EXECUTION_KEY` for a server-only usage key with wildcard execution but no management
permissions. Generate `USAGE_KEY_ENCRYPTION_KEY` with `openssl rand -hex 32` and store it
securely with the DB backup; losing it prevents managing previously issued user keys.
Do not rotate it in place without re-encrypting stored keys.

Each vault gets a fixed owner/discovery group and an incrementally populated secret
group. Its user key has execution permission only. The browser and agents call
`POST /core/v1/lit_action` directly; public-key discovery is a fixed Lit Action on
that same endpoint. Enrollment verifies an owner-signed manifest before adding a CID.
Paid status toggles the secret group's permission on the key in one operation.
Key replacement stages a durable revocation first, confirms removal, and then returns
a replacement. Owners must redistribute the replacement config/key to their agents.

Deploy this PR's **billing-owner guards in both lit-api-server and lit-payments** and
the Lit runtime telemetry fixes before distributing execution keys. Execution usage
keys must not manage saved cards, auto-recharge settings, or funding operations.
The master-key check accepts generated managed keys whose wallet is the resolved
billing owner; other accounts must use valid owner wallet authentication.

There is **no hard per-user execution/spending cap** on Chipotle usage keys. They share
the parent balance. Monitor that balance and the parent's own auto-recharge cap;
those settings bound the operator's exposure, not individual usage. A caller can
repeatedly invoke its allowed actions even if authorization fails. The app's three
execution-limit settings cover only server-sponsored login attempts.

## Cancellation, recovery and custom accounts

Normal cancellation preserves access until the paid period ends. At expiry, new
secrets, restore writes and rotations require a subscription; sign-in, owner policy
revocation, metadata and encrypted backups remain available. No data is automatically
deleted. A worker reconciles Chipotle permissions every minute; delayed provider calls,
permission caches or outages can delay sponsorship cutoff. Subscription state does
not replace signed secret access policies, and an independently funded Lit payer may
continue to execute an otherwise authorized action.

For a negotiated plan, apply a bounded storage limit and explicit expiry using the
operator-only binary with `DATABASE_URL` configured:

```sh
cargo +1.91 run --bin keychain-plan -- VAULT_ID 5000 2027-01-01T00:00:00Z
# The production image also contains /app/keychain-plan.
```

The limit may be 1–100,000. The command only adjusts storage/sponsorship entitlement;
it cannot create owner approvals or agent grants. It writes an audit event in the
same transaction. Custom billing is agreed manually and is not automatically charged
by this command. Set an expiry in the past to end a custom entitlement; any still-paid
Standard subscription then applies. A downgrade never deletes retained records.

## Validation

`node scripts/test-local.mjs` with `KEYCHAIN_TEST_DATABASE_URL` set to a dedicated
loopback test database runs real API, PostgreSQL and Chromium flows against local
Stripe/Chipotle boundary adapters. It covers checkout navigation, forged webhooks,
reordered events, unpaid/wrong-price subscriptions, duplicate checkout, capacity races,
rotation at capacity, more than ten action CIDs, scoped-key isolation, key replacement
with a lost removal response, cancellation, renewal, and backup preservation.

CI also uses the existing Stripe sandbox secret for a real contract test: price and
portal validation, Checkout creation, a paid test subscription, period-end cancellation,
and immediate cancellation through the actual Rust entitlement synchronizer. Synthetic
customers/checkouts are removed and test prices/products are archived. If the first
portal configuration becomes the sandbox default, it is retained and explicitly
labelled as a test fixture; non-default configurations are deactivated. A bounded
recovery script uses creation-event idempotency keys to find only older Keychain test
objects left by interrupted CI. No live-mode credentials are accepted by these tests.

Before launch, additionally complete a Stripe test-mode checkout/payment/portal cancel
on the deployed origin and exercise live owner/agent flows through Chipotle. Local
adapters do not establish live payment, OAuth or TEE deployment correctness.
