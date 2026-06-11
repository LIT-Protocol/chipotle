import type { FetchLike } from './types';

export interface HttpRequest {
  method: 'GET' | 'POST' | 'DELETE';
  url: string;
  headers?: Record<string, string>;
  body?: string;
}

export interface HttpResponse {
  status: number;
  ok: boolean;
  text: string;
}

/**
 * Single choke point for venue HTTP. The proxy decision is baked into
 * `fetchImpl` upstream (see transports.resolveFetch) — Deno's fetch drops
 * unknown init fields, so per-request proxying can't ride on the init object;
 * it must be the transport itself.
 */
export async function httpRequest(fetchImpl: FetchLike, req: HttpRequest): Promise<HttpResponse> {
  const init: Record<string, unknown> = { method: req.method, headers: req.headers ?? {} };
  if (req.body !== undefined) init.body = req.body;
  const res = await fetchImpl(req.url, init);
  const text = await res.text();
  return { status: res.status, ok: res.ok, text };
}
