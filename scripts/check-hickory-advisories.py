#!/usr/bin/env python3
"""Fail closed on unreviewed advisories or an unused Hickory source patch."""
import argparse
from pathlib import Path
import subprocess
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]


def read(path):
    with path.open('rb') as file:
        return tomllib.load(file)


def check(root, database):
    vendor = root / 'vendor/hickory-proto'
    ledger = read(vendor / 'ADVISORIES.toml')
    version = ledger['version']
    if version != '0.25.2' or read(vendor / 'Cargo.toml')['package']['version'] != version:
        raise ValueError('Hickory version changed: review the backport ledger and checker')
    entries = ledger['advisories']
    ids = [entry['id'] for entry in entries]
    if len(ids) != len(set(ids)):
        raise ValueError('Duplicate advisory IDs in ledger')
    for entry in entries:
        if entry['status'] not in {'fixed', 'not-affected', 'accepted'} or not entry['note'].strip():
            raise ValueError(f'Invalid ledger entry: {entry}')
    advisories = list((database / 'crates/hickory-proto').glob('RUSTSEC-*.md'))
    if not advisories:
        raise ValueError('No Hickory advisories found; refusing an empty/missing database')
    unknown = {path.stem for path in advisories} - set(ids)
    if unknown:
        raise ValueError(f'Unreviewed Hickory advisories: {sorted(unknown)}')
    for crate in ['lit-actions', 'lit-api-server']:
        manifest = read(root / crate / 'Cargo.toml')
        patch = manifest['patch']['crates-io']['hickory-proto']['path']
        if (root / crate / patch).resolve() != vendor.resolve():
            raise ValueError(f'{crate}: Hickory patch points elsewhere')
        packages = [p for p in read(root / crate / 'Cargo.lock')['package'] if p['name'] == 'hickory-proto']
        if len(packages) != 1 or packages[0]['version'] != version or 'source' in packages[0]:
            raise ValueError(f'{crate}: expected exactly one local Hickory {version}')
    print(f'Reviewed {len(advisories)} Hickory advisories; both workspaces use the backport')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--database', type=Path, help='Existing RustSec checkout (offline tests)')
    args = parser.parse_args()
    if args.database:
        check(ROOT, args.database)
    else:
        with tempfile.TemporaryDirectory(prefix='hickory-advisories-') as directory:
            database = Path(directory) / 'db'
            subprocess.run(['git', 'clone', '--depth=1', 'https://github.com/rustsec/advisory-db.git', str(database)], check=True)
            subprocess.run(['git', '-C', str(database), 'rev-parse', 'HEAD'], check=True)
            check(ROOT, database)


if __name__ == '__main__':
    main()
