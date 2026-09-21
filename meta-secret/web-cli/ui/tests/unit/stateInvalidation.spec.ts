import { afterEach, describe, expect, it, vi } from 'vitest';
import { createStateInvalidationController } from '@/utils/stateInvalidation';

type FakeStream = {
  body: ReadableStream<Uint8Array>;
  push: (text: string) => void;
  close: () => void;
};

function fakeStream(): FakeStream {
  let streamController!: ReadableStreamDefaultController<Uint8Array>;
  const body = new ReadableStream<Uint8Array>({
    start(controller) {
      streamController = controller;
    },
  });
  const encoder = new TextEncoder();
  return {
    body,
    push: (text) => streamController.enqueue(encoder.encode(text)),
    close: () => streamController.close(),
  };
}

describe('createStateInvalidationController', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('authenticates the stream and coalesces invalidation bursts', async () => {
    vi.useFakeTimers();
    const streams: FakeStream[] = [];
    const refresh = vi.fn().mockResolvedValue(undefined);
    const getAuthorization = vi.fn().mockResolvedValue('token-1');
    const fetchImpl = vi.fn(async () => {
      const stream = fakeStream();
      streams.push(stream);
      return { ok: true, status: 200, body: stream.body } as Response;
    });
    const controller = createStateInvalidationController({
      refresh,
      getAuthorization,
      fetchImpl,
      debounceMs: 25,
      reconnectDelayMs: 0,
      resolveUrl: (vaultName) => `http://localhost/state-events?vaultName=${vaultName}`,
    });

    controller.connect('vault-a');
    await Promise.resolve();
    await Promise.resolve();
    expect(fetchImpl).toHaveBeenCalledWith(
      'http://localhost/state-events?vaultName=vault-a',
      expect.objectContaining({
        headers: expect.objectContaining({ Authorization: 'Bearer token-1' }),
      }),
    );

    streams[0].push('event: state_invalidated\ndata: {"type":"state_invalidated","vaultName":"vault-a"}\n\n');
    streams[0].push('event: state_invalidated\ndata: {"type":"state_invalidated","vaultName":"vault-a"}\n\n');
    await vi.advanceTimersByTimeAsync(25);

    expect(refresh).toHaveBeenCalledTimes(1);
    controller.disconnect();
  });

  it('runs one follow-up refresh when invalidation arrives during an active refresh', async () => {
    vi.useFakeTimers();
    const streams: FakeStream[] = [];
    let resolveRefresh!: () => void;
    const refresh = vi.fn(
      () => new Promise<void>((resolve) => {
        resolveRefresh = resolve;
      }),
    );
    const fetchImpl = vi.fn(async () => {
      const stream = fakeStream();
      streams.push(stream);
      return { ok: true, status: 200, body: stream.body } as Response;
    });
    const controller = createStateInvalidationController({
      refresh,
      getAuthorization: vi.fn().mockResolvedValue('token-1'),
      fetchImpl,
      debounceMs: 10,
      reconnectDelayMs: 0,
      resolveUrl: (vaultName) => `http://localhost/state-events?vaultName=${vaultName}`,
    });

    controller.connect('vault-a');
    await Promise.resolve();
    await Promise.resolve();
    await vi.advanceTimersByTimeAsync(10);
    expect(refresh).toHaveBeenCalledTimes(1);

    streams[0].push('event: state_invalidated\ndata: {"type":"state_invalidated","vaultName":"vault-a"}\n\n');
    await vi.advanceTimersByTimeAsync(10);
    expect(refresh).toHaveBeenCalledTimes(1);

    resolveRefresh();
    await Promise.resolve();
    await Promise.resolve();
    expect(refresh).toHaveBeenCalledTimes(2);
    controller.disconnect();
  });

  it('does not create periodic polling while the stream remains open', async () => {
    vi.useFakeTimers();
    const stream = fakeStream();
    const refresh = vi.fn().mockResolvedValue(undefined);
    const controller = createStateInvalidationController({
      refresh,
      getAuthorization: vi.fn().mockResolvedValue('token-1'),
      fetchImpl: vi.fn(async () => ({ ok: true, status: 200, body: stream.body }) as Response),
      debounceMs: 10,
      reconnectDelayMs: 0,
      resolveUrl: (vaultName) => `http://localhost/state-events?vaultName=${vaultName}`,
    });

    controller.connect('vault-a');
    await Promise.resolve();
    await Promise.resolve();
    await vi.advanceTimersByTimeAsync(60_000);

    expect(refresh).toHaveBeenCalledTimes(1);
    controller.disconnect();
  });

  it('refreshes authorization after a rejected connection and then opens the stream', async () => {
    const stream = fakeStream();
    const refresh = vi.fn().mockResolvedValue(undefined);
    const getAuthorization = vi
      .fn()
      .mockResolvedValueOnce('expired-token')
      .mockResolvedValueOnce('fresh-token');
    const fetchImpl = vi
      .fn()
      .mockResolvedValueOnce({ ok: false, status: 403, body: null } as Response)
      .mockResolvedValueOnce({ ok: true, status: 200, body: stream.body } as Response);
    const controller = createStateInvalidationController({
      refresh,
      getAuthorization,
      fetchImpl,
      debounceMs: 0,
      reconnectDelayMs: 0,
      resolveUrl: (vaultName) => `http://localhost/state-events?vaultName=${vaultName}`,
    });

    controller.connect('vault-a');
    for (let index = 0; index < 8; index += 1) await Promise.resolve();

    expect(getAuthorization).toHaveBeenCalledTimes(2);
    expect(fetchImpl).toHaveBeenNthCalledWith(
      2,
      'http://localhost/state-events?vaultName=vault-a',
      expect.objectContaining({
        headers: expect.objectContaining({ Authorization: 'Bearer fresh-token' }),
      }),
    );
    controller.disconnect();
  });
});
