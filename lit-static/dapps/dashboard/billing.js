/**
 * Billing — Stripe integration, payment flow.
 *
 * ChainSecured callers (CPL-285) authenticate to /billing/* with a SIWE-lite
 * EIP-191 signed message proving they hold the wallet's private key. The
 * signed payload is cached for ~4 minutes (server allows ±5min skew, we leave
 * a buffer) so the user only sees one wallet popup per billing session.
 *
 * All in-flight billing requests are tied to a module AbortController and a
 * captured-credential snapshot — `clearBillingSession()` is called on logout
 * or wallet switch and aborts pending fetches; every post-await branch
 * re-checks `billingAuthKey() === captured` before mutating UI or calling
 * Stripe.confirmCardPayment so a mid-flight session change can never credit
 * the prior account.
 */

import {
  getApiKey,
  getChainSecuredHash,
  getChainSecuredWallet,
  getClient,
  getMode,
  hasUsageKeyOverride,
} from './auth.js';
import { formatError, logError } from './ui-utils.js';

let _stripe = null;
let _stripeCard = null;

let _billingAvailable = null;
let _billingCheckedAt = 0;
let _billingRetryTimer = null;
const BILLING_RETRY_MS = 30000;

// AbortController for in-flight billing fetches. Recreated lazily; aborted on
// session change via clearBillingSession().
let _abortController = null;

// Cached SIWE auth payload. Re-used across billing requests within the
// timestamp window. { headerValue, expiresAtMs, walletAddress }
let _walletAuthCache = null;

// Server's SIWE_TIMESTAMP_SKEW_SECONDS is 300; we cache for 4 min to leave a
// 1-min safety buffer against clock skew + in-flight latency.
const WALLET_AUTH_TTL_MS = 4 * 60 * 1000;

/**
 * Mode-branched (NOT a `||` fallback): a stale `STORAGE_KEY_API` left over
 * from a previous API login must not be sent in sovereign mode (CPL-285).
 */
function billingAuthKey() {
  return getMode() === 'sovereign' ? getChainSecuredHash() : getApiKey();
}

function ensureAbortController() {
  if (!_abortController || _abortController.signal.aborted) {
    _abortController = new AbortController();
  }
  return _abortController;
}

/**
 * Abort any in-flight billing requests and clear the cached SIWE auth
 * payload. Called on logout, mode switch, and wallet change so a mid-flight
 * fetch can't credit the prior account.
 */
export function clearBillingSession() {
  if (_abortController) {
    _abortController.abort();
    _abortController = null;
  }
  _walletAuthCache = null;
}

/**
 * Evict the cached wallet-auth payload so the next billing request prompts a
 * fresh signature. Only call from error handlers when the failure was an auth
 * rejection — never on transient network/5xx, where evicting would force the
 * user through a wallet popup on every retry (signature-prompt fatigue,
 * phishing-overlay surface).
 */
function evictWalletAuthCache() {
  _walletAuthCache = null;
}

/**
 * True when an SDK error came from the server actually rejecting the
 * credential — not a network blip, 5xx, or a Stripe.js failure. The SDK
 * stamps `err.status` on Error objects from `parseResponse` (CPL-285).
 */
function isAuthError(e) {
  return !!(e && (e.status === 401 || e.status === 400));
}

export async function checkBillingAvailable() {
  if (_billingAvailable === true) return true;
  if (_billingAvailable === false && (Date.now() - _billingCheckedAt) < BILLING_RETRY_MS) {
    return false;
  }
  try {
    const client = await getClient();
    await client.getStripeConfig();
    _billingAvailable = true;
  } catch (_) {
    _billingAvailable = false;
  }
  _billingCheckedAt = Date.now();
  return _billingAvailable;
}

export function resetBillingAvailability() {
  _billingAvailable = null;
  _billingCheckedAt = 0;
  if (_billingRetryTimer) { clearTimeout(_billingRetryTimer); _billingRetryTimer = null; }
}

/**
 * Build the SIWE-lite message body the server's `parse_create_wallet_siwe`
 * expects. Required lines (case-sensitive): `Address:`, `Chain ID:`,
 * `Issued At:`, `Purpose:`. The `Purpose` line pins this signature to the
 * billing-auth flow only — prevents cross-flow replay against
 * /create_wallet_with_signature or /convert_to_chain_secured_account (CPL-285).
 */
function buildSiweMessage(wallet, chainId, issuedAt) {
  const host = (typeof location !== 'undefined' && location.host) ? location.host : 'lit-dashboard';
  return [
    `${host} wants you to sign in to billing.`,
    '',
    'This signature lets the dashboard authenticate billing requests on your',
    "behalf for ~5 minutes. It is not a transaction and won't be broadcast.",
    '',
    `Address: ${wallet}`,
    `Chain ID: ${chainId}`,
    `Issued At: ${issuedAt}`,
    'Purpose: lit-billing-auth-v1',
  ].join('\n');
}

/**
 * Get a cached SIWE auth header, or prompt the wallet to sign a fresh one.
 * Returns base64(JSON{message, signature}) for the X-Wallet-Auth header.
 */
async function getWalletAuthHeader() {
  const wallet = getChainSecuredWallet();
  if (!wallet) {
    throw new Error('No connected ChainSecured wallet — sign in first.');
  }
  const cachedFor = _walletAuthCache && _walletAuthCache.walletAddress;
  if (cachedFor && cachedFor.toLowerCase() === wallet.toLowerCase()
      && _walletAuthCache.expiresAtMs > Date.now() + 10_000) {
    return _walletAuthCache.headerValue;
  }

  const client = await getClient();
  const chainIdNum = client.chainId != null
    ? Number(client.chainId)
    : Number((await client.getNodeChainConfig()).chain_id);
  const issuedAt = Math.floor(Date.now() / 1000);
  const message = buildSiweMessage(wallet, chainIdNum, issuedAt);

  const { connectEoa } = await import('../../wallet_connect.js');
  const { signer } = await connectEoa();
  const signature = await signer.signMessage(message);

  const headerValue = btoa(JSON.stringify({ message, signature }));
  _walletAuthCache = {
    headerValue,
    expiresAtMs: Date.now() + WALLET_AUTH_TTL_MS,
    walletAddress: wallet,
  };
  return headerValue;
}

/**
 * Resolve the SDK options for the current mode: { walletAuthHeader?, signal }.
 * In API mode we just attach the abort signal; in sovereign mode we also
 * obtain the SIWE header (prompting the wallet on cache miss).
 */
async function billingRequestOptions(signal) {
  const opts = { signal };
  if (getMode() === 'sovereign') {
    opts.walletAuthHeader = await getWalletAuthHeader();
  }
  return opts;
}

export function refreshBillingUI() {
  const capturedKey = billingAuthKey();
  const balanceEl = document.getElementById('billing-balance-display');
  const addFundsBtn = document.getElementById('btn-add-funds');
  const notRequiredEl = document.getElementById('billing-not-required');
  const billingBanner = document.getElementById('billing-disabled-banner');
  const noFundsWarning = document.getElementById('no-funds-warning');
  if (!capturedKey || hasUsageKeyOverride()) {
    if (balanceEl) balanceEl.style.display = 'none';
    if (addFundsBtn) addFundsBtn.style.display = 'none';
    if (notRequiredEl) notRequiredEl.style.display = 'none';
    if (billingBanner) billingBanner.style.display = 'none';
    if (noFundsWarning) noFundsWarning.style.display = 'none';
    return;
  }
  checkBillingAvailable().then((available) => {
    if (billingAuthKey() !== capturedKey) return;
    if (available) {
      if (balanceEl) balanceEl.style.display = '';
      if (addFundsBtn) addFundsBtn.style.display = '';
      if (notRequiredEl) notRequiredEl.style.display = 'none';
      if (billingBanner) billingBanner.style.display = 'none';
      // Don't auto-trigger a wallet popup just to render the balance — the
      // ChainSecured user signs once when they explicitly click Add Funds.
      // Display "—" until they do.
      if (getMode() === 'sovereign' && balanceEl && !balanceEl.textContent) {
        balanceEl.textContent = '—';
      } else {
        loadBillingBalance();
      }
    } else {
      if (balanceEl) balanceEl.style.display = 'none';
      if (addFundsBtn) addFundsBtn.style.display = 'none';
      if (notRequiredEl) notRequiredEl.style.display = '';
      if (billingBanner) billingBanner.style.display = '';
      if (noFundsWarning) noFundsWarning.style.display = 'none';
      if (_billingRetryTimer) clearTimeout(_billingRetryTimer);
      _billingRetryTimer = setTimeout(() => {
        _billingRetryTimer = null;
        refreshBillingUI();
      }, BILLING_RETRY_MS);
    }
  }).catch((e) => console.error('billing check failed', e));
}

async function loadBillingBalance() {
  const apiKey = billingAuthKey();
  if (!apiKey) return;
  const el = document.getElementById('billing-balance-display');
  if (!el) return;
  const noFundsWarning = document.getElementById('no-funds-warning');
  const ctrl = ensureAbortController();
  try {
    const client = await getClient();
    if (billingAuthKey() !== apiKey || ctrl.signal.aborted) return;
    const opts = await billingRequestOptions(ctrl.signal);
    if (billingAuthKey() !== apiKey || ctrl.signal.aborted) return;
    const data = await client.getBillingBalance(apiKey, opts);
    if (billingAuthKey() !== apiKey || ctrl.signal.aborted) return;
    el.textContent = data.balance_display || '';
    if (noFundsWarning) {
      const hasNoFunds = typeof data.balance_cents === 'number' && data.balance_cents >= 0;
      noFundsWarning.style.display = hasNoFunds ? '' : 'none';
    }
  } catch (e) {
    if (e && (e.name === 'AbortError' || ctrl.signal.aborted)) return;
    if (getMode() === 'sovereign' && isAuthError(e)) evictWalletAuthCache();
    logError('loadBillingBalance', e);
    if (billingAuthKey() === apiKey) {
      el.textContent = 'Balance unavailable';
      if (noFundsWarning) noFundsWarning.style.display = 'none';
    }
  }
}

async function openAddFundsModal() {
  if (_billingAvailable === false) return;
  const overlay = document.getElementById('billing-modal-overlay');
  if (!overlay) return;
  overlay.classList.add('is-open');
  overlay.setAttribute('aria-hidden', 'false');

  const statusEl = document.getElementById('billing-modal-status');
  if (statusEl) { statusEl.style.display = 'none'; }

  const payBtn = document.getElementById('billing-pay-btn');
  if (payBtn) payBtn.disabled = true;

  if (!_stripe) {
    try {
      const client = await getClient();
      const cfg = await client.getStripeConfig();
      _stripe = Stripe(cfg.publishable_key); // eslint-disable-line no-undef
      const elements = _stripe.elements();
      _stripeCard = elements.create('card');
      _stripeCard.mount('#stripe-card-element');
    } catch (e) {
      logError('stripe-init', e);
      if (statusEl) {
        statusEl.textContent = 'Billing not available: ' + formatError(e);
        statusEl.className = 'status error';
        statusEl.style.display = 'block';
      }
      return;
    }
  }

  if (payBtn) payBtn.disabled = false;
}

function closeBillingModal() {
  const overlay = document.getElementById('billing-modal-overlay');
  if (overlay) {
    overlay.classList.remove('is-open');
    overlay.setAttribute('aria-hidden', 'true');
  }
}

export function initBilling() {
  const addFundsBtn = document.getElementById('btn-add-funds');
  const closeBtn = document.getElementById('billing-modal-close-btn');
  const cancelBtn = document.getElementById('billing-cancel-btn');
  const payBtn = document.getElementById('billing-pay-btn');

  if (addFundsBtn) addFundsBtn.addEventListener('click', openAddFundsModal);
  const noFundsLink = document.getElementById('no-funds-add-funds');
  if (noFundsLink) noFundsLink.addEventListener('click', (e) => { e.preventDefault(); openAddFundsModal(); });
  if (closeBtn) closeBtn.addEventListener('click', closeBillingModal);
  if (cancelBtn) cancelBtn.addEventListener('click', closeBillingModal);

  if (payBtn) {
    payBtn.addEventListener('click', async () => {
      const apiKey = billingAuthKey();
      if (!apiKey || !_stripe || !_stripeCard) return;

      const amountInput = document.getElementById('billing-amount');
      const amountCents = parseInt(amountInput.value, 10);
      const statusEl = document.getElementById('billing-modal-status');

      // Client-side validation: minimum $5.00
      if (!amountCents || amountCents < 500) {
        if (statusEl) {
          statusEl.textContent = 'Minimum amount is $5.00.';
          statusEl.className = 'status error';
          statusEl.style.display = 'block';
        }
        return;
      }

      payBtn.disabled = true;
      if (statusEl) { statusEl.style.display = 'none'; }

      const ctrl = ensureAbortController();
      let intentId = null;
      try {
        const client = await getClient();
        if (billingAuthKey() !== apiKey || ctrl.signal.aborted) return;
        const opts = await billingRequestOptions(ctrl.signal);
        if (billingAuthKey() !== apiKey || ctrl.signal.aborted) return;
        const intent = await client.createPaymentIntent(apiKey, amountCents, opts);
        if (billingAuthKey() !== apiKey || ctrl.signal.aborted) {
          // Auth changed after intent was created — log but don't proceed.
          // The PaymentIntent will simply expire unconfirmed.
          logError('payment', new Error('session changed mid-flight, dropping intent'), { intentId: intent.payment_intent_id });
          return;
        }
        intentId = intent.payment_intent_id;

        const result = await _stripe.confirmCardPayment(intent.client_secret, {
          payment_method: { card: _stripeCard },
        });

        if (result.error) {
          throw new Error(result.error.message);
        }

        if (billingAuthKey() !== apiKey || ctrl.signal.aborted) {
          // Card was charged, but the session changed before we could call
          // confirmPayment. The funds are reserved on the original wallet's
          // Stripe customer; surface the intent ID for manual reconciliation.
          if (statusEl) {
            statusEl.textContent = 'Payment processed — session changed before crediting. Reference: ' + intent.payment_intent_id;
            statusEl.className = 'status info';
            statusEl.style.display = 'block';
          }
          return;
        }

        // Separate try for confirmPayment — card is already charged at this point
        try {
          // Re-fetch options in case the cached SIWE expired during Stripe.js round-trip.
          const opts2 = await billingRequestOptions(ctrl.signal);
          if (billingAuthKey() !== apiKey || ctrl.signal.aborted) return;
          await client.confirmPayment(apiKey, intent.payment_intent_id, opts2);
        } catch (confirmErr) {
          logError('confirmPayment', confirmErr, { intentId });
          if (statusEl) {
            statusEl.textContent = 'Payment processed — credit pending. Reference: ' + intent.payment_intent_id;
            statusEl.className = 'status info';
            statusEl.style.display = 'block';
          }
          closeBillingModal();
          await loadBillingBalance();
          return;
        }

        closeBillingModal();
        await loadBillingBalance();
      } catch (e) {
        if (e && (e.name === 'AbortError' || ctrl.signal.aborted)) return;
        if (getMode() === 'sovereign' && isAuthError(e)) evictWalletAuthCache();
        logError('payment', e, { intentId });
        if (statusEl && billingAuthKey() === apiKey) {
          statusEl.textContent = 'Payment failed: ' + formatError(e);
          statusEl.className = 'status error';
          statusEl.style.display = 'block';
        }
      } finally {
        payBtn.disabled = false;
      }
    });
  }
}
