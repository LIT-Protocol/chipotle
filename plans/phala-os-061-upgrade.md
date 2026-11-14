# Phala dstack 0.6.1 upgrade and prod16 replica

## Procedure (next executed 2026-11-12; production pending)

Use Phala CLI 1.2.4. Older 1.1.19 uses rejected numeric node selectors.
Read current CVM configuration and status immediately before changing anything;
next was stopped and its compose had changed since the initial investigation.
Never reuse a previously prepared hash after configuration changes.

1. Check `GET /cvms/<id>/available-os-images` and the node catalog. Require
   non-dev dstack-0.6.1, hash
   `ec43cb8c5e04b6bb6c53b33b16076a01d83d01f7f4b703f2f78a0d947d6247e6`.
   Verify target node has a Base KMS gateway and device ID. Preserve app identity,
   sealed environment and application images; retain the previous configuration.
2. Check `/cvms/<id>/pre-launch-script/upgrade-status`. Old official script
   v0.0.14 references unset variables, incompatible with 0.6 strict shell mode.
3. POST `/cvms/<id>/pre-launch-script/upgrade-to-latest-official` with
   `X-Prepare-Only:true`. Read the returned compose hash.
4. Authorize `addComposeHash(bytes32)` on the **app contract**, verifying its owner
   and Base chain ID 8453 first. Wait for successful receipt.
5. Authorize target node with `addDevice(bytes32)` on the same app contract and
   wait for successful receipt. Never enable allow-any-device as a workaround.
6. Repeat the pre-launch upgrade POST with `X-Compose-Hash` and
   `X-Transaction-Hash`. Wait for operation completion and verify compose hash.
7. PATCH `/cvms/<id>/os-image` with `{"os_image_name":"dstack-0.6.1"}`.
   Verify completion; acceptance alone is not proof the OS changed.
8. Prepare replica with POST `/cvms/<id>/replicas`, `X-Prepare-Only:true`, body
   `{"node":"prod16","os_image":"dstack-0.6.1"}`. Explicit OS selection works
   even if the source still reports the old image. Confirm both on-chain
   prerequisites are allowed. Commit using the returned token and compose hash
   via `phala cvms replicate <source> --commit --token <token> --compose-hash
   <hash> --transaction-hash already-registered`.
9. Verify actual running OS, container health, `/health`, `/core/v1/version`,
   and custom-domain routing. Ingress may claim the custom domain on boot.
   Do not stop a healthy source until the new instance passes validation.

## Production Safe requirements

Production is NOT upgraded by this procedure's next execution. Its app is
Safe-owned. Every next owner-key operation above must instead be a Safe
transaction proposal, reviewed, approved and executed by the production Safe:

- `addComposeHash(bytes32)` for the production-specific prepared pre-launch update.
- `addDevice(bytes32)` for prod16 if not already authorized.
- Any additional compose authorization requested by subsequent deployment steps.

Do not use next's key or hashes for prod. Encode against the production app,
verify Safe ownership, chain and calldata, and wait for executed receipts before
committing Phala changes. OS PATCH and replica commit use the Phala API credential;
they did not themselves require the next owner key. Preserve this distinction.
The user explicitly authorized a public PR for the replica identity API change
and this plan on 2026-11-12. Production deployment remains pending.

## Next execution record

- Source: `chipotle-next-rep-sa6xj` (`cvm_OeN6NBeQ`), prod2.
- App: `0x969A8C14c9e13420705B19C7246AeeD27897e7eA`.
- New compose: `9558ae0cd64aacdde1cfe49baaf225538f459da2aab51c8176d404db0ab7a1c1`.
- addComposeHash tx: `0x41e857ea8a3633daefa71526a7ed0c2dad9e54ebd9e1cf9fd383d0833880b4a2` (success).
- prod16 device: `72791b7add93ac1f84c9d20ed738a3a091f90b92bf67914e48069086378ee9d5`.
- addDevice tx: `0x138efabb18699acb7a762d9604e5c6e21ba046de21a432910315ad67b219a5e4` (success).
- Replica created: `chipotle-next-rep-sa6xj-rep-adnb3` on prod16.
- Owner key read from ignored `.context/.env`; no key included in this document.

Reference: https://github.com/Dstack-TEE/dstack/blob/master/docs/upgrading-to-0.6.md
Retain guest API v0 to preserve key derivation; changing to v1 is a separate migration.

## Validation result

Replica `cvm_yKlzO6jx` reached running with no operation pending and the expected
0.6.1 OS hash. Direct prod16 `/health` returned all true (actions reachable,
CPU available, billing keys present); `/core/v1/version` reported
`v1.1.19-5-gfa3da3152`. The prod2 source also reports OS 0.6.1 and remains stopped,
as it was at the start of this execution.

The public test domain is healthy but its CNAME still points to prod4. Therefore
this execution verifies the new replica directly; it does not claim a public
traffic cutover to prod16 or change the CI ping-pong target variables.

## Same-node ping-pong follow-up (2026-11-12)

Second prod16 replica created with the same compose and OS:
`chipotle-next-rep-sa6xj-rep-adnb3-rep-r1su8` (`cvm_gKo7YPKP`).
No additional on-chain transaction was needed: prod16 and this compose hash
were already authorized.

Intended staging variables after workflow support is deployed:

- NEXT_CVM_A = chipotle-next-rep-sa6xj-rep-adnb3
- NEXT_CVM_B = chipotle-next-rep-sa6xj-rep-adnb3-rep-r1su8

Do NOT enable these in the existing workflow as-is. Its DNS-based selector
always picks A when gateways are identical, its stop-old guard refuses to stop
an instance on the same gateway, and its DNS rollback cannot distinguish these
instances. The active variables/workflow are currently in LIT-Protocol/chipotle;
private has no corresponding variables or deployment secrets. This PR exposes
replica identity and records the rollout plan; the workflow adaptation and
variable activation remain follow-up work.

Required workflow changes before activation:

1. Determine the active instance using instance state/identity, not gateway DNS.
   A normal idle pair must have exactly one running instance and one stopped
   standby. Fail closed for ambiguous/transitional state and serialize deploys.
2. Use INSTANCE-specific URLs for health, expected version and attestation checks;
   shared app URLs can hit either replica while both are running.
3. Define and validate routing during the overlap. Same app ID and gateway may
   distribute traffic across both running instances. DNS does not provide an
   atomic cutover between replicas on the same node.
4. Stop the previous instance only after the new instance passes direct checks;
   verify public health and instance identity after retirement. Rollback must
   start and verify the previous instance, then retire the failed new instance,
   rather than changing a CNAME to the identical gateway.
5. Bootstrap the selected prod16 instance as live, verify custom domain routing,
   leave its partner stopped, then update both NEXT_CVM variables together while
   no deploy is running. Retire old-node instances only after verified cutover.
6. Exercise A-to-B and B-to-A deployments plus a failed deployment rollback.

Production must use the same corrected workflow and validation sequence. Its OS,
pre-launch and device authorizations remain Safe-controlled as described above.
A second replica on an already authorized node and compose needs no additional
Safe transaction unless Phala returns a new authorization requirement. Both
replicas on prod16 share a physical failure domain; this provides deployment
alternation, not resilience to a prod16 outage.

### Replica identity API preparation

The API change in this worktree adds optional `cvm_name` to
`/core/v1/version`, sourced from PHALA_CVM_NAME, and passes that variable into
lit-api-server (previously only the telemetry container received it).
Missing/blank values are omitted. The existing deployed images do not yet
include this field. Replication copies sealed environment values, so set each
replica's own PHALA_CVM_NAME during deployment; never trust a copied source name.
This field is operational metadata, not an attestation proof. Bind checks to
instance-specific URLs and reject missing/unexpected names.

The second prod16 replica reached running and its instance-specific `/health`
returned all true. Both replicas currently run; no old instance was stopped and
NEXT_CVM_A/B remain unchanged pending the workflow and routing rollout.


## Same-node workflow implementation and activation

The follow-up workflow uses `scripts/phala-pingpong.py`. It requires one running
member, one stopped member, matching app IDs, and the running member's own
`cvm_name` from both its instance URL and the public domain. Overrides may only
select the stopped member. All next deployments share one concurrency group,
including dispatches from other branches. Direct checks now use instance URLs;
public build checks require the expected name and build in the same response.
Retirement verifies the target directly and publicly, stops the previous member,
and verifies public health again. Failed deployment, readiness, verification,
cutover or retirement invokes recovery: verify/start the previous instance,
stop the failed target, restore the gateway CNAME, and verify public identity.
Automatic recovery does not run on cancellation; reconcile state before retrying.

Before merging/activating against a configured pair, both images must contain
PR #757 and each sealed environment must contain that instance's actual name.
Bootstrap using the prior deployment workflow/manual deployment while holding
the deployment queue; the new selector intentionally rejects missing names and
both-running pairs. Check the currently configured pair first: this requirement
also applies to the old pair during the migration. Do not merge assuming that
the API source merge alone upgraded every deployed instance.

Activation checklist (not performed by this PR):

1. Pause automatic staging triggers/hold the deployment queue and record existing
   NEXT_CVM_A/B values and public routing. Ensure no staging run is active.
2. Deploy identity-capable images with correct per-instance names to the prod16
   pair and verify each INSTANCE URL independently. Verify health, version and
   attestation. During bootstrap both may run and receive shared-gateway traffic.
3. Ensure the public domain routes to prod16; select A as initial live instance,
   stop B, and require public `/core/v1/version.cvm_name` == A plus healthy checks.
4. Set NEXT_CVM_A and NEXT_CVM_B to the names recorded above while triggers remain
   held. Resume only after the updated workflow is installed and all guards pass.
5. Run a controlled deploy A→B, another B→A (including a same-SHA redeploy), and
   a verification-failure recovery. Confirm the expected public name and exactly
   one running instance after every operation. Only then retire old-node CVMs.

This is overlapping deployment with recovery, not an atomic traffic switch:
Phala may route requests to either member while both run. The workflow makes no
claim that pre-cutover checks prevent the new version receiving traffic.
Production activation needs equivalent workflow changes and Safe authorization;
this staging PR neither alters production workflows nor proposes a Safe transaction.

### First deployment without cvm_name (next and production)

On 2026-11-13 the live next version endpoint still reported
`v1.1.19-5-gfa3da3152` WITHOUT cvm_name, despite PR #757 being merged.
Therefore next also needs bootstrap; merging source is not deployment evidence.
This workflow provides a manual `bootstrap_replica: true` dispatch with an
explicit `phala_cvm_name`. It validates that the target is stopped and shares
the reference app ID, deploys the identity-capable image with its own env name,
and verifies it through its instance URL. Bootstrap deliberately does not retire
other instances or automatically roll back: the operator must reconcile routing
and select the steady-state pair after verifying the target. It cannot be
activated by an automatic push. Use only while the deployment queue is held.
After bootstrap, stop the intended standby and verify the public name before
setting the pair variables/resuming ordinary deployments.

For production's first deployment, cvm_name will be absent. Do not require it
before permitting the bootstrap release. The existing production workflow has a
manual workflow_dispatch `phala_cvm_name` override, but automatic tag releases
cannot supply that input. In the future production workflow PR, add an explicit
temporary repository variable, e.g. PROD_BOOTSTRAP_CVM, read by the tag-triggered
proposal job. Snapshot the selected target into proposal artifacts so execution
cannot retarget if the variable changes. Require the target be a stopped replica
of the production app, preserve all staging/Safe gates, and expose the target in
the Safe review record. The override must never bypass Safe approvals. Clear the
variable after the first successful release and pair initialization; subsequent
releases must use identity-aware selection. Do not hardcode the target in CI.
This staging PR documents that production change but does not implement it.
