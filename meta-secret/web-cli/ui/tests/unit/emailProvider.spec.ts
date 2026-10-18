import { afterEach, describe, expect, it, vi } from 'vitest';

afterEach(() => {
  vi.unstubAllEnvs();
  vi.unstubAllGlobals();
  vi.resetModules();
  document.querySelectorAll('script[src]').forEach((script) => script.remove());
});

describe('Provider email is a label, never membership', () => {
  it('enables Apple only through a configured HTTPS email broker', async () => {
    vi.stubEnv('VITE_APPLE_EMAIL_BROKER_URL', 'https://provider.example.test');
    vi.stubEnv('VITE_APPLE_SERVICE_ID', '');
    vi.stubEnv('VITE_APPLE_REDIRECT_URI', '');
    const provider = await import('@/features/registration/emailProvider');
    expect(provider.emailProviderCapabilities.apple).toBe(true);
  });
  it('does not invent an email or launch a provider when browser credentials are unconfigured', async () => {
    vi.stubEnv('VITE_GOOGLE_WEB_CLIENT_ID', '');
    vi.stubEnv('VITE_APPLE_SERVICE_ID', '');
    const provider = await import('@/features/registration/emailProvider');
    expect(provider.emailProviderCapabilities.google).toBe(false);
    await expect(provider.selectGoogleEmail()).rejects.toThrow('not configured');
    expect(document.querySelector('script[src]')).toBeNull();
  });
  it('uses Google token only in memory/header and returns the exact selected email', async () => {
    vi.stubEnv('VITE_GOOGLE_WEB_CLIENT_ID', 'public-client-id');
    let callback!: (response: { access_token?: string }) => void;
    const token = { access_token: 'temporary-test-token' };
    vi.stubGlobal('google', {
      accounts: {
        oauth2: {
          initTokenClient: (options: { callback: typeof callback }) => {
            callback = options.callback;
            return { requestAccessToken: () => callback(token) };
          },
        },
      },
    });
    const fetch = vi.fn().mockResolvedValue({ ok: true, json: async () => ({ email: 'Owner@Example.com' }) });
    vi.stubGlobal('fetch', fetch);
    const provider = await import('@/features/registration/emailProvider');
    const selection = provider.selectGoogleEmail();
    document
      .querySelector<HTMLScriptElement>('script[src="https://accounts.google.com/gsi/client"]')!
      .dispatchEvent(new Event('load'));
    await expect(selection).resolves.toBe('Owner@Example.com');
    expect(fetch).toHaveBeenCalledWith('https://openidconnect.googleapis.com/v1/userinfo', {
      headers: { Authorization: 'Bearer temporary-test-token' },
      cache: 'no-store',
    });
    expect(token.access_token).toBeUndefined();
  });
  it('does not enable the former decode-only Apple SDK configuration', async () => {
    vi.stubEnv('VITE_APPLE_EMAIL_BROKER_URL', '');
    vi.stubEnv('VITE_APPLE_SERVICE_ID', 'public-service-id');
    vi.stubEnv('VITE_APPLE_REDIRECT_URI', 'https://example.test/callback');
    const provider = await import('@/features/registration/emailProvider');
    expect(provider.emailProviderCapabilities.apple).toBe(false);
    await expect(provider.selectAppleEmail()).rejects.toThrow('not configured');
  });
});
