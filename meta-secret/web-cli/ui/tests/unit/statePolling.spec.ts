import { afterEach, describe, expect, it, vi } from 'vitest';
import { createStatePoller, STATE_POLL_INTERVAL_MS } from '@/utils/statePolling';

describe('createStatePoller', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('refreshes immediately and then every 5 seconds', async () => {
    vi.useFakeTimers();
    const refresh = vi.fn().mockResolvedValue(undefined);
    const poller = createStatePoller(refresh);

    poller.start();
    await Promise.resolve();

    expect(refresh).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(STATE_POLL_INTERVAL_MS);
    expect(refresh).toHaveBeenCalledTimes(2);

    await vi.advanceTimersByTimeAsync(STATE_POLL_INTERVAL_MS);
    expect(refresh).toHaveBeenCalledTimes(3);

    poller.stop();
  });

  it('does not overlap refreshes when a previous refresh is still running', async () => {
    vi.useFakeTimers();
    let resolveRefresh!: () => void;
    const refresh = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveRefresh = resolve;
        }),
    );
    const poller = createStatePoller(refresh);

    poller.start();
    await Promise.resolve();
    await vi.advanceTimersByTimeAsync(STATE_POLL_INTERVAL_MS * 2);

    expect(refresh).toHaveBeenCalledTimes(1);

    resolveRefresh();
    await Promise.resolve();
    expect(refresh).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(STATE_POLL_INTERVAL_MS);

    expect(refresh).toHaveBeenCalledTimes(2);

    poller.stop();
  });

  it('schedules the next refresh only after the previous refresh completes', async () => {
    vi.useFakeTimers();
    const refresh = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          window.setTimeout(resolve, STATE_POLL_INTERVAL_MS * 2);
        }),
    );
    const poller = createStatePoller(refresh);

    poller.start();
    await Promise.resolve();

    expect(refresh).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(STATE_POLL_INTERVAL_MS * 2 - 1);
    expect(refresh).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(1);
    await Promise.resolve();
    expect(refresh).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(STATE_POLL_INTERVAL_MS - 1);
    expect(refresh).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(1);
    expect(refresh).toHaveBeenCalledTimes(2);

    poller.stop();
  });

  it('stops scheduled refreshes', async () => {
    vi.useFakeTimers();
    const refresh = vi.fn().mockResolvedValue(undefined);
    const poller = createStatePoller(refresh);

    poller.start();
    await Promise.resolve();
    poller.stop();

    await vi.advanceTimersByTimeAsync(STATE_POLL_INTERVAL_MS * 3);
    expect(refresh).toHaveBeenCalledTimes(1);
  });
});
