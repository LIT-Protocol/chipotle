// lit-agent-keychain agent SDK (zero-dependency, works in Node 18+, Deno, Bun, browsers).
//
// One credential: the agent's usage API key (minted in the lit-agent-keychain
// dashboard). It authenticates to the lit-agent-keychain control plane *and* to
// Chipotle. Plaintext travels Chipotle -> agent only.
//
//   import { LitAgentKeychain } from 'https://keychain.litprotocol.com/sdk/lit-agent-keychain.js';
//   const keychain = new LitAgentKeychain({ usageApiKey: process.env.LIT_AGENT_KEYCHAIN_KEY });
//   const openaiKey = await keychain.get('OPENAI_API_KEY');

export class LitAgentKeychain {
  /**
   * @param {object} opts
   * @param {string} opts.usageApiKey  Agent key from POST /api/agents.
   * @param {string} [opts.baseUrl]    lit-agent-keychain base URL.
   * @param {typeof fetch} [opts.fetch] Custom fetch (tests, proxies).
   * @param {number} [opts.timeoutMs]  Per-request deadline (default 30000). A
   *   stalled or trickling upstream otherwise leaves credential loading hung
   *   forever, blocking agent startup.
   */
  constructor({ usageApiKey, baseUrl = 'https://keychain.litprotocol.com', fetch: f, timeoutMs = 30000 } = {}) {
    if (!usageApiKey) throw new Error('usageApiKey is required');
    this.usageApiKey = usageApiKey;
    this.baseUrl = baseUrl.replace(/\/+$/, '');
    this.fetch = f || globalThis.fetch.bind(globalThis);
    this.timeoutMs = timeoutMs;
  }

  /**
   * fetch with a bounded deadline. Callers may pass their own `signal`; it is
   * combined with the timeout so either can abort the request.
   */
  async _fetch(url, opts, signal) {
    const ac = new AbortController();
    const timer = setTimeout(() => ac.abort(new Error(`request exceeded ${this.timeoutMs}ms`)), this.timeoutMs);
    if (signal) {
      if (signal.aborted) ac.abort(signal.reason);
      else signal.addEventListener('abort', () => ac.abort(signal.reason), { once: true });
    }
    try {
      return await this.fetch(url, { ...opts, signal: ac.signal });
    } catch (e) {
      if (ac.signal.aborted) {
        throw new LitAgentKeychainError(`lit-agent-keychain request aborted: ${ac.signal.reason?.message || 'timeout'}`, 0, null);
      }
      throw e;
    } finally {
      clearTimeout(timer);
    }
  }

  /**
   * Read a plaintext secret. Two hops: (1) lit-agent-keychain issues a signed grant
   * after policy evaluation, (2) Chipotle runs the reader action in the TEE and
   * returns the value straight to us.
   * @param {object} [opts]
   * @param {number} [opts.version]
   * @param {AbortSignal} [opts.signal]  Caller cancellation.
   * @returns {Promise<string>}
   */
  async get(name, { version, signal } = {}) {
    const g = await this.grant(name, { version, signal });
    const res = await this._fetch(`${g.chipotle_api_base_url}/core/v1/lit_action`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${this.usageApiKey}`,
        'X-Api-Key': this.usageApiKey,
      },
      body: JSON.stringify({ code: g.action.code, js_params: g.js_params }),
    }, signal);
    const body = await parseJson(res);
    if (!res.ok) {
      // Chipotle reports action throws (e.g. "grant expired", "grant signature
      // invalid") as a 500 whose body is a JSON *string*; surface the reason.
      const reason = typeof body === 'string' ? extractActionError(body) : `HTTP ${res.status}`;
      throw new LitAgentKeychainError(`chipotle lit_action failed: ${reason}`, res.status, body);
    }
    if (body.has_error) {
      throw new LitAgentKeychainError('reader action threw', res.status, body);
    }
    const out = typeof body.response === 'string' ? safeParse(body.response) : body.response;
    if (!out || typeof out.value !== 'string') {
      throw new LitAgentKeychainError('reader action returned no value', res.status, body);
    }
    return out.value;
  }

  /** Issue a grant without redeeming it (inspect policy decisions, custom transports). */
  async grant(name, { version, signal } = {}) {
    return this._api('POST', '/api/grants', { name, version }, signal);
  }

  /**
   * Ciphertext + vault id for the in-TEE-only tier: pass these as js_params to
   * your own permitted Lit Action and call Lit.Actions.Decrypt inside it.
   */
  async reference(name, { version, signal } = {}) {
    const q = version != null ? `?version=${encodeURIComponent(version)}` : '';
    return this._api('GET', `/api/reference/${encodeURIComponent(name)}${q}`, undefined, signal);
  }

  async _api(method, path, body, signal) {
    const res = await this._fetch(`${this.baseUrl}${path}`, {
      method,
      headers: {
        Authorization: `Bearer ${this.usageApiKey}`,
        ...(body ? { 'Content-Type': 'application/json' } : {}),
      },
      body: body ? JSON.stringify(body) : undefined,
    }, signal);
    const parsed = await parseJson(res);
    if (!res.ok) {
      const code = parsed && parsed.error ? parsed.error : `http_${res.status}`;
      throw new LitAgentKeychainError(`lit-agent-keychain ${method} ${path} failed: ${code}`, res.status, parsed);
    }
    return parsed;
  }
}

export class LitAgentKeychainError extends Error {
  constructor(message, status, body) {
    super(message);
    this.name = 'LitAgentKeychainError';
    this.status = status;
    this.body = body;
    /** Policy denial code (e.g. "rate_limited") when the control plane refused. */
    this.code = body && body.error ? body.error : undefined;
  }
}

function extractActionError(text) {
  const m = /Error: ([^\n]+)/.exec(text);
  return m ? m[1].trim() : text.slice(0, 200);
}

async function parseJson(res) {
  const text = await res.text();
  return safeParse(text);
}

function safeParse(text) {
  if (!text) return null;
  try {
    return JSON.parse(text);
  } catch {
    return { raw: text };
  }
}

export default LitAgentKeychain;
