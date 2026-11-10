/**
 * Long-lived billing sessions for ChainSecured (wallet) accounts.
 *
 * The wallet signs one `BillingSession` typed-data message that names a
 * throwaway session key generated here. The session key's private key stays
 * in this browser (localStorage) and signs every billing request, so the
 * wallet's signature alone is not a usable credential. The server accepts the
 * wallet signature for BILLING_SESSION_MAX_AGE_SECONDS after `issuedAt`.
 *
 * Why long-lived: a Safe multisig needs every owner to sign, which can take
 * days. Why the Safe Transaction Service polling: Safe{Wallet} often never
 * hands the finished signature back over WalletConnect once the last owner
 * signs (seen in production, Nov 2026). The signature is still published on
 * the Safe Transaction Service, so we read it from there. The pending session
 * is saved before the wallet prompt, so the user can close the tab and pick
 * the session up again after the other owners sign.
 */

import {
  BILLING_SESSION_MAX_AGE_SECONDS,
  billingSessionProofMessage,
  buildBillingSessionTypedData,
} from '../../core_sdk.js';

const STORAGE_PREFIX = 'lit:billing-session:v1:';

// Stop using a session this long before the server would reject it, so a
// request never races the expiry.
const EXPIRY_MARGIN_SECONDS = 10 * 60;

// How long one call waits for multisig owners before giving up. The pending
// session survives, so the next billing action resumes it.
const DEFAULT_SAFE_WAIT_MS = 10 * 60 * 1000;

// A pending multisig session that the Transaction Service still has not seen
// after this long was never signed by anyone; discard it.
const UNFILED_PENDING_GRACE_SECONDS = 3 * 60;

// Concurrent billing calls share one wallet prompt for this long.
const IN_FLIGHT_REUSE_MS = 2 * 60 * 1000;

// Poll the Safe Transaction Service gently: it rate-limits per IP.
const SAFE_POLL_INITIAL_MS = 4000;
const SAFE_POLL_MAX_MS = 20000;

// Safe Transaction Service hosts, by chain id.
const SAFE_TX_SERVICE = {
  1: 'https://api.safe.global/tx-service/eth',
  10: 'https://api.safe.global/tx-service/oeth',
  137: 'https://api.safe.global/tx-service/pol',
  8453: 'https://api.safe.global/tx-service/base',
  42161: 'https://api.safe.global/tx-service/arb1',
  84532: 'https://api.safe.global/tx-service/basesep',
  11155111: 'https://api.safe.global/tx-service/sep',
};

/** Thrown when a multisig session still lacks owner signatures. */
export class PendingSafeSignaturesError extends Error {
  constructor({ confirmations, threshold, safeAddress }) {
    super(
      `Waiting for Safe owners to sign the billing session (${confirmations} of ${threshold}). `
      + 'It is saved in this browser: once the other owners sign in Safe{Wallet}, try again.',
    );
    this.name = 'PendingSafeSignaturesError';
    this.pendingSafe = true;
    this.confirmations = confirmations;
    this.threshold = threshold;
    this.safeAddress = safeAddress;
  }
}

function getEthers() {
  if (globalThis.ethers) return globalThis.ethers;
  throw new Error('billing_session: ethers is not loaded.');
}

function nowSeconds() {
  return Math.floor(Date.now() / 1000);
}

// One session per (chain, wallet). Sessions for other chains are left alone.
function storageKey(wallet, chainId) {
  return `${STORAGE_PREFIX}${Number(chainId)}:${String(wallet).toLowerCase()}`;
}

function loadSession(wallet, chainId) {
  try {
    const raw = localStorage.getItem(storageKey(wallet, chainId));
    return raw ? JSON.parse(raw) : null;
  } catch {
    return null;
  }
}

function saveSession(wallet, chainId, session) {
  try {
    localStorage.setItem(storageKey(wallet, chainId), JSON.stringify(session));
  } catch {
    // Private mode / storage disabled: the session still works for this page.
  }
}

/** Every stored session key for `wallet`, on any chain. */
function walletKeys(wallet) {
  const suffix = `:${String(wallet).toLowerCase()}`;
  try {
    return Object.keys(localStorage).filter((k) => k.startsWith(STORAGE_PREFIX) && k.endsWith(suffix));
  } catch {
    return [];
  }
}

function forgetWhere(wallet, predicate) {
  if (!wallet) return;
  for (const key of walletKeys(wallet)) {
    try {
      const session = JSON.parse(localStorage.getItem(key) || 'null');
      if (!session || predicate(session)) localStorage.removeItem(key);
    } catch {
      try { localStorage.removeItem(key); } catch {}
    }
  }
}

/** Forget every stored session for `wallet`, signed or pending (sign-out). */
export function forgetBillingSession(wallet) {
  const normalizedWallet = String(wallet).toLowerCase();
  for (const attempt of _activeAttempts) {
    if (attempt.wallet === normalizedWallet) attempt.controller.abort();
  }
  for (const [key, entry] of _inFlight) {
    if (entry.wallet === normalizedWallet) _inFlight.delete(key);
  }
  forgetWhere(wallet, () => true);
}

/**
 * Forget `wallet`'s signed sessions after the server rejected one. Pending
 * (unsigned) multisig sessions are kept: they were never sent to the server,
 * so a rejection cannot have been about them.
 */
export function forgetRejectedBillingSession(wallet) {
  forgetWhere(wallet, (session) => !!session.signature);
}

function sessionIssuedAt(session) {
  return Number(session?.typedData?.message?.issuedAt || 0);
}

function sessionUsable(session, chainId) {
  if (!session || !session.typedData || !session.sessionPrivateKey) return false;
  if (chainId != null && String(session.typedData.domain.chainId) !== String(chainId)) return false;
  const expiresAt = sessionIssuedAt(session) + BILLING_SESSION_MAX_AGE_SECONDS - EXPIRY_MARGIN_SECONDS;
  return nowSeconds() < expiresAt;
}

/** True when a signed, unexpired session exists for `wallet` (no network). */
export function hasSignedBillingSession(wallet) {
  return walletKeys(wallet).some((key) => {
    try {
      const session = JSON.parse(localStorage.getItem(key) || 'null');
      return !!(session && session.signature && sessionUsable(session, null));
    } catch {
      return false;
    }
  });
}

/** Build the X-Wallet-Auth header value with a fresh per-request proof. */
async function headerFor(session) {
  const ethers = getEthers();
  const key = new ethers.Wallet(session.sessionPrivateKey);
  const timestamp = nowSeconds();
  const proofSignature = await key.signMessage(billingSessionProofMessage(key.address, timestamp));
  return btoa(JSON.stringify({
    typed_data: session.typedData,
    signature: session.signature,
    session_proof: { timestamp, signature: proofSignature },
  }));
}

// ─── Safe support ───────────────────────────────────────────────────────────

async function fetchJson(url) {
  const res = await fetch(url, { headers: { Accept: 'application/json' } });
  if (res.status === 404) return null;
  if (!res.ok) throw new Error(`Safe Transaction Service ${res.status} for ${url}`);
  return res.json();
}

/** fetchJson with a few retries for rate limits and network blips. */
async function fetchJsonRetrying(url, attempts = 3) {
  for (let i = 1; ; i++) {
    try {
      return await fetchJson(url);
    } catch (e) {
      if (i >= attempts) throw e;
      await sleep(1000 * i);
    }
  }
}

/**
 * Safe details ({ threshold, version }) if `wallet` is a Safe the Transaction
 * Service knows on `chainId`, else null. Only asks the service when the
 * address has contract code, so EOA addresses are never sent to it.
 */
async function lookupSafe(wallet, chainId, rpcUrl) {
  const base = SAFE_TX_SERVICE[Number(chainId)];
  if (!base || !rpcUrl) return null;
  const ethers = getEthers();
  let code;
  try {
    code = await new ethers.JsonRpcProvider(rpcUrl, Number(chainId), { staticNetwork: true })
      .getCode(wallet);
  } catch (e) {
    console.warn('[billing_session] getCode failed; treating the wallet as a plain account:', e);
    return null;
  }
  if (!code || code === '0x') return null;
  // A contract wallet. If the Safe service is unreachable we cannot tell
  // whether it is a Safe, and guessing "no" would bring back the endless wait
  // on a Safe{Wallet} response that never comes, so fail and let the user
  // retry instead.
  let info;
  try {
    info = await fetchJsonRetrying(`${base}/api/v1/safes/${ethers.getAddress(wallet)}/`);
  } catch (e) {
    throw new Error('Could not reach the Safe Transaction Service to check this wallet. Try again in a minute.', { cause: e });
  }
  if (!info || !info.threshold) return null; // a contract wallet, but not a Safe
  // The server only supports Safes >= 1.3 (bytes32 isValidSignature), so
  // assume that when the service omits the version.
  return { threshold: Number(info.threshold), version: String(info.version || '1.3.0') };
}

function safeSupportsChainIdInDomain(version) {
  const [major, minor] = String(version).split(/[.+]/).map(Number);
  return major > 1 || (major === 1 && minor >= 3);
}

/**
 * The hash Safe{Wallet} files an EIP-712 message under: the Safe's own
 * `SafeMessage` typed-data hash wrapping our EIP-712 digest. Matches
 * CompatibilityFallbackHandler.getMessageHashForSafe.
 */
export function safeMessageHash({ safeAddress, chainId, safeVersion, typedData }) {
  const ethers = getEthers();
  const { primaryType, domain, message } = typedData;
  const digest = ethers.TypedDataEncoder.hash(
    domain,
    { [primaryType]: typedData.types[primaryType] },
    message,
  );
  const safeDomain = safeSupportsChainIdInDomain(safeVersion)
    ? { chainId: Number(chainId), verifyingContract: safeAddress }
    : { verifyingContract: safeAddress };
  return ethers.TypedDataEncoder.hash(
    safeDomain,
    { SafeMessage: [{ name: 'message', type: 'bytes' }] },
    { message: digest },
  );
}

/**
 * Current state of a Safe message: null when it has not been proposed yet,
 * else { confirmations, signature } where `signature` is set once enough
 * owners have signed.
 */
async function readSafeMessage(session) {
  const base = SAFE_TX_SERVICE[Number(session.typedData.domain.chainId)];
  const msg = await fetchJson(`${base}/api/v1/messages/${session.safe.messageHash}/`);
  if (!msg) return null;
  const confirmations = Array.isArray(msg.confirmations) ? msg.confirmations.length : 0;
  const complete = confirmations >= session.safe.threshold && msg.preparedSignature;
  return { confirmations, signature: complete ? msg.preparedSignature : null };
}

function sleep(ms, signal) {
  return new Promise((resolve) => {
    const t = setTimeout(resolve, ms);
    signal?.addEventListener('abort', () => { clearTimeout(t); resolve(); }, { once: true });
  });
}

/**
 * Poll until the Safe message is fully signed. Resolves with the signature,
 * or null if `signal` aborts first. Throws PendingSafeSignaturesError after
 * `maxWaitMs`.
 */
async function pollSafeSignature(session, { onStatus, maxWaitMs, signal }) {
  const deadline = Date.now() + maxWaitMs;
  let delay = SAFE_POLL_INITIAL_MS;
  let last = { confirmations: 0 };
  while (!signal?.aborted) {
    try {
      const state = await readSafeMessage(session);
      if (signal?.aborted) return null;
      if (state) {
        if (state.signature) return state.signature;
        last = state;
        onStatus?.(
          `Waiting for Safe owners to sign the billing session (${state.confirmations} of ${session.safe.threshold}). `
          + 'You can close this page and come back after they sign.',
        );
      }
    } catch (e) {
      console.warn('[billing_session] Safe Transaction Service poll failed:', e);
    }
    if (Date.now() >= deadline) {
      throw new PendingSafeSignaturesError({
        confirmations: last.confirmations,
        threshold: session.safe.threshold,
        safeAddress: session.typedData.message.address,
      });
    }
    await sleep(delay, signal);
    delay = Math.min(SAFE_POLL_MAX_MS, Math.round(delay * 1.5));
  }
  return null;
}

// ─── Public entry point ─────────────────────────────────────────────────────

/**
 * Return an X-Wallet-Auth header value for `wallet`, prompting the wallet only
 * when no usable session exists.
 *
 * @param {object} opts
 * @param {string} opts.wallet     ChainSecured wallet address.
 * @param {number} opts.chainId    Chain the server verifies against.
 * @param {string} [opts.rpcUrl]   RPC for that chain (Safe detection).
 * @param {() => Promise<{signer: import('ethers').Signer}>} opts.connect
 *                                 Connects the wallet when a prompt is needed.
 * @param {(msg: string) => void} [opts.onStatus]  Progress text for the UI.
 * @param {number} [opts.maxSafeWaitMs]  How long to wait on multisig owners.
 */
export async function getBillingSessionHeader(opts) {
  // Concurrent callers (balance load, auto-recharge, a click) share one
  // wallet prompt instead of each filing their own Safe message.
  // A wallet prompt that is dismissed without an answer never settles, so
  // only join attempts started recently; an older one gets a fresh prompt.
  const key = storageKey(opts.wallet, opts.chainId);
  let entry = _inFlight.get(key);
  if (!entry || Date.now() - entry.startedAt > IN_FLIGHT_REUSE_MS) {
    entry = {
      wallet: String(opts.wallet).toLowerCase(),
      controller: new AbortController(),
      startedAt: Date.now(),
      users: 0,
    };
    const attempt = entry;
    _activeAttempts.add(attempt);
    attempt.promise = abortable(obtainSession(opts, attempt.controller.signal), attempt.controller.signal)
      .finally(() => {
        if (_inFlight.get(key) === attempt) _inFlight.delete(key);
      });
    _inFlight.set(key, attempt);
  }
  entry.users++;
  try {
    const session = await entry.promise;
    entry.controller.signal.throwIfAborted();
    const header = await headerFor(session);
    entry.controller.signal.throwIfAborted();
    return header;
  } finally {
    if (--entry.users === 0) _activeAttempts.delete(entry);
  }
}

const _inFlight = new Map();
// Also retain older attempts that are no longer eligible for prompt reuse.
const _activeAttempts = new Set();

function abortable(promise, signal) {
  return new Promise((resolve, reject) => {
    const onAbort = () => reject(signal.reason);
    signal.addEventListener('abort', onAbort, { once: true });
    if (signal.aborted) onAbort();
    promise.then(resolve, reject).finally(() => signal.removeEventListener('abort', onAbort));
  });
}

/** Resolves once a signed session for `wallet` is stored; returns it. */
async function obtainSession({
  wallet,
  chainId,
  rpcUrl,
  connect,
  onStatus,
  maxSafeWaitMs = DEFAULT_SAFE_WAIT_MS,
}, signal) {
  let session = loadSession(wallet, chainId);
  if (session && !sessionUsable(session, chainId)) {
    try { localStorage.removeItem(storageKey(wallet, chainId)); } catch {}
    session = null;
  }
  if (session?.signature) return session;

  // A multisig session started earlier (maybe days ago) that was waiting on
  // other owners: check whether they have signed since. If it never reached
  // the Transaction Service (the user closed the wallet prompt, or the tab,
  // before signing), it can never complete, so start over.
  if (session?.safe) {
    const filed = await readSafeMessage(session).catch(() => 'unknown');
    signal.throwIfAborted();
    const age = nowSeconds() - Number(session.createdAt || 0);
    if (filed === null && age > UNFILED_PENDING_GRACE_SECONDS) {
      try { localStorage.removeItem(storageKey(wallet, chainId)); } catch {}
      session = null;
    } else {
      const signature = await pollSafeSignature(session, { onStatus, maxWaitMs: maxSafeWaitMs, signal });
      signal.throwIfAborted();
      session.signature = signature;
      saveSession(wallet, chainId, session);
      return session;
    }
  }

  const ethers = getEthers();
  const sessionKey = ethers.Wallet.createRandom();
  const typedData = buildBillingSessionTypedData({
    address: wallet,
    sessionKey: sessionKey.address,
    chainId,
    issuedAt: nowSeconds(),
  });
  session = {
    sessionPrivateKey: sessionKey.privateKey,
    typedData,
    signature: null,
    safe: null,
    createdAt: nowSeconds(),
  };

  const safe = await lookupSafe(wallet, chainId, rpcUrl);
  signal.throwIfAborted();
  if (safe) {
    session.safe = {
      threshold: safe.threshold,
      messageHash: safeMessageHash({
        safeAddress: ethers.getAddress(wallet),
        chainId,
        safeVersion: safe.version,
        typedData,
      }),
    };
    // Save before prompting: if the wallet never answers, the session key is
    // what lets us use the signature once the owners finish.
    saveSession(wallet, chainId, session);
  }

  const { signer } = await connect();
  signal.throwIfAborted();
  const { primaryType, domain, message } = typedData;
  const walletSig = signer.signTypedData(domain, { [primaryType]: typedData.types[primaryType] }, message);

  let signature;
  if (!safe) {
    signature = await walletSig;
  } else {
    onStatus?.('Approve the billing session in Safe{Wallet}. Other owners can sign it later.');
    const stopPolling = new AbortController();
    const onAbort = () => stopPolling.abort();
    signal.addEventListener('abort', onAbort, { once: true });
    const fromWallet = walletSig.then(
      (sig) => ({ source: 'wallet', sig }),
      (err) => ({ source: 'wallet', err }),
    );
    const fromService = pollSafeSignature(session, {
      onStatus,
      maxWaitMs: maxSafeWaitMs,
      signal: stopPolling.signal,
    }).then(
      (sig) => ({ source: 'service', sig }),
      (err) => ({ source: 'service', err }),
    );
    try {
      // Whichever produces a signature first wins.
      let result = await Promise.race([fromWallet, fromService]);
      signal.throwIfAborted();
      if (!result.sig && result.source === 'wallet') {
        // The wallet errored or the user declined. If the message reached the
        // Transaction Service, owners are still signing: keep polling.
        // Otherwise nothing was signed, so drop the pending session.
        const filed = await readSafeMessage(session).catch(() => 'unknown');
        signal.throwIfAborted();
        if (filed === null) {
          try { localStorage.removeItem(storageKey(wallet, chainId)); } catch {}
          throw result.err;
        }
        result = await fromService;
      }
      if (!result.sig) throw result.err;
      signature = result.sig;
    } finally {
      signal.removeEventListener('abort', onAbort);
      stopPolling.abort();
    }
  }

  signal.throwIfAborted();
  session.signature = signature;
  saveSession(wallet, chainId, session);
  return session;
}
