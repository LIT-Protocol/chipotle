/**
 * Wallets — table rendering, CRUD.
 */

import { getEffectiveApiKey, getClient, getWalletsStore, setWalletsStore, setStat, updateStatCards, LIST_PAGE_SIZE } from './auth.js';
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
      '<td class="mono cell-address"></td>';
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
    tbody.appendChild(tr);
  });
}

// ----- Load -----

export async function loadWallets() {
  const apiKey = getEffectiveApiKey();
  if (!apiKey) return;
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
    if (!apiKey) return;
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

// ----- Init -----

export function initWallets() {
  document.getElementById('btn-load-wallets')?.addEventListener('click', () => loadWallets());
  document.getElementById('btn-add-wallet')?.addEventListener('click', () => openAddWalletModal());
}
