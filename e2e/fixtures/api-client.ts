import { API_BASE_URL } from '../playwright.config';

/**
 * Minimal HTTP client for lit-api-server's /core/v1 routes. The k6 suite has a
 * generated TypeScript client (`k6/litApiServer.ts`) but it depends on the k6
 * runtime — we hand-roll the small subset we actually call from tests.
 *
 * Surface (parity with k6/smoke + k6/correctness):
 *   GET  /core/v1/get_node_chain_config
 *   POST /core/v1/new_account
 *   POST /core/v1/add_usage_api_key            (X-Api-Key: account key)
 *   POST /core/v1/lit_action                   (X-Api-Key: usage key)
 *   POST /core/v1/get_lit_action_ipfs_id
 *   GET  /core/v1/billing/balance              (X-Api-Key: account key)
 */

export interface NodeChainConfig {
  chain_id: number;
  rpc_url?: string;
  [k: string]: unknown;
}

export interface NewAccountResponse {
  api_key: string;
  wallet_address: string;
}

export interface AddUsageKeyResponse {
  usage_api_key: string;
}

export interface LitActionResponse {
  has_error: boolean;
  response: unknown;
  logs?: string;
  [k: string]: unknown;
}

export interface BillingBalanceResponse {
  // negative when credits are available (Stripe convention used by k6)
  balance_cents: number;
}

export const HELLO_WORLD_ACTION = `async function main() {
  return "Hello World!";
}
`;

export const ENCRYPT_ACTION = `async function main({ pkpId, challenge }) {
  const result = await Lit.Actions.Encrypt({ pkpId, message: challenge });
  return result;
}
`;

export const DECRYPT_ACTION = `async function main({ pkpId, ciphertext }) {
  const result = await Lit.Actions.Decrypt({ pkpId, ciphertext });
  return result;
}
`;

export const ECDSA_SIGN_ACTION = `async function main() {
  const privateKey = await Lit.Actions.getLitActionPrivateKey();
  const wallet = new ethers.Wallet(privateKey);
  const signature = await wallet.signMessage("Chipotle Rocks!");
  return {
    wallet_address: wallet.address,
    signature,
    publicKey: wallet.publicKey,
  };
}
`;

export class LitApiClient {
  constructor(public readonly baseUrl: string = API_BASE_URL) {}

  private url(path: string): string {
    return `${this.baseUrl.replace(/\/$/, '')}/core/v1${path}`;
  }

  private async request<T>(
    method: 'GET' | 'POST',
    path: string,
    opts: { body?: unknown; headers?: Record<string, string> } = {},
  ): Promise<T> {
    const headers: Record<string, string> = {
      ...(opts.body !== undefined ? { 'content-type': 'application/json' } : {}),
      ...(opts.headers ?? {}),
    };
    const res = await fetch(this.url(path), {
      method,
      headers,
      body: opts.body !== undefined ? JSON.stringify(opts.body) : undefined,
    });
    const text = await res.text();
    if (!res.ok) {
      throw new Error(`${method} ${path} → ${res.status} ${res.statusText}: ${text.slice(0, 500)}`);
    }
    // Most routes return JSON; a couple (`get_lit_action_ipfs_id`) return a bare string.
    try {
      return JSON.parse(text) as T;
    } catch {
      return text as unknown as T;
    }
  }

  getNodeChainConfig(): Promise<NodeChainConfig> {
    return this.request<NodeChainConfig>('GET', '/get_node_chain_config');
  }

  newAccount(input: {
    account_name: string;
    account_description?: string;
  }): Promise<NewAccountResponse> {
    return this.request<NewAccountResponse>('POST', '/new_account', { body: input });
  }

  addUsageApiKey(
    apiKey: string,
    input: {
      name: string;
      description?: string;
      can_create_groups?: boolean;
      can_delete_groups?: boolean;
      can_create_pkps?: boolean;
      manage_ipfs_ids_in_groups?: number[];
      add_pkp_to_groups?: number[];
      remove_pkp_from_groups?: number[];
      execute_in_groups?: number[];
    },
  ): Promise<AddUsageKeyResponse> {
    return this.request<AddUsageKeyResponse>('POST', '/add_usage_api_key', {
      headers: { 'x-api-key': apiKey },
      body: {
        can_create_groups: false,
        can_delete_groups: false,
        can_create_pkps: false,
        manage_ipfs_ids_in_groups: [],
        add_pkp_to_groups: [],
        remove_pkp_from_groups: [],
        execute_in_groups: [0],
        ...input,
      },
    });
  }

  litAction(
    usageApiKey: string,
    input: { code: string; js_params?: unknown },
  ): Promise<LitActionResponse> {
    return this.request<LitActionResponse>('POST', '/lit_action', {
      headers: { 'x-api-key': usageApiKey },
      body: { code: input.code, js_params: input.js_params ?? null },
    });
  }

  getLitActionIpfsId(code: string): Promise<string> {
    return this.request<string>('POST', '/get_lit_action_ipfs_id', { body: code });
  }

  async billingBalance(apiKey: string): Promise<BillingBalanceResponse | null> {
    try {
      return await this.request<BillingBalanceResponse>('GET', '/billing/balance', {
        headers: { 'x-api-key': apiKey },
      });
    } catch {
      // local_test.sh runs without Stripe; the endpoint is allowed to 404/500
      // and the dashboard interprets that as "Payment Not Required".
      return null;
    }
  }
}

export const apiClient = new LitApiClient();
