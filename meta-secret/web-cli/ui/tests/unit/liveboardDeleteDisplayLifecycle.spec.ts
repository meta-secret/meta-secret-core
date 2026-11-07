import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushPromises, mount } from '@vue/test-utils';
import { defineComponent, h } from 'vue';
import VaultTile from '@/components/liveboard/VaultTile.vue';
import { createCoreAdapter } from '@/features/liveboard/coreAdapter';
import { createLiveboardController } from '@/features/liveboard/controller';
import type { CoreOperation } from '@/features/liveboard/coreTypes';
import {
  contextFixture,
  coreState,
  deferred,
  pass,
  platformFixture,
  producer,
  requestId,
  secondRequestId,
} from './liveboardCoreFixtures';

type Material = {
  secretId: string;
  primaryAction: string;
  canDelete: boolean;
  readiness: string;
  recoveryClaimId?: string;
};
const material = (ids = ['pass-a']): Material[] =>
  ids.map((secretId) => ({ secretId, primaryAction: 'showLocal', canDelete: true, readiness: 'ready' }));
const disposals: Array<() => void> = [];
beforeEach(() =>
  vi.stubGlobal('matchMedia', () => ({ matches: true, addEventListener: vi.fn(), removeEventListener: vi.fn() })),
);
afterEach(() => {
  disposals.splice(0).forEach((dispose) => dispose());
  vi.unstubAllGlobals();
});

function declined(patch: Partial<CoreOperation> = {}): CoreOperation {
  return {
    kind: 'deleteSecret',
    requestId,
    secretId: 'pass-a',
    requesterId: 'self',
    clientStatus: 'declined',
    outcome: 'declined',
    eligiblePeerIds: ['peer-a'],
    canApprove: false,
    canDecline: false,
    responderId: 'peer-a',
    resolutionSource: 'canonical',
    readiness: 'settled',
    ...patch,
  };
}
const identity = (operation: CoreOperation) => JSON.stringify([operation.kind, operation.requestId]);
const receipt = (next = false) =>
  producer({ curr: next ? '18446744073709551613' : '18446744073709551615' }).replace(
    `"requestId":"${requestId}"`,
    `"requestId":"${next ? secondRequestId : requestId}"`,
  );

async function setup(operations: CoreOperation[] = [], secrets = material()) {
  const context = contextFixture();
  const platform = platformFixture();
  const adapter = createCoreAdapter({ getContext: () => context, platform });
  const controller = createLiveboardController({ adapter, resetLocal: vi.fn(async () => {}), version: '2.0.0' });
  disposals.push(() => controller.dispose());
  async function publish(rows: CoreOperation[], values = material(), patch = {}) {
    vi.mocked(context.vaultData.secrets).mockImplementation(() => values.map((item) => pass(item.secretId)));
    vi.mocked(context.member.liveboard_state_json).mockReturnValue(
      JSON.stringify(
        coreState({
          operations: rows,
          secrets: values,
          ...patch,
        }),
      ),
    );
    await controller.refresh();
  }
  await publish(operations, secrets);
  controller.navigate('secrets');
  return { context, platform, adapter, controller, publish };
}
type Setup = Awaited<ReturnType<typeof setup>>;
async function establishDecline(s: Setup) {
  vi.mocked(s.context.manager.delete_secret_checked).mockResolvedValue(receipt());
  vi.mocked(s.context.manager.reconcile_producer_receipt).mockResolvedValue(receipt());
  await s.controller.secrets.remove('pass-a');
  await s.publish([declined()]);
  expect(s.controller.secrets.state.value.cards['pass-a'].phase).toBe('masked');
  expect(s.controller.state.value.vault.request).toMatchObject({ identity: identity(declined()), phase: 'declined' });
}
function noOtherWrites(s: Setup) {
  expect(s.context.manager.remove_member_checked).not.toHaveBeenCalled();
  expect(s.context.manager.continue_local_remove_checked).not.toHaveBeenCalled();
  expect(s.context.manager.delete_secret).not.toHaveBeenCalled();
  expect(s.context.manager.recover_checked).not.toHaveBeenCalled();
  expect(s.context.manager.cluster_distribution_checked).not.toHaveBeenCalled();
  expect(s.context.manager.sync_now).not.toHaveBeenCalled();
}

describe('Delete history display through the actual controller and typed adapter', () => {
  it('retires d1 on new Unknown admission before UUID observation, keeps d2 locked, then restores real preview controls', async () => {
    const s = await setup();
    await establishDecline(s);
    const tile = mount(
      defineComponent({
        setup: () => () =>
          h(VaultTile, {
            vault: s.controller.state.value.vault,
            onSelect: s.controller.devices.preview,
          }),
      }),
    );
    disposals.push(() => tile.unmount());
    expect(tile.get('.vault-recovery').text()).toContain('Request declined.');
    const observation = deferred<string>();
    vi.mocked(s.context.manager.delete_secret_checked).mockResolvedValue(receipt(true));
    vi.mocked(s.context.manager.reconcile_producer_receipt).mockReturnValue(observation.promise);
    const admitted = vi.spyOn(s.adapter.secrets, 'deleteSecret');
    const retry = s.controller.secrets.remove('pass-a');
    await flushPromises();
    expect(s.context.manager.delete_secret_checked).toHaveBeenCalledTimes(2);
    expect(await admitted.mock.results[0]!.value).toEqual({
      operationId: 'DeviceLog:self::18446744073709551613',
      phase: 'unknown',
    });
    // The public receipt has no UUID; its read-only observation is still pending.
    expect.soft(s.controller.state.value.vault.request?.identity).not.toBe(identity(declined()));
    expect.soft(tile.find('.vault-recovery').exists()).toBe(false);
    expect(s.controller.secrets.state.value.cards['pass-a'].phase).toBe('deleting');
    await s.controller.secrets.remove('pass-a');
    expect(s.context.manager.delete_secret_checked).toHaveBeenCalledTimes(2);
    observation.reject(new Error('ObservationUnavailable'));
    await retry;
    await s.publish([declined()]);
    await s.controller.secrets.remove('pass-a');
    expect(s.controller.secrets.state.value.cards['pass-a'].phase).toBe('deleting');
    expect(s.context.manager.delete_secret_checked).toHaveBeenCalledTimes(2);

    vi.mocked(s.context.manager.reconcile_producer_receipt).mockResolvedValue(receipt(true));
    await s.controller.refresh();
    const done = declined({ requestId: secondRequestId, clientStatus: 'done', outcome: 'accepted' });
    const history = [declined(), done];
    await s.publish(history, []);
    await s.controller.refresh();
    expect(s.adapter.project().operations).toEqual(history);
    expect(s.controller.state.value.profile.secretCount).toBe(0);
    expect(s.controller.secrets.state.value.cards['pass-a']).toBeUndefined();
    s.controller.navigate('home');
    await tile.get('[data-device-id="peer-a"]').trigger('click');
    await flushPromises();
    expect(s.controller.state.value.vault.storage).toMatchObject({ preview: true, removable: true, busy: false });
    expect.soft(s.controller.state.value.vault.request).toBeNull();
    expect.soft(tile.find('.vault-recovery').exists()).toBe(false);
    expect.soft(tile.find('[data-testid="remove-device"]').exists()).toBe(true);
    expect(s.context.manager.delete_secret_checked).toHaveBeenCalledTimes(2);
    noOtherWrites(s);
    await s.publish(history, [], { removableDeviceIds: [] });
    expect(tile.find('[data-testid="remove-device"]').exists()).toBe(false);
    noOtherWrites(s);
  });

  it.each(['auth-cancel', 'missing-capability', 'rejected-port'] as const)(
    '%s never acknowledges the old terminal display',
    async (reason) => {
      const old = declined();
      const s = await setup([old]);
      const sdk = s.context.manager.delete_secret_checked;
      const observing = vi.spyOn(s.adapter, 'reconcileProducer');
      if (reason === 'auth-cancel') s.platform.authenticate.mockResolvedValue(false);
      if (reason === 'missing-capability')
        Object.defineProperty(s.context.manager, 'delete_secret_checked', { value: undefined });
      if (reason === 'rejected-port')
        vi.spyOn(s.adapter.secrets, 'deleteSecret').mockResolvedValue({
          operationId: 'not-admitted',
          phase: 'rejected',
        });
      await s.controller.secrets.remove('pass-a');
      expect(s.controller.state.value.vault.request?.identity).toBe(identity(old));
      expect(s.adapter.project().operations).toEqual([old]);
      expect(sdk).not.toHaveBeenCalled();
      expect(observing).not.toHaveBeenCalled();
      noOtherWrites(s);
    },
  );

  it('does not acknowledge a later terminal that was not captured before the intent', async () => {
    const pending = declined({
      outcome: undefined,
      responderId: undefined,
      clientStatus: 'pending',
      resolutionSource: 'unknown',
      readiness: 'waiting',
    });
    const s = await setup([pending]);
    const sending = deferred<string>();
    vi.mocked(s.context.manager.delete_secret_checked).mockReturnValue(sending.promise);
    vi.mocked(s.context.manager.reconcile_producer_receipt).mockRejectedValue(new Error('ObservationUnavailable'));
    const retry = s.controller.secrets.remove('pass-a');
    await flushPromises();
    expect(s.context.manager.delete_secret_checked).toHaveBeenCalledOnce();
    await s.publish([declined()]);
    sending.resolve(receipt(true));
    await retry;
    await s.controller.refresh();
    expect(s.controller.state.value.vault.request?.identity).toBe(identity(declined()));
    expect(s.controller.secrets.state.value.cards['pass-a'].phase).toBe('deleting');
    await s.controller.secrets.remove('pass-a');
    expect(s.context.manager.delete_secret_checked).toHaveBeenCalledOnce();
    noOtherWrites(s);
  });

  it('never treats a settled old producer ID as a new admission on same-Pass recreation', async () => {
    const s = await setup();
    await s.controller.secrets.remove('pass-a');
    const done = declined({ clientStatus: 'done', outcome: 'accepted' });
    await s.publish([done], []);
    const later = declined({ requestId: secondRequestId });
    await s.publish([done, later]);
    expect(s.controller.state.value.vault.request?.identity).toBe(identity(later));
    // SDK returns the already admitted exact R source, after its active entry settled.
    vi.mocked(s.context.manager.delete_secret_checked).mockResolvedValue(receipt());
    vi.mocked(s.context.manager.reconcile_producer_receipt).mockResolvedValue(receipt());
    await s.controller.secrets.remove('pass-a');
    await s.controller.refresh();
    await s.controller.refresh();
    expect(s.controller.state.value.vault.request?.identity).toBe(identity(later));
    expect(s.adapter.project().operations).toEqual([done, later]);
    expect(s.controller.secrets.state.value.cards['pass-a'].phase).toBe('deleting');
    await s.controller.secrets.remove('pass-a');
    expect(s.context.manager.delete_secret_checked).toHaveBeenCalledTimes(2);
    noOtherWrites(s);
  });

  it('reobservation of the same Unknown cannot acknowledge a newly learned terminal', async () => {
    const s = await setup([declined()]);
    vi.mocked(s.context.manager.delete_secret_checked).mockResolvedValue(receipt(true));
    vi.mocked(s.context.manager.reconcile_producer_receipt).mockRejectedValue(new Error('ObservationUnavailable'));
    await s.controller.secrets.remove('pass-a');
    const later = declined({ requestId: 'later-delete' });
    await s.publish([later]);
    await s.controller.refresh();
    await s.controller.refresh();
    expect(s.controller.state.value.vault.request?.identity).toBe(identity(later));
    expect(s.controller.secrets.state.value.cards['pass-a'].phase).toBe('deleting');
    await s.controller.secrets.remove('pass-a');
    expect(s.context.manager.delete_secret_checked).toHaveBeenCalledOnce();
    noOtherWrites(s);
  });

  it.each(['other-subject', 'other-kind', 'active'] as const)(
    'preserves %s metadata on a new Delete admission',
    async (reason) => {
      const old = declined(
        reason === 'other-subject'
          ? { secretId: 'pass-b' }
          : reason === 'other-kind'
            ? { kind: 'removeDevice', secretId: undefined, targetDeviceId: 'peer-a' }
            : {
                clientStatus: 'pending',
                outcome: undefined,
                responderId: undefined,
                resolutionSource: 'unknown',
                readiness: 'waiting',
              },
      );
      const s = await setup([old], material(['pass-a', 'pass-b']));
      vi.mocked(s.context.manager.delete_secret_checked).mockResolvedValue(receipt(true));
      vi.mocked(s.context.manager.reconcile_producer_receipt).mockRejectedValue(new Error('ObservationUnavailable'));
      await s.controller.secrets.remove('pass-a');
      await s.controller.refresh();
      expect(s.controller.state.value.vault.request?.identity).toBe(identity(old));
      expect(s.adapter.project().operations).toEqual([old]);
      expect(s.context.manager.delete_secret_checked).toHaveBeenCalledOnce();
      noOtherWrites(s);
    },
  );

  it('preserves another canonical requester history rather than acknowledging it for this actor', async () => {
    const foreign = declined({ requesterId: 'peer-a', responderId: 'self', eligiblePeerIds: ['self'] });
    const s = await setup([foreign]);
    vi.mocked(s.context.manager.delete_secret_checked).mockResolvedValue(receipt(true));
    vi.mocked(s.context.manager.reconcile_producer_receipt).mockRejectedValue(new Error('ObservationUnavailable'));
    await s.controller.secrets.remove('pass-a');
    expect(s.adapter.project().operations).toEqual([foreign]);
    expect(s.controller.state.value.vault.request).toBeNull();
    // Read-only canonical actor projection exposes whether foreign display was consumed.
    await s.publish([foreign], material(), { currentDeviceId: 'peer-a' });
    expect(s.controller.state.value.vault.request?.identity).toBe(identity(foreign));
    expect(s.context.manager.delete_secret_checked).toHaveBeenCalledOnce();
    noOtherWrites(s);
  });

  it.each(['accepted', 'declined'] as const)(
    'preserves Recover %s visual and unread during Delete admission',
    async (outcome) => {
      const recovery = declined({
        kind: 'recover',
        requestId: 'claim-second',
        secretId: 'pass-b',
        outcome,
        clientStatus: outcome,
        resolutionSource: 'verified',
        readiness: outcome === 'accepted' ? 'ready' : 'waiting',
      });
      const values = material(['pass-a', 'pass-b']);
      Object.assign(values[1]!, {
        primaryAction: outcome === 'accepted' ? 'showRecovered' : 'recover',
        recoveryClaimId: 'claim-second',
      });
      const s = await setup([recovery], values);
      const color = outcome === 'accepted' ? 'green' : 'red';
      expect(s.controller.state.value.unread['claim-second']).toBe(color);
      vi.mocked(s.context.manager.delete_secret_checked).mockResolvedValue(receipt(true));
      vi.mocked(s.context.manager.reconcile_producer_receipt).mockRejectedValue(new Error('ObservationUnavailable'));
      await s.controller.secrets.remove('pass-a');
      await s.controller.refresh();
      expect(s.controller.state.value.vault.request?.identity).toBe(identity(recovery));
      expect(s.controller.state.value.unread['claim-second']).toBe(color);
      expect(s.adapter.project().operations).toEqual([recovery]);
      expect(s.context.manager.show_recovered).not.toHaveBeenCalled();
      expect(s.context.manager.show_local_secret).not.toHaveBeenCalled();
      noOtherWrites(s);
    },
  );
});
