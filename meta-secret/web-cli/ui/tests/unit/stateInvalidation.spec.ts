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

  it('cannot open a stale authenticated stream after disconnect while authorization was pending', async () => {
    let authorize!: (token: string) => void;
    const authorization = new Promise<string>((resolve) => {
      authorize = resolve;
    });
    const fetchImpl = vi.fn(async () => ({ ok: true, body: fakeStream().body }) as Response);
    const controller = createStateInvalidationController({
      refresh: vi.fn(async () => {}),
      getAuthorization: () => authorization,
      fetchImpl,
      resolveUrl: () => 'http://localhost/state-events',
      reconnectDelayMs: 0,
    });
    controller.connect('vault-a');
    controller.disconnect();
    authorize('stale-credential');
    await Promise.resolve();
    await Promise.resolve();
    controller.disconnect();
    expect(fetchImpl).not.toHaveBeenCalled();
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

  it('syncs server invalidations before refreshing state and coalesces their sync', async () => {
    vi.useFakeTimers();
    const streams: FakeStream[] = [];
    const operations: string[] = [];
    const syncBeforeRefresh = vi.fn(async () => {
      operations.push('sync');
    });
    const refresh = vi.fn(async () => {
      operations.push('refresh');
    });
    const controller = createStateInvalidationController({
      refresh,
      syncBeforeRefresh,
      getAuthorization: vi.fn().mockResolvedValue('token-1'),
      fetchImpl: vi.fn(async () => {
        const stream = fakeStream();
        streams.push(stream);
        return { ok: true, status: 200, body: stream.body } as Response;
      }),
      debounceMs: 25,
      reconnectDelayMs: 0,
      resolveUrl: (vaultName) => `http://localhost/state-events?vaultName=${vaultName}`,
    });

    controller.connect('vault-a');
    await Promise.resolve();
    await Promise.resolve();
    streams[0].push(
      'event: state_invalidated\ndata: {"type":"state_invalidated","vaultName":"vault-a","scope":"devices"}\n\n',
    );
    streams[0].push(
      'event: state_invalidated\ndata: {"type":"state_invalidated","vaultName":"vault-a","scope":"devices"}\n\n',
    );
    await vi.advanceTimersByTimeAsync(25);

    expect(operations).toEqual(['sync', 'refresh']);
    expect(syncBeforeRefresh).toHaveBeenCalledTimes(1);
    expect(refresh).toHaveBeenCalledTimes(1);
    controller.disconnect();
  });

  it('runs one follow-up refresh when invalidation arrives during an active refresh', async () => {
    vi.useFakeTimers();
    const streams: FakeStream[] = [];
    const operations: string[] = [];
    let resolveRefresh!: () => void;
    let refreshCalls = 0;
    const refresh = vi.fn(() => {
      refreshCalls += 1;
      operations.push('refresh');
      return refreshCalls === 1
        ? new Promise<void>((resolve) => {
            resolveRefresh = resolve;
          })
        : Promise.resolve();
    });
    const syncBeforeRefresh = vi.fn(async () => {
      operations.push('sync');
    });
    const fetchImpl = vi.fn(async () => {
      const stream = fakeStream();
      streams.push(stream);
      return { ok: true, status: 200, body: stream.body } as Response;
    });
    const controller = createStateInvalidationController({
      refresh,
      syncBeforeRefresh,
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
    expect(syncBeforeRefresh).toHaveBeenCalledTimes(1);
    expect(operations).toEqual(['refresh', 'sync', 'refresh']);
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
    const getAuthorization = vi.fn().mockResolvedValueOnce('expired-token').mockResolvedValueOnce('fresh-token');
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

  it('cannot reschedule a retired stream when onInvalidate disconnects before the initial debounce', async () => {
    vi.useFakeTimers();
    const stream = fakeStream();
    const refresh = vi.fn(async () => {});
    const syncBeforeRefresh = vi.fn(async () => {});
    const getAuthorization = vi.fn(async () => 'unit-test-token');
    const fetchImpl = vi
      .fn<typeof fetch>()
      .mockImplementation(async () => ({ ok: true, status: 200, body: stream.body }) as Response);
    const onInvalidate = vi.fn(() => controller.disconnect());
    const controller = createStateInvalidationController({
      refresh,
      syncBeforeRefresh,
      getAuthorization,
      fetchImpl,
      onInvalidate,
      debounceMs: 150,
      reconnectDelayMs: 1_000,
      resolveUrl: () => 'http://localhost/state-events',
    });
    try {
      controller.connect('vault-a');
      await Promise.resolve();
      await Promise.resolve();
      expect(fetchImpl).toHaveBeenCalledOnce();
      const signal = fetchImpl.mock.calls[0]?.[1]?.signal;
      expect(signal?.aborted).toBe(false);
      expect(refresh).not.toHaveBeenCalled();
      stream.push('event: state_invalidated\ndata: {"type":"state_invalidated","vaultName":"vault-a"}\n\n');
      await Promise.resolve();
      await Promise.resolve();
      expect(onInvalidate).toHaveBeenCalledOnce();
      expect(signal?.aborted).toBe(true);
      await vi.advanceTimersByTimeAsync(150);
      await vi.advanceTimersByTimeAsync(1_000);
      expect({
        refresh: refresh.mock.calls.length,
        sync: syncBeforeRefresh.mock.calls.length,
        authorization: getAuthorization.mock.calls.length,
        fetch: fetchImpl.mock.calls.length,
      }).toEqual({ refresh: 0, sync: 0, authorization: 1, fetch: 1 });
    } finally {
      controller.disconnect();
      stream.close();
    }
  });
});
