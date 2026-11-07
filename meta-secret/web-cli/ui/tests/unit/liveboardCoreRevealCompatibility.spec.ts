import { describe, expect, it, vi } from 'vitest';
import { createCoreAdapter } from '@/features/liveboard/coreAdapter';
import { readCoreProjection } from '@/features/liveboard/coreProjection';
import { contextFixture, coreState, memberFixture, operation, platformFixture } from './liveboardCoreFixtures';

const source = (objType: string, objInstance: string, commitment: string) => ({
  eventId: { fqdn: { objType, objInstance }, id: { curr: 1, prev: 0 } },
  bodyCommitment: commitment.repeat(64),
});
const revealContext = () => ({
  version: 1,
  vaultName: 'семья@example.test',
  currentDeviceId: 'self',
  vaultSource: source('Vault', 'семья@example.test', 'a'),
  localDistributionSource: source('SsDistribution', 'compatibility-local-marker', 'b'),
  recoverySources: [source('SsRecovery', 'compatibility-remote-marker', 'c')],
  claimScopeCommitment: 'd'.repeat(64),
});

function ownRecovery(status: 'accepted' | 'done' = 'accepted') {
  return coreState({
    storageMode: 'shares',
    secrets: [
      {
        secretId: 'pass-a',
        primaryAction: status === 'accepted' ? 'showRecovered' : 'recover',
        recoveryClaimId: status === 'accepted' ? 'claim-second' : undefined,
        canDelete: true,
        readiness: 'ready',
      },
    ],
    operations: [
      operation({
        requesterId: 'self',
        clientStatus: status,
        eligiblePeerIds: ['peer-a'],
        canApprove: false,
        canDecline: false,
        outcome: 'accepted',
        responderId: 'peer-a',
        resolutionSource: 'canonical',
        readiness: status === 'accepted' ? 'ready' : 'done',
      }),
    ],
  });
}

function compareProjection(state: ReturnType<typeof coreState>, ready = true) {
  const extended = {
    ...state,
    operations: state.operations.map((op) =>
      op.requesterId === 'self' ? { ...op, revealContext: revealContext() } : op,
    ),
  };
  const direct = [state, extended].map((dto) => {
    const { member } = memberFixture(JSON.stringify(dto));
    const result = readCoreProjection({ member, identity: 'session-a' });
    expect(member.liveboard_state_json).toHaveBeenCalledOnce();
    expect(member.find_recovery_claim).not.toHaveBeenCalled();
    expect(member.find_pending_incoming_recovery_claim).not.toHaveBeenCalled();
    return result;
  });
  const adapted = [state, extended].map((dto) => {
    const context = contextFixture(JSON.stringify(dto));
    context.ready = ready;
    const platform = platformFixture();
    const result = createCoreAdapter({ getContext: () => context, platform }).project();
    expect(context.member.liveboard_state_json).toHaveBeenCalledOnce();
    for (const effect of [...Object.values(context.manager), ...Object.values(platform)]) {
      if (vi.isMockFunction(effect)) expect(effect).not.toHaveBeenCalled();
    }
    return result;
  });
  expect(direct[1]).toEqual(direct[0]);
  expect(adapted[1]).toEqual(adapted[0]);
  for (const result of [...direct, ...adapted]) {
    for (const op of result.operations) expect(Object.hasOwn(op, 'revealContext')).toBe(false);
    const serialized = JSON.stringify(result);
    for (const marker of [
      'revealContext',
      'bodyCommitment',
      'vaultSource',
      'localDistributionSource',
      'recoverySources',
      'claimScopeCommitment',
      'compatibility-local-marker',
      'compatibility-remote-marker',
    ]) {
      expect(serialized).not.toContain(marker);
    }
  }
  return { direct: direct[1], adapted: adapted[1] };
}

describe('Liveboard additive Core reveal-context compatibility', () => {
  it('preserves exact own Accepted identity, provenance and Show permissions with or without context', () => {
    const { direct, adapted } = compareProjection(ownRecovery());
    expect(adapted).toEqual(direct);
    expect(adapted.session).toEqual({
      identity: 'session-a',
      vaultId: 'семья@example.test',
      member: true,
      ready: true,
    });
    expect(adapted.operations[0]).toMatchObject({
      kind: 'recover',
      requestId: 'claim-second',
      requesterId: 'self',
      secretId: 'pass-a',
      clientStatus: 'accepted',
      eligiblePeerIds: ['peer-a'],
      canApprove: false,
      canDecline: false,
      outcome: 'accepted',
      responderId: 'peer-a',
      resolutionSource: 'canonical',
      readiness: 'ready',
    });
    expect(adapted.secrets[0]).toMatchObject({
      passId: 'pass-a',
      primary: { kind: 'show-recovered', claimId: 'claim-second' },
      canDelete: true,
    });
    expect(adapted.requests).toEqual([]);
    expect(adapted.canCreateSecret).toBe(true);
    expect(adapted.devices.map(({ deviceId, current, canRemove }) => ({ deviceId, current, canRemove }))).toEqual([
      { deviceId: 'self', current: true, canRemove: false },
      { deviceId: 'peer-a', current: false, canRemove: true },
    ]);
  });

  it('keeps Done and Core Recover action without manufacturing Show or an incoming decision', () => {
    const { direct, adapted } = compareProjection(ownRecovery('done'));
    expect(adapted).toEqual(direct);
    expect(adapted.session.ready).toBe(true);
    expect(adapted.operations[0]).toMatchObject({
      requestId: 'claim-second',
      requesterId: 'self',
      secretId: 'pass-a',
      clientStatus: 'done',
      readiness: 'done',
      canApprove: false,
      canDecline: false,
    });
    expect(adapted.secrets[0].primary).toEqual({ kind: 'recover' });
    expect(adapted.requests).toEqual([]);
  });

  it('cannot override the current adapter readiness boundary or enable Create', () => {
    const { direct, adapted } = compareProjection(ownRecovery(), false);
    expect(direct.session.ready).toBe(true);
    expect(direct.canCreateSecret).toBe(true);
    expect(adapted.session.ready).toBe(false);
    expect(adapted.canCreateSecret).toBe(false);
    expect(adapted.operations).toEqual(direct.operations);
    expect(adapted.secrets).toEqual(direct.secrets);
  });

  it('preserves denied and mixed Core permissions without inventing requests', () => {
    const state = ownRecovery();
    state.canCreateSecret = false;
    state.removableDeviceIds = [];
    state.secrets[0].canDelete = false;
    state.operations.push(
      operation({ requestId: 'claim-first', canApprove: true, canDecline: false }),
      operation({ requestId: 'claim-decline-only', canApprove: false, canDecline: true }),
    );
    const { direct, adapted } = compareProjection(state);
    expect(adapted).toEqual(direct);
    expect(adapted.session.ready).toBe(true);
    expect(adapted.canCreateSecret).toBe(false);
    expect(adapted.secrets[0].canDelete).toBe(false);
    expect(adapted.devices.every((device) => !device.canRemove)).toBe(true);
    expect(adapted.requests.map(({ requestId, canApprove, canDecline }) => ({ requestId, canApprove, canDecline })))
      .toEqual([
        { requestId: 'claim-first', canApprove: true, canDecline: false },
        { requestId: 'claim-decline-only', canApprove: false, canDecline: true },
      ]);
  });

  it('still blocks a malformed known permission despite otherwise valid optional context', () => {
    const state = ownRecovery();
    state.operations[0] = operation({ requesterId: 'self', canApprove: 'not-a-boolean' });
    const { direct, adapted } = compareProjection(state);
    expect(adapted).toEqual(direct);
    expect(adapted.session).toEqual({ identity: 'session-a', vaultId: '', member: false, ready: false });
    expect(adapted.canCreateSecret).toBe(false);
    expect(adapted.operations).toEqual([]);
    expect(adapted.requests).toEqual([]);
    expect(adapted.secrets).toEqual([]);
    expect(adapted.devices).toEqual([]);
  });
});
