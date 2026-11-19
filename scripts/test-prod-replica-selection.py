#!/usr/bin/env python3
"""Offline prod target-selection checks; requires PyYAML, Bash and jq.

Run: python3 scripts/test-prod-replica-selection.py
Executes only the selection portion of the workflow with mocked DNS/Phala CLIs.
"""

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[1]
REPLICAS = ('chipotle-prod-rep-r68r6', 'chipotle-prod-rep-08tv3')


def endpoint(node, layout='endpoints'):
    return json.dumps({layout: [{'app': f'https://app-8000.dstack-base-prod{node}.phala.network'}]})


class ProdReplicaSelectionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        workflow = yaml.safe_load(
            (ROOT / '.github/workflows/deploy-prod-lane-propose.yml').read_text()
        )
        step = next(s for s in workflow['jobs']['prepare-deploy']['steps']
                    if s.get('id') == 'prepare')
        cls.input_binding = step['env'].get('INPUT_CVM')
        # Stop before any provisioning. Exercise the actual workflow shell and
        # output contract, including its set -euo pipefail behavior.
        cls.script = step['run'].split('# Build the API request payload', 1)[0]
        if '${{' in cls.script:
            raise AssertionError('Selection inputs must be passed through env')

    def run_selection(self, dns='_.dstack-base-prod2.phala.network.',
                      first=None, second=None, resolver='dig', override='',
                      dns_status=0, phala_status=0, replicas=None):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for command in ('jq', 'grep', 'sed', 'sort'):
                target = shutil.which(command)
                self.assertIsNotNone(target, f'{command} is required')
                (root / command).symlink_to(target)
            if resolver:
                checks = {
                    'dig': '[ "$1" = +short ] && [ "$2" = CNAME ] && [ "$3" = api.chipotle.litprotocol.com ]',
                    'host': '[ "$1" = -t ] && [ "$2" = CNAME ] && [ "$3" = api.chipotle.litprotocol.com ]',
                    'nslookup': '[ "$1" = -type=CNAME ] && [ "$2" = api.chipotle.litprotocol.com ]',
                }
                stub = root / resolver
                stub.write_text('#!/bin/bash\n' + checks[resolver] + ' || exit 99\n'
                                'printf "%s\\n" "$DNS_RESULT"\nexit "$DNS_STATUS"\n')
                stub.chmod(0o755)
            phala = root / 'phala'
            phala.write_text(
                '#!/bin/bash\n'
                'printf "called\\n" >> "$CALL_LOG"\n'
                '[ "$1" = cvms ] && [ "$2" = get ] && [ "$4" = --json ] || exit 99\n'
                f'case "$3" in\n{REPLICAS[0]}) printf "%s\\n" "$FIRST";;\n'
                f'{REPLICAS[1]}) printf "%s\\n" "$SECOND";;\n*) exit 99;;\nesac\n'
                'exit "$PHALA_STATUS"\n'
            )
            phala.chmod(0o755)
            output = root / 'output'
            calls = root / 'calls'
            env = dict(os.environ, PATH=directory, GITHUB_OUTPUT=str(output),
                       CALL_LOG=str(calls), INPUT_CVM=override, DNS_RESULT=dns,
                       DNS_STATUS=str(dns_status), PHALA_STATUS=str(phala_status),
                       FIRST=endpoint(2) if first is None else first,
                       SECOND=endpoint(4) if second is None else second,
                       LANE_REPLICAS=' '.join(REPLICAS if replicas is None else replicas),
                       PUBLIC_DOMAIN='api.chipotle.litprotocol.com', LANE_NAME='legacy')
            result = subprocess.run(['/bin/bash', '-c', self.script], env=env,
                                    capture_output=True, text=True, timeout=10)
            return result, output.read_text() if output.exists() else '', calls.exists()

    def assert_failure(self, message, **kwargs):
        result, output, _ = self.run_selection(**kwargs)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn('::error::' + message, result.stdout)
        self.assertIn('phala_cvm_name', result.stdout)
        self.assertEqual(output, '')

    def test_both_cutover_directions(self):
        for live, cold in [(2, REPLICAS[1]), (4, REPLICAS[0])]:
            with self.subTest(live=live):
                result, output, _ = self.run_selection(dns=f'_.dstack-base-prod{live}.phala.network.')
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(output, f'cvm_name={cold}\n')

    def test_resolver_fallbacks(self):
        for resolver, dns in [
            ('host', 'api.chipotle.litprotocol.com is an alias for _.dstack-base-prod2.phala.network.'),
            ('nslookup', 'api.chipotle.litprotocol.com\tcanonical name = _.dstack-base-prod2.phala.network.'),
        ]:
            with self.subTest(resolver=resolver):
                result, output, _ = self.run_selection(resolver=resolver, dns=dns)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(output, f'cvm_name={REPLICAS[1]}\n')

    def test_dns_failure_guidance(self):
        for kwargs in [dict(dns=''), dict(resolver=None), dict(dns_status=1),
                       dict(dns='192.0.2.1'),
                       dict(dns='_.dstack-base-prod2.phala.network.example.org.'),
                       dict(dns='_.dstack-base-prod2.phala.network.\n_.dstack-base-prod4.phala.network.')]:
            with self.subTest(kwargs=kwargs):
                self.assert_failure('Could not determine the live prod node', **kwargs)

    def test_cvm_failure_guidance(self):
        for kwargs in [dict(phala_status=1), dict(first='invalid json'),
                       dict(first='{}'), dict(second='{}'),
                       dict(first=endpoint(2)[:-1] + ', "gateway": {"cname": "gateway.dstack-base-prod4.phala.network"}}')]:
            with self.subTest(kwargs=kwargs):
                self.assert_failure('Could not resolve the node for replica', **kwargs)

    def test_neither_replica_live(self):
        self.assert_failure('Neither prod replica', dns='_.dstack-base-prod9.phala.network.')

    def test_both_replicas_live(self):
        self.assert_failure('Both prod replicas', second=endpoint(2))

    def test_same_node_pair_is_told_apart_by_power_state(self):
        def replica(status):
            return json.dumps({'status': status, 'endpoints': [
                {'app': 'https://app-8000.dstack-base-prod2.phala.network'}]})
        for first, second, cold in [('running', 'stopped', REPLICAS[1]), ('stopped', 'running', REPLICAS[0])]:
            with self.subTest(first=first):
                result, output, _ = self.run_selection(first=replica(first), second=replica(second))
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertEqual(output, f'cvm_name={cold}\n')
        # Never guess between two running or two stopped replicas.
        for first, second in [('running', 'running'), ('stopped', 'stopped'), ('running', 'updating')]:
            with self.subTest(first=first, second=second):
                self.assert_failure('Both prod replicas', first=replica(first), second=replica(second))

    def test_first_deploy_of_a_lane_targets_its_first_replica(self):
        # No DNS record yet and nothing running: there is no live replica to protect.
        stopped = json.dumps({'status': 'stopped', 'gateway': {'cname': 'gateway.dstack-base-prod16.phala.network'}})
        result, output, _ = self.run_selection(dns='', first=stopped, second=stopped)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(output, f'cvm_name={REPLICAS[0]}\n')
        # A running replica with no DNS record is not a first deploy: refuse to guess.
        running = json.dumps({'status': 'running', 'gateway': {'cname': 'gateway.dstack-base-prod16.phala.network'}})
        self.assert_failure('Could not determine the live prod node', dns='', first=stopped, second=running)

    def test_single_replica_lane_is_updated_in_place(self):
        result, output, called = self.run_selection(replicas=(REPLICAS[0],))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(output, f'cvm_name={REPLICAS[0]}\n')
        self.assertFalse(called, 'no lookup is needed to pick the only replica')

    def test_lane_needs_one_or_two_replicas(self):
        for replicas in [(), REPLICAS + ('chipotle-prod-rep-third',)]:
            with self.subTest(replicas=replicas):
                self.assert_failure('Lane', replicas=replicas)

    def test_public_urls_layout(self):
        result, output, _ = self.run_selection(first=endpoint(2, 'public_urls'))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(output, f'cvm_name={REPLICAS[1]}\n')

    def test_gateway_without_endpoints(self):
        result, output, _ = self.run_selection(second=json.dumps({
            'gateway': {'cname': 'gateway.dstack-base-prod4.phala.network.'}}))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(output, f'cvm_name={REPLICAS[1]}\n')

    def test_repeated_node_is_unambiguous(self):
        data = json.loads(endpoint(2))
        data['endpoints'] *= 1000
        data['gateway'] = {'cname': 'gateway.dstack-base-prod2.phala.network'}
        result, output, _ = self.run_selection(first=json.dumps(data))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(output, f'cvm_name={REPLICAS[1]}\n')

    def test_manual_override_skips_detection(self):
        # The operator's phala_cvm_name reaches the selection through the lane call.
        self.assertEqual(self.input_binding, '${{ inputs.input_cvm }}')
        caller = yaml.safe_load((ROOT / '.github/workflows/deploy-prod-1-propose.yml').read_text())
        self.assertEqual(caller['jobs']['lane']['with']['input_cvm'], '${{ inputs.phala_cvm_name }}')
        for override in ['new-replica', 'literal-$(exit 42)']:
            with self.subTest(override=override):
                result, output, called = self.run_selection(override=override, resolver=None, phala_status=1)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(output, f'cvm_name={override}\n')
                self.assertFalse(called)


if __name__ == '__main__':
    unittest.main()
