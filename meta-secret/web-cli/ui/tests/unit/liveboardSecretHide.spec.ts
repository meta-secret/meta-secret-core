import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushPromises, mount } from '@vue/test-utils';
import SecretValue from '@/components/liveboard/SecretValue.vue';
import { createSecretController } from '@/features/liveboard/secretController';
import { secret, secretPort, session } from './liveboardFixtures';

const sampled = vi.hoisted(() => ({
  hasTargets: false,
  reveal: vi.fn(),
  conceal: vi.fn<(hasTargets: boolean) => void>(),
  clear: vi.fn(),
}));
vi.mock('@/features/liveboard/revealMotion', () => ({
  createRevealMotion: () => ({
    reveal: () => {
      sampled.hasTargets = true;
      sampled.reveal();
    },
    clear: () => {
      sampled.hasTargets = false;
      sampled.clear();
    },
    conceal: () => sampled.conceal(sampled.hasTargets),
  }),
}));
afterEach(() => {
  vi.clearAllMocks();
  sampled.hasTargets = false;
});

describe('Liveboard actual SecretValue/controller Hide ownership', () => {
  it('passes existing numeric targets into conceal before any caller clears them', async () => {
    const controller = createSecretController(secretPort());
    controller.reconcile({ session: session(), secrets: [secret()] });
    controller.present({ active: true, visibleIds: ['pass-a'], query: '', scrollTop: 0 });
    const wrapper = mount(SecretValue, { props: { card: controller.state.value.cards['pass-a']!, index: 0 } });
    try {
      await controller.primary('pass-a');
      await flushPromises();
      expect(sampled.reveal).toHaveBeenCalledOnce();
      expect(sampled.hasTargets).toBe(true);
      controller.hide('pass-a');
      await flushPromises();
      expect(wrapper.find('.secret-value').exists()).toBe(false);
      expect(sampled.conceal).toHaveBeenCalledExactlyOnceWith(true);
    } finally {
      wrapper.unmount();
      controller.dispose();
    }
  });
});
