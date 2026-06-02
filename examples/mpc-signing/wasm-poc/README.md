# WASM proof-of-concept (de-risk)

These two scripts proved the hard parts of the
[mpc-signing plan](../../../plans/mpc-signing-example.md)
before any of the real example was written. They run under Deno, which is the
same engine the Lit Action runtime uses (`lit-actions` is Deno v2.2.2; this was
verified on Deno 1.44).

## What they prove

`smoke.ts` — end-to-end, mirroring exactly what the Lit Action will do:

1. Instantiates the DKLs23 wasm (`@silencelaboratories/dkls-wasm-ll-web`, 642 KB)
   from **base64-inlined bytes** via `initSync()` — no fetch, no file URL — the
   mechanism the action uses to carry the module in its source.
2. Runs a full **2-of-2 DKG** then **signing**, where the "Lit Action" party
   (party 1) is serialized to bytes and rebuilt from bytes **between every
   round** — the stateless encrypt-relay pattern the action depends on.
3. Verifies the resulting `[R, S]` signature recovers to the DKG public key's
   **EVM address** — i.e. plain `ecrecover` accepts it.

`measure.ts` — measures the relayed session-blob size per round, raw and
gzipped, against the action's 100 KB response-payload limit. This is how we
found that signing rounds 1–2 (~138 KB gzipped) exceed the default cap while
everything else fits.

## Run it

```bash
# download the library files next to these scripts
curl -s -o dkls-wasm-ll-web.js     https://cdn.jsdelivr.net/npm/@silencelaboratories/dkls-wasm-ll-web@1.2.0/dkls-wasm-ll-web.js
curl -s -o dkls-wasm-ll-web_bg.wasm https://cdn.jsdelivr.net/npm/@silencelaboratories/dkls-wasm-ll-web@1.2.0/dkls-wasm-ll-web_bg.wasm

deno run --allow-read --allow-net smoke.ts
deno run --allow-read measure.ts
```

The `.js`/`.wasm` are intentionally not committed (642 KB binary). The
`npm:@noble/*` imports in `smoke.ts` are only for verification/address
derivation in the test harness — the action itself uses the `ethers` global.
