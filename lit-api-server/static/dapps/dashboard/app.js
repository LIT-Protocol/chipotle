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
  if (typeof location !== 'undefined' && location.origin && (location.origin.startsWith('http://') || location.origin.startsWith('https://')))
    return location.origin;
  return 'http://localhost:8000';
}

function updateAuthUI() {
  const hasKey = !!getApiKey();
  document.body.classList.toggle('has-api-key', hasKey);
  if (hasKey) {
    refreshOverviewAccount();
    updateStatCards();
    preloadAllTables();
  }
}

// Preload groups, wallets, and actions (for default group) when dashboard is shown
async function preloadAllTables() {
  const apiKey = getApiKey();
  if (!apiKey) return;
  try {
    const groups = await loadGroups();
    await loadWallets();
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
  const apiKey = getApiKey();
  const elActions = document.getElementById('stat-actions');
  const elApi = document.getElementById('stat-api');
  if (elActions) elActions.textContent = (typeof window._statActions === 'number') ? window._statActions : '—';
  if (elApi) elApi.textContent = apiKey ? maskApiKey(apiKey) : '—';
  const elGroups = document.getElementById('stat-groups');
  const elWallets = document.getElementById('stat-wallets');
  if (elGroups) elGroups.textContent = (typeof window._statGroups === 'number') ? window._statGroups : '—';
  if (elWallets) elWallets.textContent = (typeof window._statWallets === 'number') ? window._statWallets : '—';
}

// ----- Theme -----
function getTheme() {
  return sessionStorage.getItem(STORAGE_KEY_THEME) || 'light';
}

function setTheme(theme) {
  sessionStorage.setItem(STORAGE_KEY_THEME, theme);
  document.documentElement.setAttribute('data-theme', theme === 'dark' ? 'dark' : 'light');
  const btn = document.getElementById('theme-toggle');
  if (btn) btn.textContent = theme === 'dark' ? 'Light' : 'Dark';
}

// ----- Login -----
function initLogin() {
  const apiKeyInput = document.getElementById('login-api-key');
  if (getApiKey()) apiKeyInput.value = '';

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
    const initialBalance = document.getElementById('new-account-initial-balance').value.trim() || undefined;
    hideStatus('login-status');
    if (!name) {
      showStatus('login-status', 'Enter an account name.', 'error');
      return;
    }
    const btn = document.getElementById('btn-create-account');
    btn.disabled = true;
    try {
      const client = await getClient();
      const res = await client.newAccount({ accountName: name, accountDescription: desc, initialBalance });
      setApiKey(res.api_key);
      showStatus('login-status', 'Account created. You are now logged in.', 'success');
      document.getElementById('new-account-name').value = '';
      document.getElementById('new-account-desc').value = '';
    } catch (e) {
      showStatus('login-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
    } finally {
      btn.disabled = false;
    }
  });
}

// ----- Overview -----
function maskApiKey(key) {
  if (!key || key.length < 12) return '••••••••';
  return key.slice(0, 6) + '••••••••' + key.slice(-4);
}

function refreshOverviewAccount() {
  const apiKey = getApiKey();
  const elKey = document.getElementById('overview-api-key-masked');
  const elBadge = document.getElementById('overview-status-badge');
  if (elKey) elKey.textContent = apiKey ? maskApiKey(apiKey) : '—';
  if (elBadge) {
    elBadge.textContent = apiKey ? 'Verified' : '—';
    elBadge.className = 'status-badge' + (apiKey ? ' status-badge-success' : '');
  }
}

function initOverview() {
  refreshOverviewAccount();

  document.getElementById('btn-load-overview-summary').addEventListener('click', async () => {
    const apiKey = getApiKey();
    if (!apiKey) {
      showStatus('overview-status', 'Log in first.', 'error');
      return;
    }
    hideStatus('overview-status');
    const summaryEl = document.getElementById('overview-summary');
    const countsEl = summaryEl ? summaryEl.querySelector('.overview-counts') : null;
    const btn = document.getElementById('btn-load-overview-summary');
    btn.disabled = true;
    try {
      const client = await getClient();
      const [groups, wallets] = await Promise.all([
        client.listGroups({ apiKey, pageNumber: '0', pageSize: '10' }),
        client.listWallets({ apiKey, pageNumber: '0', pageSize: '10' }),
      ]);
      if (summaryEl) summaryEl.style.display = 'block';
      if (countsEl) countsEl.textContent = 'Groups: ' + groups.length + (groups.length >= 10 ? '+' : '') + '  ·  Wallets: ' + wallets.length + (wallets.length >= 10 ? '+' : '');
      renderMetadataList('overview-groups-list', null, groups);
      renderMetadataList('overview-wallets-list', null, wallets);
      window._statGroups = groups.length;
      window._statWallets = wallets.length;
      updateStatCards();
    } catch (e) {
      showStatus('overview-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
    } finally {
      btn.disabled = false;
    }
  });

  document.getElementById('btn-add-usage-key').addEventListener('click', async () => {
    const apiKey = getApiKey();
    const usageKey = document.getElementById('usage-key-value').value.trim();
    const expiration = document.getElementById('usage-key-expiration').value.trim();
    const balance = document.getElementById('usage-key-balance').value.trim();
    if (!apiKey || !usageKey || !expiration || !balance) {
      showStatus('overview-status', 'Fill Usage API key, Expiration, and Balance.', 'error');
      return;
    }
    hideStatus('overview-status');
    const btn = document.getElementById('btn-add-usage-key');
    btn.disabled = true;
    try {
      const client = await getClient();
      await client.addUsageApiKey({ apiKey, usageApiKey: usageKey, expiration, balance });
      showStatus('overview-status', 'Usage API key added.', 'success');
    } catch (e) {
      showStatus('overview-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
    } finally {
      btn.disabled = false;
    }
  });

  document.getElementById('btn-remove-usage-key').addEventListener('click', async () => {
    const apiKey = getApiKey();
    const usageKey = document.getElementById('usage-key-value').value.trim();
    if (!apiKey || !usageKey) {
      showStatus('overview-status', 'Fill Usage API key.', 'error');
      return;
    }
    hideStatus('overview-status');
    const btn = document.getElementById('btn-remove-usage-key');
    btn.disabled = true;
    try {
      const client = await getClient();
      await client.removeUsageApiKey({ apiKey, usageApiKey: usageKey });
      showStatus('overview-status', 'Usage API key removed.', 'success');
    } catch (e) {
      showStatus('overview-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
    } finally {
      btn.disabled = false;
    }
  });

  document.getElementById('btn-update-usage-key-meta').addEventListener('click', async () => {
    const apiKey = getApiKey();
    const usageKey = document.getElementById('usage-key-value').value.trim();
    const name = document.getElementById('usage-key-name').value.trim();
    const description = document.getElementById('usage-key-desc').value.trim();
    if (!apiKey || !usageKey || !name) {
      showStatus('overview-status', 'Fill Usage API key and Name.', 'error');
      return;
    }
    hideStatus('overview-status');
    const btn = document.getElementById('btn-update-usage-key-meta');
    btn.disabled = true;
    try {
      const client = await getClient();
      await client.updateUsageApiKeyMetadata({ apiKey, usageApiKey: usageKey, name, description });
      showStatus('overview-status', 'Usage API key metadata updated.', 'success');
    } catch (e) {
      showStatus('overview-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
    } finally {
      btn.disabled = false;
    }
  });
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
    const id = String(item.id);
    tr.innerHTML =
      '<td><strong>' + escapeHtml(item.name || '') + '</strong></td>' +
      '<td class="mono">' + escapeHtml(id) + '</td>' +
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
    const tr = document.createElement('tr');
    const name = item.name || item.wallet_address || item.address || '';
    const id = item.id != null ? String(item.id) : '—';
    tr.innerHTML =
      '<td class="mono">' + escapeHtml(name) + '</td>' +
      '<td class="mono">' + escapeHtml(id) + '</td>';
    tbody.appendChild(tr);
  });
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

  document.getElementById('btn-load-wallets-in-group')?.addEventListener('click', async () => {
    const apiKey = getApiKey();
    const groupId = document.getElementById('wallets-in-group-id').value.trim();
    if (!apiKey || !groupId) {
      showStatus('wallets-status', 'Log in and enter Group ID.', 'error');
      return;
    }
    hideStatus('wallets-status');
    const btn = document.getElementById('btn-load-wallets-in-group');
    btn.disabled = true;
    try {
      const client = await getClient();
      const items = await client.listWalletsInGroup({ apiKey, groupId, pageNumber: '0', pageSize: LIST_PAGE_SIZE });
      renderMetadataList('wallets-in-group-list', null, items);
    } catch (e) {
      showStatus('wallets-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
    } finally {
      btn.disabled = false;
    }
  });

  // Add wallet: open modal with "Create wallet" and Add & Cancel
  document.getElementById('btn-add-wallet')?.addEventListener('click', () => openAddWalletModal());

  document.getElementById('btn-add-pkp').addEventListener('click', async () => {
    const apiKey = getApiKey();
    const groupId = document.getElementById('add-pkp-group').value.trim();
    const pkp = document.getElementById('add-pkp-public-key').value.trim();
    if (!apiKey || !groupId || !pkp) {
      showStatus('wallets-status', 'Fill Group ID and PKP public key.', 'error');
      return;
    }
    hideStatus('wallets-status');
    const btn = document.getElementById('btn-add-pkp');
    btn.disabled = true;
    try {
      const client = await getClient();
      await client.addPkpToGroup({ apiKey, groupId, pkpPublicKey: pkp });
      showStatus('wallets-status', 'PKP added to group.', 'success');
    } catch (e) {
      showStatus('wallets-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
    } finally {
      btn.disabled = false;
    }
  });

  const btnRemovePkp = document.getElementById('btn-remove-pkp');
  if (btnRemovePkp) {
    btnRemovePkp.addEventListener('click', async () => {
      const apiKey = getApiKey();
      const groupId = document.getElementById('remove-pkp-group').value.trim();
      const pkp = document.getElementById('remove-pkp-public-key').value.trim();
      if (!apiKey || !groupId || !pkp) {
        showStatus('wallets-status', 'Fill Group ID and PKP public key.', 'error');
        return;
      }
      hideStatus('wallets-status');
      btnRemovePkp.disabled = true;
      try {
        const client = await getClient();
        await client.removePkpFromGroup({ apiKey, groupId, pkpPublicKey: pkp });
        showStatus('wallets-status', 'PKP removed from group.', 'success');
      } catch (e) {
        showStatus('wallets-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
      } finally {
        btnRemovePkp.disabled = false;
      }
    });
  }
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
      showStatus('groups-status', 'Group created.', 'success');
      loadGroups();
    } catch (e) {
      showStatus('groups-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
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
      const client = await getClient();
      await client.updateGroup({ apiKey, groupId: id, name, description: desc, allWalletsPermitted: allWallets, allActionsPermitted: allActions });
      showStatus('groups-status', 'Group updated.', 'success');
      loadGroups();
    } catch (e) {
      showStatus('groups-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
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
      const client = await getClient();
      await client.addActionToGroup({ apiKey, groupId: gid, actionIpfsCid: cid, name, description: desc });
      showStatus('actions-status', 'Action added.', 'success');
      if (groupIdEl) groupIdEl.value = gid;
      loadActions(gid);
    } catch (e) {
      showStatus('actions-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
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
      const client = await getClient();
      await client.updateActionMetadata({ apiKey, groupId, actionIpfsCid: cid, name, description: desc });
      showStatus('actions-status', 'Action updated.', 'success');
      loadActions(groupId);
    } catch (e) {
      showStatus('actions-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
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
    const client = await getClient();
    await client.removeActionFromGroup({ apiKey, groupId, actionIpfsCid: cid });
    showStatus('actions-status', 'Action removed.', 'success');
    loadActions(groupId);
  } catch (e) {
    showStatus('actions-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
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
      const client = await getClient();
      const res = await client.createWallet(apiKey);
      showStatus('wallets-status', 'Wallet created: ' + (res.wallet_address || ''), 'success');
      loadWallets();
    } catch (e) {
      showStatus('wallets-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
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
function initHeader() {
  document.getElementById('logout-btn').addEventListener('click', () => {
    setApiKey('');
  });

  document.getElementById('theme-toggle').addEventListener('click', () => {
    const next = getTheme() === 'dark' ? 'light' : 'dark';
    setTheme(next);
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
  initSidebar();
  initHeader();
}

init();
