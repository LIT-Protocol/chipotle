#!/usr/bin/env python3
"""Fix event log for dstack verifier: compute digests for runtime events with empty digest.

Why is digest empty? The dstack guest-agent strips digests from runtime events (RTMR3,
event_type 0x08000001) to reduce response size. The digest is deterministically derived as
SHA384(event_type || ":" || event || ":" || payload), so it can be recomputed. The verifier
is meant to handle this, but the Docker verifier's serde parser rejects digest="".
We fill in the computed digest before calling the verifier.
"""
import hashlib
import json
import struct
import sys

DSTACK_RUNTIME = 0x08000001


def hex_to_bytes(s: str) -> bytes:
    return bytes.fromhex(s) if s else b""


def compute_digest(event: str, payload_hex: str) -> str:
    payload = hex_to_bytes(payload_hex)
    data = struct.pack("<I", DSTACK_RUNTIME) + b":" + event.encode() + b":" + payload
    return hashlib.sha384(data).hexdigest()


def main() -> None:
    attest_path = sys.argv[1]
    with open(attest_path) as f:
        d = json.load(f)
    events = json.loads(d["event_log"])
    for e in events:
        if e.get("digest") == "" and e.get("event_type") == DSTACK_RUNTIME:
            e["digest"] = compute_digest(e.get("event", ""), e.get("event_payload", ""))
    d["event_log"] = json.dumps(events)
    q = d["quote"]
    q = q[2:] if isinstance(q, str) and q.startswith("0x") else q
    out = {"quote": q, "event_log": d["event_log"], "vm_config": d["vm_config"], "attestation": None}
    json.dump(out, sys.stdout, separators=(",", ":"))


if __name__ == "__main__":
    main()
