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

function observationCount(s: Setup) {
  return vi.mocked(s.context.manager.reconcile_producer_receipt).mock.calls.length;
}

function observesSameKnownSource(s: Setup, since: number, stage: string) {
  const references = vi.mocked(s.context.manager.reconcile_producer_receipt).mock.calls.slice(since);
  expect.soft(references.length, stage).toBeGreaterThan(0);
  for (const [raw] of references) {
    expect.soft(raw, stage).toContain(`"requestId":"${requestId}"`);
    expect.soft(raw, stage).toContain('"curr":18446744073709551615');
    expect.soft(raw, stage).toContain('"prev":18446744073709551614');
    expect.soft(raw, stage).toContain('"objInstance":"self"');
    expect.soft(raw, stage).toContain('"bodyCommitment":"' + 'a'.repeat(64) + '"');
  }
}

async function retainsPublicOwner(s: Setup, stage: string) {
  expect.soft(s.controller.devices.state.value.busy, stage).toBe(true);
  expect.soft(s.controller.devices.state.value.preview?.targetId, stage).toBe('peer-a');
  expect.soft(s.controller.state.value.vault.storage.busy, stage).toBe(true);
  const current = s.tile.findAll('button').find((button) => button.text() === 'Current');
  expect.soft(Boolean(current), stage).toBe(true);
  if (current) expect.soft(current.attributes('disabled'), stage).toBeDefined();
  s.controller.devices.current();
  await flushPromises();
  expect.soft(s.controller.devices.state.value.preview?.targetId, `${stage}: Current`).toBe('peer-a');
  expect.soft(s.controller.devices.state.value.busy, `${stage}: Current`).toBe(true);
  expect.soft(s.context.manager.remove_member_checked, stage).toHaveBeenCalledOnce();
  expect.soft(s.platform.authenticate, stage).toHaveBeenCalledOnce();
}

describe('Remove keeps its original correlation through unusable known-UUID outcomes until a fresh valid result', () => {
  it.each([
    {
      reason: 'ready unproven Decline',
      outcome: 'declined' as const,
      unusable: terminal('declined', { resolutionSource: 'unknown' }),
    },
    {
      reason: 'ready Accepted Done without responder',
      outcome: 'accepted' as const,
      unusable: terminal('accepted', { responderId: undefined }),
    },
  ])(
    '$reason retains observation of the same checked R through absence, then resolves the valid exact result',
    async ({ outcome, unusable }) => {
      const s = await setup();
      await s.submit();
      await s.publish([removal()]);
      await s.publish([removal()]);
      expect(s.controller.devices.state.value.operation).toMatchObject({
        requestId,
        targetId: 'peer-a',
        phase: 'waiting',
      });
      const initialReferences = vi.mocked(s.context.manager.reconcile_producer_receipt).mock.calls;
      expect(initialReferences.length).toBeGreaterThan(1);
      expect(initialReferences[0]![0]).not.toContain('requestId');
      expect(initialReferences.some(([raw]) => raw.includes(`"requestId":"${requestId}"`))).toBe(true);

      let cursor = observationCount(s);
      await s.publish([unusable]);
      expect(s.controller.state.value.ready).toBe(true);
      expect(s.adapter.project().operations).toEqual([unusable]);
      observesSameKnownSource(s, cursor, 'ready unusable terminal');
      await retainsPublicOwner(s, 'ready unusable terminal');

      for (let repeat = 0; repeat < 2; repeat++) {
        cursor = observationCount(s);
        await s.publish([]);
        expect(s.controller.state.value.ready).toBe(true);
        expect(s.adapter.project().operations).toEqual([]);
        observesSameKnownSource(s, cursor, `ready absence ${repeat}`);
        await retainsPublicOwner(s, `ready absence ${repeat}`);
      }

      const valid = terminal(outcome);
      const members = outcome === 'accepted' ? ['self', 'peer-b'] : ['self', 'peer-a', 'peer-b'];
      await s.publish([valid], true, members);
      expect(s.controller.state.value.ready).toBe(true);
      expect(s.adapter.project().operations).toEqual([valid]);
      expect.soft(s.controller.devices.state.value.busy, 'fresh valid result').toBe(false);
      expect.soft(s.controller.state.value.vault.storage.busy, 'fresh valid result').toBe(false);
      expect(s.controller.state.value.vault.slots.filter((slot) => slot.device).map((slot) => slot.slotId)).toEqual(
        members,
      );

      if (outcome === 'accepted') {
        expect.soft(s.controller.devices.state.value.preview).toBeNull();
        expect.soft(s.controller.devices.state.value.operation).toBeNull();
        expect(s.tile.text()).toContain('Both devices store a complete encrypted copy.');
        expect.soft(s.tile.text()).not.toContain('Time Machine');
        expect.soft(s.tile.find('[data-testid="remove-device"]').exists()).toBe(false);
      } else {
        expect.soft(s.controller.devices.state.value.preview?.targetId).toBe('peer-a');
        const current = s.tile.findAll('button').find((button) => button.text() === 'Current');
        expect.soft(Boolean(current), 'fresh Decline Current').toBe(true);
        if (current) expect.soft(current.attributes('disabled')).toBeUndefined();
        s.controller.devices.current();
        await flushPromises();
        expect.soft(s.controller.devices.state.value.preview).toBeNull();
        expect(s.controller.state.value.vault.slots.filter((slot) => slot.device)).toHaveLength(3);
      }

      await s.publish([valid], true, members);
      expect.soft(s.controller.devices.state.value.busy).toBe(false);
      expect(s.adapter.project().operations).toEqual([valid]);
      expect(s.platform.authenticate).toHaveBeenCalledOnce();
      noOtherWrites(s, 1);
    },
  );
});
