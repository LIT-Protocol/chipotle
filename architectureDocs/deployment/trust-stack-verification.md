# Platform Verification with dstack-verifier

How to verify that a running CVM is executing on genuine Intel TDX hardware with a trusted OS —
before trusting it with API requests.

[dstack-verifier](https://github.com/Dstack-TEE/dstack/tree/main/verifier) is the tool Phala
provides for this. It automates cryptographic validation of the TDX quote, OS image, and event log
integrity. Governance checks (compose-hash and OS-image whitelists on Base) are separate and
performed with `cast` against the DstackApp / DstackKms contracts.

Reference: [Phala Cloud — Verify the Platform](https://docs.phala.com/phala-cloud/attestation/verify-the-platform)

## What dstack-verifier Checks Automatically

| Check | What it does |
|-------|-------------|
| **TDX quote signature** | Verifies the quote against Intel root certs via `dcap-qvl` |
| **TCB SVN** | Confirms the hardware is running the latest security patches; debug mode is off |
| **OS image hash** | Downloads the OS image from `download.dstack.org` and compares against the hash in the quote (MRTD/RTMR0-2) |
| **RTMR3 event log replay** | Replays the event log and asserts the result matches the quoted RTMR3 |
| **KMS identity** | Extracts the KMS ID from the `key-provider` event in RTMR3 |

When all pass, `is_valid: true`. A real TDX deployment also produces `os_image_hash_verified: true`
(the simulator uses a synthetic OS image hash not published on `download.dstack.org`, so that field
is always `false` in local testing — see CI notes below).

## What dstack-verifier Does NOT Check

These require separate governance verification (see below):

- Whether the **compose-hash is whitelisted** in the DstackApp contract
- Whether the **OS image hash is whitelisted** in the DstackKms contract
- Whether the **KMS aggregated MR** is whitelisted
- TLS certificate binding / CAA DNS records

## Verification Workflow

```mermaid
sequenceDiagram
    autonumber
    participant Op as Operator
    participant CVM as CVM<br/>(/attestation)
    participant DV as dstack-verifier<br/>(localhost:8080)
    participant Intel as Intel DCAP
    participant Phala as download.dstack.org
    participant Base as Base chain<br/>(DstackApp / DstackKms)

    Note over Op,CVM: Step 1 — Collect attestation data

    Op->>CVM: GET /attestation
    CVM-->>Op: { quote, event_log, vm_config }

    Note over Op,DV: Step 2 — Platform verification (automated by dstack-verifier)

    Op->>DV: POST /verify { quote, event_log, vm_config, attestation: null }
    DV->>Intel: Verify TDX quote signature (dcap-qvl)
    Intel-->>DV: Quote valid; TCB SVN
    DV->>Phala: Download OS image (os_image_hash from vm_config)
    Phala-->>DV: OS image bytes
    DV->>DV: Compute MRTD/RTMR0-2 from OS image;<br/>replay event log → assert RTMR3 matches;<br/>extract KMS ID from key-provider event
    DV-->>Op: { is_valid, quote_verified, os_image_hash_verified,<br/>details: { kms_id, ... } }

    Note over Op,Base: Step 3 — Governance verification (manual)

    Op->>Base: DstackApp.allowedComposeHashes(composeHash)?
    Base-->>Op: true / false
    Op->>Base: DstackKms.allowedOsImages(osImageHash)?
    Base-->>Op: true / false
```

## Running dstack-verifier

**Start as an HTTP server:**

```bash
# Docker
docker run -p 8080:8080 dstacktee/dstack-verifier:latest

# Or build from source (used in CI)
cargo build --manifest-path=path/to/dstack/verifier/Cargo.toml --bin dstack-verifier
./dstack-verifier -c dstack-verifier.toml
```

**Collect attestation data and verify:**

```bash
# 1. Get attestation data from the CVM
curl https://api.litprotocol.com/attestation -o attestation.json

# 2. Strip 0x prefix from quote, set attestation: null (required by dstack-verifier)
python3 -c '
import sys, json
d = json.load(open("attestation.json"))
q = d["quote"]
q = q[2:] if q.startswith("0x") else q
print(json.dumps({"quote": q, "event_log": d["event_log"], "vm_config": d["vm_config"], "attestation": None}))
' > verify-request.json

# 3. POST to dstack-verifier
curl -s -X POST localhost:8080/verify \
  -H 'Content-Type: application/json' \
  -d @verify-request.json | jq
```

**Expected output (real TDX hardware):**

```json
{
  "is_valid": true,
  "quote_verified": true,
  "os_image_hash_verified": true,
  "details": {
    "kms_id": "...",
    "kms_name": "phala-mainnet"
  }
}
```

## Governance Verification

After `is_valid: true`, verify the application and OS image are authorized on-chain:

```bash
# Is the current compose-hash whitelisted in DstackApp?
cast call <DSTACK_APP_ADDRESS> "allowedComposeHashes(bytes32)" <COMPOSE_HASH> --rpc-url <BASE_RPC>

# Is the OS image hash whitelisted in DstackKms?
cast call <DSTACK_KMS_ADDRESS> "allowedOsImages(bytes32)" <OS_IMAGE_HASH> --rpc-url <BASE_RPC>
```

DstackApp address for this deployment: `0x2f83172A49584C017F2B256F0FB2Dca14126Ba9C`

The compose-hash can be extracted from the RTMR3 event log (the `app-compose` event) or computed
locally: `sha384(docker-compose.yml contents)` with all image references pinned to `@sha256:`
digests.

## CI Usage Notes

The phala-simulator CI job (`phala-simulator.yml`) runs dstack-verifier in **oneshot mode**
(`--verify FILE`) against the dstack simulator. Because the simulator's `attestation.bin` contains
a synthetic OS image hash that has never been published on `download.dstack.org`, the result is:

- `quote_verified: true` — the quote signature pipeline is correct ✓
- `os_image_hash_verified: false` — expected; the hash is synthetic, not a real published image
- `is_valid: false` — expected; a real deployment against actual Phala Cloud returns `true`

The CI assertion is `quote_verified=true`, which is the meaningful signal: it confirms that
`get_quote()` → dstack-verifier is wired end-to-end correctly.

## References

- [Phala Cloud: Verify the Platform](https://docs.phala.com/phala-cloud/attestation/verify-the-platform)
- [Phala Cloud: Chain of Trust](https://docs.phala.com/phala-cloud/attestation/chain-of-trust)
- [dstack-verifier source](https://github.com/Dstack-TEE/dstack/tree/main/verifier)
- [derot-key-issuance.md](derot-key-issuance.md)
- [phala-simulator.yml](../../.github/workflows/phala-simulator.yml)
