import { test } from 'node:test';
import assert from 'node:assert/strict';
import { nextVersion } from './release.mjs';

test('first release uses the configured version floor', () => {
  assert.equal(nextVersion('0.1.0', []), '0.1.0');
});
test('patch increments the highest stable version, not lexical order or prereleases', () => {
  assert.equal(nextVersion('0.1.0', ['0.1.9', '0.1.10', '1.0.0-beta.1']), '0.1.11');
});
test('maintainers can request a minor or major bump with the floor', () => {
  assert.equal(nextVersion('1.0.0', ['0.1.10']), '1.0.0');
  assert.equal(nextVersion('0.1.0', ['1.2.3']), '1.2.4');
});
test('invalid release floors fail closed', () => {
  assert.throws(() => nextVersion('1.0.0-beta', []));
});
