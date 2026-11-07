import { describe, expect, it } from 'vitest';
import { flushPromises, mount } from '@vue/test-utils';
import SecretsPage from '@/components/liveboard/SecretsPage.vue';
import type { SecretView } from '@/features/liveboard/types';

describe('Secrets route presentation survives a real component remount', () => {
  it('restores the supplied scrollTop on its new owned container while retaining the query', async () => {
    const view: SecretView = {
      cards: {},
      presentation: { active: true, visibleIds: [], query: 'Wallet', scrollTop: 281 },
      blocked: false,
    };
    const render = () =>
      mount(SecretsPage, {
        props: { view, backHref: '/' },
        global: { stubs: { RouterLink: { template: '<a><slot /></a>' } } },
      });
    let wrapper = render();
    try {
      await flushPromises();
      const previous = wrapper.get<HTMLElement>('.secrets-list-scroll').element;
      previous.scrollTop = 281;
      await wrapper.get('.secrets-list-scroll').trigger('scroll');
      expect(wrapper.emitted('scroll')).toEqual([[281]]);
      expect(wrapper.get<HTMLInputElement>('#secrets-search-input').element.value).toBe('Wallet');
      wrapper.unmount();
      wrapper = render();
      await flushPromises();
      const current = wrapper.get<HTMLElement>('.secrets-list-scroll').element;
      expect(current).not.toBe(previous);
      expect(wrapper.get<HTMLInputElement>('#secrets-search-input').element.value).toBe('Wallet');
      expect(current.scrollTop).toBe(281);
      expect(view.presentation).toMatchObject({ query: 'Wallet', scrollTop: 281 });
    } finally {
      wrapper.unmount();
    }
  });
});
