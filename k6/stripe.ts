/**
 * Stripe sandbox helpers for K6 tests.
 *
 * Fetches the publishable key from the server's /billing/stripe_config endpoint
 * and uses it to confirm PaymentIntents with a test card token. No secret key
 * needed — the publishable key is sufficient for client-side confirmation in
 * Stripe test mode.
 */
import http from "k6/http";
import { LitApiServerClient } from "./litApiServer.ts";
import type {
  CreatePaymentIntentResponse,
  StripeConfigResponse,
} from "./litApiServer.ts";
import { assertOk } from "./helpers.ts";

const TOPUP_AMOUNT_CENTS = 1000; // $10.00
const IS_PROD = __ENV.K6_ENV === "prod";

/**
 * Minimum credit balance (in cents) below which we top up.
 * Stripe balance is negative when credits are available, so -200 means $2.00.
 * We top up when credits drop below $2.00 to ensure enough runway.
 */
const MIN_CREDIT_THRESHOLD_CENTS = 200;

/** Cache the publishable key so we only fetch it once per k6 run. */
let _publishableKey: string | null = null;

/**
 * Fetch the Stripe publishable key from the server. Returns null if billing
 * is not configured on the server (stripe_config returns non-200 or no key).
 */
function getPublishableKey(client: LitApiServerClient): string | null {
  if (_publishableKey !== null) return _publishableKey;

  const res = client.billingStripeConfig();
  if (res.response.status !== 200) {
    _publishableKey = "";
    return null;
  }
  const data = res.data as StripeConfigResponse;
  _publishableKey = data.publishable_key ?? "";
  return _publishableKey || null;
}

/**
 * Top up an account's credit balance via Stripe sandbox.
 *
 * 1. Fetches the publishable key from /billing/stripe_config.
 * 2. Creates a PaymentIntent through the lit-api-server billing API.
 * 3. Confirms the PaymentIntent directly with Stripe using the publishable
 *    key and the test card `pm_card_visa`.
 * 4. Tells lit-api-server to verify the payment and credit the account.
 *
 * @returns true if top-up succeeded, false otherwise.
 */
export function topUpAccount(
  client: LitApiServerClient,
  authHeaders: { "X-Api-Key": string },
  amountCents: number = TOPUP_AMOUNT_CENTS,
): boolean {
  if (IS_PROD) {
    console.warn("stripe: top-up disabled in prod — accounts must be pre-funded");
    return false;
  }

  const pk = getPublishableKey(client);
  if (!pk) {
    console.warn("stripe: billing not enabled on server — skipping top-up");
    return false;
  }

  // 1. Create PaymentIntent via our API
  const createRes = client.billingCreatePaymentIntent(
    { amount_cents: amountCents },
    authHeaders,
  );
  if (!assertOk("topUp/createPaymentIntent", "POST /billing/create_payment_intent", createRes)) {
    return false;
  }
  const { client_secret, payment_intent_id } = createRes.data as CreatePaymentIntentResponse;

  // 2. Confirm the PaymentIntent with Stripe using the publishable key and
  //    a test payment method. In test mode, pm_card_visa always succeeds.
  const confirmStripeRes = http.post(
    `https://api.stripe.com/v1/payment_intents/${payment_intent_id}/confirm`,
    { payment_method: "pm_card_visa", client_secret },
    {
      headers: {
        Authorization: `Bearer ${pk}`,
      },
    },
  );
  if (confirmStripeRes.status !== 200) {
    console.error(
      `stripe: confirm failed (${confirmStripeRes.status}): ${confirmStripeRes.body}`,
    );
    return false;
  }

  // 3. Tell our server to verify the payment and credit the account
  const confirmRes = client.billingConfirmPayment(
    { payment_intent_id },
    authHeaders,
  );
  if (!assertOk("topUp/confirmPayment", "POST /billing/confirm_payment", confirmRes)) {
    return false;
  }

  return true;
}

/**
 * Check whether Stripe billing is enabled on the server.
 * Makes a single HTTP call on first invocation, then caches the result.
 */
export function isBillingEnabled(client: LitApiServerClient): boolean {
  return getPublishableKey(client) !== null;
}

/**
 * Return the account's available credits in cents (≥ 0).
 * Stripe stores balance as negative when credits exist, so we negate.
 * Returns 0 if the balance check fails (e.g. no Stripe customer yet).
 */
export function getAccountCredits(
  client: LitApiServerClient,
  authHeaders: { "X-Api-Key": string },
): number {
  const res = client.billingBalance(authHeaders);
  if (res.response.status !== 200) {
    return 0;
  }
  const data = res.data as import("./litApiServer.ts").BillingBalanceResponse;
  // balance_cents is negative when credits are available
  return Math.max(0, -data.balance_cents);
}

/**
 * Ensure a pre-created account has sufficient Stripe credits for test execution.
 *
 * Handles accounts that:
 * - May not have a Stripe customer yet (created on-demand by the server)
 * - May already have sufficient credits (skips top-up)
 * - Need a top-up to cover test costs
 *
 * @returns true if the account has sufficient credits (or billing is disabled), false on failure.
 */
export function ensureAccountCredits(
  client: LitApiServerClient,
  authHeaders: { "X-Api-Key": string },
  minCreditsCents: number = MIN_CREDIT_THRESHOLD_CENTS,
): boolean {
  if (!isBillingEnabled(client)) {
    return true;
  }

  const credits = getAccountCredits(client, authHeaders);
  if (credits >= minCreditsCents) {
    return true;
  }

  if (IS_PROD) {
    console.warn(
      `stripe: account has ${credits} cents but needs ${minCreditsCents} — top-up disabled in prod`,
    );
    return false;
  }

  // Top up enough to reach the desired minimum.
  const deficit = minCreditsCents - credits;
  const topUpAmount = Math.max(TOPUP_AMOUNT_CENTS, deficit);
  return topUpAccount(client, authHeaders, topUpAmount);
}
