import type { FetchLike, VenueConfig } from './types';

/**
 * Proxy transport for venue requests. The in-TEE path routes through the
 * `Lit.Actions.proxiedFetch` op (chipotle plan D4/M2), which egresses via an
 * authenticated proxy so a venue sees a chosen non-US IP. In Node/local
 * validation the caller supplies a proxy-capable `fetchImpl` instead (e.g. an
 * undici ProxyAgent), keeping this library dependency-free.
 */

interface ProxiedFetchSdk {
  proxiedFetch(req: {
    url: string;
    method?: string;
    headers?: Array<[string, string]> | Record<string, string>;
    body?: string | null;
    proxy?: string | null;
  }): Promise<{ status: number; ok: boolean; text(): Promise<string> }>;
}

function litActionsSdk(): ProxiedFetchSdk | undefined {
  const g = globalThis as { Lit?: { Actions?: unknown }; LitActions?: unknown };
  const sdk = (g.Lit && g.Lit.Actions) || g.LitActions;
  return sdk && typeof (sdk as ProxiedFetchSdk).proxiedFetch === 'function'
    ? (sdk as ProxiedFetchSdk)
    : undefined;
}

/** A FetchLike that routes through the in-TEE proxied-fetch op. */
export function litActionProxiedFetch(proxyUrl?: string): FetchLike {
  const sdk = litActionsSdk();
  if (!sdk) {
    throw new Error(
      'lit-venues: proxy requested but Lit.Actions.proxiedFetch is unavailable — ' +
        'update the Lit node runtime (chipotle M2) or pass an explicit proxy-capable fetchImpl',
    );
  }
  return (url, init = {}) =>
    sdk.proxiedFetch({
      url,
      method: (init.method as string | undefined) ?? 'GET',
      headers: (init.headers as Record<string, string> | undefined) ?? {},
      body: (init.body as string | undefined) ?? null,
      proxy: proxyUrl ?? null,
    });
}

/**
 * Resolve the fetch a venue client should use:
 *   explicit fetchImpl  >  proxy transport (in-TEE op)  >  global fetch.
 */
export function resolveFetch(cfg: Pick<VenueConfig, 'fetchImpl' | 'proxy'>): FetchLike {
  if (cfg.fetchImpl) return cfg.fetchImpl;
  if (cfg.proxy) return litActionProxiedFetch(cfg.proxy);
  const f = (globalThis as { fetch?: FetchLike }).fetch;
  if (!f) {
    throw new Error('lit-venues: no global fetch available — pass a fetchImpl in VenueConfig');
  }
  return f;
}
