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
  const apiKeyTextEl = document.getElementById('account-api-key-text');
  if (apiKeyTextEl) {
    apiKeyTextEl.textContent = hasKey ? maskApiKey(getApiKey()) : '—';
  }
  if (hasKey) {
    refreshOverviewAccount();
    updateStatCards();
    preloadAllTables();
  }
}

// Preload groups, wallets, usage keys, and actions (for default group) when dashboard is shown
async function preloadAllTables() {
  const apiKey = getApiKey();
  if (!apiKey) return;
  try {
    const groups = await loadGroups();
    await loadWallets();
    await loadUsageKeys();
    const groupIdEl = document.getElementById('actions-group-id');
    let groupId = (groupIdEl && groupIdEl.value.trim()) || '';
    if (!groupId && groups && groups.length > 0) {
      groupId = String(groups[0].id);
      if (groupIdEl) groupIdEl.value = groupId;
    }
    if (!groupId) groupId = '0';
    await loadActions(groupId);
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
    hideStatus('login-status');
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
      const res = await client.newAccount({ accountName: name, accountDescription: desc });
      setApiKey(res.api_key);
      const walletMsg = res.wallet_address ? ' Wallet: ' + (res.wallet_address.slice(0, 10) + '…' + res.wallet_address.slice(-8)) : '';
      showStatus('login-status', 'Account created. You are now logged in.' + walletMsg, 'success');
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

function initOverview() {
  refreshOverviewAccount();
  renderUsageKeysTable();
  updateStatCards();
  document.getElementById('btn-load-usage-keys')?.addEventListener('click', () => loadUsageKeys());
  document.getElementById('btn-add-usage-key')?.addEventListener('click', () => openAddUsageKeyModal());
}

// ----- Icons (pencil, trash) -----
const ICON_PENCIL = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M3 17.25V21h3.75L17.81 9.94l-3.75-3.75L3 17.25zM20.71 7.04c.39-.39.39-1.02 0-1.41l-2.34-2.34c-.39-.39-1.02-.39-1.41 0l-1.83 1.83 3.75 3.75 1.83-1.83z"/></svg>';
const ICON_TRASH = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M6 19c0 1.1.9 2 2 2h8c1.1 0 2-.9 2-2V7H6v12zM19 4h-3.5l-1-1h-5l-1 1H5v2h14V4z"/></svg>';

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
    tbody.appendChild(tr);
  });
}

function renderActionsTable(items, groupId) {
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
    editBtn.addEventListener('click', () => openEditActionModal(item, groupId));
    actionsCell.appendChild(editBtn);
    const delBtn = document.createElement('button');
    delBtn.type = 'button';
    delBtn.className = 'btn-icon btn-icon-danger';
    delBtn.title = 'Delete';
    delBtn.innerHTML = ICON_TRASH;
    delBtn.addEventListener('click', () => confirmAndRemoveAction(item, groupId));
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
      '<td class="mono cell-address"></td>' +
      '<td class="mono">' + escapeHtml(description) + '</td>';
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
    const expiration = item.expiration != null ? String(item.expiration) : '—';
    const balance = item.balance != null ? String(item.balance) : '—';
    const tr = document.createElement('tr');
    tr.innerHTML =
      '<td>' + escapeHtml(item.name || '') + '</td>' +
      '<td class="mono">' + escapeHtml(item.description || '') + '</td>' +
      '<td class="mono">' + escapeHtml(expiration) + '</td>' +
      '<td class="mono">' + escapeHtml(balance) + '</td>' +
      '<td class="cell-actions"></td>';
    const actionsCell = tr.querySelector('.cell-actions');
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
  };
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
      api_key: it.api_key,
      usage_api_key: it.api_key,
      name: it.name ?? '',
      description: it.description ?? '',
      expiration: it.expiration,
      balance: it.balance,
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

async function loadActions(groupId) {
  const apiKey = getApiKey();
  if (!apiKey || !groupId) return;
  hideStatus('actions-status');
  const btn = document.getElementById('btn-load-actions');
  if (btn) btn.disabled = true;
  try {
    const client = await getClient();
    const items = await client.listActions({ apiKey, groupId, pageNumber: '0', pageSize: LIST_PAGE_SIZE });
    renderActionsTable(items, groupId);
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
function openAddGroupModal() {
  const body =
    '<div class="form-group"><label for="modal-group-name">Name</label><input type="text" id="modal-group-name" class="input" placeholder="My group"></div>' +
    '<div class="form-group"><label for="modal-group-desc">Description</label><input type="text" id="modal-group-desc" class="input" placeholder="Optional"></div>' +
    '<label class="checkbox-label"><input type="checkbox" id="modal-group-all-wallets"> All wallets permitted</label>' +
    '<label class="checkbox-label"><input type="checkbox" id="modal-group-all-actions"> All actions permitted</label>';
  const footer =
    '<button type="button" class="btn btn-outline" id="modal-cancel-btn">Cancel</button>' +
    '<button type="button" class="btn btn-primary" id="modal-add-btn">Add</button>';
  openModal('Add group', body, footer);
  document.getElementById('modal-cancel-btn').addEventListener('click', closeModal);
  document.getElementById('modal-add-btn').addEventListener('click', async () => {
    const name = document.getElementById('modal-group-name').value.trim();
    const desc = document.getElementById('modal-group-desc').value.trim();
    const allWallets = document.getElementById('modal-group-all-wallets').checked;
    const allActions = document.getElementById('modal-group-all-actions').checked;
    const apiKey = getApiKey();
    if (!apiKey || !name) {
      showStatus('groups-status', 'Enter a group name.', 'error');
      return;
    }
    closeModal();
    hideStatus('groups-status');
    try {
      showActionProgress('Creating group', `Creating group “${name}”.`);
      const client = await getClient();
      await client.addGroup({
        apiKey,
        groupName: name,
        groupDescription: desc,
        permittedActions: [],
        pkps: [],
        allWalletsPermitted: allWallets,
        allActionsPermitted: allActions,
      });
      await loadGroups();
      showStatus('groups-status', 'Group created.', 'success');
    } catch (e) {
      showStatus('groups-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
    } finally {
      closeActionProgress();
    }
  });
}

function openEditGroupModal(item) {
  const id = String(item.id);
  const body =
    '<div class="form-group"><label>Group ID</label><div class="mono">' + escapeHtml(id) + '</div></div>' +
    '<div class="form-group"><label for="modal-edit-group-name">Name</label><input type="text" id="modal-edit-group-name" class="input" value="' + escapeHtml(item.name || '') + '"></div>' +
    '<div class="form-group"><label for="modal-edit-group-desc">Description</label><input type="text" id="modal-edit-group-desc" class="input" value="' + escapeHtml(item.description || '') + '"></div>' +
    '<label class="checkbox-label"><input type="checkbox" id="modal-edit-group-all-wallets"' + (item.all_wallets_permitted ? ' checked' : '') + '> All wallets permitted</label>' +
    '<label class="checkbox-label"><input type="checkbox" id="modal-edit-group-all-actions"' + (item.all_actions_permitted ? ' checked' : '') + '> All actions permitted</label>';
  const footer =
    '<button type="button" class="btn btn-outline" id="modal-cancel-btn">Cancel</button>' +
    '<button type="button" class="btn btn-primary" id="modal-save-btn">Save</button>';
  openModal('Edit group', body, footer);
  document.getElementById('modal-cancel-btn').addEventListener('click', closeModal);
  document.getElementById('modal-save-btn').addEventListener('click', async () => {
    const name = document.getElementById('modal-edit-group-name').value.trim();
    const desc = document.getElementById('modal-edit-group-desc').value.trim();
    const allWallets = document.getElementById('modal-edit-group-all-wallets').checked;
    const allActions = document.getElementById('modal-edit-group-all-actions').checked;
    const apiKey = getApiKey();
    if (!apiKey || !name) {
      showStatus('groups-status', 'Enter a group name.', 'error');
      return;
    }
    closeModal();
    hideStatus('groups-status');
    try {
      showActionProgress('Updating group', `Updating group “${name}”.`);
      const client = await getClient();
      await client.updateGroup({ apiKey, groupId: id, name, description: desc, allWalletsPermitted: allWallets, allActionsPermitted: allActions });
      await loadGroups();
      showStatus('groups-status', 'Group updated.', 'success');
    } catch (e) {
      showStatus('groups-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
    } finally {
      closeActionProgress();
    }
  });
}

// ----- Action modals -----
function openAddActionModal() {
  const groupIdEl = document.getElementById('actions-group-id');
  const groupId = (groupIdEl && groupIdEl.value.trim()) || '0';
  const body =
    '<div class="form-group"><label for="modal-action-group-id">Group ID</label><input type="text" id="modal-action-group-id" class="input" value="' + escapeHtml(groupId) + '" placeholder="0"></div>' +
    '<div class="form-group"><label for="modal-action-cid">IPFS CID</label><input type="text" id="modal-action-cid" class="input" placeholder="Qm... or bafy..."></div>' +
    '<div class="form-group"><label for="modal-action-name">Name (optional)</label><input type="text" id="modal-action-name" class="input" placeholder="Action name"></div>' +
    '<div class="form-group"><label for="modal-action-desc">Description (optional)</label><input type="text" id="modal-action-desc" class="input" placeholder="Optional"></div>';
  const footer =
    '<button type="button" class="btn btn-outline" id="modal-cancel-btn">Cancel</button>' +
    '<button type="button" class="btn btn-primary" id="modal-add-btn">Add</button>';
  openModal('Add action', body, footer);
  document.getElementById('modal-cancel-btn').addEventListener('click', closeModal);
  document.getElementById('modal-add-btn').addEventListener('click', async () => {
    const gid = document.getElementById('modal-action-group-id').value.trim();
    const cid = document.getElementById('modal-action-cid').value.trim();
    const name = document.getElementById('modal-action-name').value.trim() || undefined;
    const desc = document.getElementById('modal-action-desc').value.trim() || undefined;
    const apiKey = getApiKey();
    if (!apiKey || !gid || !cid) {
      showStatus('actions-status', 'Fill Group ID and IPFS CID.', 'error');
      return;
    }
    closeModal();
    hideStatus('actions-status');
    try {
      showActionProgress('Adding action', `Adding action CID “${cid}” to group ${gid}.`);
      const client = await getClient();
      await client.addActionToGroup({ apiKey, groupId: gid, actionIpfsCid: cid, name, description: desc });
      if (groupIdEl) groupIdEl.value = gid;
      await loadActions(gid);
      showStatus('actions-status', 'Action added.', 'success');
    } catch (e) {
      showStatus('actions-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
    } finally {
      closeActionProgress();
    }
  });
}

function openEditActionModal(item, groupId) {
  const cid = item.ipfs_cid || item.cid || '';
  const body =
    '<div class="form-group"><label>Group ID</label><div class="mono">' + escapeHtml(groupId) + '</div></div>' +
    '<div class="form-group"><label>IPFS CID</label><div class="mono">' + escapeHtml(cid) + '</div></div>' +
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
    if (!apiKey || !groupId || !cid || !name) {
      showStatus('actions-status', 'Fill Name.', 'error');
      return;
    }
    closeModal();
    hideStatus('actions-status');
    try {
      showActionProgress('Updating action', `Updating action metadata for CID “${cid}”.`);
      const client = await getClient();
      await client.updateActionMetadata({ apiKey, groupId, actionIpfsCid: cid, name, description: desc });
      await loadActions(groupId);
      showStatus('actions-status', 'Action updated.', 'success');
    } catch (e) {
      showStatus('actions-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
    } finally {
      closeActionProgress();
    }
  });
}

async function confirmAndRemoveAction(item, groupId) {
  const cid = item.ipfs_cid || item.cid || String(item.id ?? '');
  const name = item.name || cid;
  const msg = 'Remove action "' + escapeHtml(name) + '" from this group? This cannot be undone.';
  const confirmed = await confirmDelete(msg);
  if (!confirmed) return;
  const apiKey = getApiKey();
  if (!apiKey) return;
  hideStatus('actions-status');
  try {
    showActionProgress('Removing action', `Removing action CID “${cid}” from group ${groupId}.`);
    const client = await getClient();
    await client.removeActionFromGroup({ apiKey, groupId, actionIpfsCid: cid });
    await loadActions(groupId);
    showStatus('actions-status', 'Action removed.', 'success');
  } catch (e) {
    showStatus('actions-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
  } finally {
    closeActionProgress();
  }
}

// ----- Usage API key Add modal and delete -----
function openAddUsageKeyModal() {
  const body =
    '<div class="form-group"><label for="modal-usage-name">Name (optional)</label><input type="text" id="modal-usage-name" class="input" placeholder="Optional"></div>' +
    '<div class="form-group"><label for="modal-usage-desc">Description (optional)</label><input type="text" id="modal-usage-desc" class="input" placeholder="Optional"></div>';
  const footer =
    '<button type="button" class="btn btn-outline" id="modal-cancel-btn">Cancel</button>' +
    '<button type="button" class="btn btn-primary" id="modal-add-btn">Add</button>';
  openModal('Add usage API key', body, footer);
  document.getElementById('modal-cancel-btn').addEventListener('click', closeModal);
  document.getElementById('modal-add-btn').addEventListener('click', async () => {
    const name = document.getElementById('modal-usage-name').value.trim() || '';
    const description = document.getElementById('modal-usage-desc').value.trim() || '';
    const apiKey = getApiKey();
    if (!apiKey) {
      showStatus('overview-status-usage-keys', 'Log in first.', 'error');
      return;
    }
    closeModal();
    hideStatus('overview-status-usage-keys');
    try {
      showActionProgress('Adding usage API key', 'Creating a new usage API key for this account.');
      const client = await getClient();
      const expiration = '9999999999';
      const balance = '1000000000000000000';
      const res = await client.addUsageApiKey({ apiKey, usageApiKey: '', expiration, balance });
      const usageKey = res && res.usage_api_key ? res.usage_api_key : '';
      if (usageKey && (name || description)) {
        await client.updateUsageApiKeyMetadata({ apiKey, usageApiKey: usageKey, name, description });
      }
      if (usageKey) {
        getUsageKeysStore().push({
          id: usageKey.slice(0, 12),
          api_key: usageKey,
          usage_api_key: usageKey,
          name: name || '',
          description: description || '',
          expiration: '9999999999',
          balance: 1000000000000000000,
        });
        window._statUsageKeys = getUsageKeysStore().length;
        renderUsageKeysTable();
        updateStatCards();
      }
      showStatus('overview-status-usage-keys', 'Usage API key added. Copy and store your key now (shown once).', 'success');
    } catch (e) {
      showStatus('overview-status-usage-keys', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
    } finally {
      closeActionProgress();
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
    showActionProgress('Removing usage API key', `Removing usage API key “${masked}”.`);
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
  document.getElementById('btn-load-actions')?.addEventListener('click', () => {
    const groupId = document.getElementById('actions-group-id').value.trim();
    if (groupId) loadActions(groupId);
    else showStatus('actions-status', 'Enter Group ID.', 'error');
  });
  document.getElementById('btn-add-action')?.addEventListener('click', () => openAddActionModal());
}

// ----- Sidebar scroll -----
function initSidebar() {
  document.querySelectorAll('.sidebar-link[data-scroll]').forEach((a) => {
    a.addEventListener('click', (e) => {
      e.preventDefault();
      const id = a.getAttribute('data-scroll');
      const el = document.getElementById('section-' + id);
      if (el) el.scrollIntoView({ behavior: 'smooth', block: 'start' });
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

  const copyBtn = document.getElementById('account-copy-btn');
  if (copyBtn) {
    copyBtn.addEventListener('click', async (e) => {
      e.stopPropagation();
      const key = getApiKey();
      if (!key) return;
      try {
        await navigator.clipboard.writeText(key);
        const orig = copyBtn.textContent;
        copyBtn.textContent = 'Copied!';
        copyBtn.setAttribute('title', 'Copied');
        setTimeout(() => {
          copyBtn.textContent = orig;
          copyBtn.setAttribute('title', 'Copy full API key');
        }, 1500);
      } catch (_) {
        try {
          const ta = document.createElement('textarea');
          ta.value = key;
          ta.setAttribute('readonly', '');
          ta.style.position = 'fixed';
          ta.style.opacity = '0';
          document.body.appendChild(ta);
          ta.select();
          document.execCommand('copy');
          document.body.removeChild(ta);
          copyBtn.textContent = 'Copied!';
          setTimeout(() => { copyBtn.textContent = 'Copy'; }, 1500);
        } catch (__) {}
      }
    });
  }

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
function initActionRunner() {
  const codeEl = document.getElementById('action-runner-code');
  const paramsEl = document.getElementById('action-runner-params');
  const btn = document.getElementById('btn-execute-lit-action');
  const btnGetCid = document.getElementById('btn-get-lit-action-ipfs-cid');
  const outputEl = document.getElementById('action-runner-output');
  const statusEl = document.getElementById('action-runner-status');

  if (!btn || !outputEl) return;

  btn.addEventListener('click', async () => {
    const apiKey = getApiKey();
    const code = codeEl?.value?.trim() ?? '';
    const paramsRaw = paramsEl?.value?.trim() ?? '';

    if (!apiKey) {
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
    const code = codeEl?.value?.trim() ?? '';
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
      const url = baseUrl + '/core/v1/get_lit_action_ipfs_id/' + encodeURIComponent(code);
      const res = await fetch(url);
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
  initActionRunner();
  initSidebar();
  initHeader();
}

init();
