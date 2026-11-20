#!/usr/bin/env python3
"""Waiting for a compose hash to be finalized.

Run: python3 scripts/test-compose-hash-final.py
"""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("final", Path(__file__).with_name("compose-hash-final.py"))
final = importlib.util.module_from_spec(spec)
spec.loader.exec_module(final)

APP = "0x969A8C14c9e13420705B19C7246AeeD27897e7eA"
HASH = "5071754a0548cac725ab7c570f8c8d764a849ccaa7fb8166fef419567fa9309d"
TRUE = "0x" + "0" * 63 + "1"
FALSE = "0x" + "0" * 64


class Chain:
    """Answers eth_call per block tag; `finalized` turns true after N calls."""

    def __init__(self, latest, finalized_after=None, fail_first=0):
        self.latest, self.finalized_after, self.fail_first = latest, finalized_after, fail_first
        self.finalized_calls = 0
        self.requests = []

    def __call__(self, url, body):
        self.requests.append(body)
        if self.fail_first:
            self.fail_first -= 1
            raise OSError("connection reset")
        if body["params"][1] == "latest":
            return {"result": TRUE if self.latest else FALSE}
        self.finalized_calls += 1
        done = self.finalized_after is not None and self.finalized_calls > self.finalized_after
        return {"result": TRUE if done else FALSE}


class Clock:
    def __init__(self):
        self.now = 0

    def sleep(self, seconds):
        self.now += seconds

    def __call__(self):
        return self.now


def wait(chain, timeout=600):
    clock = Clock()
    final.wait("rpc", APP, HASH, timeout, post=chain, sleep=clock.sleep, clock=clock)
    return clock


class FinalityTests(unittest.TestCase):
    def test_call_targets_the_app_with_the_hash(self):
        chain = Chain(latest=True, finalized_after=0)
        wait(chain)
        params = chain.requests[0]["params"]
        self.assertEqual(params[0], {"to": APP, "data": "0x2f6622e5" + HASH})
        self.assertEqual(params[1], "finalized")
        self.assertEqual(final.call_data("0x" + HASH.upper()), "0x2f6622e5" + HASH)

    def test_returns_at_once_when_already_finalized(self):
        self.assertEqual(wait(Chain(latest=True, finalized_after=0)).now, 0)

    def test_waits_until_the_approval_is_finalized(self):
        clock = wait(Chain(latest=True, finalized_after=3))
        self.assertEqual(clock.now, 60)

    def test_a_hash_never_approved_fails_without_waiting(self):
        with self.assertRaises(SystemExit) as raised:
            wait(Chain(latest=False))
        self.assertIn("not allowed", str(raised.exception))

    def test_times_out_when_finality_never_comes(self):
        with self.assertRaises(SystemExit) as raised:
            wait(Chain(latest=True), timeout=100)
        self.assertIn("not finalized after 100s", str(raised.exception))

    def test_rpc_failures_are_retried(self):
        clock = wait(Chain(latest=True, finalized_after=0, fail_first=2))
        self.assertEqual(clock.now, 40)

    def test_rpc_error_reply_is_not_read_as_false(self):
        with self.assertRaises(ValueError):
            final.allowed("rpc", APP, HASH, post=lambda url, body: {"error": {"code": -32000}})
        with self.assertRaises(ValueError):
            final.allowed("rpc", APP, HASH, post=lambda url, body: {"result": "0x"})

    def test_rejects_malformed_arguments(self):
        for args in (["0x12", HASH], [APP, "abc"], [APP]):
            with self.assertRaises(SystemExit):
                final.main(args)


if __name__ == "__main__":
    unittest.main()
