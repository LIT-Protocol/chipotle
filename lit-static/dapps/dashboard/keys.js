/**
 * Usage API Keys — table rendering, CRUD, permission summary.
 */

import { getApiKey, getClient, getUsageKeysStore, setUsageKeysStore, getGroupsStore, setStat, updateStatCards, maskApiKey, LIST_PAGE_SIZE } from './auth.js';
import { escapeHtml, showStatus, hideStatus, showActionProgress, closeActionProgress, openModal, closeModal, confirmDelete, copyToClipboard, formatError, logError, ICON_PENCIL, ICON_TRASH, ICON_COPY } from './ui-utils.js';
import { buildGroupMultiSelect, attachGroupMultiSelectLogic, updateMultiSelectSummary, getSelectedGroupIds } from './groups.js';

// ----- Helpers -----

export function normalizeUsageKeyItem(item) {
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

export function renderPermissionSummary(item) {
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

// ----- Table rendering -----

export function renderUsageKeysTable() {
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
    copyBtn.addEventListener('click', () => {
      if (!fullKey) return;
      copyToClipboard(fullKey, copyBtn, 'title');
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

// ----- Load -----

export async function loadUsageKeys() {
  const apiKey = getApiKey();
  if (!apiKey) return [];
  hideStatus('overview-status-usage-keys');
  const btn = document.getElementById('btn-load-usage-keys');
  if (btn) btn.disabled = true;
  try {
    const client = await getClient();
    const items = await client.listApiKeys({ apiKey, pageNumber: '0', pageSize: LIST_PAGE_SIZE });
    const mapped = items.map((it) => ({
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
    setUsageKeysStore(mapped);
    setStat('usageKeys', mapped.length);
    renderUsageKeysTable();
    updateStatCards();
    return mapped;
  } catch (e) {
    logError('loadUsageKeys', e);
    showStatus('overview-status-usage-keys', 'Error: ' + formatError(e), 'error');
    return [];
  } finally {
    if (btn) btn.disabled = false;
  }
}

// ----- CRUD -----

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
    const saveBtn = document.getElementById('modal-save-btn');
    if (saveBtn) saveBtn.disabled = true;
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
        logError('updateUsageKey', e);
        showStatus('overview-status-usage-keys', 'Error: ' + formatError(e), 'error');
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
          setStat('usageKeys', getUsageKeysStore().length);
          renderUsageKeysTable();
          updateStatCards();
        }
        showStatus('overview-status-usage-keys', 'Usage API key added. Copy and store your key now (shown once): ' + usageKey, 'success');
      } catch (e) {
        logError('addUsageKey', e);
        showStatus('overview-status-usage-keys', 'Error: ' + formatError(e), 'error');
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
    logError('removeUsageKey', e);
    showStatus('overview-status-usage-keys', 'Error: ' + formatError(e), 'error');
  } finally {
    closeActionProgress();
  }
}

// ----- Init -----

export function initKeys() {
  renderUsageKeysTable();
  updateStatCards();
  document.getElementById('btn-load-usage-keys')?.addEventListener('click', () => loadUsageKeys());
  document.getElementById('btn-add-usage-key')?.addEventListener('click', () => openUsageKeyModal());
}
