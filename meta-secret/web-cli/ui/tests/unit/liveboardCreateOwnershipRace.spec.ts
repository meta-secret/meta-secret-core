import { describe, expect, it, vi } from 'vitest';
import { watch } from 'vue';
import { createLiveboardController } from '@/features/liveboard/controller';
import { createControllerCreate } from '@/features/liveboard/controllerCreate';
import type { CoreAdapter, CoreProjection } from '@/features/liveboard/coreTypes';
import type { CreateDraft, CreatePort, CreateReceipt } from '@/features/liveboard/types';
import { createPort, deferred, devicePort, notificationPort, secretPort, session } from './liveboardFixtures';

const input = (name: string): CreateDraft => ({
  mode: 'secret',
  name,
  value: 'synthetic typed-port value',
  wordCount: 12,
  words: Array(24).fill(''),
});
async function parent() {
  let projection: CoreProjection = {
    session: session(),
    vaultName: 'vault-a',
    storageMode: 'whole',
    canCreateSecret: true,
    secrets: [],
    requests: [],
    operations: [],
    devices: [],
  };
  const create = createPort();
  const adapter: CoreAdapter = {
    project: vi.fn(() => projection),
    create,
    notifications: notificationPort(),
    secrets: secretPort(),
    devices: devicePort(),
    reconcileProducer: vi.fn(async () => {
      throw new Error('No producer observation expected');
    }),
    continueRemove: vi.fn(async () => {
      throw new Error('No continuation expected');
    }),
  };
  const controller = createLiveboardController({ adapter, resetLocal: vi.fn(async () => {}), version: '2.0.0' });
  await controller.refresh();
  controller.create.open();
  controller.create.change(input('Initial valid form'));
  expect(controller.create.state.value.draft.name).toBe('Initial valid form');
  return {
    controller,
    create,
    adapter,
    async publish(next: Partial<CoreProjection>) {
      projection = { ...projection, ...next };
      await controller.refresh();
    },
  };
}

describe('Create parent publication and original write ownership', () => {
  it('blocks synchronous valid-draft reentry before error, sync and known member-loss publications while allowing a fresh live write', async () => {
    for (const reason of ['error', 'sync', 'memberLoss'] as const) {
      const s = await parent();
      let entered = 0;
      const attempted: Promise<void>[] = [];
      const stop = watch(
        () => s.controller.create.state.value,
        () => {
          if (entered++) return;
          s.controller.create.open();
          s.controller.create.change(input('Observer valid form'));
          attempted.push(s.controller.create.submit());
        },
        { flush: 'sync' },
      );
      try {
        if (reason === 'error') s.controller.showError('Typed parent error');
        if (reason === 'sync') s.controller.beginSync();
        if (reason === 'memberLoss')
          await s.publish({ session: session({ member: false, ready: false }), canCreateSecret: false });
        stop();
        await Promise.all(attempted);
        expect(entered).toBeGreaterThan(0);
        const leaked = vi.mocked(s.create.create).mock.calls.length;
        expect.soft(leaked, `${reason}: observer cannot dispatch a write before the block is published`).toBe(0);
        expect(s.controller.state.value.ready).toBe(false);
        expect(s.adapter.reconcileProducer).not.toHaveBeenCalled();
        expect(s.adapter.continueRemove).not.toHaveBeenCalled();
        await s.publish({ session: session({ identity: `fresh-${reason}` }), canCreateSecret: true });
        expect(s.controller.state.value.ready).toBe(true);
        s.controller.create.open();
        s.controller.create.change(input(`Fresh ${reason}`));
        await s.controller.create.submit();
        expect(s.create.create).toHaveBeenCalledTimes(leaked + 1);
        expect(vi.mocked(s.create.create).mock.calls.at(-1)![0]).toEqual(input(`Fresh ${reason}`));
        console.info('CREATE_OWNERSHIP_STAGE', `${reason}-reentrant-block-and-live-positive-final`);
      } finally {
        stop();
        s.controller.dispose();
      }
    }
  });

  it('keeps B owned across old A reconciliation both while B is entered and after its own receipt, including hard conceal and fresh readiness', async () => {
    const oldA = deferred<CreateReceipt>(),
      nativeB = deferred<CreateReceipt>();
    const create: CreatePort = createPort({
      create: vi.fn(async (draft) => {
        if (draft.name === 'A') return { operationId: 'A', phase: 'rejected' as const };
        if (draft.name === 'B') return nativeB.promise;
        return { operationId: draft.name, phase: 'submitted' as const };
      }),
      reconcile: vi.fn(async (id) => {
        if (id === 'A' && vi.mocked(create.reconcile).mock.calls.length === 1) return oldA.promise;
        if (id === 'A') return { operationId: 'A', phase: 'rejected' as const };
        return { operationId: id, phase: 'unknown' as const };
      }),
    });
    const wrapper = createControllerCreate({ port: create, allowed: () => true });
    let heldA: Promise<void> | null = null,
      enteredB: Promise<void> | null = null;
    async function replay(label: string) {
      wrapper.conceal('hard');
      wrapper.reconcile(session());
      wrapper.open();
      wrapper.change(input(label));
      await wrapper.submit();
      expect.soft(wrapper.state.value.open, `${label}: clearing input cannot unlock B`).toBe(false);
      expect.soft(vi.mocked(create.create).mock.calls.filter(([draft]) => draft.name === label)).toHaveLength(0);
    }
    try {
      wrapper.reconcile(session());
      wrapper.open();
      wrapper.change(input('A'));
      await wrapper.submit();
      expect(create.create).toHaveBeenCalledExactlyOnceWith(input('A'));
      expect(wrapper.state.value.phase).toBe('idle');
      heldA = wrapper.refreshReceipt();
      expect(create.reconcile).toHaveBeenCalledExactlyOnceWith('A');
      wrapper.change(input('B'));
      enteredB = wrapper.submit();
      expect(create.create).toHaveBeenCalledTimes(2);
      expect(vi.mocked(create.create).mock.calls[1]![0]).toEqual(input('B'));
      expect(wrapper.state.value.phase).toBe('saving');
      // Correct ownership may skip this now-obsolete A read completely. Never wait for a missing entry.
      await wrapper.refreshReceipt();
      await replay('Replay while B entered');
      nativeB.resolve({ operationId: 'B', phase: 'submitted' });
      await enteredB;
      const before = vi.mocked(create.reconcile).mock.calls.length;
      await wrapper.refreshReceipt();
      expect(create.reconcile).toHaveBeenCalledTimes(before + 1);
      expect(vi.mocked(create.reconcile).mock.calls.at(-1)).toEqual(['B']);
      oldA.resolve({ operationId: 'A', phase: 'rejected' });
      await heldA;
      await replay('Replay after old A');
      const prior = vi.mocked(create.create).mock.calls.length;
      wrapper.reconcile(session({ identity: 'genuinely-new-owner' }));
      wrapper.open();
      wrapper.change(input('Fresh owner write'));
      await wrapper.submit();
      expect(create.create).toHaveBeenCalledTimes(prior + 1);
      expect(vi.mocked(create.create).mock.calls.at(-1)![0]).toEqual(input('Fresh owner write'));
      console.info('CREATE_OWNERSHIP_STAGE', 'old-A-new-B-entered-receipt-hard-clear-live-positive-final');
    } finally {
      nativeB.resolve({ operationId: 'B', phase: 'submitted' });
      oldA.resolve({ operationId: 'A', phase: 'rejected' });
      await enteredB;
      await heldA;
      wrapper.dispose();
    }
  });
});
