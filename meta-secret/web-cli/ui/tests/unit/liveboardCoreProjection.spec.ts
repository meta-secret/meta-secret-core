import { describe, expect, it, vi } from 'vitest';
import { readCoreProjection } from '@/features/liveboard/coreProjection';
import { coreState, memberFixture, operation, pass } from './liveboardCoreFixtures';

describe('Liveboard actual Core snapshot projection', () => {
  it.each([
    'null',
    '{}',
    '{"schemaVersion":2}',
    '{broken-private-marker',
    JSON.stringify(coreState({ storageMode: 'invented-policy' })),
    JSON.stringify(coreState({ currentDeviceId: '' })),
    JSON.stringify(coreState({ canCreateSecret: 'true' })),
    JSON.stringify(
      coreState({
        secrets: [{ secretId: 'pass-a', primaryAction: 'guessed-show', readiness: 'ready', canDelete: true }],
      }),
    ),
  ])('blocks unsupported/malformed metadata with no legacy eligibility fallback: %s', (json) => {
    const { member } = memberFixture(json);
    const projected = readCoreProjection({ member, identity: 'session-a' });
    expect(projected.session.ready).toBe(false);
    expect(projected.canCreateSecret).toBe(false);
    expect(projected.requests).toEqual([]);
    expect(
      projected.secrets.every(
        (item: { primary: { kind: string }; canDelete: boolean }) => item.primary.kind === 'blocked' && !item.canDelete,
      ),
    ).toBe(true);
    expect(JSON.stringify(projected)).not.toContain('broken-private-marker');
    expect(member.find_recovery_claim).not.toHaveBeenCalled();
    expect(member.find_pending_incoming_recovery_claim).not.toHaveBeenCalled();
  });

  it('fails closed when the installed member lacks the required export', () => {
    const { member } = memberFixture();
    Object.defineProperty(member, 'liveboard_state_json', { value: undefined });
    expect(readCoreProjection({ member, identity: 'session-a' }).session.ready).toBe(false);
  });

  it('joins escaped display metadata from the SAME member while retaining exact same-name identities', () => {
    const { member, vaultData } = memberFixture(
      JSON.stringify(
        coreState({
          secrets: [
            { secretId: 'pass-a', primaryAction: 'showLocal', canDelete: false, readiness: 'ready' },
            {
              secretId: 'pass-b',
              primaryAction: 'showRecovered',
              recoveryClaimId: 'claim-second',
              canDelete: true,
              readiness: 'ready',
            },
          ],
          operations: [operation({ requestId: 'claim-first' }), operation()],
        }),
      ),
    );
    vi.mocked(vaultData.secrets).mockReturnValue([
      pass('pass-a', '<img onerror=private-marker>'),
      pass('pass-b', '<img onerror=private-marker>'),
    ]);
    const projected = readCoreProjection({ member, identity: 'session-a' });
    expect(member.liveboard_state_json).toHaveBeenCalledOnce();
    expect(member.vault_data).toHaveBeenCalledOnce();
    expect(projected.session).toMatchObject({ identity: 'session-a', member: true, ready: true });
    expect(projected.secrets.map((item: { passId: string }) => item.passId)).toEqual(['pass-a', 'pass-b']);
    expect(projected.secrets[0]).toMatchObject({
      name: '<img onerror=private-marker>',
      canDelete: false,
      primary: { kind: 'show-local' },
    });
    expect(projected.secrets[1].primary).toEqual({ kind: 'show-recovered', claimId: 'claim-second' });
    expect(projected.requests.map((item: { requestId: string }) => item.requestId)).toEqual([
      'claim-first',
      'claim-second',
    ]);
    expect(projected.devices.find((item: { deviceId: string }) => item.deviceId === 'peer-a')).toMatchObject({
      canRemove: true,
    });
    expect(projected.devices.find((item: { deviceId: string }) => item.deviceId === 'self')).toMatchObject({
      current: true,
      canRemove: false,
    });
  });

  it.each([
    ['showLocal', 'ready', undefined, { kind: 'show-local' }],
    ['showRecovered', 'ready', 'claim-second', { kind: 'show-recovered', claimId: 'claim-second' }],
    ['recover', 'ready', undefined, { kind: 'recover' }],
    ['waiting', 'waiting', 'claim-second', { kind: 'waiting', claimId: 'claim-second' }],
    ['showLocal', 'synchronizing', undefined, { kind: 'blocked' }],
    ['unavailable', 'synchronizing', undefined, { kind: 'blocked' }],
  ])(
    'uses Core instruction %s/readiness %s without member-count inference',
    (primaryAction, readiness, recoveryClaimId, expected) => {
      const { member } = memberFixture(
        JSON.stringify(
          coreState({
            storageMode: 'shares',
            secrets: [{ secretId: 'pass-a', primaryAction, readiness, recoveryClaimId, canDelete: false }],
          }),
        ),
      );
      expect(readCoreProjection({ member, identity: 'session-a' }).secrets[0].primary).toEqual(expected);
    },
  );

  it('preserves canonical responder provenance and never fabricates a peer for log-only retirement', () => {
    const { member } = memberFixture(
      JSON.stringify(
        coreState({
          operations: [
            operation({
              kind: 'deleteSecret',
              requestId: 'delete-a',
              outcome: 'declined',
              clientStatus: 'declined',
              readiness: 'settled',
            }),
            operation({
              kind: 'deleteSecret',
              requestId: 'delete-b',
              outcome: 'accepted',
              responderId: 'self',
              resolutionSource: 'canonical',
              readiness: 'settled',
            }),
          ],
        }),
      ),
    );
    const projected = readCoreProjection({ member, identity: 'session-a' });
    expect(projected.operations[0]).toMatchObject({ requestId: 'delete-a', resolutionSource: 'unknown' });
    expect(projected.operations[0].responderId).toBeUndefined();
    expect(projected.operations[1]).toMatchObject({
      requestId: 'delete-b',
      responderId: 'self',
      resolutionSource: 'canonical',
    });
  });

  it('never retains unrecognized plaintext/key/ciphertext fields from a JSON payload', () => {
    const { member } = memberFixture(
      JSON.stringify(coreState({ secret: 'private-secret', masterKey: 'private-key', ciphertext: 'private-cipher' })),
    );
    const projected = JSON.stringify(readCoreProjection({ member, identity: 'session-a' }));
    for (const marker of ['private-secret', 'private-key', 'private-cipher']) expect(projected).not.toContain(marker);
  });
});
