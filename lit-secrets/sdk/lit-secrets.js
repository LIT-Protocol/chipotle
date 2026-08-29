// lit-secrets agent SDK (zero-dependency, works in Node 18+, Deno, Bun, browsers).
//
// One credential: the agent's usage API key (minted in the lit-secrets
// dashboard). It authenticates to the lit-secrets control plane *and* to
// Chipotle. Plaintext travels Chipotle -> agent only.
//
//   import { LitSecrets } from 'https://secrets.litprotocol.com/sdk/lit-secrets.js';
//   const secrets = new LitSecrets({ usageApiKey: process.env.LIT_SECRETS_KEY });
//   const openaiKey = await secrets.get('OPENAI_API_KEY');

export class LitSecrets {
  /**
   * @param {object} opts
   * @param {string} opts.usageApiKey  Agent key from POST /api/agents.
   * @param {string} [opts.baseUrl]    lit-secrets base URL.
   * @param {typeof fetch} [opts.fetch] Custom fetch (tests, proxies).
   */
  constructor({ usageApiKey, baseUrl = 'https://secrets.litprotocol.com', fetch: f } = {}) {
    if (!usageApiKey) throw new Error('usageApiKey is required');
    this.usageApiKey = usageApiKey;
    this.baseUrl = baseUrl.replace(/\/+$/, '');
    this.fetch = f || globalThis.fetch.bind(globalThis);
  }

  /**
   * Read a plaintext secret. Two hops: (1) lit-secrets issues a signed grant
   * after policy evaluation, (2) Chipotle runs the reader action in the TEE and
   * returns the value straight to us.
   * @returns {Promise<string>}
   */
  async get(name, { version } = {}) {
    const g = await this.grant(name, { version });
    const res = await this.fetch(`${g.chipotle_api_base_url}/core/v1/lit_action`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${this.usageApiKey}`,
        'X-Api-Key': this.usageApiKey,
      },
      body: JSON.stringify({ code: g.action.code, js_params: g.js_params }),
    });
    const body = await parseJson(res);
    if (!res.ok) {
      // Chipotle reports action throws (e.g. "grant expired", "grant signature
      // invalid") as a 500 whose body is a JSON *string*; surface the reason.
      const reason = typeof body === 'string' ? extractActionError(body) : `HTTP ${res.status}`;
      throw new LitSecretsError(`chipotle lit_action failed: ${reason}`, res.status, body);
    }
    if (body.has_error) {
      throw new LitSecretsError('reader action threw', res.status, body);
    }
    const out = typeof body.response === 'string' ? safeParse(body.response) : body.response;
    if (!out || typeof out.value !== 'string') {
      throw new LitSecretsError('reader action returned no value', res.status, body);
    }
    return out.value;
  }

  /** Issue a grant without redeeming it (inspect policy decisions, custom transports). */
  async grant(name, { version } = {}) {
    return this._api('POST', '/api/grants', { name, version });
  }

  /**
   * Ciphertext + vault id for the in-TEE-only tier: pass these as js_params to
   * your own permitted Lit Action and call Lit.Actions.Decrypt inside it.
   */
  async reference(name, { version } = {}) {
    const q = version != null ? `?version=${encodeURIComponent(version)}` : '';
    return this._api('GET', `/api/reference/${encodeURIComponent(name)}${q}`);
  }

  async _api(method, path, body) {
    const res = await this.fetch(`${this.baseUrl}${path}`, {
      method,
      headers: {
        Authorization: `Bearer ${this.usageApiKey}`,
        ...(body ? { 'Content-Type': 'application/json' } : {}),
      },
      body: body ? JSON.stringify(body) : undefined,
    });
    const parsed = await parseJson(res);
    if (!res.ok) {
      const code = parsed && parsed.error ? parsed.error : `http_${res.status}`;
      throw new LitSecretsError(`lit-secrets ${method} ${path} failed: ${code}`, res.status, parsed);
    }
    return parsed;
  }
}

export class LitSecretsError extends Error {
  constructor(message, status, body) {
    super(message);
    this.name = 'LitSecretsError';
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

export default LitSecrets;
