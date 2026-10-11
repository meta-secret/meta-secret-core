import test from 'node:test';
import assert from 'node:assert/strict';
import { recoveryClaimIds, selectNewPendingRecoveryClaim } from './test15-recovery-claims.mjs';

const oldWebClaim = {
  id: 'old-web-claim',
  type: 'Recover',
  password: 'test-secret',
  sender: 'web-device',
  receivers: [{ id: 'cli-device', status: 'Pending' }],
};
const currentAndroidClaim = {
  id: 'current-android-claim',
  type: 'Recover',
  password: 'test-secret',
  sender: 'android-device',
  receivers: [{ id: 'cli-device', status: 'Pending' }],
};

test('Test15 CLI approver ignores an older pending recovery and selects the new sender claim', () => {
  const baseline = recoveryClaimIds({ claims: [oldWebClaim] }, 'test-secret');
  const selected = selectNewPendingRecoveryClaim(
    { claims: [oldWebClaim, currentAndroidClaim] },
    { password: 'test-secret', baselineClaimIds: baseline, receiverId: 'cli-device' },
  );
  assert.equal(selected, currentAndroidClaim);
});

test('Test15 CLI recovery selection can require the configured sender', () => {
  const baseline = recoveryClaimIds({ claims: [oldWebClaim] }, 'test-secret');
  assert.equal(selectNewPendingRecoveryClaim(
    { claims: [oldWebClaim, currentAndroidClaim] },
    {
      password: 'test-secret',
      baselineClaimIds: baseline,
      receiverId: 'cli-device',
      senderId: 'android-device',
    },
  ), currentAndroidClaim);
  assert.equal(selectNewPendingRecoveryClaim(
    { claims: [oldWebClaim, currentAndroidClaim] },
    {
      password: 'test-secret',
      baselineClaimIds: baseline,
      receiverId: 'cli-device',
      senderId: 'ios-device',
    },
  ), null);
});
