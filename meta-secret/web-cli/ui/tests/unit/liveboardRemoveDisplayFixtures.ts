import { computed, defineComponent, h, watch } from 'vue';
import { expect, vi } from 'vitest';
import { flushPromises, mount } from '@vue/test-utils';
import VaultTile from '@/components/liveboard/VaultTile.vue';
import type { CoreOperation } from '@/features/liveboard/coreTypes';
import { createCoreAdapter } from '@/features/liveboard/coreAdapter';
import { createLiveboardController } from '@/features/liveboard/controller';
import {
  contextFixture,
  coreState,
  memberFixture,
  platformFixture,
  producer,
  requestId,
  secondRequestId,
} from './liveboardCoreFixtures';

const dispose: Array<() => void> = [];
export function resetRemoveHarness() {
  vi.stubGlobal('matchMedia', () => ({ matches: false, addEventListener: vi.fn(), removeEventListener: vi.fn() }));
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (this: HTMLElement) {
    const id = this.closest('[data-device-id]')?.getAttribute('data-device-id');
    return this.classList.contains('device-art')
      ? new DOMRect(id === 'self' ? 40 : id === 'peer-a' ? 340 : 640, 0, 40, 80)
      : new DOMRect(0, 0, 900, 240);
  });
}
export function cleanupRemoveHarness() {
  dispose
    .splice(0)
    .reverse()
    .forEach((stop) => stop());
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
}
export function removal(patch: Partial<CoreOperation> = {}): CoreOperation {
  return {
    kind: 'removeDevice',
    requestId,
    requesterId: 'self',
    targetDeviceId: 'peer-a',
    clientStatus: 'pending',
    eligiblePeerIds: ['peer-b'],
    canApprove: false,
    canDecline: false,
    resolutionSource: 'unknown',
    readiness: 'waiting',
    ...patch,
  };
}
export function terminal(outcome: 'accepted' | 'declined', patch: Partial<CoreOperation> = {}) {
  return removal({
    clientStatus: 'done',
    readiness: 'done',
    outcome,
    responderId: 'peer-b',
    resolutionSource: 'verified',
    eligiblePeerIds: [],
    ...patch,
  });
}
export const identity = (id = requestId) => JSON.stringify(['removeDevice', id]);
export function receipt(next = false, known = true, decision = false) {
  return producer({
    kind: 'removeDevice',
    curr: next ? '18446744073709551613' : '18446744073709551615',
    request: known,
    decision,
  }).replace(requestId, next ? secondRequestId : requestId);
}
export async function setupRemove(options: { operations?: CoreOperation[]; members?: string[] } = {}) {
  const context = contextFixture(),
    platform = platformFixture();
  const original = context.vaultData.users();
  const third = memberFixture().vaultData.users()[1]!;
  vi.spyOn(third.user_data().device.device_id, 'wasm_id_str').mockReturnValue('peer-b');
  const users = [...original, third];
  const adapter = createCoreAdapter({ getContext: () => context, platform });
  const controller = createLiveboardController({ adapter, resetLocal: vi.fn(async () => {}), version: '2.0.0' });
  dispose.push(controller.dispose);
  const presentation = computed(() => controller.state.value.vault);
  const frames: Array<{ ready: boolean; request: typeof presentation.value.request }> = [];
  dispose.push(
    watch(
      presentation,
      (vault) => {
        frames.push({
          ready: controller.state.value.ready,
          request: vault.request ? { ...vault.request, peerIds: [...vault.request.peerIds] } : null,
        });
      },
      { flush: 'sync', immediate: true },
    ),
  );
  async function publish(rows: CoreOperation[], ready = true, members = ['self', 'peer-a', 'peer-b'], patch = {}) {
    context.ready = ready;
    vi.mocked(context.vaultData.users).mockReturnValue(
      users.filter((user) => members.includes(user.user_data().device.device_id.wasm_id_str())),
    );
    vi.mocked(context.member.liveboard_state_json).mockReturnValue(
      JSON.stringify(
        coreState({
          operations: rows,
          storageMode: members.length === 3 ? 'shares' : members.length === 2 ? 'copies' : 'whole',
          removableDeviceIds: members.filter((id) => id !== 'self'),
          ...patch,
        }),
      ),
    );
    const start = frames.length;
    const refreshing = controller.refresh();
    const synchronous = frames.slice(start);
    await refreshing;
    await flushPromises();
    return synchronous;
  }
  await publish(options.operations ?? [], true, options.members);
  const tile = mount(
    defineComponent({
      setup: () => () =>
        h(VaultTile, {
          vault: presentation.value,
          onSelect: controller.devices.preview,
          onCurrent: controller.devices.current,
          onRemove: controller.devices.remove,
        }),
    }),
  );
  dispose.push(() => tile.unmount());
  await flushPromises();
  async function submit(next = false, decision = false) {
    vi.mocked(context.manager.remove_member_checked).mockResolvedValue(receipt(next, false, decision));
    vi.mocked(context.manager.reconcile_producer_receipt).mockResolvedValue(receipt(next, true, decision));
    controller.devices.preview('peer-a');
    await controller.devices.remove();
  }
  return { context, platform, adapter, controller, tile, frames, publish, submit };
}
export type RemoveSetup = Awaited<ReturnType<typeof setupRemove>>;
export function noOtherWrites(s: RemoveSetup, removals: number) {
  expect(s.context.manager.remove_member_checked).toHaveBeenCalledTimes(removals);
  for (const method of [
    s.context.manager.continue_local_remove_checked,
    s.context.manager.remove_member_from_vault,
    s.context.manager.recover_checked,
    s.context.manager.show_recovered,
    s.context.manager.show_local_secret,
    s.context.manager.delete_secret_checked,
    s.context.manager.cluster_distribution_checked,
    s.context.manager.sync_now,
    s.context.manager.get_state,
  ])
    expect(method).not.toHaveBeenCalled();
}

export const invalidRemoveCurrent: Array<{ reason: string; rows: CoreOperation[]; members?: string[] }> = [
  { reason: 'old R1 terminal', rows: [terminal('declined')] },
  { reason: 'wrong UUID', rows: [terminal('accepted', { requestId: 'd732b6ac-0881-42ac-abd6-a5cba32b34ee' })] },
  { reason: 'wrong target', rows: [terminal('accepted', { requestId: secondRequestId, targetDeviceId: 'peer-b' })] },
  { reason: 'foreign requester', rows: [terminal('accepted', { requestId: secondRequestId, requesterId: 'peer-b' })] },
  {
    reason: 'different kind',
    rows: [
      terminal('accepted', {
        requestId: secondRequestId,
        kind: 'deleteSecret',
        secretId: 'pass-a',
        targetDeviceId: undefined,
      }),
    ],
  },
  { reason: 'missing responder', rows: [terminal('accepted', { requestId: secondRequestId, responderId: undefined })] },
  {
    reason: 'unknown provenance',
    rows: [terminal('accepted', { requestId: secondRequestId, resolutionSource: 'unknown' })],
  },
  {
    reason: 'incomplete terminal',
    rows: [terminal('accepted', { requestId: secondRequestId, clientStatus: 'accepted', readiness: 'synchronizing' })],
  },
  { reason: 'absent terminal', rows: [] },
  { reason: 'count2 alone, target still present', rows: [], members: ['self', 'peer-a'] },
  { reason: 'target gone alone', rows: [], members: ['self', 'peer-b'] },
];
