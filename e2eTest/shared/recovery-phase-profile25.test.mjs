// Permanent host regressions for the closed25 extension; no SDK/Core/UI claims.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {createTest24RecoveryPhase, createRecoveryPhaseRegistry} from './recovery-phase.mjs';

const selectedProfile = 'test25:web-secondary-creator-lifecycle';
const platforms = ['web', 'android', 'ios'];
const ports = {now: () => 0, schedule: () => null, cancel() {}};
function binding(sender = 'web') {
  return {version: 1, runId: 'SYNTHETIC_PERMANENT25', testNumber: 25, attempt: 1,
    blockId: 'secret1-three-device', cycle: {web: 13, android: 16, ios: 19}[sender], logicalStep: 1,
    mode: 'web-secondary-creator-lifecycle', contextId: 'a'.repeat(32), sender,
    participants: platforms.map((platform, index) => ({platform, role: platform === sender ? 'sender' : 'receiver',
      invocationId: String(index + 1).repeat(32)}))};
}
function streams(phase) {
  return Object.fromEntries(['android', 'ios'].map(platform => [platform, phase.observer(platform, {})]));
}
function emit(phase, sinks, platform, event) {
  if (platform === 'web') { phase.web(event); return; }
  const context = phase.contextFor(platform);
  const prefix = platform === 'android' ? '11-03 00:15:35.985 18342 18362 I MetaSecretE2E: ' : '';
  sinks[platform].push('stdout', prefix + `E2E: RECOVERY_PHASE_V1 context=${context.contextId} invocation=${context.invocationId} platform=${platform} phase=${event}\n`);
}

test('literal25 registry produces true25 canonical tuples for each actual source sender role', () => {
  for (const sender of platforms) {
    const registry = createRecoveryPhaseRegistry({...ports, selectedProfile}), phase = registry.register(binding(sender));
    try {
      for (const platform of platforms) {
        const value = phase.contextFor(platform);
        assert.equal(value.testNumber, 25); assert.equal(value.mode, 'web-secondary-creator-lifecycle');
        assert.equal(value.role, platform === sender ? 'sender' : 'receiver');
      }
      const sinks = streams(phase); platforms.forEach(platform => emit(phase, sinks, platform, 'READY'));
      phase.releaseSender(); emit(phase, sinks, sender, 'REQUEST_SENT');
      platforms.filter(platform => platform !== sender).forEach(platform => emit(phase, sinks, platform, 'INCOMING_VISIBLE'));
      phase.requireBothBadges(); phase.complete(); assert.throws(() => registry.assertCycleActive(phase.cycle));
    } finally { registry.freeze(Error('SYNTHETIC_END')); }
  }
});

test('new selected25 never aliases default/literal24 and rejects every unactivated selector', () => {
  assert.throws(() => createTest24RecoveryPhase({...ports, binding: binding()}));
  assert.throws(() => createRecoveryPhaseRegistry(ports).register(binding()));
  for (const profile of ['', null, true, selectedProfile + '\n', 'test37:web-member-removal-lifecycle']) {
    assert.throws(() => createRecoveryPhaseRegistry({...ports, selectedProfile: profile}));
  }
  for (const delta of [{testNumber: 24, mode: 'ios-multi-secret-lifecycle'}, {testNumber: 26},
    {cycle: 12}, {cycle: 40}, {logicalStep: 2}, {mode: 'ios-multi-secret-lifecycle'}, {attempt: 1000000000}]) {
    const registry = createRecoveryPhaseRegistry({...ports, selectedProfile});
    try { assert.throws(() => registry.register({...binding(), ...delta})); }
    finally { registry.freeze(Error('SYNTHETIC_END')); }
  }
});

test('selected25 READY199s does not start B; SENT owns180s under unchanged600s cap', () => {
  let now = 0;
  const registry = createRecoveryPhaseRegistry({...ports, selectedProfile, now: () => now});
  const phase = registry.register(binding()), sinks = streams(phase);
  try {
    now = 199_000; platforms.forEach(platform => emit(phase, sinks, platform, 'READY'));
    assert.equal(phase.sentAt, undefined); assert.equal(phase.deadlineAt, 600_000);
    phase.releaseSender(); emit(phase, sinks, 'web', 'REQUEST_SENT');
    assert.deepEqual([phase.sentAt, phase.badgeDeadlineAt, phase.deadlineAt], [199_000, 379_000, 600_000]);
    now = 379_000; assert.throws(() => phase.assertActive(), /expired/);
    assert.throws(() => phase.releaseSender());
  } finally { registry.freeze(Error('SYNTHETIC_END')); }
});

test('selected25 both real current heads release B only; late fulfilled GET cannot extend or abort D', () => {
  let now = 0;
  const registry = createRecoveryPhaseRegistry({...ports, selectedProfile, now: () => now});
  const phase = registry.register(binding()), sinks = streams(phase), receiver = phase.contextFor('android');
  try {
    platforms.forEach(platform => emit(phase, sinks, platform, 'READY'));
    phase.releaseSender(); emit(phase, sinks, 'web', 'REQUEST_SENT');
    emit(phase, sinks, 'android', 'INCOMING_VISIBLE'); assert.throws(() => phase.requireBothBadges());
    emit(phase, sinks, 'ios', 'INCOMING_VISIBLE'); phase.requireBothBadges();
    now = 180_001;
    assert.equal(phase.snapshot(`/recovery-phase?v=1&context=${receiver.contextId}&participant=${receiver.invocationId}&view=request-sent`).status, 410);
    phase.assertActive(); assert.equal(phase.deadlineAt, 600_000);
    now = 600_000; assert.throws(() => phase.assertActive(), /expired/);
  } finally { registry.freeze(Error('SYNTHETIC_END')); }
});
