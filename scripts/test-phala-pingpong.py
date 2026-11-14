import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch, Mock

spec = importlib.util.spec_from_file_location('pingpong', Path(__file__).with_name('phala-pingpong.py'))
p = importlib.util.module_from_spec(spec)
spec.loader.exec_module(p)


def instance(name, status):
    return dict(name=name, status=status, app_id='shared', progress=None,
                endpoints=[dict(app='https://shared-8000.gateway', instance=f'https://{name}-8000.gateway')])


class Guards(unittest.TestCase):
    def selection(self, a='running', b='stopped', override='', public_name='a', direct_name='a'):
        with patch.object(p, 'cvm', side_effect=[instance('a', a), instance('b', b)]), patch.object(p, 'healthy') as health:
            def verify(url, name):
                actual = public_name if url == 'https://public' else direct_name
                if actual != name:
                    raise ValueError('identity mismatch')
            health.side_effect = verify
            return p.select('a', 'b', 'https://public', override)

    def test_same_gateway_a_to_b(self):
        self.assertEqual(self.selection(), ('a', 'b'))

    def test_same_gateway_b_to_a(self):
        self.assertEqual(self.selection('stopped', 'running', public_name='b', direct_name='b'), ('b', 'a'))

    def test_ambiguous_states(self):
        for states in [('running', 'running'), ('stopped', 'stopped'), ('running', 'processing')]:
            with self.subTest(states=states), self.assertRaises(ValueError):
                self.selection(*states)

    def test_override_cannot_target_live_or_unrelated(self):
        for name in ['a', 'elsewhere', '$(exit 1)']:
            with self.subTest(name=name), self.assertRaises(ValueError):
                self.selection(override=name)
        self.assertEqual(self.selection(override='b'), ('a', 'b'))

    def test_wrong_public_or_copied_direct_name(self):
        for kwargs in [dict(public_name='other'), dict(direct_name='copied')]:
            with self.assertRaises(ValueError):
                self.selection(**kwargs)

    def test_bootstrap_requires_stopped_replica(self):
        for status in ['running', 'processing']:
            with patch.object(p, 'cvm', side_effect=[instance('b', status), instance('a', 'running')]), self.assertRaises(ValueError):
                p.bootstrap('b', 'a')
        with patch.object(p, 'cvm', side_effect=[instance('b', 'stopped'), instance('a', 'running')]):
            p.bootstrap('b', 'a')
        with self.assertRaises(ValueError):
            p.bootstrap('', 'a')

    def test_missing_name_and_unhealthy_rejected(self):
        for version, health in [({}, {}), ({'cvm_name': 'a'}, {}),
                               ({'cvm_name': 'a'}, {'lit_actions_reachable': True, 'cpu_available': False, 'billing_keys_present': True})]:
            with patch.object(p, 'get', side_effect=[version, health]), self.assertRaises(ValueError):
                p.healthy('https://a', 'a')

    def test_no_shared_url_fallback(self):
        with self.assertRaises(ValueError):
            p.instance_url({'endpoints': [{'app': 'https://app-8000.gateway'}]})

    def test_different_app_rejected(self):
        a, b = instance('a', 'running'), instance('b', 'stopped')
        b['app_id'] = 'different'
        with patch.object(p, 'cvm', side_effect=[a, b]), self.assertRaises(ValueError):
            p.pair('a', 'b')

    def test_busy_or_alias_rejected(self):
        for data in [dict(instance('a', 'running'), progress={'target': 'running'}), instance('other', 'running')]:
            import json
            with patch.object(p, 'run', return_value=json.dumps(data)), self.assertRaises(ValueError):
                p.cvm('a')

    def test_retire_verifies_before_and_after_stop(self):
        calls = Mock()
        with patch.object(p, 'pair', return_value=(instance('a', 'running'), instance('b', 'running'))), patch.object(p, 'healthy', calls.direct), patch.object(p, 'wait_health', calls.public), patch.object(p, 'stop', calls.stop):
            p.retire('a', 'b', 'https://public')
        self.assertEqual([c[0] for c in calls.mock_calls], ['direct', 'public', 'stop', 'public'])

    def test_unhealthy_target_never_stops_old(self):
        with patch.object(p, 'pair', return_value=(instance('a', 'running'), instance('b', 'running'))), patch.object(p, 'healthy', side_effect=ValueError()), patch.object(p, 'stop') as stop:
            with self.assertRaises(ValueError):
                p.retire('a', 'b', 'https://public')
            stop.assert_not_called()

    def test_rollback_recovers_old_before_stopping_new(self):
        calls = Mock()
        with patch.object(p, 'cvm', return_value=instance('a', 'stopped')), patch.object(p, 'run', calls.start), patch.object(p, 'wait_health', calls.health), patch.object(p, 'stop', calls.stop), patch.object(p, 'restore_dns', calls.dns):
            p.rollback('a', 'b', 'https://public')
        self.assertEqual([c[0] for c in calls.mock_calls], ['start', 'health', 'stop', 'dns', 'health'])

    def test_failed_old_recovery_preserves_new(self):
        with patch.object(p, 'cvm', return_value=instance('a', 'running')), patch.object(p, 'wait_health', side_effect=ValueError()), patch.object(p, 'stop') as stop:
            with self.assertRaises(ValueError):
                p.rollback('a', 'b', 'https://public')
            stop.assert_not_called()


if __name__ == '__main__':
    unittest.main()
