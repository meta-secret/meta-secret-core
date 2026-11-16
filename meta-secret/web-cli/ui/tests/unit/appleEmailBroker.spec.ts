import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { webcrypto } from 'node:crypto';
import { configuredAppleBroker, selectAppleBrokerEmail } from '@/features/registration/appleEmailBroker';

const requestId = 'R'.repeat(43);
const base = 'https://broker.example.test';
let popup: { closed: boolean; location: { href: string }; close: ReturnType<typeof vi.fn> };
let fetcher: ReturnType<typeof vi.fn>;
const success = () => new Response(JSON.stringify({ status: 'success', email: 'Owner@privaterelay.appleid.com' }));
const start = () =>
  new Response(
    JSON.stringify({
      requestId,
      expiresAt: Math.floor(Date.now() / 1000) + 300,
      authorizationUrl:
        'https://appleid.apple.com/auth/authorize?' +
        new URLSearchParams({
          client_id: 'test.service',
          redirect_uri: base + '/apple/callback',
          response_type: 'code id_token',
          response_mode: 'form_post',
          scope: 'email',
          state: 'S'.repeat(43),
          nonce: 'N'.repeat(43),
        }),
    }),
  );
beforeEach(() => {
  popup = { closed: false, location: { href: '' }, close: vi.fn() };
  vi.stubGlobal(
    'open',
    vi.fn(() => popup),
  );
  vi.stubGlobal('crypto', { getRandomValues: (bytes: Uint8Array) => bytes.fill(0), subtle: webcrypto.subtle });
  fetcher = vi.fn(async (url: string) =>
    url.endsWith('/apple/start')
      ? start()
      : url.endsWith('/apple/take')
        ? success()
        : new Response('{"status":"cancelled"}'),
  );
  vi.stubGlobal('fetch', fetcher);
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});
const message = (origin = base, source: unknown = popup, id = requestId) =>
  window.dispatchEvent(
    new MessageEvent('message', {
      origin,
      source: source as Window,
      data: { type: 'APPLE_EMAIL_RESULT', requestId: id },
    }),
  );
async function started() {
  await vi.waitFor(() => expect(popup.location.href).toContain('https://appleid.apple.com/auth/authorize?'));
}

describe('proof-bound Apple browser email', () => {
  it('accepts only a fixed HTTPS origin configuration', () => {
    expect(configuredAppleBroker(base)).toBe(base);
    for (const value of [
      '',
      'http://broker.test',
      'https://localhost',
      'https://127.0.0.1',
      base + '/other',
      base + '?x=1',
      'https://name:pass@broker.test',
    ])
      expect(configuredAppleBroker(value)).toBeUndefined();
  });
  it('returns exact email only after the matching origin/window/id and HTTPS proof POST', async () => {
    const result = selectAppleBrokerEmail(base);
    await started();
    message('https://evil.test');
    message(base, {});
    message(base, popup, 'foreign');
    expect(fetcher.mock.calls.some(([url]) => String(url).endsWith('/apple/take'))).toBe(false);
    message();
    message();
    await expect(result).resolves.toBe('Owner@privaterelay.appleid.com');
    const take = fetcher.mock.calls.filter(([url]) => String(url).endsWith('/apple/take'));
    expect(take).toHaveLength(1);
    expect(JSON.parse(take[0][1].body)).toEqual({ requestId, verifier: 'A'.repeat(43) });
    const request = JSON.parse(fetcher.mock.calls[0][1].body);
    const expected = Buffer.from(
      await webcrypto.subtle.digest('SHA-256', new TextEncoder().encode('A'.repeat(43))),
    ).toString('base64url');
    expect(request).toEqual({ platform: 'web', challenge: expected });
    expect(
      fetcher.mock.calls.every(
        ([url, options]) =>
          !String(url).includes('Owner@') && !String(url).includes('verifier') && options.redirect === 'error',
      ),
    ).toBe(true);
    expect(popup.close).toHaveBeenCalledOnce();
  });
  it('does not create a server attempt when the popup is blocked', async () => {
    vi.stubGlobal('open', () => null);
    await expect(selectAppleBrokerEmail(base)).rejects.toThrow(/popup/i);
    expect(fetcher).not.toHaveBeenCalled();
  });
  it('matches the published Rust/Android ASCII proof vector', async () => {
    vi.stubGlobal('crypto', {
      getRandomValues: (bytes: Uint8Array) => {
        bytes.forEach((_, index) => {
          bytes[index] = index;
        });
        return bytes;
      },
      subtle: webcrypto.subtle,
    });
    const result = selectAppleBrokerEmail(base);
    await started();
    message();
    await result;
    expect(JSON.parse(fetcher.mock.calls[0][1].body).challenge).toBe('6oZqdX5MOLq_qBJ8vppAnT4fk6AP8UiP9zX8-Rev_9A');
    expect(JSON.parse(fetcher.mock.calls.find(([url]) => String(url).endsWith('/apple/take'))![1].body).verifier).toBe(
      'AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8',
    );
  });
  it('aborts, cancels the exact transaction and ignores a late callback', async () => {
    const abort = new AbortController();
    const result = selectAppleBrokerEmail(base, abort.signal);
    const rejected = expect(result).rejects.toThrow(/closed|cancel/i);
    await started();
    abort.abort();
    await rejected;
    message();
    await vi.waitFor(() =>
      expect(fetcher.mock.calls.some(([url]) => String(url).endsWith('/apple/cancel'))).toBe(true),
    );
    expect(fetcher.mock.calls.some(([url]) => String(url).endsWith('/apple/take'))).toBe(false);
    expect(popup.close).toHaveBeenCalledOnce();
  });
  it('reports browser close without accepting a forged email message', async () => {
    fetcher.mockImplementation(async (url: string) =>
      url.endsWith('/apple/start') ? start() : new Response('{"status":"pending"}'),
    );
    const result = selectAppleBrokerEmail(base);
    const rejected = expect(result).rejects.toThrow(/closed/);
    await started();
    popup.closed = true;
    await rejected;
  });
  it('takes once when popup close is observed before the queued completion message', async () => {
    const result = selectAppleBrokerEmail(base);
    await started();
    popup.closed = true;
    await expect(result).resolves.toBe('Owner@privaterelay.appleid.com');
    message();
    expect(fetcher.mock.calls.filter(([url]) => String(url).endsWith('/apple/take'))).toHaveLength(1);
    expect(fetcher.mock.calls.some(([url]) => String(url).endsWith('/apple/cancel'))).toBe(false);
  });
  for (const response of [
    { status: 'pending' },
    { status: 'success' },
    { status: 'success', email: 42 },
    { status: 'cancelled' },
    { status: 'error', code: 'invalid_response' },
  ]) {
    it(`rejects invalid/unsuccessful broker result ${JSON.stringify(response)}`, async () => {
      fetcher.mockImplementation(async (url: string) =>
        url.endsWith('/apple/start') ? start() : new Response(JSON.stringify(response)),
      );
      const result = selectAppleBrokerEmail(base);
      const rejected = expect(result).rejects.toThrow();
      await started();
      message();
      await rejected;
    });
  }
  it('rejects a returned authorization URL outside the fixed Apple endpoint', async () => {
    fetcher.mockResolvedValue(
      new Response(
        JSON.stringify({
          requestId,
          expiresAt: Math.floor(Date.now() / 1000) + 300,
          authorizationUrl: 'https://evil.test/',
        }),
      ),
    );
    await expect(selectAppleBrokerEmail(base)).rejects.toThrow();
    expect(popup.location.href).toBe('');
  });
  it('rejects oversized responses and removes the owned popup', async () => {
    fetcher.mockResolvedValue(new Response('x'.repeat(20_000)));
    await expect(selectAppleBrokerEmail(base)).rejects.toThrow();
    expect(popup.close).toHaveBeenCalledOnce();
  });
  for (const text of [
    '{"requestId":"a","requestId":"b"}',
    '{"status":"success","status":"cancelled","email":"x@example.test"}',
    '{"nested":{"status":"success"}}',
  ]) {
    it('rejects duplicate or nested JSON fields before using them: ' + text, async () => {
      fetcher.mockResolvedValue(new Response(text));
      await expect(selectAppleBrokerEmail(base)).rejects.toThrow();
    });
  }
  it('rejects otherwise successful responses carrying unknown token fields', async () => {
    fetcher.mockImplementation(async (url: string) =>
      url.endsWith('/apple/start')
        ? start()
        : new Response('{"status":"success","email":"x@example.test","token":"not-allowed"}'),
    );
    const result = selectAppleBrokerEmail(base);
    const rejected = expect(result).rejects.toThrow();
    await started();
    message();
    await rejected;
  });
  it('cancels a chunked oversized response before requesting later chunks', async () => {
    let reads = 0;
    let cancelled = false;
    const stream = new ReadableStream<Uint8Array>(
      {
        pull(controller) {
          reads++;
          controller.enqueue(new Uint8Array(9000));
        },
        cancel() {
          cancelled = true;
        },
      },
      { highWaterMark: 0 },
    );
    fetcher.mockResolvedValue(new Response(stream));
    await expect(selectAppleBrokerEmail(base)).rejects.toThrow();
    expect(reads).toBe(2);
    expect(cancelled).toBe(true);
  });
});
