#!/usr/bin/env python3
"""Inject spec_version=1 into vm_config if missing.

The dstack simulator omits spec_version from its vm_config JSON, but the
dstack-verifier Docker image requires it for serde deserialization.
This script reads a verify-request JSON from stdin, adds spec_version if
absent, and writes the patched JSON to stdout.
"""
import json
import sys


def main() -> None:
    d = json.load(sys.stdin)
    try:
        vc = json.loads(d["vm_config"])
        if "spec_version" not in vc:
            vc["spec_version"] = 1
        d["vm_config"] = json.dumps(vc, separators=(",", ":"))
    except Exception:
        pass
    json.dump(d, sys.stdout, separators=(",", ":"))


if __name__ == "__main__":
    main()
