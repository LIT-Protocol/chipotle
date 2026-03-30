/**
 * Authentication — session state, API client, theme, stat cards.
 */

import { showStatus, hideStatus, formatError, logError, showActionProgress, closeActionProgress, escapeHtml } from './ui-utils.js';

const STORAGE_KEY_API = 'accountconfig_api_key';
const STORAGE_KEY_THEME = 'accountconfig_theme';
export const LIST_PAGE_SIZE = '20';

// ----- API key session -----

export function getApiKey() {
  return sessionStorage.getItem(STORAGE_KEY_API) || '';
}

export function setApiKey(v) {
  if (v) sessionStorage.setItem(STORAGE_KEY_API, v);
  else sessionStorage.removeItem(STORAGE_KEY_API);
  updateAuthUI();
}

// ----- Base URL -----

export function getBaseUrl() {
  if (typeof location !== 'undefined' && location.origin && location.origin.indexOf('localhost') !== -1)
    return 'http://localhost:8000';
  return '__LIT_API_BASE_URL__';
}

// ----- API client (cached singleton) -----

let _clientInstance = null;
let _clientBaseUrl = null;

export async function getClient() {
  const baseUrl = getBaseUrl();
  if (_clientInstance && _clientBaseUrl === baseUrl) return _clientInstance;
  try {
    const { createClient } = await import('../../core_sdk.js');
    _clientInstance = createClient(baseUrl);
    _clientBaseUrl = baseUrl;
    return _clientInstance;
  } catch (e) {
    logError('getClient', e);
    throw new Error('Unable to connect to API: ' + formatError(e));
  }
}

// ----- Theme -----

const THEME_ICON_SUN = '<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41"/></svg>';
const THEME_ICON_MOON = '<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"/></svg>';

export function getTheme() {
  return sessionStorage.getItem(STORAGE_KEY_THEME) || 'light';
}

export function setTheme(theme) {
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

// ----- Stat cards -----

export function updateStatCards() {
  const elUsageKeys = document.getElementById('stat-usage-keys');
  const elGroups = document.getElementById('stat-groups');
  const elWallets = document.getElementById('stat-wallets');
  const elActions = document.getElementById('stat-actions');
  if (elUsageKeys) elUsageKeys.textContent = (typeof _stats.usageKeys === 'number') ? _stats.usageKeys : getUsageKeysStore().length;
  if (elGroups) elGroups.textContent = (typeof _stats.groups === 'number') ? _stats.groups : '—';
  if (elWallets) elWallets.textContent = (typeof _stats.wallets === 'number') ? _stats.wallets : '—';
  if (elActions) elActions.textContent = (typeof _stats.actions === 'number') ? _stats.actions : '—';
}

// ----- Module-scoped state (replaces window._*) -----

let _groups = [];
let _wallets = [];
let _usageKeys = [];
let _actions = [];

const _stats = { usageKeys: undefined, groups: undefined, wallets: undefined, actions: undefined };

export function getGroupsStore() { return _groups; }
export function setGroupsStore(items) { _groups = items; }

export function getWalletsStore() { return _wallets; }
export function setWalletsStore(items) { _wallets = items; }

export function getUsageKeysStore() { return _usageKeys; }
export function setUsageKeysStore(items) { _usageKeys = items; }

export function getActionsStore() { return _actions; }
export function setActionsStore(items) { _actions = items; }

export function setStat(key, value) { _stats[key] = value; }

// ----- Auth UI -----

/** @type {Function|null} Callback set by the overview module to trigger preloading */
let _onAuthReady = null;

export function setOnAuthReady(fn) { _onAuthReady = fn; }

function updateAuthUI() {
  const hasKey = !!getApiKey();
  document.body.classList.toggle('has-api-key', hasKey);
  const balanceEl = document.getElementById('billing-balance-display');
  const addFundsBtn = document.getElementById('btn-add-funds');
  if (balanceEl) balanceEl.style.display = hasKey ? '' : 'none';
  if (addFundsBtn) addFundsBtn.style.display = hasKey ? '' : 'none';
  if (hasKey && _onAuthReady) {
    _onAuthReady();
  }
}

// ----- Display helpers -----

export function maskApiKey(key) {
  if (!key || key.length < 12) return '••••••••';
  return key.slice(0, 6) + '••••••••' + key.slice(-4);
}

export function keyPreview(key) {
  if (!key || key.length < 9) return '••••••••';
  return key.slice(0, 4) + '…' + key.slice(-4);
}

// ----- Login -----

export function initLogin() {
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
      logError('login', e);
      showStatus('login-status', 'Error: ' + formatError(e), 'error');
    } finally {
      btn.disabled = false;
    }
  });

  // If a session already exists, trigger the auth-ready flow on load
  if (getApiKey()) {
    updateAuthUI();
  }

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
      logError('create-account', e);
      showStatus('login-status', 'Error: ' + formatError(e), 'error');
    } finally {
      closeActionProgress();
      btn.disabled = false;
    }
  });
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
    const { copyToClipboard } = await import('./ui-utils.js');
    await copyToClipboard(apiKey, copyBtn);
  };
  dismissBtn.onclick = () => { banner.style.display = 'none'; };
}
