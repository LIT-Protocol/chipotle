/**
 * Billing — Stripe integration, payment flow.
 */

import { getApiKey, getClient, hasUsageKeyOverride } from './auth.js';
import { formatError, logError } from './ui-utils.js';

let _stripe = null;
let _stripeCard = null;

let _billingAvailable = null;
let _billingCheckedAt = 0;
let _billingRetryTimer = null;
const BILLING_RETRY_MS = 30000;

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

export function refreshBillingUI() {
  const capturedKey = getApiKey();
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
    if (getApiKey() !== capturedKey) return;
    if (available) {
      if (balanceEl) balanceEl.style.display = '';
      if (addFundsBtn) addFundsBtn.style.display = '';
      if (notRequiredEl) notRequiredEl.style.display = 'none';
      if (billingBanner) billingBanner.style.display = 'none';
      loadBillingBalance();
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
  const apiKey = getApiKey();
  if (!apiKey) return;
  const el = document.getElementById('billing-balance-display');
  if (!el) return;
  const noFundsWarning = document.getElementById('no-funds-warning');
  try {
    const client = await getClient();
    const data = await client.getBillingBalance(apiKey);
    el.textContent = data.balance_display || '';
    if (noFundsWarning) {
      const hasNoFunds = typeof data.balance_cents === 'number' && data.balance_cents >= 0;
      noFundsWarning.style.display = hasNoFunds ? '' : 'none';
    }
  } catch (e) {
    logError('loadBillingBalance', e);
    el.textContent = 'Balance unavailable';
    if (noFundsWarning) noFundsWarning.style.display = 'none';
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
      const apiKey = getApiKey();
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

      let intentId = null;
      try {
        const client = await getClient();
        const intent = await client.createPaymentIntent(apiKey, amountCents);
        intentId = intent.payment_intent_id;

        const result = await _stripe.confirmCardPayment(intent.client_secret, {
          payment_method: { card: _stripeCard },
        });

        if (result.error) {
          throw new Error(result.error.message);
        }

        // Separate try for confirmPayment — card is already charged at this point
        try {
          await client.confirmPayment(apiKey, intent.payment_intent_id);
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
        logError('payment', e, { intentId });
        if (statusEl) {
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

