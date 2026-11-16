import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushPromises } from '@vue/test-utils';
import { deferred, requestId, secondRequestId } from './liveboardCoreFixtures';
import {
  cleanupRemoveHarness,
  identity,
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

function reply(s: Setup, outcome: 'accepted' | 'declined', id = requestId) {
  const phase = outcome === 'accepted' ? 'approved' : 'declined';
  expect
    .soft(s.controller.state.value.vault.request)
    .toMatchObject({ identity: identity(id), phase, requesterId: 'self', responderId: 'peer-b', peerIds: ['peer-b'] });
  expect.soft(s.tile.text()).toContain(outcome === 'accepted' ? 'Request approved.' : 'Request declined.');
  const packets = s.tile.findAll('.recovery-packet').map((packet) => {
    const animation = packet.find('animateMotion');
    const points = (animation.attributes('path') ?? '').match(/-?\d+(?:\.\d+)?/gu)?.map(Number) ?? [];
    return {
      color: packet.classes().filter((name) => name.startsWith('is-')),
      direction: animation.attributes('keyPoints'),
      from: points.slice(0, 2),
      to: points.slice(-2),
    };
  });
  expect.soft(packets).toEqual([
    {
      color: [outcome === 'accepted' ? 'is-approve' : 'is-decline'],
      direction: '1;0',
      from: [60, 16],
      to: [660, 16],
    },
  ]);
  expect.soft(s.tile.findAll('.recovery-connection:not(.is-idle)')).toHaveLength(1);
}

describe('Remove exact current display through public synchronous state and actual Vault DOM', () => {
  it.each(['declined', 'accepted'] as const)(
    'replaces SAME Pending with real Applied %s, empty eligibility and the verified response before ready',
    async (outcome) => {
      const s = await setup();
      await s.submit();
      await s.publish([removal()]);
      await s.publish([removal()]);
      expect(s.controller.state.value.vault.request).toMatchObject({
        identity: identity(),
        phase: 'waiting',
        peerIds: ['peer-b'],
      });
      expect(s.tile.findAll('.recovery-packet:not(.is-approve):not(.is-decline)')).toHaveLength(1);
      const done = terminal(outcome);
      const blocked = await s.publish([done], false);
      expect(s.adapter.project().operations).toEqual([done]);
      expect(s.controller.state.value.ready).toBe(false);
      expect
        .soft(blocked.some((frame) => frame.request?.phase === (outcome === 'accepted' ? 'approved' : 'declined')))
        .toBe(true);
      expect.soft(blocked.some((frame) => frame.request?.phase === 'waiting')).toBe(false);
      reply(s, outcome);
      expect(s.controller.state.value.vault.storage.busy).toBe(true);
      for (const button of s.tile.findAll('[data-testid="remove-device"]'))
        expect(button.attributes('disabled')).toBeDefined();
      const members = outcome === 'accepted' ? ['self', 'peer-b'] : ['self', 'peer-a', 'peer-b'];
      const ready = await s.publish([done], true, members);
      expect.soft(ready.some((frame) => frame.request?.phase === 'waiting')).toBe(false);
      if (outcome === 'declined') {
        reply(s, outcome);
        expect(s.controller.state.value.vault.slots.filter((slot) => slot.device)).toHaveLength(3);
        expect(s.controller.devices.state.value.preview?.targetId).toBe('peer-a');
        await s.publish([done]);
        s.controller.navigate('profile');
        s.controller.navigate('home');
        await flushPromises();
        reply(s, outcome);
      } else {
        expect.soft(ready.every((frame) => frame.request === null)).toBe(true);
        expect.soft(s.controller.state.value.vault.request).toBeNull();
        expect.soft(s.tile.find('.vault-recovery').exists()).toBe(false);
        await s.publish([removal()], false); // arbitrary older blocked snapshot, not a new session
        expect.soft(s.controller.state.value.vault.request).toBeNull();
        await s.publish([done], true, members);
        expect.soft(s.controller.state.value.vault.request).toBeNull();
      }
      noOtherWrites(s, 1);
    },
  );

  it('Current acknowledges only the actual same declined preview and does not resurrect its result', async () => {
    const s = await setup();
    await s.submit();
    await s.publish([removal()]);
    await s.publish([removal()]);
    const done = terminal('declined');
    await s.publish([done], false);
    await s.publish([done]);
    reply(s, 'declined');
    const current = s.tile.findAll('button').find((button) => button.text() === 'Current')!;
    expect(current.attributes('disabled')).toBeUndefined();
    await current.trigger('click');
    expect(s.controller.devices.state.value.preview).toBeNull();
    expect.soft(s.controller.state.value.vault.request).toBeNull();
    expect(s.adapter.project().operations).toEqual([done]);
    await s.publish([done]);
    await s.publish([removal()], false);
    expect.soft(s.controller.state.value.vault.request).toBeNull();
    await s.publish([done]);
    expect.soft(s.tile.find('.vault-recovery').exists()).toBe(false);
    noOtherWrites(s, 1);
  });

  it('new Unknown supersedes only R1 display, retains exact source, then R2 sync and ready completion settle the current two-member Vault', async () => {
    const s = await setup();
    await s.submit();
    await s.publish([removal()]);
    await s.publish([removal()]);
    const first = terminal('declined');
    await s.publish([first], false);
    await s.publish([first]);
    const observing = deferred<string>();
    vi.mocked(s.context.manager.remove_member_checked).mockResolvedValue(receipt(true, false));
    vi.mocked(s.context.manager.reconcile_producer_receipt).mockReturnValue(observing.promise);
    const port = vi.spyOn(s.adapter.devices, 'remove');
    const retry = s.controller.devices.remove();
    await flushPromises();
    expect(s.context.manager.remove_member_checked).toHaveBeenCalledTimes(2);
    expect(await port.mock.results[0]!.value).toEqual({
      operationId: 'DeviceLog:self::18446744073709551613',
      phase: 'unknown',
    });
    expect.soft(s.controller.state.value.vault.request).toBeNull();
    expect.soft(s.tile.find('.vault-recovery').exists()).toBe(false);
    expect(s.controller.devices.state.value.busy).toBe(true);
    s.controller.devices.current();
    await s.controller.devices.remove();
    expect(s.controller.devices.state.value.preview?.targetId).toBe('peer-a');
    expect(s.context.manager.remove_member_checked).toHaveBeenCalledTimes(2);
    observing.reject(new Error('ObservationUnavailable'));
    await retry;
    vi.mocked(s.context.manager.reconcile_producer_receipt).mockRejectedValue(new Error('ObservationUnavailable'));
    await s.publish([first]);
    await s.controller.devices.remove();
    expect(s.controller.devices.state.value.busy).toBe(true);
    expect(s.context.manager.remove_member_checked).toHaveBeenCalledTimes(2);
    const unknownReferences = vi
      .mocked(s.context.manager.reconcile_producer_receipt)
      .mock.calls.map(([raw]) => raw)
      .filter((raw) => raw.includes('18446744073709551613'));
    expect(unknownReferences.length).toBeGreaterThan(0);
    expect(unknownReferences.every((raw) => !raw.includes('requestId'))).toBe(true);
    expect(unknownReferences.every((raw) => raw.includes('"bodyCommitment":"' + 'a'.repeat(64) + '"'))).toBe(true);
    vi.mocked(s.context.manager.reconcile_producer_receipt).mockResolvedValue(receipt(true));
    await s.controller.refresh();
    const applying = terminal('accepted', {
      requestId: secondRequestId,
      clientStatus: 'synchronizing',
      readiness: 'synchronizing',
    });
    await s.publish([first, applying]);
    reply(s, 'accepted', secondRequestId);
    expect(s.controller.devices.state.value.busy).toBe(true);
    expect(s.tile.get('[data-testid="remove-device"]').attributes('disabled')).toBeDefined();
    const done = terminal('accepted', { requestId: secondRequestId });
    await s.publish([first, done], false, ['self', 'peer-b']);
    expect(s.controller.state.value.ready).toBe(false);
    reply(s, 'accepted', secondRequestId);
    const ready = await s.publish([first, done], true, ['self', 'peer-b']);
    expect.soft(ready.every((frame) => frame.request === null)).toBe(true);
    expect(s.controller.devices.state.value).toMatchObject({ preview: null, operation: null, busy: false });
    expect(s.controller.state.value.vault.slots.filter((slot) => slot.device).map((slot) => slot.slotId)).toEqual([
      'self',
      'peer-b',
    ]);
    expect(s.tile.findAll('.device-slot.is-inactive')).toHaveLength(1);
    expect(s.tile.text()).toContain('Both devices store a complete encrypted copy.');
    expect(s.tile.text()).toContain('Storage model');
    expect(s.tile.text()).not.toContain('Time Machine');
    expect(s.tile.find('[data-testid="remove-device"]').exists()).toBe(false);
    expect.soft(s.tile.find('.vault-recovery').exists()).toBe(false);
    expect.soft(s.tile.find('.recovery-packet').exists()).toBe(false);
    s.controller.navigate('secrets');
    s.controller.navigate('home');
    await s.publish([first, removal({ requestId: secondRequestId })], false);
    expect.soft(s.controller.state.value.vault.request).toBeNull();
    await s.publish([first, done], true, ['self', 'peer-b']);
    expect.soft(s.controller.state.value.vault.request).toBeNull();
    expect(s.adapter.project().operations).toEqual([first, done]);
    noOtherWrites(s, 2);
  });
});
