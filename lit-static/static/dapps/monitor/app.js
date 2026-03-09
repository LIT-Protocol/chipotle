/**
 * Lit Admin – read AccountConfig contract view functions directly.
 * ABI subset matches lit-api-server/src/accounts/contracts/AccountConfig.json (view functions only).
 */

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

    if (currentRpcUrl) {
      const provider = new ethers.JsonRpcProvider(currentRpcUrl);
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
  setValue('val-owner-balance', '', false);
  setValue('val-pricing-operator', '…', false);
  setValue('val-pricing-operator-balance', '', false);
  setValue('val-admin-api-payer', '…', false);
  setValue('val-admin-api-payer-balance', '', false);
  setValue('val-payer-count', '…', false);
  setValue('val-requested-api-payer-count', '…', false);
  if (results) results.style.display = 'block';

  try {
    const provider = new ethers.JsonRpcProvider(rpcUrl);
    const contract = new ethers.Contract(contractAddress, ACCOUNT_CONFIG_VIEW_ABI, provider);

    const [owner, pricingOperator, adminApiPayer, apiPayerCount, requestedApiPayerCount, rebalanceAmountWei] = await Promise.all([
      contract.owner(),
      contract.pricingOperator(),
      contract.adminApiPayerAccount(),
      contract.apiPayerCount(),
      contract.requestedApiPayerCount(),
      contract.rebalanceAmount(),
    ]);

    setValue('val-owner', owner ?? '—', !owner);
    setValue('val-pricing-operator', pricingOperator ?? '—', !pricingOperator);
    setValue('val-admin-api-payer', adminApiPayer ?? '—', !adminApiPayer);
    setValue('val-payer-count', String(apiPayerCount), false);
    setValue('val-requested-api-payer-count', String(requestedApiPayerCount), false);
    populateApiPayerCountDropdown(Number(requestedApiPayerCount));

    const rebalanceInput = el('rebalance-amount');
    if (rebalanceInput) rebalanceInput.value = ethers.formatEther(rebalanceAmountWei);

    // Fetch balances asynchronously so they don't block the main display.
    const balanceTargets = [
      [owner,        'val-owner-balance'],
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
  } catch (e) {
    showSignerCountStatus('Error: ' + (e?.reason || e?.message || String(e)), true);
  } finally {
    btn.disabled = false;
    select.disabled = false;
  }
});

