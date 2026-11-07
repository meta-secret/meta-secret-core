import { test } from 'node:test';
import assert from 'node:assert/strict';
import { selectCanonicalMember } from './web-liveboard-members.mjs';

test('platform category selects canonical identity, never an emulator alias or punctuation split', () => {
  const members = [{ deviceId: 'web-id', name: 'Browser', category: 'Web', current: true, canRemove: false },
    { deviceId: 'ios-id', name: 'iPhone18,3', category: 'Iphone', current: false, canRemove: true }];
  assert.equal(selectCanonicalMember(members, 'iPhone', { current: false }).name, 'iPhone18,3');
  assert.throws(() => selectCanonicalMember(members, 'iPhone', { expectedDeviceName: 'iPhone18' }), /canonical/);
  assert.throws(() => selectCanonicalMember(members, 'Web', { current: false }), /canonical/);
  assert.throws(() => selectCanonicalMember([...members, { ...members[1], deviceId: 'second' }], 'iPhone', {}), /canonical/);
});
