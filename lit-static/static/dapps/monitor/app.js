/**
 * Lit Admin – read AccountConfig contract view functions directly.
 * ABI subset matches lit-api-server/src/accounts/contracts/AccountConfig.json (view functions only).
 */

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

function el(id) {
  return document.getElementById(id);
}

function setValue(id, text, isEmpty) {
  const node = el(id);
  if (!node) return;
  node.textContent = text ?? '—';
  node.classList.toggle('empty', isEmpty);
}


// ── Network selector ──────────────────────────────────────────────────────────

function getServerUrl() {
  // Option values include trailing slash; strip it for consistent path joining.
  return (el('network')?.value || '').replace(/\/$/, '');
}

// ── resolveRpcUrlFromChainlist ───────────────────────────────────────────────────
// Uses Whatever Origin CORS proxy (free, no domain whitelist) to fetch from chainlistapi.com.

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

// ── getNodeChainConfig ────────────────────────────────────────────────────────

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

    // Propagate contract address to the contract card.
    const contractInput = el('contract-address');
    if (contractInput && cfg.contract_address) contractInput.value = cfg.contract_address;
  } catch (e) {
    const rpcInput = el('cc-rpc-url');
    if (rpcInput) rpcInput.value = '';
    if (resultsEl) resultsEl.style.display = 'none';
    if (errEl) { errEl.textContent = e?.message || String(e); errEl.style.display = 'block'; }
  }
}

// ── getApiPayers ──────────────────────────────────────────────────────────────

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
              `<span style="font-family:'JetBrains Mono',monospace;word-break:break-all">${firstPayer}</span>` +
              `<br>Once this account is  set, please fund with native tokens. ` +
            `</p>`
          : '');
      return;
    }

    // Render table immediately, then fill balances as they resolve.
    listEl.innerHTML =
      `<table>` +
        `<thead><tr>` +
          `<th>Description</th>` +
          `<th>Wallet</th>` +
          `<th class="eth">ETH</th>` +
        `</tr></thead>` +
        `<tbody>` +
          payers.map((addr, i) =>
            `<tr>` +
              `<td>Payer ${i + 1}</td>` +
              `<td>${addr}</td>` +
              `<td class="eth" id="payer-balance-${i}" style="color:var(--muted)">…</td>` +
            `</tr>`
          ).join('') +
        `</tbody>` +
      `</table>`;

    const rpcUrl = (el('cc-rpc-url')?.value || '').trim();
    if (rpcUrl) {
      const provider = new ethers.JsonRpcProvider(rpcUrl);
      payers.forEach(async (addr, i) => {
        try {
          const balanceWei = await provider.getBalance(addr);
          const balEl = document.getElementById(`payer-balance-${i}`);
          if (balEl) {
            const eth = parseFloat(ethers.formatEther(balanceWei));
            balEl.textContent = eth.toFixed(6) + ' ETH';
            balEl.style.color = '';
          }
        } catch {}
      });
    }
  } catch (e) {
    if (resultsEl) resultsEl.style.display = 'none';
    if (errEl) { errEl.textContent = e?.message || String(e); errEl.style.display = 'block'; }
  }
}

// ── loadNetwork ───────────────────────────────────────────────────────────────

async function loadNetwork() {
  const serverUrl = getServerUrl();
  await getNodeChainConfig(serverUrl); // Must run first to populate RPC URL
  await getApiPayers(serverUrl);
  await fetchContractValues();
  await fetchNodeConfigValues();
  await fetchLitActionClientConfig(serverUrl);
  await fetchChainConfigKeys(serverUrl);
}

async function refreshBalances() {
  const serverUrl = getServerUrl();
  await getApiPayers(serverUrl);
  await fetchContractValues();
  await fetchNodeConfigValues();
}

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

// ── fetchContractValues ───────────────────────────────────────────────────────

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
    populateApiPayerCountDropdown(Number(requestedApiPayerCount));

    const rebalanceInput = el('rebalance-amount');
    if (rebalanceInput) rebalanceInput.value = ethers.formatEther(rebalanceAmountWei);

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

// ── setRequestedApiPayerCount ─────────────────────────────────────────────────

function populateApiPayerCountDropdown(currentValue) {
  const select = el('payer-count');
  if (!select) return;
  select.innerHTML = Array.from({ length: 20 }, (_, i) => i + 1)
    .map(n => `<option value="${n}"${n === currentValue ? ' selected' : ''}>${n}</option>`)
    .join('');
}

function showSignerCountStatus(msg, isError) {
  const node = el('payer-count-status');
  if (!node) return;
  node.textContent = msg;
  node.style.color = isError ? '#f87171' : '#34d399';
  node.style.display = msg ? 'block' : 'none';
}

async function connectWallet() {
  if (!window.ethereum) throw new Error('Wallet not found. Install a wallet and try again.');
  const provider = new ethers.BrowserProvider(window.ethereum);
  await provider.send('eth_requestAccounts', []);
  return provider;
}

// ── setAdminApiPayerAccount ───────────────────────────────────────────────────

function showDefaultApiPayerStatus(msg, isError) {
  const node = el('default-api-payer-status');
  if (!node) return;
  node.textContent = msg;
  node.style.color = isError ? '#f87171' : '#34d399';
  node.style.display = msg ? 'block' : 'none';
}

el('btn-set-default-api-payer')?.addEventListener('click', async () => {
  const contractAddress = (el('contract-address')?.value || '').trim();
  const btn = el('btn-set-default-api-payer');
  let newApiPayer;

  try {
    newApiPayer = ethers.getAddress((el('default-api-payer')?.value || '').trim());
  } catch {
    showDefaultApiPayerStatus('Enter a valid Ethereum address.', true);
    return;
  }

  if (!contractAddress) {
    showDefaultApiPayerStatus('Contract address not yet loaded — select a network first.', true);
    return;
  }

  btn.disabled = true;
  showDefaultApiPayerStatus('Connecting wallet…', false);

  try {
    const provider = await connectWallet();
    const signer = await provider.getSigner();
    const contract = new ethers.Contract(contractAddress, SET_ADMIN_API_PAYER_ABI, signer);

    showDefaultApiPayerStatus('Waiting for signature…', false);
    const tx = await contract.setAdminApiPayerAccount(newApiPayer);

    showDefaultApiPayerStatus('Transaction submitted: ' + tx.hash + '. Waiting for confirmation…', false);
    await tx.wait();

    showDefaultApiPayerStatus('Done. Default API payer updated to ' + newApiPayer, false);
    el('default-api-payer').value = '';
  } catch (e) {
    showDefaultApiPayerStatus('Error: ' + (e?.reason || e?.message || String(e)), true);
  } finally {
    btn.disabled = false;
  }
});

// ── setRebalanceAmount ────────────────────────────────────────────────────────

function showRebalanceAmountStatus(msg, isError) {
  const node = el('rebalance-amount-status');
  if (!node) return;
  node.textContent = msg;
  node.style.color = isError ? '#f87171' : '#34d399';
  node.style.display = msg ? 'block' : 'none';
}

el('btn-set-rebalance-amount')?.addEventListener('click', async () => {
  const contractAddress = (el('contract-address')?.value || '').trim();
  const btn = el('btn-set-rebalance-amount');
  const raw = (el('rebalance-amount')?.value || '').trim();

  if (!contractAddress) {
    showRebalanceAmountStatus('Contract address not yet loaded — select a network first.', true);
    return;
  }

  let amountWei;
  try {
    amountWei = ethers.parseEther(raw);
  } catch {
    showRebalanceAmountStatus('Enter a valid ETH amount (e.g. 0.05).', true);
    return;
  }

  btn.disabled = true;
  showRebalanceAmountStatus('Connecting wallet…', false);

  try {
    const provider = await connectWallet();
    const signer = await provider.getSigner();
    const contract = new ethers.Contract(contractAddress, SET_REBALANCE_AMOUNT_ABI, signer);

    showRebalanceAmountStatus('Waiting for signature…', false);
    const tx = await contract.setRebalanceAmount(amountWei);

    showRebalanceAmountStatus('Transaction submitted: ' + tx.hash + '. Waiting for confirmation…', false);
    await tx.wait();

    showRebalanceAmountStatus('Done. Rebalance amount set to ' + ethers.formatEther(amountWei) + ' ETH', false);
  } catch (e) {
    showRebalanceAmountStatus('Error: ' + (e?.reason || e?.message || String(e)), true);
  } finally {
    btn.disabled = false;
  }
});

// ── getChainConfigKeys ────────────────────────────────────────────────────────

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

// ── getLitActionClientConfig ──────────────────────────────────────────────────

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

// ── nodeConfigurationValues ───────────────────────────────────────────────────

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

function escapeHtml(str) {
  return String(str).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;');
}

function showNodeConfigStatus(msg, isError) {
  const node = el('node-config-status');
  if (!node) return;
  node.textContent = msg;
  node.style.color = isError ? '#f87171' : '#34d399';
  node.style.display = msg ? 'block' : 'none';
}

el('btn-set-node-config')?.addEventListener('click', async () => {
  const contractAddress = (el('contract-address')?.value || '').trim();
  const key   = (el('node-config-key')?.value   || '').trim();
  const value = (el('node-config-value')?.value || '').trim();
  const btn   = el('btn-set-node-config');

  if (!contractAddress) {
    showNodeConfigStatus('Contract address not yet loaded — select a network first.', true);
    return;
  }
  if (!key) {
    showNodeConfigStatus('Enter a configuration key.', true);
    return;
  }

  btn.disabled = true;
  showNodeConfigStatus('Connecting wallet…', false);

  try {
    const provider = await connectWallet();
    const signer   = await provider.getSigner();
    const contract = new ethers.Contract(contractAddress, SET_NODE_CONFIGURATION_ABI, signer);

    showNodeConfigStatus('Waiting for signature…', false);
    const tx = await contract.setNodeConfiguration(key, value);

    showNodeConfigStatus('Transaction submitted: ' + tx.hash + '. Waiting for confirmation…', false);
    await tx.wait();

    showNodeConfigStatus('Done. Configuration key "' + key + '" set.', false);
    el('node-config-key').value   = '';
    el('node-config-value').value = '';
    await fetchNodeConfigValues();
  } catch (e) {
    showNodeConfigStatus('Error: ' + (e?.reason || e?.message || String(e)), true);
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

el('btn-set-payer-count')?.addEventListener('click', async () => {
  const select = el('payer-count');
  const contractAddress = (el('contract-address')?.value || '').trim();
  const newCount = Number(select?.value);
  const btn = el('btn-set-payer-count');

  if (!contractAddress) {
    showSignerCountStatus('Contract address not yet loaded — select a network first.', true);
    return;
  }
  if (!newCount) {
    showSignerCountStatus('Select a value first.', true);
    return;
  }

  btn.disabled = true;
  select.disabled = true;
  showSignerCountStatus('Connecting wallet…', false);

  try {
    const provider = await connectWallet();
    const signer = await provider.getSigner();
    const contract = new ethers.Contract(contractAddress, SET_REQUESTED_API_PAYER_COUNT_ABI, signer);

    showSignerCountStatus('Waiting for signature…', false);
    const tx = await contract.setRequestedApiPayerCount(newCount);

    showSignerCountStatus('Transaction submitted: ' + tx.hash + '. Waiting for confirmation…', false);
    await tx.wait();

    showSignerCountStatus('Done. Requested payer count updated to ' + newCount, false);
    await refreshBalances();
  } catch (e) {
    showSignerCountStatus('Error: ' + (e?.reason || e?.message || String(e)), true);
  } finally {
    btn.disabled = false;
    select.disabled = false;
  }
});

