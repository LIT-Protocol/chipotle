/**
 * SCA recovery page driver (Phase 8.5).
 *
 * Flow when user clicks the email link `…/recover_topup.html?token=…`:
 *
 *   1. Read `token` from the URL.
 *   2. GET `/billing/auto_topup_resume?token=…` on lit-payments. Server
 *      returns `{ payment_intent_id, client_secret, publishable_key }`
 *      (and clears the token internally only AFTER its Stripe retrieve
 *      succeeds — codex P2 #3 fix).
 *   3. Mount Stripe Payment Element bound to the PI's client_secret and
 *      let `confirmPayment` render the bank's 3DS challenge.
 *   4. On `succeeded`, POST `/billing/auto_topup_resume/complete` with
 *      the PI id; that endpoint verifies status server-side and applies
 *      the sync credit.
 *
 * This page is unauthenticated by BillingAuth: the `recovery_token` IS
 * the credential. It's single-use, 24h-expiring, and tied to a specific
 * pending PI.
 */

import { getLitPaymentsBaseUrl } from './auth.js';

const statusEl = document.getElementById('recover-status');
const confirmBtn = document.getElementById('recover-confirm-btn');

let _stripe = null;
let _elements = null;
let _paymentElement = null;
let _paymentIntentId = null;
let _clientSecret = null;

(async function main() {
  const token = new URLSearchParams(location.search).get('token');
  if (!token) {
    setStatus('Missing recovery token. Use the link from your email.', 'error');
    return;
  }
  try {
    const intent = await fetchResume(token);
    _paymentIntentId = intent.payment_intent_id;
    _clientSecret = intent.client_secret;
    // eslint-disable-next-line no-undef
    _stripe = Stripe(intent.publishable_key);
    _elements = _stripe.elements({ clientSecret: _clientSecret });
    _paymentElement = _elements.create('payment');
    _paymentElement.mount('#recover-payment-element');
    confirmBtn.disabled = false;
    setStatus('Enter the verification details requested by your bank, then click Confirm.', 'info');
  } catch (e) {
    setStatus(formatError(e), 'error');
  }
})();

confirmBtn.addEventListener('click', async () => {
  confirmBtn.disabled = true;
  setStatus('Contacting your bank…', 'info');
  try {
    const result = await _stripe.confirmPayment({
      elements: _elements,
      // No redirect_url: we want confirmCardPayment-style behaviour where
      // the result comes back to this page rather than navigating away.
      redirect: 'if_required',
    });
    if (result.error) {
      setStatus(result.error.message || 'Authentication failed.', 'error');
      confirmBtn.disabled = false;
      return;
    }
    const pi = result.paymentIntent;
    if (!pi || pi.status !== 'succeeded') {
      setStatus(
        `Bank returned status "${pi?.status || 'unknown'}". Please try again or use a different card.`,
        'error',
      );
      confirmBtn.disabled = false;
      return;
    }
    setStatus('Applying credit…', 'info');
    await postComplete(_paymentIntentId);
    setStatus('Topped up! Your account credit is updated. You can close this tab.', 'success');
  } catch (e) {
    setStatus(formatError(e), 'error');
    confirmBtn.disabled = false;
  }
});

async function fetchResume(token) {
  const base = getLitPaymentsBaseUrl();
  const res = await fetch(
    `${base}/billing/auto_topup_resume?token=${encodeURIComponent(token)}`,
  );
  const text = await res.text();
  let json;
  try { json = text ? JSON.parse(text) : null; } catch { json = null; }
  if (!res.ok) {
    const msg = json && json.message ? json.message : text || `HTTP ${res.status}`;
    throw new Error(msg);
  }
  return json;
}

async function postComplete(paymentIntentId) {
  const base = getLitPaymentsBaseUrl();
  const res = await fetch(`${base}/billing/auto_topup_resume/complete`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ payment_intent_id: paymentIntentId }),
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

function setStatus(msg, tone) {
  statusEl.textContent = msg || '';
  statusEl.className = tone || 'info';
  statusEl.style.display = msg ? '' : 'none';
}

function formatError(e) {
  if (!e) return 'Unknown error.';
  if (typeof e === 'string') return e;
  return e.message || String(e);
}
