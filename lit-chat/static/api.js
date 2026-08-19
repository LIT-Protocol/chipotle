// API client. Same-origin, cookie-authed; CSRF token attached to every
// state-changing call.

let csrfToken = null;

export function setCsrf(token) { csrfToken = token; }

async function request(method, path, body) {
  const headers = {};
  if (body !== undefined) headers['Content-Type'] = 'application/json';
  if (method !== 'GET' && csrfToken) headers['X-CSRF-Token'] = csrfToken;
  const res = await fetch(path, {
    method,
    headers,
    credentials: 'same-origin',
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (res.status === 401) throw Object.assign(new Error('unauthorized'), { code: 401 });
  if (!res.ok) {
    let slug = 'server_error';
    try { slug = (await res.json()).error || slug; } catch { /* not json */ }
    throw Object.assign(new Error(slug), { code: res.status, slug });
  }
  return res.json();
}

export const api = {
  anonSession: () => request('POST', '/api/session/anon'),
  me: () => request('GET', '/api/me'),
  models: () => request('GET', '/api/models'),
  status: () => request('GET', '/api/status'),
  listConversations: () => request('GET', '/api/conversations'),
  createConversation: (modelId) => request('POST', '/api/conversations', { model_id: modelId }),
  renameConversation: (id, title, expectedVersion) =>
    request('PATCH', `/api/conversations/${id}`, { title, expected_version: expectedVersion }),
  deleteConversation: (id) => request('DELETE', `/api/conversations/${id}`),
  listMessages: (id) => request('GET', `/api/conversations/${id}/messages`),
  requestCode: (email) => request('POST', '/api/auth/request', { email }),
  verifyCode: (code) => request('POST', '/api/auth/verify', { code }),
  logout: () => request('POST', '/api/auth/logout'),
  exportData: () => request('GET', '/api/export'),
  deleteAccount: () => request('POST', '/api/account/delete'),
};

// SSE-over-fetch: POST /api/chat/stream and parse the event stream.
// handlers: {meta, delta, done, error}; returns an abort function.
export function streamChat(payload, handlers) {
  const controller = new AbortController();
  (async () => {
    let res;
    try {
      res = await fetch('/api/chat/stream', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'X-CSRF-Token': csrfToken || '',
        },
        credentials: 'same-origin',
        body: JSON.stringify(payload),
        signal: controller.signal,
      });
    } catch (e) {
      if (e.name !== 'AbortError') handlers.error?.({ error: 'network_error' });
      return;
    }
    if (!res.ok) {
      let slug = 'server_error';
      try { slug = (await res.json()).error || slug; } catch { /* not json */ }
      handlers.error?.({ error: slug, code: res.status });
      return;
    }
    const reader = res.body.getReader();
    const decoder = new TextDecoder();
    let buf = '';
    try {
      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        buf += decoder.decode(value, { stream: true });
        let sep;
        while ((sep = buf.indexOf('\n\n')) >= 0) {
          const frame = buf.slice(0, sep);
          buf = buf.slice(sep + 2);
          let event = 'message';
          let data = '';
          for (const line of frame.split('\n')) {
            if (line.startsWith('event:')) event = line.slice(6).trim();
            else if (line.startsWith('data:')) data += line.slice(5).trim();
          }
          if (!data) continue;
          let parsed;
          try { parsed = JSON.parse(data); } catch { continue; }
          handlers[event]?.(parsed);
        }
      }
    } catch (e) {
      if (e.name !== 'AbortError') handlers.error?.({ error: 'stream_interrupted' });
    }
    handlers.finished?.();
  })();
  return () => controller.abort();
}
