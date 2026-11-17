# Production prod16 migration and alternating releases

Status: implementation proposal; no production CVMs or contract permissions have
been changed by this work. Keep this operational plan in the private repository.
See `phala-os-061-upgrade.md` for the tested next OS/KMS migration sequence.

## Before the first production release

1. Finish next bootstrap, its direct-instance verification, attestation, OpenAPI,
   smoke, correctness and soak gates. Retire the other prod16 instance only after
   the replacement is verified. Then test a normal alternating deployment and
   restore automatic next deployments. A feature-branch dispatch does not replace
   the main commit's `perf-gate/soak` status required by production preparation.
2. Inventory production's app identity, current healthy replica, OS/compose hashes,
   gateway and DNS. Confirm Base KMS availability on prod16. Retain the existing
   healthy production replica as the first deployment's recovery source.
3. Prepare Safe proposals for EVERY operation performed with the dstack app owner
   key during next migration: in particular `addComposeHash` for the upgraded
   manifest and `addDevice` for the destination device. Verify the exact production
   AppAuth contract, Safe ownership and hash/device values before approval. Do not
   use the next app key for production. Execute the approved transactions before
   starting a replica that depends on them; record transaction hashes privately.
4. Upgrade/replicate using the reviewed dstack 0.6.1 sequence. Provision a stopped
   prod16 target belonging to the SAME production app. Do not stop the live source.
   Starting a replica can immediately expose it through the shared app gateway;
   this is not an isolated canary.
5. Configure GitHub's `production` environment with required reviewers and allowed
   release refs. Set `PROD_CVM_A` to the current healthy source and `PROD_CVM_B` to
   the stopped prod16 target. Verify `APP_AUTH_ADDRESS_CHIPOTLE_PROD` and
   `SAFE_ADDRESS_CHIPOTLE_PROD`. These are configured in the repository that will
   actually run the released workflows; this PR does not change those settings.
6. In the `production` environment set `PROD_BOOTSTRAP_CVM` to the stopped member
   and `PROD_BOOTSTRAP_TAG` to the EXACT upcoming release tag. The first production
   deployment may have no `cvm_name`, so bootstrap verifies source health without
   requiring that field. The target must be one of the configured pair and must
   be stopped. An absent, mismatched or stale tag fails closed. Set both variables
   before pushing the release tag: a tag push cannot accept workflow inputs.

## Release and Safe approval

Phase 1 runs on a release tag (manual dispatch must also use a tag ref). It records
both CVM names, stable CVM IDs, app identity, source SHA and bootstrap mode in the
draft release deployment context. Review this context along with the compose hash
and Safe transaction. Normal selection requires exactly one running and one
stopped member, with matching live identity on both direct and public endpoints.
A manual target override can only select the stopped member.

After Safe signers approve and execute the transaction, dispatch Phase 2 with the
release tag and Safe transaction hash. Before the Phala mutation, Phase 2 verifies
that its context has not changed since download and revalidates the frozen pair's
IDs, app identity and live/stopped state. A changed live pair requires preparing a
new proposal; editing current CVM variables does not retarget an existing proposal.
The production environment approval applies again at commit time.

Phase 2 verifies the target's own instance endpoint and expected commit/name,
attestation, OpenAPI and k6 smoke/correctness before confirming the public endpoint
and stopping the previous replica. Publishing the release and triggering static
site deployment happen only after successful retirement. On a deployment or gate
failure after mutation begins, recovery verifies/starts the previous replica,
stops the failed target and restores DNS. Bootstrap recovery accepts the old
replica's missing `cvm_name`. Pre-mutation validation failures do not run recovery.

Do not cancel a deployment after mutation starts: cancellation and runner loss
cannot guarantee automatic recovery. Inspect both CVMs and DNS, verify the healthy
source directly, then perform the documented recovery manually if necessary.
Phases share a non-cancelling concurrency group, but GitHub may replace a pending
run with a newer pending run. Submit one release at a time and check run state;
the group does not reserve the pair while waiting for Safe approval.

## Complete migration and steady state

After the first release passes, clear BOTH bootstrap variables. Create the second
prod16 replica using the same production app and authorized OS/device settings,
keeping it stopped. Update the configured pair to the two prod16 members, with
exactly the verified new release running. Preserve the old source stopped until
the migration owner approves its retirement.

The next release selects the stopped prod16 member automatically. Repeat all Safe
and verification gates; the previously running member becomes the next stopped
target. Confirm one successful release in each direction before declaring the
migration complete. No production tag, Safe proposal, deployment, or public release
is authorized merely by opening or merging this implementation PR.

## Validation

Offline tests cover tag-scoped bootstrap, pair membership, production app checks,
manual override conflicts and stale/replaced target snapshots. Shared selector and
recovery tests cover health retry and rollback behaviour. Workflow syntax is checked
with actionlint. Live production execution remains an operational acceptance step.
