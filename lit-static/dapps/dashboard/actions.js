/**
 * IPFS Actions — table rendering, CRUD.
 */

import { getEffectiveApiKey, isAuthenticated, getClient, getActionsStore, setActionsStore, setStat, updateStatCards, LIST_PAGE_SIZE } from './auth.js';
import { escapeHtml, showStatus, hideStatus, showActionProgress, closeActionProgress, openModal, closeModal, confirmDelete, formatError, logError, ICON_PENCIL, ICON_TRASH } from './ui-utils.js';

// ----- Table rendering -----

export function renderActionsTable(items) {
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

// ----- Load -----

export async function loadActions() {
  const apiKey = getEffectiveApiKey();
  if (!isAuthenticated()) return;
  hideStatus('actions-status');
  const btn = document.getElementById('btn-load-actions');
  if (btn) btn.disabled = true;
  try {
    const client = await getClient();
    const items = await client.listActions({ apiKey, pageNumber: '0', pageSize: LIST_PAGE_SIZE });
    setActionsStore(items);
    renderActionsTable(items);
    setStat('actions', items.length);
    updateStatCards();
    return items;
  } catch (e) {
    logError('loadActions', e);
    showStatus('actions-status', 'Error: ' + formatError(e), 'error');
    return [];
  } finally {
    if (btn) btn.disabled = false;
  }
}

// ----- CRUD -----

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
    const apiKey = getEffectiveApiKey();
    if (!apiKey || !cid) {
      showStatus('actions-status', 'Fill in the IPFS CID.', 'error');
      return;
    }
    const addBtn = document.getElementById('modal-add-btn');
    if (addBtn) addBtn.disabled = true;
    closeModal();
    hideStatus('actions-status');
    try {
      showActionProgress('Adding action', `Adding action CID "${cid}".`);
      const client = await getClient();
      await client.addAction({ apiKey, actionIpfsCid: cid, name: name || '', description: desc || '' });
      await loadActions();
      showStatus('actions-status', 'Action added.', 'success');
    } catch (e) {
      logError('addAction', e);
      showStatus('actions-status', 'Error: ' + formatError(e), 'error');
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
    const apiKey = getEffectiveApiKey();
    if (!apiKey || !cid || !name) {
      showStatus('actions-status', 'Fill Name.', 'error');
      return;
    }
    const saveBtn = document.getElementById('modal-save-btn');
    if (saveBtn) saveBtn.disabled = true;
    closeModal();
    hideStatus('actions-status');
    try {
      showActionProgress('Updating action', `Updating action metadata for CID "${cid}".`);
      const client = await getClient();
      await client.updateActionMetadata({ apiKey, hashedCid: cid, name, description: desc });
      await loadActions();
      showStatus('actions-status', 'Action updated.', 'success');
    } catch (e) {
      logError('updateAction', e);
      showStatus('actions-status', 'Error: ' + formatError(e), 'error');
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
  const apiKey = getEffectiveApiKey();
  if (!isAuthenticated()) return;
  hideStatus('actions-status');
  try {
    showActionProgress('Deleting action', `Deleting action CID "${cid}".`);
    const client = await getClient();
    await client.deleteAction({ apiKey, hashedCid: cid });
    await loadActions();
    showStatus('actions-status', 'Action deleted.', 'success');
  } catch (e) {
    logError('removeAction', e);
    showStatus('actions-status', 'Error: ' + formatError(e), 'error');
  } finally {
    closeActionProgress();
  }
}

// ----- Init -----

export function initActions() {
  document.getElementById('btn-load-actions')?.addEventListener('click', () => loadActions());
  document.getElementById('btn-add-action')?.addEventListener('click', () => openAddActionModal());
}
