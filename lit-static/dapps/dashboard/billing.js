/**
 * Billing — Stripe integration, payment flow.
 *
 * Uses Stripe Payment Element which auto-renders whichever methods
 * are enabled on the account (card, USDC, USDP, ETH, SOL). Crypto
 * payments use a redirect flow; card payments complete inline when
 * no additional action is required.
 */

import { getApiKey, getClient, hasUsageKeyOverride } from './auth.js';
import { formatError, logError } from './ui-utils.js';

let _stripe = null;
let _publishableKey = null;
let _elements = null;
let _paymentElement = null;
let _paymentIntentId = null;

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
    const cfg = await client.getStripeConfig();
    _publishableKey = cfg.publishable_key;
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

function setModalStep(step) {
  const amountGroup = document.getElementById('billing-amount-group');
  const paymentGroup = document.getElementById('billing-payment-group');
  const continueBtn = document.getElementById('billing-continue-btn');
  const payBtn = document.getElementById('billing-pay-btn');
  const backBtn = document.getElementById('billing-back-btn');
  if (step === 'amount') {
    if (amountGroup) amountGroup.style.display = '';
    if (paymentGroup) paymentGroup.style.display = 'none';
    if (continueBtn) continueBtn.style.display = '';
    if (payBtn) payBtn.style.display = 'none';
    if (backBtn) backBtn.style.display = 'none';
  } else {
    if (amountGroup) amountGroup.style.display = 'none';
    if (paymentGroup) paymentGroup.style.display = '';
    if (continueBtn) continueBtn.style.display = 'none';
    if (payBtn) payBtn.style.display = '';
    if (backBtn) backBtn.style.display = '';
  }
}

function setStatus(message, kind) {
  const el = document.getElementById('billing-modal-status');
  if (!el) return;
  if (!message) {
    el.style.display = 'none';
    el.textContent = '';
    return;
  }
  el.textContent = message;
  el.className = 'status ' + (kind || 'info');
  el.style.display = 'block';
}

function resetPaymentElement() {
  if (_paymentElement) {
    try { _paymentElement.unmount(); } catch (_) { /* ignore */ }
    try { _paymentElement.destroy(); } catch (_) { /* ignore */ }
  }
  _paymentElement = null;
  _elements = null;
  _paymentIntentId = null;
}

async function ensureStripe() {
  if (_stripe) return _stripe;
  if (!_publishableKey) {
    const client = await getClient();
    const cfg = await client.getStripeConfig();
    _publishableKey = cfg.publishable_key;
  }
  _stripe = Stripe(_publishableKey); // eslint-disable-line no-undef
  return _stripe;
}

async function openAddFundsModal() {
  if (_billingAvailable === false) return;
  const overlay = document.getElementById('billing-modal-overlay');
  if (!overlay) return;
  overlay.classList.add('is-open');
  overlay.setAttribute('aria-hidden', 'false');

  setStatus('');
  setModalStep('amount');
  resetPaymentElement();

  try {
    await ensureStripe();
  } catch (e) {
    logError('stripe-init', e);
    setStatus('Billing not available: ' + formatError(e), 'error');
  }
}

function closeBillingModal() {
  const overlay = document.getElementById('billing-modal-overlay');
  if (overlay) {
    overlay.classList.remove('is-open');
    overlay.setAttribute('aria-hidden', 'true');
  }
  resetPaymentElement();
  setModalStep('amount');
  setStatus('');
}

async function handleContinue() {
  const apiKey = getApiKey();
  if (!apiKey) return;

  const amountInput = document.getElementById('billing-amount');
  const amountCents = parseInt(amountInput?.value || '0', 10);
  if (!amountCents || amountCents < 500) {
    setStatus('Minimum amount is $5.00.', 'error');
    return;
  }

  const continueBtn = document.getElementById('billing-continue-btn');
  if (continueBtn) continueBtn.disabled = true;
  setStatus('');

  try {
    await ensureStripe();
    const client = await getClient();
    const intent = await client.createPaymentIntent(apiKey, amountCents);
    _paymentIntentId = intent.payment_intent_id;

    _elements = _stripe.elements({ clientSecret: intent.client_secret });
    _paymentElement = _elements.create('payment');
    _paymentElement.mount('#stripe-payment-element');
    setModalStep('payment');
  } catch (e) {
    logError('createPaymentIntent', e);
    setStatus('Could not start payment: ' + formatError(e), 'error');
  } finally {
    if (continueBtn) continueBtn.disabled = false;
  }
}

function handleBack() {
  resetPaymentElement();
  setStatus('');
  setModalStep('amount');
}

async function handlePay() {
  const apiKey = getApiKey();
  if (!apiKey || !_stripe || !_elements || !_paymentIntentId) return;

  const payBtn = document.getElementById('billing-pay-btn');
  const backBtn = document.getElementById('billing-back-btn');
  if (payBtn) payBtn.disabled = true;
  if (backBtn) backBtn.disabled = true;
  setStatus('');

  const intentId = _paymentIntentId;
  // Stripe redirects to return_url for methods that require it (crypto);
  // card payments complete inline when redirect: 'if_required' is set.
  const returnUrl = window.location.origin + window.location.pathname;

  try {
    const result = await _stripe.confirmPayment({
      elements: _elements,
      confirmParams: { return_url: returnUrl },
      redirect: 'if_required',
    });

    if (result.error) {
      throw new Error(result.error.message);
    }

    try {
      const client = await getClient();
      await client.confirmPayment(apiKey, intentId);
    } catch (confirmErr) {
      logError('confirmPayment', confirmErr, { intentId });
      setStatus('Payment processed — credit pending. Reference: ' + intentId, 'info');
      closeBillingModal();
      await loadBillingBalance();
      return;
    }

    closeBillingModal();
    await loadBillingBalance();
  } catch (e) {
    logError('payment', e, { intentId });
    setStatus('Payment failed: ' + formatError(e), 'error');
  } finally {
    if (payBtn) payBtn.disabled = false;
    if (backBtn) backBtn.disabled = false;
  }
}

export async function handleBillingReturn() {
  const params = new URLSearchParams(window.location.search);
  const intentId = params.get('payment_intent');
  const status = params.get('redirect_status');
  if (!intentId || !status) return;

  // Strip Stripe redirect params regardless of outcome so reloads don't retrigger.
  const cleanUrl = window.location.origin + window.location.pathname + window.location.hash;
  window.history.replaceState({}, '', cleanUrl);

  const apiKey = getApiKey();
  if (!apiKey) return;

  if (status !== 'succeeded') {
    showTopLevelStatus('Payment ' + status + '. Reference: ' + intentId, 'error');
    return;
  }

  try {
    const client = await getClient();
    await client.confirmPayment(apiKey, intentId);
    showTopLevelStatus('Credits added to your account.', 'success');
    await loadBillingBalance();
  } catch (e) {
    logError('handleBillingReturn', e, { intentId });
    showTopLevelStatus('Payment processed — credit pending. Reference: ' + intentId, 'info');
  }
}

function showTopLevelStatus(message, kind) {
  const el = document.getElementById('overview-status');
  if (!el) return;
  el.textContent = message;
  el.className = 'status ' + (kind || 'info');
  el.style.display = 'block';
}

export function initBilling() {
  const addFundsBtn = document.getElementById('btn-add-funds');
  const closeBtn = document.getElementById('billing-modal-close-btn');
  const cancelBtn = document.getElementById('billing-cancel-btn');
  const continueBtn = document.getElementById('billing-continue-btn');
  const backBtn = document.getElementById('billing-back-btn');
  const payBtn = document.getElementById('billing-pay-btn');

  if (addFundsBtn) addFundsBtn.addEventListener('click', openAddFundsModal);
  const noFundsLink = document.getElementById('no-funds-add-funds');
  if (noFundsLink) noFundsLink.addEventListener('click', (e) => { e.preventDefault(); openAddFundsModal(); });
  if (closeBtn) closeBtn.addEventListener('click', closeBillingModal);
  if (cancelBtn) cancelBtn.addEventListener('click', closeBillingModal);
  if (continueBtn) continueBtn.addEventListener('click', handleContinue);
  if (backBtn) backBtn.addEventListener('click', handleBack);
  if (payBtn) payBtn.addEventListener('click', handlePay);
}
