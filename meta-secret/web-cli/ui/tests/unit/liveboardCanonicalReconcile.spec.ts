import { describe, expect, it, vi } from 'vitest';
import type { MetaPasswordId } from 'meta-secret-web-cli';
import { createCoreAdapter } from '@/features/liveboard/coreAdapter';
import { createLiveboardController } from '@/features/liveboard/controller';
import {
  checked,
  contextFixture,
  coreState,
  operation,
  pass as fixturePass,
  platformFixture,
  producer,
  requestId,
} from './liveboardCoreFixtures';

vi.mock('meta-secret-web-cli', () => {
  const lifetime = () => ({ free: vi.fn(), [Symbol.dispose]: vi.fn() });
  return {
    PlainPassInfo: class {
      free = vi.fn();
      [Symbol.dispose] = vi.fn();
      pass_id: MetaPasswordId;
      constructor(
        name: string,
        public pass: string,
      ) {
        this.pass_id = {
          ...lifetime(),
          id_str: vi.fn(() => 'pass-new'),
          name,
          id: { ...lifetime(), text: lifetime() },
        };
      }
    },
  };
});
function setup() {
  const context = contextFixture(JSON.stringify(coreState({ operations: [] })));
  const platform = platformFixture();
  const adapter = createCoreAdapter({ getContext: () => context, platform });
  const controller = createLiveboardController({ adapter, resetLocal: vi.fn(async () => {}), version: '2.0.0' });
  controller.refresh();
  const publish = (patch: Parameters<typeof coreState>[0]) => {
    vi.mocked(context.member.liveboard_state_json).mockReturnValue(JSON.stringify(coreState(patch)));
    return controller.refresh();
  };
  return { context, platform, adapter, controller, publish };
}

describe('Liveboard canonical readiness and exact operation correlation', () => {
  it.each([12, 24] as const)(
    'sends the existing %s-word seed format through the real checked Create port',
    async (wordCount) => {
      const s = setup();
      const words = Array.from({ length: wordCount }, (_, index) => `word${index}`);
      s.controller.create.open();
      s.controller.create.change({ mode: 'seed', name: ' Seed ', wordCount, words });
      await s.controller.create.submit();
      const input = vi.mocked(s.context.manager.cluster_distribution_checked).mock.calls[0]![0];
      expect(input.pass).toBe(words.join(' '));
      expect(input.pass_id.name).toBe('Seed');
      expect(input.free).toHaveBeenCalledOnce();
      expect(s.platform.authenticate).not.toHaveBeenCalled();
      s.controller.dispose();
    },
  );

  it('never resubmits a real checked Unknown Create on navigation or repeated submit', async () => {
    const s = setup();
    vi.mocked(s.context.manager.cluster_distribution_checked).mockResolvedValue(
      checked({ kind: 'create', secretId: 'pass-new', claimId: undefined, phase: 'unknown' }),
    );
    s.controller.create.open();
    s.controller.create.change({ name: 'New name', value: 'private draft' });
    await s.controller.create.submit();
    s.controller.navigate('profile');
    s.controller.navigate('home');
    await s.controller.create.submit();
    expect(s.context.manager.cluster_distribution_checked).toHaveBeenCalledOnce();
    expect(s.context.manager.cluster_distribution).not.toHaveBeenCalled();
    s.controller.dispose();
  });
  it('closes Create only after exact receipt ID appears with Core-ready material, not count/name or queued receipt', async () => {
    const s = setup();
    vi.mocked(s.context.manager.cluster_distribution_checked).mockResolvedValue(
      checked({ kind: 'create', secretId: 'pass-new', claimId: undefined }),
    );
    s.controller.create.open();
    s.controller.create.change({ name: ' Same secret name ', value: ' value ' });
    await s.controller.create.submit();
    expect(s.controller.create.state.value.open).toBe(true);
    await s.publish({
      secrets: [
        { secretId: 'pass-a', primaryAction: 'showLocal', canDelete: true, readiness: 'ready' },
        { secretId: 'pass-b', primaryAction: 'showLocal', canDelete: true, readiness: 'ready' },
      ],
      operations: [],
    });
    expect(s.controller.create.state.value.open).toBe(true);
    vi.mocked(s.context.vaultData.secrets).mockImplementation(() => [fixturePass('pass-new')]);
    await s.publish({
      secrets: [{ secretId: 'pass-new', primaryAction: 'unavailable', canDelete: false, readiness: 'synchronizing' }],
      operations: [],
    });
    expect(s.controller.create.state.value.open).toBe(true);
    await s.publish({
      secrets: [{ secretId: 'pass-new', primaryAction: 'showLocal', canDelete: true, readiness: 'ready' }],
      operations: [],
    });
    await Promise.resolve();
    expect(s.controller.create.state.value.open).toBe(false);
    expect(s.context.manager.cluster_distribution_checked).toHaveBeenCalledOnce();
    expect(s.context.manager.cluster_distribution).not.toHaveBeenCalled();
    s.controller.dispose();
  });

  it('observes the SAME actual R/U, retains late UUID correlation and continues local removal only explicitly', async () => {
    const s = setup();
    const expectedReference = {
      version: 1,
      kind: 'removeDevice',
      requestStage: {
        eventId: {
          fqdn: { objType: 'DeviceLog', objInstance: 'self' },
          id: { curr: '18446744073709551615', prev: '18446744073709551614' },
        },
        bodyCommitment: 'a'.repeat(64),
      },
      decisionStage: {
        eventId: {
          fqdn: { objType: 'DeviceLog', objInstance: 'self' },
          id: { curr: '9007199254740993', prev: '9007199254740992' },
        },
        bodyCommitment: 'b'.repeat(64),
      },
    };
    const losslessAssertionView = (wire: string) => {
      let converted = 0;
      const normalized = wire.replace(/"(?:curr|prev)"\s*:\s*\d+(?=\s*[,}])/gu, (token) => {
        converted++;
        return token.replace(/\d+$/u, '"$&"');
      });
      expect(converted).toBe(4);
      return JSON.parse(normalized);
    };
    vi.mocked(s.context.manager.remove_member_checked).mockResolvedValue(
      producer({ kind: 'removeDevice', decision: true, request: false }),
    );
    vi.mocked(s.context.manager.reconcile_producer_receipt).mockResolvedValue(
      producer({ kind: 'removeDevice', decision: true }),
    );
    expect(await s.adapter.devices.authenticate('peer-a')).toBe(true);
    const receipt = await s.adapter.devices.remove('peer-a');
    expect(receipt.phase).toBe('unknown');
    expect(receipt.operationId).not.toBe('peer-a');
    await s.adapter.reconcileProducer(receipt.operationId);
    const reference = vi.mocked(s.context.manager.reconcile_producer_receipt).mock.calls[0]![0];
    expect(losslessAssertionView(reference)).toEqual(expectedReference);
    expect(reference).toContain('"curr":18446744073709551615');
    expect(reference).toContain('"curr":9007199254740993');
    expect(reference).not.toContain('observation');
    expect(s.context.manager.continue_local_remove_checked).not.toHaveBeenCalled();
    expect(s.context.manager.remove_member_checked).toHaveBeenCalledOnce();
    expect(s.context.manager.sync_now).not.toHaveBeenCalled();
    expect(s.context.manager.get_state).not.toHaveBeenCalled();
    await s.adapter.continueRemove(receipt.operationId);
    const continued = vi.mocked(s.context.manager.continue_local_remove_checked).mock.calls[0]![0];
    expect(losslessAssertionView(continued)).toEqual({ ...expectedReference, requestId });
    expect(continued).toContain(`"requestId":"${requestId}"`);
    expect(continued).toContain('"curr":18446744073709551615');
    expect(continued).toContain('"curr":9007199254740993');
    expect(s.context.manager.continue_local_remove_checked).toHaveBeenCalledOnce();
    expect(s.context.manager.remove_member_from_vault).not.toHaveBeenCalled();
    s.controller.dispose();
  });

  it('keeps Current/Remove blocked during accepted resharing, then clears preview and clocks on actual finalized readiness', async () => {
    const s = setup();
    vi.mocked(s.context.manager.reconcile_producer_receipt).mockResolvedValue(producer({ kind: 'removeDevice' }));
    s.controller.devices.preview('peer-a');
    await s.controller.devices.remove();
    await s.publish({
      operations: [
        operation({
          kind: 'removeDevice',
          requestId,
          targetDeviceId: 'peer-a',
          requesterId: 'self',
          outcome: 'accepted',
          clientStatus: 'synchronizing',
          readiness: 'synchronizing',
          responderId: 'peer-b',
          resolutionSource: 'verified',
        }),
      ],
    });
    expect(s.controller.devices.state.value.busy).toBe(true);
    expect(s.controller.devices.state.value.operation?.phase).toBe('settling');
    s.controller.devices.current();
    expect(s.controller.devices.state.value.preview).not.toBeNull();
    const self = s.context.vaultData.users()[0]!;
    vi.mocked(s.context.vaultData.users).mockReturnValue([self]);
    const freshMembers = vi.mocked(s.context.vaultData.members).getMockImplementation()!;
    vi.mocked(s.context.vaultData.members).mockImplementation(() =>
      freshMembers().filter((member) => member.user_data.device.device_id.wasm_id_str() === 'self'),
    );
    await s.publish({
      removableDeviceIds: [],
      operations: [
        operation({
          kind: 'removeDevice',
          requestId,
          targetDeviceId: 'peer-a',
          requesterId: 'self',
          outcome: 'accepted',
          clientStatus: 'done',
          readiness: 'done',
          responderId: 'peer-b',
          resolutionSource: 'verified',
        }),
      ],
    });
    expect(s.controller.devices.state.value.preview).toBeNull();
    expect(s.controller.devices.state.value.operation).toBeNull();
    expect(s.controller.devices.state.value.busy).toBe(false);
    expect(s.controller.state.value.vault.storage.removable).toBe(false);
    s.controller.dispose();
  });

  it('deletes in place with exact tombstone and preserves query/scroll, then permits only a canonical declined retry', async () => {
    const s = setup();
    s.controller.navigate('secrets');
    s.controller.search('Same');
    s.controller.scroll(137);
    await s.controller.secrets.remove('pass-a');
    await s.publish({
      operations: [
        operation({
          kind: 'deleteSecret',
          requestId,
          requesterId: 'self',
          outcome: 'declined',
          clientStatus: 'declined',
          readiness: 'settled',
        }),
      ],
    });
    expect(s.controller.secrets.state.value.cards['pass-a'].phase).toBe('masked');
    await s.controller.secrets.remove('pass-a');
    expect(s.context.manager.delete_secret_checked).toHaveBeenCalledTimes(2);
    await s.publish({
      secrets: [],
      operations: [
        operation({
          kind: 'deleteSecret',
          requestId,
          requesterId: 'self',
          outcome: 'accepted',
          clientStatus: 'done',
          readiness: 'settled',
          responderId: 'self',
          resolutionSource: 'canonical',
        }),
      ],
    });
    expect(s.controller.secrets.state.value.cards['pass-a']).toBeUndefined();
    expect(s.controller.secrets.state.value.presentation).toMatchObject({
      active: true,
      query: 'Same',
      scrollTop: 137,
    });
    s.controller.dispose();
  });

  it('consumes green unread only after deliberate visible Show for the exact accepted Claim', async () => {
    const s = setup();
    await s.publish({
      secrets: [
        {
          secretId: 'pass-a',
          primaryAction: 'showRecovered',
          recoveryClaimId: 'claim-second',
          canDelete: true,
          readiness: 'ready',
        },
      ],
      operations: [
        operation({
          requestId: 'claim-second',
          requesterId: 'self',
          clientStatus: 'accepted',
          outcome: 'accepted',
          responderId: 'peer-a',
          resolutionSource: 'verified',
          readiness: 'ready',
        }),
      ],
    });
    expect(s.controller.state.value.unread['claim-second']).toBe('green');
    s.controller.navigate('profile');
    await s.controller.secrets.primary('pass-a');
    expect(s.context.manager.show_recovered).not.toHaveBeenCalled();
    expect(s.controller.state.value.unread['claim-second']).toBe('green');
    s.controller.navigate('secrets');
    await s.controller.secrets.primary('pass-a');
    expect(s.context.manager.show_recovered).toHaveBeenCalledOnce();
    expect(s.controller.state.value.unread['claim-second']).toBeUndefined();
    s.controller.dispose();
  });

  it('retains a canonical declined unread indicator across navigation until the permitted next explicit attempt', async () => {
    const s = setup();
    await s.publish({
      secrets: [{ secretId: 'pass-a', primaryAction: 'recover', canDelete: true, readiness: 'ready' }],
      operations: [
        operation({
          requestId: 'claim-second',
          requesterId: 'self',
          clientStatus: 'declined',
          outcome: 'declined',
          readiness: 'ready',
          resolutionSource: 'verified',
          responderId: 'peer-a',
        }),
      ],
    });
    s.controller.navigate('profile');
    s.controller.navigate('secrets');
    expect(s.controller.state.value.unread['claim-second']).toBe('red');
    await s.controller.secrets.primary('pass-a');
    expect(s.context.manager.recover_checked).toHaveBeenCalledOnce();
    expect(s.controller.state.value.unread['claim-second']).toBeUndefined();
    s.controller.dispose();
  });
});
