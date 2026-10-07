import assert from 'node:assert/strict';
import test from 'node:test';
import { classifyError, formatError } from './ui-utils.js';

test('action and wallet permission failures keep the API explanation', () => {
  const messages = [
    'The provided API key is not authorized to execute the specified action (CID/hash).',
    'API key cannot use selected wallet in selected action',
    '403 Forbidden',
  ];
  for (const message of messages) {
    assert.equal(classifyError(new Error(message)).type, 'permission');
    assert.equal(formatError(new Error(message)), message);
  }
  const error = Object.assign(new Error('Wallet access denied'), { status: 403 });
  assert.equal(classifyError(error).type, 'permission');
  assert.equal(formatError(error), error.message);
});

test('authentication failures still prompt login', () => {
  const errors = [
    new Error('401 Unauthorized'),
    new Error('Session expired'),
    new Error('Token expired'),
    Object.assign(new Error('Invalid credential'), { status: 401 }),
  ];
  for (const error of errors) {
    assert.equal(classifyError(error).type, 'auth');
    assert.equal(formatError(error), 'Session expired — please log in again.');
  }
});

test('mentioning an API key alone does not imply an expired session', () => {
  assert.equal(formatError(new Error('Missing API key')), 'Missing API key');
  assert.equal(classifyError(new Error('Failed to fetch')).type, 'network');
  assert.equal(classifyError(new Error('500 Internal Server Error')).type, 'server');
});
