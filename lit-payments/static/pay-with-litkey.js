(function () {
  'use strict';

  const BASE_CHAIN_ID_DEC = 8453;
  const BASE_RPC_URL = 'https://mainnet.base.org';
  const BASE_ADD_PARAMS = {
    chainName: 'Base',
    nativeCurrency: { name: 'Ether', symbol: 'ETH', decimals: 18 },
    rpcUrls: [BASE_RPC_URL],
    blockExplorerUrls: ['https://basescan.org'],
  };
  const MIN_CENTS = 500n;
  const WEI = 1000000000000000000n;
  const USD = 1000000000000000000n;

  // Lazy-loaded ESM wallet helper (shared with the dashboard).
  // Provides a MetaMask / WalletConnect picker on first connect.
  let _walletModule = null;
  async function getWalletModule() {
    if (!_walletModule) _walletModule = await import('/static/wallet_connect.js');
    return _walletModule;
  }

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
  function formatDiscountBps(bps) {
    const value = Number(bps || 0);
    if (!Number.isFinite(value) || value <= 0) return 'No discount';
    const whole = Math.trunc(value / 100);
    const fractional = Math.abs(value % 100);
    return (fractional ? whole + '.' + String(fractional).padStart(2, '0').replace(/0+$/, '') : String(whole)) + '% off';
  }

  function marketRate(quote) {
    return quote && quote.rate && quote.rate.usd_wei_per_litkey ? quote.rate.usd_wei_per_litkey : null;
  }

  function setQuoteDisplays(quote) {
    const market = marketRate(quote);
    setText('rate-display', market ? formatUsdWei(market) + ' / LITKEY' : '—');
    setText('discount-display', formatDiscountBps(quote ? quote.discount_basis_points : 0));
    setText('effective-rate-display', quote && quote.effective_usd_wei_per_litkey ? formatUsdWei(quote.effective_usd_wei_per_litkey) + ' credit / LITKEY' : '—');
  }

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
      setQuoteDisplays(quote);
      setText('litkey-amount', '—');
      $('approve').disabled = true;
      $('pay').disabled = true;
      return;
    }

    if (!state.config) {
      setText('quote-status', 'Payments unavailable');
      setQuoteDisplays(quote);
      setText('litkey-amount', '—');
      $('approve').disabled = true;
      $('pay').disabled = true;
      return;
    }

    const cents = parseCents($('credit-dollars').value);

    if (state.frozenAmountWei !== null) {
      state.amountWei = state.frozenAmountWei;
      setText('litkey-amount', formatToken(state.frozenAmountWei) + ' LITKEY');
      setQuoteDisplays(quote);
      setText('quote-status', 'Frozen for approval');
      $('approve').disabled = true;
      $('pay').disabled = state.approvedAmountWei === null;
      return;
    }

    if (!cents || cents < MIN_CENTS) {
      setText('quote-status', 'Enter at least $5.00');
      setText('litkey-amount', '—');
      setQuoteDisplays(quote);
      $('approve').disabled = true;
      $('pay').disabled = true;
      return;
    }
    state.amountWei = amountWeiForCents(cents, quote.effective_usd_wei_per_litkey);
    setText('litkey-amount', formatToken(state.amountWei) + ' LITKEY');
    setQuoteDisplays(quote);
    setText('quote-status', 'Live');
    $('approve').disabled = !$('confirm-account').checked || !state.signer;
    $('pay').disabled = true;
  }

  async function loadConfig() { state.config = await fetchJson('/api/litkey/payment-config'); }

  async function ensureBase() {
    const mod = await getWalletModule();
    const snap = mod.snapshotState();
    if (snap.connected && snap.chainId === BASE_CHAIN_ID_DEC) return;
    // Route through the shared switchChain so the call goes to the active
    // provider (injected OR WalletConnect), with EIP-3085 add params for
    // wallets that don't know Base yet.
    const { signer } = await mod.switchChain(BASE_CHAIN_ID_DEC, BASE_ADD_PARAMS);
    state.signer = signer;
    state.provider = mod.getProvider();
  }

  async function connectWallet() {
    const button = $('connect-wallet');
    if (button) button.disabled = true;
    try {
      setStatus('Opening wallet…', 'info');
      const mod = await getWalletModule();
      const { signer, chainId } = await mod.connectWallet({
        chainId: BASE_CHAIN_ID_DEC,
        rpcUrl: BASE_RPC_URL,
      });
      state.signer = signer;
      state.provider = mod.getProvider();
      if (chainId !== BASE_CHAIN_ID_DEC) {
        try {
          await ensureBase();
        } catch (e) {
          throw new Error('Switch to Base mainnet to pay with LITKEY.');
        }
      }
      setStatus('Wallet connected. Confirm the credited account, then approve exact LITKEY.', 'success');
      renderQuote();
    } catch (e) {
      if (e && e.cancelled) {
        setStatus('Wallet connection cancelled.', 'info');
        return;
      }
      throw e;
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

  async function installWalletGuards() {
    const mod = await getWalletModule();
    let prevAddress = null;
    let prevChainId = null;
    mod.onWalletChange((snap) => {
      if (!snap.connected) {
        state.provider = null;
        state.signer = null;
        resetApproval('Wallet disconnected. Reconnect before paying.');
        prevAddress = null;
        prevChainId = null;
        return;
      }
      const addressChanged = prevAddress && snap.address && prevAddress.toLowerCase() !== snap.address.toLowerCase();
      const chainChanged = prevChainId !== null && snap.chainId !== prevChainId;
      prevAddress = snap.address;
      prevChainId = snap.chainId;
      if (chainChanged) {
        state.provider = null;
        state.signer = null;
        resetApproval(snap.chainId === BASE_CHAIN_ID_DEC
          ? 'Network changed. Reconnect wallet before paying.'
          : 'Wallet switched away from Base. Switch back to Base and reconnect before paying.');
        return;
      }
      if (addressChanged) {
        state.provider = null;
        state.signer = null;
        resetApproval('Wallet account changed. Reconnect before paying.');
      }
    });
  }

  async function init() {
    try {
      await Promise.all([loadAccount(), loadConfig(), refreshQuote()]);
      if (state.config.chain_id !== BASE_CHAIN_ID_DEC) throw new Error('Payment backend is not configured for Base mainnet.');
      await installWalletGuards();
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
