# PRD: Lit Chat — a private web chat app served from the TEE

**Status**: Draft for review
**Author**: drafted with Claude (3-pass exploration → architecture → adversarial review)
**Date**: 2026-08-18
**Working name**: Lit Chat (final name TBD)

---

## 1. Summary

A web chat app (Claude/ChatGPT-style) whose serving stack runs inside a TEE (Phala dstack CVM in the Chipotle environment) and whose chat history is stored **outside** the TEE as ciphertext, encrypted under per-user keys that are derived — and only ever exist — inside the enclave.

The headline user promise, privacy first, phrased at exactly the strength the architecture delivers:

> **Your conversations are private — by architecture, not by policy.**
> **Your chat history is post-quantum encrypted, and Lit cannot access it.**

Why the post-quantum claim is honest: the storage path is *symmetric-only* — AES-256-GCM envelope encryption under enclave-derived KEKs (keccak/HKDF derivation), with no public-key cryptography anywhere at rest — which is quantum-resistant under current NIST guidance. The claim is scoped to stored history; the TLS channel and TDX attestation signatures are classical (see §7.2). Supporting line for the verify page: *the code that could read your history is attested and auditable.*

The product is three parts, mapping to the request:

| Part | What | Where |
|---|---|---|
| 1 | Chat web UI (conversations, streaming, model picker) | Served from inside the chat CVM |
| 2 | Inference: OpenRouter proxy (P1) → in-enclave model (P3) | Dedicated Phala CVM (`lit-chat`) |
| 3 | Encrypted session/history store | Off-TEE Postgres, ciphertext-only |

It is also a flagship demonstration of what the Chipotle platform is for — and, via Lit Action tool-calling (P2), the first chat assistant with governed wallet powers.

## 2. Goals and non-goals

### Goals
- G1. Zero-friction private chat: no signup required; an anonymous visitor gets a working, private, persistent-in-browser-lifetime chat in one click.
- G2. Provable operator blindness at rest: chat content is never persisted or logged in plaintext outside the enclave; the claim is backed by attestation (compose_hash → digest-pinned images → published source).
- G3. Optional accounts: email magic-link login upgrades an anonymous user to a persistent, multi-device identity; history migrates.
- G4. Model choice via OpenRouter with per-model privacy labeling; a path to models that run *inside* the enclave so prompts never leave it.
- G5. Showcase Chipotle: the app is a public, open-source-able reference for "TEE service + off-TEE encrypted storage + attestation UX" on the platform.

### Non-goals (v1)
- Client-side E2EE (the model must see plaintext to respond; the enclave is the trust boundary, not the browser).
- Mobile apps, shared/team conversations, fine-tuning, RAG/file-upload corpus features.
- Replacing the developer dashboard; Lit Chat is consumer-facing and does not require a Chipotle developer account.
- Being a general LLM API gateway (no public API surface for third parties in v1).

## 3. Users

- **Anonymous visitor** — wants private AI chat without creating an account or trusting a provider's retention policy. Tolerates "lose the cookie, lose the history."
- **Account holder** — wants history across devices; accepts email as identity.
- **Crypto-native user (P2)** — wants the assistant to *do* things: sign, check balances, run verifiable fetches, via Lit Action tools bound to their wallet permissions.
- **Platform evaluator** — a developer using the app + verify page as the proof-of-concept for building on Chipotle.

## 4. Product requirements

### 4.1 Part 1 — Chat web app

**Stack**: house style — vanilla ES modules, no build step (per `lit-static` conventions and `DESIGN.md`), served by the `lit-chat` service itself (the `lit-triggers` Rocket `FileServer` pattern). **All assets self-hosted — no CDN imports** (the dashboard's runtime-CDN habit is explicitly not inherited; CDN JS would sit outside the attested image and outside the CSP).

**v1 features**
- Conversation list (encrypted titles, auto-titled after first exchange), create/rename/delete.
- Streaming responses (SSE), markdown rendering, stop generation, regenerate, copy.
- Model picker from a curated OpenRouter subset; each model carries a **privacy badge**: *External model — prompts leave the enclave; routed only to zero-data-retention providers* vs (P3) *In-enclave — prompts never leave the TEE*.
- Privacy panel: what's protected, what isn't, attestation status, link to verify page, key-custody explainer ("your key is derived inside the enclave; we can't derive it without your session").
- Anonymous by default; "Save my history" → email magic link → account upgrade (history rewrapped, §5.4).
- Data export (decrypt-for-owner → JSON download) and account deletion.
- Honest empty-state and deletion copy (see §7 wording rules).

**Hard client-side security requirements**
- Model output is attacker-influenceable content: sanitize before render (self-hosted DOMPurify or equivalent), no `innerHTML` of unsanitized output, strict CSP (self-only scripts, no inline), `__Host-` cookie prefix, SameSite=Lax + CSRF token on state-changing routes.
- Latency budget: TTFT p95 ≤ 1.5s over the proxy path (excluding model queue time), sustained ≥ 30 tokens/sec passthrough.

### 4.2 Part 2 — Inference in the TEE

**Decision: `lit-chat` is a dedicated service in its own Phala CVM** (own docker-compose, own compose_hash, own attestation, own domain, e.g. `chat.litprotocol.com`) — **not** a Lit Action, and **not** a container added to the core Chipotle CVM.

Why not a Lit Action (JS or gVisor binary): the action path is structurally wrong for chat — request/response only with a 1 MB buffered response (no SSE), no persistence ops (an action cannot touch a database; the op table is the whole trust boundary), $0.01/sec platform billing (a 20s generation = $0.20 before model costs), billing-flush failures cancel actions mid-flight, and `/lit_binary_action` is gated off in prod on three fail-closed axes. The gRPC layer under the runners is bidi-streaming, so actions *could* grow streaming one day, but chat should not wait on that.

Why not a sibling container in the core CVM (this was the pass-2 draft; reversed after adversarial review):
1. **Release cadence**: every core-CVM compose change is a Safe-multisig ceremony on Base plus staging soak — forever, per release, not one-time. A consumer app iterating weekly cannot live there.
2. **Key-oracle risk**: any container mounting `/var/run/dstack.sock` can call `get_key(path, purpose)` with *arbitrary strings* — i.e. it can derive every PKP and AES client key on the platform. Putting the largest new attack surface (a public consumer app parsing untrusted LLM output) next to that socket weakens the core platform.
3. **Ingress**: the core `dstack-ingress` binds 443 with a single `TARGET_ENDPOINT`; a second domain in the same CVM has no clean path. A separate CVM gets its own 443.
4. **Resources**: chat traffic (and P3 CPU inference) would fight the core API's CPU load-shedding guard, which already trips under modest load on the current staging instance size.

A separate CVM gives lit-chat its **own dstack app root**: chat keys and platform PKP keys live in disjoint derivation universes by construction. Chat's compose governance can be lighter-weight (normal review + signed digest pins; multisig optional) without touching the core trust story.

**`lit-chat` CVM composition** (one compose_hash):
- `lit-chat` — Rust (Rocket or Axum; team standard), serving: static frontend, auth/session routes, chat CRUD, `POST /chat/stream` (SSE), OpenRouter proxy, `GET /attestation` + `GET /info` (same dstack endpoints as the core server).
- `dstack-ingress` — TLS termination in-TEE (Let's Encrypt), 443 → `lit-chat`. SSE requires `proxy_buffering off` / matching read timeouts; note the dstack-gateway 10-minute idle close is a non-issue for streaming (tokens are bytes).
- `otel-collector` — subject to the logging rules in §7.3.

**OpenRouter integration**
- The OpenRouter provisioning key is a Phala `encrypted_env` secret, sealed to the CVM — it never exists client-side or in the repo.
- Upstream calls use OpenRouter's SSE; lit-chat forwards tokens to the client as it encrypts-and-buffers the completed message for storage.
- **Catalog policy (hard requirement, not a badge)**: default routing restricted to OpenRouter zero-data-retention-eligible providers; per-model retention/training metadata surfaced in the picker; models that can't meet ZDR are excluded or explicitly labeled opt-in.
- Cost controls: OpenRouter provisioning-key spend caps, per-session daily token budgets, per-IP token-bucket rate limits (the CPL-367 `RateLimiter` pattern), and a global daily spend circuit breaker that degrades to "account holders only" before hard-off.
- BYOK (P2): a user may store their own OpenRouter key, encrypted under their KEK like any other message payload.

**Lit Action tool calls (P2)** — this is where "maybe lit action calls?" lands: not for inference, but as the **tool layer**. The chat service exposes model tools (sign message, wallet balance, verifiable fetch, user-registered action CIDs) that execute via the core `POST /core/v1/lit_action` with a scoped usage API key and the end-user identity in `auth_context`. Tool calls are permission-gated by the existing on-chain group model, individually confirmed in the UI (prompt-injection defense: the model can *propose* a tool call; only the human approves state-changing ones), and rate-limited per session.

**In-enclave inference (P3)**: a `llama-server` (llama.cpp) container joins the chat CVM compose with a small quantized model (1–3B class) baked into the image — weights become part of the attested compose_hash. Set expectations: CPU-only TDX today (no GPU, no `/dev/kvm` in the CVM), so this is a "prompts never leave the enclave" *option*, not a frontier-model replacement; TTFT in seconds. The real P3 investigation is confidential-GPU inference (Phala GPU TEE / H100 CC) — tracked as a research spike, not committed.

### 4.3 Part 3 — Off-TEE encrypted storage

**Decision: dedicated Postgres (Railway, mirroring lit-payments ops), reached directly from `lit-chat` via sqlx over TLS (`sslmode=verify-full`, pinned CA), DB credential sealed via `encrypted_env`.** No storage micro-service in v1; the DB sees only ciphertext, so the service boundary buys nothing. This is the first implementation of the CPL-364 "no data persists inside the TEE / ciphertext-only off-TEE" pattern (that plan is designed, blessed, and unshipped — cite it for shape, don't expect reusable code).

**Envelope encryption**
- **User KEK** (32 bytes): `get_key("chat/v1/user/{user_ref}", "chat-kek")` against the chat CVM's own dstack socket, keccak-wrapped per the platform discipline. Derived on demand, never stored, never leaves the enclave.
- **Per-conversation DEK**: random 32 bytes minted in-enclave at conversation creation; stored wrapped by the KEK.
- All content encryption is AES-256-GCM (the `aes.rs` construction) **extended with AAD** — this is a hard requirement, not an optimization:
  - message ciphertext AAD = `(conversation_id, message_id, seq, role)`
  - `wrapped_dek` AAD = `(user_ref_hash, conversation_id)`
  - `enc_title` AAD = `conversation_id`
  - Without AAD, a DB-level attacker can swap ciphertexts between rows/conversations, reorder via `seq`, or flip `role` to poison the user's own future model context. With AAD, any moved/re-labeled ciphertext fails decryption.
- Why envelope: anonymous→account migration rewraps N DEKs instead of re-encrypting all messages; future conversation sharing = wrap the DEK for a second principal; rotation is cheap.

**Schema (content columns are ciphertext; plaintext limited to routing/ordering metadata)**

```sql
chat_users     (user_ref_hash PK, kind {anon,account}, created_at)
conversations  (id PK, user_ref_hash FK, wrapped_dek, enc_title,
                model_id, version, created_at, updated_at)
messages       (id PK, conversation_id FK, seq, role, ciphertext,
                enc_usage_meta, created_at)
sessions_revoked (token_hash PK, revoked_at)        -- revocation list only
magic_links    (token_hash PK, expires_at, used_at)
                                                    -- replay guard only; identity comes from the signed token
```

- `user_ref_hash = keccak(user_ref)`; the raw ref never touches the DB (see §6 for what this does and does not protect per user class).
- **Account lookup needs no email column, because the derivation *is* the lookup**: on magic-link verification the enclave derives `user_ref = HKDF(user-id-namespace, lower(email))` (§5.3) and hashes it, and that value is the `chat_users` primary key — a returning user resolves to their existing row without the DB ever holding an email, a hash of one, or a UUID mapped to one. This is a deliberate departure from lit-triggers' `users(id, email)` shape, where a DB dump is directly identifying. Consequence to accept: there is no way to list accounts by address or reverse a row to a person, including for support.
- Token-usage metadata is **encrypted** (`enc_usage_meta`), not plaintext — usage patterns are content-adjacent. Aggregate, non-attributable counters for ops/billing are kept separately.
- Optimistic concurrency on `conversations.version` (CPL-364's `expected_version → 409` shape) for multi-tab/multi-device safety.

**Integrity honesty**: GCM+AAD gives per-row integrity and binding. It does not give the store tamper-*evidence* over time: the DB operator can delete rows, truncate a conversation's tail, or roll the database back wholesale, and a stateless TEE cannot detect it. `seq` contiguity makes mid-conversation deletion detectable; tail truncation and rollback are disclosed in the threat model (§6). A per-conversation hash chain is a P3 option if this matters in practice.

**Deletion honesty**: "Delete conversation" hard-deletes the conversation row and its messages immediately — no `deleted_at` tombstone, because a soft-deleted row still holds the ciphertext and its wrapped DEK, which is a weaker guarantee than the one the UI states. Encrypted copies persist in database backups until backup expiry (window to be stated in the privacy policy, target ≤ 30 days), and remain decryptable only inside the enclave by the key holder's session. **We do not use the phrase "crypto-shred"**: the KEK is statelessly re-derivable by design (that's what makes the system recoverable), so no key destruction event exists.

## 5. Identity, sessions, and keys

### 5.1 Sessions are TEE-signed, never DB-rooted
Session tokens are minted and MAC'd (or signed) inside the enclave with a dstack-derived service secret (`get_key("chat/v1/session-mac", "chat-svc")`). The database holds at most a **revocation list** — never authoritative session rows. This is load-bearing: with lit-triggers-style DB-backed sessions, an operator with DB write access could forge a session for any known user and have the TEE decrypt that user's history, collapsing the headline claim to a policy promise. With TEE-signed tokens, the operator cannot mint a valid session without the enclave.

### 5.2 Anonymous users
- First visit: enclave mints a random 128-bit `user_ref`, sets it in a `__Host-`-prefixed, httpOnly, Secure cookie (long expiry), and issues a TEE-signed session token bound to it.
- The cookie is a **bearer capability and is not revocable** — it *is* the key-derivation input; there is no server-side account to reset. Theft of the cookie = access to that anonymous history, past and future. This is disclosed in the privacy panel, and "Save my history" (account upgrade) is presented as the remedy — after upgrade the anon ref is retired.
- Lost cookie / new browser = fresh identity; old history is unreachable (by anyone). Disclosed in UI copy.

### 5.3 Account users
- Email magic link (lit-triggers auth as the code template; `token_hash` discipline): short TTL, single-use, and **same-session code entry** (the user types a short code into the originating tab rather than the link logging in whatever device clicks it) — this bounds the email provider's power to session-hijack.
- **The magic-link token carries the identity, signed in-enclave** (`get_key("chat/v1/magic-link-mac", "chat-svc")`), exactly as lit-triggers does (`token::verify` checks signed claims before the row is consumed). The `magic_links` row is a replay guard — expiry and single-use — and is never the token→identity binding. This is the same rule as §5.1 and it is load-bearing for the same reason: if the DB row decided *whose* account a token unlocks, an operator with write access could insert a row pointing a token they chose at any victim, redeem it, and receive a valid TEE-signed session for that user — the enclave would then decrypt that history on request. The row holds only `token_hash`, expiry, and use state — nothing identifying.
- On verification, the enclave derives `user_ref = HKDF(user-id-namespace, lower(email))` directly and issues TEE-signed sessions per device. No user-UUID indirection: an operator-visible email→UUID table would be one more row an operator could rewrite to point an account at a different key, and it buys nothing the derivation doesn't already give us. The cost is that **the email address is permanent** — changing it derives a different KEK and orphans the history, so an address change is a migration (§5.4's rewrap, run against the new derivation), not a profile edit.
- **Honest limitation**: for account users the KEK input derives from an identity the operator's systems know (they send the mail). The control protecting account history is therefore *authenticated-session gating enforced by attested enclave code* — not secrecy of the derivation input. An operator cannot decrypt without either forging a TEE-signed session (can't, without the enclave secret) or shipping malicious enclave code (visible: compose_hash change). This is stated plainly in the threat model instead of implied away.
- Account deletion: delete rows + revoke sessions; same backup-window disclosure as conversation deletion.

### 5.4 Anonymous → account migration
On upgrade, within one enclave operation: authenticate both refs (live anon session + fresh magic-link verification), derive both KEKs, rewrap all conversation DEKs from anon-KEK to account-KEK, repoint rows to the new `user_ref_hash`, retire the anon ref. Cost is O(conversations), not O(messages).

## 6. Threat model

| Adversary | P1 (OpenRouter) | P3 (in-enclave model) |
|---|---|---|
| **Lit operator (honest-but-curious)** | Cannot read history: ciphertext off-TEE, KEKs enclave-only, sessions TEE-signed. Cannot silently change behavior: compose_hash is published and attested. | Same, plus prompts never leave the enclave. |
| **Lit operator (fully malicious, controls DNS/infra)** | Can repoint DNS to a non-TEE impostor with a valid Let's Encrypt cert and phish plaintext going forward (web-PKI ceiling — no RA-TLS channel binding). Cannot decrypt existing stored history. Mitigations: CAA, CT monitoring, published compose_hash, verify page. **Not eliminated — this is the honesty ceiling of any web-delivered TEE app and we say so.** | Same delivery-channel caveat. |
| **DB/storage operator (Railway)** | Reads ciphertext + metadata (timestamps, seq, sizes, model id). Cannot swap/reorder/re-label rows usefully (AAD). **Can** delete, truncate tails, or roll back wholesale — integrity/availability attack, partially detectable, disclosed. | Same. |
| **OpenRouter + upstream providers** | **See plaintext prompts/completions in flight.** Bounded by ZDR-only default routing + per-model disclosure. This is the phase-1 residual and the reason P3 exists. | N/A for in-enclave models. |
| **Email provider** (account users) | Sees magic-link mail; same-session code entry + TTL + single-use bounds takeover. Residual trust disclosed. | Same. |
| **Telemetry pipeline (GCP)** | Content-free by launch-gated policy (§7.3); IP/access metadata retained per stated policy. | Same. |
| **Payment processor (Stripe)** (paying accounts) | Sees account identity, charge amounts, and flush timestamps — never content, never model or conversation identifiers (§8.4). Unavoidable for paid usage; avoidable entirely by staying on the free tier. | Same. |
| **Cookie thief / shared device** | Full access to that user's chat (anon: non-revocable; account: revoke via re-login/revocation list). | Same. |
| **XSS / malicious model output** | Primary practical risk; CSP + sanitization + no-CDN + human-approved tool calls (§4.1, §4.2). | Same. |
| **Network observer** | TLS end-to-end into the CVM; sees traffic shape only. | Same. |

Explicitly out of scope: compromised end-user device/browser; TDX silicon-level attacks; availability guarantees against a malicious storage operator.

## 7. Trust, honesty, and operations

### 7.1 Attestation UX
- Chat CVM exposes `GET /attestation` (TDX quote, `report_data` nonce-binding supported) and `GET /info` (compose_hash) — same machinery as the core server; CI runs the dstack verifier against the live deployment after every deploy (existing `verify-attestation.yml` precedent).
- Verify page (extend `lit-static/dapps/verify`): walks quote → compose_hash → digest-pinned images → source tag. Framed as **"the deployment is attested and auditable"**, never "your browser has verified the enclave" (it hasn't; see the DNS row above). Per-session nonce-bound quotes in the UI are P2 polish.

### 7.2 Claim discipline (marketing/UI wording rules)
- ✅ "Your conversations are private — by architecture, not by policy." (lead claim)
- ✅ "Your chat history is post-quantum encrypted, and Lit cannot access it." — valid because the at-rest envelope is symmetric-only (AES-256-GCM, enclave-derived keys); harvest-now-decrypt-later against the stored ciphertext does not work even with a future quantum computer.
- ✅ "Your history is ciphertext everywhere outside the enclave; Lit cannot read it."
- ✅ "The serving code is attested; changing it is a visible, governed event."
- ✅ (P3, in-enclave models only) "Your prompts never leave the enclave."
- ❌ "Nobody can ever see your prompts" (false in P1 — external models see plaintext).
- ❌ "Crypto-shredded" / "gone forever" deletion claims (backups + re-derivable KEK).
- ❌ "End-to-end encrypted" (it is not E2EE; the enclave sees plaintext by design).
- ❌ "We have no idea when you use Lit Chat" — for paying accounts, the payment processor sees identity, amount, and timing at flush granularity (§8.4). True only on the free tier.
- ❌ "Post-quantum secure" as a blanket product claim — the PQ property covers stored history only. TLS to the CVM (ECDHE) and TDX attestation signatures (ECDSA) are classical; a quantum adversary recording traffic today could eventually decrypt *in-flight* sessions, not the stored archive. Revisit when PQ TLS hybrids (X25519MLKEM) are deployable at the ingress.
- ⚠️ **The at-rest claim has one classical link too, and it is the root of the key chain.** The envelope is symmetric all the way down, but the app key those KEKs derive from is not born in the CVM — the on-chain KMS releases it to the enclave over an attested TLS/ECDH channel (`docs/architecture/verification/onchain-kms.mdx`). An adversary who recorded that provisioning exchange *and* holds the DB ciphertext could, with a future quantum computer, recover the app key and re-derive every KEK. That is a narrow attack (it requires having captured a specific short handshake, not bulk traffic), but it is the honest boundary of "post-quantum encrypted history," and it belongs in this list alongside TLS and ECDSA rather than being discovered later. Closing it is upstream of us: it needs PQ-hybrid key transport in dstack's KMS. State the claim as scoped to the stored ciphertext, and do not extend it to "no future computer can ever reach your history."

### 7.3 Logging & telemetry (launch gate)
Content-free logging is a release blocker, not a guideline: no message bodies, titles, prompts, or raw `user_ref`s in any log line or span (UUID-only, per the CPL-378 precedent); the core stack's `RUST_LOG=trace` habit does not carry over — lit-chat ships at `info` with an explicit deny-list lint/review for body-bearing logs; otel export reviewed for attribute leakage. IP addresses: used for rate limiting in memory, retained in logs per a stated policy (target ≤ 7 days), disclosed. The telemetry posture is documented on the verify page.

### 7.4 Abuse, moderation, and legal
The privacy design removes proactive content moderation by construction — the operator cannot scan what it cannot read. The PRD position, to be validated by counsel before launch:
- P1: upstream providers' moderation/safety applies to external models; ToS prohibits illegal use; a **user-initiated report flow** submits a decrypted excerpt with explicit consent.
- Anonymous + free = extraction-abuse magnet: per-IP buckets, per-session budgets, bot friction (PoW or turnstile on anonymous stream start), and the global spend breaker (§4.2) are launch requirements, not nice-to-haves.
- P3 in-enclave models have no upstream moderator; ship with a local safety-classifier gate or accept and document the position — decision owed before P3, with legal review of CSAM-reporting obligations in operating jurisdictions.
- GDPR reality: emails, IPs, and timing metadata are personal data regardless of content encryption; export (§4.1) and deletion flows are the compliance surface. "We can't read your chats" ≠ "no PII."

### 7.5 Key continuity & disaster recovery (existential, must be tested)
Persistent encrypted history makes key stability the product's existential dependency: the KEK must derive identically across compose upgrades, CVM resizes, and instance migrations (dstack app-root/KMS governance). Launch gates: (a) a documented statement of what the derivation is anchored to and who governs it; (b) a rehearsed compose-upgrade test proving pre-upgrade history decrypts post-upgrade; (c) the honest DR line: *if the app root key is ever lost, all history is unrecoverable — by design*.

## 8. Pricing and billing

**Decision: cost-plus passthrough, metered against the Stripe credit ledger Chipotle already runs.** Chat usage costs *OpenRouter's reported cost for the generation × (1 + markup)*, drawn from prepaid credits — no subscription, no per-seat pricing, no separate token currency. The markup is **5% as a working placeholder; the number is not decided**, and §8.5 argues honestly that it is probably too low to be the whole story.

Why reuse rather than invent: `lit-billing-core` already implements the ledger (Stripe customer balance as credits — funding writes a negative balance transaction, charging writes a positive one), and the same primitives back both `lit-api-server` and `lit-payments`. Chat becomes a third consumer of that ledger, not a second billing system. Every funding path comes along for free: card, Stripe crypto rails, LITKEY on-chain (§8.5 flags a collision), auto top-up (`/billing/auto_topup_config`), $5.00 minimum top-up (`MIN_TOPUP_CENTS`).

### 8.1 What's metered

| Item | Price | Notes |
|---|---|---|
| Model inference (external, P1) | OpenRouter's reported generation cost **+ 5% (TBD)** | Metered per completed generation in micro-USD (§8.2) |
| In-enclave model (P3) | Free at launch | No upstream cost; the cost is CVM capacity. Priced only if it becomes load-bearing. |
| Lit Action tool calls (P2) | $0.01/sec, passed through at the platform rate | The chat service calls `/core/v1/lit_action` with its own usage key, so these land on *chat's* Chipotle account by default — they must be re-billed to the end user's chat ledger, not absorbed. |
| Storage, history, sync, export | Free | Durable encrypted history is the differentiator (§12); paywalling it would sell the wedge. Ciphertext is cheap. |
| Everything read-only | Free | Matches the existing pricing page's "no charge for read-only operations". |

### 8.2 Sub-cent metering is the hard part

The existing ledger is integer cents (`COST_*_CENTS: i64`; Stripe balance transactions are denominated in cents) and its smallest charge is $0.01. A single chat message on a mid-tier model routinely costs a fraction of a cent, so charging per message at cent granularity would over-bill by 10–100×. This is the one piece of billing that cannot be reused as-is:

- Accumulate in **micro-USD (`i64` µUSD)** per user. Keep the exact remainder; flush *floor(µUSD / 10 000)* whole cents to Stripe and carry the rest forward. Never round a single message up — that rounding *is* the over-billing bug.
- The accumulator is per-user spend, i.e. content-adjacent metadata, so it is stored encrypted under the user's KEK with AAD binding, like `enc_usage_meta` (§4.3); ops/finance read the separate aggregate, non-attributable counters that §4.3 already provides for.
- Flush in-request at end of stream (the enclave holds the user's KEK during their session), so no offline flusher that would need to derive keys for absent users.
- One Stripe balance transaction per flush, with a deliberately boring description — see §8.4.

### 8.3 Metering mechanics

- **Cost truth comes from OpenRouter, not from our own token counting.** Use the cost OpenRouter reports for the generation (usage accounting on the completion / generation lookup). Provider routes and prices change without notice; a local price table would drift and silently under- or over-bill.
- **Reserve before, settle after.** Pre-flight, refuse the stream if credits cannot cover a worst-case estimate (`max_tokens` at the model's list price) — the same "check balance + cost ≤ 0 before the call" shape the API server uses. Post-stream, settle the actual reported cost into the accumulator.
- **Never kill a stream for a billing failure.** §4.2 cites mid-flight billing-flush cancellation as one reason chat is not a Lit Action; chat must not reproduce it. On settlement failure: finish the generation, retry in the background (the existing retry path plus the `billing.charge.settlement_failed` metric), and absorb the loss — bounded by the pre-flight reserve.
- **Anonymous users are budget-metered, not billed.** An anonymous visitor has no identity to charge, so they get a daily token budget, per-IP buckets, and the global spend breaker (§4.2). Exhausting it is an upgrade prompt, not a payment wall — which conveniently makes the "save my history" moment (§9) also the moment the meter starts.
- New accounts get a starter grant via the `STARTER_CREDITS_CENTS` shape, so the first paid-tier conversation needs no card.

### 8.4 Billing is the largest deliberate metadata leak in the design

Payment creates identified, off-TEE records by definition: the processor sees who paid, how much, and when. That does not touch content, but it is squarely at odds with the rest of this document's posture, so it is designed rather than discovered:

- The chat Stripe customer is keyed by `user_ref_hash` — the same opaque account id the chat DB uses (§4.3) — **not** by a wallet address, since chat users have no Chipotle developer account (§2) and the existing customer model keys on the wallet derived from an API key. Stripe still learns the payer's real identity from the card and receipt email; the point of the opaque key is that the Stripe record and the chat database cannot be joined by anyone holding only one of them.
- Charge descriptions and metadata carry no conversation id, model id, title, or token counts — "Lit Chat usage" plus a request id, nothing content-adjacent. The core server puts the action CID in the charge description; chat has no equivalent that is safe to write down.
- **Flush frequency is a privacy knob.** One transaction per accumulated cent is a usage timeline; a threshold-plus-daily-cap flush is not. Prefer the coarser one.
- Carried into the threat model as its own adversary row (§6) and into the claim rules as a ❌ (§7.2): "we have no idea when you use Lit Chat" is true on the free tier and false for paying accounts.

### 8.5 Why 5% is probably not the whole answer

Arithmetic worth doing before a number goes on a page:

- **Payment processing dwarfs the markup.** Stripe's standard card fee (~2.9% + $0.30) on a $5.00 minimum top-up is ~8.9% of that top-up. A 5% markup on the usage those credits fund does not cover the cost of collecting them. Raising the chat minimum top-up, or a subscription for heavy users, fixes this more cleanly than inflating per-token pricing.
- **LITKEY collides head-on.** The 25% LITKEY discount is applied at *funding* — paying $0.75 of LITKEY buys $1.00 of credit (`lit-payments/src/rate.rs`) — so it discounts whatever those credits are later spent on. Cost + 5% funded with LITKEY sells $1.00 of OpenRouter cost for about $0.79: structurally below cost. Decision owed before any LITKEY messaging mentions chat: exclude chat metering from the discount, set chat's markup above it, or cap LITKEY-funded credits for chat.
- **Markup revenue does not pay for the CVM.** At 5%, a $0.02 conversation nets $0.001; covering a dedicated chat CVM (open question 1) would take conversation volume in the hundreds of thousands per month. The honest P1 position is that the CVM is a marketing expense and the markup exists mainly to keep model spend bounded and abuse unattractive.
- Consequence: treat the markup as a cost-control first and a revenue line second, and — if margin ever matters — price the thing that is actually differentiated (durable, multi-device encrypted history), which is also the shape Venice monetizes (§12).

### 8.6 Pricing claim discipline (extends §7.2)

- ✅ "You pay what the model costs, plus X%" — valid only while the base really is OpenRouter's reported cost rather than a marked-up internal table.
- ✅ "No subscription, and credits don't expire" — true of the existing ledger.
- ❌ "At cost" / "we don't profit on inference" while charging any markup.
- ❌ "Free forever" — the free tier is budget-capped and the spend breaker can restrict it to account holders (§4.2).
- The model picker shows $/1M tokens beside the privacy badge, and the credit balance is visible before a message is sent: the no-surprise-charges posture the dashboard already sets.

## 9. Success metrics
- Activation: % of visitors who send ≥1 message; TTFM (time to first message) < 10s from landing.
- Depth: median messages/conversation; D7 return rate for account holders.
- Conversion: anonymous → account upgrade rate (the "save my history" moment is the product's key transaction).
- Trust surface engagement: verify-page visits per WAU (unusual metric, core to this product's thesis).
- Cost: fully-loaded cost per conversation (OpenRouter + CVM amortization) vs budget; zero spend-cap breaches.
- Billing accuracy (P2): metered µUSD vs OpenRouter's invoiced total within 1% per month; net margin per paying account after Stripe fees and LITKEY-funded credits (§8.5) — the number that says whether the markup is real.
- Quality: TTFT p95 ≤ 1.5s (proxy overhead), stream error rate < 0.5%.
- Platform: ≥1 external team adopting the encrypted-storage pattern or the open-sourced template.

## 10. Phasing

| Phase | Scope | Exit criteria |
|---|---|---|
| **P1 — MVP** (own CVM from day one) | `lit-chat` service + SSE OpenRouter proxy (ZDR catalog) + anonymous sessions (TEE-signed) + envelope-encrypted Postgres (AAD) + web UI + attestation endpoints + logging gate + rate limits/spend breaker. Free tier only — no payment path, but the µUSD cost accumulator (§8.2) runs in shadow mode from day one so the markup is set from measured costs, not guesses. Staging → prod on chat's own governance. | Anonymous user chats privately end-to-end; verify page live; security review (incl. log-hygiene + XSS) passed; key-continuity test (§7.5) passed; ≥2 weeks of shadow cost data. |
| **P2 — Accounts & tools** | Magic-link accounts, multi-device, anon→account rewrap, data export/deletion, BYOK, Lit Action tool calls with human-in-the-loop approvals, per-session attestation nonce UX. Billing goes live with accounts (§8) — an anonymous user has no ledger to charge. | Upgrade flow ships; first wallet-tool demo; counsel sign-off on §7.4 posture; markup + LITKEY decision (§8.5) made and metering reconciled against an OpenRouter invoice. |
| **P3 — In-enclave inference & hardening** | llama.cpp container with baked-in small model (attested weights), confidential-GPU research spike, optional conversation hash-chaining, moderation decision for local models. | ≥1 in-enclave model in the picker with the "prompts never leave the enclave" badge. |

Cut from earlier drafts (deliberately): a gVisor binary-action PoC phase — outbound-LLM-call feasibility is already proven in-repo by the `claude` gVisor example, and UX validation doesn't need a TEE; the week goes to the `lit-chat` skeleton instead.

## 11. Open questions
1. CVM sizing/cost for the chat app (and P3 model residency) — needs a load model; staging-class `tdx.small` is known-insufficient under modest concurrency.
2. Chat CVM governance: plain signed-digest review, or lightweight multisig? (Must stay cheaper than the core ceremony or the separate-CVM rationale erodes.)
3. Domain and product name.
4. Postgres tenancy: fresh Railway instance vs shared cluster with lit-payments (separate DB either way; blast-radius and ops-ownership question).
5. Free-tier economics: which default model keeps cost/conversation acceptable at the target rate limits, and how large is the anonymous daily budget before the upgrade prompt?
6. **The markup number** (§8): 5% is a placeholder. Does chat instead carry a higher markup, a larger minimum top-up, or a subscription for heavy users — and does the 25% LITKEY funding discount apply to chat metering at all? Below-cost is the default outcome if this is left unanswered.
7. Whether the frontend stays no-build vanilla JS at this feature size (streaming markdown + sanitization + SSE state) or becomes the repo's first build-step frontend — team taste decision.
8. Open-source timing: template from day one, or after security review?

## 12. Competitive comparison: Venice.ai

Venice.ai is the closest shipping competitor for the "private AI chat" positioning and the most useful foil for explaining what Lit Chat is — the two products make **dual architectural bets**. Venice's answer to "how do we not read your chats?" is *remove the server from the story*. Ours is *make the server provable*.

### How Venice works (as publicly documented)

- **History is client-side only.** Conversations live in encrypted local browser storage on the originating device; Venice states it stores and logs no prompts or responses server-side. Consequences they accept: no multi-device sync (history differs across devices even on one account), and clearing browser storage destroys history permanently.
- **Inference flow**: browser → Venice proxy (SSL, stated no-logging) → a pool of GPUs across decentralized providers → response streams back. Venice runs **open-source models it hosts itself** (Llama/Mistral-family, including its flagship uncensored Dolphin Mistral 24B fine-tune) — no closed-model vendors, which is also how it escapes upstream content policies.
- **Trust model: policy, not proof.** No TEEs, no attestation, no third-party audit published. The zero-retention proxy claim is a commitment, architecturally reinforced by having nothing to store — but the proxy *does* see plaintext in flight, and the GPU provider running a request sees that request in the clear (unlinked from identity/history, per Venice's design).
- **Accounts optional; anonymous default** (IP/browser metadata still collected). Monetization: Pro subscription, plus an API where access can be earned by staking the VVV token.

### The OpenRouter axis (upstream vs downstream)

The two products sit on **opposite sides of OpenRouter**:

- **Venice is downstream** — a *provider on* OpenRouter: it hosts its own models on its own GPU pool and sells that inference through OpenRouter's marketplace (its uncensored model is listed there). Its consumer app never touches OpenRouter; the app path is vertically integrated (Venice app → Venice proxy → Venice-contracted GPUs).
- **Lit Chat (P1) is upstream** — a *client of* OpenRouter: we route to the whole provider marketplace (conceivably including Venice's models) and inherit model breadth, including frontier closed models Venice structurally cannot offer.
- **Lit Chat (P3) becomes what Venice is** — a self-hosting inference operator — but inside an attested enclave, converting "we don't log" from a policy into a property.

### Dimension-by-dimension

| Dimension | Venice.ai | Lit Chat P1 | Lit Chat P3 |
|---|---|---|---|
| History storage | Browser local storage only; no server copy | Off-TEE Postgres, ciphertext-only, enclave-held keys | Same |
| Multi-device / durability | None; clear cache = gone forever | Yes — the core wedge: persistence *with* operator blindness | Same |
| Trust anchor | Policy promise + "nothing to store" architecture | TDX attestation → compose_hash → pinned images → source | Same |
| Who sees plaintext prompts at inference | Venice proxy (transient) + the GPU provider serving the request (unlinked from identity) | The enclave (attested) + OpenRouter + ZDR-routed provider | The enclave only |
| Model catalog | Self-hosted open models only (incl. deliberately uncensored) | OpenRouter breadth incl. frontier closed models, ZDR-filtered | + small in-enclave models; confidential-GPU research |
| Verifiability by a user | None published (no audit, no attestation) | Verify page + CI-verified quotes; web-PKI delivery ceiling applies (§6) | Same |
| Server compromise blast radius | Near-nil for stored data (nothing stored); live proxy compromise leaks in-flight plaintext | DB breach = ciphertext + metadata; enclave is the target that matters, and it's attested | Same |
| Moderation stance | Deliberately uncensored (their differentiator) | Upstream provider moderation + report flow (§7.4) | Local-model moderation decision owed (§7.4) |
| Accounts / anonymity | Optional accounts; anonymous default | Same (anon default, magic-link upgrade) | Same |
| Monetization | Pro subscription, VVV-staked API | Free + rate limits in P1; cost-plus credits on the existing Chipotle Stripe ledger in P2 (§8) | — |

### What we take from Venice, honestly

1. **Their storage story is simpler and, for stored data, strictly smaller-surface.** A Venice server breach leaks no history because there is none; our DB holds ciphertext plus real metadata (timing, sizes, counts — §6). Our claim must therefore stay precise: we are *more durable and equally blind*, not "more private on every axis."
2. **Anonymous-by-default works** — Venice validated zero-friction private chat as a product; our G1 mirrors it.
3. **Their weakness is our wedge.** No sync, no durability, and cache-clear data loss are direct costs of their architecture, not oversights — they cannot fix them without becoming a server-side custodian, which collapses their "nothing to store" story into an unverifiable "we store but don't look." We are building exactly the thing that fixes it *with* a proof: TEE-derived keys make server-side persistence compatible with operator blindness. "Privacy first — post-quantum encrypted chat history that not even Lit can access, and that survives your browser cache" is the one-line competitive positioning (a durability-plus-proof claim Venice's policy-based, local-only architecture cannot make).
4. **In-flight exposure is a shared P1 weakness in different clothes**: their GPU providers see plaintext requests; our ZDR-routed OpenRouter providers do too. Neither product's phase-1 marketing may claim otherwise. The differentiated end-state is P3 in-enclave inference — the claim Venice's non-TEE architecture cannot reach.

Sources: [Venice privacy architecture](https://venice.ai/blog/venice-ai-privacy-architecture), [Venice × OpenRouter partnership](https://venice.ai/blog/venice-openrouter-partner-to-expand-reach-of-private-uncensored-ai-to-developers), [OpenRouter: Venice provider page](https://openrouter.ai/venice), [OpenRouter: Dolphin Mistral 24B Venice Edition](https://openrouter.ai/cognitivecomputations/dolphin-mistral-24b-venice-edition).

## 13. Platform grounding (what exists vs what's new)

**Exists and is reused directly**: dstack `get_key` stateless derivation + keccak wrap; `aes.rs` AES-256-GCM construction (extended with AAD); dstack-ingress TLS-in-TEE; `/attestation` + `/info` endpoints and the CI verifier; Phala `encrypted_env` sealed secrets; per-IP token-bucket rate limiting (CPL-367); magic-link auth code shape (lit-triggers); Railway Postgres ops (lit-payments); digest-pinned, Sigstore-signed image pipeline; the whole credit ledger and its funding paths (`lit-billing-core` Stripe customer-balance credits, card/crypto/LITKEY top-ups, auto top-up, starter credits, the background-settle-with-retry charge path).

**New builds**: the `lit-chat` service and CVM (compose, CI lanes, domain); SSE proxy with encrypt-on-completion; AAD extension to the AES helper; TEE-signed session tokens; the sub-cent µUSD metering accumulator and cost-plus markup (§8.2 — the existing ledger bottoms out at $0.01, so this is the one billing piece that cannot be reused); a chat Stripe customer keyed by the opaque `user_ref_hash` rather than a wallet address; the chat schema and envelope-encryption layer (first implementation of the CPL-364 off-TEE ciphertext pattern — the plan exists, the code does not); the chat frontend; identity→derivation-path mapping (documented in `auth-model.md` as the Stytch shape, previously unimplemented).

**Explicitly not used for inference**: `/lit_action` and `/lit_binary_action` (no streaming, 1 MB response cap, no persistence ops, per-second billing, prod-gated gVisor). Lit Actions enter as the P2 tool-calling layer, which is where they're strong: governed, on-chain-permissioned, attested side effects.
