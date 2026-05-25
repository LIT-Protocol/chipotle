/**
 * Browser-wallet connect helpers for sovereign-mode direct contract writes.
 *
 * Connectors:
 *   - 'eoa'           — EIP-1193 injected provider (MetaMask, Rabby, etc.)
 *   - 'walletconnect' — WalletConnect v2 via @walletconnect/ethereum-provider
 *
 * The public entry is `connectWallet({ chainId, rpcUrl })`. It is idempotent:
 * if a connection is already active it returns the cached state without
 * re-prompting (so e.g. typed-data signing during ChainSecured billing reuses
 * the live signer). The first call shows a picker dialog. `connectEoa` is
 * preserved as an alias for back-compat with existing callers.
 *
 * `switchChain` is provider-aware — it routes through whichever EIP-1193
 * provider is currently active, not always `window.ethereum`.
 */

function getEthers() {
  if (typeof globalThis.ethers !== 'undefined') return globalThis.ethers;
  throw new Error('wallet_connect: ethers is not loaded. Preload it before importing this module.');
}

const WALLETCONNECT_PROJECT_ID = '8feea2064504b04d14a55d6fbef18966';

const listeners = {
  accountsChanged: null,
  chainChanged: null,
  disconnect: null,
};

let _state = {
  provider: null,        // ethers.BrowserProvider
  signer: null,          // ethers.Signer
  address: null,
  chainId: null,
  source: null,          // 'eoa' | 'walletconnect'
  eth: null,             // active EIP-1193 provider (window.ethereum or wc instance)
};

let _wcProvider = null;  // cached WalletConnect provider instance

const subscribers = new Set();

export function onWalletChange(fn) {
  subscribers.add(fn);
  return () => subscribers.delete(fn);
}

function emit() {
  const snapshot = snapshotState();
  for (const fn of subscribers) {
    try { fn(snapshot); } catch (e) { console.warn('[wallet_connect] subscriber threw:', e); }
  }
}

export function snapshotState() {
  return {
    connected: !!_state.signer,
    address: _state.address,
    chainId: _state.chainId,
    source: _state.source,
  };
}

export function getSigner() {
  return _state.signer;
}

export function getProvider() {
  return _state.provider;
}

/**
 * Connect a wallet, prompting the user with a picker on first call.
 *
 * @param {Object} [opts]
 * @param {number} [opts.chainId]  Required for WalletConnect; ignored for EOA.
 * @param {string} [opts.rpcUrl]   Optional WalletConnect RPC override for the
 *                                 chain. Ignored for EOA.
 * @param {boolean} [opts.force]   Bypass the cached-state shortcut and re-prompt.
 * @returns {Promise<{signer: import('ethers').Signer, address: string, chainId: number, source: string}>}
 */
export async function connectWallet(opts = {}) {
  if (!opts.force && _state.signer) {
    return { signer: _state.signer, address: _state.address, chainId: _state.chainId, source: _state.source };
  }
  const choice = await showWalletPicker({ allowWalletConnect: !!opts.chainId });
  if (choice === 'cancel') {
    const err = new Error('Wallet connection cancelled.');
    err.cancelled = true;
    throw err;
  }
  if (choice === 'walletconnect') {
    if (!opts.chainId) {
      throw new Error('WalletConnect requires a chainId. Wait for chain config to load and try again.');
    }
    return _connectWalletConnect({ chainId: Number(opts.chainId), rpcUrl: opts.rpcUrl });
  }
  return _connectInjected();
}

/**
 * Back-compat alias. Some callers (billing typed-data signing, chain-switch
 * re-derives) still import `connectEoa`; keep them working by routing through
 * the picker-aware entry. Cached state short-circuits the picker after the
 * initial connection.
 */
export const connectEoa = connectWallet;

async function _connectInjected() {
  const ethers = getEthers();
  if (!window.ethereum) {
    throw new Error('No browser wallet found. Install MetaMask (or another EIP-1193 wallet) and reload, or use WalletConnect.');
  }
  // If a WalletConnect session is currently active, tear it down before
  // switching to the injected provider so we don't leak an open relay session.
  if (_wcProvider) {
    try {
      await Promise.race([
        _wcProvider.disconnect(),
        new Promise((_, reject) => setTimeout(() => reject(new Error('timeout')), 3000)),
      ]);
    } catch {}
    _wcProvider = null;
  }
  const eth = window.ethereum;
  const provider = new ethers.BrowserProvider(eth);
  await provider.send('eth_requestAccounts', []);
  const signer = await provider.getSigner();
  const address = await signer.getAddress();
  const network = await provider.getNetwork();

  detachListeners();
  attachListeners(eth);

  _state = {
    provider,
    signer,
    address,
    chainId: Number(network.chainId),
    source: 'eoa',
    eth,
  };
  emit();
  return { signer, address, chainId: _state.chainId, source: 'eoa' };
}

async function _connectWalletConnect({ chainId, rpcUrl }) {
  const ethers = getEthers();

  let EthereumProvider;
  try {
    ({ EthereumProvider } = await import(
      'https://esm.sh/@walletconnect/ethereum-provider@2.23.9'
    ));
  } catch {
    throw new Error('Failed to load WalletConnect. Check your network connection or try again.');
  }

  // Tear down any stale session before reconnecting (with timeout so a hung
  // relay doesn't block the picker indefinitely).
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
    // Re-emit the WC SDK's `display_uri` event as a DOM event so e2e tests can
    // pair a headless wallet without scraping the QR modal. The dashboard
    // itself ignores it.
    _wcProvider.on?.('display_uri', (uri) => {
      window.dispatchEvent(new CustomEvent('lit:wc-display-uri', { detail: uri }));
    });
    await _wcProvider.connect();
  } catch (err) {
    _wcProvider = null;
    throw err;
  }

  const eth = _wcProvider;
  const provider = new ethers.BrowserProvider(eth);
  const signer = await provider.getSigner();
  const address = await signer.getAddress();
  const network = await provider.getNetwork();

  detachListeners();
  attachListeners(eth);

  _state = {
    provider,
    signer,
    address,
    chainId: Number(network.chainId),
    source: 'walletconnect',
    eth,
  };
  emit();
  return { signer, address, chainId: _state.chainId, source: 'walletconnect' };
}

/**
 * Request a chain switch via EIP-3326 against the active provider. If the
 * wallet doesn't know the chain and `addParams` is provided, fall back to
 * EIP-3085 wallet_addEthereumChain. Routes through whichever EIP-1193 provider
 * is currently connected (window.ethereum or the WalletConnect instance).
 */
export async function switchChain(targetChainId, addParams) {
  const eth = _state.eth || window.ethereum;
  if (!eth) throw new Error('No browser wallet available to switch chains.');
  const hexId = '0x' + Number(targetChainId).toString(16);

  try {
    await eth.request({
      method: 'wallet_switchEthereumChain',
      params: [{ chainId: hexId }],
    });
  } catch (err) {
    // EIP-3326 error code 4902: chain not added. Add it, then re-issue switch.
    // Some wallets auto-switch after add, others do not, so we always retry
    // the switch and verify chainId afterwards.
    const code = err?.code ?? err?.data?.originalError?.code;
    if (code === 4902 && addParams) {
      await eth.request({
        method: 'wallet_addEthereumChain',
        params: [{ ...addParams, chainId: hexId }],
      });
      try {
        await eth.request({
          method: 'wallet_switchEthereumChain',
          params: [{ chainId: hexId }],
        });
      } catch (switchErr) {
        // Some wallets return success from add + auto-switch internally; if
        // this second switch throws 4902 or "already on chain", treat it as
        // non-fatal and fall through to the network-verification step below.
        const sc = switchErr?.code ?? switchErr?.data?.originalError?.code;
        if (sc !== 4902 && sc !== -32602) throw switchErr;
      }
    } else {
      throw err;
    }
  }

  // Refresh state to pick up the new chain / signer. Re-derive the address
  // too in case the active account changed during the switch (the wallet may
  // surface a different account on the new network).
  const ethers = getEthers();
  const provider = new ethers.BrowserProvider(eth);
  const signer = await provider.getSigner();
  const address = await signer.getAddress();
  const network = await provider.getNetwork();
  const activeChainId = Number(network.chainId);
  if (activeChainId !== Number(targetChainId)) {
    throw Object.assign(
      new Error(
        `Chain switch did not take effect: wallet is on chain ${activeChainId}, expected ${targetChainId}. Switch network manually and retry.`,
      ),
      { wrongChain: true, actual: activeChainId, expected: Number(targetChainId) },
    );
  }
  _state = {
    ..._state,
    provider,
    signer,
    address,
    chainId: activeChainId,
  };
  emit();
  return { signer, address, chainId: activeChainId, source: _state.source };
}

/**
 * Guard: throw unless the wallet is on the expected chain. Caller should
 * catch and invoke `switchChain(expectedChainId, addParams)` on failure.
 */
export function assertChain(expectedChainId) {
  if (_state.chainId !== Number(expectedChainId)) {
    throw Object.assign(
      new Error(`Wallet is on chain ${_state.chainId}, expected ${expectedChainId}. Switch network and retry.`),
      { wrongChain: true, actual: _state.chainId, expected: Number(expectedChainId) },
    );
  }
}

export function disconnect() {
  detachListeners();
  if (_wcProvider) {
    Promise.race([
      _wcProvider.disconnect(),
      new Promise((_, reject) => setTimeout(() => reject(new Error('timeout')), 3000)),
    ]).catch(() => {});
    _wcProvider = null;
  }
  _state = { provider: null, signer: null, address: null, chainId: null, source: null, eth: null };
  emit();
}

function attachListeners(eth) {
  listeners.accountsChanged = async (accounts) => {
    if (!accounts || accounts.length === 0) {
      disconnect();
      return;
    }
    try {
      const ethers = getEthers();
      const provider = new ethers.BrowserProvider(eth);
      const signer = await provider.getSigner();
      _state = { ..._state, provider, signer, address: accounts[0] };
      emit();
    } catch (e) {
      console.warn('[wallet_connect] accountsChanged handler failed:', e);
      disconnect();
    }
  };
  listeners.chainChanged = async (hexChainId) => {
    const chainId = typeof hexChainId === 'string' ? parseInt(hexChainId, 16) : Number(hexChainId);
    try {
      const ethers = getEthers();
      const provider = new ethers.BrowserProvider(eth);
      const signer = _state.address ? await provider.getSigner() : null;
      _state = { ..._state, provider, signer, chainId };
      emit();
    } catch (e) {
      console.warn('[wallet_connect] chainChanged handler failed:', e);
    }
  };
  listeners.disconnect = () => disconnect();

  eth.on?.('accountsChanged', listeners.accountsChanged);
  eth.on?.('chainChanged', listeners.chainChanged);
  eth.on?.('disconnect', listeners.disconnect);
}

function detachListeners() {
  const eth = _state.eth;
  if (!eth) return;
  const off = eth.removeListener?.bind(eth) || eth.off?.bind(eth);
  if (!off) return;
  if (listeners.accountsChanged) off('accountsChanged', listeners.accountsChanged);
  if (listeners.chainChanged) off('chainChanged', listeners.chainChanged);
  if (listeners.disconnect) off('disconnect', listeners.disconnect);
  listeners.accountsChanged = null;
  listeners.chainChanged = null;
  listeners.disconnect = null;
}

/* ═══ Picker dialog ═════════════════════════════════════════════════════════
 *
 * Auto-injected on first use so consumers (dashboard, future dapps) don't have
 * to add markup. Styled with explicit colors that read on light + dark hosts.
 */

const PICKER_DIALOG_ID = 'lit-wallet-picker';

function ensurePickerDialog() {
  let dialog = document.getElementById(PICKER_DIALOG_ID);
  if (dialog) return dialog;

  dialog = document.createElement('dialog');
  dialog.id = PICKER_DIALOG_ID;
  dialog.innerHTML = `
    <style>
      #${PICKER_DIALOG_ID} {
        background: #ffffff;
        color: #1e293b;
        border: 1px solid #e2e8f0;
        border-radius: 16px;
        padding: 1.75rem;
        font-family: 'Inter', 'Outfit', system-ui, -apple-system, sans-serif;
        min-width: 320px;
        max-width: 400px;
        box-shadow: 0 20px 50px rgba(15, 23, 42, 0.2);
      }
      #${PICKER_DIALOG_ID}::backdrop {
        background: rgba(15, 23, 42, 0.45);
      }
      #${PICKER_DIALOG_ID} h3 {
        margin: 0 0 0.4rem; font-size: 1.05rem; font-weight: 600;
      }
      #${PICKER_DIALOG_ID} p.lit-wp-sub {
        color: #64748b; font-size: 0.85rem; margin: 0 0 1.1rem;
      }
      #${PICKER_DIALOG_ID} .lit-wp-list {
        display: flex; flex-direction: column; gap: 0.6rem;
      }
      #${PICKER_DIALOG_ID} button[data-wallet] {
        display: flex; align-items: center; gap: 0.75rem;
        width: 100%; padding: 0.8rem 1rem;
        background: #f8fafc; border: 1px solid #e2e8f0; border-radius: 10px;
        color: #1e293b; font: inherit; font-size: 0.95rem; font-weight: 500;
        cursor: pointer;
      }
      #${PICKER_DIALOG_ID} button[data-wallet]:hover {
        background: #f1f5f9;
      }
      #${PICKER_DIALOG_ID} button[data-wallet="cancel"] {
        margin-top: 1rem; justify-content: center;
        background: transparent; color: #64748b; font-weight: 400; font-size: 0.85rem;
      }
      @media (prefers-color-scheme: dark) {
        #${PICKER_DIALOG_ID} {
          background: #1e293b; color: #f1f5f9; border-color: #334155;
          box-shadow: 0 20px 50px rgba(0, 0, 0, 0.5);
        }
        #${PICKER_DIALOG_ID} p.lit-wp-sub { color: #94a3b8; }
        #${PICKER_DIALOG_ID} button[data-wallet] {
          background: #0f172a; border-color: #334155; color: #f1f5f9;
        }
        #${PICKER_DIALOG_ID} button[data-wallet]:hover { background: #233048; }
        #${PICKER_DIALOG_ID} button[data-wallet="cancel"] {
          background: transparent; color: #94a3b8;
        }
      }
    </style>
    <h3>Connect Wallet</h3>
    <p class="lit-wp-sub">Choose how you'd like to connect.</p>
    <div class="lit-wp-list">
      <button type="button" data-wallet="metamask">
        <svg width="24" height="24" viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="M21.3 2L13.1 8.2l1.5-3.6L21.3 2z" fill="#E2761B" stroke="#E2761B" stroke-width=".1"/><path d="M2.7 2l8.1 6.3-1.4-3.7L2.7 2zm15.7 14.4l-2.2 3.3 4.6 1.3 1.3-4.5-3.7-.1zm-16.8.1l1.3 4.5 4.6-1.3-2.2-3.3-3.7.1z" fill="#E4761B" stroke="#E4761B" stroke-width=".1"/><path d="M7.3 10.5l-1.3 2 4.6.2-.2-5-3.1 2.8zm9.4 0l-3.2-2.9-.1 5.1 4.6-.2-1.3-2z" fill="#E4761B" stroke="#E4761B" stroke-width=".1"/></svg>
        MetaMask / Browser Wallet
      </button>
      <button type="button" data-wallet="walletconnect">
        <svg width="24" height="24" viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="M6.09 8.56c3.26-3.19 8.54-3.19 11.8 0l.39.38a.4.4 0 010 .58l-1.34 1.31a.21.21 0 01-.3 0l-.54-.53a5.93 5.93 0 00-8.24 0l-.58.57a.21.21 0 01-.3 0L5.65 9.56a.4.4 0 010-.58l.44-.42zm14.58 2.71l1.2 1.17a.4.4 0 010 .58l-5.38 5.27a.42.42 0 01-.59 0l-3.82-3.74a.1.1 0 00-.15 0l-3.82 3.74a.42.42 0 01-.59 0L2.14 13.02a.4.4 0 010-.58l1.2-1.17a.42.42 0 01.59 0l3.82 3.74a.1.1 0 00.15 0l3.82-3.74a.42.42 0 01.59 0l3.82 3.74a.1.1 0 00.15 0l3.82-3.74a.42.42 0 01.59 0z" fill="#3B99FC"/></svg>
        WalletConnect
      </button>
    </div>
    <button type="button" data-wallet="cancel">Cancel</button>
  `;
  document.body.appendChild(dialog);
  return dialog;
}

function showWalletPicker({ allowWalletConnect = true } = {}) {
  return new Promise((resolve) => {
    const dialog = ensurePickerDialog();
    // Toggle the WalletConnect option based on whether the caller provided
    // chain context. Without a chainId, EthereumProvider.init() can't pick a
    // chain so we hide the option rather than throw after the click.
    const wcBtn = dialog.querySelector('button[data-wallet="walletconnect"]');
    if (wcBtn) wcBtn.style.display = allowWalletConnect ? '' : 'none';

    if (typeof dialog.showModal !== 'function') {
      // Pre-2022 browsers without <dialog>. Auto-pick the only viable
      // connector instead of forcing the user into a dead end.
      resolve(window.ethereum ? 'metamask' : (allowWalletConnect ? 'walletconnect' : 'metamask'));
      return;
    }

    const cleanup = () => {
      dialog.removeEventListener('click', onClick);
      dialog.removeEventListener('cancel', onCancel);
    };
    const onClick = (e) => {
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

    dialog.addEventListener('click', onClick);
    dialog.addEventListener('cancel', onCancel, { once: true });
    dialog.showModal();
  });
}
