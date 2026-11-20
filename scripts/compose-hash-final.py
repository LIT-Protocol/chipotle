#!/usr/bin/env python3
"""Wait until a compose hash is allowed on the app contract at a finalized block.

The key service decides from finalized chain state. An instance started
between the approval transaction and its finalization (about 18 minutes on
Base) is refused its keys and ends in the `error` state, so a deploy waits
here before it starts the instance.

Usage: compose-hash-final.py <app contract> <compose hash> [timeout seconds]
RPC: $BASE_CHAIN_RPC, else the public Base endpoint.
"""
import json
import os
import re
import sys
import time
import urllib.request

ALLOWED_COMPOSE_HASHES = "0x2f6622e5"  # allowedComposeHashes(bytes32)
DEFAULT_RPC = "https://mainnet.base.org"
ADDRESS = re.compile(r"(0x)?[0-9a-fA-F]{40}")
HASH = re.compile(r"(0x)?[0-9a-fA-F]{64}")


def call_data(compose_hash):
    return ALLOWED_COMPOSE_HASHES + compose_hash.lower().removeprefix("0x")


def allowed(rpc, app, compose_hash, block="finalized", post=None):
    """Whether the hash is allowed as of `block`. Raises on an RPC error."""
    body = {"jsonrpc": "2.0", "id": 1, "method": "eth_call",
            "params": [{"to": app, "data": call_data(compose_hash)}, block]}
    reply = (post or http_post)(rpc, body)
    if "result" not in reply:
        raise ValueError(f"RPC error: {reply.get('error')}")
    result = reply["result"]
    if not re.fullmatch(r"0x[0-9a-fA-F]{64}", result or ""):
        raise ValueError(f"Unexpected eth_call result {result!r}")
    return int(result, 16) == 1


def http_post(url, body):
    request = urllib.request.Request(
        url, data=json.dumps(body).encode(),
        headers={"Content-Type": "application/json", "User-Agent": "compose-hash-final"})
    with urllib.request.urlopen(request, timeout=20) as response:
        return json.load(response)


def wait(rpc, app, compose_hash, timeout, post=None, sleep=time.sleep, clock=time.monotonic):
    deadline = clock() + timeout
    announced = False
    while True:
        error = None
        try:
            if allowed(rpc, app, compose_hash, "finalized", post):
                return
            # Not allowed even at the head: nothing to wait for.
            if not allowed(rpc, app, compose_hash, "latest", post):
                raise SystemExit("::error::Compose hash is not allowed on the app contract")
        except SystemExit:
            raise
        except Exception as caught:  # a flaky RPC is retried until the deadline
            error = caught
        if clock() >= deadline:
            raise SystemExit(f"::error::Compose hash not finalized after {timeout}s"
                             + (f" (last RPC error: {error})" if error else ""))
        if not announced and error is None:
            print("Compose hash is allowed but not yet finalized; waiting.", flush=True)
            announced = True
        sleep(20)


def main(argv):
    if len(argv) not in (2, 3):
        raise SystemExit(__doc__)
    app, compose_hash = argv[0], argv[1]
    timeout = int(argv[2]) if len(argv) == 3 else 2400
    if not ADDRESS.fullmatch(app) or not HASH.fullmatch(compose_hash):
        raise SystemExit("::error::Malformed app contract address or compose hash")
    if not app.startswith("0x"):
        app = "0x" + app
    rpc = os.environ.get("BASE_CHAIN_RPC", "").strip() or DEFAULT_RPC
    wait(rpc, app, compose_hash, timeout)
    print("Compose hash is allowed at a finalized block.")


if __name__ == "__main__":
    main(sys.argv[1:])
