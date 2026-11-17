#!/usr/bin/env python3
"""Instance-aware staging deployment guards. Names are operational metadata."""
import json
import os
import subprocess
import sys
import time
import urllib.request


def run(*args):
    return subprocess.check_output(args, text=True, timeout=90).strip()


class Busy(ValueError):
    pass


def cvm(name):
    data = json.loads(run('phala', 'cvms', 'get', name, '--json'))
    if data.get('name') != name:
        raise ValueError(f'{name}: wrong identity')
    if data.get('progress') or data.get('in_progress') or data.get('status') not in ('running', 'stopped'):
        raise Busy(f'{name}: operation pending')
    return data


def instance_url(data):
    urls = [e['instance'].rstrip('/') for e in data.get('endpoints', [])
            if '-8000.' in e.get('instance', '')]
    if len(urls) != 1 or not urls[0].startswith('https://'):
        raise ValueError('Expected one HTTPS instance-specific API URL')
    return urls[0]


def get(url):
    request = urllib.request.Request(url, headers={'Cache-Control': 'no-cache'})
    with urllib.request.urlopen(request, timeout=10) as response:
        return json.load(response)


def healthy(url, name):
    if name is not None and get(url + '/core/v1/version').get('cvm_name') != name:
        raise ValueError(f'{url}: unexpected or missing cvm_name')
    health = get(url + '/health')
    if not all(health.get(k) is True for k in
               ('lit_actions_reachable', 'cpu_available')):
        raise ValueError(f'{name}: unhealthy')


def wait_health(url, name):
    deadline = time.monotonic() + 300
    while True:
        try:
            healthy(url, name)
            return
        except Exception:
            if time.monotonic() >= deadline:
                raise
            time.sleep(5)


def pair(a, b):
    if a == b or not a or not b:
        raise ValueError('Configure two distinct CVM names')
    da, db = cvm(a), cvm(b)
    if not da.get('app_id') or da['app_id'] != db.get('app_id'):
        raise ValueError('Pair must share an app identity')
    return da, db


def select(a, b, public, override=''):
    da, db = pair(a, b)
    if (da['status'], db['status']) == ('running', 'stopped'):
        live, cold, live_data = a, b, da
    elif (da['status'], db['status']) == ('stopped', 'running'):
        live, cold, live_data = b, a, db
    else:
        raise ValueError('Pair must have exactly one running and one stopped instance')
    wait_health(instance_url(live_data), live)
    wait_health(public, live)
    if override and override != cold:
        raise ValueError('Override must name the stopped member of the configured pair')
    return live, cold


def bootstrap(target, reference):
    if not target or target == reference:
        raise ValueError('Bootstrap requires an explicit replica distinct from the reference')
    replica, source = pair(target, reference)
    if replica['status'] != 'stopped':
        raise ValueError('Bootstrap target must be stopped')
    if source['status'] != 'running':
        raise ValueError('Bootstrap reference must be running')
    wait_health(instance_url(source), None)


def wait_state(name, status):
    deadline = time.monotonic() + 300
    while time.monotonic() < deadline:
        try:
            data = cvm(name)
            if data['status'] == status:
                return data
        except Busy:
            pass
        time.sleep(5)
    raise ValueError(f'{name}: did not reach {status}')


def stop(name):
    deadline = time.monotonic() + 300
    requested = False
    while time.monotonic() < deadline:
        try:
            data = cvm(name)
            if data['status'] == 'stopped':
                return
            if not requested:
                run('phala', 'cvms', 'stop', name)
                requested = True
        except (Busy, subprocess.CalledProcessError, subprocess.TimeoutExpired):
            # A failed deployment can still be settling; retry within the deadline.
            pass
        time.sleep(5)
    raise ValueError(f'{name}: stop did not complete; manual intervention required')


def retire(old, new, public):
    _, target = pair(old, new)
    if target['status'] != 'running':
        raise ValueError('Target is not running')
    wait_health(instance_url(target), new)
    # During overlap the public gateway may serve either instance. This proves
    # the new one is reachable, not that the old one has stopped receiving traffic.
    wait_health(public, new)
    stop(old)
    wait_health(public, new)


def restore_dns(old, public):
    domain = public.removeprefix('https://').rstrip('/')
    gateway = old.get('gateway', {}).get('cname', '')
    if not gateway.startswith('gateway.') or not gateway.endswith('.phala.network'):
        raise ValueError('Invalid rollback gateway')
    credentials = json.loads(run('aws', 'sts', 'assume-role', '--role-arn',
                                 os.environ['CERTBOT_AWS_ROLE_ARN'],
                                 '--role-session-name', 'pingpong-rollback'))['Credentials']
    os.environ.update(AWS_ACCESS_KEY_ID=credentials['AccessKeyId'],
                      AWS_SECRET_ACCESS_KEY=credentials['SecretAccessKey'],
                      AWS_SESSION_TOKEN=credentials['SessionToken'])
    zone_name = domain.split('.', 1)[1] + '.'
    zones = json.loads(run('aws', 'route53', 'list-hosted-zones-by-name',
                          '--dns-name', zone_name))['HostedZones']
    zones = [z for z in zones if z['Name'] == zone_name and not z['Config']['PrivateZone']]
    if len(zones) != 1:
        raise ValueError('Expected exactly one public hosted zone')
    batch = {'Changes': [{'Action': 'UPSERT', 'ResourceRecordSet': {
        'Name': domain, 'Type': 'CNAME', 'TTL': 30,
        'ResourceRecords': [{'Value': '_.' + gateway.split('.', 1)[1]}]}}]}
    run('aws', 'route53', 'change-resource-record-sets', '--hosted-zone-id',
        zones[0]['Id'], '--change-batch', json.dumps(batch))


def rollback(old, new, public, bootstrap_mode=False):
    # A failed update may leave NEW busy. Only OLD must be stable before recovery.
    source = cvm(old)
    if source['status'] == 'stopped':
        run('phala', 'cvms', 'start', old)
    elif source['status'] != 'running':
        raise ValueError('Previous instance cannot be recovered automatically')
    source = wait_state(old, 'running')
    expected_name = None if bootstrap_mode else old
    wait_health(instance_url(source), expected_name)
    # Remove the failed instance before restoring DNS so its ingress cannot
    # immediately reclaim the record. A healthy OLD has been verified first.
    stop_error = None
    try:
        stop(new)
    except Exception as error:
        stop_error = error
        print(f'::error::Failed to stop {new}: {error}. Restoring DNS; recovery remains incomplete.', file=sys.stderr)
    restore_dns(source, public)
    wait_health(public, expected_name)
    if stop_error:
        raise ValueError('Recovery incomplete: failed target may still serve traffic') from stop_error


def main():
    action, a, b, public, *extra = sys.argv[1:]
    if not public.startswith('https://'):
        raise ValueError('Public URL must use HTTPS')
    if action == 'select':
        live, target = select(a, b, public, extra[0] if extra else '')
        print(f'prev_live_cvm={live}\ntarget_cvm={target}\npingpong=true')
    elif action == 'bootstrap':
        bootstrap(a, b)
    elif action == 'retire':
        retire(a, b, public)
    elif action in ('rollback', 'rollback-bootstrap'):
        rollback(a, b, public, bootstrap_mode=action == 'rollback-bootstrap')
    else:
        raise ValueError('Unknown action')


if __name__ == '__main__':
    main()
