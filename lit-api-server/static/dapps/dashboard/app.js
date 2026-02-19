/**
 * AccountConfig Dashboard – 4-page app
 * Uses core_sdk.js; apiKey and session state in sessionStorage.
 */

const STORAGE_KEY_API = 'accountconfig_api_key';
const STORAGE_KEY_BASE = 'accountconfig_base_url';
const STORAGE_KEY_GROUPS = 'accountconfig_groups';
const STORAGE_KEY_ACTIONS = 'accountconfig_actions';
const STORAGE_KEY_THEME = 'accountconfig_theme';

function getApiKey() {
  return sessionStorage.getItem(STORAGE_KEY_API) || '';
}

function setApiKey(v) {
  if (v) sessionStorage.setItem(STORAGE_KEY_API, v);
  else sessionStorage.removeItem(STORAGE_KEY_API);
  updateAuthUI();
}

function getBaseUrl() {
  const u = sessionStorage.getItem(STORAGE_KEY_BASE) || '';
  if (u) return u;
  if (typeof location !== 'undefined' && location.origin && (location.origin.startsWith('http://') || location.origin.startsWith('https://')))
    return location.origin;
  return 'http://localhost:8000';
}

function setBaseUrl(v) {
  sessionStorage.setItem(STORAGE_KEY_BASE, v || '');
}

function getGroups() {
  try {
    const j = sessionStorage.getItem(STORAGE_KEY_GROUPS);
    return j ? JSON.parse(j) : [];
  } catch (_) {
    return [];
  }
}

function setGroups(groups) {
  sessionStorage.setItem(STORAGE_KEY_GROUPS, JSON.stringify(groups));
  renderGroupsList();
  renderActionsByGroup();
}

function getActionsByGroup() {
  try {
    const j = sessionStorage.getItem(STORAGE_KEY_ACTIONS);
    return j ? JSON.parse(j) : {};
  } catch (_) {
    return {};
  }
}

function setActionsByGroup(obj) {
  sessionStorage.setItem(STORAGE_KEY_ACTIONS, JSON.stringify(obj));
  renderActionsByGroup();
}

function addActionToGroupLocal(groupId, cid) {
  const key = String(groupId);
  const obj = getActionsByGroup();
  if (!obj[key]) obj[key] = [];
  if (!obj[key].includes(cid)) obj[key].push(cid);
  setActionsByGroup(obj);
}

function updateAuthUI() {
  const hasKey = !!getApiKey();
  document.body.classList.toggle('has-api-key', hasKey);
  if (hasKey) {
    const links = document.querySelectorAll('.nav a[data-page="login"]');
    links.forEach(a => a.classList.remove('active'));
    const first = document.querySelector('.nav a.requires-auth');
    if (first) {
      first.classList.add('active');
      showPage(first.getAttribute('data-page'));
    }
  } else {
    showPage('login');
  }
}

function showPage(id) {
  document.querySelectorAll('.page').forEach(p => p.classList.remove('active'));
  document.querySelectorAll('.nav a').forEach(a => a.classList.remove('active'));
  const page = document.getElementById('page-' + id);
  const link = document.querySelector('.nav a[data-page="' + id + '"]');
  if (page) page.classList.add('active');
  if (link) link.classList.add('active');
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

// ----- Page 1: Login / Create account -----
function initLogin() {
  const apiKeyInput = document.getElementById('login-api-key');
  const baseUrlInput = document.getElementById('api-base-url');
  if (getApiKey()) apiKeyInput.value = '';
  baseUrlInput.value = getBaseUrl();

  document.getElementById('btn-login').addEventListener('click', () => {
    const key = (apiKeyInput.value || '').trim();
    const base = (baseUrlInput.value || '').trim();
    if (!key) {
      showStatus('login-status', 'Enter an API key.', 'error');
      return;
    }
    setBaseUrl(base || location.origin);
    setApiKey(key);
    showStatus('login-status', 'Logged in. Use the nav to open Wallets, Groups, or IPFS Actions.', 'success');
  });

  document.getElementById('btn-create-account').addEventListener('click', async () => {
    const name = document.getElementById('new-account-name').value.trim();
    const desc = document.getElementById('new-account-desc').value.trim();
    const base = (document.getElementById('api-base-url').value || '').trim() || location.origin;
    setBaseUrl(base);
    hideStatus('login-status');
    if (!name) {
      showStatus('login-status', 'Enter an account name.', 'error');
      return;
    }
    const btn = document.getElementById('btn-create-account');
    btn.disabled = true;
    try {
      const client = await getClient();
      const res = await client.newAccount({ accountName: name, accountDescription: desc });
      setApiKey(res.api_key);
      showStatus('login-status', 'Account created. API key and wallet address are stored; you are now logged in.', 'success');
      document.getElementById('new-account-name').value = '';
      document.getElementById('new-account-desc').value = '';
    } catch (e) {
      showStatus('login-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
    } finally {
      btn.disabled = false;
    }
  });
}

// ----- Page 2: Wallets -----
function initWallets() {
  document.getElementById('btn-create-wallet').addEventListener('click', async () => {
    const apiKey = getApiKey();
    if (!apiKey) return;
    hideStatus('wallets-status');
    const resultEl = document.getElementById('create-wallet-result');
    resultEl.style.display = 'none';
    const btn = document.getElementById('btn-create-wallet');
    btn.disabled = true;
    try {
      const client = await getClient();
      const res = await client.createWallet(apiKey);
      resultEl.textContent = 'Wallet address: ' + (res.wallet_address || '');
      resultEl.style.display = 'block';
      showStatus('wallets-status', 'Wallet created. Add it to a group from "Add existing PKP to group" if you have a PKP.', 'success');
    } catch (e) {
      showStatus('wallets-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
    } finally {
      btn.disabled = false;
    }
  });

  document.getElementById('btn-add-pkp').addEventListener('click', async () => {
    const apiKey = getApiKey();
    const groupId = document.getElementById('add-pkp-group').value.trim();
    const pkp = document.getElementById('add-pkp-public-key').value.trim();
    if (!apiKey || !groupId || !pkp) {
      showStatus('wallets-status', 'Fill API key (you are logged in), Group ID, and PKP public key.', 'error');
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
}

// ----- Page 3: Groups -----
function renderGroupsList() {
  const groups = getGroups();
  const list = document.getElementById('groups-list');
  const empty = document.getElementById('groups-empty');
  list.innerHTML = '';
  if (groups.length === 0) {
    empty.style.display = 'block';
    return;
  }
  empty.style.display = 'none';
  groups.forEach((g, i) => {
    const li = document.createElement('li');
    li.className = 'list-item';
    li.innerHTML = '<span><strong>' + escapeHtml(g.name) + '</strong> (ID: ' + g.id + ')</span><span class="mono">' + escapeHtml(g.description || '') + '</span>';
    list.appendChild(li);
  });
}

function initGroups() {
  document.getElementById('btn-add-group').addEventListener('click', async () => {
    const apiKey = getApiKey();
    const name = document.getElementById('new-group-name').value.trim();
    const desc = document.getElementById('new-group-desc').value.trim();
    if (!apiKey) return;
    if (!name) {
      showStatus('groups-status', 'Enter a group name.', 'error');
      return;
    }
    hideStatus('groups-status');
    const btn = document.getElementById('btn-add-group');
    btn.disabled = true;
    try {
      const client = await getClient();
      await client.addGroup({
        apiKey,
        groupName: name,
        groupDescription: desc,
        permittedActions: [],
        pkps: [],
      });
      const groups = getGroups();
      const nextId = groups.length;
      groups.push({ id: nextId, name, description: desc });
      setGroups(groups);
      showStatus('groups-status', 'Group created with ID ' + nextId + '.', 'success');
      document.getElementById('new-group-name').value = '';
      document.getElementById('new-group-desc').value = '';
    } catch (e) {
      showStatus('groups-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
    } finally {
      btn.disabled = false;
    }
  });
  renderGroupsList();
}

// ----- Page 4: IPFS Actions -----
function renderActionsByGroup() {
  const groups = getGroups();
  const actions = getActionsByGroup();
  const container = document.getElementById('actions-by-group');
  if (groups.length === 0) {
    container.innerHTML = '<p class="page-desc">Create groups first, then add IPFS CIDs here.</p>';
    return;
  }
  let html = '';
  groups.forEach((g) => {
    const cids = actions[String(g.id)] || [];
    html += '<div style="margin-bottom: 1.5rem;"><div class="card-title">' + escapeHtml(g.name) + ' (ID: ' + g.id + ')</div>';
    if (cids.length === 0) {
      html += '<p class="page-desc">No IPFS CIDs added yet.</p>';
    } else {
      html += '<ul class="list">';
      cids.forEach((cid) => {
        html += '<li class="list-item"><span class="mono">' + escapeHtml(cid) + '</span></li>';
      });
      html += '</ul>';
    }
    html += '</div>';
  });
  container.innerHTML = html || '<p class="page-desc">No groups or CIDs.</p>';
}

function initActions() {
  document.getElementById('btn-add-action').addEventListener('click', async () => {
    const apiKey = getApiKey();
    const groupId = document.getElementById('action-group-id').value.trim();
    const cid = document.getElementById('action-ipfs-cid').value.trim();
    if (!apiKey || !groupId || !cid) {
      showStatus('actions-status', 'Fill Group ID and IPFS CID.', 'error');
      return;
    }
    hideStatus('actions-status');
    const btn = document.getElementById('btn-add-action');
    btn.disabled = true;
    try {
      const client = await getClient();
      await client.addActionToGroup({ apiKey, groupId, actionIpfsCid: cid });
      addActionToGroupLocal(groupId, cid);
      showStatus('actions-status', 'IPFS action added to group.', 'success');
      document.getElementById('action-ipfs-cid').value = '';
    } catch (e) {
      showStatus('actions-status', 'Error: ' + (e && e.message ? e.message : String(e)), 'error');
    } finally {
      btn.disabled = false;
    }
  });
  renderActionsByGroup();
}

function escapeHtml(s) {
  if (s == null) return '';
  const div = document.createElement('div');
  div.textContent = s;
  return div.innerHTML;
}

// ----- Nav & routing -----
function initNav() {
  document.querySelectorAll('.nav a').forEach((a) => {
    a.addEventListener('click', (e) => {
      e.preventDefault();
      const page = a.getAttribute('data-page');
      if (page) showPage(page);
    });
  });
  window.addEventListener('hashchange', () => {
    const hash = (window.location.hash || '').replace(/^#/, '') || 'login';
    const link = document.querySelector('.nav a[data-page="' + hash + '"]');
    if (link) {
      link.classList.add('active');
      showPage(hash);
    }
  });
  const hash = (window.location.hash || '').replace(/^#/, '') || 'login';
  const link = document.querySelector('.nav a[data-page="' + hash + '"]');
  if (link) showPage(hash);
}

// ----- Logout & theme -----
function initHeader() {
  document.getElementById('logout-btn').addEventListener('click', () => {
    setApiKey('');
    setGroups([]);
    setActionsByGroup({});
    showPage('login');
  });

  document.getElementById('theme-toggle').addEventListener('click', () => {
    const next = getTheme() === 'dark' ? 'light' : 'dark';
    setTheme(next);
  });
}

// ----- Init -----
function init() {
  setTheme(getTheme());
  updateAuthUI();
  initLogin();
  initWallets();
  initGroups();
  initActions();
  initNav();
  initHeader();
}

init();
