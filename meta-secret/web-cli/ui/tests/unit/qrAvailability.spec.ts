import { describe, expect, it, vi } from 'vitest';
import { readWebQrAvailability, type WebQrNavigator } from '@/features/registration/qrAvailability';

function device(userAgent: string, platform = 'Linux armv8l', maxTouchPoints = 5) {
  const enumerateDevices = vi.fn().mockResolvedValue([{ kind: 'videoinput' }]);
  const getUserMedia = vi.fn();
  const nav: WebQrNavigator = { userAgent, platform, maxTouchPoints, mediaDevices: { enumerateDevices, getUserMedia } };
  return { nav, enumerateDevices, getUserMedia };
}

describe('Web QR device policy', () => {
  it.each([
    ['MacBook', 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)', 'MacIntel', 0],
    ['Windows touchscreen computer', 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)', 'Win32', 10],
    ['Linux desktop', 'Mozilla/5.0 (X11; Linux x86_64)', 'Linux x86_64', 0],
  ])('disables %s without probing or opening the camera', async (_name, agent, platform, touch) => {
    const env = device(agent, platform, touch);
    expect(await readWebQrAvailability(env.nav, true)).toBe(false);
    expect(env.enumerateDevices).not.toHaveBeenCalled();
    expect(env.getUserMedia).not.toHaveBeenCalled();
  });

  it.each([
    ['iPhone', 'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X)', 'iPhone'],
    ['iPad', 'Mozilla/5.0 (iPad; CPU OS 18_0 like Mac OS X)', 'iPad'],
    ['Android phone', 'Mozilla/5.0 (Linux; Android 16; Phone) Mobile', 'Linux armv8l'],
    ['Android tablet', 'Mozilla/5.0 (Linux; Android 16; Tablet)', 'Linux armv8l'],
    ['iPad desktop mode', 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15)', 'MacIntel'],
  ])('allows %s with a camera without requesting permission', async (_name, agent, platform) => {
    const env = device(agent, platform);
    expect(await readWebQrAvailability(env.nav, true)).toBe(true);
    expect(env.enumerateDevices).toHaveBeenCalledOnce();
    expect(env.getUserMedia).not.toHaveBeenCalled();
  });

  it('uses an explicit Tablet hint even with a desktop user agent', async () => {
    const env = device('Mozilla/5.0 (Windows NT 10.0)', 'Win32');
    env.nav.userAgentData = { getHighEntropyValues: vi.fn().mockResolvedValue({ formFactors: ['Tablet'] }) };
    expect(await readWebQrAvailability(env.nav, true)).toBe(true);
  });

  it('keeps explicit Desktop hints disabled even if mobile hints conflict', async () => {
    const env = device('Mozilla/5.0 (Android 16)');
    env.nav.userAgentData = {
      mobile: true,
      getHighEntropyValues: vi.fn().mockResolvedValue({ formFactors: ['Desktop'] }),
    };
    expect(await readWebQrAvailability(env.nav, true)).toBe(false);
    expect(env.enumerateDevices).not.toHaveBeenCalled();
  });

  it('falls back to iPad detection when form-factor hints are blocked', async () => {
    const env = device('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15)', 'MacIntel');
    env.nav.userAgentData = { getHighEntropyValues: vi.fn().mockRejectedValue(new Error('unavailable')) };
    expect(await readWebQrAvailability(env.nav, true)).toBe(true);
  });

  it.each([undefined, {}, { formFactors: [] }, { formFactors: 'Mobile' }])(
    'falls back when the browser omits usable form-factor hints: %j',
    async (values) => {
      const env = device('iPhone');
      env.nav.userAgentData = { getHighEntropyValues: vi.fn().mockResolvedValue(values) };
      expect(await readWebQrAvailability(env.nav, true)).toBe(true);
    },
  );

  it('disables a phone with no available video input', async () => {
    const env = device('iPhone');
    env.enumerateDevices.mockResolvedValue([{ kind: 'audioinput' }]);
    expect(await readWebQrAvailability(env.nav, true)).toBe(false);
    expect(env.getUserMedia).not.toHaveBeenCalled();
  });

  it('disables unavailable camera APIs, insecure contexts and failed inventory', async () => {
    const env = device('iPhone');
    expect(await readWebQrAvailability(env.nav, false)).toBe(false);
    expect(env.enumerateDevices).not.toHaveBeenCalled();
    expect(await readWebQrAvailability({ ...env.nav, mediaDevices: undefined }, true)).toBe(false);
    env.enumerateDevices.mockRejectedValue(new Error('blocked'));
    expect(await readWebQrAvailability(env.nav, true)).toBe(false);
    expect(env.getUserMedia).not.toHaveBeenCalled();
  });
});
