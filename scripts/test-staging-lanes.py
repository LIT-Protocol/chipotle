#!/usr/bin/env python3
"""Lane resolution for Deploy Staging. Run: python3 scripts/test-staging-lanes.py"""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("lanes", Path(__file__).with_name("staging-lanes.py"))
lanes = importlib.util.module_from_spec(spec)
spec.loader.exec_module(lanes)

def resolve(env):
    return lanes.resolve(env)[0]


PAIR = {"NEXT_CVM_A": "intel-a", "NEXT_CVM_B": "intel-b"}
V2 = {"NEXT_V2_DOMAIN": "test2.chipotle.litprotocol.com", "NEXT_V2_CVM_A": "amd-a", "NEXT_V2_CVM_B": "amd-b",
      "NEXT_PAYER_SHARD": "0/2", "NEXT_V2_PAYER_SHARD": "1/2"}


class LaneTests(unittest.TestCase):
    def test_without_v2_vars_the_run_is_exactly_the_single_lane_it_always_was(self):
        for env in ({}, PAIR, {**PAIR, "LANE": "all"}, {**PAIR, "LANE": "legacy"}):
            result = resolve(env)
            self.assertEqual([lane["name"] for lane in result], ["legacy"])
            lane = result[0]
            self.assertEqual(lane["domain"], "test.chipotle.litprotocol.com")
            self.assertEqual(lane["default_cvm"], "chipotle-next-rep-sa6xj")
            self.assertEqual((lane["cvm_a"], lane["cvm_b"]), (env.get("NEXT_CVM_A", ""), env.get("NEXT_CVM_B", "")))
            self.assertTrue(lane["soak"])
            self.assertEqual(lane["payer_shard"], "")
            self.assertEqual(lane["attestation_evidence"], "v3")
            self.assertEqual(lane["ingress"], "1")

    def test_manual_target_and_bootstrap_still_work_on_a_single_lane(self):
        lane = resolve({**PAIR, "INPUT_CVM": "intel-b", "BOOTSTRAP": "true"})[0]
        self.assertEqual((lane["input_cvm"], lane["bootstrap"]), ("intel-b", True))
        # A bootstrap is not a release: it carries no perf gate.
        self.assertFalse(lane["soak"])
        self.assertFalse(resolve(PAIR)[0]["bootstrap"])
        self.assertTrue(resolve({**PAIR, "INPUT_CVM": "intel-b"})[0]["soak"])

    def test_a_retired_legacy_lane_leaves_v2_alone(self):
        result = resolve({**PAIR, **V2, "NEXT_LEGACY_LANE_RETIRED": "true"})
        self.assertEqual([(lane["name"], lane["soak"]) for lane in result], [("v2", True)])
        # One lane again, so the shard requirement no longer applies.
        solo = resolve({"NEXT_V2_DOMAIN": V2["NEXT_V2_DOMAIN"], "NEXT_V2_CVM_A": "amd-a",
                              "NEXT_LEGACY_LANE_RETIRED": "true", "INPUT_CVM": "amd-a"})
        self.assertEqual((solo[0]["name"], solo[0]["payer_shard"], solo[0]["input_cvm"]), ("v2", "", "amd-a"))
        for env in ({**PAIR, "NEXT_LEGACY_LANE_RETIRED": "true"},
                    {**PAIR, **V2, "NEXT_LEGACY_LANE_RETIRED": "true", "LANE": "legacy"}):
            with self.assertRaises(ValueError):
                resolve(env)

    def test_two_lanes_deploy_v2_first_with_the_perf_gate_on_v2(self):
        result = resolve({**PAIR, **V2, "NEXT_ORIGIN_SUNSET": "Fri, 15 Jan 2027 00:00:00 GMT"})
        self.assertEqual([(lane["name"], lane["soak"]) for lane in result], [("v2", True), ("legacy", False)])
        self.assertEqual([lane["payer_shard"] for lane in result], ["1/2", "0/2"])
        self.assertEqual(result[0]["domain"], "test2.chipotle.litprotocol.com")
        # Only the retiring hostname announces a sunset.
        self.assertEqual(result[0]["origin_sunset"], "")
        self.assertEqual(result[1]["origin_sunset"], "Fri, 15 Jan 2027 00:00:00 GMT")

    def test_the_v2_lane_runs_the_newer_ingress_unless_told_otherwise(self):
        self.assertEqual([l["ingress"] for l in resolve({**PAIR, **V2})], ["2", "1"])
        self.assertEqual([l["ingress"] for l in resolve({**PAIR, **V2, "NEXT_V2_INGRESS": "1"})], ["1", "1"])
        with self.assertRaises(ValueError):
            resolve({**PAIR, **V2, "NEXT_V2_INGRESS": "3"})

    def test_only_the_legacy_lane_can_be_verified_by_tdx_quote(self):
        result = resolve({**PAIR, **V2, "NEXT_ATTESTATION_EVIDENCE": "tdx-quote"})
        self.assertEqual([lane["attestation_evidence"] for lane in result], ["v3", "tdx-quote"])
        with self.assertRaises(ValueError):
            resolve({**PAIR, "NEXT_ATTESTATION_EVIDENCE": "none"})

    def test_one_lane_of_two_can_be_deployed_alone(self):
        self.assertEqual([l["name"] for l in resolve({**PAIR, **V2, "LANE": "v2"})], ["v2"])
        legacy = resolve({**PAIR, **V2, "LANE": "legacy", "INPUT_CVM": "intel-b"})
        self.assertEqual([(l["name"], l["soak"], l["input_cvm"]) for l in legacy], [("legacy", True, "intel-b")])

    def test_shards_are_compared_as_numbers(self):
        # "00/2" is shard 0 to the server, so it collides with "0/2".
        with self.assertRaises(ValueError):
            resolve({**PAIR, **V2, "NEXT_PAYER_SHARD": "0/2", "NEXT_V2_PAYER_SHARD": "00/2"})
        result = resolve({**PAIR, **V2, "NEXT_PAYER_SHARD": "00/02", "NEXT_V2_PAYER_SHARD": "1/2"})
        self.assertEqual([lane["payer_shard"] for lane in result], ["1/2", "0/2"])

    def test_only_a_run_over_every_enabled_lane_posts_the_perf_gate(self):
        complete = lambda env: lanes.resolve(env)[1]
        self.assertTrue(complete(PAIR))
        self.assertTrue(complete({**PAIR, **V2}))
        # One lane of two: its soak still runs, but it does not speak for the commit.
        self.assertFalse(complete({**PAIR, **V2, "LANE": "legacy"}))
        self.assertFalse(complete({**PAIR, **V2, "LANE": "v2"}))
        self.assertFalse(complete({**PAIR, "INPUT_CVM": "intel-b", "BOOTSTRAP": "true"}))
        self.assertTrue(complete({**PAIR, **V2, "NEXT_LEGACY_LANE_RETIRED": "true"}))

    def test_the_v2_lane_must_be_amd(self):
        self.assertEqual([l["expect_platform"] for l in resolve({**PAIR, **V2})], ["sev-snp", ""])

    def test_two_lanes_require_disjoint_payer_shards(self):
        for shards in ({}, {"NEXT_PAYER_SHARD": "0/2"}, {"NEXT_PAYER_SHARD": "0/2", "NEXT_V2_PAYER_SHARD": "0/2"},
                       {"NEXT_PAYER_SHARD": "0/2", "NEXT_V2_PAYER_SHARD": "1/3"}):
            env = {**PAIR, "NEXT_V2_DOMAIN": V2["NEXT_V2_DOMAIN"], "NEXT_V2_CVM_A": "amd-a", **shards}
            with self.assertRaises(ValueError, msg=shards):
                resolve(env)
        # The check holds even when only one of the two lanes is being deployed.
        with self.assertRaises(ValueError):
            resolve({**PAIR, "NEXT_V2_DOMAIN": V2["NEXT_V2_DOMAIN"], "LANE": "legacy"})

    def test_misconfiguration_is_rejected(self):
        bad = [
            {**PAIR, "LANE": "v2"},                                   # lane not enabled
            {**PAIR, "LANE": "other"},
            {**PAIR, **V2, "INPUT_CVM": "amd-b"},                     # ambiguous target
            {**PAIR, **V2, "BOOTSTRAP": "true"},
            {**PAIR, **V2, "NEXT_V2_CVM_B": "intel-a"},               # shared instance
            {**PAIR, **V2, "NEXT_V2_DOMAIN": "test.chipotle.litprotocol.com"},
            {**PAIR, **V2, "NEXT_V2_DOMAIN": "https://x.example"},
            {**PAIR, "NEXT_PAYER_SHARD": "2/2"},
            {**PAIR, "NEXT_PAYER_SHARD": "half"},
        ]
        for env in bad:
            with self.assertRaises(ValueError, msg=env):
                resolve(env)


if __name__ == "__main__":
    unittest.main()
