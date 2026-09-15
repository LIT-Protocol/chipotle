"""Example any-language Lit Action in Python.

Shells out to the preinstalled `lit` CLI for every op. A signing library
(e.g. solana / eth_keys) would consume the raw key from `lit
get-private-key` — the point of the any-language runner is that YOUR
imported code does the signing inside the sandbox.
"""

import json
import subprocess


def lit(*args, stdin=None):
    """Call the `lit` CLI, returning stdout without the trailing newline."""
    out = subprocess.run(
        ["lit", *args], input=stdin, capture_output=True, text=True, check=True
    )
    return out.stdout.rstrip("\n")


def main():
    params = json.loads(lit("params") or "null") or {}
    lit("print", f"python action running with params: {params}")

    # Fetch the raw derived key for a permitted PKP wallet and "sign" with it.
    # Replace with a real signature (ed25519/secp256k1) via your own library.
    pkp_id = params.get("pkpId", "example-pkp")
    secret = lit("get-private-key", pkp_id)
    signature = f"signed-with-{len(secret)}-byte-key"

    lit("set-response", stdin=json.dumps({"ok": True, "signature": signature}))


if __name__ == "__main__":
    main()
