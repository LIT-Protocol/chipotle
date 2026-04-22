/**
 * Sovereign-mode transaction preview + confirm modal.
 *
 * Decision from CPL-267 plan-eng-review (Issue 5): "required preview+confirm
 * modal before every wallet pop, decoded calldata + function name + account
 * impact." This module renders that modal and returns a Promise that resolves
 * true on confirm / false on cancel (wired as runContractWrite.onPreview).
 *
 * Required DOM (from index.html):
 *   #modal-overlay, #modal-title, #modal-body, #modal-footer, #modal-close-btn
 *   (existing generic modal, reused via openModal/closeModal)
 */

import { openModal, closeModal, escapeHtml } from './ui-utils.js';
import { ACCOUNT_CONFIG_FULL_ABI } from '../../account_config_full_abi.js';

/**
 * Labels for each admin-write contract method, surfaced in the preview title.
 * Fallback: prettify camelCase if the method is unknown.
 */
const METHOD_LABELS = {
  addGroup: 'Create group',
  removeGroup: 'Delete group',
  updateGroup: 'Update group (full)',
  updateGroupMetadata: 'Update group metadata',
  addAction: 'Register action',
  removeAction: 'Delete action',
  addActionToGroup: 'Grant action to group',
  removeActionFromGroup: 'Revoke action from group',
  updateActionMetadata: 'Update action metadata',
  addPkpToGroup: 'Grant PKP to group',
  removePkpFromGroup: 'Revoke PKP from group',
  setUsageApiKey: 'Create/update usage API key',
  removeUsageApiKey: 'Delete usage API key',
  updateUsageApiKeyMetadata: 'Update usage API key metadata',
  newAccount: 'Create account',
  registerWalletDerivation: 'Register wallet',
};

/**
 * Friendly labels for the "apiKeyHash" / "accountApiKeyHash" input across
 * methods — this is always the account identifier.
 */
const ARG_LABELS = {
  apiKeyHash: 'Account (apiKeyHash)',
  accountApiKeyHash: 'Account (apiKeyHash)',
  usageApiKeyHash: 'Usage key hash',
  actionHash: 'Action hash',
  cidHashes: 'Action CID hashes',
  pkpIds: 'PKP addresses',
  pkpId: 'PKP address',
  groupId: 'Group ID',
  manageIPFSIdsInGroups: 'Can manage IPFS in groups',
  addPkpToGroups: 'Can add PKPs to groups',
  removePkpFromGroups: 'Can remove PKPs from groups',
  executeInGroups: 'Can execute in groups',
  createGroups: 'Can create groups',
  deleteGroups: 'Can delete groups',
  createPKPs: 'Can create PKPs',
  expiration: 'Expires (unix s, 0=never)',
  balance: 'Starting balance',
  creatorWalletAddress: 'Creator wallet',
  derivationPath: 'Derivation path',
};

function prettifyMethod(name) {
  if (METHOD_LABELS[name]) return METHOD_LABELS[name];
  return name.replace(/([A-Z])/g, ' $1').replace(/^./, (c) => c.toUpperCase());
}

function prettifyArg(name) {
  if (ARG_LABELS[name]) return ARG_LABELS[name];
  return name;
}

/**
 * Best-effort human-readable rendering for an argument value.
 */
function formatArg(value) {
  if (value == null) return '<em>(empty)</em>';
  if (typeof value === 'boolean') return value ? 'yes' : 'no';
  if (typeof value === 'bigint') return value.toString();
  if (Array.isArray(value)) {
    if (value.length === 0) return '<em>(none)</em>';
    return '<code>[' + value.map((v) => escapeHtml(String(v))).join(', ') + ']</code>';
  }
  if (typeof value === 'string') {
    if (value.startsWith('0x') && value.length === 66) {
      // 32-byte hash: show first+last
      return '<code>' + escapeHtml(value.slice(0, 10)) + '…' + escapeHtml(value.slice(-6)) + '</code>';
    }
    if (value.startsWith('0x') && value.length === 42) {
      // 20-byte address
      return '<code>' + escapeHtml(value) + '</code>';
    }
    return escapeHtml(value);
  }
  return escapeHtml(String(value));
}

function findMethodInputs(methodName) {
  const entry = ACCOUNT_CONFIG_FULL_ABI.find(
    (i) => i.type === 'function' && i.name === methodName,
  );
  return entry?.inputs ?? [];
}

/**
 * Show the preview modal. Resolves true on confirm, false on cancel.
 *
 * @param {string} methodName - contract function name (e.g. 'addGroup')
 * @param {any[]} args - positional args in contract-ABI order
 * @param {Object} [meta]
 * @param {string} [meta.chainLabel] - "Base", "Base Sepolia", etc.
 * @param {string} [meta.signerAddress] - 0x-prefixed signer
 * @param {string} [meta.contractAddress] - target contract
 * @returns {Promise<boolean>}
 */
export function showTxPreview(methodName, args, meta = {}) {
  return new Promise((resolve) => {
    const inputs = findMethodInputs(methodName);
    const rows = inputs.map((input, idx) => {
      const val = args[idx];
      const label = prettifyArg(input.name);
      return `<tr>
        <th style="text-align:left;padding:4px 8px;vertical-align:top;font-weight:600">${escapeHtml(label)}</th>
        <td style="padding:4px 8px;word-break:break-all">${formatArg(val)}</td>
      </tr>`;
    }).join('');

    const metaRows = [];
    if (meta.signerAddress) {
      metaRows.push(`<tr><th style="text-align:left;padding:4px 8px">Signing wallet</th><td style="padding:4px 8px"><code>${escapeHtml(meta.signerAddress)}</code></td></tr>`);
    }
    if (meta.chainLabel || meta.chainId) {
      metaRows.push(`<tr><th style="text-align:left;padding:4px 8px">Network</th><td style="padding:4px 8px">${escapeHtml(meta.chainLabel || `chain ${meta.chainId}`)}</td></tr>`);
    }
    if (meta.contractAddress) {
      metaRows.push(`<tr><th style="text-align:left;padding:4px 8px">AccountConfig</th><td style="padding:4px 8px"><code>${escapeHtml(meta.contractAddress)}</code></td></tr>`);
    }

    const bodyHTML = `
      <div style="display:grid;gap:12px">
        <div>
          <div style="font-size:13px;color:var(--muted,#888);margin-bottom:4px">Contract method</div>
          <div style="font-weight:600"><code>${escapeHtml(methodName)}</code></div>
        </div>
        ${metaRows.length ? `<table style="width:100%;font-size:13px;border-collapse:collapse">${metaRows.join('')}</table>` : ''}
        <div>
          <div style="font-size:13px;color:var(--muted,#888);margin-bottom:4px">Arguments</div>
          <table style="width:100%;font-size:13px;border-collapse:collapse;background:var(--bg-soft,#fafafa);border-radius:6px">
            ${rows || '<tr><td style="padding:8px"><em>(no arguments)</em></td></tr>'}
          </table>
        </div>
        <div style="font-size:12px;color:var(--muted,#888)">
          After you confirm, your wallet will pop up to sign. You can still reject there.
        </div>
      </div>
    `;

    const footerHTML = `
      <button type="button" class="btn btn-ghost" id="tx-preview-cancel">Cancel</button>
      <button type="button" class="btn btn-primary" id="tx-preview-confirm">Confirm &amp; sign in wallet</button>
    `;

    openModal(prettifyMethod(methodName), bodyHTML, footerHTML);

    // The generic modal (initModalClose) already wires #modal-close-btn and
    // overlay-background clicks to closeModal(). Without our own handlers on
    // those paths, closing via the X or backdrop would hide the modal but
    // leave this Promise unresolved, hanging the write flow in PREVIEWING.
    // Resolve false on any close path; closeModal() is idempotent.
    let settled = false;
    const overlay = document.getElementById('modal-overlay');
    const closeBtn = document.getElementById('modal-close-btn');
    const onOverlayClick = (e) => { if (e.target.id === 'modal-overlay') settle(false); };
    const onCloseClick = () => settle(false);
    const settle = (result) => {
      if (settled) return;
      settled = true;
      overlay?.removeEventListener('click', onOverlayClick);
      closeBtn?.removeEventListener('click', onCloseClick);
      closeModal();
      resolve(result);
    };
    overlay?.addEventListener('click', onOverlayClick);
    closeBtn?.addEventListener('click', onCloseClick);
    document.getElementById('tx-preview-cancel')?.addEventListener('click', () => settle(false), { once: true });
    document.getElementById('tx-preview-confirm')?.addEventListener('click', () => settle(true), { once: true });
  });
}
