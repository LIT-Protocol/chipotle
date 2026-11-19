#!/usr/bin/env python3
"""Resolve which staging lanes a Deploy Staging run deploys.

A lane is one public hostname and the instance pair that serves it. Reads the
workflow's environment and prints `lanes=<json array>` for GITHUB_OUTPUT, in
deploy order. See .github/workflows/deploy-staging.yml.
"""
import json
import os
import re
import sys

LEGACY_DOMAIN = "test.chipotle.litprotocol.com"
LEGACY_DEFAULT_CVM = "chipotle-next-rep-sa6xj"
SHARD = re.compile(r"(\d+)/(\d+)")
HOSTNAME = re.compile(r"(?=.{1,253}$)([a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]{2,}")


def shard(value, what):
    """Validate a shard and return it in canonical form ("00/2" is "0/2")."""
    if not value:
        return ""
    match = SHARD.fullmatch(value)
    if not (match and int(match[1]) < int(match[2]) <= 16):
        raise ValueError(f"{what} must be <index>/<count> with index < count <= 16, got {value!r}")
    return f"{int(match[1])}/{int(match[2])}"


def evidence(value):
    if value not in ("", "v3", "tdx-quote"):
        raise ValueError(f"NEXT_ATTESTATION_EVIDENCE must be v3 or tdx-quote, got {value!r}")
    return value or "v3"


def ingress(value):
    if value not in ("", "1", "2"):
        raise ValueError(f"NEXT_V2_INGRESS must be 1 or 2, got {value!r}")
    return value or "2"


def resolve(env):
    get = lambda key: env.get(key, "").strip()
    selected = get("LANE") or "all"
    if selected not in ("all", "legacy", "v2"):
        raise ValueError(f"Unknown lane {selected!r}")
    input_cvm = get("INPUT_CVM")
    bootstrap = get("BOOTSTRAP") == "true"

    lanes = []
    v2_domain = get("NEXT_V2_DOMAIN").lower()
    if v2_domain:
        if not HOSTNAME.fullmatch(v2_domain) or v2_domain == LEGACY_DOMAIN:
            raise ValueError("NEXT_V2_DOMAIN must be a hostname different from the legacy one")
        # Deploys first: the new hostname is the canary for a build.
        lanes.append({
            "name": "v2",
            "domain": v2_domain,
            "cvm_a": get("NEXT_V2_CVM_A"),
            "cvm_b": get("NEXT_V2_CVM_B"),
            "default_cvm": get("NEXT_V2_CVM_A"),
            "payer_shard": shard(get("NEXT_V2_PAYER_SHARD"), "NEXT_V2_PAYER_SHARD"),
            "peer_base_urls": get("NEXT_V2_PEER_BASE_URLS"),
            "origin_sunset": "",
            "origin_successor_url": "",
            "attestation_evidence": "v3",
            # The v2 hostname is the AMD one; an Intel box there is a failure.
            "expect_platform": "sev-snp",
            # 2.x publishes the TLS evidence clients bind to on any platform.
            "ingress": ingress(get("NEXT_V2_INGRESS")),
        })
    # After the legacy hostname's sunset its lane is dropped, leaving v2 alone.
    retired = get("NEXT_LEGACY_LANE_RETIRED") == "true"
    if retired and not lanes:
        raise ValueError("NEXT_LEGACY_LANE_RETIRED needs the v2 lane (set NEXT_V2_DOMAIN)")
    legacy = {
        "name": "legacy",
        "domain": LEGACY_DOMAIN,
        "cvm_a": get("NEXT_CVM_A"),
        "cvm_b": get("NEXT_CVM_B"),
        "default_cvm": LEGACY_DEFAULT_CVM,
        "payer_shard": shard(get("NEXT_PAYER_SHARD"), "NEXT_PAYER_SHARD"),
        "peer_base_urls": get("NEXT_PEER_BASE_URLS"),
        "origin_sunset": get("NEXT_ORIGIN_SUNSET"),
        "origin_successor_url": get("NEXT_ORIGIN_SUCCESSOR_URL"),
        # An Intel box whose guest agent predates Attest cannot serve v3.
        "attestation_evidence": evidence(get("NEXT_ATTESTATION_EVIDENCE")),
        "ingress": "1",
        "expect_platform": "",
    }
    if not retired:
        lanes.append(legacy)

    # Two serving lanes derive the same payer keys. Without disjoint shares
    # they reuse each other's nonces, which fails user writes at random.
    if len(lanes) == 2:
        shards = [lane["payer_shard"] for lane in lanes]
        counts = {SHARD.fullmatch(value)[2] for value in shards if value}
        if "" in shards or shards[0] == shards[1] or len(counts) != 1:
            raise ValueError(
                "Two lanes need distinct payer shards of the same count: set NEXT_PAYER_SHARD and "
                "NEXT_V2_PAYER_SHARD (for example 0/2 and 1/2)")
        instances = [lane[key] for lane in lanes for key in ("cvm_a", "cvm_b") if lane[key]]
        if len(instances) != len(set(instances)):
            raise ValueError("An instance cannot belong to both lanes")

    enabled = len(lanes)
    if selected != "all":
        lanes = [lane for lane in lanes if lane["name"] == selected]
        if not lanes:
            raise ValueError(f"Lane {selected!r} is not enabled (set NEXT_V2_DOMAIN)")
    if (input_cvm or bootstrap) and len(lanes) != 1:
        raise ValueError("phala_cvm_name and bootstrap_replica apply to one lane: choose it with the lane input")

    # The perf gate runs on one lane: the platform production is moving to when
    # that lane is part of the run, otherwise the only lane there is. A
    # bootstrap brings up an instance's identity; it is not a release and its
    # box may not be the one the baseline was measured on, so it is not gated.
    soak_lane = "v2" if any(lane["name"] == "v2" for lane in lanes) else lanes[0]["name"]
    for lane in lanes:
        lane["input_cvm"] = input_cvm
        lane["bootstrap"] = bootstrap
        lane["soak"] = lane["name"] == soak_lane and not bootstrap
    # The perf-gate status speaks for the commit, so only a run over every
    # enabled lane may post it: a legacy-only run must not overwrite a failed
    # gate on the AMD lane with its own success.
    complete = len(lanes) == enabled and any(lane["soak"] for lane in lanes)
    return lanes, complete


if __name__ == "__main__":
    try:
        resolved, complete = resolve(os.environ)
        print("lanes=" + json.dumps(resolved, separators=(",", ":")))
        # Whether this run may post the perf-gate status.
        print("soak=" + str(complete).lower())
    except ValueError as error:
        sys.exit(f"::error::{error}")
