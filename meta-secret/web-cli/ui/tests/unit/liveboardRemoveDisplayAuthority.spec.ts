import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushPromises } from '@vue/test-utils';
import { watch } from 'vue';
import type { CoreOperation } from '@/features/liveboard/coreTypes';
import { deferred, requestId, secondRequestId } from './liveboardCoreFixtures';
import {
  cleanupRemoveHarness,
  identity,
  invalidRemoveCurrent,
  noOtherWrites,
  receipt,
  removal,
  resetRemoveHarness,
  setupRemove as setup,
  terminal,
  type RemoveSetup as Setup,
} from './liveboardRemoveDisplayFixtures';

beforeEach(resetRemoveHarness);
afterEach(cleanupRemoveHarness);

function decline(s: Setup, id = requestId) {
  expect.soft(s.controller.state.value.vault.request).toMatchObject({ identity: identity(id), phase: 'declined' });
  expect.soft(s.tile.text()).toContain('Request declined.');
}
async function establish(s: Setup) {
  await s.submit();
  await s.publish([removal()]);
  await s.publish([removal()]);
  await s.publish([terminal('declined')], false);
  await s.publish([terminal('declined')]);
}
async function unknown(s: Setup, decision = false) {
  vi.mocked(s.context.manager.remove_member_checked).mockResolvedValue(receipt(true, false, decision));
  vi.mocked(s.context.manager.reconcile_producer_receipt).mockRejectedValue(new Error('ObservationUnavailable'));
  s.controller.devices.preview('peer-a');
  await s.controller.devices.remove();
}

describe('Remove display never becomes mutation or actor authority', () => {
  for (const ready of [false, true]) {
    it.each(['requester', 'target'] as const)(
      `remembered immutable %s rejects repeated same-key replacement at ready=${ready}`,
      async (field) => {
        const s = await setup({ operations: [removal()] });
        const contradictory = terminal('accepted', {
          clientStatus: 'synchronizing',
          readiness: 'synchronizing',
          ...(field === 'requester' ? { requesterId: 'peer-b' } : { targetDeviceId: 'peer-b' }),
        });
        for (let repeat = 0; repeat < 2; repeat++) {
          const seen = await s.publish([contradictory], ready);
          expect.soft(seen.every((frame) => frame.request === null)).toBe(true);
          expect.soft(s.controller.state.value.vault.request).toBeNull();
          expect.soft(s.tile.find('.recovery-packet').exists()).toBe(false);
        }
        await s.publish([terminal('declined')]);
        decline(s);
        await s.publish([contradictory]);
        expect.soft(s.controller.state.value.vault.request).toBeNull();
        noOtherWrites(s, 0);
      },
    );
  }

  it.each(['unknown-source', 'missing-responder'] as const)(
    '%s never invents a returning peer from eligibility',
    async (reason) => {
      const row = terminal('accepted', {
        clientStatus: 'synchronizing',
        readiness: 'synchronizing',
        eligiblePeerIds: ['peer-b'],
        ...(reason === 'unknown-source' ? { resolutionSource: 'unknown' } : { responderId: undefined }),
      });
      const s = await setup({ operations: [row] });
      expect(s.controller.state.value.vault.request).toBeNull();
      expect(s.tile.find('.recovery-packet.is-approve, .recovery-packet.is-decline').exists()).toBe(false);
      noOtherWrites(s, 0);
    },
  );

  it.each(['no-preview', 'other-preview', 'blocked'] as const)(
    'Current with %s is not acknowledgement of another actual result',
    async (reason) => {
      const s = await setup({ operations: [terminal('declined')] });
      if (reason !== 'no-preview') s.controller.devices.preview(reason === 'other-preview' ? 'peer-b' : 'peer-a');
      if (reason === 'blocked') await s.publish([terminal('declined')], false);
      s.controller.devices.current();
      await s.publish([terminal('declined')]);
      decline(s);
      expect(s.adapter.project().operations).toEqual([terminal('declined')]);
      noOtherWrites(s, 0);
      expect(s.platform.authenticate).not.toHaveBeenCalled();
    },
  );

  it('Current cannot acknowledge after a synchronous public observer changes readiness during its actual clear', async () => {
    const s = await setup({ operations: [terminal('declined')] });
    s.controller.devices.preview('peer-a');
    const stop = watch(
      () => s.controller.devices.state.value.preview,
      (next, previous) => {
        if (previous?.targetId === 'peer-a' && !next) {
          s.context.ready = false;
          void s.controller.refresh();
        }
      },
      { flush: 'sync' },
    );
    try {
      s.controller.devices.current();
      expect(s.controller.devices.state.value.preview).toBeNull();
      expect(s.controller.state.value.ready).toBe(false);
    } finally {
      stop();
    }
    await s.publish([terminal('declined')]);
    decline(s);
    noOtherWrites(s, 0);
  });

  it.each(['auth-cancel', 'pre-admission', 'rejected'] as const)(
    '%s retry preserves the old exact terminal and original source',
    async (reason) => {
      const s = await setup();
      await establish(s);
      const observed = vi.mocked(s.context.manager.reconcile_producer_receipt).mock.calls.length;
      const submitted = s.context.manager.remove_member_checked;
      if (reason === 'auth-cancel') s.platform.authenticate.mockResolvedValue(false);
      if (reason === 'pre-admission')
        Object.defineProperty(s.context.manager, 'remove_member_checked', { value: undefined });
      if (reason === 'rejected')
        vi.spyOn(s.adapter.devices, 'remove').mockResolvedValue({ operationId: 'not-admitted', phase: 'rejected' });
      await s.controller.devices.remove();
      expect(submitted).toHaveBeenCalledTimes(1);
      expect(s.context.manager.reconcile_producer_receipt).toHaveBeenCalledTimes(observed);
      await s.publish([terminal('declined')]);
      decline(s);
      expect(s.adapter.project().operations).toEqual([terminal('declined')]);
      if (reason === 'pre-admission')
        Object.defineProperty(s.context.manager, 'remove_member_checked', { value: submitted });
      noOtherWrites(s, 1);
    },
  );

  it('a terminal first learned after intent cannot acknowledge or unlock its new Unknown', async () => {
    const s = await setup();
    s.controller.devices.preview('peer-a');
    const sending = deferred<string>();
    vi.mocked(s.context.manager.remove_member_checked).mockReturnValue(sending.promise);
    vi.mocked(s.context.manager.reconcile_producer_receipt).mockRejectedValue(new Error('ObservationUnavailable'));
    const pending = s.controller.devices.remove();
    await flushPromises();
    expect(s.context.manager.remove_member_checked).toHaveBeenCalledOnce();
    await s.publish([terminal('declined')]);
    sending.resolve(receipt(true, false));
    await pending;
    decline(s);
    expect.soft(s.controller.devices.state.value.busy).toBe(true);
    s.controller.devices.current();
    expect.soft(s.controller.devices.state.value.preview?.targetId).toBe('peer-a');
    await s.controller.devices.remove();
    noOtherWrites(s, 1);
  });

  it('a settled old R returned again is not a new admission that consumes the later declined display', async () => {
    const s = await setup();
    await establish(s);
    s.controller.devices.current();
    const later = terminal('declined', { requestId: secondRequestId });
    await s.publish([terminal('declined'), later]);
    s.controller.devices.preview('peer-a');
    vi.mocked(s.context.manager.remove_member_checked).mockResolvedValue(receipt(false, false));
    vi.mocked(s.context.manager.reconcile_producer_receipt).mockResolvedValue(receipt());
    await s.controller.devices.remove();
    await s.controller.refresh();
    decline(s, secondRequestId);
    expect(s.controller.devices.state.value.busy).toBe(true);
    await s.controller.devices.remove();
    noOtherWrites(s, 2);
  });

  it.each(invalidRemoveCurrent)(
    '$reason cannot settle Unknown, clear its preview, release controls or replace its actual R',
    async ({ rows, members }) => {
      const s = await setup({ operations: [terminal('declined')] });
      await unknown(s);
      await s.publish(rows, true, members);
      expect.soft(s.controller.devices.state.value.busy).toBe(true);
      s.controller.devices.current();
      expect.soft(s.controller.devices.state.value.preview?.targetId).toBe('peer-a');
      await s.publish([terminal('declined')]);
      await s.controller.devices.remove();
      expect.soft(s.platform.authenticate).toHaveBeenCalledOnce();
      const references = vi.mocked(s.context.manager.reconcile_producer_receipt).mock.calls.map(([raw]) => raw);
      expect(references.length).toBeGreaterThan(0);
      expect(references.every((raw) => raw.includes('"curr":18446744073709551613') && !raw.includes('requestId'))).toBe(
        true,
      );
      noOtherWrites(s, 1);
    },
  );

  it.each(['wrong-id', 'wrong-target', 'wrong-actor', 'unproven', 'no-responder', 'absent'] as const)(
    '%s completion metadata cannot consume the original display key',
    async (reason) => {
      const s = await setup({ operations: [removal()] });
      const patch: Partial<CoreOperation> =
        reason === 'wrong-id'
          ? { requestId: secondRequestId }
          : reason === 'wrong-target'
            ? { targetDeviceId: 'peer-b' }
            : reason === 'wrong-actor'
              ? { requesterId: 'peer-b' }
              : reason === 'unproven'
                ? { resolutionSource: 'unknown' }
                : { responderId: undefined };
      await s.publish(reason === 'absent' ? [] : [terminal('accepted', patch)]);
      expect.soft(s.controller.state.value.vault.request).toBeNull();
      await s.publish([terminal('declined')]);
      decline(s);
      noOtherWrites(s, 0);
    },
  );

  it('reobservation of the same Unknown does not acknowledge a newly learned terminal', async () => {
    const s = await setup({ operations: [terminal('declined')] });
    await unknown(s);
    const later = terminal('declined', { requestId: secondRequestId });
    await s.publish([later]);
    await s.controller.refresh();
    await s.controller.refresh();
    decline(s, secondRequestId);
    expect.soft(s.controller.devices.state.value.busy).toBe(true);
    await s.controller.devices.remove();
    noOtherWrites(s, 1);
  });

  it('new admission for another exact target cannot acknowledge this declined subject', async () => {
    const other = terminal('declined', { targetDeviceId: 'peer-b', responderId: 'peer-a' });
    const s = await setup({ operations: [other] });
    await unknown(s);
    decline(s);
    expect(s.adapter.project().operations).toEqual([other]);
    noOtherWrites(s, 1);
  });

  it.each(['foreign-actor', 'foreign-subject', 'unproven'] as const)(
    'new admission with current %s metadata cannot acknowledge its remembered old terminal',
    async (reason) => {
      const s = await setup();
      await establish(s);
      const changed = terminal(
        'declined',
        reason === 'foreign-actor'
          ? { requesterId: 'peer-b' }
          : reason === 'foreign-subject'
            ? { targetDeviceId: 'peer-b' }
            : { resolutionSource: 'unknown' },
      );
      await s.publish([changed]);
      await unknown(s);
      await s.publish([terminal('declined')]);
      decline(s);
      expect(s.adapter.project().operations).toEqual([terminal('declined')]);
      noOtherWrites(s, 2);
    },
  );

  it('malformed canonical metadata blocks the public boundary and never becomes retry or completion permission', async () => {
    const s = await setup({ operations: [terminal('declined')] });
    await unknown(s);
    await s.publish([terminal('accepted', { requestId: '' })]);
    expect(s.controller.state.value.ready).toBe(false);
    expect(s.controller.state.value.vault.request).toBeNull();
    s.controller.devices.current();
    await s.controller.devices.remove();
    expect(s.platform.authenticate).toHaveBeenCalledOnce();
    noOtherWrites(s, 1);
  });

  it('consumed display belongs to its session and does not erase the same canonical UUID in a new identity', async () => {
    const s = await setup();
    await establish(s);
    s.controller.devices.current();
    expect.soft(s.controller.state.value.vault.request).toBeNull();
    s.context.identity = 'session-b';
    await s.publish([terminal('declined')]);
    decline(s);
    expect(s.adapter.project().operations).toEqual([terminal('declined')]);
    noOtherWrites(s, 1);
  });

  it.each(['recover-accepted', 'recover-declined', 'delete-declined'] as const)(
    'Remove admission preserves unrelated %s unread/history display',
    async (kind) => {
      const recover = kind.startsWith('recover');
      const accepted = kind === 'recover-accepted';
      const old = removal({
        kind: recover ? 'recover' : 'deleteSecret',
        requestId: recover ? 'claim-second' : requestId,
        targetDeviceId: undefined,
        secretId: 'pass-a',
        outcome: accepted ? 'accepted' : 'declined',
        clientStatus: accepted ? 'accepted' : 'declined',
        responderId: 'peer-b',
        resolutionSource: 'verified',
        readiness: accepted ? 'ready' : 'waiting',
      });
      const s = await setup();
      await s.publish([old], true, undefined, {
        secrets: [
          {
            secretId: 'pass-a',
            primaryAction: accepted ? 'showRecovered' : 'recover',
            recoveryClaimId: recover ? 'claim-second' : undefined,
            canDelete: true,
            readiness: 'ready',
          },
        ],
      });
      await unknown(s);
      await s.controller.refresh();
      expect(s.controller.state.value.vault.request).toMatchObject({
        identity: JSON.stringify([old.kind, old.requestId]),
        phase: accepted ? 'approved' : 'declined',
      });
      if (recover) expect(s.controller.state.value.unread['claim-second']).toBe(accepted ? 'green' : 'red');
      expect(s.adapter.project().operations).toEqual([old]);
      noOtherWrites(s, 1);
    },
  );

  it('local 2-to-1 uses its actual requester response without fictional remote paths and preserves original R/U', async () => {
    const s = await setup({ members: ['self', 'peer-a'] });
    await s.submit(false, true);
    const applying = terminal('accepted', {
      clientStatus: 'synchronizing',
      readiness: 'synchronizing',
      responderId: 'self',
    });
    await s.publish([applying], true, ['self', 'peer-a']);
    expect
      .soft(s.controller.state.value.vault.request)
      .toMatchObject({ phase: 'approved', requesterId: 'self', responderId: 'self', peerIds: [] });
    expect(s.tile.find('.recovery-packet').exists()).toBe(false);
    const references = vi.mocked(s.context.manager.reconcile_producer_receipt).mock.calls.map(([raw]) => raw);
    expect(references.length).toBeGreaterThan(0);
    for (const raw of references) {
      expect(raw).toContain('"curr":18446744073709551615');
      expect(raw).toContain('"curr":9007199254740993');
      expect(raw).toContain('"bodyCommitment":"' + 'a'.repeat(64) + '"');
      expect(raw).toContain('"bodyCommitment":"' + 'b'.repeat(64) + '"');
    }
    await s.publish([terminal('declined', { responderId: 'self' })], true, ['self', 'peer-a']);
    decline(s);
    expect(s.tile.find('.recovery-packet').exists()).toBe(false);
    noOtherWrites(s, 1);
  });
});
