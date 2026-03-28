/**
 * Lit Express Node Dashboard – single-page app after login (TailAdmin-style)
 * Uses core_sdk.js; apiKey and session state in sessionStorage.
 */

const STORAGE_KEY_API = 'accountconfig_api_key';
const STORAGE_KEY_THEME = 'accountconfig_theme';
const LIST_PAGE_SIZE = '20';

function getApiKey() {
  return sessionStorage.getItem(STORAGE_KEY_API) || '';
}

function setApiKey(v) {
  if (v) sessionStorage.setItem(STORAGE_KEY_API, v);
  else sessionStorage.removeItem(STORAGE_KEY_API);
  updateAuthUI();
}

function getBaseUrl() {
  // Falls back to localhost for local development.
  // For deployments, __LIT_API_BASE_URL__ is replaced at image build time
  // via ARG BASE_URL in Dockerfile.lit-static.
  if (typeof location !== 'undefined' && location.origin && location.origin.indexOf('localhost') !== -1)
    return 'http://localhost:8000';
  return '__LIT_API_BASE_URL__';
}

function updateAuthUI() {
  const hasKey = !!getApiKey();
  document.body.classList.toggle('has-api-key', hasKey);
  const balanceEl = document.getElementById('billing-balance-display');
  const addFundsBtn = document.getElementById('btn-add-funds');
  if (balanceEl) balanceEl.style.display = hasKey ? '' : 'none';
  if (addFundsBtn) addFundsBtn.style.display = hasKey ? '' : 'none';
  if (hasKey) {
    refreshOverviewAccount();
    updateStatCards();
    preloadAllTables();
    loadBillingBalance();
  }
}

// Preload groups, wallets, usage keys, and actions (for default group) when dashboard is shown
async function preloadAllTables() {
  const apiKey = getApiKey();
  if (!apiKey) return;
  try {
    await loadGroups();
    await loadWallets();
    await loadUsageKeys();
    await loadActions();
  } catch (_) { /* ignore */ }
}

function showStatus(elId, message, type = 'info') {
  const el = document.getElementById(elId);
  if (!el) return;
  el.textContent = message;
  el.className = 'status ' + type;
  el.style.display = 'block';
}

function hideStatus(elId) {
  const el = document.getElementById(elId);
  if (el) el.style.display = 'none';
}

async function getClient() {
  const baseUrl = getBaseUrl();
  const { createClient } = await import('../../core_sdk.js');
  return createClient(baseUrl);
}

function updateStatCards() {
  const elUsageKeys = document.getElementById('stat-usage-keys');
  const elGroups = document.getElementById('stat-groups');
  const elWallets = document.getElementById('stat-wallets');
  const elActions = document.getElementById('stat-actions');
  if (elUsageKeys) elUsageKeys.textContent = (typeof window._statUsageKeys === 'number') ? window._statUsageKeys : getUsageKeysStore().length;
  if (elGroups) elGroups.textContent = (typeof window._statGroups === 'number') ? window._statGroups : '—';
  if (elWallets) elWallets.textContent = (typeof window._statWallets === 'number') ? window._statWallets : '—';
  if (elActions) elActions.textContent = (typeof window._statActions === 'number') ? window._statActions : '—';
}

// ----- Theme (sun = switch to light, moon = switch to dark) -----
const THEME_ICON_SUN = '<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41"/></svg>';
const THEME_ICON_MOON = '<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"/></svg>';

function getTheme() {
  return sessionStorage.getItem(STORAGE_KEY_THEME) || 'light';
}

function setTheme(theme) {
  sessionStorage.setItem(STORAGE_KEY_THEME, theme);
  document.documentElement.setAttribute('data-theme', theme === 'dark' ? 'dark' : 'light');
  const btn = document.getElementById('theme-toggle');
  if (btn) {
    const iconEl = btn.querySelector('.theme-icon');
    if (iconEl) {
      iconEl.innerHTML = theme === 'dark' ? THEME_ICON_SUN : THEME_ICON_MOON;
    }
    btn.setAttribute('aria-label', theme === 'dark' ? 'Switch to light mode' : 'Switch to dark mode');
  }
}

// ----- Login -----
function initLogin() {
  const apiKeyInput = document.getElementById('login-api-key');
  if (getApiKey()) apiKeyInput.value = '';

  const tabExisting = document.getElementById('login-tab-existing');
  const tabNew = document.getElementById('login-tab-new');
  const panelExisting = document.getElementById('login-panel-existing');
  const panelNew = document.getElementById('login-panel-new');
  function switchLoginTab(toExisting) {
    const isExisting = toExisting === true;
    tabExisting.classList.toggle('is-active', isExisting);
    tabNew.classList.toggle('is-active', !isExisting);
    tabExisting.setAttribute('aria-selected', isExisting ? 'true' : 'false');
    tabNew.setAttribute('aria-selected', !isExisting ? 'true' : 'false');
    panelExisting.classList.toggle('is-active', isExisting);
    panelNew.classList.toggle('is-active', !isExisting);
    if (panelExisting) panelExisting.hidden = !isExisting;
    if (panelNew) panelNew.hidden = isExisting;
  }
  tabExisting?.addEventListener('click', () => switchLoginTab(true));
  tabNew?.addEventListener('click', () => switchLoginTab(false));

  document.getElementById('btn-login').addEventListener('click', async () => {
    const key = (apiKeyInput.value || '').trim();
    if (!key) {
      showStatus('login-status', 'Enter an API key.', 'error');
      return;
    }
    hideStatus('login-status');
    const btn = document.getElementById('btn-login');
    btn.disabled = true;
    try {
      const client = await getClient();
      const exists = await client.accountExists(key);
      if (exists) {
        setApiKey(key);
        showStatus('login-status', 'Logged in. You can now use the dashboard.', 'success');
      } else {
        showStatus('login-status', 'Account not found or not mutable for this API key.', 'error');
      }
    } catch (e) {
      showStatus('login-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
    } finally {
      btn.disabled = false;
    }
  });

  document.getElementById('btn-create-account').addEventListener('click', async () => {
    const name = document.getElementById('new-account-name').value.trim();
    const desc = document.getElementById('new-account-desc').value.trim();
    const email = document.getElementById('new-account-email').value.trim();
    hideStatus('login-status');
    if (!email) {
      showStatus('login-status', 'Enter an email address.', 'error');
      return;
    }
    if (!name) {
      showStatus('login-status', 'Enter an account name.', 'error');
      return;
    }
    const btn = document.getElementById('btn-create-account');
    btn.disabled = true;
    try {
      showActionProgress(
        'Creating account',
        'Creating a new Lit Express account and returning an API key.'
      );
      const client = await getClient();
      const res = await client.newAccount({ accountName: name, accountDescription: desc, email: email || undefined });
      setApiKey(res.api_key);
      showNewAccountBanner(res.api_key);
      document.getElementById('new-account-name').value = '';
      document.getElementById('new-account-desc').value = '';
    } catch (e) {
      showStatus('login-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
    } finally {
      closeActionProgress();
      btn.disabled = false;
    }
  });
}

// ----- Overview -----
function maskApiKey(key) {
  if (!key || key.length < 12) return '••••••••';
  return key.slice(0, 6) + '••••••••' + key.slice(-4);
}

/** First 4 and last 4 characters for display (e.g. "abcd...wxyz"). */
function keyPreview(key) {
  if (!key || key.length < 9) return '••••••••';
  return key.slice(0, 4) + '…' + key.slice(-4);
}

function refreshOverviewAccount() {
  // Overview no longer displays API key or status; kept for any future use.
}

// ----- Billing -----
async function loadBillingBalance() {
  const apiKey = getApiKey();
  if (!apiKey) return;
  const el = document.getElementById('billing-balance-display');
  if (!el) return;
  try {
    const client = await getClient();
    const data = await client.getBillingBalance(apiKey);
    el.textContent = data.balance_display || '';
  } catch (_) {
    el.textContent = '';
  }
}

// Stripe card element singleton
let _stripe = null;
let _stripeCard = null;

async function openAddFundsModal() {
  const overlay = document.getElementById('billing-modal-overlay');
  if (!overlay) return;
  overlay.classList.add('is-open');
  overlay.setAttribute('aria-hidden', 'false');

  const statusEl = document.getElementById('billing-modal-status');
  if (statusEl) { statusEl.style.display = 'none'; }

  // Reset pay button to enabled state each time the modal is opened so that
  // a previous Stripe init failure does not permanently disable it.
  const payBtn = document.getElementById('billing-pay-btn');
  if (payBtn) payBtn.disabled = true; // disabled until Stripe is ready

  // Initialise Stripe.js if not already done
  if (!_stripe) {
    try {
      const client = await getClient();
      const cfg = await client.getStripeConfig();
      _stripe = Stripe(cfg.publishable_key); // eslint-disable-line no-undef
      const elements = _stripe.elements();
      _stripeCard = elements.create('card');
      _stripeCard.mount('#stripe-card-element');
    } catch (e) {
      if (statusEl) {
        statusEl.textContent = 'Billing not available: ' + (e && e.message ? e.message : String(e));
        statusEl.className = 'status error';
        statusEl.style.display = 'block';
      }
      // payBtn stays disabled (already set above)
      return;
    }
  }

  // Stripe is ready – allow the user to pay
  if (payBtn) payBtn.disabled = false;
}

function closeBillingModal() {
  const overlay = document.getElementById('billing-modal-overlay');
  if (overlay) {
    overlay.classList.remove('is-open');
    overlay.setAttribute('aria-hidden', 'true');
  }
}

function initBilling() {
  const addFundsBtn = document.getElementById('btn-add-funds');
  const closeBtn = document.getElementById('billing-modal-close-btn');
  const cancelBtn = document.getElementById('billing-cancel-btn');
  const payBtn = document.getElementById('billing-pay-btn');

  if (addFundsBtn) addFundsBtn.addEventListener('click', openAddFundsModal);
  if (closeBtn) closeBtn.addEventListener('click', closeBillingModal);
  if (cancelBtn) cancelBtn.addEventListener('click', closeBillingModal);

  if (payBtn) {
    payBtn.addEventListener('click', async () => {
      const apiKey = getApiKey();
      if (!apiKey || !_stripe || !_stripeCard) return;

      const amountCents = parseInt(document.getElementById('billing-amount').value, 10);
      const statusEl = document.getElementById('billing-modal-status');

      payBtn.disabled = true;
      if (statusEl) { statusEl.style.display = 'none'; }

      try {
        const client = await getClient();
        const intent = await client.createPaymentIntent(apiKey, amountCents);

        const result = await _stripe.confirmCardPayment(intent.client_secret, {
          payment_method: { card: _stripeCard },
        });

        if (result.error) {
          throw new Error(result.error.message);
        }

        await client.confirmPayment(apiKey, intent.payment_intent_id);
        closeBillingModal();
        await loadBillingBalance();
      } catch (e) {
        if (statusEl) {
          statusEl.textContent = 'Payment failed: ' + (e && e.message ? e.message : String(e));
          statusEl.className = 'status error';
          statusEl.style.display = 'block';
        }
      } finally {
        payBtn.disabled = false;
      }
    });
  }
}

function showNewAccountBanner(apiKey) {
  const banner = document.getElementById('new-account-banner');
  const keyEl = document.getElementById('new-account-key-text');
  const copyBtn = document.getElementById('new-account-copy-btn');
  const dismissBtn = document.getElementById('new-account-dismiss-btn');
  if (!banner || !keyEl || !copyBtn || !dismissBtn) return;
  keyEl.textContent = apiKey;
  banner.style.display = '';
  copyBtn.textContent = 'Copy';
  copyBtn.onclick = async () => {
    try {
      await navigator.clipboard.writeText(apiKey);
    } catch (_) {
      const ta = document.createElement('textarea');
      ta.value = apiKey;
      ta.style.cssText = 'position:fixed;opacity:0';
      document.body.appendChild(ta);
      ta.select();
      document.execCommand('copy');
      document.body.removeChild(ta);
    }
    copyBtn.textContent = 'Copied!';
    setTimeout(() => { copyBtn.textContent = 'Copy'; }, 1500);
  };
  dismissBtn.onclick = () => { banner.style.display = 'none'; };
}

function initOverview() {
  refreshOverviewAccount();
  renderUsageKeysTable();
  updateStatCards();
  document.getElementById('btn-load-usage-keys')?.addEventListener('click', () => loadUsageKeys());
  document.getElementById('btn-add-usage-key')?.addEventListener('click', () => openUsageKeyModal());
}

// ----- Icons (pencil, trash, copy) -----
const ICON_PENCIL = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M3 17.25V21h3.75L17.81 9.94l-3.75-3.75L3 17.25zM20.71 7.04c.39-.39.39-1.02 0-1.41l-2.34-2.34c-.39-.39-1.02-.39-1.41 0l-1.83 1.83 3.75 3.75 1.83-1.83z"/></svg>';
const ICON_TRASH = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M6 19c0 1.1.9 2 2 2h8c1.1 0 2-.9 2-2V7H6v12zM19 4h-3.5l-1-1h-5l-1 1H5v2h14V4z"/></svg>';
const ICON_COPY = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M16 1H4c-1.1 0-2 .9-2 2v14h2V3h12V1zm3 4H8c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h11c1.1 0 2-.9 2-2V7c0-1.1-.9-2-2-2zm0 16H8V7h11v14z"/></svg>';

// ----- Modal (Add / Edit) -----
function openModal(title, bodyHTML, footerHTML) {
  const overlay = document.getElementById('modal-overlay');
  const titleEl = document.getElementById('modal-title');
  const body = document.getElementById('modal-body');
  const footer = document.getElementById('modal-footer');
  if (!overlay || !titleEl || !body || !footer) return;
  titleEl.textContent = title;
  body.innerHTML = bodyHTML;
  footer.innerHTML = footerHTML;
  overlay.classList.add('is-open');
  overlay.setAttribute('aria-hidden', 'false');
  const firstInput = body.querySelector('input, select, textarea');
  if (firstInput) firstInput.focus();
}

function closeModal() {
  const overlay = document.getElementById('modal-overlay');
  if (overlay) {
    overlay.classList.remove('is-open');
    overlay.setAttribute('aria-hidden', 'true');
  }
}

function initModalClose() {
  document.getElementById('modal-close-btn')?.addEventListener('click', closeModal);
  document.getElementById('modal-overlay')?.addEventListener('click', (e) => {
    if (e.target.id === 'modal-overlay') closeModal();
  });
}

// ----- Confirm delete -----
let _confirmResolve = null;

function confirmDelete(message) {
  return new Promise((resolve) => {
    _confirmResolve = resolve;
    const overlay = document.getElementById('confirm-overlay');
    const msgEl = document.getElementById('confirm-message');
    if (msgEl) msgEl.textContent = message || 'Are you sure you want to delete this item?';
    if (overlay) {
      overlay.classList.add('is-open');
      overlay.setAttribute('aria-hidden', 'false');
    }
  });
}

function closeConfirm(confirmed) {
  const overlay = document.getElementById('confirm-overlay');
  if (overlay) {
    overlay.classList.remove('is-open');
    overlay.setAttribute('aria-hidden', 'true');
  }
  if (_confirmResolve) {
    _confirmResolve(!!confirmed);
    _confirmResolve = null;
  }
}

function initConfirmClose() {
  document.getElementById('confirm-cancel-btn')?.addEventListener('click', () => closeConfirm(false));
  document.getElementById('confirm-close-btn')?.addEventListener('click', () => closeConfirm(false));
  document.getElementById('confirm-delete-btn')?.addEventListener('click', () => closeConfirm(true));
  document.getElementById('confirm-overlay')?.addEventListener('click', (e) => {
    if (e.target.id === 'confirm-overlay') closeConfirm(false);
  });
}

// ----- Action progress modal (non-dismissible) -----
let _actionProgressPreviousFocus = null;

function showActionProgress(title, description) {
  const overlay = document.getElementById('action-overlay');
  const titleEl = document.getElementById('action-title');
  const descEl = document.getElementById('action-desc');
  if (!overlay || !titleEl || !descEl) return;
  titleEl.textContent = title || 'Working…';
  descEl.textContent = description || '';
  _actionProgressPreviousFocus = document.activeElement;
  overlay.classList.add('is-open');
  overlay.setAttribute('aria-hidden', 'false');
  const dialog = overlay.querySelector('[role="dialog"]');
  if (dialog) dialog.focus();
}

function closeActionProgress() {
  const overlay = document.getElementById('action-overlay');
  if (!overlay) return;
  overlay.classList.remove('is-open');
  overlay.setAttribute('aria-hidden', 'true');
  if (_actionProgressPreviousFocus && typeof _actionProgressPreviousFocus.focus === 'function') {
    try { _actionProgressPreviousFocus.focus(); } catch (_) { /* element may have been removed */ }
  }
  _actionProgressPreviousFocus = null;
}

// ----- Shared list render (overview lists stay as list) -----
function renderMetadataList(listElId, emptyElId, items) {
  const list = document.getElementById(listElId);
  const empty = emptyElId ? document.getElementById(emptyElId) : null;
  if (!list) return;
  list.innerHTML = '';
  if (!items || items.length === 0) {
    if (empty) empty.style.display = 'block';
    return;
  }
  if (empty) empty.style.display = 'none';
  items.forEach((item) => {
    const li = document.createElement('li');
    li.className = 'list-item';
    li.innerHTML = '<span><strong>' + escapeHtml(item.name || '') + '</strong> (ID: ' + escapeHtml(String(item.id)) + ')</span><span class="mono">' + escapeHtml(item.description || '') + '</span>';
    list.appendChild(li);
  });
}

function escapeHtml(s) {
  if (s == null) return '';
  const div = document.createElement('div');
  div.textContent = s;
  return div.innerHTML;
}

// ----- Table renderers (with edit/delete at end of row) -----
function renderGroupsTable(items) {
  const tbody = document.getElementById('groups-tbody');
  const empty = document.getElementById('groups-empty');
  if (!tbody) return;
  tbody.innerHTML = '';
  if (!items || items.length === 0) {
    if (empty) empty.style.display = 'block';
    return;
  }
  if (empty) empty.style.display = 'none';
  items.forEach((item) => {
    const tr = document.createElement('tr');
    tr.innerHTML =
      '<td><strong>' + escapeHtml(item.name || '') + '</strong></td>' +
      '<td class="mono">' + escapeHtml(item.description || '') + '</td>' +
      '<td class="cell-actions"></td>';
    const actionsCell = tr.querySelector('.cell-actions');
    const editBtn = document.createElement('button');
    editBtn.type = 'button';
    editBtn.className = 'btn-icon';
    editBtn.title = 'Edit';
    editBtn.innerHTML = ICON_PENCIL;
    editBtn.addEventListener('click', () => openEditGroupModal(item));
    actionsCell.appendChild(editBtn);
    const delBtn = document.createElement('button');
    delBtn.type = 'button';
    delBtn.className = 'btn-icon btn-icon-danger';
    delBtn.title = 'Delete';
    delBtn.innerHTML = ICON_TRASH;
    delBtn.addEventListener('click', () => confirmAndRemoveGroup(item));
    actionsCell.appendChild(delBtn);
    tbody.appendChild(tr);
  });
}

async function confirmAndRemoveGroup(item) {
  const label = item.name || item.id || '—';
  const msg = 'Delete group "' + label + '"? This cannot be undone.';
  const confirmed = await confirmDelete(msg);
  if (!confirmed) return;
  const apiKey = getApiKey();
  if (!apiKey) return;
  hideStatus('groups-status');
  try {
    showActionProgress('Deleting group', `Deleting group "${escapeHtml(label)}".`);
    const client = await getClient();
    await client.removeGroup({ apiKey, groupId: item.id });
    window._groups = (window._groups || []).filter((g) => g.id !== item.id);
    renderGroupsTable(window._groups);
    updateStatCards();
    showStatus('groups-status', 'Group deleted.', 'success');
  } catch (e) {
    showStatus('groups-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
  } finally {
    closeActionProgress();
  }
}

function renderActionsTable(items) {
  const tbody = document.getElementById('actions-tbody');
  const empty = document.getElementById('actions-empty');
  if (!tbody) return;
  tbody.innerHTML = '';
  if (!items || items.length === 0) {
    if (empty) empty.style.display = 'block';
    return;
  }
  if (empty) empty.style.display = 'none';
  items.forEach((item) => {
    const tr = document.createElement('tr');
    const cid = item.ipfs_cid || item.cid || String(item.id ?? '');
    tr.innerHTML =
      '<td><strong>' + escapeHtml(item.name || '') + '</strong></td>' +
      '<td class="mono">' + escapeHtml(cid) + '</td>' +
      '<td class="mono">' + escapeHtml(item.description || '') + '</td>' +
      '<td class="cell-actions"></td>';
    const actionsCell = tr.querySelector('.cell-actions');
    const editBtn = document.createElement('button');
    editBtn.type = 'button';
    editBtn.className = 'btn-icon';
    editBtn.title = 'Edit';
    editBtn.innerHTML = ICON_PENCIL;
    editBtn.addEventListener('click', () => openEditActionModal(item));
    actionsCell.appendChild(editBtn);
    const delBtn = document.createElement('button');
    delBtn.type = 'button';
    delBtn.className = 'btn-icon btn-icon-danger';
    delBtn.title = 'Delete';
    delBtn.innerHTML = ICON_TRASH;
    delBtn.addEventListener('click', () => confirmAndRemoveAction(item));
    actionsCell.appendChild(delBtn);
    tbody.appendChild(tr);
  });
}

function renderWalletsTable(items) {
  const tbody = document.getElementById('wallets-tbody');
  const empty = document.getElementById('wallets-empty');
  if (!tbody) return;
  tbody.innerHTML = '';
  if (!items || items.length === 0) {
    if (empty) empty.style.display = 'block';
    return;
  }
  if (empty) empty.style.display = 'none';
  items.forEach((item) => {
    const address = item.wallet_address ?? item.address ?? item.name ?? '';
    const description = item.description ?? '';
    const tr = document.createElement('tr');
    tr.innerHTML =
      '<td class="mono">' + escapeHtml(description) + '</td>' +
      '<td class="mono cell-address"></td>';
    const addressCell = tr.querySelector('.cell-address');
    const addressCopyBtn = document.createElement('button');
    addressCopyBtn.type = 'button';
    addressCopyBtn.className = 'btn-copy-key';
    addressCopyBtn.textContent = address;
    addressCopyBtn.title = 'Copy full address';
    addressCopyBtn.addEventListener('click', async () => {
      try {
        await navigator.clipboard.writeText(address);
        const orig = addressCopyBtn.textContent;
        addressCopyBtn.textContent = 'Copied!';
        addressCopyBtn.title = 'Copied!';
        setTimeout(() => { addressCopyBtn.textContent = orig; addressCopyBtn.title = 'Copy full address'; }, 1500);
      } catch (_) {
        addressCopyBtn.title = 'Copy failed';
      }
    });
    addressCell.appendChild(addressCopyBtn);
    tbody.appendChild(tr);
  });
}

// Store for usage API keys (loaded from listApiKeys or pushed when adding)
function getUsageKeysStore() {
  if (!window._usageKeys) window._usageKeys = [];
  return window._usageKeys;
}

function getGroupsStore() {
  if (!window._groups) window._groups = [];
  return window._groups;
}

function getWalletsStore() {
  if (!window._wallets) window._wallets = [];
  return window._wallets;
}

function getActionsStore() {
  if (!window._actions) window._actions = [];
  return window._actions;
}

function renderUsageKeysTable() {
  const items = getUsageKeysStore();
  const tbody = document.getElementById('usage-keys-tbody');
  const empty = document.getElementById('usage-keys-empty');
  if (!tbody) return;
  tbody.innerHTML = '';
  if (!items || items.length === 0) {
    if (empty) empty.style.display = 'block';
    return;
  }
  if (empty) empty.style.display = 'none';
  items.forEach((item) => {
    const expiration = (() => {
      const ts = Number(item.expiration);
      if (!ts) return '—';
      return new Date(ts * 1000).toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
    })();
    const balance = item.balance != null ? String(item.balance) : '—';
    const tr = document.createElement('tr');
    tr.innerHTML =
      '<td>' + escapeHtml(item.name || '') + '</td>' +
      '<td class="mono">' + escapeHtml(item.description || '') + '</td>' +
      '<td class="mono" style="font-size:0.82em;">' + escapeHtml(renderPermissionSummary(item)) + '</td>' +
      '<td class="mono">' + escapeHtml(expiration) + '</td>' +
      '<td class="mono">' + escapeHtml(balance) + '</td>' +
      '<td class="cell-actions"></td>';
    const actionsCell = tr.querySelector('.cell-actions');
    const fullKey = item.api_key || (!item.api_key_hash ? item.usage_api_key : null);
    const copyBtn = document.createElement('button');
    copyBtn.type = 'button';
    copyBtn.className = 'btn-icon';
    copyBtn.title = fullKey ? 'Copy usage API key' : 'Key only available when first created';
    copyBtn.innerHTML = ICON_COPY;
    if (!fullKey) copyBtn.disabled = true;
    copyBtn.addEventListener('click', async () => {
      if (!fullKey) return;
      try {
        await navigator.clipboard.writeText(fullKey);
      } catch (_) {
        const ta = document.createElement('textarea');
        ta.value = fullKey;
        ta.style.cssText = 'position:fixed;opacity:0';
        document.body.appendChild(ta);
        ta.select();
        document.execCommand('copy');
        document.body.removeChild(ta);
      }
      const origTitle = copyBtn.title;
      copyBtn.title = 'Copied!';
      setTimeout(() => { copyBtn.title = origTitle; }, 1500);
    });
    actionsCell.appendChild(copyBtn);
    const canEdit = !!(item.usage_api_key ?? item.api_key);
    const editBtn = document.createElement('button');
    editBtn.type = 'button';
    editBtn.className = 'btn-icon';
    editBtn.title = canEdit ? 'Edit' : 'Edit requires key (add key in this session to edit later)';
    editBtn.innerHTML = ICON_PENCIL;
    if (!canEdit) editBtn.disabled = true;
    editBtn.addEventListener('click', () => canEdit && openUsageKeyModal(normalizeUsageKeyItem(item)));
    actionsCell.appendChild(editBtn);
    const delBtn = document.createElement('button');
    delBtn.type = 'button';
    delBtn.className = 'btn-icon btn-icon-danger';
    delBtn.title = (item.usage_api_key ?? item.api_key) ? 'Delete' : 'Delete requires key (add key in this session to remove later)';
    delBtn.innerHTML = ICON_TRASH;
    const canRemove = !!(item.usage_api_key ?? item.api_key);
    if (!canRemove) delBtn.disabled = true;
    delBtn.addEventListener('click', () => canRemove && confirmAndRemoveUsageKey(normalizeUsageKeyItem(item)));
    actionsCell.appendChild(delBtn);
    tbody.appendChild(tr);
  });
}

function normalizeUsageKeyItem(item) {
  return {
    id: item.id,
    usage_api_key: item.usage_api_key ?? item.api_key,
    name: item.name,
    description: item.description,
    expiration: item.expiration,
    balance: item.balance,
    can_create_groups: item.can_create_groups ?? false,
    can_delete_groups: item.can_delete_groups ?? false,
    can_create_pkps: item.can_create_pkps ?? false,
    can_manage_ipfs_ids_in_groups: item.can_manage_ipfs_ids_in_groups ?? [],
    can_add_pkp_to_groups: item.can_add_pkp_to_groups ?? [],
    can_remove_pkp_from_groups: item.can_remove_pkp_from_groups ?? [],
    can_execute_in_groups: item.can_execute_in_groups ?? [],
  };
}

function renderPermissionSummary(item) {
  const parts = [];
  if (item.can_create_groups) parts.push('create groups');
  if (item.can_delete_groups) parts.push('delete groups');
  if (item.can_create_pkps) parts.push('create PKPs');
  const fmtGroups = (ids) => (ids && ids.length > 0) ? (ids.includes(0) ? 'all' : ids.join(', ')) : null;
  const exec = fmtGroups(item.can_execute_in_groups);
  if (exec) parts.push('execute: ' + exec);
  const manage = fmtGroups(item.can_manage_ipfs_ids_in_groups);
  if (manage) parts.push('manage actions: ' + manage);
  const addPkp = fmtGroups(item.can_add_pkp_to_groups);
  if (addPkp) parts.push('add PKP: ' + addPkp);
  const removePkp = fmtGroups(item.can_remove_pkp_from_groups);
  if (removePkp) parts.push('remove PKP: ' + removePkp);
  return parts.length > 0 ? parts.join('; ') : 'none';
}

async function loadUsageKeys() {
  const apiKey = getApiKey();
  if (!apiKey) return [];
  hideStatus('overview-status-usage-keys');
  const btn = document.getElementById('btn-load-usage-keys');
  if (btn) btn.disabled = true;
  try {
    const client = await getClient();
    const items = await client.listApiKeys({ apiKey, pageNumber: '0', pageSize: LIST_PAGE_SIZE });
    window._usageKeys = items.map((it) => ({
      id: it.id,
      api_key_hash: it.api_key_hash,
      usage_api_key: it.api_key_hash,
      name: it.name ?? '',
      description: it.description ?? '',
      expiration: it.expiration,
      balance: it.balance,
      can_create_groups: it.can_create_groups ?? false,
      can_delete_groups: it.can_delete_groups ?? false,
      can_create_pkps: it.can_create_pkps ?? false,
      can_manage_ipfs_ids_in_groups: it.can_manage_ipfs_ids_in_groups ?? [],
      can_add_pkp_to_groups: it.can_add_pkp_to_groups ?? [],
      can_remove_pkp_from_groups: it.can_remove_pkp_from_groups ?? [],
      can_execute_in_groups: it.can_execute_in_groups ?? [],
    }));
    window._statUsageKeys = window._usageKeys.length;
    renderUsageKeysTable();
    updateStatCards();
    return window._usageKeys;
  } catch (e) {
    showStatus('overview-status-usage-keys', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
    return [];
  } finally {
    if (btn) btn.disabled = false;
  }
}

// ----- Load table data (used by preload and refresh buttons) -----

async function loadGroups() {
  const apiKey = getApiKey();
  if (!apiKey) return;
  hideStatus('groups-status');
  const btn = document.getElementById('btn-load-groups');
  if (btn) btn.disabled = true;
  try {
    const client = await getClient();
    const items = await client.listGroups({ apiKey, pageNumber: '0', pageSize: LIST_PAGE_SIZE });
    window._groups = items;
    renderGroupsTable(items);
    window._statGroups = items.length;
    updateStatCards();
    return items;
  } catch (e) {
    showStatus('groups-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
    return [];
  } finally {
    if (btn) btn.disabled = false;
  }
}

async function loadWallets() {
  const apiKey = getApiKey();
  if (!apiKey) return;
  hideStatus('wallets-status');
  const btn = document.getElementById('btn-load-wallets');
  if (btn) btn.disabled = true;
  try {
    const client = await getClient();
    const items = await client.listWallets({ apiKey, pageNumber: '0', pageSize: LIST_PAGE_SIZE });
    window._wallets = items;
    renderWalletsTable(items);
    window._statWallets = items.length;
    updateStatCards();
    return items;
  } catch (e) {
    showStatus('wallets-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
    return [];
  } finally {
    if (btn) btn.disabled = false;
  }
}

async function loadActions() {
  const apiKey = getApiKey();
  if (!apiKey) return;
  hideStatus('actions-status');
  const btn = document.getElementById('btn-load-actions');
  if (btn) btn.disabled = true;
  try {
    const client = await getClient();
    const items = await client.listActions({ apiKey, pageNumber: '0', pageSize: LIST_PAGE_SIZE });
    window._actions = items;
    renderActionsTable(items);
    window._statActions = items.length;
    updateStatCards();
    return items;
  } catch (e) {
    showStatus('actions-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
    return [];
  } finally {
    if (btn) btn.disabled = false;
  }
}

// ----- Wallets -----
function initWallets() {
  document.getElementById('btn-load-wallets')?.addEventListener('click', () => loadWallets());
  document.getElementById('btn-add-wallet')?.addEventListener('click', () => openAddWalletModal());
}

// ----- Group modals -----
function selectAllInMultiSelect(id) {
  const wrap = document.getElementById(id);
  if (!wrap) return;
  const allCb = wrap.querySelector('input[value="0"]') || wrap.querySelector('input[value="0x0000000000000000000000000000000000000000"]');
  wrap.querySelectorAll('input[type="checkbox"]').forEach((cb) => { cb.checked = cb === allCb; });
  updateMultiSelectSummary(id);
}

function openGroupModal(item = null) {
  const isEdit = item != null;
  const nameId = isEdit ? 'modal-edit-group-name' : 'modal-group-name';
  const descId = isEdit ? 'modal-edit-group-desc' : 'modal-group-desc';

  const overlay = document.getElementById('modal-overlay');
  const titleEl = document.getElementById('modal-title');
  const modalBody = document.getElementById('modal-body');
  const modalFooter = document.getElementById('modal-footer');
  if (!overlay || !titleEl || !modalBody || !modalFooter) return;

  titleEl.textContent = isEdit ? 'Edit group' : 'Add group';
  modalBody.innerHTML =
    '<div class="form-group"><label for="' + nameId + '">Name</label><input type="text" id="' + nameId + '" class="input" placeholder="My" value="' + escapeHtml(item?.name ?? '') + '"></div>' +
    '<div class="form-group"><label for="' + descId + '">Description</label><input type="text" id="' + descId + '" class="input" placeholder="Optional" value="' + escapeHtml(item?.description ?? '') + '"></div>' +
    '<div class="form-group"><label>PKP IDs permitted</label>' + buildWalletMultiSelect('modal-group-pkp-ids', false) + '</div>' +
    '<div class="form-group"><label>CID hashes permitted</label>' + buildActionsMultiSelect('modal-group-cid-hashes', false) + '</div>';
  modalFooter.innerHTML =
    '<button type="button" class="btn btn-outline" id="modal-all-opts-btn" style="margin-right:auto;">All Options</button>' +
    '<button type="button" class="btn btn-outline" id="modal-cancel-btn">Cancel</button>' +
    '<button type="button" class="btn btn-primary" id="modal-save-btn">' + (isEdit ? 'Save' : 'Add') + '</button>';
  overlay.classList.add('is-open');
  overlay.setAttribute('aria-hidden', 'false');
  const firstInput = modalBody.querySelector('input, select, textarea');
  if (firstInput) firstInput.focus();

  const allOptsBtn = modalFooter.querySelector('#modal-all-opts-btn');
  const cancelBtn = modalFooter.querySelector('#modal-cancel-btn');
  const saveBtn = modalFooter.querySelector('#modal-save-btn');

  attachGroupMultiSelectLogic('modal-group-pkp-ids');
  attachGroupMultiSelectLogic('modal-group-cid-hashes');
  if (allOptsBtn) allOptsBtn.addEventListener('click', () => {
    selectAllInMultiSelect('modal-group-pkp-ids');
    selectAllInMultiSelect('modal-group-cid-hashes');
  });
  if (cancelBtn) cancelBtn.addEventListener('click', closeModal);
  if (saveBtn) saveBtn.addEventListener('click', async () => {
    const name = document.getElementById(nameId).value.trim() || 'Group';
    const desc = document.getElementById(descId).value.trim();
    const pkpIdsPermitted = getSelectedStringValues('modal-group-pkp-ids');
    const cidHashesPermitted = getSelectedStringValues('modal-group-cid-hashes');
    const apiKey = getApiKey();
    if (!apiKey) {
      showStatus('groups-status', 'Log in first.', 'error');
      return;
    }
    closeModal();
    hideStatus('groups-status');
    try {
      const client = await getClient();
      if (isEdit) {
        const id = String(Number(item.id));
        showActionProgress('Updating group', `Updating group "${name}".`);
        await client.updateGroup({ apiKey, groupId: id, name, description: desc, pkpIdsPermitted, cidHashesPermitted });
      } else {
        showActionProgress('Creating group', `Creating group "${name}".`);
        await client.addGroup({ apiKey, groupName: name, groupDescription: desc, pkpIdsPermitted, cidHashesPermitted });
      }
      await loadGroups();
      showStatus('groups-status', isEdit ? 'Group updated.' : 'Group created.', 'success');
    } catch (e) {
      showStatus('groups-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
    } finally {
      closeActionProgress();
    }
  });
}

function openAddGroupModal() { openGroupModal(); }
function openEditGroupModal(item) { openGroupModal(item); }

// ----- Action modals -----
function openAddActionModal() {
  const body =
    '<div class="form-group"><label for="modal-action-cid">IPFS CID</label><input type="text" id="modal-action-cid" class="input" placeholder="Qm... or bafy..."></div>' +
    '<div class="form-group"><label for="modal-action-name">Name (optional)</label><input type="text" id="modal-action-name" class="input" placeholder="Action name"></div>' +
    '<div class="form-group"><label for="modal-action-desc">Description (optional)</label><input type="text" id="modal-action-desc" class="input" placeholder="Optional"></div>';
  const footer =
    '<button type="button" class="btn btn-outline" id="modal-cancel-btn">Cancel</button>' +
    '<button type="button" class="btn btn-primary" id="modal-add-btn">Add</button>';
  openModal('Add action', body, footer);
  document.getElementById('modal-cancel-btn').addEventListener('click', closeModal);
  document.getElementById('modal-add-btn').addEventListener('click', async () => {
    const cid = document.getElementById('modal-action-cid').value.trim();
    const name = document.getElementById('modal-action-name').value.trim() || undefined;
    const desc = document.getElementById('modal-action-desc').value.trim() || undefined;
    const apiKey = getApiKey();
    if (!apiKey || !cid) {
      showStatus('actions-status', 'Fill in the IPFS CID.', 'error');
      return;
    }
    closeModal();
    hideStatus('actions-status');
    try {
      showActionProgress('Adding action', `Adding action CID "${cid}".`);
      const client = await getClient();
      await client.addAction({ apiKey, actionIpfsCid: cid, name: name || '', description: desc || '' });
      await loadActions();
      showStatus('actions-status', 'Action added.', 'success');
    } catch (e) {
      showStatus('actions-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
    } finally {
      closeActionProgress();
    }
  });
}

function openEditActionModal(item) {
  const cid = item.ipfs_cid || item.cid || String(item.id ?? '');
  const body =
    '<div class="form-group"><label>Hashed CID</label><div class="mono">' + escapeHtml(cid) + '</div></div>' +
    '<div class="form-group"><label for="modal-edit-action-name">Name</label><input type="text" id="modal-edit-action-name" class="input" value="' + escapeHtml(item.name || '') + '"></div>' +
    '<div class="form-group"><label for="modal-edit-action-desc">Description</label><input type="text" id="modal-edit-action-desc" class="input" value="' + escapeHtml(item.description || '') + '"></div>';
  const footer =
    '<button type="button" class="btn btn-outline" id="modal-cancel-btn">Cancel</button>' +
    '<button type="button" class="btn btn-primary" id="modal-save-btn">Save</button>';
  openModal('Edit action', body, footer);
  document.getElementById('modal-cancel-btn').addEventListener('click', closeModal);
  document.getElementById('modal-save-btn').addEventListener('click', async () => {
    const name = document.getElementById('modal-edit-action-name').value.trim();
    const desc = document.getElementById('modal-edit-action-desc').value.trim();
    const apiKey = getApiKey();
    if (!apiKey || !cid || !name) {
      showStatus('actions-status', 'Fill Name.', 'error');
      return;
    }
    closeModal();
    hideStatus('actions-status');
    try {
      showActionProgress('Updating action', `Updating action metadata for CID "${cid}".`);
      const client = await getClient();
      await client.updateActionMetadata({ apiKey, hashedCid: cid, name, description: desc });
      await loadActions();
      showStatus('actions-status', 'Action updated.', 'success');
    } catch (e) {
      showStatus('actions-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
    } finally {
      closeActionProgress();
    }
  });
}

async function confirmAndRemoveAction(item) {
  const cid = item.ipfs_cid || item.cid || String(item.id ?? '');
  const name = item.name || cid;
  const msg = 'Delete action "' + escapeHtml(name) + '"? This cannot be undone.';
  const confirmed = await confirmDelete(msg);
  if (!confirmed) return;
  const apiKey = getApiKey();
  if (!apiKey) return;
  hideStatus('actions-status');
  try {
    showActionProgress('Deleting action', `Deleting action CID "${cid}".`);
    const client = await getClient();
    await client.deleteAction({ apiKey, hashedCid: cid });
    await loadActions();
    showStatus('actions-status', 'Action deleted.', 'success');
  } catch (e) {
    showStatus('actions-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
  } finally {
    closeActionProgress();
  }
}

// ----- Usage API key Add modal and delete -----
function buildMultiSelect(id, items, getValue, getLabel, placeholder, disabled) {
  const d = disabled ? ' disabled' : '';
  let opts = items.map((item) =>
    '<label class="ms-option"><input type="checkbox" value="' + escapeHtml(String(getValue(item))) + '"' + d + '><span>' + escapeHtml(getLabel(item)) + '</span></label>'
  ).join('');
  if (!opts) opts = '<div class="ms-option" style="opacity:0.6;cursor:default;">No items available</div>';
  return (
    '<div class="ms-wrap" id="' + id + '" data-placeholder="' + escapeHtml(placeholder) + '">' +
      '<button type="button" class="ms-trigger"' + (disabled ? ' disabled' : '') + '>' +
        '<span class="ms-summary">' + escapeHtml(placeholder) + '</span>' +
        '<span class="ms-arrow" aria-hidden="true"></span>' +
      '</button>' +
      '<div class="ms-dropdown">' + opts + '</div>' +
    '</div>'
  );
}

function buildGroupMultiSelect(id, disabled) {
  const items = [{ id: '0', name: 'All Groups' }, ...getGroupsStore()];
  return buildMultiSelect(id, items, (g) => g.id, (g) => g.name || String(g.id), 'Select groups\u2026', disabled);
}

function buildWalletMultiSelect(id, disabled) {
  const items = [{ wallet_address: '0x0000000000000000000000000000000000000000', name: 'All' }, ...getWalletsStore()];
  return buildMultiSelect(id, items, (w) => w.wallet_address, (w) => w.name || w.wallet_address, 'Select PKPs\u2026', disabled);
}

function buildActionsMultiSelect(id, disabled) {
  const items = [{ id: '0', name: 'All' }, ...getActionsStore()];
  return buildMultiSelect(id, items, (a) => a.id, (a) => a.name || a.id, 'Select actions\u2026', disabled);
}

function updateMultiSelectSummary(id) {
  const wrap = document.getElementById(id);
  if (!wrap) return;
  const summaryEl = wrap.querySelector('.ms-summary');
  const checked = [...wrap.querySelectorAll('input[type="checkbox"]:checked')];
  const placeholder = wrap.dataset.placeholder || 'Select\u2026';
  summaryEl.textContent = checked.length === 0
    ? placeholder
    : checked.map((c) => c.nextElementSibling.textContent).join(', ');
}

function attachGroupMultiSelectLogic(id) {
  const wrap = document.getElementById(id);
  if (!wrap) return;
  const trigger = wrap.querySelector('.ms-trigger');
  const allCb = wrap.querySelector('input[value="0"]');

  trigger.addEventListener('click', (e) => {
    e.preventDefault();
    document.querySelectorAll('.ms-wrap.is-open').forEach((w) => { if (w !== wrap) w.classList.remove('is-open'); });
    wrap.classList.toggle('is-open');
  });

  document.addEventListener('click', function msOutside(e) {
    if (!wrap.isConnected) { document.removeEventListener('click', msOutside); return; }
    if (!wrap.contains(e.target)) wrap.classList.remove('is-open');
  });

  wrap.querySelectorAll('input[type="checkbox"]').forEach((cb) => {
    cb.addEventListener('change', () => {
      if (cb === allCb && allCb.checked) {
        wrap.querySelectorAll('input[type="checkbox"]').forEach((c) => { if (c !== allCb) c.checked = false; });
      } else if (cb !== allCb && cb.checked && allCb && allCb.checked) {
        allCb.checked = false;
      }
      updateMultiSelectSummary(id);
    });
  });
}

function getSelectedGroupIds(id) {
  const wrap = document.getElementById(id);
  if (!wrap) return [];
  return [...wrap.querySelectorAll('input[type="checkbox"]:checked')].map((c) => Number(c.value));
}

function getSelectedStringValues(id) {
  const wrap = document.getElementById(id);
  if (!wrap) return [];
  return [...wrap.querySelectorAll('input[type="checkbox"]:checked')].map((c) => c.value);
}

function openUsageKeyModal(item = null) {
  const isEdit = item != null;
  const body =
    '<div class="form-group"><label for="modal-usage-name">Name (optional)</label><input type="text" id="modal-usage-name" class="input" placeholder="Optional" value="' + escapeHtml(item?.name ?? '') + '"></div>' +
    '<div class="form-group"><label for="modal-usage-desc">Description (optional)</label><input type="text" id="modal-usage-desc" class="input" placeholder="Optional" value="' + escapeHtml(item?.description ?? '') + '"></div>' +
    '<div class="form-group">' +
      '<label class="checkbox-label"><input type="checkbox" id="modal-usage-can-create-groups"' + (isEdit ? ' disabled' : '') + '> Can create groups</label>' +
      '<label class="checkbox-label"><input type="checkbox" id="modal-usage-can-delete-groups"' + (isEdit ? ' disabled' : '') + '> Can delete groups</label>' +
      '<label class="checkbox-label"><input type="checkbox" id="modal-usage-can-create-pkps"' + (isEdit ? ' disabled' : '') + '> Can create PKPs</label>' +
    '</div>' +
    '<div class="form-group"><label>Can execute in groups</label>' + buildGroupMultiSelect('modal-usage-execute-groups', false) + '</div>' +
    '<div class="form-group"><label>Can manage IPFS actions in groups</label>' + buildGroupMultiSelect('modal-usage-manage-ipfs-groups', false) + '</div>' +
    '<div class="form-group"><label>Can add PKP to groups</label>' + buildGroupMultiSelect('modal-usage-add-pkp-groups', false) + '</div>' +
    '<div class="form-group"><label>Can remove PKP from groups</label>' + buildGroupMultiSelect('modal-usage-remove-pkp-groups', false) + '</div>';
  const footer =
    (!isEdit ? '<button type="button" class="btn btn-outline" id="modal-all-options-btn" style="margin-right:auto;">All Options</button>' : '') +
    '<button type="button" class="btn btn-outline" id="modal-cancel-btn">Cancel</button>' +
    '<button type="button" class="btn btn-primary" id="modal-save-btn">' + (isEdit ? 'Save' : 'Add') + '</button>';
  openModal(isEdit ? 'Edit usage API key' : 'Add usage API key', body, footer);
  if (isEdit && item) {
    const setCb = (id, val) => { const el = document.getElementById(id); if (el) el.checked = !!val; };
    setCb('modal-usage-can-create-groups', item.can_create_groups);
    setCb('modal-usage-can-delete-groups', item.can_delete_groups);
    setCb('modal-usage-can-create-pkps', item.can_create_pkps);
    const preSelect = (msId, ids) => {
      const wrap = document.getElementById(msId);
      if (!wrap || !ids) return;
      const idSet = new Set(ids.map(String));
      wrap.querySelectorAll('input[type="checkbox"]').forEach((cb) => { if (idSet.has(cb.value)) cb.checked = true; });
      updateMultiSelectSummary(msId);
    };
    preSelect('modal-usage-execute-groups', item.can_execute_in_groups);
    preSelect('modal-usage-manage-ipfs-groups', item.can_manage_ipfs_ids_in_groups);
    preSelect('modal-usage-add-pkp-groups', item.can_add_pkp_to_groups);
    preSelect('modal-usage-remove-pkp-groups', item.can_remove_pkp_from_groups);
  }
  attachGroupMultiSelectLogic('modal-usage-execute-groups');
  attachGroupMultiSelectLogic('modal-usage-manage-ipfs-groups');
  attachGroupMultiSelectLogic('modal-usage-add-pkp-groups');
  attachGroupMultiSelectLogic('modal-usage-remove-pkp-groups');
  if (!isEdit) {
    document.getElementById('modal-all-options-btn').addEventListener('click', () => {
      document.getElementById('modal-usage-can-create-groups').checked = true;
      document.getElementById('modal-usage-can-delete-groups').checked = true;
      document.getElementById('modal-usage-can-create-pkps').checked = true;
      ['modal-usage-execute-groups', 'modal-usage-manage-ipfs-groups', 'modal-usage-add-pkp-groups', 'modal-usage-remove-pkp-groups'].forEach((id) => {
        const wrap = document.getElementById(id);
        if (!wrap) return;
        wrap.querySelectorAll('input[type="checkbox"]').forEach((cb) => { cb.checked = cb.value === '0'; });
        updateMultiSelectSummary(id);
      });
    });
  }
  document.getElementById('modal-cancel-btn').addEventListener('click', closeModal);
  document.getElementById('modal-save-btn').addEventListener('click', async () => {
    const name = document.getElementById('modal-usage-name').value.trim() || 'Usage Key';
    const description = document.getElementById('modal-usage-desc').value.trim() || '';
    const apiKey = getApiKey();
    if (!apiKey) {
      showStatus('overview-status-usage-keys', 'Log in first.', 'error');
      return;
    }
    closeModal();
    hideStatus('overview-status-usage-keys');
    if (isEdit) {
      const executeInGroups = getSelectedGroupIds('modal-usage-execute-groups');
      const manageIpfsIdsInGroups = getSelectedGroupIds('modal-usage-manage-ipfs-groups');
      const addPkpToGroups = getSelectedGroupIds('modal-usage-add-pkp-groups');
      const removePkpFromGroups = getSelectedGroupIds('modal-usage-remove-pkp-groups');
      try {
        showActionProgress('Updating usage API key', 'Saving changes to usage API key.');
        const client = await getClient();
        await client.updateUsageApiKey({
          apiKey,
          usageApiKey: item.usage_api_key,
          name,
          description,
          canCreateGroups: item.can_create_groups,
          canDeleteGroups: item.can_delete_groups,
          canCreatePkps: item.can_create_pkps,
          manageIpfsIdsInGroups,
          addPkpToGroups,
          removePkpFromGroups,
          executeInGroups,
        });
        const store = getUsageKeysStore();
        const idx = store.findIndex((k) => k.usage_api_key === item.usage_api_key);
        if (idx !== -1) {
          store[idx].name = name;
          store[idx].description = description;
          store[idx].can_manage_ipfs_ids_in_groups = manageIpfsIdsInGroups;
          store[idx].can_add_pkp_to_groups = addPkpToGroups;
          store[idx].can_remove_pkp_from_groups = removePkpFromGroups;
          store[idx].can_execute_in_groups = executeInGroups;
        }
        renderUsageKeysTable();
        showStatus('overview-status-usage-keys', 'Usage API key updated.', 'success');
      } catch (e) {
        showStatus('overview-status-usage-keys', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
      } finally {
        closeActionProgress();
      }
    } else {
      const canCreateGroups = document.getElementById('modal-usage-can-create-groups').checked;
      const canDeleteGroups = document.getElementById('modal-usage-can-delete-groups').checked;
      const canCreatePkps = document.getElementById('modal-usage-can-create-pkps').checked;
      const executeInGroups = getSelectedGroupIds('modal-usage-execute-groups');
      const manageIpfsIdsInGroups = getSelectedGroupIds('modal-usage-manage-ipfs-groups');
      const addPkpToGroups = getSelectedGroupIds('modal-usage-add-pkp-groups');
      const removePkpFromGroups = getSelectedGroupIds('modal-usage-remove-pkp-groups');
      try {
        showActionProgress('Adding usage API key', 'Creating a new usage API key for this account.');
        const client = await getClient();
        const res = await client.addUsageApiKey({
          apiKey, name, description,
          canCreateGroups, canDeleteGroups, canCreatePkps,
          manageIpfsIdsInGroups,
          addPkpToGroups,
          removePkpFromGroups,
          executeInGroups,
        });
        const usageKey = res && res.usage_api_key ? res.usage_api_key : '';
        if (usageKey) {
          getUsageKeysStore().push({
            id: usageKey.slice(0, 12),
            api_key: usageKey,
            usage_api_key: usageKey,
            name: name || '',
            description: description || '',
            expiration: '',
            balance: 0,
          });
          window._statUsageKeys = getUsageKeysStore().length;
          renderUsageKeysTable();
          updateStatCards();
        }
        showStatus('overview-status-usage-keys', 'Usage API key added. Copy and store your key now (shown once): ' + usageKey, 'success');
      } catch (e) {
        showStatus('overview-status-usage-keys', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
      } finally {
        closeActionProgress();
      }
    }
  });
}

async function confirmAndRemoveUsageKey(item) {
  const keyOrId = item.usage_api_key || item.api_key || item.id || '';
  const masked = keyOrId ? (keyOrId.length > 12 ? maskApiKey(keyOrId) : keyOrId) : '—';
  const msg = 'Remove usage API key "' + escapeHtml(masked) + '" from this account? This cannot be undone.';
  const confirmed = await confirmDelete(msg);
  if (!confirmed) return;
  const apiKey = getApiKey();
  if (!apiKey) return;
  hideStatus('overview-status-usage-keys');
  try {
    showActionProgress('Removing usage API key', `Removing usage API key "${masked}".`);
    const client = await getClient();
    await client.removeUsageApiKey({ apiKey, usageApiKey: item.usage_api_key });
    const store = getUsageKeysStore();
    const idx = store.findIndex((k) => k.usage_api_key === item.usage_api_key);
    if (idx !== -1) store.splice(idx, 1);
    renderUsageKeysTable();
    updateStatCards();
    showStatus('overview-status-usage-keys', 'Usage API key removed.', 'success');
  } catch (e) {
    showStatus('overview-status-usage-keys', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
  } finally {
    closeActionProgress();
  }
}

// ----- Wallet Add modal -----
function openAddWalletModal() {
  const body =
    '<p class="form-hint">Creates a new wallet and registers it for this account. The wallet address will be shown after creation.</p>';
  const footer =
    '<button type="button" class="btn btn-outline" id="modal-cancel-btn">Cancel</button>' +
    '<button type="button" class="btn btn-primary" id="modal-add-btn">Add</button>';
  openModal('Create wallet', body, footer);
  document.getElementById('modal-cancel-btn').addEventListener('click', closeModal);
  document.getElementById('modal-add-btn').addEventListener('click', async () => {
    const apiKey = getApiKey();
    if (!apiKey) return;
    closeModal();
    hideStatus('wallets-status');
    try {
      showActionProgress('Creating wallet', 'Creating and registering a new wallet for this account.');
      const client = await getClient();
      const res = await client.createWallet(apiKey);
      await loadWallets();
      showStatus('wallets-status', 'Wallet created: ' + (res.wallet_address || ''), 'success');
    } catch (e) {
      showStatus('wallets-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
    } finally {
      closeActionProgress();
    }
  });
}

// ----- Groups -----
function initGroups() {
  document.getElementById('btn-load-groups')?.addEventListener('click', () => loadGroups());
  document.getElementById('btn-add-group')?.addEventListener('click', () => openAddGroupModal());
}

// ----- IPFS Actions -----
function initActions() {
  document.getElementById('btn-load-actions')?.addEventListener('click', () => loadActions());
  document.getElementById('btn-add-action')?.addEventListener('click', () => openAddActionModal());
}

// ----- Sidebar scroll -----
const ACTION_RUNNER_ID = 'action-runner';
const MAIN_SECTION_IDS = ['overview', 'usage-keys', 'groups', 'actions', 'wallets'];

function setActionRunnerVisible(visible) {
  const actionRunnerSection = document.getElementById('section-' + ACTION_RUNNER_ID);
  if (actionRunnerSection) actionRunnerSection.style.display = visible ? '' : 'none';
  MAIN_SECTION_IDS.forEach((id) => {
    const el = document.getElementById('section-' + id);
    if (el) el.style.display = visible ? 'none' : '';
  });
}

function initSidebar() {
  document.querySelectorAll('.sidebar-link[data-scroll]').forEach((a) => {
    a.addEventListener('click', (e) => {
      e.preventDefault();
      const id = a.getAttribute('data-scroll');
      if (id === ACTION_RUNNER_ID) {
        setActionRunnerVisible(true);
        const el = document.getElementById('section-' + id);
        if (el) el.scrollIntoView({ behavior: 'smooth', block: 'start' });
      } else {
        setActionRunnerVisible(false);
        const el = document.getElementById('section-' + id);
        if (el) el.scrollIntoView({ behavior: 'smooth', block: 'start' });
      }
    });
  });
}

// ----- Header -----
function closeAccountDropdown() {
  const wrap = document.getElementById('account-dropdown');
  const trigger = document.getElementById('account-dropdown-trigger');
  const panel = document.getElementById('account-dropdown-panel');
  if (wrap) wrap.classList.remove('is-open');
  if (trigger) {
    trigger.setAttribute('aria-expanded', 'false');
  }
  if (panel) panel.setAttribute('aria-hidden', 'true');
}

function initHeader() {
  const themeToggle = document.getElementById('theme-toggle');
  if (themeToggle) {
    themeToggle.addEventListener('click', () => {
      const next = getTheme() === 'dark' ? 'light' : 'dark';
      setTheme(next);
    });
  }

  const dropdown = document.getElementById('account-dropdown');
  const trigger = document.getElementById('account-dropdown-trigger');
  const panel = document.getElementById('account-dropdown-panel');
  if (trigger && panel) {
    trigger.addEventListener('click', (e) => {
      e.stopPropagation();
      const isOpen = dropdown?.classList.toggle('is-open');
      trigger.setAttribute('aria-expanded', isOpen ? 'true' : 'false');
      panel.setAttribute('aria-hidden', isOpen ? 'false' : 'true');
    });
  }

  document.addEventListener('click', (e) => {
    if (dropdown && !dropdown.contains(e.target)) closeAccountDropdown();
  });

  const signoutBtn = document.getElementById('account-signout-btn');
  if (signoutBtn) {
    signoutBtn.addEventListener('click', (e) => {
      e.stopPropagation();
      closeAccountDropdown();
      setApiKey('');
    });
  }
}

// ----- Action Runner -----
let _codeJarEditor = null;
let _paramsJarEditor = null;

async function initActionRunner() {
  const codeEl = document.getElementById('action-runner-code');
  const paramsEl = document.getElementById('action-runner-params');
  const btn = document.getElementById('btn-execute-lit-action');
  const btnGetCid = document.getElementById('btn-get-lit-action-ipfs-cid');
  const outputEl = document.getElementById('action-runner-output');
  const statusEl = document.getElementById('action-runner-status');

  if (!btn || !outputEl) return;

  let getCode;
  let getParams;

  try {
    const { CodeJar } = await import('https://cdn.jsdelivr.net/npm/codejar@4.2.0/+esm');
    const highlight = (editor) => {
      if (!window.Prism) return;
      const lang = editor.classList.contains('language-json') ? 'json' : 'javascript';
      const grammar = Prism.languages[lang];
      if (grammar) editor.innerHTML = Prism.highlight(editor.textContent, grammar, lang);
    };
    _codeJarEditor = CodeJar(codeEl, highlight, { tab: '  ' });
    _paramsJarEditor = CodeJar(paramsEl, highlight, { tab: '  ' });

    getCode = () => _codeJarEditor ? _codeJarEditor.toString() : (codeEl?.textContent ?? '');
    getParams = () => _paramsJarEditor ? _paramsJarEditor.toString() : (paramsEl?.textContent ?? '');
  } catch (e) {
    // Fallback: enable plain contenteditable editors if CodeJar fails to load
    console.error('Failed to load CodeJar for Action Runner editor, falling back to plain editor.', e);
    if (codeEl) {
      codeEl.setAttribute('contenteditable', 'true');
    }
    if (paramsEl) {
      paramsEl.setAttribute('contenteditable', 'true');
    }
    getCode = () => (codeEl?.textContent ?? '');
    getParams = () => (paramsEl?.textContent ?? '');
  }

  btn.addEventListener('click', async () => {
    const accountKey = getApiKey();
    const usageKeyEl = document.getElementById('action-runner-usage-key');
    const usageKey = usageKeyEl?.value?.trim() ?? '';
    const apiKey = usageKey || accountKey;
    const code = (getCode ? getCode() : (codeEl?.textContent ?? '')).trim();
    const paramsRaw = (getParams ? getParams() : (paramsEl?.textContent ?? '')).trim();

    if (!accountKey) {
      hideStatus('action-runner-status');
      outputEl.textContent = 'Log in first to execute Lit Actions.';
      outputEl.className = 'action-runner-output error';
      return;
    }
    if (!code) {
      hideStatus('action-runner-status');
      outputEl.textContent = 'Enter Lit Action code.';
      outputEl.className = 'action-runner-output error';
      return;
    }

    let jsParams = null;
    if (paramsRaw) {
      try {
        jsParams = JSON.parse(paramsRaw);
      } catch (e) {
        outputEl.textContent = 'Invalid JSON in parameters: ' + (e && e.message ? e.message : String(e));
        outputEl.className = 'action-runner-output error';
        return;
      }
    }

    hideStatus('action-runner-status');
    outputEl.textContent = 'Executing…';
    outputEl.className = 'action-runner-output';
    btn.disabled = true;

    try {
      const client = await getClient();
      const result = await client.litAction({ apiKey, code, jsParams });
      outputEl.textContent = JSON.stringify(result, null, 2);
      outputEl.className = 'action-runner-output success';
    } catch (e) {
      outputEl.textContent = 'Error: ' + (e && e.message ? e.message : String(e));
      outputEl.className = 'action-runner-output error';
    } finally {
      btn.disabled = false;
    }
  });

  btnGetCid?.addEventListener('click', async () => {
    const code = getCode().trim();
    if (!code) {
      outputEl.textContent = 'Enter Lit Action code to get its IPFS CID.';
      outputEl.className = 'action-runner-output error';
      return;
    }
    outputEl.textContent = 'Fetching…';
    outputEl.className = 'action-runner-output';
    btnGetCid.disabled = true;
    try {
      const baseUrl = getBaseUrl().replace(/\/$/, '');
      const url = baseUrl + '/core/v1/get_lit_action_ipfs_id';
      const res = await fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(code),
      });
      const text = await res.text();
      if (!res.ok) throw new Error(text || res.status + ' ' + res.statusText);
      let cid = text;
      try {
        const parsed = JSON.parse(text);
        if (typeof parsed === 'string') cid = parsed;
      } catch (_) {}
      outputEl.textContent = cid;
      outputEl.className = 'action-runner-output success';
    } catch (e) {
      outputEl.textContent = 'Error: ' + (e && e.message ? e.message : String(e));
      outputEl.className = 'action-runner-output error';
    } finally {
      btnGetCid.disabled = false;
    }
  });
}

// ----- Init -----
function init() {
  setTheme(getTheme());
  initModalClose();
  initConfirmClose();
  updateAuthUI();
  initLogin();
  initOverview();
  initWallets();
  initGroups();
  initActions();
  initActionRunner(); // async; CodeJar loads lazily on first use
  initSidebar();
  initHeader();
  initBilling();
}

init();
