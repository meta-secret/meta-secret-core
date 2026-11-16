import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import VaultTile from '@/components/liveboard/VaultTile.vue';
import type { VaultPresentation } from '@/features/liveboard/presentation';

beforeEach(() =>
  vi.stubGlobal('matchMedia', () => ({
    matches: true,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  })),
);
afterEach(() => vi.unstubAllGlobals());

describe('Liveboard pending removal retains its distinct approved controls', () => {
  it('shows the request links and disabled Current/Remove clock together until canonical settlement', () => {
    const vault: VaultPresentation = {
      slots: [
        { slotId: 'self', heading: 'THIS DEVICE', art: 'phone', name: 'Phone', state: 'Awaiting approval' },
        {
          slotId: 'target',
          heading: 'DEVICE02',
          art: 'tablet',
          name: 'Tablet',
          state: 'Selected for removal',
          removed: true,
        },
        { slotId: 'peer', heading: 'DEVICE03', art: 'laptop', name: 'Desktop', state: 'Approval needed' },
      ],
      storage: {
        label: 'Time Machine',
        title: 'Full encrypted copies',
        detail: 'Core forecast',
        preview: true,
        removable: true,
        busy: true,
      },
      request: {
        identity: 'exact-remove',
        kind: 'remove-device',
        phase: 'waiting',
        requesterId: 'self',
        peerIds: ['peer'],
        title: 'REMOVE DEVICE REQUEST',
        subject: 'Tablet',
        detail: 'Waiting for Desktop',
      },
      canInvite: false,
    };
    const wrapper = mount(VaultTile, { props: { vault } });
    try {
      expect(wrapper.find('.recovery-links').exists()).toBe(true);
      expect(wrapper.find('.vault-storage-actions').exists()).toBe(true);
      const current = wrapper.findAll('button').find((button) => button.text() === 'Current');
      expect(current?.attributes('disabled')).toBeDefined();
      const remove = wrapper.get('[data-testid="remove-device"]');
      expect(remove.attributes('disabled')).toBeDefined();
      expect(remove.find('svg').attributes('data-icon')).toBe('1-9');
    } finally {
      wrapper.unmount();
    }
  });
});

const recoveringVault = (): VaultPresentation => ({
  slots: [
    { slotId: 'self', heading: 'THIS DEVICE', art: 'phone', name: 'Phone', state: 'This device', disabled: true },
    { slotId: 'target', heading: 'DEVICE02', art: 'tablet', name: 'Tablet', state: 'Connected', preview: true },
    { slotId: 'peer', heading: 'DEVICE03', art: 'laptop', name: 'Desktop', state: 'Connected' },
  ],
  storage: {
    label: 'Time Machine',
    title: 'Full encrypted copies',
    detail: 'Core forecast',
    preview: true,
    removable: true,
    busy: false,
  },
  request: {
    identity: 'exact-recover',
    kind: 'recover',
    phase: 'waiting',
    requesterId: 'self',
    peerIds: ['target', 'peer'],
    title: 'RECOVERY',
    subject: 'Fixture subject',
    detail: 'Waiting',
  },
  canInvite: true,
});

describe('Active Recover retains canonical device preview controls', () => {
  it('keeps recovery identity, links and status while emitting real Remove then Current intents only', async () => {
    const vault = recoveringVault();
    const original = JSON.stringify(vault);
    const wrapper = mount(VaultTile, { props: { vault } });
    try {
      expect(wrapper.find('.recovery-links').exists()).toBe(true);
      expect(wrapper.get('.vault-recovery').text()).toContain('Fixture subject');
      await wrapper.get('[data-testid="remove-device"]').trigger('click');
      const current = wrapper.findAll('button').find((button) => button.text() === 'Current');
      expect(current).toBeDefined();
      await current!.trigger('click');
      expect(wrapper.emitted('remove')).toEqual([[]]);
      expect(wrapper.emitted('current')).toEqual([[]]);
      expect(JSON.stringify(vault)).toBe(original);
      expect(wrapper.find('.recovery-links').exists()).toBe(true);
      expect(wrapper.get('.vault-recovery').text()).toContain('Fixture subject');
      await wrapper.setProps({ vault: { ...vault, storage: { ...vault.storage, preview: false, removable: false } } });
      expect(wrapper.find('.storage-info').exists()).toBe(false);
      expect(wrapper.find('.recovery-links').exists()).toBe(true);
      expect(wrapper.get('.vault-recovery').text()).toContain('Fixture subject');
    } finally {
      wrapper.unmount();
    }
  });
  it('keeps the original Recover view without storage when the preview is closed', () => {
    const vault = recoveringVault();
    vault.storage.preview = false;
    vault.storage.removable = false;
    const wrapper = mount(VaultTile, { props: { vault } });
    try {
      expect(wrapper.find('.storage-info').exists()).toBe(false);
      expect(wrapper.find('.vault-storage-actions').exists()).toBe(false);
      expect(wrapper.find('[data-testid="remove-device"]').exists()).toBe(false);
      expect(wrapper.find('.vault-recovery').exists()).toBe(true);
    } finally {
      wrapper.unmount();
    }
  });
  it('does not invent Remove when canonical presentation denies removal', () => {
    const vault = recoveringVault();
    vault.storage.removable = false;
    const wrapper = mount(VaultTile, { props: { vault } });
    try {
      expect(wrapper.find('.vault-storage-actions').exists()).toBe(true);
      expect(wrapper.find('[data-testid="remove-device"]').exists()).toBe(false);
      expect(wrapper.emitted('remove')).toBeUndefined();
    } finally {
      wrapper.unmount();
    }
  });
  it('preserves busy and dead-readiness disabled controls without emitting removal', async () => {
    const vault = recoveringVault();
    vault.storage.busy = true;
    const wrapper = mount(VaultTile, { props: { vault } });
    try {
      const remove = wrapper.get('[data-testid="remove-device"]');
      expect(remove.attributes('disabled')).toBeDefined();
      await remove.trigger('click');
      expect(wrapper.emitted('remove')).toBeUndefined();
      expect(wrapper.find('.recovery-links').exists()).toBe(true);
    } finally {
      wrapper.unmount();
    }
  });
  it('retains the previous non-Recover request visibility policy', () => {
    const vault = recoveringVault();
    vault.request!.kind = 'delete-secret';
    const wrapper = mount(VaultTile, { props: { vault } });
    try {
      expect(wrapper.find('.storage-info').exists()).toBe(false);
      expect(wrapper.find('.vault-recovery').exists()).toBe(true);
    } finally {
      wrapper.unmount();
    }
  });
  it('retains idle storage and keeps the current-device slot disabled', async () => {
    const vault = recoveringVault();
    vault.request = null;
    const wrapper = mount(VaultTile, { props: { vault } });
    try {
      expect(wrapper.find('.storage-info').exists()).toBe(true);
      await wrapper.get('[data-slot-id="self"]').trigger('click');
      expect(wrapper.emitted('select')).toBeUndefined();
    } finally {
      wrapper.unmount();
    }
  });
});
