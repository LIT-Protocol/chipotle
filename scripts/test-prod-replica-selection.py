#!/usr/bin/env python3
"""Offline production selection and Safe-wait snapshot regression tests."""
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('prod', Path(__file__).with_name('phala-prod-target.py'))
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


def cvm(name):
    return dict(id='id-'+name, app_id='abc', name=name)


class ProductionSelection(unittest.TestCase):
    def setUp(self):
        self.env = dict(PROD_CVM_A='a', PROD_CVM_B='b', RELEASE_TAG='v2.0.0',
                        SOURCE_SHA='a'*40, REF_TYPE='tag', PROD_APP_ID='0xabc')
        self.patch = patch.dict(m.p, {'pair': lambda a,b: (cvm(a), cvm(b)),
                                    'select': lambda a,b,url,override='': (a,b),
                                    'bootstrap': lambda a,b: None})
        self.patch.start()
        self.addCleanup(self.patch.stop)

    def test_normal_snapshot(self):
        s = m.prepare(self.env)
        self.assertEqual((s['target'], s['previous'], s['bootstrap']), ('b','a',False))
        self.assertEqual(s['target_id'], 'id-b')

    def test_bootstrap_either_member(self):
        for target, previous in [('a','b'), ('b','a')]:
            s = m.prepare(dict(self.env, PROD_BOOTSTRAP_CVM=target, PROD_BOOTSTRAP_TAG='v2.0.0'))
            self.assertEqual((s['target'],s['previous'],s['bootstrap']), (target,previous,True))

    def test_bootstrap_tag_must_match(self):
        for value in ['', 'v1.0.0']:
            with self.assertRaises(ValueError):
                m.prepare(dict(self.env, PROD_BOOTSTRAP_CVM='b', PROD_BOOTSTRAP_TAG=value))

    def test_bootstrap_requires_configured_pair_member(self):
        with self.assertRaises(ValueError):
            m.prepare(dict(self.env, PROD_BOOTSTRAP_CVM='outside', PROD_BOOTSTRAP_TAG='v2.0.0'))

    def test_manual_override_cannot_replace_bootstrap(self):
        with self.assertRaises(ValueError):
            m.prepare(dict(self.env, PROD_BOOTSTRAP_CVM='b', PROD_BOOTSTRAP_TAG='v2.0.0', INPUT_CVM='other'))

    def test_non_tag_and_wrong_app_rejected(self):
        for change in [dict(REF_TYPE='branch'), dict(PROD_APP_ID='different')]:
            with self.assertRaises(ValueError): m.prepare(dict(self.env, **change))

    def context(self):
        return dict(base_url=m.PUBLIC, api_root_url=m.PUBLIC+'/core/v1', selection=m.prepare(self.env), version_tag='v2.0.0', source_sha='a'*40, phala_app_name='b')

    def test_validate_ignores_changed_target_variables(self):
        m.validate(self.context(), dict(self.env, PROD_CVM_A='other',PROD_BOOTSTRAP_CVM='other'))

    def test_validate_rejects_swapped_live_state(self):
        context=self.context()
        with patch.dict(m.p, {'select': lambda *a: ('b','a')}), self.assertRaises(ValueError):
            m.validate(context,self.env)

    def test_validate_rejects_recreated_cvm(self):
        context=self.context()
        with patch.dict(m.p, {'pair': lambda *a: (cvm('a'),dict(cvm('b'),id='replacement'))}), self.assertRaises(ValueError):
            m.validate(context,self.env)

    def test_context_tampering_rejected(self):
        for key,value in [('source_sha','b'*40),('version_tag','v3'),('phala_app_name','a')]:
            context=self.context();context[key]=value
            with self.assertRaises(ValueError):m.validate(context,self.env)


if __name__ == '__main__': unittest.main()
