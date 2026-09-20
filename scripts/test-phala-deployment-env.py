#!/usr/bin/env python3
"""Offline deployment contracts; requires PyYAML, Node.js and Docker Compose v2.

Run: python3 scripts/test-phala-deployment-env.py
No Phala calls, Docker daemon, credentials or running containers are needed.
"""

import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import tempfile
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[1]
KEY = "NEW_ACCOUNT_RATE_LIMIT_ENABLED"


def workflow_runs(filename):
    workflow = yaml.safe_load((ROOT / ".github/workflows" / filename).read_text())
    return "\n".join(
        step["run"]
        for job in workflow["jobs"].values()
        for step in job.get("steps", [])
        if "run" in step
    )


def extract(pattern, text):
    match = re.search(pattern, text, re.DOTALL)
    if not match:
        raise AssertionError(f"Missing deployment configuration matching {pattern}")
    return match.group(1)


class PhalaDeploymentEnvTests(unittest.TestCase):
    def test_compose_forwards_with_enabled_default(self):
        text = (ROOT / "docker-compose.phala.yml").read_text()
        env = yaml.safe_load(text)["services"]["lit-api-server"]["environment"]
        self.assertEqual(env.get(KEY), "${NEW_ACCOUNT_RATE_LIMIT_ENABLED:-true}")
        self.assertRegex(text, rf'{KEY}: "\$\{{{KEY}:-true\}}"')

    def test_compose_renders_unset_empty_and_explicit_values(self):
        text = (ROOT / "docker-compose.phala.yml").read_text()
        # Isolate from the caller's environment and any repository .env file.
        env = {"PATH": os.environ["PATH"], "HOME": "/nonexistent"}
        env.update({key: "unused" for key in re.findall(r"\$\{(\w+)", text)})
        for key in list(env):
            if key.startswith("DOCKER_IMAGE_"):
                env[key] = "example.invalid/offline-test:latest"
        with tempfile.TemporaryDirectory() as directory:
            compose = Path(directory) / "compose.yml"
            compose.write_text(text)
            for value, expected in [(None, "true"), ("", "true"), ("false", "false"), ("true", "true")]:
                with self.subTest(value=value):
                    env.pop(KEY, None)
                    if value is not None:
                        env[KEY] = value
                    result = subprocess.run(
                        ["docker", "compose", "--env-file", "/dev/null", "-f", str(compose),
                         "config", "--format", "json"],
                        cwd=directory, env=env, check=True, capture_output=True, text=True,
                    )
                    rendered = json.loads(result.stdout)["services"]["lit-api-server"]["environment"]
                    self.assertEqual(rendered.get(KEY), expected)

    def assert_cli_disabled(self, filename, command):
        runs = workflow_runs(filename)
        # Inspect the actual continued command, not a comment or step env block.
        command_text = extract(rf"(?m)^\s*({re.escape(command)}\b[^\n]*)", runs.replace("\\\n", " "))
        command_text = re.sub(r"\$\{\{.*?\}\}", "workflow-input", command_text)
        tokens = shlex.split(command_text)
        values = [tokens[i + 1] for i, token in enumerate(tokens[:-1]) if token == "-e"]
        self.assertEqual([value for value in values if value.startswith(KEY + "=")], [KEY + "=false"])

    def test_next_deploy_disables_limit(self):
        self.assert_cli_disabled("deploy-staging.yml", "phala deploy")

    def test_manual_env_update_disables_limit(self):
        self.assert_cli_disabled("manual_phala-envs-update.yml", "phala envs update")

    def production_envs(self):
        runs = workflow_runs("deploy-prod-1-propose.yml")
        expression = extract(r"const envs = (\[.*?\]);", runs)
        # Evaluate only the array in an isolated JS context with empty envs.
        # Never run the workflow or import the Phala SDK.
        result = subprocess.run(
            ["node", "-e", 'const vm = require("node:vm"); '
             'const fs = require("node:fs"); '
             'console.log(JSON.stringify(vm.runInNewContext(fs.readFileSync(0, "utf8"), '
             '{process: {env: {}}})));'],
            input=expression, check=True, capture_output=True, text=True,
        )
        return json.loads(result.stdout)

    def test_production_encrypts_literal_string_false(self):
        envs = self.production_envs()
        self.assertEqual([entry for entry in envs if entry["key"] == KEY], [{"key": KEY, "value": "false"}])

    def test_production_env_lists_match_without_duplicates(self):
        propose = workflow_runs("deploy-prod-1-propose.yml")
        execute = workflow_runs("deploy-prod-2-execute-manual.yml")
        allowed = json.loads(extract(r"allowed_envs:\s*(\[.*?\])", propose))
        committed = json.loads(extract(r"ENV_KEYS='(\[.*?\])'", execute))
        encrypted = [entry["key"] for entry in self.production_envs()]
        for keys in (allowed, committed, encrypted):
            self.assertIn(KEY, keys)
            self.assertEqual(len(keys), len(set(keys)), "Duplicate environment key")
        self.assertEqual(set(allowed), set(encrypted))
        self.assertEqual(set(committed), set(encrypted))


if __name__ == "__main__":
    unittest.main(verbosity=2)
