/**
 * Swps dapp – interfaces with core_sdk and transfer_sdk.
 * Lit Protocol styling; 4 pages: Overview, Transfer, Swap, History.
 */

const HISTORY_KEY = 'swps_tx_history';

let coreClient = null;
let transferClient = null;
let swapsClient = null;
let state = {
  baseUrl: 'http://localhost:8000',
  apiKey: null,
  pkpPublicKey: null,
  walletAddress: null,
  pkpAddress: null, // EVM address derived from PKP (from balance API response)
  isTestnet: true, // when true, getAllChains uses isTestnet: true (testnet EVM chains)
  chainList: [], // { chain, display_name, token }[] from getAllChains({ isEvm: true, isTestnet: state.isTestnet })
};

function getBaseUrl() {
  return (document.getElementById('baseUrl').value || 'http://localhost:8000').trim().replace(/\/$/, '');
}

/** Format address or key as first 8 + "..." + last 8. */
function formatAddress(str) {
  if (!str || typeof str !== 'string') return '–';
  const s = str.trim();
  if (s.length <= 16) return s;
  return `${s.slice(0, 8)}...${s.slice(-8)}`;
}

/** Format a balance for display with at most 6 decimal places. */
function formatBalanceMax6(val) {
  if (val == null || val === '') return '–';
  const s = String(val).trim();
  if (!s) return '–';
  try {
    const n = Number(s);
    if (!Number.isFinite(n)) return s;
    if (n >= 1e12 && Number.isInteger(n)) {
      const eth = n / 1e18;
      return eth.toFixed(6).replace(/\.?0+$/, '') || '0';
    }
    return n.toFixed(6).replace(/\.?0+$/, '') || '0';
  } catch {
    return s;
  }
}

function initClients() {
  const baseUrl = getBaseUrl();
  if (state.baseUrl !== baseUrl) {
    state.baseUrl = baseUrl;
    coreClient = null;
    transferClient = null;
    swapsClient = null;
  }
}

async function getCoreClient() {
  initClients();
  if (!coreClient) {
    const { createClient } = await import('../../core_sdk.js');
    coreClient = createClient(state.baseUrl);
  }
  return coreClient;
}

async function getTransferClient() {
  initClients();
  if (!transferClient) {
    const { createTransferClient } = await import('../../transfer_sdk.js');
    transferClient = createTransferClient(state.baseUrl);
  }
  return transferClient;
}

async function getSwapsClient() {
  initClients();
  if (!swapsClient) {
    const { createSwapsClient } = await import('../../swaps_sdk.js');
    swapsClient = createSwapsClient(state.baseUrl);
  }
  return swapsClient;
}

// --- Routing ---
function getPageFromHash() {
  const hash = (window.location.hash || '#overview').slice(1);
  return hash === '' ? 'overview' : hash;
}

function setPage(pageId) {
  document.querySelectorAll('.page').forEach((p) => p.classList.remove('active'));
  document.querySelectorAll('.nav a').forEach((a) => a.classList.remove('active'));
  const page = document.getElementById(`page-${pageId}`);
  const link = document.querySelector(`.nav a[data-page="${pageId}"]`);
  if (page) page.classList.add('active');
  if (link) link.classList.add('active');
  window.location.hash = pageId;
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

// --- Chains (getAllChains: isEvm=true, isTestnet from toggle) ---
async function loadChains() {
  try {
    const client = await getTransferClient();
    const res = await client.getAllChains({ isEvm: true, isTestnet: state.isTestnet });
    state.chainList = res.chains || [];
    populateChainSelect('overview-chain', state.chainList);
    populateChainSelect('transfer-chain', state.chainList);
    populateSwapTokenSelects();
  } catch (err) {
    console.error('loadChains failed:', err);
    state.chainList = [];
    populateChainSelect('overview-chain', []);
    populateChainSelect('transfer-chain', []);
    populateSwapTokenSelects();
  }
}

/** Options for swap dropdowns: { value: "symbol|chainKey", text: "Symbol - Chain display name" } */
function getSwapTokenChainOptions() {
  const chains = state.chainList || [];
  return chains
    .filter((c) => c.token)
    .map((c) => ({
      value: `${c.token}|${c.chain}`,
      text: `${c.token} - ${c.display_name}`,
    }));
}

function populateSwapTokenSelects() {
  const options = getSwapTokenChainOptions();
  const list = options.length ? options : [{ value: '', text: 'No tokens loaded' }];
  const fromSel = document.getElementById('swap-token-from');
  const toSel = document.getElementById('swap-token-to');
  [fromSel, toSel].forEach((sel) => {
    if (!sel) return;
    sel.innerHTML = '';
    list.forEach((o) => {
      const opt = document.createElement('option');
      opt.value = o.value;
      opt.textContent = o.text;
      sel.appendChild(opt);
    });
  });
  if (fromSel && list[0]?.value) fromSel.value = list[0].value;
  if (toSel && list.length > 1) toSel.value = list[1].value;
  else if (toSel && list[0]?.value) toSel.value = list[0].value;
}

/** Parse "symbol|chainKey" value from swap token select into { symbol, chain }. */
function parseSwapTokenValue(value) {
  if (!value || !value.includes('|')) return { symbol: '', chain: '' };
  const [symbol, chain] = value.split('|');
  return { symbol: symbol?.trim() || '', chain: chain?.trim() || '' };
}

function populateChainSelect(selectId, chains) {
  const sel = document.getElementById(selectId);
  if (!sel) return;
  sel.innerHTML = '';
  if (!chains.length) {
    const opt = document.createElement('option');
    opt.value = '';
    opt.textContent = 'No chains loaded';
    sel.appendChild(opt);
    return;
  }
  chains.forEach((c) => {
    const opt = document.createElement('option');
    opt.value = c.chain;
    opt.textContent = c.display_name;
    sel.appendChild(opt);
  });
}

// --- Overview ---
function showOverviewStatus(msg, type = 'info') {
  const el = document.getElementById('overview-status');
  el.textContent = msg;
  el.className = `status ${type}`;
  el.style.display = 'block';
}

function showOverviewAccount() {
  document.getElementById('overview-actions').style.display = 'none';
  const wrap = document.getElementById('overview-account');
  wrap.style.display = 'block';
  document.getElementById('overview-wallet').textContent = formatAddress(state.walletAddress);
  document.getElementById('overview-pkp').textContent = formatAddress(state.pkpPublicKey);
  const addrEl = document.getElementById('overview-pkp-address');
  if (addrEl) addrEl.textContent = formatAddress(state.pkpAddress);
  document.getElementById('overview-chain-wrap').style.display = 'block';
  updateCopyButtonStates();
  updateLogoutButtonVisibility();
  fetchPkpAddressIfNeeded();
}

function updateCopyButtonStates() {
  document.getElementById('btn-copy-wallet').disabled = !state.walletAddress;
  document.getElementById('btn-copy-pkp').disabled = !state.pkpPublicKey;
  document.getElementById('btn-copy-pkp-address').disabled = !state.pkpAddress;
}

async function copyToClipboard(text) {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch (_) {
    return false;
  }
}

async function createAccount() {
  const btn = document.getElementById('btn-create-account');
  btn.disabled = true;
  showOverviewStatus('Creating account…', 'info');
  try {
    const client = await getCoreClient();
    const { api_key, wallet_address } = await client.getApiKey();
    const { pkp_public_key } = await client.mintPkp(api_key);
    state.apiKey = api_key;
    state.pkpPublicKey = pkp_public_key;
    state.walletAddress = wallet_address;
    showOverviewAccount();
    showOverviewStatus('Account created. You can refresh balances below.', 'success');
    await refreshBalances();
  } catch (err) {
    showOverviewStatus(err.message || String(err), 'error');
  } finally {
    btn.disabled = false;
  }
}

const WALLET_BALANCE_CHAIN = 'yellowstone'; // Chain key for API; wallet balance always shown for Yellowstone (Litkey)

async function refreshBalances() {
  const chainEl = document.getElementById('overview-chain');
  const chain = (chainEl && chainEl.value ? chainEl.value : (state.chainList[0]?.chain || '')).trim();
  if (!chain) return;
  const wrap = document.getElementById('overview-balances');
  if (!state.apiKey && !state.pkpPublicKey) {
    wrap.innerHTML = '<p class="history-empty">Load an account first.</p>';
    return;
  }
  wrap.innerHTML = '<p class="history-empty">Loading…</p>';
  try {
    const client = await getTransferClient();
    const list = document.createElement('ul');
    list.className = 'balance-list';
    if (state.apiKey) {
      const bal = await client.getApiKeyBalance(state.apiKey, WALLET_BALANCE_CHAIN);
      if (bal.address) {
        state.walletAddress = bal.address;
        const walletEl = document.getElementById('overview-wallet');
        if (walletEl) walletEl.textContent = formatAddress(bal.address);
        updateCopyButtonStates();
      }
      const li = document.createElement('li');
      li.innerHTML = `<span class="balance-symbol">API Key balance (Litkey - Yellowstone)</span><span>${formatBalanceMax6(bal.balance)}</span>`;
      list.appendChild(li);
      const coreClient = await getCoreClient();
      let ledgerBalance = '–';
      try {
        const ledger = await coreClient.getLedgerBalance(state.apiKey);
        ledgerBalance = typeof ledger === 'string' ? ledger : String(ledger);
      } catch (_) {}
      const liLedger = document.createElement('li');
      liLedger.innerHTML = `<span class="balance-symbol">Network Ledger Balance (Litkey)</span><span>${formatBalanceMax6(ledgerBalance)}</span>`;
      list.appendChild(liLedger);
    }
    if (state.pkpPublicKey) {
      const bal = await client.getPkpBalance(state.pkpPublicKey, chain);
      if (bal.address) {
        state.pkpAddress = bal.address;
        const addrEl = document.getElementById('overview-pkp-address');
        if (addrEl) addrEl.textContent = formatAddress(bal.address);
        updateCopyButtonStates();
      }
      const selectedChain = state.chainList.find((c) => c.chain === chain);
      const symbol = selectedChain?.token ?? bal.symbol ?? chain;
      const displayName = selectedChain?.display_name ?? chain;
      const li = document.createElement('li');
      li.innerHTML = `<span class="balance-symbol">PKP (${symbol} – ${displayName})</span><span>${formatBalanceMax6(bal.balance)}</span>`;
      list.appendChild(li);
    }
    wrap.innerHTML = '';
    wrap.appendChild(list);
  } catch (err) {
    wrap.innerHTML = `<p class="status error">${err.message || String(err)}</p>`;
  }
}

function loadStoredAccount() {
  try {
    const raw = localStorage.getItem('swps_account');
    if (!raw) return;
    const data = JSON.parse(raw);
    if (data.apiKey && data.pkpPublicKey) {
      state.apiKey = data.apiKey;
      state.pkpPublicKey = data.pkpPublicKey;
      state.walletAddress = data.walletAddress || null;
      showOverviewAccount();
      document.getElementById('overview-actions').style.display = 'none';
    }
  } catch (_) {}
}

function loadPastedAccount() {
  const apiKey = document.getElementById('overview-api-key').value.trim();
  const pkpKey = document.getElementById('overview-pkp-key').value.trim();
  if (!apiKey || !pkpKey) {
    showOverviewStatus('Enter both API key and PKP public key.', 'error');
    return;
  }
  state.apiKey = apiKey;
  state.pkpPublicKey = pkpKey;
  state.walletAddress = null; // not available from paste
  showOverviewAccount();
  document.getElementById('overview-actions').style.display = 'none';
  showOverviewStatus('Account loaded. Refresh balances to see data.', 'success');
  saveAccount();
}

function saveAccount() {
  if (state.apiKey && state.pkpPublicKey) {
    localStorage.setItem(
      'swps_account',
      JSON.stringify({
        apiKey: state.apiKey,
        pkpPublicKey: state.pkpPublicKey,
        walletAddress: state.walletAddress,
      })
    );
  }
}

/** Fetch PKP address once from balance API when we have PKP but no cached address. */
async function fetchPkpAddressIfNeeded() {
  if (!state.pkpPublicKey || state.pkpAddress) return;
  const chain = state.chainList[0]?.chain;
  if (!chain) return;
  try {
    const client = await getTransferClient();
    const bal = await client.getPkpBalance(state.pkpPublicKey, chain);
    if (bal.address) {
      state.pkpAddress = bal.address;
      const addrEl = document.getElementById('overview-pkp-address');
      if (addrEl) addrEl.textContent = formatAddress(bal.address);
      updateCopyButtonStates();
    }
  } catch (_) {}
}

function updateLogoutButtonVisibility() {
  const btn = document.getElementById('btn-logout');
  if (btn) btn.style.display = state.apiKey ? 'inline-flex' : 'none';
}

function logout() {
  state.apiKey = null;
  state.pkpPublicKey = null;
  state.walletAddress = null;
  state.pkpAddress = null;
  localStorage.removeItem('swps_account');
  document.getElementById('overview-actions').style.display = 'block';
  document.getElementById('overview-account').style.display = 'none';
  document.getElementById('overview-chain-wrap').style.display = 'none';
  document.getElementById('overview-balances').innerHTML =
    '<p class="history-empty">Load an account and choose a chain to see balances.</p>';
  updateLogoutButtonVisibility();
}

// --- Transfer ---
function showTransferStatus(msg, type = 'info') {
  const el = document.getElementById('transfer-status');
  el.textContent = msg;
  el.className = `status ${type}`;
  el.style.display = 'block';
}

function pushHistory(record) {
  let list = [];
  try {
    const raw = sessionStorage.getItem(HISTORY_KEY);
    if (raw) list = JSON.parse(raw);
  } catch (_) {}
  list.unshift(record);
  sessionStorage.setItem(HISTORY_KEY, JSON.stringify(list.slice(0, 50)));
}

async function submitTransfer(e) {
  e.preventDefault();
  if (!state.apiKey || !state.pkpPublicKey) {
    showTransferStatus('Create an account on Overview first.', 'error');
    return;
  }
  const chainEl = document.getElementById('transfer-chain');
  const chain = chainEl ? chainEl.value.trim() : '';
  if (!chain) {
    showTransferStatus('Select a chain.', 'error');
    return;
  }
  const destination = document.getElementById('transfer-destination').value.trim();
  const amountRaw = document.getElementById('transfer-amount').value.trim();
  const amountNum = parseFloat(amountRaw);
  if (!amountRaw || !Number.isFinite(amountNum) || amountNum <= 0) {
    showTransferStatus('Enter a valid amount in ETH.', 'error');
    return;
  }
  const btn = document.getElementById('btn-transfer');
  btn.disabled = true;
  showTransferStatus('Sending…', 'info');
  try {
    const client = await getTransferClient();
    const result = await client.send({
      apiKey: state.apiKey,
      pkpPublicKey: state.pkpPublicKey,
      chain,
      destinationAddress: destination,
      amount: amountNum,
    });
    showTransferStatus(
      result.success ? `Sent. Txn: ${result.txn_id || 'N/A'}` : `Transfer failed: ${result.txn_id || 'unknown'}`,
      result.success ? 'success' : 'error'
    );
    if (result.success) {
      pushHistory({
        txn_id: result.txn_id,
        success: result.success,
        chain: result.chain,
        origin_symbol: result.origin_symbol,
        origin_amount: result.origin_amount,
        destination_address: result.destination_address,
        from_address: state.walletAddress || null,
        timestamp: result.timestamp || new Date().toISOString(),
      });
    }
  } catch (err) {
    showTransferStatus(err.message || String(err), 'error');
  } finally {
    btn.disabled = false;
  }
}

// --- Swap (intent-style UI; no backend yet) ---
function showSwapStatus(msg, type = 'info') {
  const el = document.getElementById('swap-status');
  el.textContent = msg;
  el.className = `status ${type}`;
  el.style.display = 'block';
}

function swapFlip() {
  const from = document.getElementById('swap-token-from');
  const to = document.getElementById('swap-token-to');
  const tmp = from.value;
  from.value = to.value;
  to.value = tmp;
  const amtFrom = document.getElementById('swap-amount-from');
  const amtTo = document.getElementById('swap-amount-to');
  const tmpAmt = amtFrom.value;
  amtFrom.value = amtTo.value || '';
  amtTo.value = tmpAmt || '0';
}

async function requestSwap() {
  const originValue = document.getElementById('swap-token-from')?.value?.trim();
  const destinationValue = document.getElementById('swap-token-to')?.value?.trim();
  const originAmountRaw = document.getElementById('swap-amount-from')?.value?.trim();
  const destinationAmountRaw = document.getElementById('swap-amount-to')?.value?.trim();
  const slippageRaw = document.getElementById('swap-slippage')?.value?.trim();

  const origin = parseSwapTokenValue(originValue);
  const destination = parseSwapTokenValue(destinationValue);
  const originSymbol = origin.symbol;
  const originChain = origin.chain;
  const destinationSymbol = destination.symbol;
  const destinationChain = destination.chain;

  if (!state.walletAddress && !state.pkpAddress) {
    showSwapStatus('Load an account on Overview first (wallet/PKP address required).', 'error');
    return;
  }
  const from = state.walletAddress || state.pkpAddress;
  if (!originSymbol || !originChain) {
    showSwapStatus('Select origin token (Symbol - Chain).', 'error');
    return;
  }
  if (!destinationSymbol || !destinationChain) {
    showSwapStatus('Select destination token (Symbol - Chain).', 'error');
    return;
  }
  const originAmountNum = parseFloat(originAmountRaw);
  if (!originAmountRaw || !Number.isFinite(originAmountNum) || originAmountNum <= 0) {
    showSwapStatus('Enter an origin amount (in ether).', 'error');
    return;
  }

  const destinationAmountNum = parseFloat(destinationAmountRaw || '0') || 0;
  const slippageNum = parseFloat(slippageRaw || '1') || 1;

  const btn = document.getElementById('btn-swap');
  btn.disabled = true;
  showSwapStatus('Requesting swap…', 'info');
  try {
    const client = await getSwapsClient();
    const result = await client.newQuoteRequest({
      from,
      originChain,
      originSymbol,
      originAmount: originAmountNum,
      destinationChain,
      destinationSymbol,
      destinationAmount: destinationAmountNum,
      slippage: slippageNum,
      quoteDeadlineSeconds: 60,
      originAddress: from,
      refundAddress: from,
      transactionDeadlineSeconds: 0,
    });
    showSwapStatus(
      `Swap requested. ID: ${result.swap_request_id || 'N/A'}. Tx: ${result.transaction_hash || 'N/A'}`,
      'success'
    );
  } catch (err) {
    showSwapStatus(err.message || String(err), 'error');
  } finally {
    btn.disabled = false;
  }
}

// --- History ---
function getHistoryAddress() {
  return document.getElementById('history-address').value.trim();
}

function loadHistory() {
  const address = getHistoryAddress();
  let list = [];
  try {
    const raw = sessionStorage.getItem(HISTORY_KEY);
    if (raw) list = JSON.parse(raw);
  } catch (_) {}
  const ul = document.getElementById('history-list');
  const empty = document.getElementById('history-empty');
  ul.innerHTML = '';
  const addrLower = address ? address.toLowerCase() : '';
  const forAddress = address
    ? list.filter(
        (t) =>
          (t.from_address || '').toLowerCase() === addrLower ||
          (t.destination_address || '').toLowerCase() === addrLower
      )
    : list;
  if (forAddress.length === 0) {
    empty.style.display = 'block';
    empty.textContent = address
      ? `No in-session transactions for ${address.slice(0, 10)}… Use a block explorer for full history.`
      : 'No in-session transactions. Transfers you make in this session appear here.';
    return;
  }
  empty.style.display = 'none';
  forAddress.forEach((t) => {
    const li = document.createElement('li');
    li.className = 'history-item';
    li.innerHTML = `
      <div class="txn-amount">${t.origin_amount || '–'} ${t.origin_symbol || ''} → ${(t.destination_address || '').slice(0, 10)}…</div>
      <div class="txn-id">${t.txn_id || '–'} · ${t.chain || ''} · ${t.timestamp || ''}</div>
    `;
    ul.appendChild(li);
  });
}

// --- Init ---
function init() {
  initRouting();
  loadChains();
  loadStoredAccount();
  if (state.apiKey && state.pkpPublicKey) showOverviewAccount();
  updateLogoutButtonVisibility();

  const toggleTestnet = document.getElementById('toggle-testnet');
  if (toggleTestnet) {
    toggleTestnet.checked = state.isTestnet;
    toggleTestnet.addEventListener('change', () => {
      state.isTestnet = toggleTestnet.checked;
      loadChains();
    });
  }

  document.getElementById('baseUrl').addEventListener('change', () => {
    initClients();
    loadChains();
  });

  document.getElementById('btn-logout').addEventListener('click', logout);
  document.getElementById('btn-create-account').addEventListener('click', createAccount);
  document.getElementById('btn-load-account').addEventListener('click', loadPastedAccount);
  document.getElementById('btn-refresh-balance').addEventListener('click', refreshBalances);
  document.getElementById('overview-chain').addEventListener('change', refreshBalances);

  document.getElementById('btn-copy-wallet').addEventListener('click', () => {
    if (state.walletAddress) copyToClipboard(state.walletAddress);
  });
  document.getElementById('btn-copy-pkp').addEventListener('click', () => {
    if (state.pkpPublicKey) copyToClipboard(state.pkpPublicKey);
  });
  document.getElementById('btn-copy-pkp-address').addEventListener('click', () => {
    if (state.pkpAddress) copyToClipboard(state.pkpAddress);
  });

  document.getElementById('transfer-form').addEventListener('submit', submitTransfer);

  document.getElementById('swap-flip').addEventListener('click', swapFlip);
  document.getElementById('btn-swap').addEventListener('click', requestSwap);

  document.getElementById('btn-load-history').addEventListener('click', loadHistory);
  document.getElementById('btn-history-use-wallet').addEventListener('click', () => {
    if (state.walletAddress) {
      document.getElementById('history-address').value = state.walletAddress;
    }
  });
  document.getElementById('history-address').addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      loadHistory();
    }
  });

  // Persist account when we have it (e.g. after create)
  const observer = () => saveAccount();
  setInterval(observer, 2000);
}

init();
