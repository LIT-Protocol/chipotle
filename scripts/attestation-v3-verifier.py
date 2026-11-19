#!/usr/bin/env python3
"""Adapt v3 evidence to dstack-verifier and enforce its verified nonce binding.

expected_report_data must be computed locally from the request nonce, never taken
from the response. dstack-verifier 0.6.1 emits authenticated details.report_data.
"""
import json
import re
import sys


def prepare(response, nonce, expected_report_data):
    if not re.fullmatch(r"[0-9a-f]{64}", nonce):
        raise ValueError("invalid request nonce")
    validate_expected(expected_report_data)
    if response.get("version") != 3 or response.get("nonce") != nonce:
        raise ValueError("v3 response does not match request")
    if response.get("report_data") != expected_report_data:
        raise ValueError("v3 report-data echo does not match request")
    evidence = response.get("attestation")
    if not isinstance(evidence, str) or not re.fullmatch(r"(?:[0-9a-fA-F]{2})+", evidence):
        raise ValueError("invalid v3 attestation encoding")
    return {"attestation": evidence}


def validate_expected(expected_report_data):
    if not re.fullmatch(r"[0-9a-f]{128}", expected_report_data):
        raise ValueError("invalid locally computed report data")


def check(result, expected_report_data):
    validate_expected(expected_report_data)
    if result.get("is_valid") is not True:
        raise ValueError("attestation verification failed")
    details = result.get("details")
    if not isinstance(details, dict) or details.get("report_data") != expected_report_data:
        raise ValueError("verified report data does not match request (stale or wrong evidence)")


def main():
    mode, filename, *args = sys.argv[1:]
    with open(filename) as source:
        document = json.load(source)
    if mode == "prepare" and len(args) == 2:
        json.dump(prepare(document, *args), sys.stdout)
    elif mode == "check" and len(args) == 1:
        check(document, args[0])
        print("Remote attestation and nonce binding verified.")
    else:
        raise ValueError("usage: prepare RESPONSE NONCE REPORT_DATA | check RESULT REPORT_DATA")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, TypeError, AttributeError) as error:
        sys.exit(str(error))
