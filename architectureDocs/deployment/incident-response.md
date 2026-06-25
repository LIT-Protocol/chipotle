# Incident Response & Key-Compromise Runbook

How to respond when something in the trust chain goes wrong. The planned-upgrade and
rollback flow lives in [vm-code-upgrade.md](vm-code-upgrade.md); this document covers
the *adversarial* and *failure* scenarios that one does not.

> **Status:** several response steps below depend on operational decisions that are
> not yet finalized (marked **TODO/DECIDE**). Treat this as the working runbook and
> resolve the open items before relying on it in a real incident. The technical markers
> were re-checked against the repo on 2026-06-25 and annotated inline with the verified
> current state; the role-assignment markers (incident commander, Phala account owner,
> comms) remain open org decisions with no answer in the repo.

## Roles & contacts

| Role | Who | Responsibility |
|---|---|---|
| Incident commander | **TODO/DECIDE** | Owns the response, declares severity, coordinates comms |
| Safe signers | 4 keyholders (2-of-4 quorum) | Execute on-chain governance actions (whitelist removal, owner change) |
| Phala account owner | **TODO/DECIDE** | Holds `PHALA_CLOUD_API_KEY`; can redeploy/stop CVMs |
| Comms / disclosure | **TODO/DECIDE** | External notifications, status page, `security@` triage |

Confirm the live Safe signer set before any action:

```bash
cast call 0xF688411c0FFc300cAb33EB1dA651DBb3E6891098 \
  "getOwners()(address[])" --rpc-url https://mainnet.base.org
```

## Key facts that shape the response

- **Governance is 2-of-4 with no timelock.** A quorum can act immediately — good for
  emergency response, but also means two compromised signer keys are sufficient to
  change the whitelist. There is no delay to "catch" a malicious change in flight;
  detection must be real-time (see [Monitoring](#detective-controls)).
- **Key derivation is deterministic per compose hash.** The same attested code always
  derives the same application keys from the KMS root. This is why rollback works
  without re-issuance — and why a compromised *code version* that obtained keys must
  be treated as having compromised the data those keys protect.
- **Removing a compose hash blocks key issuance, not a running process.** See below.

## Scenario 1 — Malicious or vulnerable compose hash got whitelisted

Goal: stop the bad version from obtaining keys and serving.

1. **Revoke the whitelist.** Have the Safe execute `removeComposeHash(badHash)` on
   `DstackApp` (`0x3F91…05FfC`). 2-of-4 approval required.
2. **Stop the running CVM.** Revocation prevents *future* key issuance and prevents
   the version from booting again — but a CVM that already holds its keys keeps
   running until restarted. The Phala account owner must explicitly **stop/redeploy**
   the affected CVM via the Phala CLI/dashboard to take it out of service.
   The redeploy/restart command is `phala deploy -c docker-compose.deploy.yml
   --cvm-id <app> --private-key "$PHALA_PRIVATE_KEY"` (see `justfile.deploy` and
   [deployment.md](deployment.md)); it re-applies the compose and restarts the CVM in
   place. **TODO/DECIDE (still open, verified 2026-06-25):** whether the built-in Phala
   gateway can drain in-flight traffic before the restart is not documented anywhere in
   the repo.
3. **Re-deploy a known-good version** whose compose hash is still whitelisted.
4. **Assess data exposure.** Any data the bad version could decrypt with its derived
   keys must be considered exposed.

## Scenario 2 — Safe signer key compromised

1. **If still below quorum (1 key):** rotate immediately but no funds/governance at
   immediate risk. The remaining 3 honest signers still control the 2-of-4.
2. **Replace the signer:** Safe executes `swapOwner` (or `removeOwner` +
   `addOwnerWithThreshold`) to drop the compromised key. 2-of-4 of the *honest*
   signers required.
3. **If two keys compromised (quorum reached by attacker):** treat as full governance
   compromise — the attacker can whitelist arbitrary code. **TODO/DECIDE:** there is
   currently no higher-authority backstop above the Safe. Mitigations to evaluate:
   raising the threshold, adding a timelock to create a reaction window, or a
   guardian/recovery module.

## Scenario 3 — `PHALA_CLOUD_API_KEY` leaked

The Phala API key can redeploy and reconfigure CVMs and read/set CVM secrets. It
**cannot** mint keys for un-whitelisted code (the on-chain gate still holds), but it
can disrupt availability and reconfigure encrypted env injection.

1. **Revoke the key** in the Phala dashboard (Avatar → API Tokens) and issue a new one.
2. **Rotate all CVM-injected secrets** (see Scenario 5) — assume they were readable.
3. **Audit recent deployments** for unexpected CVM changes.

## Scenario 4 — Base / KMS availability failure

- The KMS issues keys at boot. If Base RPC or the KMS is unreachable, **already-running
  CVMs are unaffected** (they hold their keys); only restarts/new boots are blocked.
- Do **not** force a redeploy during a KMS outage — a restarted CVM may fail to
  obtain keys and go down.
- **Verified 2026-06-25:** there is currently **no** RPC redundancy — the KMS/Base path
  resolves a single `BASE_CHAIN_RPC` endpoint (`docker-compose.phala.yml`;
  `lit-api-server/src/utils/chain_info.rs` `rpc_url()`), with no failover configured.
  **TODO/DECIDE:** add fallback RPC endpoints (e.g. via eRPC — see
  [deployment.md](deployment.md)) so a single-provider outage can't block CVM restarts.

## Scenario 5 — Secret compromise (Stripe, AWS, GCP, RPC)

These are injected as encrypted Phala env vars and are **outside the attestation
measurement** (not part of the compose hash). See
[secrets-management.md](secrets-management.md).

1. Rotate the upstream credential (Stripe dashboard, AWS IAM, GCP service account).
2. Update the encrypted Phala secret and redeploy.
3. Scope the blast radius per credential (e.g. Route53 IAM should be DNS-01 only).

## Detective controls

Real-time detection matters more here than usual because there is no governance
timelock to provide a reaction window.

- **Alert on governance writes.** Monitor `DstackApp` and the Safe for
  `addComposeHash` / `removeComposeHash`, `addOwner` / `swapOwner` / `removeOwner`,
  and `changeThreshold` events; page on any unplanned occurrence. **TODO/DECIDE
  (verified not implemented, 2026-06-25):** no governance-event alerting exists in the
  repo today — the otel-collector only forwards app telemetry to GCP, and the existing
  on-chain listeners (`account_events.rs`, `restart.rs`) watch account/restart events,
  not `DstackApp`/Safe governance. Still needs a Grafana alert or a contract-watcher.
- **Alert on attestation drift.** Periodically confirm the live `compose_hash` from
  `/info` is still whitelisted and matches the expected release.
- **Alert on CVM lifecycle changes** from the Phala account.

## Post-incident

- Capture a timeline and the on-chain transaction hashes of every governance action.
- File a coordinated-disclosure note per [SECURITY.md](../../SECURITY.md) if external
  parties are affected.
- Open follow-ups for any **TODO/DECIDE** item this incident exercised.

## References

- [vm-code-upgrade.md](vm-code-upgrade.md) — planned upgrade & rollback
- [derot-key-issuance.md](derot-key-issuance.md) — how keys are issued
- [secrets-management.md](secrets-management.md) — out-of-band secret injection
- [SECURITY.md](../../SECURITY.md) — disclosure policy
