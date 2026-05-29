// webhook-echo — the canonical starter / smoke test.
//
// The lit-triggers runtime invokes `main(params)` and wraps the returned value
// in Lit.Actions.setResponse(). Do NOT call main() yourself.
//
// For a webhook trigger, `params` looks like:
//   { source: "webhook", event: <parsed JSON body>, headers: { ... } }
const main = async (params) => {
  return {
    ok: true,
    received_at: new Date().toISOString(),
    source: (params && params.source) || null,
    event: (params && params.event) || null,
    header_keys: params && params.headers ? Object.keys(params.headers) : [],
  };
};
