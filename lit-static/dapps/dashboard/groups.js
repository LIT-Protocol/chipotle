/**
 * Groups — CRUD, table rendering, multi-select builders.
 */

import { getEffectiveApiKey, isAuthenticated, getClient, getGroupsStore, setGroupsStore, getWalletsStore, getActionsStore, setStat, updateStatCards, LIST_PAGE_SIZE } from './auth.js';
import { escapeHtml, showStatus, hideStatus, showActionProgress, closeActionProgress, openModal, closeModal, confirmDelete, formatError, logError, ICON_PENCIL, ICON_TRASH } from './ui-utils.js';

// ----- Multi-select builders -----

export function buildMultiSelect(id, items, getValue, getLabel, placeholder, disabled) {
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

export function buildGroupMultiSelect(id, disabled) {
  const items = [{ id: '0', name: 'All Groups' }, ...getGroupsStore()];
  return buildMultiSelect(id, items, (g) => g.id, (g) => g.name || String(g.id), 'Select groups\u2026', disabled);
}

export function buildWalletMultiSelect(id, disabled) {
  const items = [{ wallet_address: '0x0000000000000000000000000000000000000000', name: 'All' }, ...getWalletsStore()];
  return buildMultiSelect(id, items, (w) => w.wallet_address, (w) => w.name || w.wallet_address, 'Select PKPs\u2026', disabled);
}

export function buildActionsMultiSelect(id, disabled) {
  const items = [{ id: '0', name: 'All' }, ...getActionsStore()];
  return buildMultiSelect(id, items, (a) => a.id, (a) => a.name || a.id, 'Select actions\u2026', disabled);
}

export function updateMultiSelectSummary(id) {
  const wrap = document.getElementById(id);
  if (!wrap) return;
  const summaryEl = wrap.querySelector('.ms-summary');
  const checked = [...wrap.querySelectorAll('input[type="checkbox"]:checked')];
  const placeholder = wrap.dataset.placeholder || 'Select\u2026';
  summaryEl.textContent = checked.length === 0
    ? placeholder
    : checked.map((c) => c.nextElementSibling.textContent).join(', ');
}

export function attachGroupMultiSelectLogic(id) {
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

export function getSelectedGroupIds(id) {
  const wrap = document.getElementById(id);
  if (!wrap) return [];
  return [...wrap.querySelectorAll('input[type="checkbox"]:checked')].map((c) => Number(c.value));
}

export function getSelectedStringValues(id) {
  const wrap = document.getElementById(id);
  if (!wrap) return [];
  return [...wrap.querySelectorAll('input[type="checkbox"]:checked')].map((c) => c.value);
}

function selectAllInMultiSelect(id) {
  const wrap = document.getElementById(id);
  if (!wrap) return;
  const allCb = wrap.querySelector('input[value="0"]') || wrap.querySelector('input[value="0x0000000000000000000000000000000000000000"]');
  wrap.querySelectorAll('input[type="checkbox"]').forEach((cb) => { cb.checked = cb === allCb; });
  updateMultiSelectSummary(id);
}

// ----- Table rendering -----

export function renderGroupsTable(items) {
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

// ----- Load -----

export async function loadGroups() {
  const apiKey = getEffectiveApiKey();
  if (!isAuthenticated()) return;
  hideStatus('groups-status');
  const btn = document.getElementById('btn-load-groups');
  if (btn) btn.disabled = true;
  try {
    const client = await getClient();
    const items = await client.listGroups({ apiKey, pageNumber: '0', pageSize: LIST_PAGE_SIZE });
    setGroupsStore(items);
    renderGroupsTable(items);
    setStat('groups', items.length);
    updateStatCards();
    return items;
  } catch (e) {
    logError('loadGroups', e);
    showStatus('groups-status', 'Error: ' + formatError(e), 'error');
    return [];
  } finally {
    if (btn) btn.disabled = false;
  }
}

// ----- CRUD -----

async function confirmAndRemoveGroup(item) {
  const label = item.name || item.id || '—';
  const msg = 'Delete group "' + label + '"? This cannot be undone.';
  const confirmed = await confirmDelete(msg);
  if (!confirmed) return;
  const apiKey = getEffectiveApiKey();
  if (!isAuthenticated()) return;
  hideStatus('groups-status');
  try {
    showActionProgress('Deleting group', `Deleting group "${escapeHtml(label)}".`);
    const client = await getClient();
    await client.removeGroup({ apiKey, groupId: item.id });
    setGroupsStore(getGroupsStore().filter((g) => g.id !== item.id));
    renderGroupsTable(getGroupsStore());
    updateStatCards();
    showStatus('groups-status', 'Group deleted.', 'success');
  } catch (e) {
    logError('removeGroup', e);
    showStatus('groups-status', 'Error: ' + formatError(e), 'error');
  } finally {
    closeActionProgress();
  }
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
    const apiKey = getEffectiveApiKey();
    if (!isAuthenticated()) {
      showStatus('groups-status', 'Log in first.', 'error');
      return;
    }
    saveBtn.disabled = true;
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
      logError('saveGroup', e);
      showStatus('groups-status', 'Error: ' + formatError(e), 'error');
    } finally {
      closeActionProgress();
    }
  });
}

function openAddGroupModal() { openGroupModal(); }
function openEditGroupModal(item) { openGroupModal(item); }

// ----- Init -----

export function initGroups() {
  document.getElementById('btn-load-groups')?.addEventListener('click', () => loadGroups());
  document.getElementById('btn-add-group')?.addEventListener('click', () => openAddGroupModal());
}
