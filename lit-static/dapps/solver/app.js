/**
 * Solver dapp – Web3 app using MetaMask to sign fulfill (newQuote) transactions.
 * Uses swaps_sdk for reading open swap requests and quotes. 3 pages: Swap requests, Quotes, Fulfill.
 */

const NEW_QUOTE_ABI = [
  {
    inputs: [
      { internalType: 'uint256', name: 'swapRequestId', type: 'uint256' },
      { internalType: 'address', name: 'providerRefundAddress', type: 'address' },
    ],
    name: 'newQuote',
    outputs: [{ internalType: 'uint256', name: '', type: 'uint256' }],
    stateMutability: 'nonpayable',
    type: 'function',
  },
];

let swapsClient = null;
let state = {
  baseUrl: 'http://localhost:8000',
  provider: null,
  signer: null,
  walletAddress: null,
};

function getBaseUrl() {
  return (document.getElementById('baseUrl')?.value || 'http://localhost:8000').trim().replace(/\/$/, '');
}

function initClients() {
  const baseUrl = getBaseUrl();
  if (state.baseUrl !== baseUrl) {
    state.baseUrl = baseUrl;
    swapsClient = null;
  }
}

async function getSwapsClient() {
  initClients();
  if (!swapsClient) {
    const { createSwapsClient } = await import('../../swaps_sdk.js');
    swapsClient = createSwapsClient(state.baseUrl);
  }
  return swapsClient;
}

function formatAddress(str) {
  if (!str || typeof str !== 'string') return '–';
  const s = String(str).trim();
  if (s.length <= 16) return s;
  return `${s.slice(0, 6)}...${s.slice(-4)}`;
}

/** Format swap origin/destination amount for display. Amounts are in ether (no conversion). */
function formatSwapAmount(val) {
  if (val == null || val === '') return '–';
  const n = Number(val);
  if (!Number.isFinite(n) || n < 0) return '–';
  if (n === 0) return '0';
  if (n < 1e-6 && n > 0) return n.toFixed(10).replace(/\.?0+$/, '') || '0';
  return n.toFixed(6).replace(/\.?0+$/, '') || '0';
}

// --- Routing ---
function getPageFromHash() {
  const hash = (window.location.hash || '#swap-requests').slice(1);
  return hash === '' ? 'swap-requests' : hash;
}

function setPage(pageId) {
  document.querySelectorAll('.page').forEach((p) => p.classList.remove('active'));
  document.querySelectorAll('.nav a').forEach((a) => a.classList.remove('active'));
  const page = document.getElementById(`page-${pageId}`);
  const link = document.querySelector(`.nav a[data-page="${pageId}"]`);
  if (page) page.classList.add('active');
  if (link) link.classList.add('active');
  window.location.hash = pageId;
  if (pageId === 'swap-requests') loadSwapRequests();
  if (pageId === 'quotes') loadQuotes();
  if (pageId === 'fulfill') loadFulfillContractAddress();
}

function initRouting() {
  function applyRoute() {
    setPage(getPageFromHash());
  }
  window.addEventListener('hashchange', applyRoute);
  applyRoute();
  document.querySelectorAll('.nav a').forEach((a) => {
    a.addEventListener('click', (e) => {
      e.preventDefault();
      setPage(a.getAttribute('data-page'));
    });
  });
}

// --- MetaMask ---
function getEthers() {
  if (typeof window.ethers === 'undefined') {
    throw new Error('ethers not loaded. Ensure the ethers script is loaded.');
  }
  return window.ethers;
}

function updateWalletUI() {
  const addrEl = document.getElementById('wallet-address');
  const connectBtn = document.getElementById('btn-connect-wallet');
  const disconnectBtn = document.getElementById('btn-disconnect-wallet');
  const fulfillRefund = document.getElementById('fulfill-provider-refund');
  if (state.walletAddress) {
    if (addrEl) {
      addrEl.textContent = formatAddress(state.walletAddress);
      addrEl.style.display = 'inline';
    }
    if (connectBtn) connectBtn.style.display = 'none';
    if (disconnectBtn) disconnectBtn.style.display = 'inline-flex';
    if (fulfillRefund) fulfillRefund.placeholder = 'Leave empty to use your connected wallet';
  } else {
    if (addrEl) addrEl.style.display = 'none';
    if (connectBtn) connectBtn.style.display = 'inline-flex';
    if (disconnectBtn) disconnectBtn.style.display = 'none';
    if (fulfillRefund) fulfillRefund.placeholder = '0x... (defaults to your wallet)';
  }
}

async function connectWallet() {
  const btn = document.getElementById('btn-connect-wallet');
  if (btn) btn.disabled = true;
  try {
    const ethers = getEthers();
    const provider = new ethers.BrowserProvider(window.ethereum);
    const accounts = await provider.send('eth_requestAccounts', []);
    if (!accounts || accounts.length === 0) {
      showFulfillStatus('No accounts returned from MetaMask.', 'error');
      return;
    }
    const signer = await provider.getSigner();
    const address = await signer.getAddress();
    state.provider = provider;
    state.signer = signer;
    state.walletAddress = address;
    updateWalletUI();
  } catch (err) {
    console.error('MetaMask connect error:', err);
    showFulfillStatus(err.message || String(err), 'error');
  } finally {
    if (btn) btn.disabled = false;
  }
}

function disconnectWallet() {
  state.provider = null;
  state.signer = null;
  state.walletAddress = null;
  updateWalletUI();
}

// --- Swap requests list ---
function showSwapRequestsStatus(msg, type = 'info') {
  const el = document.getElementById('swap-requests-status');
  if (!el) return;
  el.textContent = msg;
  el.className = `status ${type}`;
  el.style.display = msg ? 'block' : 'none';
}

function renderSwapRequestRow(id, sr) {
  const tr = document.createElement('tr');
  tr.className = 'swap-requests-row';
  const from = sr.from || '–';
  const origin = `${formatSwapAmount(sr.origin_amount)} ${sr.origin_symbol ?? ''} on ${sr.origin_chain ?? ''}`;
  const dest = `${formatSwapAmount(sr.destination_amount)} ${sr.destination_symbol ?? ''} on ${sr.destination_chain ?? ''}`;
  tr.innerHTML = `
    <td class="swap-requests-num">${id}</td>
    <td class="swap-requests-from mono">${formatAddress(from)}</td>
    <td class="swap-requests-origin">${origin}</td>
    <td class="swap-requests-dest">${dest}</td>
    <td class="swap-requests-slippage">${sr.slippage ?? '–'}</td>
    <td class="swap-requests-action"><button type="button" class="btn btn-secondary btn-fulfill-sr" data-id="${id}">Fulfill</button></td>
  `;
  return tr;
}

async function loadSwapRequests() {
  const tableWrap = document.getElementById('swap-requests-table-wrap');
  const tbody = document.getElementById('swap-requests-tbody');
  const emptyEl = document.getElementById('swap-requests-empty');
  if (!tbody || !emptyEl) return;
  tbody.innerHTML = '';
  emptyEl.textContent = 'Loading…';
  emptyEl.style.display = 'block';
  if (tableWrap) tableWrap.style.display = 'none';
  showSwapRequestsStatus('');
  try {
    const client = await getSwapsClient();
    const res = await client.getOpenSwapRequests();
    const list = res.swap_requests || [];
    if (list.length === 0) {
      emptyEl.textContent = 'No open swap requests.';
      emptyEl.style.display = 'block';
      return;
    }
    emptyEl.style.display = 'none';
    if (tableWrap) tableWrap.style.display = 'block';
    list.forEach((sr, i) => {
      const id = i + 1;
      tbody.appendChild(renderSwapRequestRow(id, sr));
    });
    tbody.querySelectorAll('.btn-fulfill-sr').forEach((btn) => {
      btn.addEventListener('click', () => {
        const id = btn.getAttribute('data-id');
        document.getElementById('fulfill-swap-request-id').value = id;
        setPage('fulfill');
      });
    });
  } catch (err) {
    emptyEl.textContent = '';
    emptyEl.style.display = 'none';
    if (tableWrap) tableWrap.style.display = 'none';
    showSwapRequestsStatus(err.message || String(err), 'error');
  }
}

// --- Quotes list ---
function showQuotesStatus(msg, type = 'info') {
  const el = document.getElementById('quotes-status');
  if (!el) return;
  el.textContent = msg;
  el.className = `status ${type}`;
  el.style.display = msg ? 'block' : 'none';
}

function renderQuoteRow(idx, q) {
  const tr = document.createElement('tr');
  tr.className = 'quotes-table-row';
  const expiry = q.quote_expiry ? new Date(q.quote_expiry * 1000).toISOString() : '–';
  const sr = q.swap_request_data;
  const pkpAddress = sr?.pkp_address ?? '';
  const origin =
    sr
      ? `${formatSwapAmount(sr.origin_amount)} ${sr.origin_symbol ?? ''} on ${sr.origin_chain ?? ''}`.trim() || '–'
      : '–';
  const destination =
    sr
      ? `${formatSwapAmount(sr.destination_amount)} ${sr.destination_symbol ?? ''} on ${sr.destination_chain ?? ''}`.trim() || '–'
      : '–';
  const pkpCell = pkpAddress
    ? `<div class="quotes-table-pkp-cell-inner"><span class="quotes-table-pkp mono">${formatAddress(pkpAddress)}</span><button type="button" class="btn btn-copy btn-copy-pkp" data-pkp-address="${pkpAddress}" title="Copy PKP address">Copy</button></div>`
    : '–';
  tr.innerHTML = `
    <td class="quotes-table-num">${idx}</td>
    <td class="quotes-table-from mono">${formatAddress(sr?.from)}</td>
    <td class="quotes-table-pkp-cell">${pkpCell}</td>
    <td class="quotes-table-origin">${origin}</td>
    <td class="quotes-table-dest">${destination}</td>
    <td class="quotes-table-refund mono">${formatAddress(q.provider_refund_address)}</td>
    <td class="quotes-table-expiry">${expiry}</td>
    <td class="quotes-table-action"><button type="button" class="btn btn-secondary btn-process-swap">Process swap</button></td>
  `;
  return tr;
}

async function loadQuotes() {
  const tableWrap = document.getElementById('quotes-table-wrap');
  const tbody = document.getElementById('quotes-tbody');
  const emptyEl = document.getElementById('quotes-empty');
  if (!tbody || !emptyEl) return;
  tbody.innerHTML = '';
  emptyEl.textContent = 'Loading…';
  emptyEl.style.display = 'block';
  if (tableWrap) tableWrap.style.display = 'none';
  showQuotesStatus('');
  try {
    const client = await getSwapsClient();
    const res = await client.getOpenQuotes();
    const list = res.quotes || [];
    if (list.length === 0) {
      emptyEl.textContent = 'No open quotes.';
      emptyEl.style.display = 'block';
      return;
    }
    emptyEl.style.display = 'none';
    if (tableWrap) tableWrap.style.display = 'block';
    list.forEach((q, i) => {
      tbody.appendChild(renderQuoteRow(i + 1, q));
    });
    tbody.querySelectorAll('.btn-process-swap').forEach((btn) => {
      btn.addEventListener('click', () => setPage('attempt-swap'));
    });
  } catch (err) {
    emptyEl.textContent = '';
    emptyEl.style.display = 'none';
    if (tableWrap) tableWrap.style.display = 'none';
    showQuotesStatus(err.message || String(err), 'error');
  }
}

// --- Fulfill (MetaMask signs newQuote) ---
function showFulfillStatus(msg, type = 'info') {
  const el = document.getElementById('fulfill-status');
  if (!el) return;
  el.textContent = msg;
  el.className = `status ${type}`;
  el.style.display = msg ? 'block' : 'none';
}

/** Load quote storage contract address from API and pre-fill the fulfill form. */
async function loadFulfillContractAddress() {
  const input = document.getElementById('fulfill-contract-address');
  if (!input) return;
  try {
    const client = await getSwapsClient();
    const address = await client.getContractAddress();
    if (address && typeof address === 'string') {
      input.value = address.trim();
      input.readOnly = true;
      input.title = 'Fetched from get_contract_address';
      showFulfillStatus('');
    } else {
      input.readOnly = false;
      showFulfillStatus('Could not load contract address. Enter it manually.', 'error');
    }
  } catch (err) {
    input.value = '';
    input.readOnly = false;
    input.title = '';
    showFulfillStatus(`Contract address: ${err.message}. Enter it manually.`, 'error');
  }
}

async function submitFulfill(e) {
  e.preventDefault();
  const contractAddress = document.getElementById('fulfill-contract-address')?.value?.trim();
  const swapRequestIdStr = document.getElementById('fulfill-swap-request-id')?.value?.trim();
  let providerRefund = document.getElementById('fulfill-provider-refund')?.value?.trim();

  if (!contractAddress) {
    showFulfillStatus('Contract address is missing. Open the Fulfill page again to load it from the API, or enter it manually.', 'error');
    return;
  }
  if (!swapRequestIdStr) {
    showFulfillStatus('Enter a swap request ID.', 'error');
    return;
  }

  if (!state.signer) {
    showFulfillStatus('Connect Wallet first (click Connect Wallet in the header).', 'error');
    return;
  }

  if (!providerRefund) {
    providerRefund = state.walletAddress;
  }
  if (!providerRefund || !providerRefund.startsWith('0x')) {
    showFulfillStatus('Provide a valid provider refund address (0x...) or use your connected wallet.', 'error');
    return;
  }

  const btn = document.getElementById('btn-fulfill');
  btn.disabled = true;
  showFulfillStatus('Confirm the transaction in MetaMask…', 'info');

  try {
    const ethers = getEthers();
    const contract = new ethers.Contract(contractAddress, NEW_QUOTE_ABI, state.signer);
    const swapRequestId = BigInt(swapRequestIdStr);
    const tx = await contract.newQuote(swapRequestId, providerRefund);
    showFulfillStatus('Transaction sent. Waiting for confirmation…', 'info');
    const receipt = await tx.wait();
    const txHash = receipt?.hash || tx?.hash || 'N/A';
    showFulfillStatus(`Quote created. Transaction: ${txHash}`, 'success');
    loadQuotes();
  } catch (err) {
    console.error('Fulfill error:', err);
    showFulfillStatus(err.message || String(err), 'error');
  } finally {
    btn.disabled = false;
  }
}

// --- Attempt swap (calls attempt_swap_request) ---
function showAttemptSwapStatus(msg, type = 'info') {
  const el = document.getElementById('attempt-swap-status');
  if (!el) return;
  el.textContent = msg;
  el.className = `status ${type}`;
  el.style.display = msg ? 'block' : 'none';
}

/** Format wei as ETH for display on Process Swap quote balances. */
function formatWeiToEth(weiStr) {
  if (!weiStr || weiStr === '0') return '0 ETH';
  try {
    const w = BigInt(weiStr);
    const eth = Number(w) / 1e18;
    if (eth === 0 && w !== 0n) {
      const small = (Number(w) / 1e18).toFixed(8).replace(/\.?0+$/, '') || '0';
      return `${small} ETH`;
    }
    const s = eth.toFixed(6).replace(/\.?0+$/, '') || '0';
    return `${s} ETH`;
  } catch {
    return `${weiStr} wei`;
  }
}

async function fetchQuoteBalances() {
  const quoteIdInput = document.getElementById('attempt-swap-quote-id');
  const quoteId = quoteIdInput?.value?.trim();
  const balancesEl = document.getElementById('attempt-swap-balances');
  if (!quoteId) {
    showAttemptSwapStatus('Enter a quote ID to get balances.', 'error');
    if (balancesEl) balancesEl.style.display = 'none';
    return;
  }

  showAttemptSwapStatus('Fetching quote balances…', 'info');
  if (balancesEl) balancesEl.style.display = 'none';

  try {
    const client = await getSwapsClient();
    const data = await client.getQuoteBalances(quoteId);
    showAttemptSwapStatus('');
    if (balancesEl) {
      const srcSufficient = data.source_balance_sufficient === true;
      const dstSufficient = data.destination_balance_sufficient === true;
      const srcIcon = srcSufficient ? '<span class="balance-sufficient" title="Balance sufficient">✓</span>' : '<span class="balance-insufficient" title="Insufficient balance">✗</span>';
      const dstIcon = dstSufficient ? '<span class="balance-sufficient" title="Balance sufficient">✓</span>' : '<span class="balance-insufficient" title="Insufficient balance">✗</span>';
      balancesEl.innerHTML = `
        <div class="card-title" style="margin-top: 0.5rem;">Quote balances (PKP address)</div>
        <div class="quote-balances-row"><span class="key">PKP address</span><span class="val mono">${formatAddress(data.pkp_address)}</span></div>
        <div class="quote-balances-row"><span class="key">Source (${data.source_chain})</span><span class="val quote-balance-val">${formatWeiToEth(data.source_balance_wei)} ${srcIcon}</span></div>
        <div class="quote-balances-row"><span class="key">Destination (${data.destination_chain})</span><span class="val quote-balance-val">${formatWeiToEth(data.destination_balance_wei)} ${dstIcon}</span></div>
      `;
      balancesEl.style.display = 'block';
    }
  } catch (err) {
    showAttemptSwapStatus(err.message || String(err), 'error');
    if (balancesEl) balancesEl.style.display = 'none';
  }
}

async function submitAttemptSwap(e) {
  e.preventDefault();
  const quoteIdInput = document.getElementById('attempt-swap-quote-id');
  const quoteId = quoteIdInput?.value?.trim();
  if (!quoteId) {
    showAttemptSwapStatus('Enter a quote ID.', 'error');
    return;
  }

  const btn = document.getElementById('btn-attempt-swap');
  if (btn) btn.disabled = true;
  showAttemptSwapStatus('Calling attempt_swap_request…', 'info');

  try {
    const client = await getSwapsClient();
    await client.attemptSwapRequest(quoteId);
    showAttemptSwapStatus('Swap attempt completed successfully.', 'success');
  } catch (err) {
    showAttemptSwapStatus(err.message || String(err), 'error');
  } finally {
    if (btn) btn.disabled = false;
  }
}

// --- Init ---
function init() {
  initRouting();

  document.getElementById('baseUrl')?.addEventListener('change', () => {
    initClients();
  });

  document.getElementById('btn-connect-wallet')?.addEventListener('click', connectWallet);
  document.getElementById('btn-disconnect-wallet')?.addEventListener('click', disconnectWallet);

  document.getElementById('btn-refresh-swap-requests')?.addEventListener('click', loadSwapRequests);
  document.getElementById('btn-refresh-quotes')?.addEventListener('click', loadQuotes);

  document.getElementById('fulfill-form')?.addEventListener('submit', submitFulfill);
  document.getElementById('attempt-swap-form')?.addEventListener('submit', submitAttemptSwap);
  document.getElementById('btn-get-quote-balances')?.addEventListener('click', fetchQuoteBalances);

  document.getElementById('quotes-tbody')?.addEventListener('click', async (e) => {
    const btn = e.target.closest('.btn-copy-pkp');
    if (!btn) return;
    const addr = btn.getAttribute('data-pkp-address');
    if (!addr) return;
    try {
      await navigator.clipboard.writeText(addr);
      const label = btn.textContent;
      btn.textContent = 'Copied!';
      btn.disabled = true;
      setTimeout(() => {
        btn.textContent = label;
        btn.disabled = false;
      }, 1500);
    } catch (err) {
      console.error('Copy failed:', err);
    }
  });

  updateWalletUI();
}

init();
