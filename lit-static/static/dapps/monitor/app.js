/**
 * Lit Admin – read AccountConfig contract view functions directly.
 * ABI subset matches lit-api-server/src/accounts/contracts/AccountConfig.json (view functions only).
 */

const SET_API_PAYER_ABI = [
  {
    inputs: [{ internalType: 'address', name: 'newApiPayer', type: 'address' }],
    name: 'setApiPayer',
    outputs: [],
    stateMutability: 'nonpayable',
    type: 'function',
  },
];

const ACCOUNT_CONFIG_VIEW_ABI = [
  {
    inputs: [],
    name: 'owner',
    outputs: [{ internalType: 'address', name: '', type: 'address' }],
    stateMutability: 'view',
    type: 'function',
  },
  {
    inputs: [],
    name: 'api_payer',
    outputs: [{ internalType: 'address', name: '', type: 'address' }],
    stateMutability: 'view',
    type: 'function',
  },
  {
    inputs: [],
    name: 'pricing_operator',
    outputs: [{ internalType: 'address', name: '', type: 'address' }],
    stateMutability: 'view',
    type: 'function',
  },
  {
    inputs: [],
    name: 'nextAccountCount',
    outputs: [{ internalType: 'uint256', name: '', type: 'uint256' }],
    stateMutability: 'view',
    type: 'function',
  },
  {
    inputs: [],
    name: 'nextWalletCount',
    outputs: [{ internalType: 'uint256', name: '', type: 'uint256' }],
    stateMutability: 'view',
    type: 'function',
  },
  {
    inputs: [{ internalType: 'uint256', name: 'index', type: 'uint256' }],
    name: 'indexToAccountHashAt',
    outputs: [{ internalType: 'uint256', name: '', type: 'uint256' }],
    stateMutability: 'view',
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

// RPC URL populated by getNodeChainConfig; used by fetchContractValues.
let currentRpcUrl = '';

// ── Network selector ──────────────────────────────────────────────────────────

function getServerUrl() {
  // Option values include trailing slash; strip it for consistent path joining.
  return (el('network')?.value || '').replace(/\/$/, '');
}

// ── getNodeChainConfig ────────────────────────────────────────────────────────

async function getNodeChainConfig(serverUrl) {
  const resultsEl = el('chain-config-results');
  const errEl = el('chain-config-error');

  if (errEl) errEl.style.display = 'none';
  if (resultsEl) resultsEl.style.display = 'block';
  ['cc-chain-name','cc-chain-id','cc-is-evm','cc-testnet','cc-token','cc-rpc-url','cc-contract-address']
    .forEach(id => setValue(id, '…', false));

  try {
    const res = await fetch(`${serverUrl}/get_node_chain_config`);
    if (!res.ok) throw new Error(`HTTP ${res.status} ${res.statusText}`);
    const cfg = await res.json();

    setValue('cc-chain-name',       cfg.chain_name       ?? '—', !cfg.chain_name);
    setValue('cc-chain-id',         cfg.chain_id != null ? String(cfg.chain_id) : '—', cfg.chain_id == null);
    setValue('cc-is-evm',           cfg.is_evm   != null ? String(cfg.is_evm)   : '—', cfg.is_evm   == null);
    setValue('cc-testnet',          cfg.testnet  != null ? String(cfg.testnet)  : '—', cfg.testnet  == null);
    setValue('cc-token',            cfg.token        ?? '—', !cfg.token);
    setValue('cc-rpc-url',          cfg.rpc_url      ?? '—', !cfg.rpc_url);
    setValue('cc-contract-address', cfg.contract_address ?? '—', !cfg.contract_address);

    // Propagate values to the contract card.
    currentRpcUrl = cfg.rpc_url ?? '';
    const contractInput = el('contract-address');
    if (contractInput && cfg.contract_address) contractInput.value = cfg.contract_address;
  } catch (e) {
    currentRpcUrl = '';
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
      listEl.innerHTML = '<span style="color:var(--muted);font-family:\'JetBrains Mono\',monospace;font-size:0.85rem">No payers returned.</span>';
      return;
    }

    listEl.innerHTML = payers.map((addr, i) =>
      `<div class="result-row">` +
        `<span class="result-label">Payer ${i + 1}</span>` +
        `<span class="result-value">${addr}</span>` +
      `</div>`
    ).join('');
  } catch (e) {
    if (resultsEl) resultsEl.style.display = 'none';
    if (errEl) { errEl.textContent = e?.message || String(e); errEl.style.display = 'block'; }
  }
}

// ── loadNetwork ───────────────────────────────────────────────────────────────

async function loadNetwork() {
  const serverUrl = getServerUrl();
  await Promise.all([
    getNodeChainConfig(serverUrl),
    getApiPayers(serverUrl),
  ]);
  await fetchContractValues();
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
  const rpcUrl = currentRpcUrl;
  const contractAddress = (el('contract-address')?.value || '').trim();
  const results = el('results');

  if (!rpcUrl || !contractAddress) {
    showError('RPC URL and contract address not yet loaded — select a network first.');
    return;
  }

  hideError();
  setValue('val-owner', '…', false);
  setValue('val-api-payer', '…', false);
  setValue('val-api-payer-balance', '…', false);
  setValue('val-pricing-operator', '…', false);
  setValue('val-next-account-count', '…', false);
  setValue('val-next-wallet-count', '…', false);
  setValue('val-index-to-account', '…', false);
  if (results) results.style.display = 'block';

  try {
    const provider = new ethers.JsonRpcProvider(rpcUrl);
    const contract = new ethers.Contract(contractAddress, ACCOUNT_CONFIG_VIEW_ABI, provider);

    const [owner, apiPayer, pricingOperator, nextAccountCount, nextWalletCount, indexToAccountHashAt1] = await Promise.all([
      contract.owner(),
      contract.api_payer(),
      contract.pricing_operator(),
      contract.nextAccountCount(),
      contract.nextWalletCount(),
      contract.indexToAccountHashAt(1),
    ]);

    setValue('val-owner', owner ?? '—', !owner);
    setValue('val-api-payer', apiPayer ?? '—', !apiPayer);

    if (apiPayer && apiPayer !== ethers.ZeroAddress) {
      const balanceWei = await provider.getBalance(apiPayer);
      setValue('val-api-payer-balance', ethers.formatEther(balanceWei) + ' ETH', false);
    } else {
      setValue('val-api-payer-balance', '—', true);
    }
    setValue('val-pricing-operator', pricingOperator ?? '—', !pricingOperator);
    setValue('val-next-account-count', nextAccountCount != null ? String(nextAccountCount) : '—', nextAccountCount == null);
    setValue('val-next-wallet-count', nextWalletCount != null ? String(nextWalletCount) : '—', nextWalletCount == null);
    setValue('val-index-to-account', indexToAccountHashAt1 != null ? String(indexToAccountHashAt1) : '—', indexToAccountHashAt1 == null);
  } catch (e) {
    showError(e?.message || String(e));
  }
}

// ── setApiPayer ───────────────────────────────────────────────────────────────

function showWriteStatus(msg, isError) {
  const node = el('write-status');
  if (!node) return;
  node.textContent = msg;
  node.style.color = isError ? '#f87171' : '#34d399';
  node.style.display = msg ? 'block' : 'none';
}

async function connectWallet() {
  if (!window.ethereum) {
    showWriteStatus('Wallet not found. Install a wallet and try again.', true);
    return null;
  }
  try {
    const provider = new ethers.BrowserProvider(window.ethereum);
    await provider.send('eth_requestAccounts', []);
    return provider;
  } catch (e) {
    showWriteStatus('Wallet connection rejected: ' + (e?.message || String(e)), true);
    return null;
  }
}

async function setApiPayer() {
  const contractAddress = (el('contract-address')?.value || '').trim();
  const newApiPayerString = (el('new-api-payer')?.value || '').trim();
  const newApiPayer = ethers.getAddress(newApiPayerString);
  const btn = el('btn-set-api-payer');

  if (!contractAddress) {
    showWriteStatus('Contract address not yet loaded — select a network first.', true);
    return;
  }
  if (!ethers.isAddress(newApiPayer)) {
    showWriteStatus('Enter a valid Ethereum address for the new api_payer.', true);
    return;
  }

  showWriteStatus('Connecting wallet…', false);
  if (btn) btn.disabled = true;

  try {
    const provider = await connectWallet();
    if (!provider) return;

    const signer = await provider.getSigner();
    const contract = new ethers.Contract(contractAddress, SET_API_PAYER_ABI, signer);

    showWriteStatus('Waiting for signature…', false);
    const tx = await contract.setApiPayer(newApiPayer);

    showWriteStatus('Transaction submitted: ' + tx.hash + '. Waiting for confirmation…', false);
    await tx.wait();

    showWriteStatus('Done. api_payer updated to ' + newApiPayer, false);
    el('new-api-payer').value = '';

    fetchContractValues();
  } catch (e) {
    showWriteStatus('Error: ' + (e?.reason || e?.message || String(e)), true);
  } finally {
    if (btn) btn.disabled = false;
  }
}

el('btn-set-api-payer')?.addEventListener('click', setApiPayer);
