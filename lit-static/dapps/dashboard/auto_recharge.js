/**
 * Auto-recharge UI (Phase 8 of the auto top-up feature).
 *
 * Wires three new bits of UX into the existing billing tab:
 *
 *   1. Status banner — shows "off / on / pending SCA / paused after
 *      failures" for the user's current `auto_topup_config` row, with a
 *      `[Manage]` CTA that opens the modal.
 *
 *   2. Auto-recharge modal — toggle, "drops to" / "restore to" inputs
 *      (recharge amount displayed read-only as the difference), monthly
 *      cap toggle + input, card picker, consent checkbox. Save persists
 *      via `PUT /billing/auto_topup_config`.
 *
 *   3. Save-card sub-flow — `POST /billing/setup_intent` returns a
 *      `client_secret`; Stripe.js mounts a Payment Element in setup mode
 *      and `confirmCardSetup` handles 3DS automatically for SCA cards.
 *      Resulting `pm_xxx` populates the modal's card picker.
 *
 * All endpoints are on **lit-payments** (port 8001 in dev) — separate
 * from the legacy `client.*` calls which go to `lit-api-server`.
 *
 * Auth: reuses `getWalletAuthHeader()` from billing.js (sovereign mode)
 * or the cached API key (api mode). The SCA recovery page does NOT use
 * either — the `recovery_token` IS the credential.
 */

import { getMode, getApiKey } from './auth.js';
import { getLitPaymentsBaseUrl } from './auth.js';
import { getWalletAuthHeader } from './billing.js';

// ─── State ──────────────────────────────────────────────────────────────────

let _config = null;            // last-known AutoTopupConfigRow or null
let _stripe = null;            // lazy-init Stripe.js
let _publishableKey = null;    // populated by save-card flow
let _addCardElements = null;   // Stripe Elements instance for save-card
let _addCardElement = null;    // PaymentElement instance
let _addCardClientSecret = null;
let _stagedPaymentMethodId = null; // freshly saved pm_xxx awaiting Save

// ─── Public entrypoints ─────────────────────────────────────────────────────

export async function initAutoRecharge() {
  attachModalEventHandlers();
  attachAddCardModalEventHandlers();
  try {
    _config = await fetchConfig();
  } catch (e) {
    logError('init-auto-recharge', e);
    _config = null;
  }
  renderStatusBanner();
}

// ─── Fetch / persist ────────────────────────────────────────────────────────

async function authHeaders() {
  // Build the auth headers our lit-payments dashboard endpoints expect.
  //
  // DEV-ONLY bypass: `window.__LIT_DEV_WALLET__` or
  // `sessionStorage.__lit_dev_wallet__` lets the Phase 8 QA harness
  // drive the dashboard without a connected wallet. The matching
  // server-side flag (`LIT_DEV_WALLET_BYPASS=1`) is required for the
  // backend to accept the header. Disabled in production deploys.
  const headers = { 'Content-Type': 'application/json' };
  const devWallet =
    (typeof window !== 'undefined' && window.__LIT_DEV_WALLET__) ||
    (typeof sessionStorage !== 'undefined' && sessionStorage.getItem('__lit_dev_wallet__')) ||
    null;
  if (devWallet) {
    headers['X-Dev-Wallet'] = devWallet;
    return headers;
  }
  if (getMode() === 'sovereign') {
    headers['X-Wallet-Auth'] = await getWalletAuthHeader();
  } else {
    const key = getApiKey();
    if (key) headers['X-Api-Key'] = key;
  }
  return headers;
}

async function fetchConfig() {
  const base = getLitPaymentsBaseUrl();
  const res = await fetch(`${base}/billing/auto_topup_config`, {
    method: 'GET',
    headers: await authHeaders(),
  });
  if (res.status === 401) throw new Error('Wallet signature failed.');
  if (res.status === 400) return null; // "no Stripe customer yet" — same UX as off
  if (res.status === 501) return null; // API-mode caller
  if (!res.ok) throw new Error(`GET /billing/auto_topup_config -> ${res.status}`);
  return res.json();
}

async function saveConfig(body) {
  const base = getLitPaymentsBaseUrl();
  const res = await fetch(`${base}/billing/auto_topup_config`, {
    method: 'PUT',
    headers: await authHeaders(),
    body: JSON.stringify(body),
  });
  const text = await res.text();
  let json;
  try { json = text ? JSON.parse(text) : null; } catch { json = null; }
  if (!res.ok) {
    const msg = json && json.message ? json.message : text || `HTTP ${res.status}`;
    throw new Error(msg);
  }
  return json;
}

async function createSetupIntent() {
  const base = getLitPaymentsBaseUrl();
  const res = await fetch(`${base}/billing/setup_intent`, {
    method: 'POST',
    headers: await authHeaders(),
  });
  const text = await res.text();
  let json;
  try { json = text ? JSON.parse(text) : null; } catch { json = null; }
  if (!res.ok) {
    const msg = json && json.message ? json.message : text || `HTTP ${res.status}`;
    throw new Error(msg);
  }
  return json; // { client_secret, publishable_key }
}

// ─── Status banner ──────────────────────────────────────────────────────────

function renderStatusBanner() {
  const host = document.getElementById('auto-recharge-banner');
  if (!host) return;
  host.innerHTML = '';
  if (!_config) {
    host.appendChild(makeBanner({
      tone: 'info',
      title: 'Auto recharge',
      body: 'Automatically add credits when your balance drops below a threshold.',
      ctaLabel: 'Set up',
      ctaAction: () => openAutoRechargeModal(),
    }));
    return;
  }
  if (_config.disabled_reason === 'requires_action' && _config.pending_action_pi_id) {
    host.appendChild(makeBanner({
      tone: 'error',
      title: 'Action required',
      body: 'Your most recent auto top-up needs you to confirm with your bank. Check your email for the verification link.',
      ctaLabel: 'Manage',
      ctaAction: () => openAutoRechargeModal(),
    }));
    return;
  }
  if (_config.disabled_reason === 'failures') {
    host.appendChild(makeBanner({
      tone: 'error',
      title: 'Auto recharge paused',
      body: 'Three consecutive charges failed. Update your card and re-enable.',
      ctaLabel: 'Manage',
      ctaAction: () => openAutoRechargeModal(),
    }));
    return;
  }
  if (_config.enabled) {
    const t = dollarsFromCents(_config.threshold_cents);
    const amt = dollarsFromCents(_config.topup_amount_cents);
    const cap = dollarsFromCents(_config.monthly_cap_cents);
    const cardSuffix = formatCardSuffix(_config.payment_method_id);
    host.appendChild(makeBanner({
      tone: 'success',
      title: 'Auto recharge is on',
      body: `When your balance drops below $${t}, we'll charge $${amt} to ${cardSuffix}, up to ~$${cap}/month.`,
      ctaLabel: 'Modify',
      ctaAction: () => openAutoRechargeModal(),
    }));
    return;
  }
  // enabled=false, no special disabled_reason — treat as "off".
  host.appendChild(makeBanner({
    tone: 'info',
    title: 'Auto recharge is off',
    body: 'Turn this on to keep your balance topped up automatically.',
    ctaLabel: 'Enable',
    ctaAction: () => openAutoRechargeModal(),
  }));
}

function makeBanner({ tone, title, body, ctaLabel, ctaAction }) {
  const wrap = document.createElement('div');
  wrap.className = `auto-recharge-banner auto-recharge-banner-${tone}`;
  const left = document.createElement('div');
  left.className = 'auto-recharge-banner-left';
  const h = document.createElement('strong');
  h.textContent = title;
  const p = document.createElement('p');
  p.textContent = body;
  left.appendChild(h);
  left.appendChild(p);
  wrap.appendChild(left);
  if (ctaLabel) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'btn btn-outline btn-sm';
    btn.textContent = ctaLabel;
    btn.addEventListener('click', ctaAction);
    wrap.appendChild(btn);
  }
  return wrap;
}

function dollarsFromCents(cents) {
  if (cents == null) return '0';
  return (Number(cents) / 100).toFixed(2).replace(/\.00$/, '');
}

function formatCardSuffix(pmId) {
  if (!pmId) return 'your saved card';
  // We don't keep card metadata in our DB; pm_xxx is opaque here. The
  // backend has the card; this UI surfaces only what it has. (The full
  // last4 + brand could be fetched from Stripe in a future iteration.)
  return 'your saved card';
}

// ─── Auto-recharge modal ────────────────────────────────────────────────────

function attachModalEventHandlers() {
  document.getElementById('auto-recharge-modal-close-btn')?.addEventListener(
    'click',
    closeAutoRechargeModal,
  );
  document.getElementById('auto-recharge-cancel-btn')?.addEventListener(
    'click',
    closeAutoRechargeModal,
  );
  document.getElementById('auto-recharge-save-btn')?.addEventListener(
    'click',
    handleSave,
  );
  document.getElementById('auto-recharge-enabled-toggle')?.addEventListener(
    'change',
    syncEnabledToggleUI,
  );
  document.getElementById('auto-recharge-cap-toggle')?.addEventListener(
    'change',
    syncCapToggleUI,
  );
  document.getElementById('auto-recharge-drops-to')?.addEventListener(
    'input',
    updateComputedAmount,
  );
  document.getElementById('auto-recharge-restore-to')?.addEventListener(
    'input',
    updateComputedAmount,
  );
  document.getElementById('auto-recharge-add-card-btn')?.addEventListener(
    'click',
    () => openAddCardModal(),
  );
}

export async function openAutoRechargeModal() {
  const overlay = document.getElementById('auto-recharge-modal-overlay');
  if (!overlay) return;
  // Re-fetch on each open so the modal reflects whatever may have
  // changed since page load (other tabs, dashboard webhook tick, etc.).
  setModalStatus('');
  try {
    _config = await fetchConfig();
  } catch (e) {
    setModalStatus(formatError(e), 'error');
  }
  populateModalFromConfig();
  overlay.classList.add('is-open');
  overlay.setAttribute('aria-hidden', 'false');
}

function closeAutoRechargeModal() {
  const overlay = document.getElementById('auto-recharge-modal-overlay');
  if (!overlay) return;
  overlay.classList.remove('is-open');
  overlay.setAttribute('aria-hidden', 'true');
  setModalStatus('');
}

function populateModalFromConfig() {
  const enabled = !!(_config && _config.enabled);
  setVal('auto-recharge-enabled-toggle', 'checked', enabled);
  const drops = dollarsFromCents(_config?.threshold_cents ?? 500);
  const topup = Number(_config?.topup_amount_cents ?? 1000);
  const threshold = Number(_config?.threshold_cents ?? 500);
  const restore = ((threshold + topup) / 100).toFixed(2).replace(/\.00$/, '');
  setVal('auto-recharge-drops-to', 'value', drops);
  setVal('auto-recharge-restore-to', 'value', restore);

  const capEnabled = !!(_config && _config.monthly_cap_cents);
  setVal('auto-recharge-cap-toggle', 'checked', capEnabled);
  setVal(
    'auto-recharge-cap-amount',
    'value',
    dollarsFromCents(_config?.monthly_cap_cents ?? 10_000),
  );

  // Card selection: use staged-just-saved card if any, else config's
  // current card, else leave the picker showing "no card on file".
  const pm = _stagedPaymentMethodId || _config?.payment_method_id || null;
  renderCardPicker(pm);

  setVal('auto-recharge-consent', 'checked', false);
  syncEnabledToggleUI();
  syncCapToggleUI();
  updateComputedAmount();
}

function renderCardPicker(pmId) {
  const label = document.getElementById('auto-recharge-card-label');
  if (!label) return;
  label.textContent = pmId ? `Card on file: ${pmId}` : 'No card saved yet';
  label.dataset.pmId = pmId || '';
}

function syncEnabledToggleUI() {
  const enabled = !!document.getElementById('auto-recharge-enabled-toggle')?.checked;
  const fieldset = document.getElementById('auto-recharge-fields');
  if (fieldset) {
    fieldset.style.opacity = enabled ? '1' : '0.5';
    fieldset.querySelectorAll('input').forEach((el) => {
      if (el.id !== 'auto-recharge-enabled-toggle') el.disabled = !enabled;
    });
  }
  // Consent is only meaningful when enabling.
  const consent = document.getElementById('auto-recharge-consent-row');
  if (consent) consent.style.display = enabled ? '' : 'none';
}

function syncCapToggleUI() {
  const on = !!document.getElementById('auto-recharge-cap-toggle')?.checked;
  const input = document.getElementById('auto-recharge-cap-amount');
  if (input) input.disabled = !on;
}

function updateComputedAmount() {
  const drops = parseFloat(getVal('auto-recharge-drops-to') || '0') || 0;
  const restore = parseFloat(getVal('auto-recharge-restore-to') || '0') || 0;
  const amount = Math.max(0, restore - drops);
  const display = document.getElementById('auto-recharge-amount-display');
  if (display) display.textContent = `$${amount.toFixed(2).replace(/\.00$/, '')}`;
}

async function handleSave() {
  setModalStatus('');
  const enabled = !!document.getElementById('auto-recharge-enabled-toggle')?.checked;
  const drops = parseFloat(getVal('auto-recharge-drops-to') || '0') || 0;
  const restore = parseFloat(getVal('auto-recharge-restore-to') || '0') || 0;
  const capOn = !!document.getElementById('auto-recharge-cap-toggle')?.checked;
  const cap = parseFloat(getVal('auto-recharge-cap-amount') || '0') || 0;
  const consent = !!document.getElementById('auto-recharge-consent')?.checked;
  const pmId =
    document.getElementById('auto-recharge-card-label')?.dataset.pmId || null;

  if (enabled) {
    if (!consent) {
      setModalStatus(
        'Tick the consent checkbox to enable auto recharge.',
        'error',
      );
      return;
    }
    if (drops <= 0 || restore <= drops) {
      setModalStatus(
        'Restore balance must be higher than the trigger threshold.',
        'error',
      );
      return;
    }
    const topupCents = Math.round((restore - drops) * 100);
    if (topupCents < 500) {
      setModalStatus(
        'Recharge amount must be at least $5.',
        'error',
      );
      return;
    }
    if (!capOn || cap < (topupCents / 100)) {
      setModalStatus(
        'Monthly limit must be at least the recharge amount.',
        'error',
      );
      return;
    }
    if (!pmId) {
      setModalStatus(
        'Save a card before enabling auto recharge.',
        'error',
      );
      return;
    }
  }

  const body = enabled
    ? {
        enabled: true,
        threshold_cents: Math.round(drops * 100),
        topup_amount_cents: Math.round((restore - drops) * 100),
        monthly_cap_cents: Math.round(cap * 100),
        payment_method_id: pmId,
        consent_version: 'v1',
      }
    : {
        enabled: false,
        threshold_cents: null,
        topup_amount_cents: null,
        monthly_cap_cents: null,
        payment_method_id: null,
        consent_version: null,
      };

  const saveBtn = document.getElementById('auto-recharge-save-btn');
  if (saveBtn) saveBtn.disabled = true;
  try {
    _config = await saveConfig(body);
    _stagedPaymentMethodId = null;
    closeAutoRechargeModal();
    renderStatusBanner();
  } catch (e) {
    setModalStatus(formatError(e), 'error');
  } finally {
    if (saveBtn) saveBtn.disabled = false;
  }
}

// ─── Save-card sub-flow ─────────────────────────────────────────────────────

function attachAddCardModalEventHandlers() {
  document.getElementById('add-card-modal-close-btn')?.addEventListener(
    'click',
    closeAddCardModal,
  );
  document.getElementById('add-card-cancel-btn')?.addEventListener(
    'click',
    closeAddCardModal,
  );
  document.getElementById('add-card-save-btn')?.addEventListener(
    'click',
    handleAddCardSave,
  );
}

async function openAddCardModal() {
  setAddCardStatus('');
  const overlay = document.getElementById('add-card-modal-overlay');
  if (!overlay) return;
  overlay.classList.add('is-open');
  overlay.setAttribute('aria-hidden', 'false');

  try {
    const intent = await createSetupIntent();
    _addCardClientSecret = intent.client_secret;
    _publishableKey = intent.publishable_key;
    if (!_stripe) {
      // eslint-disable-next-line no-undef
      _stripe = Stripe(_publishableKey);
    }
    if (_addCardElements) {
      try { _addCardElement?.unmount(); } catch (_) { /* ignore */ }
    }
    _addCardElements = _stripe.elements({ clientSecret: _addCardClientSecret });
    _addCardElement = _addCardElements.create('payment');
    _addCardElement.mount('#add-card-payment-element');
  } catch (e) {
    setAddCardStatus(formatError(e), 'error');
  }
}

function closeAddCardModal() {
  const overlay = document.getElementById('add-card-modal-overlay');
  if (!overlay) return;
  overlay.classList.remove('is-open');
  overlay.setAttribute('aria-hidden', 'true');
  if (_addCardElement) {
    try { _addCardElement.unmount(); } catch (_) { /* ignore */ }
    _addCardElement = null;
    _addCardElements = null;
  }
}

async function handleAddCardSave() {
  if (!_stripe || !_addCardElements) {
    setAddCardStatus('Card form not initialised yet.', 'error');
    return;
  }
  setAddCardStatus('Saving card…', 'info');
  const saveBtn = document.getElementById('add-card-save-btn');
  if (saveBtn) saveBtn.disabled = true;
  try {
    const result = await _stripe.confirmSetup({
      elements: _addCardElements,
      redirect: 'if_required',
    });
    if (result.error) {
      setAddCardStatus(result.error.message || 'Save card failed.', 'error');
      return;
    }
    const pmId = result.setupIntent?.payment_method;
    if (!pmId) {
      setAddCardStatus(
        'Stripe returned no payment_method id; please retry.',
        'error',
      );
      return;
    }
    _stagedPaymentMethodId = pmId;
    closeAddCardModal();
    renderCardPicker(pmId);
    setModalStatus('Card saved — ready to enable auto recharge.', 'success');
  } catch (e) {
    setAddCardStatus(formatError(e), 'error');
  } finally {
    if (saveBtn) saveBtn.disabled = false;
  }
}

// ─── Utilities ──────────────────────────────────────────────────────────────

function setModalStatus(msg, tone) {
  setStatus('auto-recharge-modal-status', msg, tone);
}

function setAddCardStatus(msg, tone) {
  setStatus('add-card-modal-status', msg, tone);
}

function setStatus(id, msg, tone) {
  const el = document.getElementById(id);
  if (!el) return;
  el.textContent = msg || '';
  el.classList.remove('info', 'error', 'success');
  if (msg && tone) el.classList.add(tone);
  el.style.display = msg ? '' : 'none';
}

function setVal(id, prop, val) {
  const el = document.getElementById(id);
  if (el) el[prop] = val;
}

function getVal(id) {
  const el = document.getElementById(id);
  return el ? el.value : '';
}

function formatError(e) {
  if (!e) return 'Unknown error.';
  if (typeof e === 'string') return e;
  return e.message || String(e);
}

function logError(tag, e) {
  // eslint-disable-next-line no-console
  console.error(`[auto_recharge:${tag}]`, e);
}
