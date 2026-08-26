import { afterEach, describe, expect, it, vi } from 'vitest';
import { createStateInvalidationController } from '@/utils/stateInvalidation';

class FakeEventSource {
  onopen: (() => void) | null = null;
  onerror: (() => void) | null = null;
  closed = false;
  readonly listeners = new Map<string, Array<(message: MessageEvent<string>) => void>>();

  constructor(readonly url: string) {}

  addEventListener(type: string, listener: (message: MessageEvent<string>) => void) {
    const listeners = this.listeners.get(type) || [];
    listeners.push(listener);
    this.listeners.set(type, listeners);
  }

  close() {
    this.closed = true;
  }

  open() {
    this.onopen?.();
  }

  emit(type: string, data: unknown) {
    for (const listener of this.listeners.get(type) || []) {
      listener(new MessageEvent(type, { data: JSON.stringify(data) }));
    }
  }
}

describe('createStateInvalidationController', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('refreshes on socket connect and coalesces invalidation bursts', async () => {
    vi.useFakeTimers();
    const sources: FakeEventSource[] = [];
    const refresh = vi.fn().mockResolvedValue(undefined);
    const controller = createStateInvalidationController({
      refresh,
      debounceMs: 25,
      createEventSource: (url) => {
        const source = new FakeEventSource(url);
        sources.push(source);
        return source as unknown as EventSource;
      },
      resolveUrl: (vaultName) => `http://localhost/state-events?vaultName=${vaultName}`,
    });

    controller.connect('vault-a');
    sources[0].open();
    sources[0].emit('state_invalidated', { type: 'state_invalidated', vaultName: 'vault-a', scope: 'devices' });
    sources[0].emit('state_invalidated', { type: 'state_invalidated', vaultName: 'vault-a', scope: 'ss_claims' });

    await vi.advanceTimersByTimeAsync(25);

    expect(refresh).toHaveBeenCalledTimes(1);
  });

  it('runs one follow-up refresh when invalidation arrives during an active refresh', async () => {
    vi.useFakeTimers();
    const sources: FakeEventSource[] = [];
    let resolveRefresh!: () => void;
    const refresh = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveRefresh = resolve;
        }),
    );
    const controller = createStateInvalidationController({
      refresh,
      debounceMs: 10,
      createEventSource: (url) => {
        const source = new FakeEventSource(url);
        sources.push(source);
        return source as unknown as EventSource;
      },
      resolveUrl: (vaultName) => `http://localhost/state-events?vaultName=${vaultName}`,
    });

    controller.connect('vault-a');
    sources[0].open();
    await vi.advanceTimersByTimeAsync(10);
    expect(refresh).toHaveBeenCalledTimes(1);

    sources[0].emit('state_invalidated', { type: 'state_invalidated', vaultName: 'vault-a' });
    await vi.advanceTimersByTimeAsync(10);
    expect(refresh).toHaveBeenCalledTimes(1);

    resolveRefresh();
    await Promise.resolve();
    await Promise.resolve();

    expect(refresh).toHaveBeenCalledTimes(2);
  });

  it('does not create periodic interval polling', async () => {
    vi.useFakeTimers();
    const refresh = vi.fn().mockResolvedValue(undefined);
    const controller = createStateInvalidationController({
      refresh,
      debounceMs: 10,
      createEventSource: (url) => new FakeEventSource(url) as unknown as EventSource,
      resolveUrl: (vaultName) => `http://localhost/state-events?vaultName=${vaultName}`,
    });

    controller.connect('vault-a');

    await vi.advanceTimersByTimeAsync(60_000);

    expect(refresh).not.toHaveBeenCalled();
  });
});
