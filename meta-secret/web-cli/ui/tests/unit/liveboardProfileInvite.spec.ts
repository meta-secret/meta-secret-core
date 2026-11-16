import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushPromises, mount } from '@vue/test-utils';
import { createCoreAdapter } from '@/features/liveboard/coreAdapter';
import { createLiveboardController } from '@/features/liveboard/controller';
import InviteQrDialog from '@/components/liveboard/InviteQrDialog.vue';
import ProfilePage from '@/components/liveboard/ProfilePage.vue';
import { contextFixture, coreState, deferred, platformFixture } from './liveboardCoreFixtures';

const qr = vi.hoisted(() => ({ options: [] as Array<{ data: string }>, append: vi.fn() }));
vi.mock('qr-code-styling', () => ({
  default: class {
    constructor(options: { data: string }) {
      qr.options.push(options);
    }
    async getRawData() {
      return new Blob();
    }
    append(host: HTMLElement) {
      host.append(document.createElement('svg'));
      qr.append(host);
    }
  },
}));
vi.mock('@/features/liveboard/inviteMotion', () => ({
  createInviteMotion: () => ({
    open: async () => {},
    close: async () => {},
    cancel: vi.fn(),
    fit: vi.fn(),
  }),
}));
const showModal = Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, 'showModal');
const close = Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, 'close');
beforeEach(() => {
  Object.defineProperty(HTMLDialogElement.prototype, 'showModal', {
    configurable: true,
    value(this: HTMLDialogElement) {
      this.open = true;
    },
  });
  Object.defineProperty(HTMLDialogElement.prototype, 'close', {
    configurable: true,
    value(this: HTMLDialogElement) {
      this.open = false;
    },
  });
});
afterEach(() => {
  if (showModal) Object.defineProperty(HTMLDialogElement.prototype, 'showModal', showModal);
  else Reflect.deleteProperty(HTMLDialogElement.prototype, 'showModal');
  if (close) Object.defineProperty(HTMLDialogElement.prototype, 'close', close);
  else Reflect.deleteProperty(HTMLDialogElement.prototype, 'close');
  qr.options.length = 0;
  vi.clearAllMocks();
});
function setup() {
  const context = contextFixture(JSON.stringify(coreState({ operations: [] })));
  const platform = platformFixture();
  const resetLocal = vi.fn(async () => {});
  const adapter = createCoreAdapter({ getContext: () => context, platform });
  const controller = createLiveboardController({ adapter, resetLocal, version: '2.0.0-real-build' });
  controller.refresh();
  return { context, platform, resetLocal, controller };
}

describe('Liveboard real canonical Profile, invitation and confirmed local reset', () => {
  it('passes canonical Vault UTF-8 bytes directly to the actual QR component without a generation delay', async () => {
    const s = setup();
    const anchor = document.createElement('button');
    s.controller.invite(anchor);
    const wrapper = mount(InviteQrDialog, {
      props: {
        open: s.controller.state.value.inviteOpen,
        anchor,
        vaultName: s.controller.state.value.profile.vaultName,
      },
    });
    await flushPromises();
    const bytes = Array.from(new TextEncoder().encode('семья@example.test'), (byte) => String.fromCharCode(byte)).join(
      '',
    );
    expect(qr.options).toHaveLength(1);
    expect(qr.options[0]?.data).toBe(bytes);
    expect(qr.append).toHaveBeenCalledOnce();
    vi.mocked(s.context.member.liveboard_state_json).mockReturnValue(JSON.stringify(coreState({ vaultName: '' })));
    s.controller.refresh();
    expect(s.controller.state.value.inviteOpen).toBe(false);
    await wrapper.setProps({ open: false, vaultName: s.controller.state.value.profile.vaultName });
    expect(wrapper.find('.invite-code svg').exists()).toBe(false);
    wrapper.unmount();
    s.controller.dispose();
  });

  it('renders canonical counts and injected actual build version as escaped text', () => {
    const s = setup();
    const profile = s.controller.state.value.profile;
    expect(profile).toMatchObject({
      vaultName: 'семья@example.test',
      deviceCount: 2,
      secretCount: 1,
      version: '2.0.0-real-build',
    });
    const wrapper = mount(ProfilePage, {
      props: { profile: { ...profile, vaultName: '<img src=x onerror=marker>' }, backHref: '/' },
      global: { stubs: { RouterLink: { template: '<a><slot /></a>' } } },
    });
    expect(wrapper.text()).toContain('<img src=x onerror=marker>');
    expect(wrapper.find('.profile-vault-name img').exists()).toBe(false);
    expect(wrapper.get('#profile-version').text()).toBe('2.0.0-real-build');
    wrapper.unmount();
    s.controller.dispose();
  });

  it('requires confirmation, clears session/draft before awaiting one existing local cleanup, and never adds auth or remote reset', async () => {
    const s = setup();
    s.controller.navigate('secrets');
    await s.controller.secrets.primary('pass-a');
    const held = s.controller.secrets.state.value.cards['pass-a'];
    s.controller.create.open();
    s.controller.create.change({ name: 'draft', value: 'private draft' });
    s.controller.requestReset();
    expect(s.resetLocal).not.toHaveBeenCalled();
    s.controller.cancelReset();
    expect(s.resetLocal).not.toHaveBeenCalled();
    const cleanup = deferred<void>();
    s.resetLocal.mockReturnValue(cleanup.promise);
    s.controller.requestReset();
    const reset = s.controller.confirmReset();
    const duplicate = s.controller.confirmReset();
    expect(held.value).toBeNull();
    expect(s.controller.create.state.value.draft.value).toBe('');
    expect(s.controller.state.value.resetBusy).toBe(true);
    expect(s.resetLocal).toHaveBeenCalledOnce();
    expect(s.platform.authenticate).not.toHaveBeenCalled();
    expect(s.context.manager.generate_user_creds).not.toHaveBeenCalled();
    expect(s.context.manager.sign_up).not.toHaveBeenCalled();
    expect(s.context.manager.delete_secret_checked).not.toHaveBeenCalled();
    cleanup.resolve();
    await Promise.all([reset, duplicate]);
    expect(s.controller.state.value.resetOpen).toBe(false);
    expect(s.controller.secrets.state.value.blocked).toBe(true);
    s.controller.dispose();
  });
});
