(function () {
  'use strict';

  const BASE_CHAIN_ID_HEX = '0x2105';
  const BASE_CHAIN_ID_DEC = 8453;
  const MIN_CENTS = 500n;
  const WEI = 1000000000000000000n;
  const USD = 1000000000000000000n;

  const ERC20_ABI = [
    'function approve(address spender,uint256 amount) returns (bool)',
    'function decimals() view returns (uint8)',
    'function balanceOf(address account) view returns (uint256)',
    'function allowance(address owner,address spender) view returns (uint256)',
  ];
  const GATEWAY_ABI = ['function pay(uint256 amount,address wallet)'];

  const state = {
    wallet: null,
    quote: null,
    config: null,
    frozenQuote: null,
    frozenAmountWei: null,
    approvedAmountWei: null,
    provider: null,
    signer: null,
    amountWei: 0n,
    txHash: null,

  };

  const $ = (id) => document.getElementById(id);
  const setText = (id, value) => { const el = $(id); if (el) el.textContent = value; };
  const setStatus = (message, kind) => { const el = $('status'); if (!el) return; el.textContent = message; el.className = 'status ' + (kind || 'info'); };
  const isAddress = (value) => /^0x[0-9a-fA-F]{40}$/.test(value || '');
  const fmtUsd = (cents) => '$' + (Number(cents) / 100).toFixed(2);

  function formatUsdWei(usdWei) {
    const value = BigInt(usdWei);
    const whole = value / USD;
    const fraction = value % USD;
    if (fraction === 0n) return '$' + whole.toString();
    const decimals = fraction.toString().padStart(18, '0').slice(0, 6).replace(/0+$/, '');
    return '$' + whole.toString() + (decimals ? '.' + decimals : '');
  }

  function parseCents(value) {
    const trimmed = String(value || '').trim();
    const match = trimmed.match(/^(\d+)(?:\.(\d{0,2}))?$/);
    if (!match) return null;
    return BigInt(match[1]) * 100n + BigInt((match[2] || '').padEnd(2, '0'));
  }

  function ceilDiv(a, b) { return (a + b - 1n) / b; }
  function amountWeiForCents(cents, effectiveRate) { return ceilDiv(cents * WEI * USD, BigInt(effectiveRate) * 100n); }
  function creditCentsForAmount(amountWei, effectiveRate) { return (amountWei * BigInt(effectiveRate) * 100n + (WEI * USD / 2n)) / (WEI * USD); }
  function formatToken(amountWei) { return ethers.formatUnits(amountWei, 18).replace(/(\.\d{6})\d+$/, '$1'); }

  async function fetchJson(url) {
    const res = await fetch(url, { credentials: 'omit' });
    const body = await res.json().catch(() => ({}));
    if (!res.ok) throw new Error(body.error || ('Request failed: ' + res.status));
    return body;
  }

  async function postJson(url, payload) {
    const res = await fetch(url, {
      method: 'POST',
      credentials: 'omit',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(payload),
    });
    const body = await res.json().catch(() => ({}));
    if (!res.ok) throw new Error(body.error || ('Request failed: ' + res.status));
    return body;
  }

  async function loadAccount() {
    const params = new URLSearchParams(window.location.search);
    const wallet = params.get('wallet');
    if (!isAddress(wallet)) throw new Error('Missing or invalid wallet= address in URL.');
    state.wallet = ethers.getAddress(wallet).toLowerCase();
    const preview = await fetchJson('/api/customer/preview?wallet=' + encodeURIComponent(state.wallet));
    if (!preview.found) throw new Error('No Stripe customer exists for this wallet. Add funds once from the dashboard first.');
    setText('account-email', preview.email || 'No email on file');
    setText('account-wallet', preview.wallet_address || state.wallet);
  }

  async function refreshQuote() {
    if (state.frozenQuote) return;
    state.quote = await fetchJson('/api/litkey/quote');
    renderQuote();
  }

  function resetApproval(message) {
    state.frozenQuote = null;
    state.frozenAmountWei = null;
    state.approvedAmountWei = null;
    $('credit-dollars').disabled = false;
    $('confirm-account').disabled = false;
    $('pay').disabled = true;
    if (message) setStatus(message, 'info');
    renderQuote();
  }

  function renderQuote() {
    const quote = state.frozenQuote || state.quote;
    if (!quote || quote.crediting_paused || !quote.effective_usd_wei_per_litkey) {
      setText('quote-status', 'Crediting paused');
      $('approve').disabled = true;
      $('pay').disabled = true;
      return;
    }

    if (!state.config) {
      setText('quote-status', 'Payments unavailable');
      $('approve').disabled = true;
      $('pay').disabled = true;
      return;
    }

    if (state.frozenAmountWei !== null) {
      state.amountWei = state.frozenAmountWei;
      setText('litkey-amount', formatToken(state.frozenAmountWei) + ' LITKEY');
      setText('rate-display', formatUsdWei(quote.effective_usd_wei_per_litkey) + ' credit / LITKEY');
      setText('quote-status', 'Frozen for approval');
      $('approve').disabled = true;
      $('pay').disabled = state.approvedAmountWei === null;
      return;
    }

    const cents = parseCents($('credit-dollars').value);
    if (!cents || cents < MIN_CENTS) {
      setText('quote-status', 'Enter at least $5.00');
      setText('litkey-amount', '—');
      $('approve').disabled = true;
      $('pay').disabled = true;
      return;
    }
    state.amountWei = amountWeiForCents(cents, quote.effective_usd_wei_per_litkey);
    setText('litkey-amount', formatToken(state.amountWei) + ' LITKEY');
    setText('rate-display', formatUsdWei(quote.effective_usd_wei_per_litkey) + ' credit / LITKEY');
    setText('quote-status', 'Live');
    $('approve').disabled = !$('confirm-account').checked || !state.signer;
    $('pay').disabled = true;
  }

  async function loadConfig() { state.config = await fetchJson('/api/litkey/payment-config'); }

  async function ensureBase() {
    if (!window.ethereum) throw new Error('No browser wallet found.');
    let chainId = await window.ethereum.request({ method: 'eth_chainId' });
    if (chainId === BASE_CHAIN_ID_HEX) return;
    try {
      await window.ethereum.request({ method: 'wallet_switchEthereumChain', params: [{ chainId: BASE_CHAIN_ID_HEX }] });
    } catch (e) {
      if (e && e.code === 4902) {
        await window.ethereum.request({
          method: 'wallet_addEthereumChain',
          params: [{
            chainId: BASE_CHAIN_ID_HEX,
            chainName: 'Base',
            nativeCurrency: { name: 'Ether', symbol: 'ETH', decimals: 18 },
            rpcUrls: ['https://mainnet.base.org'],
            blockExplorerUrls: ['https://basescan.org'],
          }],
        });
        await window.ethereum.request({ method: 'wallet_switchEthereumChain', params: [{ chainId: BASE_CHAIN_ID_HEX }] });
      } else {
        throw e;
      }
    }
    chainId = await window.ethereum.request({ method: 'eth_chainId' });
    if (chainId !== BASE_CHAIN_ID_HEX) throw new Error('Switch to Base mainnet to pay with LITKEY.');
  }

  async function connectWallet() {
    if (!window.ethereum) throw new Error('No browser wallet found.');
    const button = $('connect-wallet');
    if (button) button.disabled = true;
    try {
      setStatus('Opening wallet…', 'info');
      await window.ethereum.request({ method: 'eth_requestAccounts' });
      await ensureBase();
      // Re-read accounts after a possible network switch. Some injected wallets
      // briefly expose a null selected account/provider during the switch, which
      // made ethers BrowserProvider.getSigner() throw on the first click.
      const accounts = await window.ethereum.request({ method: 'eth_requestAccounts' });
      const account = Array.isArray(accounts) && accounts[0] ? accounts[0] : null;
      if (!account) throw new Error('No wallet account selected.');
      state.provider = new ethers.BrowserProvider(window.ethereum);
      await state.provider.getNetwork();
      state.signer = await state.provider.getSigner(account);
      setStatus('Wallet connected. Confirm the credited account, then approve exact LITKEY.', 'success');
      renderQuote();
    } finally {
      if (button) button.disabled = false;
    }
  }

  async function approve() {
    await ensureBase();
    if (!state.signer) throw new Error('Connect your wallet first.');
    const quote = state.quote;
    if (!quote || quote.crediting_paused || !quote.effective_usd_wei_per_litkey) throw new Error('No live LITKEY quote is available.');
    const cents = parseCents($('credit-dollars').value);
    if (!cents || cents < MIN_CENTS) throw new Error('Minimum payment is $5.00 equivalent.');

    state.frozenQuote = quote;
    state.frozenAmountWei = amountWeiForCents(cents, quote.effective_usd_wei_per_litkey);
    state.approvedAmountWei = null;
    $('credit-dollars').disabled = true;
    $('confirm-account').disabled = true;
    renderQuote();

    const owner = await state.signer.getAddress();
    const token = new ethers.Contract(state.config.token_address, ERC20_ABI, state.signer);
    const balance = await token.balanceOf(owner);
    if (balance < state.frozenAmountWei) throw new Error('Insufficient LITKEY balance.');
    const allowance = await token.allowance(owner, state.config.gateway_address);
    if (allowance < state.frozenAmountWei) {
      setStatus('Approve the exact LITKEY amount in your wallet…', 'info');
      const tx = await token.approve(state.config.gateway_address, state.frozenAmountWei);
      await tx.wait();
    }
    state.approvedAmountWei = state.frozenAmountWei;
    $('pay').disabled = false;
    setStatus('Approval ready. Submit payment to credit the account.', 'success');
  }

  async function pay() {
    await ensureBase();
    if (!state.signer || state.approvedAmountWei === null) throw new Error('Approve the exact LITKEY amount first.');
    const gateway = new ethers.Contract(state.config.gateway_address, GATEWAY_ABI, state.signer);
    setStatus('Submit payment in your wallet…', 'info');
    const tx = await gateway.pay(state.approvedAmountWei, state.wallet);
    state.txHash = tx.hash;
    $('pay').disabled = true;
    setStatus('Payment submitted. Waiting for transaction receipt…', 'info');
    await tx.wait();
    setStatus('Payment mined. Verifying transaction and applying credit…', 'info');
    const data = await postJson('/api/litkey/payment-claim', {
      tx_hash: state.txHash,
      wallet: state.wallet,
    });
    if (data.found) {
      if (data.status === 'credited') setStatus('Credited ' + fmtUsd(data.cents_credited || 0) + ' to the account.', 'success');
      else setStatus('Payment recorded with status: ' + data.status, 'warning');
    } else if (data.status === 'tx_failed') {
      setStatus('Payment transaction failed on-chain. No credit was applied.', 'error');
    } else {
      setStatus('Payment was mined, but the backend did not find the gateway event. Contact support with tx ' + state.txHash + '.', 'warning');
    }
  }

  function installWalletGuards() {
    if (!window.ethereum || !window.ethereum.on) return;
    window.ethereum.on('chainChanged', (chainId) => {
      state.provider = null;
      state.signer = null;
      resetApproval(chainId === BASE_CHAIN_ID_HEX
        ? 'Network changed. Reconnect wallet before paying.'
        : 'Wallet switched away from Base. Switch back to Base and reconnect before paying.');
    });
    window.ethereum.on('accountsChanged', () => {
      state.provider = null;
      state.signer = null;
      resetApproval('Wallet account changed. Reconnect before paying.');
    });
  }

  async function init() {
    try {
      await Promise.all([loadAccount(), loadConfig(), refreshQuote()]);
      if (state.config.chain_id !== BASE_CHAIN_ID_DEC) throw new Error('Payment backend is not configured for Base mainnet.');
      installWalletGuards();
      setStatus('Confirm the credited account, then connect your wallet.', 'info');
      setInterval(refreshQuote, 30000);
    } catch (e) {
      $('connect-wallet').disabled = true;
      $('approve').disabled = true;
      $('pay').disabled = true;
      setText('quote-status', 'Payments unavailable');
      setStatus(e.message || String(e), 'error');
    }
  }

  $('credit-dollars').addEventListener('input', () => resetApproval());
  $('confirm-account').addEventListener('change', () => resetApproval());
  $('connect-wallet').addEventListener('click', () => connectWallet().catch((e) => setStatus(e.message || String(e), 'error')));
  $('approve').addEventListener('click', () => approve().catch((e) => { resetApproval(); setStatus(e.message || String(e), 'error'); }));
  $('pay').addEventListener('click', () => pay().catch((e) => setStatus(e.message || String(e), 'error')));
  init();
}());
