import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushPromises, mount } from '@vue/test-utils';
import { createCoreAdapter } from '@/features/liveboard/coreAdapter';
import { createLiveboardController } from '@/features/liveboard/controller';
import SecretCard from '@/components/liveboard/SecretCard.vue';
import SecretsPage from '@/components/liveboard/SecretsPage.vue';
import { contextFixture, coreState, deferred, platformFixture } from './liveboardCoreFixtures';

const motion = vi.hoisted(() => ({ targets: [] as number[], clear: vi.fn(), conceal: vi.fn() }));
vi.mock('@/features/liveboard/revealMotion', () => ({
  createRevealMotion: () => ({
    reveal: () => {
      motion.targets = [3, 7, 11];
    },
    clear: () => {
      motion.targets.length = 0;
      motion.clear();
    },
    conceal: motion.conceal,
  }),
}));
beforeEach(() =>
  vi.stubGlobal('matchMedia', () => ({ matches: true, addEventListener: vi.fn(), removeEventListener: vi.fn() })),
);
afterEach(() => {
  vi.unstubAllGlobals();
  vi.clearAllMocks();
  motion.targets.length = 0;
});
function setup() {
  let context = contextFixture(JSON.stringify(coreState({ operations: [] })));
  const platform = platformFixture();
  const adapter = createCoreAdapter({ getContext: () => context, platform });
  const controller = createLiveboardController({ adapter, resetLocal: vi.fn(async () => {}), version: '2.0.0' });
  controller.refresh();
  controller.navigate('secrets');
  return {
    controller,
    adapter,
    platform,
    get context() {
      return context;
    },
    replace: (value: typeof context) => {
      context = value;
    },
  };
}

describe('Liveboard integrated plaintext and submitted-operation lifetime', () => {
  it.each(['route', 'filter', 'sync', 'auth', 'tombstone', 'pagehide'] as const)(
    'clears held card, real DOM, Copy and numeric motion targets on %s',
    async (reason) => {
      const s = setup();
      const held = s.controller.secrets.state.value.cards['pass-a']!;
      const wrapper = mount(SecretCard, { props: { card: held, index: 0, blocked: false } });
      await s.controller.secrets.primary('pass-a');
      await flushPromises();
      expect(wrapper.text()).toContain('private marker');
      expect(wrapper.find('[data-secret-action="copy"]').exists()).toBe(true);
      expect(motion.targets.length).toBeGreaterThan(0);
      if (reason === 'route') s.controller.navigate('profile');
      if (reason === 'filter') s.controller.search('no-match');
      if (reason === 'sync') s.controller.beginSync();
      if (reason === 'auth') {
        s.replace({ ...s.context, epoch: 2, ready: false });
        s.controller.refresh();
      }
      if (reason === 'tombstone') {
        vi.mocked(s.context.member.liveboard_state_json).mockReturnValue(
          JSON.stringify(coreState({ secrets: [], operations: [] })),
        );
        s.controller.refresh();
      }
      if (reason === 'pagehide') window.dispatchEvent(new Event('pagehide'));
      expect(held.value).toBeNull();
      expect(held.copied).toBe(false);
      expect(motion.targets).toEqual([]);
      await flushPromises();
      expect(wrapper.text()).not.toContain('private marker');
      expect(wrapper.find('[data-secret-action="copy"]').exists()).toBe(false);
      wrapper.unmount();
      s.controller.dispose();
    },
  );

  it('rejects stale Show/Copy on deterministic SAME PassID after deletion and same-name recreation', async () => {
    const s = setup();
    const showing = deferred<string>();
    vi.mocked(s.context.manager.show_local_secret).mockReturnValue(showing.promise);
    const pending = s.controller.secrets.primary('pass-a');
    vi.mocked(s.context.member.liveboard_state_json).mockReturnValue(
      JSON.stringify(coreState({ secrets: [], operations: [] })),
    );
    s.controller.refresh();
    vi.mocked(s.context.member.liveboard_state_json).mockReturnValue(JSON.stringify(coreState({ operations: [] })));
    s.controller.refresh(); // The recreated deterministic ID is still pass-a, not a synthetic new ID.
    showing.resolve('old private material');
    await pending;
    expect(s.controller.secrets.state.value.cards['pass-a'].value).toBeNull();
    await s.controller.secrets.copy('pass-a');
    expect(s.platform.copy).not.toHaveBeenCalled();
    s.controller.dispose();
  });

  it('cannot restore copied/plaintext state after route exit and a late clipboard completion', async () => {
    const s = setup();
    const copying = deferred<void>();
    s.platform.copy.mockReturnValue(copying.promise);
    await s.controller.secrets.primary('pass-a');
    const held = s.controller.secrets.state.value.cards['pass-a'];
    const pending = s.controller.secrets.copy('pass-a');
    s.controller.navigate('home');
    copying.resolve();
    await pending;
    expect(held.value).toBeNull();
    expect(held.copied).toBe(false);
    s.controller.dispose();
  });

  it('retains the SAME Unknown producer R/U across route changes without automatic continuation or resubmission', async () => {
    const s = setup();
    await s.controller.secrets.remove('pass-a');
    s.controller.navigate('profile');
    s.controller.navigate('secrets');
    s.controller.refresh();
    await s.controller.secrets.remove('pass-a');
    expect(s.context.manager.delete_secret_checked).toHaveBeenCalledOnce();
    expect(s.context.manager.delete_secret).not.toHaveBeenCalled();
    expect(s.context.manager.continue_local_remove_checked).not.toHaveBeenCalled();
    expect(s.controller.secrets.state.value.cards['pass-a'].phase).toBe('deleting');
    s.controller.dispose();
  });

  it('keeps an actually visible spaced-query card eligible for exact Show and Copy', async () => {
    const s = setup();
    const shownPasses: string[] = [];
    vi.mocked(s.context.manager.show_local_secret).mockImplementation(async (pass) => {
      shownPasses.push(pass.id_str());
      return 'private marker';
    });
    const wrapper = mount(SecretsPage, {
      props: {
        view: s.controller.secrets.state.value,
        backHref: '/',
        onSearch: s.controller.search,
        onPrimary: s.controller.secrets.primary,
        onCopy: s.controller.secrets.copy,
      },
      global: { stubs: { RouterLink: { template: '<a><slot /></a>' } } },
    });
    try {
      const query = '  Same secret name  ';
      await wrapper.get<HTMLInputElement>('#secrets-search-input').setValue(query);
      await flushPromises();
      expect(wrapper.find('[data-secret-id="pass-a"]').exists()).toBe(true);
      const show = wrapper.get('[data-testid="secret-primary-action-Same secret name"]');
      expect(show.attributes('disabled')).toBeUndefined();
      expect(wrapper.get<HTMLInputElement>('#secrets-search-input').element.value).toBe(query);
      expect(s.controller.secrets.state.value.presentation.query).toBe(query);
      await show.trigger('click');
      await flushPromises();
      expect(s.context.manager.show_local_secret).toHaveBeenCalledOnce();
      expect(shownPasses).toEqual(['pass-a']);
      expect(s.controller.secrets.state.value.presentation.visibleIds).toEqual(['pass-a']);
      expect(wrapper.text()).toContain('private marker');
      await wrapper.get('[data-secret-action="copy"]').trigger('click');
      await flushPromises();
      expect(s.platform.copy).toHaveBeenCalledExactlyOnceWith('private marker');
      expect(s.controller.secrets.state.value.cards['pass-a'].copied).toBe(true);
      expect(s.controller.secrets.state.value.presentation.query).toBe(query);
      expect(s.context.manager.recover_checked).not.toHaveBeenCalled();
      expect(s.context.manager.delete_secret_checked).not.toHaveBeenCalled();
    } finally {
      wrapper.unmount();
      s.controller.dispose();
    }
  });
});
