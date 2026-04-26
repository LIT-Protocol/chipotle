/**
 * Authentication — session state, API client, theme, stat cards.
 */

import { showStatus, hideStatus, formatError, logError, showActionProgress, closeActionProgress, escapeHtml } from './ui-utils.js';

const STORAGE_KEY_API = 'accountconfig_api_key';
const STORAGE_KEY_THEME = 'accountconfig_theme';
const STORAGE_KEY_USAGE_OVERRIDE = 'accountconfig_usage_key_override';
const STORAGE_KEY_OVERRIDE_ENABLED = 'accountconfig_usage_override_enabled';
const STORAGE_KEY_MODE = 'accountconfig_mode';
const STORAGE_KEY_CHAINSECURED_WALLET = 'accountconfig_chainsecured_wallet';
const STORAGE_KEY_CHAINSECURED_HASH = 'accountconfig_chainsecured_hash';
export const LIST_PAGE_SIZE = '20';

/**
 * SDK methods that perform admin writes. In sovereign mode these get
 * auto-injected with a sovereignLifecycle (wallet connect + preview modal +
 * tx status banner) by the getClient() Proxy.
 *
 * Keep in sync with branched writes in core_sdk.js.
 */
const SOVEREIGN_WRITE_METHODS = new Set([
  'newChainSecuredAccount',
  'addGroup', 'removeGroup', 'updateGroup',
  'addAction', 'deleteAction', 'addActionToGroup', 'removeActionFromGroup', 'updateActionMetadata',
  'addPkpToGroup', 'removePkpFromGroup',
  'addUsageApiKey', 'updateUsageApiKey', 'removeUsageApiKey', 'updateUsageApiKeyMetadata',
]);

// ----- Mode (api | sovereign) -----

/** @returns {'api'|'sovereign'} */
export function getMode() {
  return sessionStorage.getItem(STORAGE_KEY_MODE) === 'sovereign' ? 'sovereign' : 'api';
}

export function setMode(mode) {
  if (mode === 'sovereign') sessionStorage.setItem(STORAGE_KEY_MODE, 'sovereign');
  else sessionStorage.removeItem(STORAGE_KEY_MODE);
  // Invalidate cached client so the next getClient() rebuilds with the new mode.
  resetClient();
}

// ----- API key session -----

export function getApiKey() {
  return sessionStorage.getItem(STORAGE_KEY_API) || '';
}

export function setApiKey(v) {
  if (v) sessionStorage.setItem(STORAGE_KEY_API, v);
  else sessionStorage.removeItem(STORAGE_KEY_API);
  import('./billing.js').then((m) => m.resetBillingAvailability()).catch(() => {});
  updateAuthUI();
}

// ----- ChainSecured session (wallet-backed) -----

export function getChainSecuredWallet() {
  return sessionStorage.getItem(STORAGE_KEY_CHAINSECURED_WALLET) || '';
}

export function getChainSecuredHash() {
  return sessionStorage.getItem(STORAGE_KEY_CHAINSECURED_HASH) || '';
}

/**
 * Persist a ChainSecured login: wallet address + precomputed apiKeyHash.
 * Pushes the hash onto the cached SDK client's `adminHashOverride` so the
 * 20 identity sites in core_sdk.js use the wallet-derived hash instead of
 * hashing an (empty) api key string.
 */
export async function setChainSecuredSession({ walletAddress, apiKeyHash }) {
  if (walletAddress) sessionStorage.setItem(STORAGE_KEY_CHAINSECURED_WALLET, walletAddress);
  else sessionStorage.removeItem(STORAGE_KEY_CHAINSECURED_WALLET);
  if (apiKeyHash) sessionStorage.setItem(STORAGE_KEY_CHAINSECURED_HASH, apiKeyHash);
  else sessionStorage.removeItem(STORAGE_KEY_CHAINSECURED_HASH);
  if (_clientInstance) _clientInstance.adminHashOverride = apiKeyHash || null;
  updateAuthUI();
}

/** Clears ChainSecured session markers and the client cache. */
export function clearChainSecuredSession() {
  sessionStorage.removeItem(STORAGE_KEY_CHAINSECURED_WALLET);
  sessionStorage.removeItem(STORAGE_KEY_CHAINSECURED_HASH);
  resetClient();
}

/** True if the user has any authenticated session — api key OR ChainSecured. */
export function isAuthenticated() {
  return !!getApiKey() || (getMode() === 'sovereign' && !!getChainSecuredWallet());
}

/** Full sign-out: clears api-key, ChainSecured session, and the client cache. */
export function logOut() {
  sessionStorage.removeItem(STORAGE_KEY_API);
  sessionStorage.removeItem(STORAGE_KEY_CHAINSECURED_WALLET);
  sessionStorage.removeItem(STORAGE_KEY_CHAINSECURED_HASH);
  clearOverrideState();
  resetClient();
  updateAuthUI();
}

/** Invalidate the cached SDK client; next getClient() rebuilds fresh. */
export function resetClient() {
  _clientInstance = null;
  _clientBaseUrl = null;
  _clientMode = null;
}

/** Returns the usage API key override if set, otherwise the account API key. */
export function getEffectiveApiKey() {
  return sessionStorage.getItem(STORAGE_KEY_USAGE_OVERRIDE) || getApiKey();
}

export function setUsageKeyOverride(v) {
  if (v) sessionStorage.setItem(STORAGE_KEY_USAGE_OVERRIDE, v);
  else sessionStorage.removeItem(STORAGE_KEY_USAGE_OVERRIDE);
  updateUsageKeyOverrideUI();
}

export function isOverrideEnabled() {
  return sessionStorage.getItem(STORAGE_KEY_OVERRIDE_ENABLED) === 'true';
}

export function toggleOverrideEnabled() {
  const next = !isOverrideEnabled();
  if (next) {
    sessionStorage.setItem(STORAGE_KEY_OVERRIDE_ENABLED, 'true');
  } else {
    sessionStorage.removeItem(STORAGE_KEY_OVERRIDE_ENABLED);
    setUsageKeyOverride('');
  }
  updateUsageKeyOverrideUI();
}

export function hasUsageKeyOverride() {
  return !!sessionStorage.getItem(STORAGE_KEY_USAGE_OVERRIDE);
}

export function updateUsageKeyOverrideUI() {
  const card = document.getElementById('usage-key-override-card');
  const badge = document.getElementById('usage-key-override-badge');
  const input = document.getElementById('usage-key-override-input');
  const clearBtn = document.getElementById('usage-key-override-clear');
  const toggleBtn = document.getElementById('toggle-usage-override-btn');
  const enabled = isOverrideEnabled();
  const hasOverride = hasUsageKeyOverride();
  if (card) card.style.display = enabled ? '' : 'none';
  if (badge) {
    badge.style.display = hasOverride ? '' : 'none';
    if (hasOverride) badge.textContent = 'Using Key: ' + sessionStorage.getItem(STORAGE_KEY_USAGE_OVERRIDE).substring(0, 6) + '\u2026';
  }
  if (input) input.value = sessionStorage.getItem(STORAGE_KEY_USAGE_OVERRIDE) || '';
  if (clearBtn) clearBtn.style.display = hasOverride ? '' : 'none';
  if (toggleBtn) toggleBtn.textContent = enabled ? '\u2713 Usage Key Override' : 'Usage Key Override';
  import('./billing.js').then((m) => m.refreshBillingUI()).catch(() => {});
}

export function clearOverrideState() {
  sessionStorage.removeItem(STORAGE_KEY_OVERRIDE_ENABLED);
  setUsageKeyOverride('');
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
let _clientMode = null;

export async function getClient() {
  const baseUrl = getBaseUrl();
  const mode = getMode();
  if (_clientInstance && _clientBaseUrl === baseUrl && _clientMode === mode) {
    return _clientInstance;
  }
  try {
    const { createClient } = await import('../../core_sdk.js');
    const opts = { baseUrl, mode };
    if (mode === 'sovereign') {
      // Bootstrap RPC + contract address via the server config endpoint
      // (explicitly stays on the API path per CPL-267 plan).
      const bootstrap = createClient({ baseUrl });
      const cfg = await bootstrap.getNodeChainConfig();
      if (!cfg || !cfg.rpc_url || !cfg.contract_address) {
        throw new Error('Node chain config missing rpc_url or contract_address');
      }
      opts.rpcUrl = cfg.rpc_url;
      opts.contractAddress = cfg.contract_address;
      if (cfg.chain_id != null) opts.chainId = Number(cfg.chain_id);
    }
    // Re-apply any active ChainSecured adminHash override so freshly-built
    // clients resume with wallet-derived identity.
    if (mode === 'sovereign') {
      const chainSecuredHash = getChainSecuredHash();
      if (chainSecuredHash) opts.adminHashOverride = chainSecuredHash;
    }
    const client = createClient(opts);
    _clientInstance = new Proxy(client, {
      get(target, prop) {
        const val = target[prop];
        if (typeof val !== 'function') return val;
        return async function (...args) {
          const apiKey = (args[0] && typeof args[0] === 'object' && args[0].apiKey) || args[0];
          let authPreview;
          if (mode === 'sovereign' && target.adminHashOverride) {
            const wallet = getChainSecuredWallet();
            authPreview = wallet ? `wallet: ${wallet.slice(0, 6)}\u2026${wallet.slice(-4)}` : `hash: ${target.adminHashOverride.slice(0, 10)}\u2026`;
          } else {
            authPreview = typeof apiKey === 'string' ? `key: ${apiKey.substring(0, 6)}\u2026` : 'auth: (none)';
          }
          console.log(`[dashboard:${mode}] ${prop} \u2192 ${baseUrl} | ${authPreview}`);

          // Sovereign-mode writes: auto-inject lifecycle (wallet connect +
          // preview modal + tx status banner) unless caller already supplied
          // its own sovereignLifecycle.
          if (mode === 'sovereign' && SOVEREIGN_WRITE_METHODS.has(prop) &&
              args[0] && typeof args[0] === 'object' && !args[0].sovereignLifecycle) {
            await ensureSovereignSigner(target);
            const sovereignLifecycle = await buildSovereignLifecycle(target, prop);
            args = [{ ...args[0], sovereignLifecycle }];
          }
          return val.apply(target, args);
        };
      },
    });
    _clientBaseUrl = baseUrl;
    _clientMode = mode;
    return _clientInstance;
  } catch (e) {
    logError('getClient', e);
    throw new Error('Unable to connect to API: ' + formatError(e));
  }
}

/**
 * Lazy wallet-connect for sovereign writes. Prompts the user to connect a
 * browser wallet (MetaMask et al.) on first sovereign write. Also enforces
 * the chain guard against the SDK's expected chainId.
 */
async function ensureSovereignSigner(client) {
  if (client.signer) return;
  const { connectEoa, switchChain } = await import('../../wallet_connect.js');
  let { signer, chainId } = await connectEoa();
  if (client.chainId != null && chainId !== client.chainId) {
    try {
      await switchChain(client.chainId);
    } catch (err) {
      throw new Error(
        `Your wallet is on chain ${chainId} but this dashboard expects chain ${client.chainId}. Switch network in your wallet and retry.`,
      );
    }
    // switchChain rebinds window.ethereum to the new network; re-derive the
    // signer from a fresh BrowserProvider so ethers caches the new chainId
    // instead of signing against the previous network.
    ({ signer } = await connectEoa());
  }
  client.connectSigner(signer);
}

/**
 * Build the sovereignLifecycle hook set for a given SDK method invocation.
 * onPreview: blocks on the tx preview modal (must resolve true to proceed).
 * onState: surfaces tx status in a non-dismissible progress banner.
 */
async function buildSovereignLifecycle(client, sdkMethodName) {
  const [{ showTxPreview }, { describeState, TX_STATES }] = await Promise.all([
    import('./tx_preview_modal.js'),
    import('../../tx_lifecycle.js'),
  ]);
  const signerAddress = client.signer ? await client.signer.getAddress() : null;
  const meta = {
    signerAddress,
    chainId: client.chainId,
    contractAddress: client.contractAddress,
  };
  return {
    onPreview: (contractMethod, contractArgs) =>
      showTxPreview(contractMethod, contractArgs, meta),
    onState: (state, payload) => {
      if (state === TX_STATES.SIGNING) {
        showActionProgress(sdkMethodName, 'Open your wallet to sign the transaction…');
      } else if (state === TX_STATES.PENDING) {
        showActionProgress(sdkMethodName, `Broadcast — ${payload?.txHash?.slice(0, 10)}… waiting for inclusion.`);
      } else if (state === TX_STATES.CONFIRMING) {
        showActionProgress(sdkMethodName, `Included — waiting for ${payload?.target} confirmations.`);
      } else if (state === TX_STATES.CONFIRMED || state === TX_STATES.FAILED || state === TX_STATES.REORGED) {
        closeActionProgress();
      } else {
        showActionProgress(sdkMethodName, describeState(state));
      }
    },
  };
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
  const usageKeysVal = (typeof _stats.usageKeys === 'number') ? _stats.usageKeys : getUsageKeysStore().length;
  if (elUsageKeys) elUsageKeys.textContent = usageKeysVal;
  if (elGroups) elGroups.textContent = (typeof _stats.groups === 'number') ? _stats.groups : '—';
  if (elWallets) elWallets.textContent = (typeof _stats.wallets === 'number') ? _stats.wallets : '—';
  if (elActions) elActions.textContent = (typeof _stats.actions === 'number') ? _stats.actions : '—';

  // Empty hero ↔ stats grid swap. Only swap once all four stores have resolved
  // to numbers, otherwise we'd flash the hero during initial load.
  const allResolved = typeof _stats.groups === 'number'
    && typeof _stats.wallets === 'number'
    && typeof _stats.actions === 'number';
  const allZero = allResolved
    && usageKeysVal === 0
    && _stats.groups === 0
    && _stats.wallets === 0
    && _stats.actions === 0;
  const heroEl = document.getElementById('overview-empty-state');
  const statsEl = document.getElementById('stats-row');
  if (heroEl && statsEl) {
    heroEl.hidden = !allZero;
    statsEl.hidden = allZero;
  }
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
  const authed = isAuthenticated();
  const isChainSecured = authed && getMode() === 'sovereign' && !!getChainSecuredWallet();
  // Keep the legacy `has-api-key` hook — it's what existing CSS gates the
  // dashboard shell on. `is-chainsecured` toggles ChainSecured-specific UI
  // (hides Action Runner + Billing, shows owner pill, etc.).
  document.body.classList.toggle('has-api-key', authed);
  document.body.classList.toggle('is-chainsecured', isChainSecured);
  renderModeBadge();
  import('./billing.js').then((m) => m.refreshBillingUI()).catch(() => {});
  if (authed && _onAuthReady) {
    _onAuthReady();
  }
}

/**
 * Paint the topbar mode badge + (ChainSecured only) owner wallet pill.
 * Called on every `updateAuthUI` pass. ChainSecured pill paints from session
 * immediately; on-chain reconciliation via `getAccountWalletAddress` runs
 * async and overwrites if the chain disagrees.
 */
function renderModeBadge() {
  const host = document.querySelector('.topbar-title');
  if (!host) return;
  const mode = getMode();
  if (!isAuthenticated()) {
    host.innerHTML = '&nbsp;';
    return;
  }
  const isChainSecured = mode === 'sovereign' && !!getChainSecuredWallet();
  const modeLabel = isChainSecured ? 'ChainSecured mode' : 'API mode';
  const popoverContent = isChainSecured
    ? `<strong>ChainSecured mode</strong>
       <p>Your wallet is the account identity. Writes are signed on-chain as transactions you authorize in your wallet.</p>
       <p class="mode-popover-hidden">Hidden in this mode: Action Runner, Billing.</p>`
    : `<strong>API mode</strong>
       <p>Writes go through the Lit Express API using your account API key. Fastest path, no wallet popups.</p>`;
  let pillHtml = '';
  if (isChainSecured) {
    const wallet = getChainSecuredWallet();
    const trunc = `${wallet.slice(0, 6)}\u2026${wallet.slice(-4)}`;
    pillHtml = ` <button type="button" class="topbar-wallet-pill" id="topbar-wallet-pill" title="Copy wallet address" data-wallet="${escapeHtml(wallet)}">${escapeHtml(trunc)}</button>`;
  }
  host.innerHTML = `<span class="topbar-mode-badge-wrap">
      <button type="button" class="topbar-mode-badge" id="topbar-mode-badge" aria-haspopup="dialog" aria-expanded="false">${escapeHtml(modeLabel)}</button>
      <div class="topbar-mode-popover" id="topbar-mode-popover" role="dialog" aria-label="${escapeHtml(modeLabel)} details" hidden>${popoverContent}</div>
    </span>${pillHtml}`;
  const badgeBtn = document.getElementById('topbar-mode-badge');
  const popover = document.getElementById('topbar-mode-popover');
  if (badgeBtn && popover) {
    const close = () => {
      popover.hidden = true;
      badgeBtn.setAttribute('aria-expanded', 'false');
      document.removeEventListener('click', onDocClick, true);
      document.removeEventListener('keydown', onKeyDown, true);
    };
    const onDocClick = (ev) => {
      if (!badgeBtn.contains(ev.target) && !popover.contains(ev.target)) close();
    };
    const onKeyDown = (ev) => { if (ev.key === 'Escape') close(); };
    badgeBtn.addEventListener('click', (ev) => {
      ev.stopPropagation();
      const open = popover.hidden;
      popover.hidden = !open;
      badgeBtn.setAttribute('aria-expanded', String(open));
      if (open) {
        document.addEventListener('click', onDocClick, true);
        document.addEventListener('keydown', onKeyDown, true);
      }
    });
  }
  const pill = document.getElementById('topbar-wallet-pill');
  if (pill) {
    pill.addEventListener('click', async () => {
      const { copyToClipboard } = await import('./ui-utils.js');
      await copyToClipboard(pill.dataset.wallet, pill);
    });
  }
}

/**
 * Start the wallet-change watcher once. Logs out if the user disconnects the
 * wallet or switches to a different account mid-session.
 */
let _walletWatchStarted = false;
async function ensureWalletWatch() {
  if (_walletWatchStarted) return;
  _walletWatchStarted = true;
  try {
    const { onWalletChange } = await import('../../wallet_connect.js');
    onWalletChange((snap) => {
      if (getMode() !== 'sovereign' || !getChainSecuredWallet()) return;
      const expected = getChainSecuredWallet().toLowerCase();
      if (!snap.connected) {
        logOut();
      } else if (snap.address && snap.address.toLowerCase() !== expected) {
        logOut();
      }
    });
  } catch (e) {
    console.warn('[auth] onWalletChange wiring failed:', e);
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
  // Null-deref guard (TODOS.md:3-9) — pages that don't host the login form
  // still import this module via app.js; skip wiring when the form is absent.
  if (!apiKeyInput) {
    // If a session already exists, surface the authed state on any page that
    // imports auth (e.g. monitor's shared topbar) and return.
    if (isAuthenticated()) updateAuthUI();
    return;
  }
  if (getApiKey()) apiKeyInput.value = '';

  ensureWalletWatch();

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

  // ----- Card A (Existing): API-key login -----
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
      setMode('api');
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

  // ----- Card B (Existing): ChainSecured wallet login -----
  const btnLoginWallet = document.getElementById('btn-login-wallet');
  if (btnLoginWallet) {
    btnLoginWallet.addEventListener('click', () => loginWithWallet(btnLoginWallet));
  }

  // If a session already exists, trigger the auth-ready flow on load
  if (isAuthenticated()) {
    updateAuthUI();
  }

  // ----- Card A (New): managed account creation -----
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
      setMode('api');
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

  // ----- Card B (New): ChainSecured account creation -----
  const btnCreateChainSecured = document.getElementById('btn-create-chainsecured');
  if (btnCreateChainSecured) {
    btnCreateChainSecured.addEventListener('click', () => createChainSecuredAccount(btnCreateChainSecured));
  }
}

/**
 * Card B (Existing) click handler. Error ordering matters here — we prefetch
 * chain config BEFORE popping MetaMask so a failed API server doesn't waste a
 * wallet-connect. Same rule applies to newChainSecuredAccount.
 */
async function loginWithWallet(btn) {
  hideStatus('login-status');
  btn.disabled = true;
  const prevMode = getMode();
  try {
    setMode('sovereign');
    // Prefetch chain config via getClient() sovereign bootstrap — throws
    // BEFORE wallet popup on unreachable API server.
    const client = await getClient();
    const { connectEoa } = await import('../../wallet_connect.js');
    const ethers = await loadEthersLocal();
    const { address } = await connectEoa();
    const apiKeyHash = ethers.solidityPackedKeccak256(['address'], [address]);
    const exists = await client.accountExistsByHash(apiKeyHash);
    if (!exists) {
      setMode(prevMode); // revert
      showStatus(
        'login-status',
        'No ChainSecured account found for this wallet. Sign up in the New User tab.',
        'error',
      );
      return;
    }
    await setChainSecuredSession({ walletAddress: address, apiKeyHash });
    showStatus('login-status', 'Connected. Wallet is the account admin.', 'success');
    runPostConnectDriftCheck(client).catch((e) => logError('drift-check', e));
  } catch (e) {
    setMode(prevMode);
    logError('login-wallet', e);
    showStatus('login-status', 'Error: ' + formatError(e), 'error');
  } finally {
    btn.disabled = false;
  }
}

/**
 * Card B (New) click handler. Creates a ChainSecured account via the
 * WritesFacet.newChainSecuredAccount path; the Proxy injects the
 * sovereignLifecycle (wallet connect + preview modal + tx banner).
 */
async function createChainSecuredAccount(btn) {
  const name = (document.getElementById('new-chainsecured-name')?.value || '').trim();
  const desc = (document.getElementById('new-chainsecured-desc')?.value || '').trim();
  hideStatus('login-status');
  if (!name) {
    showStatus('login-status', 'Enter an account name.', 'error');
    return;
  }
  btn.disabled = true;
  const prevMode = getMode();
  try {
    setMode('sovereign');
    const client = await getClient();
    // Proxy auto-runs ensureSovereignSigner → wallet-connect before the tx.
    const res = await client.newChainSecuredAccount({ accountName: name, accountDescription: desc });
    await setChainSecuredSession({ walletAddress: res.wallet_address, apiKeyHash: res.api_key_hash });
    showChainSecuredBanner(res.wallet_address);
    const nameEl = document.getElementById('new-chainsecured-name');
    const descEl = document.getElementById('new-chainsecured-desc');
    if (nameEl) nameEl.value = '';
    if (descEl) descEl.value = '';
    runPostConnectDriftCheck(client).catch((e) => logError('drift-check', e));
  } catch (e) {
    setMode(prevMode);
    logError('create-chainsecured', e);
    showStatus('login-status', 'Error: ' + formatError(e), 'error');
  } finally {
    btn.disabled = false;
  }
}

async function loadEthersLocal() {
  if (typeof globalThis !== 'undefined' && globalThis.ethers) return globalThis.ethers;
  const mod = await import(/* @vite-ignore */ 'https://cdn.jsdelivr.net/npm/ethers@6.13.0/dist/ethers.min.js');
  const e = mod.ethers ?? mod.default ?? mod;
  if (typeof globalThis !== 'undefined') globalThis.ethers = e;
  return e;
}

async function runPostConnectDriftCheck(client) {
  if (typeof client.checkAbiDrift !== 'function') return;
  const result = await client.checkAbiDrift();
  const banner = document.getElementById('abi-drift-banner');
  if (!banner) return;
  if (!result.ok) {
    banner.textContent = 'ABI drift detected — on-chain contract does not match this dashboard build. Writes are disabled. Reason: ' + result.reason;
    banner.style.display = '';
  } else {
    banner.style.display = 'none';
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
    const { copyToClipboard } = await import('./ui-utils.js');
    await copyToClipboard(apiKey, copyBtn);
  };
  dismissBtn.onclick = () => { banner.style.display = 'none'; };
}

/**
 * ChainSecured post-creation success banner. Re-uses the `new-account-banner`
 * DOM when present; swaps the copy target from apiKey to wallet address and
 * removes the "save this key" phrasing (there is no key).
 */
function showChainSecuredBanner(walletAddress) {
  const banner = document.getElementById('new-account-banner');
  const keyEl = document.getElementById('new-account-key-text');
  const copyBtn = document.getElementById('new-account-copy-btn');
  const dismissBtn = document.getElementById('new-account-dismiss-btn');
  if (!banner || !keyEl || !copyBtn || !dismissBtn) return;
  const body = banner.querySelector('.new-account-banner-body');
  if (body) {
    body.innerHTML = `<strong>Account created.</strong> Connected wallet <code class="mono">${escapeHtml(walletAddress)}</code> is the admin. No API key to copy \u2014 your wallet signs all writes.
      <div class="new-account-key-row">
        <code id="new-account-key-text" class="new-account-key mono">${escapeHtml(walletAddress)}</code>
        <button type="button" id="new-account-copy-btn" class="btn btn-sm btn-outline">Copy</button>
      </div>`;
  }
  banner.style.display = '';
  const freshCopy = document.getElementById('new-account-copy-btn');
  if (freshCopy) {
    freshCopy.onclick = async () => {
      const { copyToClipboard } = await import('./ui-utils.js');
      await copyToClipboard(walletAddress, freshCopy);
    };
  }
  dismissBtn.onclick = () => { banner.style.display = 'none'; };
}
