// Shared Lit Chipotle REST helpers for the orchestrator scripts.

async function call(base, apiKey, p, init = {}) {
  const res = await fetch(`${base}/core/v1/${p}`, {
    ...init,
    headers: { "X-Api-Key": apiKey, "Content-Type": "application/json", ...(init.headers || {}) },
  });
  const body = await res.json();
  if (!res.ok) {
    const msg = body.message || body.error || JSON.stringify(body);
    const err = new Error(`${p} -> ${res.status}: ${msg}`);
    err.body = body;
    throw err;
  }
  return body;
}

// Execute an action (passed as source code) and return its response.
async function runAction(base, apiKey, code, jsParams) {
  const body = await call(base, apiKey, "lit_action", {
    method: "POST",
    body: JSON.stringify({ code, js_params: jsParams }),
  });
  if (body.has_error) throw new Error(`action error: ${body.logs || JSON.stringify(body)}`);
  return body.response;
}

module.exports = { call, runAction };
