#!/usr/bin/env python3
"""Regression checks for the deployment gate, using synthetic verifier outputs."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("v3", Path(__file__).with_name("attestation-v3-verifier.py"))
v3 = importlib.util.module_from_spec(spec)
spec.loader.exec_module(v3)
NONCE = "11" * 32
REPORT = "22" * 32 + "00" * 32


class GateTests(unittest.TestCase):
    def response(self):
        return {"version": 3, "nonce": NONCE, "report_data": REPORT, "attestation": "0102"}

    def test_prepare_preserves_only_exact_evidence(self):
        self.assertEqual(v3.prepare(self.response(), NONCE, REPORT), {"attestation": "0102"})

    def test_wrong_nonce_version_or_echo_rejected(self):
        for field, value in [("nonce", "00" * 32), ("version", 2), ("report_data", "00" * 64)]:
            response = self.response()
            response[field] = value
            with self.assertRaises(ValueError):
                v3.prepare(response, NONCE, REPORT)

    def test_malformed_evidence_rejected(self):
        for value in [None, "", "0", "zz", [], "0x0102"]:
            response = self.response()
            response["attestation"] = value
            with self.assertRaises(ValueError):
                v3.prepare(response, NONCE, REPORT)

    def test_verified_binding_passes(self):
        v3.check({"is_valid": True, "details": {"report_data": REPORT}}, REPORT)

    def test_valid_but_replayed_evidence_rejected_despite_matching_echoes(self):
        v3.prepare(self.response(), NONCE, REPORT)
        with self.assertRaises(ValueError):
            v3.check({"is_valid": True, "details": {"report_data": "00" * 64}}, REPORT)

    def test_unverified_or_missing_binding_rejected(self):
        for result in [{}, {"is_valid": "true"}, {"is_valid": 1},
                       {"is_valid": False, "details": {"report_data": REPORT}},
                       {"is_valid": True}, {"is_valid": True, "details": {}},
                       {"is_valid": True, "details": None}]:
            with self.assertRaises(ValueError):
                v3.check(result, REPORT)

    def test_invalid_local_commitment_rejected(self):
        with self.assertRaises(ValueError):
            v3.prepare(self.response(), NONCE, "bad")
        with self.assertRaises(ValueError):
            v3.check({"is_valid": True, "details": {"report_data": "bad"}}, "bad")


if __name__ == "__main__":
    unittest.main()
