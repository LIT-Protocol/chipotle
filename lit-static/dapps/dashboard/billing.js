/**
 * Billing — Stripe integration, payment flow.
 *
 * Uses Stripe Payment Element which auto-renders whichever methods are
 * enabled on the Stripe account (card, USDC, USDP, ETH, SOL). Crypto
 * payments use a redirect flow; card payments complete inline when no
 * additional action is required.
 *
 * ChainSecured callers (CPL-285, CPL-286) authenticate to /billing/* with an
 * EIP-712 typed-data signature proving they hold the wallet's private key.
 * The wallet UI surfaces `primaryType: BillingAuth` and labelled fields so a
 * phishing dApp can't disguise the request as a generic message and replay
 * it against the secret-emitting endpoints. The signed payload is cached
 * for ~4 minutes (server allows ±5min skew, we leave a buffer) so the user
 * only sees one wallet popup per billing session.
 *
 * All in-flight billing requests are tied to a module AbortController and a
 * captured-credential snapshot — `clearBillingSession()` is called on logout
 * or wallet switch and aborts pending fetches; every post-await branch
 * re-checks `billingAuthKey() === captured` before mutating UI or calling
 * Stripe so a mid-flight session change can never credit the prior account.
 */

import {
  PRIMARY_TYPE_BILLING_AUTH,
  buildChainSecuredTypedData,
  signChainSecuredTypedData,
} from '../../core_sdk.js';
import {
  getApiKey,
  getChainSecuredHash,
  getChainSecuredWallet,
  getClient,
  getMode,
  hasUsageKeyOverride,
  LIST_PAGE_SIZE,
  setWalletsStore,
  setOnApiCallSuccess,
} from './auth.js';
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
const LITKEY_PAYMENT_URL = 'https://payments.litprotocol.com/payWithLitkey';

// AbortController for in-flight billing fetches. Recreated lazily; aborted on
// session change via clearBillingSession().
let _abortController = null;

// Cached wallet-auth payload. Re-used across billing requests within the
// timestamp window. { headerValue, expiresAtMs, walletAddress }
let _walletAuthCache = null;

function isAddress(value) {
  return /^0x[0-9a-fA-F]{40}$/.test(value || '');
}

function walletAddressFromItem(item) {
  return item && (item.wallet_address || item.address || item.name || '');
}

function pickAccountFundingWallet(wallets) {
  if (!Array.isArray(wallets) || wallets.length === 0) return '';
  const accountWallet = wallets.find((item) => {
    const label = String(item.description || item.name || '').toLowerCase();
    return label.includes('account master wallet') || label === 'amw';
  });
  return walletAddressFromItem(accountWallet);
}

/**
 * Resolve the wallet to prefill on the pay-with-LITKEY page. This MUST be the
 * account's billing wallet — the address the Stripe customer is keyed on
 * (`metadata.wallet_address`) — because the payments service maps the URL
 * wallet straight to a customer with no further resolution. The billing wallet
 * is preserved across admin-wallet rotation (CPL-313/CPL-324), so it can differ
 * from the currently connected wallet.
 */
async function resolveLitkeyPaymentWallet() {
  // ChainSecured / sovereign: the connected wallet may have rotated away from
  // the billing wallet, so read the authoritative value on-chain rather than
  // assuming the login wallet.
  if (getMode() === 'sovereign') {
    if (!isAddress(getChainSecuredWallet())) {
      throw new Error('No connected ChainSecured wallet — sign in first.');
    }
    const client = await getClient();
    const wallet = await client.getBillingWalletAddress({ apiKey: '' });
    if (!isAddress(wallet)) throw new Error('No billing wallet is available for LITKEY payment.');
    return wallet;
  }

  // API-key mode: managed accounts never rotate their admin wallet, so the
  // billing wallet equals the Account Master Wallet from listWallets (on-chain
  // `getBillingWalletAddress` falls back to the admin wallet for these accounts).
  if (hasUsageKeyOverride()) throw new Error('Clear Usage Key Override before adding funds.');
  const apiKey = getApiKey();
  if (!apiKey) throw new Error('No account wallet is available for LITKEY payment.');
  const client = await getClient();
  const wallets = await client.listWallets({ apiKey, pageNumber: '0', pageSize: LIST_PAGE_SIZE });
  setWalletsStore(wallets || []);

  const wallet = pickAccountFundingWallet(wallets);
  if (!isAddress(wallet)) throw new Error('No billing wallet is available for LITKEY payment.');
  return wallet;
}

async function openLitkeyPaymentPage() {
  const btn = document.getElementById('billing-litkey-btn');
  if (btn) btn.disabled = true;
  setStatus('Preparing LITKEY payment link…', 'info');
  try {
    const wallet = await resolveLitkeyPaymentWallet();
    const url = LITKEY_PAYMENT_URL + '?wallet=' + encodeURIComponent(wallet);
    window.open(url, '_blank', 'noopener,noreferrer');
  } catch (e) {
    logError('openLitkeyPaymentPage', e);
    setStatus('Could not open LITKEY payment: ' + formatError(e), 'error');
  } finally {
    if (btn) btn.disabled = false;
  }
}

/**
 * True when we have a valid (unexpired, current-wallet) wallet-auth header.
 * Used to decide whether to load the balance silently vs. wait for the user
 * to click Add Funds — we never want to auto-trigger a wallet popup just to
 * render the topbar balance (CPL-285 review feedback).
 */
function hasValidWalletAuthCache() {
  if (!_walletAuthCache) return false;
  const wallet = getChainSecuredWallet();
  if (!wallet || _walletAuthCache.walletAddress.toLowerCase() !== wallet.toLowerCase()) return false;
  return _walletAuthCache.expiresAtMs > Date.now() + 10_000;
}

// Server's TIMESTAMP_SKEW_SECONDS is 300 (lit-api-server/src/core/eip712.rs);
// we cache for 4 min to leave a 1-min safety buffer against clock skew +
// in-flight latency.
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
 * Abort any in-flight billing requests and clear the cached wallet-auth
 * payload. Called on logout, mode switch, and wallet change so a mid-flight
 * fetch can't credit the prior account.
 */
export function clearBillingSession() {
  if (_abortController) {
    _abortController.abort();
    _abortController = null;
  }
  _walletAuthCache = null;
  if (_balanceRefreshDebounceTimer) {
    clearTimeout(_balanceRefreshDebounceTimer);
    _balanceRefreshDebounceTimer = null;
  }
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

/**
 * Get a cached wallet-auth header, or prompt the wallet to sign a fresh one.
 * Returns base64(JSON{typed_data, signature}) for the X-Wallet-Auth header.
 * Server validates `primaryType: "BillingAuth"` against the canonical schema
 * in `lit-api-server/src/core/eip712.rs` (CPL-286).
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
  const typedData = buildChainSecuredTypedData({
    primaryType: PRIMARY_TYPE_BILLING_AUTH,
    address: wallet,
    chainId: chainIdNum,
    issuedAt,
  });

  const { connectWallet } = await import('../../wallet_connect.js');
  const { signer } = await connectWallet({ chainId: chainIdNum, rpcUrl: client.rpcUrl });
  const { typed_data, signature } = await signChainSecuredTypedData(signer, typedData);

  const headerValue = btoa(JSON.stringify({ typed_data, signature }));
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
 * obtain the EIP-712 wallet-auth header (prompting the wallet on cache miss).
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
      // In sovereign mode never auto-trigger a wallet popup just to render
      // the topbar balance. Only load the balance when we already hold a
      // valid wallet-auth cache (e.g. immediately after the user funded).
      // Otherwise the user explicitly opts in by clicking Add Funds —
      // `openAddFundsModal` primes the auth and refreshes the balance after
      // a successful payment.
      if (getMode() === 'sovereign') {
        if (hasValidWalletAuthCache()) {
          loadBillingBalance();
        } else if (balanceEl && !balanceEl.textContent) {
          balanceEl.textContent = '—';
        }
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

// Billing endpoints we must NOT re-trigger from the post-call hook below —
// refreshing the balance after `getBillingBalance` would recurse forever, and
// firing during the Stripe payment dance (`createPaymentIntent` /
// `confirmPayment`) would race the modal's own balance refresh on completion.
const BILLING_METHODS = new Set([
  'getBillingBalance',
  'getStripeConfig',
  'createPaymentIntent',
  'confirmPayment',
]);

// Debounce timer so a burst of API calls (e.g. preloadAllTables fires 4 in
// parallel on sign-in) coalesces into a single balance refresh.
let _balanceRefreshDebounceTimer = null;
const BALANCE_REFRESH_DEBOUNCE_MS = 200;

/**
 * Called after every successful client API call (NODE-4971). Refreshes the
 * Stripe credit balance display so the topbar reflects the latest balance
 * after any dashboard activity. No-op when:
 *   - the method itself is a billing endpoint (would recurse),
 *   - no auth key / billing not yet known to be available,
 *   - a usage-key override is active (the topbar balance belongs to the
 *     account key, not the override, and is hidden in this mode anyway),
 *   - we're in sovereign mode without a cached wallet-auth header (we won't
 *     trigger a wallet popup just to refresh the topbar — same rule as
 *     refreshBillingUI()).
 */
function refreshBalanceFromApiCall(methodName) {
  if (BILLING_METHODS.has(methodName)) return;
  if (!billingAuthKey()) return;
  if (hasUsageKeyOverride()) return;
  if (_billingAvailable !== true) return;
  if (getMode() === 'sovereign' && !hasValidWalletAuthCache()) return;

  if (_balanceRefreshDebounceTimer) return;
  _balanceRefreshDebounceTimer = setTimeout(() => {
    _balanceRefreshDebounceTimer = null;
    loadBillingBalance();
  }, BALANCE_REFRESH_DEBOUNCE_MS);
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
    return;
  }

  // Sovereign mode: prime the wallet-auth cache now (one EIP-712 typed-data
  // popup) so the user's later Continue/Pay clicks — and the balance refresh
  // that follows — flow without a second prompt. If they cancel the signature,
  // surface that in the modal status and leave the modal open so they can retry.
  if (getMode() === 'sovereign' && !hasValidWalletAuthCache()) {
    setStatus('Approve the wallet signature to enable billing for this session…', 'info');
    try {
      await getWalletAuthHeader();
      setStatus('');
    } catch (e) {
      logError('wallet-auth-prime', e);
      setStatus('Wallet signature required to fund a ChainSecured account: ' + formatError(e), 'error');
      return;
    }
    // Now that we have a valid cache, surface the actual balance.
    loadBillingBalance();
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
  const apiKey = billingAuthKey();
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

  const ctrl = ensureAbortController();
  try {
    await ensureStripe();
    if (billingAuthKey() !== apiKey || ctrl.signal.aborted) return;
    const client = await getClient();
    if (billingAuthKey() !== apiKey || ctrl.signal.aborted) return;
    const opts = await billingRequestOptions(ctrl.signal);
    if (billingAuthKey() !== apiKey || ctrl.signal.aborted) return;
    const intent = await client.createPaymentIntent(apiKey, amountCents, opts);
    if (billingAuthKey() !== apiKey || ctrl.signal.aborted) return;
    _paymentIntentId = intent.payment_intent_id;

    _elements = _stripe.elements({ clientSecret: intent.client_secret });
    // Let Stripe render the minimal address fields each payment method needs
    // (country + postal for cards, nothing for most crypto). Card networks
    // require postal_code for AVS, so we can't suppress it without breaking
    // real charges.
    _paymentElement = _elements.create('payment');
    _paymentElement.mount('#stripe-payment-element');
    setModalStep('payment');
  } catch (e) {
    if (e && (e.name === 'AbortError' || ctrl.signal.aborted)) return;
    if (getMode() === 'sovereign' && isAuthError(e)) evictWalletAuthCache();
    logError('createPaymentIntent', e);
    if (billingAuthKey() === apiKey) {
      setStatus('Could not start payment: ' + formatError(e), 'error');
    }
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
  const apiKey = billingAuthKey();
  if (!apiKey || !_stripe || !_elements || !_paymentIntentId) return;

  const payBtn = document.getElementById('billing-pay-btn');
  const backBtn = document.getElementById('billing-back-btn');
  if (payBtn) payBtn.disabled = true;
  if (backBtn) backBtn.disabled = true;
  setStatus('');

  const intentId = _paymentIntentId;
  const ctrl = ensureAbortController();
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

    if (billingAuthKey() !== apiKey || ctrl.signal.aborted) {
      // Payment succeeded on Stripe but the session changed before we could
      // credit. The PaymentIntent is settled against the prior wallet's
      // Stripe customer; surface the intent ID for manual reconciliation.
      setStatus('Payment processed — session changed before crediting. Reference: ' + intentId, 'info');
      return;
    }

    try {
      const client = await getClient();
      if (billingAuthKey() !== apiKey || ctrl.signal.aborted) return;
      // Re-fetch options in case the cached wallet-auth header expired during
      // the Stripe.js round-trip.
      const opts = await billingRequestOptions(ctrl.signal);
      if (billingAuthKey() !== apiKey || ctrl.signal.aborted) return;
      await client.confirmPayment(apiKey, intentId, opts);
    } catch (confirmErr) {
      if (confirmErr && (confirmErr.name === 'AbortError' || ctrl.signal.aborted)) return;
      logError('confirmPayment', confirmErr, { intentId });
      closeBillingModal();
      showTopLevelStatus('Payment processed — credit pending. Reference: ' + intentId, 'info');
      await loadBillingBalance();
      return;
    }

    closeBillingModal();
    await loadBillingBalance();
  } catch (e) {
    if (e && (e.name === 'AbortError' || ctrl.signal.aborted)) return;
    if (getMode() === 'sovereign' && isAuthError(e)) evictWalletAuthCache();
    logError('payment', e, { intentId });
    if (billingAuthKey() === apiKey) {
      setStatus('Payment failed: ' + formatError(e), 'error');
    }
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

  const apiKey = billingAuthKey();
  if (!apiKey) return;

  // Stripe redirect_status values: `succeeded`, `processing`,
  // `requires_payment_method`, `requires_action`, `failed`, `canceled`.
  // For `succeeded` and `processing` we still call the backend so it can
  // credit (succeeded) or record-pending (processing) — Stripe only flips
  // crypto intents to `succeeded` after on-chain confirmation, but the
  // settled state can arrive while the user is mid-redirect.
  if (status !== 'succeeded' && status !== 'processing') {
    showTopLevelStatus('Payment ' + status + '. Reference: ' + intentId, 'error');
    return;
  }

  const ctrl = ensureAbortController();
  try {
    const client = await getClient();
    if (billingAuthKey() !== apiKey || ctrl.signal.aborted) return;
    const opts = await billingRequestOptions(ctrl.signal);
    if (billingAuthKey() !== apiKey || ctrl.signal.aborted) return;
    await client.confirmPayment(apiKey, intentId, opts);
    if (billingAuthKey() !== apiKey || ctrl.signal.aborted) return;
    if (status === 'succeeded') {
      showTopLevelStatus('Credits added to your account.', 'success');
    } else {
      showTopLevelStatus('Payment processing — credits will appear once settled. Reference: ' + intentId, 'info');
    }
    await loadBillingBalance();
  } catch (e) {
    if (e && (e.name === 'AbortError' || ctrl.signal.aborted)) return;
    if (getMode() === 'sovereign' && isAuthError(e)) evictWalletAuthCache();
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
  const litkeyBtn = document.getElementById('billing-litkey-btn');

  if (addFundsBtn) addFundsBtn.addEventListener('click', openAddFundsModal);
  if (litkeyBtn) litkeyBtn.addEventListener('click', openLitkeyPaymentPage);
  const noFundsLink = document.getElementById('no-funds-add-funds');
  if (noFundsLink) noFundsLink.addEventListener('click', (e) => { e.preventDefault(); openAddFundsModal(); });
  if (closeBtn) closeBtn.addEventListener('click', closeBillingModal);
  if (cancelBtn) cancelBtn.addEventListener('click', closeBillingModal);
  if (continueBtn) continueBtn.addEventListener('click', handleContinue);
  if (backBtn) backBtn.addEventListener('click', handleBack);
  if (payBtn) payBtn.addEventListener('click', handlePay);

  setOnApiCallSuccess(refreshBalanceFromApiCall);
}
