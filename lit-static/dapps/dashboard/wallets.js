/**
 * Wallets — table rendering, CRUD.
 */

import { getEffectiveApiKey, isAuthenticated, getClient, getWalletsStore, setWalletsStore, setStat, updateStatCards, LIST_PAGE_SIZE } from './auth.js';
import { escapeHtml, showStatus, hideStatus, showActionProgress, closeActionProgress, openModal, closeModal, copyToClipboard, formatError, logError } from './ui-utils.js';

// ----- Table rendering -----

export function renderWalletsTable(items) {
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
      '<td class="mono cell-address"></td>' +
      '<td class="col-actions cell-actions"></td>';
    const addressCell = tr.querySelector('.cell-address');
    const addressCopyBtn = document.createElement('button');
    addressCopyBtn.type = 'button';
    addressCopyBtn.className = 'btn-copy-key';
    addressCopyBtn.textContent = address;
    addressCopyBtn.title = 'Copy full address';
    addressCopyBtn.addEventListener('click', () => {
      copyToClipboard(address, addressCopyBtn);
    });
    addressCell.appendChild(addressCopyBtn);

    const actionsCell = tr.querySelector('.cell-actions');
    if (address) {
      const deleteBtn = document.createElement('button');
      deleteBtn.type = 'button';
      deleteBtn.className = 'btn btn-sm btn-danger';
      deleteBtn.textContent = 'Delete';
      deleteBtn.title = 'Permanently delete this wallet';
      deleteBtn.addEventListener('click', () => openDeleteWalletModal(address, description));
      actionsCell.appendChild(deleteBtn);
    }
    tbody.appendChild(tr);
  });
}

// ----- Load -----

export async function loadWallets() {
  const apiKey = getEffectiveApiKey();
  if (!isAuthenticated()) return;
  hideStatus('wallets-status');
  const btn = document.getElementById('btn-load-wallets');
  if (btn) btn.disabled = true;
  try {
    const client = await getClient();
    const items = await client.listWallets({ apiKey, pageNumber: '0', pageSize: LIST_PAGE_SIZE });
    setWalletsStore(items);
    renderWalletsTable(items);
    setStat('wallets', items.length);
    updateStatCards();
    return items;
  } catch (e) {
    logError('loadWallets', e);
    showStatus('wallets-status', 'Error: ' + formatError(e), 'error');
    return [];
  } finally {
    if (btn) btn.disabled = false;
  }
}

// ----- CRUD -----

function openAddWalletModal() {
  const body =
    '<p class="form-hint">Creates a new wallet and registers it for this account. The wallet address will be shown after creation.</p>';
  const footer =
    '<button type="button" class="btn btn-outline" id="modal-cancel-btn">Cancel</button>' +
    '<button type="button" class="btn btn-primary" id="modal-add-btn">Add</button>';
  openModal('Create wallet', body, footer);
  document.getElementById('modal-cancel-btn').addEventListener('click', closeModal);
  document.getElementById('modal-add-btn').addEventListener('click', async () => {
    const apiKey = getEffectiveApiKey();
    if (!isAuthenticated()) return;
    const addBtn = document.getElementById('modal-add-btn');
    if (addBtn) addBtn.disabled = true;
    closeModal();
    hideStatus('wallets-status');
    try {
      showActionProgress('Creating wallet', 'Creating and registering a new wallet for this account.');
      const client = await getClient();
      const res = await client.createWallet({ apiKey });
      await loadWallets();
      showStatus('wallets-status', 'Wallet created: ' + (res.wallet_address || ''), 'success');
    } catch (e) {
      logError('createWallet', e);
      showStatus('wallets-status', 'Error: ' + formatError(e), 'error');
    } finally {
      closeActionProgress();
    }
  });
}

// ----- Delete (permanent / irreversible) -----

function openDeleteWalletModal(address, description) {
  const label = description ? (escapeHtml(description) + ' (' + escapeHtml(address) + ')') : escapeHtml(address);
  const body =
    '<div class="danger-panel">' +
      '<p class="danger-lead"><strong>This permanently and irreversibly deletes this wallet (PKP).</strong></p>' +
      '<ul class="danger-list">' +
        '<li>The wallet’s on-chain derivation path is <strong>wiped</strong>. Keys are derived on demand from that path and stored nowhere else — once it is gone the private key can <strong>never be re-derived</strong>.</li>' +
        '<li><strong>Anything encrypted or secured by this wallet becomes permanently unretrievable.</strong> This includes any data whose access control depends on this PKP.</li>' +
        '<li>The wallet is removed from <strong>every group</strong> it belongs to.</li>' +
        '<li>Any on-chain assets held by this wallet address on other chains are <strong>not</strong> transferred and become inaccessible through this account.</li>' +
        '<li><strong>There is no undo. Recreating a wallet produces a different key.</strong></li>' +
      '</ul>' +
      '<p class="danger-target">Deleting: <span class="mono">' + label + '</span></p>' +
      '<div class="form-group">' +
        '<label for="delete-wallet-confirm-input">Type the wallet address to confirm:</label>' +
        '<p class="form-hint mono danger-confirm-target">' + escapeHtml(address) + '</p>' +
        '<input type="text" id="delete-wallet-confirm-input" class="input" autocomplete="off" autocapitalize="off" spellcheck="false" placeholder="0x…" style="font-family:ui-monospace,\'JetBrains Mono\',monospace;" />' +
      '</div>' +
    '</div>';
  const footer =
    '<button type="button" class="btn btn-outline" id="modal-cancel-btn">Cancel</button>' +
    '<button type="button" class="btn btn-danger" id="modal-delete-btn" disabled>Permanently delete</button>';
  openModal('Delete wallet permanently', body, footer);

  const input = document.getElementById('delete-wallet-confirm-input');
  const deleteBtn = document.getElementById('modal-delete-btn');
  // Enable the delete button only once the typed value matches the address exactly
  // (case-insensitive, since hex addresses are case-insensitive).
  const target = address.trim().toLowerCase();
  input.addEventListener('input', () => {
    deleteBtn.disabled = input.value.trim().toLowerCase() !== target;
  });

  document.getElementById('modal-cancel-btn').addEventListener('click', closeModal);
  deleteBtn.addEventListener('click', async () => {
    if (input.value.trim().toLowerCase() !== target) return;
    const apiKey = getEffectiveApiKey();
    if (!isAuthenticated()) return;
    deleteBtn.disabled = true;
    closeModal();
    hideStatus('wallets-status');
    try {
      showActionProgress('Deleting wallet', 'Permanently removing this wallet from the account. This cannot be undone.');
      const client = await getClient();
      await client.deleteWallet({ apiKey, walletAddress: address });
      await loadWallets();
      showStatus('wallets-status', 'Wallet permanently deleted: ' + address, 'success');
    } catch (e) {
      logError('deleteWallet', e);
      showStatus('wallets-status', 'Error: ' + formatError(e), 'error');
    } finally {
      closeActionProgress();
    }
  });
}

// ----- Init -----

export function initWallets() {
  document.getElementById('btn-load-wallets')?.addEventListener('click', () => loadWallets());
  document.getElementById('btn-add-wallet')?.addEventListener('click', () => openAddWalletModal());
}
