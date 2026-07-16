# PKP owner backfill (#575)

After the #575 facet upgrade, `registerWalletDerivation` binds each pkpId to its
first owner (`pkpIdToOwnerMaster`) and `getWalletDerivation` refuses to serve a
pkpId to any account that isn't the owner. Existing PKPs have an empty binding
(`owner == 0`), which falls through so signing keeps working — but also means
they are still claimable until backfilled. This tooling generates and verifies
the one-time backfill.

## Source of truth: account enumeration, not events

Ownership is reconstructed by enumerating every account and reading its on-chain
`pkpData` via `listPkps` — the exact mapping the node reads to sign. This matters
because some PKPs were migrated into the diamond's storage without ever emitting
a `WalletDerivationRegistered` event; an event-only scan silently misses them.
A pkpId held by more than one account is an on-chain hijack and is excluded
unless you pass `--allow-conflicts` (which then disambiguates via the
first-registrant event).

## Diamond / admin (Base mainnet)

- Diamond: `0xaAaAA9120fE271F653cfDb6bf400dB93D2DEa7Aa`
- `backfillPkpOwners` is callable by the diamond owner (the 2-of-4 Safe
  `0xF688411c0FFc300cAb33EB1dA651DBb3E6891098`) or the `configOperator` EOA.

## Safe path (recommended)

Set `ALCHEMY_API_KEY` in `.context/.env` (or `BASE_RPC_URL` in the env), then:

```bash
# 1. Generate Safe Transaction Builder JSON (one file per Safe tx, multisend-bundled).
npx hardhat pkp-backfill:gen-safe

# 2. Verify the generated JSON against authoritative on-chain ownership.
#    Decodes every call, checks each pkpId is owned by the bound account, that it
#    is currently unbound or already correct, and that no unbound PKP is missing.
npx hardhat pkp-backfill:verify-safe --dir .context/pkp-backfill
```

Output lands in `.context/pkp-backfill/` (gitignored): `safe-backfill-NN.json`
plus `manifest.json`. Import each `safe-backfill-NN.json` into the Safe
**Transaction Builder** app, then sign + execute. Order does not matter and
re-running is safe — `backfillPkpOwners` skips already-bound pkpIds.

**Re-verify after execution**: re-run `verify-safe`; every executed pkpId should
report "already correct" and 0 remaining to write.

Useful flags: `--batch-size` (pkpIds per `backfillPkpOwners` call, default 1000),
`--calls-per-file` (calls bundled into one Safe tx, default 2 ≈ ~53M gas/file on
Base's 400M limit), `--diamond`, `--rpc-url`.

## Direct EOA path (if you hold the configOperator/owner key)

```bash
CONFIG_OPERATOR_PRIVATE_KEY=0x... \
  npx hardhat backfill-pkp-owners --diamond 0xaAaAA9120fE271F653cfDb6bf400dB93D2DEa7Aa --network base --execute
```

Dry-run (no `--execute`) prints what would be written. Conflicts are a hard stop
under `--execute` unless `--allow-conflicts`.
