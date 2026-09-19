# Private Apps on Lit — A Backend-as-Lit-Actions Framework

Status: design proposal / RFC
Author: Chris (with Claude)
Date: 2026-06-04

## The pitch

Let a developer host only a frontend (Vercel, Netlify, S3, IPFS — anywhere) and
have **every backend route run as a Lit Action**, with application data living in
Postgres where **row contents are encrypted at rest and only ever decrypted
inside the TEE**, on an as-needed basis. Index fields stay plaintext so the
database can still filter, sort, and join. The result: a "serverless backend"
where the people running the database and the relay **cannot read the data**, and
where the exact code that touches plaintext is auditable by its IPFS CID.

This is the natural generalization of `examples/dark-pool/` — which already
proves the "encrypted Postgres + decrypt-and-compute-in-TEE" mechanic for one
narrow use case. This plan turns that mechanic into a reusable **framework** for
building arbitrary private CRUD apps.

## The form-factor question (the honest answer up front)

> "Is the best form factor a JS library people import into their web app and use
> as the backend instead of hosting a backend elsewhere?"

**Almost — but a pure frontend-only library is not possible, and it's worth being
precise about why.** Running a Lit Action requires a **usage API key** that gates
execution at the gateway (see `examples/dark-pool/scripts/setup.js:217`
`createUsageApiKey`, scoped to `execute_in_groups`). That key is a secret. If you
ship it in the browser bundle, anyone can pull it and run your actions / drain
your account. So you cannot eliminate the server entirely.

What you **can** do is shrink the server to a **stateless ~50-line relay** that:

- holds the usage API key,
- maps a route name → an action CID,
- forwards `{ route, args, userAuth }` to `POST /core/v1/lit_action`,
- returns the response verbatim.

The relay holds **no application logic, no database credentials, and never sees
plaintext**. All of that lives in Lit Actions and the TEE. So the *effective*
answer to the user is: "you write your backend as a library of route handlers,
and the only thing you deploy yourself is a trivial relay (or use ours)."

**Therefore the recommended form factor is a framework, not a single library** —
made of four pieces:

```
┌─────────────────────────────────────────────────────────────────────┐
│ 1. @lit/private-app           in-action runtime (imported into the    │
│                               action code via jsDelivr ESM):          │
│                               router · encrypted-ORM · Neon client ·  │
│                               auth/session verification · decrypt-    │
│                               DB-url helper                           │
│                                                                       │
│ 2. lit-app  (CLI)             bundle routes → action(s), compute      │
│                               CIDs, mint vault PKP + group + usage     │
│                               key, run migrations, write config,      │
│                               deploy the relay.  Wraps setup.js.      │
│                                                                       │
│ 3. @lit/private-app/client    frontend SDK: typed RPC to your routes  │
│                               through the relay; user auth helpers    │
│                                                                       │
│ 4. relay                      stateless edge function (Cloudflare /   │
│                               Vercel) OR a tiny Express app — holds    │
│                               the usage key, forwards calls           │
└─────────────────────────────────────────────────────────────────────┘
```

(Working name TBD; "Salsa" / "Burrito" fit the existing `chipotle` theme of the
API host `api.chipotle.litprotocol.com`. Use a placeholder `@lit/private-app`
below.)

## Non-goals

- **Replacing Postgres-the-database.** We use a SQL-over-HTTP provider (Neon, or
  any HTTP-fronted Postgres). A Lit Action only has `fetch` — it cannot open a
  raw TCP Postgres socket (see `examples/dark-pool/README.md:79`).
- **Full encrypted-query / homomorphic search.** Equality search on sensitive
  columns is supported via blind indexes; range queries and full-text search on
  encrypted columns are out of scope.
- **Hiding metadata.** Plaintext index columns, row counts, and timing are
  visible to the DB operator — same caveat as the dark pool
  (`examples/dark-pool/README.md:102` privacy table). We document this loudly,
  we don't pretend to solve it.
- **A new persistence engine inside the TEE.** Actions stay stateless; all state
  is in Postgres.

## How a developer builds an app (target DX)

The whole point is that this should *feel* like writing Next.js API routes or a
tiny Express app — the privacy is mechanical, not something the dev hand-rolls.

### 1. Define models — declare which fields are encrypted vs. indexed

```js
// app/models.js
import { defineModel, indexed, encrypted } from "@lit/private-app";

export const Note = defineModel("notes", {
  id:        indexed.uuid(),          // plaintext PK
  owner:     indexed.address(),       // plaintext — filterable / joinable
  createdAt: indexed.timestamp(),     // plaintext — sortable
  title:     encrypted.text(),        // sealed at rest
  body:      encrypted.text(),        // sealed at rest
  tags:      encrypted.json(),        // sealed at rest
  emailHash: indexed.blind(),         // HMAC blind index — equality-searchable
                                      // without exposing the email
});
```

- `indexed.*` → a real plaintext column. You can `WHERE` / `ORDER BY` / `JOIN`
  on it. The DB operator can read it.
- `encrypted.*` → folded into **one** `ciphertext` column per row (see Data
  model below). Never queryable; only visible inside the TEE.
- `indexed.blind()` → stores `HMAC(secret, value)`. Lets you do exact-match
  lookups (`where({ emailHash: blind(email) })`) on an otherwise-secret value
  without ever storing it in the clear. The HMAC key is itself an encrypted
  secret decrypted in-action.

### 2. Write routes — plain async handlers

```js
// app/routes/notes.js
import { route } from "@lit/private-app";
import { Note } from "../models.js";

export const create = route(async (ctx, { title, body, tags }) => {
  const user = await ctx.requireUser();              // verifies the session
  return Note.insert({
    owner: user.address, title, body, tags, createdAt: ctx.now,
  });                                                // auto-encrypts title/body/tags
});

export const list = route(async (ctx, { limit = 20, cursor }) => {
  const user = await ctx.requireUser();
  // Filters on the PLAINTEXT `owner` index, paginates, then decrypts only the
  // rows on this page — keeps decrypt count bounded (see Performance below).
  return Note.where({ owner: user.address })
             .orderBy("createdAt", "desc")
             .paginate({ limit, cursor })
             .all();                                 // auto-decrypts the page
});

export const remove = route(async (ctx, { id }) => {
  const user = await ctx.requireUser();
  const note = await Note.find(id);
  if (!note || note.owner !== user.address) throw ctx.forbidden();
  return Note.delete(id);
});
```

### 3. Deploy

```bash
npx lit-app deploy
```

The CLI (wrapping the `examples/dark-pool/scripts/setup.js` flow) does:

1. Bundle each route file into Lit Action source (the `@lit/private-app`
   in-action runtime is inlined via the existing SWC bundler / jsDelivr imports).
2. Compute each action's CID (`get_lit_action_ipfs_id`).
3. Mint the **vault PKP** (encrypts row contents + the DB URL + the blind-index
   HMAC key) — `create_wallet`.
4. Create a **group**, add the PKP, **pin every route's CID** (no wildcard —
   `cid_hashes_permitted: []` then explicit `add_action_to_group`, exactly as
   dark-pool does at `setup.js:99-104`).
5. Mint a **usage API key** scoped to `execute_in_groups: [groupId]`.
6. Encrypt `DATABASE_URL` (and the HMAC key) against the vault PKP; store only
   ciphertext.
7. Run migrations to create the plaintext index columns + the `ciphertext`
   column for each model.
8. Write `.lit-app/config.json` (route → CID map, PKP id, encrypted DB url,
   group id) and deploy/print the relay with the usage key as its only secret.

### 4. Call it from the frontend

```js
// web/api.js
import { createClient } from "@lit/private-app/client";
export const api = createClient({ url: "/api" });   // points at the relay

// anywhere in the app:
await api.notes.create({ title: "secret", body: "...", tags: ["x"] });
const page = await api.notes.list({ limit: 20 });
```

`api.notes.create` → `POST /api` `{ route: "notes.create", args, auth }` → relay
adds the usage key → `/core/v1/lit_action` with `js_params` → the `notes.create`
action runs in the TEE → returns the result.

## Architecture

```
  browser (frontend only)                    relay (stateless, ~50 LOC)
  ┌────────────────────┐   POST /api          ┌───────────────────────┐
  │ @lit/private-app/  │  {route,args,auth}   │ holds usage API key    │
  │ client             ├─────────────────────►│ route → CID lookup     │
  │ - typed RPC        │                      │ injects api key        │
  │ - user auth (sign  │                      └──────────┬────────────┘
  │   in w/ wallet/JWT)│                                  │ POST /core/v1/lit_action
  └────────────────────┘                                 │ {code|ipfs_id, js_params}
                                                          ▼
                                          ┌──────────────────────────────┐
                                          │ TEE: Lit Action (one per      │
                                          │ route, pinned CID)            │
                                          │  @lit/private-app runtime:    │
                                          │  1. verify ctx.user (auth)    │
                                          │  2. Decrypt(DB url)           │
                                          │  3. query Neon over HTTPS     │
                                          │     (filter on plaintext idx) │
                                          │  4. Encrypt on write /        │
                                          │     Decrypt the page on read  │
                                          │  5. return result (no secrets)│
                                          └───────────────┬──────────────┘
                                                          │ SQL over HTTPS
                                                          ▼
                                          ┌──────────────────────────────┐
                                          │ Neon Postgres                 │
                                          │  idx cols: plaintext          │
                                          │  ciphertext col: opaque blob  │
                                          └──────────────────────────────┘
```

The relay operator and the DB operator each see only ciphertext + plaintext index
metadata. Plaintext app data exists **only** in TEE memory during a call.

## Data model: one ciphertext per row

The single most important design decision, driven by Lit's limits (below):
**bundle all `encrypted.*` columns of a row into one JSON blob and seal it as a
single ciphertext column.** Not one ciphertext per field.

```sql
CREATE TABLE notes (
  id          uuid PRIMARY KEY,
  owner       text NOT NULL,          -- indexed.address
  created_at  timestamptz NOT NULL,   -- indexed.timestamp
  email_hash  bytea,                  -- indexed.blind  (HMAC)
  ciphertext  text NOT NULL           -- Encrypt(JSON{title, body, tags})
);
CREATE INDEX ON notes (owner, created_at DESC);
```

- **Write** = 1 `Lit.Actions.Encrypt` call per row.
- **Read** = 1 `Lit.Actions.Decrypt` call per row returned.

This keeps the count of cryptographic remote ops equal to *rows touched*, not
*rows × encrypted-columns*. The ORM hides the (de)serialization; the dev just
sees fields.

Cost of this choice: rotating or selectively disclosing a single encrypted field
means re-encrypting the whole row blob (acceptable for app data; documented).

## Auth & per-user isolation

Two layers, both needed:

1. **Gateway** — the relay's usage key is required to run any action at all.
   Random internet traffic without the relay can't execute routes.
2. **In-action user identity** — `ctx.requireUser()` verifies an end-user
   credential *inside the action*. Options the runtime supports:
   - **Wallet signature**: client signs a session challenge (nonce + deadline);
     the action recovers it with `ethers.utils.verifyMessage` and exposes
     `ctx.user.address` (the pattern in `docs/lit-actions/patterns.mdx:231` and
     `examples/dark-pool` order auth). Also available: `Lit.Auth.authSigAddress`.
   - **App JWT / session token**: action `fetch`es the app's auth endpoint to
     verify, à la `docs/lit-actions/patterns.mdx:318`.

Row ownership is then enforced in the handler (`note.owner === ctx.user.address`)
— the gateway proves *a* legitimate caller; the in-action check proves *which*
user and what they may touch. This mirrors dark-pool's "the usage key lets you
run the action, but per-record authority comes from a signature verified in the
enclave."

**Stronger isolation (optional, phase 4+):** a **vault PKP per user** so each
user's rows are encrypted under a distinct key (`docs/lit-actions/patterns.mdx:291`
shows `user-alice-data-vault`). This gives crypto-level blast-radius isolation but
multiplies key/group management and runs into PKP-per-user scale; default to
**one app vault PKP** and gate by `owner`, offer per-user vaults as an advanced
mode.

## Performance & limits (the real constraints)

From `docs/lit-actions/limits.mdx`:

| Limit | Default | Implication for the framework |
|---|---|---|
| Execution time | 15 min | fine for CRUD |
| Memory | 64 MB | bounds page size / payload |
| Outbound HTTP per action | **50** | each Neon query is 1 fetch; budget queries per route |
| Response payload | **1 MB** | hard cap on a page of decrypted rows |
| Console log | 100 KB | never log plaintext anyway |
| **Key/signature requests per action** | **10** | ⚠️ **see open question** |

Two things shape the ORM:

1. **Decrypt is a remote op.** Each `Decrypt`/`Encrypt` is a gRPC round-trip to
   the node (`lit-actions/ext/bindings.rs`), and recent work explicitly targets
   this cost (`a8a69edd perf: avoid deferred Lit Action remote ops`). So: filter
   and paginate on **plaintext index columns first**, then decrypt only the rows
   you actually return. Never "decrypt the table to filter it."

2. **The per-action key/signature cap.** Docs say **10**. But `matchEpoch.js`
   decrypts the DB URL **plus every order in a batch (up to 200)** in a single
   action and works — so either `Decrypt` against a PKP-derived symmetric key
   does **not** count toward that cap, or the cap is raised per account. **This is
   load-bearing and must be confirmed** (see Open questions). The framework should
   (a) minimize decrypts regardless, and (b) expose a `maxPageSize` the CLI can
   tune to the account's real cap; if the cap genuinely binds at ~10, the default
   page size shrinks and large lists paginate.

Other notes:
- **No connection pooling** (stateless actions) — Neon HTTP is per-call; this is
  inherent and acceptable for Neon's serverless driver.
- **Cold start** — first call to a freshly-bundled action pays bundle/CID
  resolution; warm thereafter (the API server LRU-caches code by CID).

## Security & trust model (state it plainly, like dark-pool does)

- **You trust the TEE.** Plaintext exists in enclave memory during a call.
  TEEs have known side-channel attacks (`examples/dark-pool/README.md:170`).
- **You trust the pinned action CIDs.** A permitted action *can* exfiltrate
  plaintext if its code chooses to (`docs/lit-actions/secrets.mdx:18`). The
  framework's value is that the in-action runtime is open and the deployed CIDs
  are auditable — `lit-app deploy` prints them, and you can diff a CID against
  the published `@lit/private-app` version. **The CLI must make "what code is
  pinned" trivially verifiable**, or the privacy claim is hand-wavy.
- **Metadata leaks.** Index columns, row counts, and timing are visible to the
  DB operator. `indexed.blind()` mitigates *value* exposure for equality columns
  but not existence/count/timing. Document per-model what's plaintext.
- **The relay is untrusted for confidentiality** (sees only ciphertext-bound
  traffic) but **is trusted for availability and rate-limiting** (it holds the
  usage key; if compromised, an attacker can run your actions / burn your
  account quota, but cannot read data). Recommend the relay also rate-limits and
  optionally checks the user credential cheaply before forwarding.

## Where this lives / build order

Mirror how `dark-pool` proved the mechanic before generalizing:

- **Phase 0 — Reference app (validates the whole pattern end-to-end).**
  Add `examples/private-app-starter/` — a runnable private notes (or chat) app:
  models, a few routes, the setup script (forked from dark-pool's), a tiny relay,
  and a minimal frontend. No framework abstraction yet; hand-written. Proves
  encrypt-on-write / decrypt-the-page / plaintext-index-filter / wallet-auth all
  work together and surfaces the real decrypt-count behavior.

- **Phase 1 — `@lit/private-app` in-action runtime.**
  Extract from Phase 0: `defineModel`, `indexed`/`encrypted`/`blind` field types,
  the query builder (`where/orderBy/paginate/insert/find/delete`), the Neon HTTP
  client (lifted from `matchEpoch.js:253`), the row encrypt/decrypt codec, the
  `route()` wrapper + `ctx` (`requireUser`, `now`, `forbidden`). Lives under
  `lit-actions/packages/` next to `naga-la-types`. Ship TS types.

- **Phase 2 — `lit-app` CLI.**
  Generalize `dark-pool/scripts/setup.js` into a project-aware tool: discover
  models + routes, bundle, compute CIDs, mint PKP/group/usage key, pin CIDs, run
  migrations, write `.lit-app/config.json`, print/deploy the relay. Add
  `lit-app migrate`, `lit-app verify` (re-derive and diff pinned CIDs), and
  `lit-app dev` (local loop).

- **Phase 3 — client SDK + relay templates.**
  `@lit/private-app/client` (`createClient`, typed route proxies, wallet/JWT
  sign-in). Relay templates for Cloudflare Workers, Vercel Edge, and a tiny
  Express app (this repo is literally `lit-node-express` — an Express template is
  the natural reference).

- **Phase 4 — hardening / advanced.**
  Per-user vault PKPs; blind-index range tricks; migration/rotation tooling; a
  `lit-triggers`-driven background-job story (cron/webhook routes, since triggers
  already exist in `examples/lit-triggers/`); request-level rate limiting in the
  relay.

## Open questions (resolve before/within Phase 0)

1. **Does `Lit.Actions.Decrypt` count toward the per-action 10 key/signature
   cap?** `matchEpoch.js` decrypting ~200 rows says no (or the cap is raised).
   This single answer sets the default `maxPageSize` and whether large reads must
   chunk across multiple action calls. Confirm with the runtime/limits owner;
   measure in Phase 0.
2. **One mega-router action vs. one action per route.** Per-route CIDs give
   finer audit + permission granularity (you can pin/unpin a single route) but
   more CIDs to manage and re-pin on change; a single router action is simpler
   but any route edit re-CIDs the whole backend. Lean **per-route** for
   auditability; let the CLI bundle small apps into one action as an option.
3. **Blind-index HMAC key custody.** Store it as an encrypted secret under the
   vault PKP (decrypted in-action) — confirm that's acceptable vs. a separate
   vault, and define rotation (rotating re-derives every blind index → a
   migration job).
4. **Migrations against an encrypted column.** Adding/removing an `encrypted.*`
   field changes the row-blob shape. Define a backfill/migration story (lazy
   re-encrypt on next write vs. a one-shot migration action that pages through
   and re-seals rows — itself bounded by the decrypt cap).
5. **Where does user-auth verification belong** — purely in-action (max trust,
   every call pays a `fetch` to the auth service) vs. partly in the relay (cheap
   pre-check, but the relay becomes slightly trusted)? Default in-action;
   document the relay pre-check as an optimization.
6. **Multi-tenant relay vs. self-hosted.** Offer a hosted relay (Lit-run) so devs
   truly deploy "frontend only," with self-hosting as the escape hatch. Decide if
   the hosted relay is in scope.

## TL;DR recommendation

Ship a **framework** (`@lit/private-app` in-action runtime + `lit-app` CLI +
client SDK + a stateless relay template), not a single browser library — because
the usage API key forces a thin server, but that server can be a logic-free,
plaintext-blind relay. Model data as **plaintext index columns + one sealed
ciphertext blob per row**, filter/paginate on the indexes and decrypt only the
returned page, and gate every route with a gateway usage key *plus* an in-action
user-identity check. Prove it first as `examples/private-app-starter/` (a private
notes app), then extract the framework. The dark pool already demonstrates every
primitive this needs; this plan is mostly about packaging them into a DX a
developer can adopt in an afternoon.
