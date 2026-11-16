import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createStateInvalidationController, type StateInvalidationControllerOptions } from '@/utils/stateInvalidation';
import {
  cleanupRevealHarness,
  controlledTransport,
  debounce,
  resetRevealHarness,
  settle,
  vaultName,
  type Restartable,
} from './liveboardRecoveredRevealFixtures';
import { deferred } from './liveboardCoreFixtures';

const disposals: Array<() => void> = [];
beforeEach(resetRevealHarness);
afterEach(async () => {
  disposals.splice(0).forEach((dispose) => dispose());
  await cleanupRevealHarness();
});
function setup() {
  const transport = controlledTransport();
  const refresh = vi.fn(async () => undefined);
  const getAuthorization = vi.fn(async () => 'fixture-credential');
  const onInvalidate = vi.fn();
  const onUnavailable = vi.fn();
  const options: StateInvalidationControllerOptions & { onUnavailable(): void } = {
    refresh,
    getAuthorization,
    onInvalidate,
    onUnavailable,
    fetchImpl: transport.fetchImpl,
    resolveUrl: () => 'http://localhost/state-events',
  };
  const connection = createStateInvalidationController(options) as Restartable;
  disposals.push(() => {
    connection.disconnect();
    transport.dispose();
  });
  return { connection, transport, refresh, getAuthorization, onInvalidate, onUnavailable };
}
type Setup = ReturnType<typeof setup>;
function restart(s: Setup) {
  // Missing planned API is structural RED, explicitly distinct from parent lost-Show RED.
  expect(typeof s.connection.restartReady).toBe('function');
  return s.connection.restartReady(vaultName);
}

describe('HTTP-ready leases over the actual authenticated fetch/SSE controller', () => {
  it('retires old generation synchronously and resolves on successful HTTP body, without a fabricated ready event', async () => {
    const s = setup();
    s.connection.connect(vaultName);
    await settle();
    s.transport.requests[0]!.respond();
    await settle();
    expect(s.refresh).not.toHaveBeenCalled();
    let resolved = false;
    const ready = restart(s).then((lease) => {
      resolved = true;
      return lease;
    });
    expect(s.transport.requests[0]!.signal.aborted).toBe(true);
    await settle();
    expect(resolved).toBe(false);
    expect(s.transport.requests).toHaveLength(2);
    expect(s.getAuthorization).toHaveBeenCalledTimes(2);
    expect(s.transport.fetchImpl.mock.calls[1]![1]?.headers).toMatchObject({
      Accept: 'text/event-stream',
      Authorization: 'Bearer fixture-credential',
    });
    expect(s.transport.fetchImpl.mock.calls[1]![0]).toBe('http://localhost/state-events');
    s.transport.requests[1]!.respond();
    const lease = await ready;
    expect(lease.isCurrent()).toBe(true);
    // No data/initial/ready frame was ever enqueued in the new response.
    s.transport.requests[0]!.push();
    await debounce();
    await vi.advanceTimersByTimeAsync(1_000);
    expect(s.onInvalidate).not.toHaveBeenCalled();
    expect(s.onUnavailable).not.toHaveBeenCalled();
    expect(s.refresh).not.toHaveBeenCalled();
    expect(s.transport.requests.filter((request) => !request.signal.aborted)).toHaveLength(1);
    expect(s.transport.requests).toHaveLength(2);
    s.transport.requests[1]!.push();
    await debounce();
    expect(s.onInvalidate).toHaveBeenCalledOnce();
    expect(s.refresh).toHaveBeenCalledExactlyOnceWith({ requiredSync: true });
  });

  it('preserves ordinary connect bootstrap behavior outside explicit restart', async () => {
    const s = setup();
    s.connection.connect(vaultName);
    await settle();
    s.transport.requests[0]!.respond();
    await debounce();
    expect(s.refresh).toHaveBeenCalledExactlyOnceWith({ requiredSync: false });
    expect(s.getAuthorization).toHaveBeenCalledOnce();
    s.connection.connect(vaultName);
    await settle();
    expect(s.transport.requests).toHaveLength(1);
  });

  it.each(['auth403', 'missing-body', 'fetch-error'] as const)(
    '%s before headers rejects and signals the persistent owner',
    async (reason) => {
      const s = setup();
      const result = restart(s).then(
        () => true,
        () => false,
      );
      await settle();
      const request = s.transport.requests[0]!;
      if (reason === 'auth403') request.respond(403);
      if (reason === 'missing-body') request.respond(200, false);
      if (reason === 'fetch-error') request.response.reject(new Error('FetchUnavailable'));
      expect(await result).toBe(false);
      expect(s.onUnavailable).toHaveBeenCalledOnce();
      expect(s.refresh).not.toHaveBeenCalled();
    },
  );

  it.each(['EOF', 'transport-error'] as const)(
    '%s revokes an actual ready lease and notifies loss before reconnect',
    async (reason) => {
      const s = setup();
      const ready = restart(s);
      await settle();
      s.transport.requests[0]!.respond();
      const lease = await ready;
      const lost = vi.fn();
      const unsubscribe = lease.onLoss(lost);
      if (reason === 'EOF') s.transport.requests[0]!.close();
      else s.transport.requests[0]!.fail();
      await settle();
      expect(lease.isCurrent()).toBe(false);
      expect(lost).toHaveBeenCalledOnce();
      expect(s.onUnavailable).toHaveBeenCalledOnce();
      unsubscribe();
      unsubscribe();
      await vi.advanceTimersByTimeAsync(1_000);
      await settle();
      s.transport.requests[1]!.respond();
      await settle();
      expect(lease.isCurrent()).toBe(false);
      expect(lost).toHaveBeenCalledOnce();
      expect(s.onUnavailable).toHaveBeenCalledOnce();
    },
  );

  it('temporary lease unsubscribe never removes the persistent parent loss owner', async () => {
    const s = setup();
    const ready = restart(s);
    await settle();
    s.transport.requests[0]!.respond();
    const lease = await ready;
    const lost = vi.fn();
    const unsubscribe = lease.onLoss(lost);
    unsubscribe();
    s.transport.requests[0]!.close();
    await settle();
    expect(lease.isCurrent()).toBe(false);
    expect(lost).not.toHaveBeenCalled();
    expect(s.onUnavailable).toHaveBeenCalledOnce();
  });

  it.each(['authorization', 'headers'] as const)(
    'disconnect while %s is pending rejects without opening a retired receiver',
    async (stage) => {
      const s = setup();
      const authorization = deferred<string>();
      if (stage === 'authorization') s.getAuthorization.mockReturnValue(authorization.promise);
      const ready = restart(s).then(
        () => true,
        () => false,
      );
      await settle();
      s.connection.disconnect();
      authorization.resolve('retired-credential');
      if (stage === 'headers') s.transport.requests[0]!.respond();
      expect(await ready).toBe(false);
      await debounce();
      expect(s.refresh).not.toHaveBeenCalled();
      expect(s.onUnavailable).not.toHaveBeenCalled();
      if (stage === 'authorization') expect(s.transport.fetchImpl).not.toHaveBeenCalled();
      else expect(s.transport.requests[0]!.signal.aborted).toBe(true);
    },
  );

  it('old EOF/error after a ready rotation cannot revoke the successor or notify its parent', async () => {
    const s = setup();
    const first = restart(s);
    await settle();
    s.transport.requests[0]!.respond();
    const old = await first;
    const second = restart(s);
    await settle();
    s.transport.requests[1]!.respond();
    const current = await second;
    s.transport.requests[0]!.fail();
    await settle();
    expect(old.isCurrent()).toBe(false);
    expect(current.isCurrent()).toBe(true);
    expect(s.onUnavailable).not.toHaveBeenCalled();
    expect(s.refresh).not.toHaveBeenCalled();
  });
});
