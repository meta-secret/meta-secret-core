import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushPromises } from '@vue/test-utils';
import { requestId } from './liveboardCoreFixtures';
import {
  cleanupRemoveHarness,
  noOtherWrites,
  removal,
  resetRemoveHarness,
  setupRemove as setup,
  terminal,
  type RemoveSetup as Setup,
} from './liveboardRemoveDisplayFixtures';

beforeEach(resetRemoveHarness);
afterEach(cleanupRemoveHarness);

function currentButton(s: Setup) {
  return s.tile.findAll('button').find((button) => button.text() === 'Current');
}

async function retainsLocalOwner(s: Setup, stage: string) {
  expect.soft(s.controller.devices.state.value.busy, stage).toBe(true);
  expect.soft(s.controller.devices.state.value.preview?.targetId, stage).toBe('peer-a');
  expect.soft(s.controller.state.value.vault.storage.busy, stage).toBe(true);
  const current = currentButton(s);
  expect.soft(Boolean(current), stage).toBe(true);
  if (current) expect.soft(current.attributes('disabled'), stage).toBeDefined();
  s.controller.devices.current();
  await flushPromises();
  expect.soft(s.controller.devices.state.value.preview?.targetId, `${stage}: Current`).toBe('peer-a');
  expect.soft(s.controller.devices.state.value.busy, `${stage}: Current`).toBe(true);
  expect.soft(s.platform.authenticate, stage).toHaveBeenCalledOnce();
  noOtherWrites(s, 1);
}

function originalKnownSource(s: Setup) {
  const references = vi.mocked(s.context.manager.reconcile_producer_receipt).mock.calls.map(([raw]) => raw);
  expect(references.length).toBeGreaterThan(1);
  expect(references[0]).not.toContain('requestId');
  expect(references.some((raw) => raw.includes(`"requestId":"${requestId}"`))).toBe(true);
  for (const raw of references) {
    expect(raw).toContain('"curr":18446744073709551615');
    expect(raw).toContain('"prev":18446744073709551614');
    expect(raw).toContain('"objInstance":"self"');
    expect(raw).toContain('"bodyCommitment":"' + 'a'.repeat(64) + '"');
  }
  noOtherWrites(s, 1);
}

describe('Remove private ownership survives readiness gaps without replacing passive canonical guards', () => {
  it.each(['waiting', 'settling'] as const)(
    'ordinary preview with no local producer retains canonical %s through repeated current refreshes',
    async (phase) => {
      const s = await setup();
      s.controller.devices.preview('peer-a');
      expect(s.controller.devices.state.value.preview?.targetId).toBe('peer-a');
      expect(s.controller.devices.state.value.busy).toBe(false);
      const row =
        phase === 'waiting'
          ? removal()
          : terminal('accepted', { clientStatus: 'synchronizing', readiness: 'synchronizing' });

      for (let repeat = 0; repeat < 3; repeat++) {
        await s.publish([row]);
        expect.soft(s.controller.devices.state.value.operation).toMatchObject({
          requestId,
          targetId: 'peer-a',
          phase,
        });
        expect.soft(s.controller.devices.state.value.busy).toBe(true);
        expect.soft(s.controller.state.value.vault.storage.busy).toBe(true);
        const current = currentButton(s);
        expect.soft(Boolean(current)).toBe(true);
        if (current) expect.soft(current.attributes('disabled')).toBeDefined();
        s.controller.devices.current();
        await s.controller.devices.remove();
        expect.soft(s.controller.devices.state.value.preview?.targetId).toBe('peer-a');
        expect.soft(s.controller.devices.state.value.busy).toBe(true);
        expect(s.adapter.project().operations).toEqual([row]);
        expect(s.platform.authenticate).not.toHaveBeenCalled();
        expect(s.context.manager.reconcile_producer_receipt).not.toHaveBeenCalled();
        noOtherWrites(s, 0);
      }
    },
  );

  it.each(['accepted', 'declined'] as const)(
    'known local UUID retains its owner across blocked %s, absent ready metadata and only resolves on fresh exact ready proof',
    async (outcome) => {
      const s = await setup();
      await s.submit();
      await s.publish([removal()]);
      await s.publish([removal()]);
      expect(s.controller.devices.state.value.operation).toMatchObject({ requestId, phase: 'waiting' });
      originalKnownSource(s);

      const done = terminal(outcome);
      const members = outcome === 'accepted' ? ['self', 'peer-b'] : ['self', 'peer-a', 'peer-b'];
      await s.publish([done], false, members);
      expect(s.controller.state.value.ready).toBe(false);
      expect(s.adapter.project().operations).toEqual([done]);
      await retainsLocalOwner(s, 'blocked exact terminal');

      for (let repeat = 0; repeat < 2; repeat++) {
        await s.publish([], true, members);
        expect(s.controller.state.value.ready).toBe(true);
        expect(s.adapter.project().operations).toEqual([]);
        await retainsLocalOwner(s, `ready absence ${repeat}`);
        await s.controller.devices.remove();
        expect(s.platform.authenticate).toHaveBeenCalledOnce();
        originalKnownSource(s);
      }

      await s.publish([done], true, members);
      expect(s.controller.state.value.ready).toBe(true);
      expect.soft(s.controller.devices.state.value.busy).toBe(false);
      expect.soft(s.controller.state.value.vault.storage.busy).toBe(false);
      expect(s.adapter.project().operations).toEqual([done]);
      expect(s.controller.state.value.vault.slots.filter((slot) => slot.device).map((slot) => slot.slotId)).toEqual(
        members,
      );

      if (outcome === 'accepted') {
        expect.soft(s.controller.devices.state.value.preview).toBeNull();
        expect.soft(s.controller.devices.state.value.operation).toBeNull();
        expect(s.tile.text()).toContain('Storage model');
        expect(s.tile.text()).toContain('Both devices store a complete encrypted copy.');
        expect.soft(s.tile.text()).not.toContain('Time Machine');
        expect.soft(Boolean(currentButton(s))).toBe(false);
        expect.soft(s.tile.find('[data-testid="remove-device"]').exists()).toBe(false);
      } else {
        expect.soft(s.controller.devices.state.value.preview?.targetId).toBe('peer-a');
        const current = currentButton(s);
        expect.soft(Boolean(current)).toBe(true);
        if (current) {
          expect.soft(current.attributes('disabled')).toBeUndefined();
          await current.trigger('click');
        }
        expect.soft(s.controller.devices.state.value.preview).toBeNull();
        expect(s.controller.state.value.vault.slots.filter((slot) => slot.device)).toHaveLength(3);
        expect(s.adapter.project().operations).toEqual([done]);
      }

      await s.publish([done], true, members);
      originalKnownSource(s);
      expect(s.platform.authenticate).toHaveBeenCalledOnce();
      noOtherWrites(s, 1);
    },
  );
});
