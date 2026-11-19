#!/usr/bin/env python3
"""Ingress selection for lane compose files; requires PyYAML.

Run: python3 scripts/test-phala-compose.py
"""
import importlib.util
from pathlib import Path
import re
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("compose", Path(__file__).with_name("phala-compose.py"))
compose = importlib.util.module_from_spec(spec)
spec.loader.exec_module(compose)
SOURCE = (ROOT / "docker-compose.phala.yml").read_text()
V2 = (ROOT / "docker-compose.phala.ingress-v2.yml").read_text()


class ComposeTests(unittest.TestCase):
    def services(self, generation):
        text = compose.render(generation, SOURCE, V2)
        self.assertNotIn("BEGIN dstack-ingress", text)
        self.assertNotIn("END dstack-ingress", text)
        return text, yaml.safe_load(text)

    def test_generation_1_is_the_compose_file_minus_its_markers(self):
        text, parsed = self.services("1")
        expected = "\n".join(line for line in SOURCE.split("\n")
                              if not line.startswith(("  # BEGIN dstack-ingress", "  # END dstack-ingress")))
        self.assertEqual(text, expected)
        ingress = parsed["services"]["dstack-ingress"]
        self.assertTrue(ingress["image"].startswith("dstacktee/dstack-ingress:1.4@sha256:"))
        # The source file itself stays a valid compose file with the same services.
        self.assertEqual(yaml.safe_load(SOURCE)["services"].keys(), parsed["services"].keys())

    def test_generation_2_swaps_only_the_ingress(self):
        _, one = self.services("1")
        text, two = self.services("2")
        self.assertEqual(one["services"].keys(), two["services"].keys())
        self.assertEqual(one["volumes"], two["volumes"])
        for name in one["services"]:
            if name != "dstack-ingress":
                self.assertEqual(one["services"][name], two["services"][name], name)
        ingress = two["services"]["dstack-ingress"]
        self.assertRegex(ingress["image"], r"^ghcr\.io/dstack-tee/dstack-ingress:2\.\d+@sha256:[0-9a-f]{64}$")
        self.assertEqual(text.count("  dstack-ingress:"), 1)

    def test_generation_2_keeps_the_settings_the_deploy_relies_on(self):
        _, parsed = self.services("2")
        ingress = parsed["services"]["dstack-ingress"]
        env = ingress["environment"]
        old = yaml.safe_load(SOURCE)["services"]["dstack-ingress"]["environment"]
        # Same hostname, gateway, backend and Route 53 credentials as 1.x.
        # Android refuses a CNAME target that starts with an underscore.
        self.assertEqual(env["GATEWAY_DOMAIN"], "gateway.${DSTACK_GATEWAY_DOMAIN}")
        for key in ("DOMAIN", "DNS_PROVIDER", "TARGET_ENDPOINT",
                    "AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY", "AWS_ROLE_ARN", "AWS_REGION"):
            self.assertEqual(env[key], old[key], key)
        # A CAA would turn the hostname's CNAME into an A record, which the
        # rollback and the production replica selection cannot follow.
        self.assertEqual(env["SET_CAA"], "false")
        self.assertIn("/var/run/dstack.sock:/var/run/dstack.sock", ingress["volumes"])
        self.assertIn("cert-data:/etc/letsencrypt", ingress["volumes"])
        self.assertEqual(ingress["ports"], ["443:443"])

    def test_generation_2_distrusts_the_compromised_roots_everywhere(self):
        _, parsed = self.services("2")
        ingress = parsed["services"]["dstack-ingress"]
        script = ingress["entrypoint"][-1]
        self.assertEqual(ingress["entrypoint"][:2], ["/bin/sh", "-c"])
        for needle in ("emSign_ECC_Root_CA_-_G3", "Comodo_AAA_Services_root", "update-ca-certificates --fresh",
                       "! grep -E 'emSign ECC Root CA - G3|AAA Certificate Services'"):
            self.assertIn(needle, script)
        # Overriding entrypoint drops the image CMD, so it is restated.
        self.assertTrue(script.endswith("exec /scripts/entrypoint.sh haproxy -W -f /etc/haproxy/haproxy.cfg"))
        # Compose would interpolate a dollar sign in the script.
        self.assertNotIn("$", script)
        # The image's Python tools have their own CA bundle; point them at the patched one.
        for key in ("REQUESTS_CA_BUNDLE", "AWS_CA_BUNDLE", "SSL_CERT_FILE"):
            self.assertEqual(ingress["environment"][key], "/etc/ssl/certs/ca-certificates.crt")

    def test_placeholders_survive_for_the_workflows_to_substitute(self):
        for generation in ("1", "2"):
            text, _ = self.services(generation)
            for placeholder in ("${DOCKER_IMAGE_LIT_API_SERVER}", "${CERTBOT_DOMAIN}", "${CERTBOT_AWS_REGION}", "${GCP_PROJECT_ID}"):
                self.assertIn(placeholder, text, f"{placeholder} in generation {generation}")

    def test_bad_input_is_rejected(self):
        with self.assertRaises(ValueError):
            compose.render("3", SOURCE, V2)
        with self.assertRaises(ValueError):
            compose.render("1", re.sub(r"  # END dstack-ingress\n", "", SOURCE), V2)
        with self.assertRaises(ValueError):
            compose.render("2", SOURCE, "  something-else:\n    image: x\n")


if __name__ == "__main__":
    unittest.main()
