import { describe, expect, it, vi } from 'vitest';
import { createJoinResultController } from '@/utils/joinResult';

function makeStream() {
  let controller!: ReadableStreamDefaultController<Uint8Array>;
  const body = new ReadableStream<Uint8Array>({
    start: (value) => {
      controller = value;
    },
  });
  return {
    body,
    push: (value: string) => controller.enqueue(new TextEncoder().encode(value)),
  };
}

describe('createJoinResultController', () => {
  it('subscribes with the scoped token and refreshes state once for a terminal event', async () => {
    const stream = makeStream();
    const refresh = vi.fn().mockResolvedValue(undefined);
    const fetchImpl = vi.fn(async () => ({ ok: true, status: 200, body: stream.body }) as Response);
    const controller = createJoinResultController({
      refresh,
      getAuthorization: vi.fn().mockResolvedValue('request-scoped-secret-token'),
      fetchImpl,
      resolveUrl: (vaultName) => `http://localhost/join-events?vaultName=${vaultName}`,
    });

    controller.connect('vault-a');
    await vi.waitFor(() => expect(fetchImpl).toHaveBeenCalledTimes(1));
    expect(fetchImpl).toHaveBeenCalledWith(
      'http://localhost/join-events?vaultName=vault-a',
      expect.objectContaining({
        headers: expect.objectContaining({ Authorization: 'Bearer request-scoped-secret-token' }),
      }),
    );
    expect(String(fetchImpl.mock.calls[0][0])).not.toContain('request-scoped-secret-token');

    stream.push('event: join_result\ndata: {"requestId":"join-id-1","outcome":"accepted"}\n\n');
    await vi.waitFor(() => expect(refresh).toHaveBeenCalledTimes(1));
    controller.disconnect();
  });

  it('does not refresh for a result event missing a request id', async () => {
    const stream = makeStream();
    const refresh = vi.fn().mockResolvedValue(undefined);
    const fetchImpl = vi.fn(async () => ({ ok: true, status: 200, body: stream.body }) as Response);
    const controller = createJoinResultController({
      refresh,
      getAuthorization: vi.fn().mockResolvedValue('token'),
      fetchImpl,
      resolveUrl: () => 'http://localhost/join-events?vaultName=vault-a',
      reconnectDelayMs: 60_000,
    });

    controller.connect('vault-a');
    await vi.waitFor(() => expect(fetchImpl).toHaveBeenCalledTimes(1));
    stream.push('event: join_result\ndata: {"outcome":"accepted"}\n\n');
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(refresh).not.toHaveBeenCalled();
    controller.disconnect();
  });

  it('surfaces a terminal authorization failure without reconnecting in a loop', async () => {
    const onError = vi.fn();
    const fetchImpl = vi.fn(async () => ({ ok: false, status: 403, body: null }) as Response);
    const controller = createJoinResultController({
      refresh: vi.fn().mockResolvedValue(undefined),
      getAuthorization: vi.fn().mockResolvedValue('token'),
      onError,
      fetchImpl,
      resolveUrl: () => 'http://localhost/join-events?vaultName=vault-a',
      reconnectDelayMs: 0,
    });

    controller.connect('vault-a');
    await vi.waitFor(() => expect(onError).toHaveBeenCalledWith('join result authorization rejected (HTTP 403)', true));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(fetchImpl).toHaveBeenCalledTimes(1);
    controller.disconnect();
  });
});
