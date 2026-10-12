import { test } from 'node:test';
import assert from 'node:assert/strict';
import { canonicalDeletionMetadata } from './metadata.mjs';

test('canonical response whitelist binds exact Pass ID/UUID and excludes sensitive fields', () => {
  const pass = { name: 'synthetic-label', id: 'opaque-pass' };
  const snapshot = canonicalDeletionMetadata({ data: [{ vault: { key: { objId: { id: { curr: 7 } } }, value: {
    vaultName: 'hidden-vault', users: { 'hidden-device': { keyShare: 'hidden-share' } },
    secrets: [], deletedSecrets: [pass],
    deletionResults: { 'exact-uuid': { request: { requestId: 'exact-uuid', passId: pass, sender: 'hidden-device' }, accepted: true } },
    privateKey: 'hidden-private', secret: 'hidden-plaintext', cipherText: 'hidden-cipher',
  } } }] });
  assert.deepEqual(snapshot, { revision: 7, secrets: [], deletedIds: ['opaque-pass'], states: [{ secretId: 'opaque-pass', requestId: 'exact-uuid', clientStatus: 'Done' }] });
  assert.equal(JSON.stringify(snapshot).includes('hidden-'), false);
  assert.equal(canonicalDeletionMetadata({ unrelated: { secrets: [pass] } }), null);
});

test('latest canonical event wins even when historical snapshots arrive last', () => {
  const event = (revision, secrets) => ({ vault: { key: { objId: { id: { curr: revision } } }, value: { vaultName: 'hidden-vault', users: {}, secrets, deletedSecrets: [], deletionResults: {} } } });
  const result = canonicalDeletionMetadata({ data: [event(9, [{ id: 'fresh', name: 'same-name' }]), event(3, [{ id: 'obsolete', name: 'same-name' }])] });
  assert.equal(result.revision, 9); assert.equal(result.secrets[0].id, 'fresh');
});

test('unknown opaque-ID schema fails rather than fabricate identity', () => {
  assert.throws(() => canonicalDeletionMetadata({ key: { objId: { id: { curr: 1 } } }, value: { vaultName: 'vault', secrets: [{ name: 'label', id: { text: 'unknown' } }], deletedSecrets: [], deletionResults: {} } }), /unknown wire shape/);
});
