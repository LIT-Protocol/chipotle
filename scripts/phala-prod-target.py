#!/usr/bin/env python3
"""Snapshot and revalidate a production deployment target across Safe approval."""
import json
import os
from pathlib import Path
import runpy
import sys

p = runpy.run_path(str(Path(__file__).with_name('phala-pingpong.py')))
PUBLIC = 'https://api.chipotle.litprotocol.com'


def app_id(value):
    return value.lower().removeprefix('0x').removeprefix('app_')


def prepare(env):
    a, b = env.get('PROD_CVM_A', ''), env.get('PROD_CVM_B', '')
    override = env.get('INPUT_CVM', '')
    target = env.get('PROD_BOOTSTRAP_CVM', '')
    bootstrap_tag = env.get('PROD_BOOTSTRAP_TAG', '')
    tag = env['RELEASE_TAG']
    if not tag.startswith('v') or env.get('REF_TYPE') != 'tag':
        raise ValueError('Production preparation requires a release tag, including manual dispatch')
    bootstrap = bool(target or bootstrap_tag)
    if bootstrap:
        if not a or not b or a == b or target not in (a, b) or bootstrap_tag != tag:
            raise ValueError('Bootstrap target and exact release tag must both match')
        if override and override != target:
            raise ValueError('Manual target conflicts with reviewed bootstrap target')
        previous = b if target == a else a
        p['bootstrap'](target, previous)
    else:
        previous, target = p['select'](a, b, PUBLIC, override)
    old, new = p['pair'](previous, target)
    expected = env.get('PROD_APP_ID', '')
    if not expected or app_id(new['app_id']) != app_id(expected):
        raise ValueError('Target does not belong to the configured production app')
    if not old.get('id') or not new.get('id'):
        raise ValueError('Missing stable CVM identifiers')
    return dict(target=target, previous=previous, target_id=new['id'], previous_id=old['id'],
                app_id=new['app_id'], bootstrap=bootstrap, version_tag=tag,
                source_sha=env['SOURCE_SHA'], public_url=PUBLIC)


def validate(context, env):
    selection = context['selection']
    if context.get('base_url') != PUBLIC or context.get('api_root_url') != PUBLIC + '/core/v1':
        raise ValueError('Deployment context contains a non-production public endpoint')
    if selection['version_tag'] != env['RELEASE_TAG'] or context['version_tag'] != env['RELEASE_TAG']:
        raise ValueError('Release tag differs from prepared context')
    if context['source_sha'] != selection['source_sha'] or context['phala_app_name'] != selection['target']:
        raise ValueError('Deployment context does not match its target snapshot')
    if selection['public_url'] != PUBLIC or app_id(selection['app_id']) != app_id(env['PROD_APP_ID']):
        raise ValueError('Wrong production app or public URL')
    old, new = p['pair'](selection['previous'], selection['target'])
    if (old['id'], new['id']) != (selection['previous_id'], selection['target_id']):
        raise ValueError('CVM identity changed after proposal')
    if app_id(new['app_id']) != app_id(selection['app_id']):
        raise ValueError('App identity changed after proposal')
    if selection['bootstrap'] is True:
        p['bootstrap'](selection['target'], selection['previous'])
    elif selection['bootstrap'] is False:
        live, target = p['select'](selection['previous'], selection['target'], PUBLIC)
        if (live, target) != (selection['previous'], selection['target']):
            raise ValueError('Pair changed while waiting for Safe approval; prepare a new proposal')
    else:
        raise ValueError('Invalid bootstrap flag')


if __name__ == '__main__':
    if sys.argv[1] == 'prepare':
        print(json.dumps(prepare(os.environ)))
    elif sys.argv[1] == 'validate':
        validate(json.loads(Path(sys.argv[2]).read_text()), os.environ)
    else:
        raise ValueError('Unknown command')
