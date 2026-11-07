import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushPromises, mount } from '@vue/test-utils';
import { createMemoryHistory, createRouter } from 'vue-router';
import SecretCard from '@/components/liveboard/SecretCard.vue';
import NotificationTile from '@/components/liveboard/NotificationTile.vue';
import AnimatedIcon from '@/components/liveboard/AnimatedIcon.vue';
import SvgPrimitive from '@/components/liveboard/SvgPrimitive.vue';
import LiveboardHome from '@/components/liveboard/LiveboardHome.vue';
import type { NotificationView, SecretCardView } from '@/features/liveboard/types';
import type { VaultPresentation } from '@/features/liveboard/presentation';
import { request, secret } from './liveboardFixtures';

beforeEach(() =>
  vi.stubGlobal('matchMedia', () => ({ matches: true, addEventListener: vi.fn(), removeEventListener: vi.fn() })),
);
afterEach(() => vi.unstubAllGlobals());
const empty = (): NotificationView => ({ requests: [], selected: null, expanded: false, busy: false, error: null });
const masked = (): SecretCardView => ({ secret: secret(), phase: 'masked', value: null, copied: false });

describe('Liveboard actual component safety and interaction seams', () => {
  it('renders exactly three compact cipher rows and no concealed plaintext or Copy control', () => {
    const wrapper = mount(SecretCard, { props: { card: masked(), index: 0, blocked: false } });
    const circles = wrapper.findAll('.secret-cipher circle');
    expect(circles).toHaveLength(135);
    expect(new Set(circles.map((circle) => circle.attributes('cy')))).toEqual(new Set(['9', '14', '19']));
    expect(wrapper.find('.secret-value').exists()).toBe(false);
    expect(wrapper.find('[data-secret-action="copy"]').exists()).toBe(false);
    wrapper.unmount();
  });
  it('escapes actual inline plaintext and drops it and Copy when the same card is masked', async () => {
    const text = '<img src=x onerror=alert(1)>';
    const wrapper = mount(SecretCard, {
      props: { card: { ...masked(), phase: 'revealed', value: { kind: 'text', text } }, index: 0, blocked: false },
    });
    expect(wrapper.find('.secret-plain').text()).toBe(text);
    expect(wrapper.find('img').exists()).toBe(false);
    expect(wrapper.find('[data-secret-action="copy"]').exists()).toBe(true);
    await wrapper.setProps({ card: masked() });
    expect(wrapper.text()).not.toContain(text);
    expect(wrapper.find('.secret-value').exists()).toBe(false);
    expect(wrapper.find('[data-secret-action="copy"]').exists()).toBe(false);
    wrapper.unmount();
  });
  it('honors the Core-disabled branch without emitting an outgoing intent', async () => {
    const wrapper = mount(SecretCard, {
      props: {
        card: { ...masked(), secret: secret('pass-a', { primary: { kind: 'blocked' }, canDelete: false }) },
        index: 0,
        blocked: false,
      },
    });
    await wrapper.get('[data-testid="secret-primary-action-Same secret name"]').trigger('click');
    expect(wrapper.emitted('primary')).toBeUndefined();
    expect(wrapper.get('[data-testid="delete-secret-Same secret name"]').attributes('disabled')).toBeDefined();
    wrapper.unmount();
  });
  it('error overlays retain the queue and only emit manual dismissal', async () => {
    const view = { ...empty(), requests: [request(), request('recover', 'claim-b')], error: 'Could not sync Vault.' };
    const wrapper = mount(NotificationTile, { props: { view, ...{ ready: false } } });
    expect(wrapper.find('.notification-icon.is-error').exists()).toBe(true);
    expect(wrapper.findAll('.notice-control')).toHaveLength(0);
    await wrapper.get('button.notification-hit').trigger('click');
    expect(wrapper.emitted('dismissError')).toHaveLength(1);
    expect(view.requests).toHaveLength(2);
    expect(view.error).toBe('Could not sync Vault.');
    wrapper.unmount();
  });
  it('expanded requests keep their exact device and subject above real round controls', async () => {
    const item = request('delete-secret', 'exact-delete', {
      requesterName: '<b>device</b>',
      subjectName: 'Wallet seed',
    });
    const wrapper = mount(NotificationTile, {
      props: { view: { ...empty(), requests: [item], selected: item, expanded: true }, ...{ ready: true } },
    });
    expect(wrapper.find('.request-device').text()).toBe('<b>device</b>');
    expect(wrapper.find('b').exists()).toBe(false);
    expect(wrapper.find('.request-subject').text()).toBe('Wallet seed');
    await wrapper.get('[data-testid="decline-delete-secret-request"]').trigger('click');
    expect(wrapper.emitted('decide')).toEqual([[false]]);
    expect(wrapper.get('.notification-actions').attributes('data-request-id')).toBe('exact-delete');
    wrapper.unmount();
  });
  it('uses package viewBox, dotted bell stroke and svg-target animation classes', () => {
    const bell = mount(AnimatedIcon, { props: { name: 'bell' } });
    expect(bell.attributes('viewBox')).toBe('0 0 32 32');
    expect(bell.find('path').attributes('stroke-dasharray')).toBe('.2 2.3');
    const create = mount(AnimatedIcon, { props: { name: 'create' } });
    expect(create.classes()).toContain('create-plus');
    bell.unmount();
    create.unmount();
  });
  it('rejects unsafe element names and attributes even at the bundled SVG renderer boundary', () => {
    const unsafe = mount(SvgPrimitive, { props: { primitive: { type: 'script', d: 'not rendered' } } });
    expect(unsafe.find('script').exists()).toBe(false);
    const path = mount(SvgPrimitive, {
      props: { primitive: { type: 'path', d: 'M0 0', onload: 'forbidden', href: 'forbidden', style: 'forbidden' } },
    });
    expect(path.attributes()).toEqual({ d: 'M0 0' });
    unsafe.unmount();
    path.unmount();
  });
  it('navigates through actual RouterLinks without replacing the mounted session presentation', async () => {
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [{ path: '/:pathMatch(.*)*', component: { render: () => null } }],
    });
    await router.push('/liveboard');
    await router.isReady();
    const vault: VaultPresentation = {
      slots: [
        { slotId: 'one', heading: 'DEVICE01', art: 'placeholder', name: 'Not connected', state: '' },
        { slotId: 'two', heading: 'DEVICE02', art: 'placeholder', name: 'Not connected', state: '' },
        { slotId: 'three', heading: 'DEVICE03', art: 'placeholder', name: 'Not connected', state: '' },
      ],
      storage: {
        label: 'Storage model',
        title: 'Core wording',
        detail: '',
        preview: false,
        removable: false,
        busy: false,
      },
      request: null,
      canInvite: true,
    };
    const wrapper = mount(LiveboardHome, {
      props: {
        vault,
        secretCount: 2,
        notifications: empty(),
        secretsHref: '/secrets',
        profileHref: '/profile',
        ...{ notificationsReady: true },
      },
      global: { plugins: [router] },
    });
    const element = wrapper.element;
    await wrapper.get('[data-testid="liveboard-secrets-open"]').trigger('click');
    await flushPromises();
    expect(router.currentRoute.value.path).toBe('/secrets');
    expect(wrapper.element).toBe(element);
    await router.back();
    await flushPromises();
    expect(wrapper.element).toBe(element);
    wrapper.unmount();
  });

  it('disables the actual Create button and emits no intent when Core canCreate is false', async () => {
    const vault: VaultPresentation = {
      slots: [
        { slotId: 'one', heading: 'DEVICE01', art: 'placeholder', name: 'Not connected', state: '' },
        { slotId: 'two', heading: 'DEVICE02', art: 'placeholder', name: 'Not connected', state: '' },
        { slotId: 'three', heading: 'DEVICE03', art: 'placeholder', name: 'Not connected', state: '' },
      ],
      storage: {
        label: 'Storage model',
        title: 'Core wording',
        detail: '',
        preview: false,
        removable: false,
        busy: false,
      },
      request: null,
      canInvite: true,
    };
    const wrapper = mount(LiveboardHome, {
      props: {
        vault,
        secretCount: 0,
        notifications: empty(),
        secretsHref: '/secrets',
        profileHref: '/profile',
        canCreate: false,
        ...{ notificationsReady: true },
      },
      global: { stubs: { RouterLink: { template: '<a><slot /></a>' } } },
    });
    try {
      const create = wrapper.get('[data-testid="create-secret-open"]');
      expect.soft(create.attributes('disabled')).toBeDefined();
      await create.trigger('click');
      expect(wrapper.emitted('create')).toBeUndefined();
    } finally {
      wrapper.unmount();
    }
  });
});
