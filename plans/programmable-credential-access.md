# Programmable Credential Access on Chipotle (`lit-secrets`)

Clone of Turnkey's "programmable credential access" (their Secrets API, closed
beta: https://docs.turnkey.com/solutions/key-management/programmable-credential-access),
built as an **app on top of Chipotle** — the same shape as lit-payments and
lit-triggers — not as a change to Chipotle itself.

Pitch: **a password manager for machines.** Secrets live sealed inside the TEE;
humans, services, and AI agents get policy-gated, programmable access with one
credential.

## What Turnkey ships (parity target)

- Secrets store with static classification (`kind`, `environment`) bound at import.
- Policy engine, editable for the life of a secret, evaluated in the enclave.
- Three access patterns: unilateral agent access, agent-proposes/human-approves,
  M-of-N multi-agent consensus.
- Agent identity as durable users + scoped expiring session keys; per-instance
  or per-role revocation.
- Export re-encrypted to the requester's ephemeral key.

## Our posture: two release tiers

| Tier | What it means | Turnkey equivalent |
|---|---|---|
| **`plaintext`** (default) | Authorized agents read the value. Decrypted in the TEE by a pinned, auditable reader action and returned *straight to the agent*. The control plane never sees it. | Their whole product |
| **`in_tee_only`** | Only Lit Actions the user explicitly attached to their vault group can decrypt, only inside the TEE. Nobody can read it out, including us. | None — our differentiator |

Decision log: sealed-export (HPKE to an ephemeral key) was considered and
dropped; plaintext-over-TLS is the same trust model as every normal secrets
manager, and consensus flows still work because approvers sign a request digest
and only the requester receives the value.

## Architecture: control plane vs. data plane

```
                 ┌─────────────── lit-secrets (Railway, Postgres) ────────────────────┐
  user ──login──▶│ secrets CRUD · agents (usage keys) · policy · grants · audit       │
                 │ stores: ciphertext, policy, key hashes — never plaintext           │
                 └────────────┬───────────────────────────────────┬──────────────────┘
        management API (master key)│                              │ POST /api/grants → signed grant
                                   ▼                              ▼
                 ┌──────────── Chipotle (TEE) ──────────┐   agent ── POST /lit_action (reader, own usage key)
                 │ per-tenant vault PKP + group          │   ◀── plaintext ── Decrypt in TEE
                 │ encrypt action · reader action · CIDs │
                 └───────────────────────────────────────┘
```

The rule that makes the app model work: **the app is control plane only.** If
plaintext ever transited the app it would be a Railway box handling secrets and
the TEE story would collapse. It doesn't: the reader action returns the value to
whoever invoked it, and only agents invoke it.

### Concept map

| Turnkey | lit-secrets |
|---|---|
| Organization | Tenant (one per user; vault PKP + Chipotle group on the operator account) |
| Secret + classification | `secrets` row: name, `kind`, `environment`, `release`, versioned ciphertext |
| Agent user + session key | Agent = scoped Chipotle usage key (`execute_in_groups: [tenant group]`). One credential for both APIs. Revoke ⇒ `remove_usage_api_key`; effective within ≤300s across Chipotle replicas (issue #631). |
| Policy | Per-secret JSON: `allowed_agents`, `max_reads_per_day`, `not_after` (+ approvals, Phase 2) |
| Export | `POST /api/grants` → EIP-191-signed grant `{tenant,name,version,pkpId,ciphertextHash,agent,iat,exp}` → agent redeems on Chipotle |
| (none) | `in_tee_only` + `POST /api/actions` to attach customer CIDs |

### Why this beats integrating into Chipotle

- Zero core risk: no diamond upgrade, no new ops, no in-TEE persistence (the
  API server keeps no state across CVM swaps — so "where does the registry
  live" is just Postgres).
- Shipped in days: it's the lit-triggers codebase shape with different tables.
- Best possible platform proof: Turnkey parity built entirely on public APIs.
- Billing flows through Chipotle credits on the operator account, like lit-payments.

### Chipotle primitives reused (unchanged)

TEE-derived AES-GCM sealing (`Lit.Actions.Encrypt/Decrypt`), `canUseWalletInAction`
group authz, the Deno sandbox, scoped usage keys, `add_action_to_group` /
`remove_usage_api_key` management API, bundled ethers (grant verification),
`ipfs-hasher` CID identity for inline `code`.

## Status

### Phase 1 — shipped in `lit-secrets/` (this branch)

- Rocket + Postgres service, magic-link auth + agent access tokens (from lit-triggers).
- Tenant provisioning: `create_wallet` → `add_group` → `add_action`/`add_action_to_group`
  for the pinned encrypt + reader actions → per-tenant service usage key.
- Secrets CRUD with versions; values sealed by running `actions/encrypt.js` via
  the service key; `kind`/`environment`/`release`/`policy`.
- Agents: mint/list/revoke scoped usage keys; `AgentKey` request guard (bearer =
  usage key, sha256 lookup).
- Grants: policy evaluation → EIP-191 signature (k256) over canonical JSON →
  response includes ciphertext, reader code+CID, ready `js_params`.
- `actions/reader.js`: verifies signer, expiry, vault, `keccak256(ciphertext)`,
  decrypts, returns value. Signer address baked in ⇒ CID pinned per deployment.
- Reference endpoint for `in_tee_only`; tenant action attach/detach.
- Access log for every allow/deny; dashboard (`static/`), agent SDK
  (`sdk/lit-secrets.js`), `SKILL.md`, Dockerfile + railway.json.
- Unit tests: signer recovery vector, policy matrix, grant canonical form,
  CID/hash helpers, group-id parsing.

### Phase 1 follow-ups before prod

- ~~Live smoke test~~ **done 2026-08-27 against prod Chipotle** (see
  `lit-secrets/README.md`): provisioning, seal, grant, redeem, rotate, policy
  denials, in-TEE-only via customer action, forged/tampered/expired grants
  rejected in-TEE, revoke. `pkp_ids_permitted` accepts wallet addresses.
  Found + filed: usage-key revocation lags ≤300s across Chipotle replicas.
- Operator Chipotle account is funded (~$9k credit); decide re-billing
  (per-read fee vs. flat).
- Reader-CID rotation job: when `GRANT_SIGNING_KEY` changes, attach the new
  reader CID to every tenant group (currently `/api/tenant.reader_cid_stale`
  + `503 reader_not_attached`).
- Deploy: `secrets.litprotocol.com` on Railway, root dir `lit-secrets`.

### Phase 2 — approvals & agent identity (Turnkey's 3 patterns)

1. **Unilateral** — done (Phase 1). Add `expires_at` on agents + a revoke cron
   for ephemeral instances.
2. **Human-in-the-loop** — `approval: "human"` policy: `POST /api/grants`
   returns `202 {request_id}`; owner gets email (Resend, already wired) →
   approves in dashboard (session) or via EIP-712 signature from a passkey
   smart wallet (Chipotle's EIP-1271 path already exists) → grant issued,
   agent polls `GET /api/grants/<request_id>`.
3. **M-of-N consensus** — `approval: "consensus", approvers: [...], threshold`:
   approver agents `POST /api/grants/<id>/approve` with their own keys; grant
   issued when threshold met. Only the requesting agent can redeem (its key is
   in the group; approvers needn't be). Later hardening: pass approver
   signatures into the reader so verification happens in-TEE, not just in the app.

### Phase 3 — Chipotle-side hardening (small, generic)

- **Issue #630** `Lit.Actions.requesterIpAddress()` — lets the reader bind
  grants to the requesting IP / CIDR (grant gets an `ip` field; reader
  compares). Also proposed there: `requesterApiKeyHash()` to bind grants to the
  exact agent key — closes grant-theft entirely.
- Optional later: sealed import (client encrypts to an attestation-bound enclave
  pubkey so the control plane never sees the value even at import).

### Phase 4 — polish

- Use-without-reveal helpers for `in_tee_only`: a JS helper module agents can
  import into their own actions (`getSecret(name)` = fetch reference → Decrypt).
- OAuth refresh-token lifecycle (store refresh token; cron re-mints access
  tokens in-TEE and rotates the secret version).
- Dashboard: policy editor UI beyond prompts, per-secret usage charts.

## Open questions

1. Billing model for reads (each is a Chipotle execution on the operator account).
2. Should tenants be able to BYO Chipotle account (sovereign mode) instead of
   riding the operator account? Probably later; lit-triggers precedent is BYO
   usage key.
3. One vault PKP per tenant means any attached `in_tee_only` action can decrypt
   all of that tenant's ciphertexts. Per-secret or per-tier PKPs would tighten
   this at the cost of more on-chain writes.
