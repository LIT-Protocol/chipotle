#!/usr/bin/env python3
"""Exercise fail-closed behavior without network access."""
from pathlib import Path
import runpy
import shutil
import tempfile
import unittest

MODULE = runpy.run_path(str(Path(__file__).with_name('check-hickory-advisories.py')))
CHECK = MODULE['check']
ROOT = MODULE['ROOT']


class AdvisoryGuardTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        for directory, files in {
            'vendor/hickory-proto': ['Cargo.toml', 'ADVISORIES.toml'],
            'lit-actions': ['Cargo.toml', 'Cargo.lock'],
            'lit-api-server': ['Cargo.toml', 'Cargo.lock'],
        }.items():
            (self.root / directory).mkdir(parents=True)
            for file in files:
                shutil.copyfile(ROOT / directory / file, self.root / directory / file)
        self.db = self.root / 'database'
        self.advisories = self.db / 'crates/hickory-proto'
        self.advisories.mkdir(parents=True)
        for entry in MODULE['read'](self.root / 'vendor/hickory-proto/ADVISORIES.toml')['advisories']:
            (self.advisories / (entry['id'] + '.md')).touch()

    def check(self):
        CHECK(self.root, self.db)

    def test_reviewed(self):
        self.check()

    def test_new_advisory(self):
        (self.advisories / 'RUSTSEC-2099-9999.md').touch()
        with self.assertRaisesRegex(ValueError, 'Unreviewed'):
            self.check()

    def test_empty_database(self):
        shutil.rmtree(self.advisories)
        with self.assertRaisesRegex(ValueError, 'No Hickory'):
            self.check()

    def test_registry_resolution(self):
        for crate in ['lit-actions', 'lit-api-server']:
            path = self.root / crate / 'Cargo.lock'
            original = path.read_text()
            path.write_text(original.replace('name = "hickory-proto"', 'name = "hickory-proto"\nsource = "registry+https://github.com/rust-lang/crates.io-index"'))
            with self.assertRaisesRegex(ValueError, 'expected exactly one local'):
                self.check()
            path.write_text(original)

    def test_version_change(self):
        path = self.root / 'vendor/hickory-proto/Cargo.toml'
        path.write_text(path.read_text().replace('version = "0.25.2"', 'version = "0.25.3"', 1))
        with self.assertRaisesRegex(ValueError, 'version changed'):
            self.check()

    def test_wrong_patch(self):
        path = self.root / 'lit-actions/Cargo.toml'
        path.write_text(path.read_text().replace('../vendor/hickory-proto', '../vendor/other'))
        with self.assertRaisesRegex(ValueError, 'points elsewhere'):
            self.check()

    def test_invalid_status(self):
        path = self.root / 'vendor/hickory-proto/ADVISORIES.toml'
        path.write_text(path.read_text().replace('status = "fixed"', 'status = "unknown"'))
        with self.assertRaisesRegex(ValueError, 'Invalid ledger'):
            self.check()

    def test_duplicate_id(self):
        path = self.root / 'vendor/hickory-proto/ADVISORIES.toml'
        path.write_text(path.read_text().replace('RUSTSEC-2026-0489', 'RUSTSEC-2026-0487'))
        with self.assertRaisesRegex(ValueError, 'Duplicate'):
            self.check()


if __name__ == '__main__':
    unittest.main()
