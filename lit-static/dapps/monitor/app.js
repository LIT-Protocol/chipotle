/**
 * Lit Node Monitor — Payer Safety Console
 * Vanilla JS + ethers.js v6.13.0
 */

/* ═══ ABIs ═══════════════════════════════════════════════════════════════════ */

const ACCOUNT_CONFIG_VIEW_ABI = [
  {
    inputs: [],
    name: 'nodeConfigurationValues',
    outputs: [
      {
        components: [
          { internalType: 'string', name: 'key',   type: 'string' },
          { internalType: 'string', name: 'value', type: 'string' },
        ],
        internalType: 'struct ViewsFacet.KeyValueReturn[]',
        name: '',
        type: 'tuple[]',
      },
    ],
    stateMutability: 'view',
    type: 'function',
  },
  {
    inputs: [],
    name: 'pricingOperator',
    outputs: [{ internalType: 'address', name: '', type: 'address' }],
    stateMutability: 'view',
    type: 'function',
  },
  {
    inputs: [],
    name: 'adminApiPayerAccount',
    outputs: [{ internalType: 'address', name: '', type: 'address' }],
    stateMutability: 'view',
    type: 'function',
  },
  {
    // apiPayerCount() returns api_payers.length() — the actual current payer count.
    inputs: [],
    name: 'apiPayerCount',
    outputs: [{ internalType: 'uint256', name: '', type: 'uint256' }],
    stateMutability: 'view',
    type: 'function',
  },
  {
    inputs: [],
    name: 'requestedApiPayerCount',
    outputs: [{ internalType: 'uint256', name: '', type: 'uint256' }],
    stateMutability: 'view',
    type: 'function',
  },
  {
    inputs: [],
    name: 'rebalanceAmount',
    outputs: [{ internalType: 'uint256', name: '', type: 'uint256' }],
    stateMutability: 'view',
    type: 'function',
  },
  {
    inputs: [],
    name: 'pkpCount',
    outputs: [{ internalType: 'uint256', name: '', type: 'uint256' }],
    stateMutability: 'view',
    type: 'function',
  },
];

const SET_REBALANCE_AMOUNT_ABI = [
  {
    inputs: [{ internalType: 'uint256', name: 'newRebalanceAmount', type: 'uint256' }],
    name: 'setRebalanceAmount',
    outputs: [],
    stateMutability: 'nonpayable',
    type: 'function',
  },
];

const SET_REQUESTED_API_PAYER_COUNT_ABI = [
  {
    inputs: [{ internalType: 'uint256', name: 'newRequestedApiPayerCount', type: 'uint256' }],
    name: 'setRequestedApiPayerCount',
    outputs: [],
    stateMutability: 'nonpayable',
    type: 'function',
  },
];

const SET_NODE_CONFIGURATION_ABI = [
  {
    inputs: [
      { internalType: 'string', name: 'key',   type: 'string' },
      { internalType: 'string', name: 'value', type: 'string' },
    ],
    name: 'setNodeConfiguration',
    outputs: [],
    stateMutability: 'nonpayable',
    type: 'function',
  },
];

const SET_ADMIN_API_PAYER_ABI = [
  {
    inputs: [{ internalType: 'address', name: 'newAdminApiPayerAccount', type: 'address' }],
    name: 'setAdminApiPayerAccount',
    outputs: [],
    stateMutability: 'nonpayable',
    type: 'function',
  },
];

/* ═══ WalletConnect ══════════════════════════════════════════════════════════ */

const WALLETCONNECT_PROJECT_ID = '8feea2064504b04d14a55d6fbef18966';

let _wcProvider = null; // cached WalletConnect provider instance

/* ═══ Utilities ══════════════════════════════════════════════════════════════ */

function el(id) {
  return document.getElementById(id);
}

function escapeHtml(str) {
  return String(str).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

function setValue(id, text, isEmpty) {
  const node = el(id);
  if (!node) return;
  node.textContent = text ?? '—';
  node.classList.toggle('empty', isEmpty);
}

function showStatus(elementId, msg, isError) {
  const node = el(elementId);
  if (!node) return;
  node.textContent = msg;
  node.style.color = isError ? '#f87171' : '#34d399';
  node.style.display = msg ? '' : 'none';
}

/* ═══ Copy to clipboard ══════════════════════════════════════════════════════ */

function copyText(text, triggerEl) {
  if (!navigator.clipboard?.writeText) return;
  if (!triggerEl) return;

  const handleSuccess = () => {
    triggerEl.classList.remove('copy-failed');
    triggerEl.classList.add('copied');
    setTimeout(() => triggerEl.classList.remove('copied'), 1200);
  };

  const handleFailure = () => {
    triggerEl.classList.remove('copied');
    triggerEl.classList.add('copy-failed');
    setTimeout(() => triggerEl.classList.remove('copy-failed'), 1200);
  };

  if (
    typeof navigator !== 'undefined' &&
    navigator.clipboard &&
    typeof navigator.clipboard.writeText === 'function'
  ) {
    navigator.clipboard.writeText(text).then(handleSuccess).catch(handleFailure);
    return;
  }

  // Fallback for older browsers / non-secure contexts
  if (typeof document === 'undefined' || !document.body) {
    handleFailure();
    return;
  }

  try {
    const textarea = document.createElement('textarea');
    textarea.value = text;
    textarea.setAttribute('readonly', '');
    textarea.style.position = 'fixed';
    textarea.style.top = '-9999px';
    textarea.style.left = '-9999px';
    textarea.style.opacity = '0';
    document.body.appendChild(textarea);
    textarea.select();
    const successful = typeof document.execCommand === 'function' && document.execCommand('copy');
    document.body.removeChild(textarea);
    if (successful) {
      handleSuccess();
    } else {
      handleFailure();
    }
  } catch (e) {
    handleFailure();
  }
}

/* ═══ Threshold management ═══════════════════════════════════════════════════ */

const THRESHOLD_DEFAULTS = { warning: 0.02, critical: 0.005, target: 0.05 };

function thresholdStorageKey() {
  return 'lit-monitor-thresholds-' + getServerUrl();
}

function getThresholds() {
  try {
    const stored = JSON.parse(localStorage.getItem(thresholdStorageKey()));
    if (stored && typeof stored === 'object') {
      const result = { ...THRESHOLD_DEFAULTS };
      for (const key of ['warning', 'critical', 'target']) {
        if (typeof stored[key] === 'number' && isFinite(stored[key]) && stored[key] >= 0) {
          result[key] = stored[key];
        }
      }
      return result;
    }
  } catch {}
  return { ...THRESHOLD_DEFAULTS };
}

function saveThresholdsToStorage(t) {
  localStorage.setItem(thresholdStorageKey(), JSON.stringify(t));
}

function classifyHealth(balance, thresholds) {
  if (balance == null || isNaN(balance)) return 'unknown';
  if (balance < thresholds.critical) return 'critical';
  if (balance < thresholds.warning) return 'warning';
  return 'healthy';
}

function loadThresholdInputs() {
  const t = getThresholds();
  const w = el('threshold-warning');
  const c = el('threshold-critical');
  const tgt = el('threshold-target');
  if (w) w.value = t.warning;
  if (c) c.value = t.critical;
  if (tgt) tgt.value = t.target;
}

/* ═══ Payer data ═════════════════════════════════════════════════════════════ */

let payerData = []; // [{ address, balance, health, index }]

function getTokenLabel() {
  const t = (el('cc-token')?.textContent || 'ETH').trim();
  return t === '—' ? 'ETH' : t;
}

function renderPayerHealthTable() {
  const listEl = el('api-payers-list');
  if (!listEl || payerData.length === 0) return;

  const thresholds = getThresholds();
  payerData.forEach(p => { p.health = classifyHealth(p.balance, thresholds); });

  const order = { critical: 0, warning: 1, unknown: 2, healthy: 3 };
  const sorted = [...payerData].sort((a, b) => {
    const od = (order[a.health] ?? 2) - (order[b.health] ?? 2);
    if (od !== 0) return od;
    if (a.balance == null) return 1;
    if (b.balance == null) return -1;
    return a.balance - b.balance;
  });

  const token = getTokenLabel();

  listEl.innerHTML =
    `<table><thead><tr>` +
      `<th style="width:12px"></th><th>Payer</th><th>Address</th><th class="eth">${escapeHtml(token)}</th>` +
    `</tr></thead><tbody>` +
    sorted.map(p => {
      const balText = p.balance != null ? p.balance.toFixed(6) : '…';
      const balStyle = p.balance == null ? ' style="color:var(--muted)"' : '';
      return `<tr class="payer-row ${p.health}">` +
        `<td><span class="health-dot ${p.health}"></span></td>` +
        `<td style="white-space:nowrap">Payer ${p.index}</td>` +
        `<td class="copyable" data-copy="${escapeHtml(p.address)}">${escapeHtml(p.address)}</td>` +
        `<td class="eth copyable" data-copy="${balText}"${balStyle}>${balText}${p.balance != null ? ' ' + escapeHtml(token) : ''}</td>` +
      `</tr>`;
    }).join('') +
    `</tbody></table>`;

  listEl.querySelectorAll('.copyable').forEach(cell => {
    cell.style.cursor = 'pointer';
    cell.title = 'Click to copy';
    cell.addEventListener('click', () => copyText(cell.dataset.copy, cell));
  });

  updateHealthSummary();
}

function updateHealthSummary() {
  const bar = el('health-summary');
  if (!bar) return;

  if (payerData.length === 0) {
    bar.style.display = 'none';
    return;
  }

  bar.style.display = '';
  const counts = { healthy: 0, warning: 0, critical: 0, unknown: 0 };
  let totalBalance = 0;
  payerData.forEach(p => {
    counts[p.health] = (counts[p.health] || 0) + 1;
    if (p.balance != null) totalBalance += p.balance;
  });

  const token = getTokenLabel();
  const parts = [];
  if (counts.critical > 0) parts.push(`<span style="color:var(--critical)">${counts.critical} critical</span>`);
  if (counts.warning > 0) parts.push(`<span style="color:var(--warning)">${counts.warning} warning</span>`);
  if (counts.healthy > 0) parts.push(`<span style="color:var(--success)">${counts.healthy} healthy</span>`);
  if (counts.unknown > 0) parts.push(`<span style="color:var(--muted)">${counts.unknown} loading</span>`);

  const summaryText = el('health-summary-text');
  if (summaryText) summaryText.innerHTML = `${payerData.length} payers: ${parts.join(' &middot; ')}`;

  const poolTotal = el('health-pool-total');
  if (poolTotal) poolTotal.textContent = totalBalance.toFixed(6) + ' ' + token;

  const adminBal = el('val-admin-api-payer-balance');
  const reserveEl = el('health-admin-reserve');
  if (reserveEl) reserveEl.textContent = adminBal?.textContent || '—';
}

/* ═══ Network selector ═══════════════════════════════════════════════════════ */

function getServerUrl() {
  return (el('network')?.value || '').replace(/\/$/, '');
}

/* ═══ CORS proxy + chainlist ═════════════════════════════════════════════════ */

const CORS_PROXY = 'https://whateverorigin.org/get?url=';

async function resolveRpcUrlFromChainlist(chainId) {
  if (chainId == null || chainId === '') return null;
  try {
    const url = `${CORS_PROXY}${encodeURIComponent(`https://chainlistapi.com/chains/${chainId}`)}`;
    const res = await fetch(url);
    if (!res.ok) return null;
    const wrapper = await res.json();
    const data = typeof wrapper?.contents === 'string' ? JSON.parse(wrapper.contents) : wrapper;
    const rpcs = data?.rpc;
    if (!Array.isArray(rpcs)) return null;
    const entry = rpcs.find((r) => {
      const u = typeof r === 'string' ? r : r?.url;
      return typeof u === 'string' && u.startsWith('https://');
    });
    return entry ? (typeof entry === 'string' ? entry : entry.url) : null;
  } catch (_) {
    return null;
  }
}

/* ═══ Data fetching ══════════════════════════════════════════════════════════ */

async function getNodeChainConfig(serverUrl) {
  const resultsEl = el('chain-config-results');
  const errEl = el('chain-config-error');

  if (errEl) errEl.style.display = 'none';
  if (resultsEl) resultsEl.style.display = 'block';
  ['cc-chain-name','cc-chain-id','cc-is-evm','cc-testnet','cc-token','cc-contract-address']
    .forEach(id => setValue(id, '…', false));
  const rpcUrlInput = el('cc-rpc-url');
  if (rpcUrlInput) rpcUrlInput.value = '';

  try {
    const res = await fetch(`${serverUrl}/get_node_chain_config`);
    if (!res.ok) throw new Error(`HTTP ${res.status} ${res.statusText}`);
    const cfg = await res.json();

    setValue('cc-chain-name',       cfg.chain_name       ?? '—', !cfg.chain_name);
    setValue('cc-chain-id',         cfg.chain_id != null ? String(cfg.chain_id) : '—', cfg.chain_id == null);
    setValue('cc-is-evm',           cfg.is_evm   != null ? String(cfg.is_evm)   : '—', cfg.is_evm   == null);
    setValue('cc-testnet',          cfg.testnet  != null ? String(cfg.testnet)  : '—', cfg.testnet  == null);
    setValue('cc-token',            cfg.token        ?? '—', !cfg.token);

    let rpcUrl = cfg.rpc_url ?? '';
    if (!rpcUrl && cfg.chain_id != null && cfg.is_evm) {
      rpcUrl = (await resolveRpcUrlFromChainlist(cfg.chain_id)) ?? '';
    }
    const rpcInput = el('cc-rpc-url');
    if (rpcInput) rpcInput.value = rpcUrl;

    setValue('cc-contract-address', cfg.contract_address ?? '—', !cfg.contract_address);

    const contractInput = el('contract-address');
    if (contractInput && cfg.contract_address) contractInput.value = cfg.contract_address;
  } catch (e) {
    const rpcInput = el('cc-rpc-url');
    if (rpcInput) rpcInput.value = '';
    if (resultsEl) resultsEl.style.display = 'none';
    if (errEl) { errEl.textContent = e?.message || String(e); errEl.style.display = 'block'; }
  }
}

async function getApiPayers(serverUrl) {
  const resultsEl = el('api-payers-results');
  const listEl = el('api-payers-list');
  const errEl = el('api-payers-error');

  if (errEl) errEl.style.display = 'none';
  if (listEl) listEl.innerHTML = '<span style="color:var(--muted);font-family:\'JetBrains Mono\',monospace;font-size:0.85rem">Loading…</span>';
  if (resultsEl) resultsEl.style.display = 'block';

  try {
    const res = await fetch(`${serverUrl}/get_api_payers`);
    if (!res.ok) throw new Error(`HTTP ${res.status} ${res.statusText}`);
    const payers = await res.json();

    if (!Array.isArray(payers) || payers.length === 0) {
      payerData = [];
      let firstPayer = '';
      try {
        const fpRes = await fetch(`${serverUrl}/get_admin_api_payer`);
        if (fpRes.ok) {
          const fpData = await fpRes.json();
          if (typeof fpData === 'string' && fpData) firstPayer = fpData;
        }
      } catch {}
      listEl.innerHTML =
        `<p style="color:var(--muted);font-size:0.85rem;margin:0">No payers configured.</p>` +
        (firstPayer
          ? `<p style="font-size:0.85rem;margin:0.5rem 0 0;color:#f87171">` +
              `Please set the default API payer address: <br> ` +
              `<span style="font-family:'JetBrains Mono',monospace;word-break:break-all">${escapeHtml(firstPayer)}</span>` +
              `<br>Once this account is set, please fund with native tokens.` +
            `</p>`
          : '');
      updateHealthSummary();
      return;
    }

    // Initialize payer data with loading state
    payerData = payers.map((addr, i) => ({ address: addr, balance: null, health: 'unknown', index: i + 1 }));
    renderPayerHealthTable();

    // Fetch all balances in parallel
    const rpcUrl = (el('cc-rpc-url')?.value || '').trim();
    if (rpcUrl) {
      const provider = new ethers.JsonRpcProvider(rpcUrl);
      const results = await Promise.allSettled(
        payers.map(addr => provider.getBalance(addr))
      );
      results.forEach((result, i) => {
        if (result.status === 'fulfilled') {
          payerData[i].balance = parseFloat(ethers.formatEther(result.value));
        }
      });
      renderPayerHealthTable();
    }
  } catch (e) {
    payerData = [];
    if (resultsEl) resultsEl.style.display = 'none';
    if (errEl) { errEl.textContent = e?.message || String(e); errEl.style.display = 'block'; }
    updateHealthSummary();
  }
}

async function fetchVersion(serverUrl) {
  const resultsEl = el('version-results');
  const errEl = el('version-error');

  if (errEl) errEl.style.display = 'none';
  setValue('ver-version', '…', false);
  const submodulesEl = el('ver-submodules');
  if (submodulesEl) submodulesEl.innerHTML = '';
  if (resultsEl) resultsEl.style.display = 'block';

  try {
    const res = await fetch(`${serverUrl}/version`);
    if (!res.ok) throw new Error(`HTTP ${res.status} ${res.statusText}`);
    const data = await res.json();

    setValue('ver-name',           data.name           ?? '—', !data.name);
    setValue('ver-version',        data.version        ?? '—', !data.version);
    setValue('ver-commit-version', data.commit_version ?? '—', !data.commit_version);

    if (submodulesEl) {
      const rows = (data.submodule_versions ?? []);
      if (rows.length === 0) {
        submodulesEl.innerHTML = '<span style="color:var(--muted);font-size:0.85rem">None</span>';
      } else {
        submodulesEl.innerHTML =
          `<table>` +
            `<thead><tr><th>Submodule</th><th>Version</th></tr></thead>` +
            `<tbody>` +
              rows.map(([name, ver]) =>
                `<tr><td>${escapeHtml(name)}</td><td>${escapeHtml(ver)}</td></tr>`
              ).join('') +
            `</tbody>` +
          `</table>`;
      }
    }
  } catch (e) {
    if (resultsEl) resultsEl.style.display = 'none';
    if (errEl) { errEl.textContent = e?.message || String(e); errEl.style.display = 'block'; }
  }
}

// ── fetchContractValues ───────────────────────────────────────────────────

function showError(msg) {
  const err = el('error');
  if (err) { err.textContent = msg; err.style.display = 'block'; }
}

function hideError() {
  const err = el('error');
  if (err) err.style.display = 'none';
}

async function fetchContractValues() {
  const rpcUrl = (el('cc-rpc-url')?.value || '').trim();
  const contractAddress = (el('contract-address')?.value || '').trim();
  const results = el('results');

  if (!rpcUrl || !contractAddress) {
    showError('RPC URL and contract address not yet loaded — select a network first.');
    return;
  }

  hideError();
  setValue('val-pricing-operator', '…', false);
  setValue('val-pricing-operator-balance', '', false);
  setValue('val-admin-api-payer', '…', false);
  setValue('val-admin-api-payer-balance', '', false);
  setValue('val-payer-count', '…', false);
  setValue('val-requested-api-payer-count', '…', false);
  setValue('val-rebalance-amount', '…', false);
  setValue('val-pkp-count', '…', false);
  if (results) results.style.display = 'block';

  try {
    const provider = new ethers.JsonRpcProvider(rpcUrl);
    const contract = new ethers.Contract(contractAddress, ACCOUNT_CONFIG_VIEW_ABI, provider);

    const [pricingOperator, adminApiPayer, apiPayerCount, requestedApiPayerCount, rebalanceAmountWei, pkpCount] = await Promise.all([
      contract.pricingOperator(),
      contract.adminApiPayerAccount(),
      contract.apiPayerCount(),
      contract.requestedApiPayerCount(),
      contract.rebalanceAmount(),
      contract.pkpCount(),
    ]);

    setValue('val-pricing-operator', pricingOperator ?? '—', !pricingOperator);
    setValue('val-admin-api-payer', adminApiPayer ?? '—', !adminApiPayer);
    setValue('val-payer-count', String(apiPayerCount), false);
    setValue('val-requested-api-payer-count', String(requestedApiPayerCount), false);
    setValue('val-rebalance-amount', ethers.formatEther(rebalanceAmountWei) + ' ETH', false);
    setValue('val-pkp-count', String(pkpCount), false);
    const payerCountSelect = el('payer-count');
    if (!payerCountSelect || document.activeElement !== payerCountSelect) {
      populateApiPayerCountDropdown(Number(requestedApiPayerCount));
    }

    const rebalanceInput = el('rebalance-amount');
    if (rebalanceInput && document.activeElement !== rebalanceInput) {
      rebalanceInput.value = ethers.formatEther(rebalanceAmountWei);
    }

    // Fetch balances asynchronously so they don't block the main display.
    const balanceTargets = [
      [pricingOperator, 'val-pricing-operator-balance'],
      [adminApiPayer,   'val-admin-api-payer-balance'],
    ];
    for (const [addr, id] of balanceTargets) {
      if (!addr || addr === ethers.ZeroAddress) continue;
      provider.getBalance(addr).then(wei => {
        const node = el(id);
        if (node) {
          node.textContent = parseFloat(ethers.formatEther(wei)).toFixed(6) + ' ETH';
          node.style.color = '';
        }
      }).catch(() => {});
    }
  } catch (e) {
    showError(e?.message || String(e));
  }
}

// ── setRequestedApiPayerCount ─────────────────────────────────────────────

function populateApiPayerCountDropdown(currentValue) {
  const select = el('payer-count');
  if (!select) return;
  select.innerHTML = Array.from({ length: 20 }, (_, i) => i + 1)
    .map(n => `<option value="${n}"${n === currentValue ? ' selected' : ''}>${n}</option>`)
    .join('');
}

function showWalletPicker() {
  return new Promise((resolve) => {
    const dialog = el('wallet-picker');
    if (!dialog || typeof dialog.showModal !== 'function') { resolve('metamask'); return; }

    const cleanup = () => {
      dialog.removeEventListener('click', handler);
      dialog.removeEventListener('cancel', onCancel);
    };

    const handler = (e) => {
      const btn = e.target.closest('[data-wallet]');
      if (!btn) return;
      cleanup();
      dialog.close();
      resolve(btn.dataset.wallet);
    };

    const onCancel = () => {
      cleanup();
      resolve('cancel');
    };

    dialog.addEventListener('click', handler);
    dialog.addEventListener('cancel', onCancel, { once: true });

    dialog.showModal();
  });
}

async function connectWallet() {
  const choice = await showWalletPicker();

  if (choice === 'cancel') throw new Error('Wallet connection cancelled.');

  if (choice === 'walletconnect') {
    const chainIdText = (el('cc-chain-id')?.textContent || '').trim();
    const parsed = Number(chainIdText);
    if (!Number.isFinite(parsed) || parsed <= 0) {
      throw new Error('Invalid or missing chain ID. Please wait for network configuration to load and try again.');
    }
    const chainId = parsed;
    const rpcUrl = (el('cc-rpc-url')?.value || '').trim();

    // Dynamically import WalletConnect Ethereum Provider
    let EthereumProvider;
    try {
      ({ EthereumProvider } = await import(
        'https://esm.sh/@walletconnect/ethereum-provider@2.23.9'
      ));
    } catch {
      throw new Error('Failed to load WalletConnect. Check your network connection or try again.');
    }

    // Disconnect any stale session before reconnecting (with timeout to avoid hanging)
    if (_wcProvider) {
      try {
        await Promise.race([
          _wcProvider.disconnect(),
          new Promise((_, reject) => setTimeout(() => reject(new Error('timeout')), 5000)),
        ]);
      } catch {}
      _wcProvider = null;
    }

    try {
      _wcProvider = await EthereumProvider.init({
        projectId: WALLETCONNECT_PROJECT_ID,
        chains: [chainId],
        rpcMap: rpcUrl ? { [chainId]: rpcUrl } : undefined,
        showQrModal: true,
      });

      await _wcProvider.connect();
    } catch (err) {
      _wcProvider = null;
      throw err;
    }
    return new ethers.BrowserProvider(_wcProvider);
  }

  // Default: MetaMask / browser wallet
  if (!window.ethereum) throw new Error('No browser wallet found. Install MetaMask or use WalletConnect.');
  const provider = new ethers.BrowserProvider(window.ethereum);
  await provider.send('eth_requestAccounts', []);
  return provider;
}

// ── fetchChainConfigKeys ────────────────────────────────────────────────

async function fetchChainConfigKeys(serverUrl) {
  const listEl = el('chain-config-keys-list');
  const errEl = el('chain-config-keys-error');

  if (errEl) errEl.style.display = 'none';
  if (listEl) listEl.innerHTML = '<span style="color:var(--muted)">Loading…</span>';

  try {
    const res = await fetch(`${serverUrl}/get_chain_config_keys`);
    if (!res.ok) throw new Error(`HTTP ${res.status} ${res.statusText}`);
    const data = await res.json();
    const keys = data.keys ?? [];

    if (listEl) {
      listEl.innerHTML = keys.length === 0
        ? '<span style="color:var(--muted)">No keys returned.</span>'
        : keys.map(k => `<div class="result-row"><span class="result-value">${escapeHtml(k)}</span></div>`).join('');
    }
  } catch (e) {
    if (listEl) listEl.innerHTML = '';
    if (errEl) { errEl.textContent = e?.message || String(e); errEl.style.display = 'block'; }
  }
}

// ── fetchLitActionClientConfig ──────────────────────────────────────────

async function fetchLitActionClientConfig(serverUrl) {
  const resultsEl = el('lit-action-config-results');
  const errEl = el('lit-action-config-error');

  if (errEl) errEl.style.display = 'none';
  if (resultsEl) resultsEl.style.display = 'block';

  const fields = [
    'lac-timeout-ms', 'lac-async-timeout-ms', 'lac-memory-limit-mb',
    'lac-max-code-length', 'lac-max-response-length', 'lac-max-console-log-length',
    'lac-max-fetch-count', 'lac-max-get-keys-count', 'lac-max-retries',
  ];
  fields.forEach(id => setValue(id, '…', false));

  try {
    const res = await fetch(`${serverUrl}/get_lit_action_client_config`);
    if (!res.ok) throw new Error(`HTTP ${res.status} ${res.statusText}`);
    const cfg = await res.json();

    setValue('lac-timeout-ms',            cfg.timeout_ms           != null ? String(cfg.timeout_ms)           : '—', cfg.timeout_ms           == null);
    setValue('lac-async-timeout-ms',      cfg.async_timeout_ms     != null ? String(cfg.async_timeout_ms)     : '—', cfg.async_timeout_ms     == null);
    setValue('lac-memory-limit-mb',       cfg.memory_limit_mb      != null ? String(cfg.memory_limit_mb)      : '—', cfg.memory_limit_mb      == null);
    setValue('lac-max-code-length',       cfg.max_code_length      != null ? String(cfg.max_code_length)      : '—', cfg.max_code_length      == null);
    setValue('lac-max-response-length',   cfg.max_response_length  != null ? String(cfg.max_response_length)  : '—', cfg.max_response_length  == null);
    setValue('lac-max-console-log-length',cfg.max_console_log_length != null ? String(cfg.max_console_log_length) : '—', cfg.max_console_log_length == null);
    setValue('lac-max-fetch-count',       cfg.max_fetch_count      != null ? String(cfg.max_fetch_count)      : '—', cfg.max_fetch_count      == null);
    setValue('lac-max-get-keys-count',    cfg.max_get_keys_count   != null ? String(cfg.max_get_keys_count)   : '—', cfg.max_get_keys_count   == null);
    setValue('lac-max-retries',           cfg.max_retries          != null ? String(cfg.max_retries)          : '—', cfg.max_retries          == null);
  } catch (e) {
    if (resultsEl) resultsEl.style.display = 'none';
    if (errEl) { errEl.textContent = e?.message || String(e); errEl.style.display = 'block'; }
  }
}

// ── nodeConfigurationValues ───────────────────────────────────────────────

async function fetchNodeConfigValues() {
  const rpcUrl = (el('cc-rpc-url')?.value || '').trim();
  const contractAddress = (el('contract-address')?.value || '').trim();
  const tableEl = el('node-config-table');
  const errEl = el('node-config-fetch-error');

  if (errEl) errEl.style.display = 'none';
  if (!rpcUrl || !contractAddress) return;

  if (tableEl) tableEl.innerHTML = '<tr><td colspan="2" style="color:var(--muted)">Loading…</td></tr>';

  try {
    const provider = new ethers.JsonRpcProvider(rpcUrl);
    const contract = new ethers.Contract(contractAddress, ACCOUNT_CONFIG_VIEW_ABI, provider);
    const pairs = await contract.nodeConfigurationValues();

    if (!tableEl) return;
    if (!pairs || pairs.length === 0) {
      tableEl.innerHTML = '<tr><td colspan="2" style="color:var(--muted)">No configuration values set.</td></tr>';
      return;
    }
    tableEl.innerHTML = pairs.map(([key, value]) =>
      `<tr><td>${escapeHtml(key)}</td><td>${escapeHtml(value)}</td></tr>`
    ).join('');
  } catch (e) {
    if (tableEl) tableEl.innerHTML = '';
    if (errEl) { errEl.textContent = e?.message || String(e); errEl.style.display = 'block'; }
  }
}

/* ═══ Auto-refresh ═══════════════════════════════════════════════════════════ */

const REFRESH_INTERVAL = 30;
let countdownTimer = null;
let secondsLeft = REFRESH_INTERVAL;
let isRefreshing = false;

function startAutoRefresh() {
  stopAutoRefresh();
  secondsLeft = REFRESH_INTERVAL;
  updateCountdown();
  countdownTimer = setInterval(() => {
    secondsLeft--;
    updateCountdown();
    if (secondsLeft <= 0) {
      doAutoRefresh().catch((err) => {
        console.error('Auto-refresh failed:', err);
      });
    }
  }, 1000);
}

function stopAutoRefresh() {
  if (countdownTimer) {
    clearInterval(countdownTimer);
    countdownTimer = null;
  }
}

function updateCountdown() {
  const cdEl = el('refresh-countdown');
  if (cdEl) cdEl.textContent = secondsLeft + 's';
}

async function doAutoRefresh() {
  if (isRefreshing) return;
  try {
    await refreshBalances();
  } finally {
    secondsLeft = REFRESH_INTERVAL;
    updateCountdown();
  }
}

// Page Visibility API — pause polling when tab is hidden, refresh immediately on return

document.addEventListener('visibilitychange', async () => {
  if (document.hidden) {
    stopAutoRefresh();
  } else {
    try {
      await doAutoRefresh();
    } finally {
      startAutoRefresh();
    }
  }
});


/* ═══ Load / refresh ═════════════════════════════════════════════════════════ */

async function loadNetwork() {
  const serverUrl = getServerUrl();
  stopAutoRefresh();
  await getNodeChainConfig(serverUrl); // Must run first — populates RPC URL
  await Promise.all([
    getApiPayers(serverUrl),
    fetchVersion(serverUrl),
    fetchContractValues(),
    fetchNodeConfigValues(),
    fetchLitActionClientConfig(serverUrl),
    fetchChainConfigKeys(serverUrl),
  ]);
  updateHealthSummary(); // pick up admin reserve after fetchContractValues
  loadThresholdInputs();
  startAutoRefresh();
}

async function refreshBalances() {
  if (isRefreshing) return;
  isRefreshing = true;
  try {
    const serverUrl = getServerUrl();
    await Promise.all([
      getApiPayers(serverUrl),
      fetchContractValues(),
      fetchNodeConfigValues(),
    ]);
    updateHealthSummary(); // pick up admin reserve after fetchContractValues
  } finally {
    isRefreshing = false;
  }
}

/* ═══ Accordion ══════════════════════════════════════════════════════════════ */

function toggleAccordion(card) {
  if (!card) return;
  const header = card.querySelector(':scope > .card-header');
  const body = card.querySelector(':scope > .card-body');
  const willCollapse = !card.classList.contains('collapsed');
  if (body) {
    if (willCollapse) {
      body.style.maxHeight = body.scrollHeight + 'px';
      requestAnimationFrame(() => {
        card.classList.add('collapsed');
        body.style.maxHeight = '0px';
      });
    } else {
      card.classList.remove('collapsed');
      body.style.maxHeight = body.scrollHeight + 'px';
      const onEnd = (e) => {
        if (e.propertyName !== 'max-height') return;
        body.style.maxHeight = '';
        body.removeEventListener('transitionend', onEnd);
      };
      body.addEventListener('transitionend', onEnd);
    }
  }
  if (header) header.setAttribute('aria-expanded', String(!willCollapse));
}

document.addEventListener('click', (e) => {
  const header = e.target.closest('[data-accordion] > .card-header');
  if (!header) return;
  if (e.target.closest('button, input, select, textarea, a')) return;
  toggleAccordion(header.parentElement);
});

document.addEventListener('keydown', (e) => {
  if (e.key !== 'Enter' && e.key !== ' ') return;
  const header = e.target.closest('[data-accordion] > .card-header');
  if (!header || e.target !== header) return;
  e.preventDefault();
  toggleAccordion(header.parentElement);
});

/* ═══ Settings panel ═════════════════════════════════════════════════════════ */

el('btn-toggle-settings')?.addEventListener('click', () => {
  const panel = el('settings-panel');
  if (panel) panel.classList.toggle('open');
});

el('btn-save-thresholds')?.addEventListener('click', () => {
  const w = parseFloat(el('threshold-warning')?.value);
  const c = parseFloat(el('threshold-critical')?.value);
  const t = parseFloat(el('threshold-target')?.value);

  if (isNaN(w) || isNaN(c) || isNaN(t) || w < 0 || c < 0 || t < 0) {
    showStatus('threshold-status', 'Enter valid positive numbers.', true);
    return;
  }
  if (c >= w) {
    showStatus('threshold-status', 'Critical must be less than warning.', true);
    return;
  }

  saveThresholdsToStorage({ warning: w, critical: c, target: t });
  renderPayerHealthTable();
  showStatus('threshold-status', 'Saved.', false);
});

/* ═══ Contract write handlers ════════════════════════════════════════════════ */

el('btn-set-default-api-payer')?.addEventListener('click', async () => {
  const contractAddress = (el('contract-address')?.value || '').trim();
  const btn = el('btn-set-default-api-payer');
  let newApiPayer;

  try {
    newApiPayer = ethers.getAddress((el('default-api-payer')?.value || '').trim());
  } catch {
    showStatus('default-api-payer-status', 'Enter a valid Ethereum address.', true);
    return;
  }

  if (!contractAddress) {
    showStatus('default-api-payer-status', 'Contract address not yet loaded — select a network first.', true);
    return;
  }

  btn.disabled = true;
  showStatus('default-api-payer-status', 'Connecting wallet…', false);

  try {
    const provider = await connectWallet();
    const signer = await provider.getSigner();
    const contract = new ethers.Contract(contractAddress, SET_ADMIN_API_PAYER_ABI, signer);

    showStatus('default-api-payer-status', 'Waiting for signature…', false);
    const tx = await contract.setAdminApiPayerAccount(newApiPayer);

    showStatus('default-api-payer-status', 'Transaction submitted: ' + tx.hash + '. Waiting for confirmation…', false);
    await tx.wait();

    showStatus('default-api-payer-status', 'Done. Default API payer updated to ' + newApiPayer, false);
    el('default-api-payer').value = '';
  } catch (e) {
    showStatus('default-api-payer-status', 'Error: ' + (e?.reason || e?.message || String(e)), true);
  } finally {
    btn.disabled = false;
  }
});

el('btn-set-rebalance-amount')?.addEventListener('click', async () => {
  const contractAddress = (el('contract-address')?.value || '').trim();
  const btn = el('btn-set-rebalance-amount');
  const raw = (el('rebalance-amount')?.value || '').trim();

  if (!contractAddress) {
    showStatus('rebalance-amount-status', 'Contract address not yet loaded — select a network first.', true);
    return;
  }

  let amountWei;
  try {
    amountWei = ethers.parseEther(raw);
  } catch {
    showStatus('rebalance-amount-status', 'Enter a valid ETH amount (e.g. 0.05).', true);
    return;
  }

  btn.disabled = true;
  showStatus('rebalance-amount-status', 'Connecting wallet…', false);

  try {
    const provider = await connectWallet();
    const signer = await provider.getSigner();
    const contract = new ethers.Contract(contractAddress, SET_REBALANCE_AMOUNT_ABI, signer);

    showStatus('rebalance-amount-status', 'Waiting for signature…', false);
    const tx = await contract.setRebalanceAmount(amountWei);

    showStatus('rebalance-amount-status', 'Transaction submitted: ' + tx.hash + '. Waiting for confirmation…', false);
    await tx.wait();

    showStatus('rebalance-amount-status', 'Done. Rebalance amount set to ' + ethers.formatEther(amountWei) + ' ETH', false);
  } catch (e) {
    showStatus('rebalance-amount-status', 'Error: ' + (e?.reason || e?.message || String(e)), true);
  } finally {
    btn.disabled = false;
  }
});

el('btn-set-node-config')?.addEventListener('click', async () => {
  const contractAddress = (el('contract-address')?.value || '').trim();
  const key   = (el('node-config-key')?.value   || '').trim();
  const value = (el('node-config-value')?.value || '').trim();
  const btn   = el('btn-set-node-config');

  if (!contractAddress) {
    showStatus('node-config-status', 'Contract address not yet loaded — select a network first.', true);
    return;
  }
  if (!key) {
    showStatus('node-config-status', 'Enter a configuration key.', true);
    return;
  }

  btn.disabled = true;
  showStatus('node-config-status', 'Connecting wallet…', false);

  try {
    const provider = await connectWallet();
    const signer   = await provider.getSigner();
    const contract = new ethers.Contract(contractAddress, SET_NODE_CONFIGURATION_ABI, signer);

    showStatus('node-config-status', 'Waiting for signature…', false);
    const tx = await contract.setNodeConfiguration(key, value);

    showStatus('node-config-status', 'Transaction submitted: ' + tx.hash + '. Waiting for confirmation…', false);
    await tx.wait();

    showStatus('node-config-status', 'Done. Configuration key "' + key + '" set.', false);
    el('node-config-key').value   = '';
    el('node-config-value').value = '';
    await fetchNodeConfigValues();
  } catch (e) {
    showStatus('node-config-status', 'Error: ' + (e?.reason || e?.message || String(e)), true);
  } finally {
    btn.disabled = false;
  }
});

el('btn-refresh-node-config')?.addEventListener('click', async () => {
  const btn = el('btn-refresh-node-config');
  btn.disabled = true;
  try { await fetchNodeConfigValues(); } finally { btn.disabled = false; }
});

el('cc-rpc-url')?.addEventListener('change', () => refreshBalances());

el('btn-refresh-contract')?.addEventListener('click', async () => {
  const btn = el('btn-refresh-contract');
  btn.disabled = true;
  try {
    await refreshBalances();
  } finally {
    btn.disabled = false;
  }
});

el('btn-refresh-all')?.addEventListener('click', async () => {
  const btn = el('btn-refresh-all');
  btn.disabled = true;
  try {
    await refreshBalances();
    secondsLeft = REFRESH_INTERVAL;
    updateCountdown();
  } finally {
    btn.disabled = false;
  }
});

el('btn-set-payer-count')?.addEventListener('click', async () => {
  const select = el('payer-count');
  const contractAddress = (el('contract-address')?.value || '').trim();
  const newCount = Number(select?.value);
  const btn = el('btn-set-payer-count');

  if (!contractAddress) {
    showStatus('payer-count-status', 'Contract address not yet loaded — select a network first.', true);
    return;
  }
  if (!newCount) {
    showStatus('payer-count-status', 'Select a value first.', true);
    return;
  }

  btn.disabled = true;
  select.disabled = true;
  showStatus('payer-count-status', 'Connecting wallet…', false);

  try {
    const provider = await connectWallet();
    const signer = await provider.getSigner();
    const contract = new ethers.Contract(contractAddress, SET_REQUESTED_API_PAYER_COUNT_ABI, signer);

    showStatus('payer-count-status', 'Waiting for signature…', false);
    const tx = await contract.setRequestedApiPayerCount(newCount);

    showStatus('payer-count-status', 'Transaction submitted: ' + tx.hash + '. Waiting for confirmation…', false);
    await tx.wait();

    showStatus('payer-count-status', 'Done. Requested payer count updated to ' + newCount, false);
    await refreshBalances();
  } catch (e) {
    showStatus('payer-count-status', 'Error: ' + (e?.reason || e?.message || String(e)), true);
  } finally {
    btn.disabled = false;
    select.disabled = false;
  }
});

/* ═══ Initialization ═════════════════════════════════════════════════════════ */

(function () {
  const select = el('network');
  if (!select) return;

  // Pre-select the option whose hostname matches the current page.
  const host = window.location.hostname;
  for (const opt of select.options) {
    try {
      if (new URL(opt.value).hostname === host) { opt.selected = true; break; }
    } catch {}
  }

  select.addEventListener('change', loadNetwork);
  loadNetwork();
})();
