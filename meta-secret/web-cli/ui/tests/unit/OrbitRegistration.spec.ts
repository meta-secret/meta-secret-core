import { mount, flushPromises } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import OrbitRegistration from '@/components/registration/OrbitRegistration.vue';
import type { RegistrationPort } from '@/features/registration/registrationController';
import * as emailProviders from '@/features/registration/emailProvider';

function makePort(kind: 'local' | 'available' | 'exists' | 'pending' | 'declined' = 'local'): RegistrationPort {
  return {
    snapshot: () => ({ kind, name: kind === 'local' ? '' : 'person@example.com' }),
    prepare: vi.fn().mockResolvedValue({ kind: 'exists', name: 'person@example.com' }),
    submit: vi.fn().mockResolvedValue({ kind: 'pending', name: 'person@example.com' }),
    refresh: vi.fn().mockResolvedValue({ kind: 'pending', name: 'person@example.com' }),
    sync: vi.fn().mockResolvedValue(undefined),
    reset: vi.fn().mockResolvedValue(undefined),
    observe: vi.fn(),
    stopObserving: vi.fn(),
  };
}

beforeEach(() => {
  vi.stubGlobal('matchMedia', () => ({ matches: true }));
});
afterEach(() => {
  vi.unstubAllGlobals();
});

describe('Orbit registration presentation', () => {
  it('keeps available Join disabled, motionless and unable to navigate', async () => {
    const port = makePort('available');
    const wrapper = mount(OrbitRegistration, { props: { port } });
    await flushPromises();
    const join = wrapper.get('button[aria-label="Join"]');
    expect(join.attributes('disabled')).toBeDefined();
    expect(join.classes()).not.toContain('has-motion');
    await join.trigger('click');
    expect(wrapper.attributes('data-step')).toBe('2');
    expect(wrapper.attributes('data-phase')).toBe('available');
    expect(port.submit).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it('cancels queued layout measurement and ignores its late callback after unmount', async () => {
    let frame!: FrameRequestCallback;
    const queue = vi.fn((callback: FrameRequestCallback) => {
      frame = callback;
      return 41;
    });
    const cancel = vi.fn();
    vi.stubGlobal('requestAnimationFrame', queue);
    vi.stubGlobal('cancelAnimationFrame', cancel);
    const wrapper = mount(OrbitRegistration, { props: { port: makePort() } });
    await flushPromises();
    expect(queue).toHaveBeenCalled();
    wrapper.unmount();
    expect(cancel).toHaveBeenCalledWith(41);
    expect(() => frame(0)).not.toThrow();
  });
  it('exposes no Join while checking and keeps a failed check disabled and motionless', async () => {
    let fail!: (error: Error) => void;
    const port = makePort();
    port.prepare = vi.fn<RegistrationPort['prepare']>(
      () =>
        new Promise((_resolve, reject) => {
          fail = reject;
        }),
    );
    const wrapper = mount(OrbitRegistration, { props: { port } });
    await flushPromises();
    await wrapper.get('button[aria-label="Enter your email"]').trigger('click');
    await wrapper.get('input').setValue('person@example.com');
    await wrapper.get('button[aria-label="OK"]').trigger('click');
    await flushPromises();
    expect(wrapper.attributes('data-phase')).toBe('checking');
    expect(wrapper.find('button[aria-label="Join"]').exists()).toBe(false);
    fail(new Error('offline'));
    await flushPromises();
    const join = wrapper.get('button[aria-label="Join"]');
    expect(join.attributes('disabled')).toBeDefined();
    expect(join.classes()).not.toContain('has-motion');
    expect(port.submit).not.toHaveBeenCalled();
    wrapper.unmount();
  });
  it('aborts Apple selection on unmount and ignores its late email', async () => {
    const previous = emailProviders.emailProviderCapabilities.apple;
    emailProviders.emailProviderCapabilities.apple = true;
    let finish!: (email: string) => void;
    let signal: AbortSignal | undefined;
    const select = vi.spyOn(emailProviders, 'selectAppleEmail').mockImplementation((value) => {
      signal = value;
      return new Promise<string>((resolve) => {
        finish = resolve;
      });
    });
    const port = makePort();
    const wrapper = mount(OrbitRegistration, { props: { port } });
    try {
      await flushPromises();
      await wrapper.get('button[aria-label="Use Apple email"]').trigger('click');
      expect(signal?.aborted).toBe(false);
      wrapper.unmount();
      expect(signal?.aborted).toBe(true);
      finish('Late@privaterelay.appleid.com');
      await flushPromises();
      expect(port.prepare).not.toHaveBeenCalled();
    } finally {
      select.mockRestore();
      emailProviders.emailProviderCapabilities.apple = previous;
    }
  });
  it('starts on01 without launching an authorization dialog', async () => {
    const wrapper = mount(OrbitRegistration, { props: { port: makePort() } });
    await flushPromises();
    expect(wrapper.get('h1').text()).toBe('MetaSecret');
    expect(wrapper.text()).toContain('Your network of trust');
    expect(wrapper.find('[role="dialog"]').exists()).toBe(false);
    expect(wrapper.find('button[aria-label="Enter your email"]').exists()).toBe(true);
    wrapper.unmount();
  });
  it('renders taken name with a disabled motionless Create and opens04', async () => {
    const port = makePort('exists');
    const wrapper = mount(OrbitRegistration, { props: { port } });
    await flushPromises();
    expect(wrapper.text()).toContain('This name is taken');
    expect(wrapper.get('button[aria-label="Create"]').attributes('disabled')).toBeDefined();
    await wrapper.get('button[aria-label="Join"]').trigger('click');
    expect(wrapper.attributes('data-step')).toBe('4');
    expect(wrapper.text()).toContain('This Vault exists. Join to continue.');
    expect(wrapper.findAll('button').map((button) => button.attributes('aria-label'))).toEqual([
      'QR',
      'Back',
      'Request',
    ]);
    expect(port.submit).not.toHaveBeenCalled();
    wrapper.unmount();
  });
  it('keeps declined on04 with Back and no active loader', async () => {
    const wrapper = mount(OrbitRegistration, { props: { port: makePort('declined') } });
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toBe('Request declined.');
    expect(wrapper.find('.orbit-loader').exists()).toBe(false);
    expect(wrapper.find('button[aria-label="Back"]').exists()).toBe(true);
    wrapper.unmount();
  });
});

describe('Timed Create failure', () => {
  it('shows only red status without controls or loader for3seconds, then restores02', async () => {
    vi.useFakeTimers();
    try {
      const port = makePort('local');
      port.prepare = vi.fn().mockResolvedValue({ kind: 'available', name: 'person@example.com' });
      port.snapshot = () => ({ kind: 'available', name: 'person@example.com' });
      port.submit = vi.fn().mockRejectedValue(new Error('network'));
      port.refresh = vi.fn().mockResolvedValue({ kind: 'available', name: 'person@example.com' });
      const wrapper = mount(OrbitRegistration, { props: { port } });
      await flushPromises();
      await wrapper.get('button[aria-label="Create"]').trigger('click');
      await flushPromises();
      expect(wrapper.get('[role="alert"]').text()).toBe('Network error. Try again.');
      expect(wrapper.findAll('button')).toHaveLength(0);
      expect(wrapper.find('.orbit-loader').exists()).toBe(false);
      await vi.advanceTimersByTimeAsync(2999);
      expect(wrapper.findAll('button')).toHaveLength(0);
      await vi.advanceTimersByTimeAsync(1);
      expect(wrapper.get('button[aria-label="Create"]').attributes('disabled')).toBeUndefined();
      expect(wrapper.text()).toContain('This name is available');
      wrapper.unmount();
    } finally {
      vi.useRealTimers();
    }
  });
});

describe('QR camera capability', () => {
  function browser(userAgent: string) {
    const media = Object.assign(new EventTarget(), {
      enumerateDevices: vi.fn().mockResolvedValue([{ kind: 'videoinput' }]),
      getUserMedia: vi.fn(),
    });
    vi.stubGlobal('isSecureContext', true);
    vi.stubGlobal('navigator', { userAgent, platform: 'test', maxTouchPoints: 5, mediaDevices: media });
    return media;
  }

  it('keeps desktop QR disabled and blocks a forced click without starting a scanner', async () => {
    const media = browser('Mozilla/5.0 (Windows NT 10.0; Win64; x64)');
    const cameraFactory = vi.fn();
    const wrapper = mount(OrbitRegistration, { props: { port: makePort('exists'), cameraFactory } });
    await flushPromises();
    await wrapper.get('button[aria-label="Join"]').trigger('click');
    expect(wrapper.get('button[aria-label="QR"]').attributes('disabled')).toBeDefined();
    await wrapper.get('button[aria-label="QR"]').trigger('click');
    await flushPromises();
    expect(wrapper.find('[role="dialog"]').exists()).toBe(false);
    expect(media.enumerateDevices).not.toHaveBeenCalled();
    expect(media.getUserMedia).not.toHaveBeenCalled();
    expect(cameraFactory).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it('allows a phone camera, then disables QR when its camera disappears', async () => {
    const media = browser('Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X)');
    const start = vi.fn().mockResolvedValue(undefined);
    const stop = vi.fn();
    const cameraFactory = vi.fn(() => ({ start, stop }));
    const wrapper = mount(OrbitRegistration, { props: { port: makePort('exists'), cameraFactory } });
    await flushPromises();
    await wrapper.get('button[aria-label="Join"]').trigger('click');
    expect(wrapper.get('button[aria-label="QR"]').attributes('disabled')).toBeUndefined();
    expect(media.getUserMedia).not.toHaveBeenCalled();
    expect(start).not.toHaveBeenCalled();
    await wrapper.get('button[aria-label="QR"]').trigger('click');
    await flushPromises();
    expect(start).toHaveBeenCalledOnce();
    await wrapper.get('[role="dialog"] button[aria-label="Back"]').trigger('click');
    await flushPromises();
    expect(stop).toHaveBeenCalledOnce();
    media.enumerateDevices.mockResolvedValue([]);
    media.dispatchEvent(new Event('devicechange'));
    await flushPromises();
    expect(wrapper.get('button[aria-label="QR"]').attributes('disabled')).toBeDefined();
    wrapper.unmount();
    const calls = media.enumerateDevices.mock.calls.length;
    media.dispatchEvent(new Event('devicechange'));
    await flushPromises();
    expect(media.enumerateDevices).toHaveBeenCalledTimes(calls);
  });

  it('does not enable QR from a late obsolete camera probe', async () => {
    const media = browser('iPhone');
    let finishFirst!: (devices: { kind: string }[]) => void;
    media.enumerateDevices.mockReturnValueOnce(
      new Promise((resolve) => {
        finishFirst = resolve;
      }),
    );
    const wrapper = mount(OrbitRegistration, { props: { port: makePort('exists') } });
    await flushPromises();
    await wrapper.get('button[aria-label="Join"]').trigger('click');
    media.enumerateDevices.mockResolvedValue([]);
    media.dispatchEvent(new Event('devicechange'));
    await flushPromises();
    finishFirst([{ kind: 'videoinput' }]);
    await flushPromises();
    expect(wrapper.get('button[aria-label="QR"]').attributes('disabled')).toBeDefined();
    wrapper.unmount();
  });
});

describe('Incomplete Back destination', () => {
  it.each(['pending', 'declined'] as const)('shows provider01 after %s cleanup with no old email', async (initial) => {
    const port = makePort(initial);
    let cleared = false;
    port.snapshot = () => (cleared ? { kind: 'local', name: '' } : { kind: initial, name: 'person@example.com' });
    port.reset = vi.fn(async () => {
      cleared = true;
    });
    const wrapper = mount(OrbitRegistration, { props: { port } });
    await flushPromises();
    if (initial === 'declined') expect(wrapper.get('[role="alert"]').text()).toBe('Request declined.');
    await wrapper.get('button[aria-label="Back"]').trigger('click');
    await flushPromises();
    expect(wrapper.attributes('data-step')).toBe('1');
    expect(wrapper.get('h1').text()).toBe('MetaSecret');
    expect(wrapper.text()).not.toContain('person@example.com');
    expect(wrapper.find('button[aria-label="Enter your email"]').exists()).toBe(true);
    expect(port.reset).toHaveBeenCalledOnce();
    wrapper.unmount();
  });
});
