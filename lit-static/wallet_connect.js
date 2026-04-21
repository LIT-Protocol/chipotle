/**
 * Browser-wallet connect helpers for sovereign-mode direct contract writes.
 *
 * Scope (CPL-267 Phase 1):
 *   - EOA / browser-injected providers only (MetaMask, Rabby, etc.).
 *   - Chain-switching prompt (wallet_switchEthereumChain / wallet_addEthereumChain).
 *   - Account-change + chain-change listener teardown on disconnect.
 *
 * Deferred to later Phase 1 sub-tasks:
 *   - WalletConnect v2 / Safe Transaction Service integration (monitor app
 *     already has a working EthereumProvider flow we'll lift from).
 */

function getEthers() {
  if (typeof globalThis.ethers !== 'undefined') return globalThis.ethers;
  throw new Error('wallet_connect: ethers is not loaded. Preload it before importing this module.');
}

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
  source: null,          // 'eoa' for this Phase 1 pass
};

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
 * Connect to window.ethereum (EOA). Throws if no injected provider or user
 * rejects. Wires up accountsChanged / chainChanged / disconnect listeners.
 *
 * @returns {Promise<{signer: import('ethers').Signer, address: string, chainId: number}>}
 */
export async function connectEoa() {
  const ethers = getEthers();
  if (!window.ethereum) {
    throw new Error('No browser wallet found. Install MetaMask (or another EIP-1193 wallet) and reload.');
  }

  const provider = new ethers.BrowserProvider(window.ethereum);
  await provider.send('eth_requestAccounts', []);
  const signer = await provider.getSigner();
  const address = await signer.getAddress();
  const network = await provider.getNetwork();

  detachListeners();
  attachListeners(window.ethereum);

  _state = {
    provider,
    signer,
    address,
    chainId: Number(network.chainId),
    source: 'eoa',
  };
  emit();
  return { signer, address, chainId: _state.chainId };
}

/**
 * Request a chain switch via EIP-3326. If the wallet doesn't know the chain,
 * and `addParams` is provided, fall back to EIP-3085 wallet_addEthereumChain.
 *
 * @param {number} targetChainId
 * @param {Object} [addParams] - EIP-3085 params for wallet_addEthereumChain fallback
 */
export async function switchChain(targetChainId, addParams) {
  if (!window.ethereum) throw new Error('No browser wallet available to switch chains.');
  const hexId = '0x' + Number(targetChainId).toString(16);

  try {
    await window.ethereum.request({
      method: 'wallet_switchEthereumChain',
      params: [{ chainId: hexId }],
    });
  } catch (err) {
    // EIP-3326 error code 4902: chain not added. Try to add then re-switch.
    const code = err?.code ?? err?.data?.originalError?.code;
    if (code === 4902 && addParams) {
      await window.ethereum.request({
        method: 'wallet_addEthereumChain',
        params: [{ ...addParams, chainId: hexId }],
      });
    } else {
      throw err;
    }
  }

  // Refresh state to pick up the new chain / signer.
  const ethers = getEthers();
  const provider = new ethers.BrowserProvider(window.ethereum);
  const signer = await provider.getSigner();
  const network = await provider.getNetwork();
  _state = {
    ..._state,
    provider,
    signer,
    chainId: Number(network.chainId),
  };
  emit();
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
  _state = { provider: null, signer: null, address: null, chainId: null, source: null };
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
  const eth = window.ethereum;
  if (!eth?.removeListener) return;
  if (listeners.accountsChanged) eth.removeListener('accountsChanged', listeners.accountsChanged);
  if (listeners.chainChanged) eth.removeListener('chainChanged', listeners.chainChanged);
  if (listeners.disconnect) eth.removeListener('disconnect', listeners.disconnect);
  listeners.accountsChanged = null;
  listeners.chainChanged = null;
  listeners.disconnect = null;
}
