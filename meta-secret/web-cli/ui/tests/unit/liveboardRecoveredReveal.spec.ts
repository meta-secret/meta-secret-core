import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createCoreAdapter } from '@/features/liveboard/coreAdapter';
import { contextFixture, coreState, deferred } from './liveboardCoreFixtures';
import {
  canonical,
  cleanupRevealHarness,
  debounce,
  expectNoValue,
  expectNoWrites,
  historical,
  intent,
  rawValue,
  resetRevealHarness,
  settle,
  setupReveal,
  type RevealSetup,
} from './liveboardRecoveredRevealFixtures';
import {
  boundaries,
  hardBoundary,
  invalidProof,
  invalidProofs,
  restoreBoundary,
} from './liveboardRecoveredRevealCases';

beforeEach(resetRevealHarness);
afterEach(cleanupRevealHarness);

async function finish(s: RevealSetup) {
  s.setCanonical();
  s.raw.resolve(rawValue);
  await settle();
  await debounce();
}
function expectDelivered(s: RevealSetup) {
  expect(s.card()).toMatchObject({ phase: 'revealed', value: { kind: 'text', text: rawValue }, copied: false });
  expect(s.wrapper.text()).toContain(rawValue);
  expect(s.controller.state.value.unread[intent.claimId]).toBeUndefined();
  expect(s.controller.state.value.unread[historical.requestId]).toBe('red');
  expect(s.listeners.attached).toBeGreaterThan(0);
  expect(s.listeners.active).toBe(0);
  expect(s.physical.maximum).toBe(1);
  expect(s.claims).toHaveLength(1);
  expect(s.claims[0]!.free).toHaveBeenCalledOnce();
  expect(s.manager.free).not.toHaveBeenCalled();
  for (const root of s.roots) expect(root.state.free).not.toHaveBeenCalled();
  expectNoWrites(s);
}

describe('One explicit recovered Show through the actual parent, adapter, Store and SSE', () => {
  it('survives its old completion before return, physically refreshes fresh Done, then deliberately Copy/Hide once', async () => {
    const s = await setupReveal();
    await s.controller.secrets.primary('pass-b');
    await s.controller.secrets.copy('pass-b');
    const other = s.controller.secrets.state.value.cards['pass-b'];
    expect(other.value).not.toBeNull();
    expect(other.copied).toBe(true);
    const showing = s.start();
    await settle();
    expect(s.manager.show_recovered).toHaveBeenCalledOnce();
    expect(s.claims[0]!.free).not.toHaveBeenCalled();
    await s.start();
    expect(s.manager.show_recovered).toHaveBeenCalledOnce();
    s.setCanonical();
    s.transport.broadcast();
    await settle();
    expect(other).toMatchObject({ value: null, copied: false });
    expectNoValue(s);
    await debounce();
    expect(s.manager.sync_now).toHaveBeenCalledOnce();
    s.raw.resolve(rawValue);
    await settle();
    await debounce();
    await showing;
    // Genuine existing lost-Show oracle: no future API is invoked by this test.
    expect(s.card().value).toEqual({ kind: 'text', text: rawValue });
    expectDelivered(s);
    expect(s.manager.sync_now).toHaveBeenCalledTimes(2);
    expect(s.transport.requests).toHaveLength(2);
    expect(s.transport.requests[0]!.signal.aborted).toBe(true);
    expect(s.transport.requests.filter((request) => !request.signal.aborted)).toHaveLength(1);
    expect(s.manager.state_events_auth_token).toHaveBeenCalledTimes(2);
    await s.controller.secrets.copy('pass-a');
    expect(s.platform.copy).toHaveBeenLastCalledWith(rawValue);
    expect(s.card().copied).toBe(true);
    await s.controller.secrets.primary('pass-a');
    expectNoValue(s);
    expect(s.card().secret.primary.kind).toBe('recover');
    expectNoWrites(s);
  });

  it('waits for real fresh HTTP response/body and a post-ready physical sync, never a pre-ready flight', async () => {
    const s = await setupReveal({ autoHeaders: false });
    const showing = s.start();
    await settle();
    const preReady = s.holdSync();
    const earlier = s.session.refresh({ requiredSync: true });
    await settle();
    s.setCanonical();
    s.raw.resolve(rawValue);
    await settle();
    expectNoValue(s);
    expect(s.transport.requests).toHaveLength(2);
    expect(s.transport.requests[1]!.subscribed).toBe(false);
    preReady.resolve();
    await earlier;
    expectNoValue(s);
    const physical = s.holdSync();
    const before = vi.mocked(s.manager.sync_now).mock.calls.length;
    s.order.push('headers-ready');
    s.transport.requests[1]!.respond();
    await settle();
    expect(s.manager.sync_now).toHaveBeenCalledTimes(before + 1);
    expect(s.order.slice(-2)).toEqual(['headers-ready', 'sync']);
    expectNoValue(s);
    physical.resolve();
    await settle();
    await showing;
    expectDelivered(s);
    const reads = vi.mocked(s.manager.get_state).mock.calls.length;
    s.transport.requests[0]!.push(); // arbitrarily late OLD completion, after first publication
    await debounce();
    await vi.advanceTimersByTimeAsync(1_000);
    expect(s.manager.get_state).toHaveBeenCalledTimes(reads);
    expect(s.card().value).toEqual({ kind: 'text', text: rawValue });
    expect(s.transport.requests).toHaveLength(2);
    expectNoWrites(s);
  });

  it('never reveals raw return without fresh exact Done, and never treats passive Done as returned text', async () => {
    const s = await setupReveal();
    const showing = s.start();
    await settle();
    s.setCanonical();
    await s.session.refresh({ requiredSync: true });
    expectNoValue(s);
    expect(s.claims[0]!.free).not.toHaveBeenCalled();
    s.setCanonical(canonical(false));
    s.raw.resolve(rawValue);
    await settle();
    await debounce();
    await showing;
    expectNoValue(s);
    s.setCanonical();
    await s.session.refresh({ requiredSync: true });
    expectNoValue(s);
    expect(s.controller.state.value.unread[intent.claimId]).toBe('green');
    expectNoWrites(s);
  });

  it.each(invalidProofs)(
    'rejects %s continuity even when returned text and a matching Done row exist',
    async (reason) => {
      const s = await setupReveal();
      const showing = s.start();
      await settle();
      s.setCanonical(invalidProof(reason));
      s.raw.resolve(rawValue);
      await settle();
      await debounce();
      await showing;
      expectNoValue(s);
      s.setCanonical();
      await s.session.refresh({ requiredSync: true });
      expectNoValue(s);
      expectNoWrites(s);
    },
  );

  it('never forgets an observed replacement just because final metadata later looks matching again', async () => {
    const s = await setupReveal();
    const showing = s.start();
    await settle();
    s.setCanonical(invalidProof('foreign-new-claim'));
    await s.session.refresh({ requiredSync: true });
    expectNoValue(s);
    await finish(s);
    await showing;
    expectNoValue(s);
    expectNoWrites(s);
  });

  for (const phase of ['raw', 'http', 'sync'] as const) {
    it.each(boundaries)(`${phase} await: %s permanently revokes, even after later matching Done`, async (reason) => {
      const s = await setupReveal({ autoHeaders: phase === 'raw' });
      const showing = s.start();
      await settle();
      let syncing: ReturnType<typeof deferred<void>> | undefined;
      if (phase !== 'raw') {
        s.setCanonical();
        s.raw.resolve(rawValue);
        await settle();
        expectNoValue(s);
        expect(s.transport.requests).toHaveLength(2);
        if (phase === 'sync') {
          syncing = s.holdSync();
          s.transport.requests[1]!.respond();
          await settle();
          expect(s.manager.sync_now).toHaveBeenCalledOnce();
        }
      }
      await hardBoundary(s, reason);
      expectNoValue(s);
      s.setCanonical();
      s.raw.resolve(rawValue);
      if (phase === 'http') s.transport.requests[1]!.respond();
      syncing?.resolve();
      await settle();
      await debounce();
      await showing;
      restoreBoundary(s, reason);
      if (!['dispose', 'auth', 'key', 'member', 'reset'].includes(reason)) {
        await s.session.refresh({ requiredSync: true }).catch(() => undefined);
        expectNoValue(s);
      }
      expectNoWrites(s);
      expect(s.listeners.active).toBe(0);
      for (const owned of s.claims) expect(owned.free).toHaveBeenCalledOnce();
    });
  }

  for (const when of ['before', 'after'] as const) {
    it.each(['invalidation', 'EOF', 'transport-error'] as const)(
      `new stream %s ${when} publication hard-conceals without restore`,
      async (loss) => {
        const s = await setupReveal();
        const showing = s.start();
        await settle();
        const gate = when === 'before' ? s.holdSync() : undefined;
        s.setCanonical();
        s.raw.resolve(rawValue);
        await settle();
        expect(s.transport.requests).toHaveLength(2);
        if (when === 'after') {
          await showing;
          expectDelivered(s);
          await s.controller.secrets.copy('pass-a');
        } else expectNoValue(s);
        const current = s.transport.requests[1]!;
        if (loss === 'invalidation') current.push();
        if (loss === 'EOF') current.close();
        if (loss === 'transport-error') current.fail();
        await settle();
        expectNoValue(s);
        gate?.resolve();
        await settle();
        await debounce();
        await showing;
        await vi.advanceTimersByTimeAsync(1_000);
        await s.session.refresh({ requiredSync: true }).catch(() => undefined);
        expectNoValue(s);
        expectNoWrites(s);
        expect(s.listeners.active).toBe(0);
      },
    );
  }

  it('failed required physical sync cannot be satisfied by a cached read or manual error dismissal', async () => {
    const s = await setupReveal();
    const showing = s.start();
    await settle();
    vi.mocked(s.manager.sync_now).mockRejectedValueOnce(new Error('SyncUnavailable'));
    await finish(s);
    await showing;
    expectNoValue(s);
    s.session.dismissError();
    await s.store.updateState();
    expectNoValue(s);
    await s.session.refresh({ requiredSync: true });
    expectNoValue(s);
    expectNoWrites(s);
  });

  it('rejects wrong/missing/reused public delivery owners without stranding the actual pending Show', async () => {
    const s = await setupReveal();
    const showing = s.start();
    await settle();
    const lookup = Reflect.get(s.controller.secrets, 'pendingRecoveredReveal');
    const deliver = Reflect.get(s.controller.secrets, 'deliverRecoveredReveal');
    expect(typeof lookup).toBe('function');
    expect(typeof deliver).toBe('function');
    const owner = lookup(intent);
    expect(owner).toBeDefined();
    expect(deliver({ owner: {}, value: { kind: 'text', text: rawValue } })).toBe(false);
    expect(deliver({ owner: undefined, value: { kind: 'text', text: rawValue } })).toBe(false);
    expectNoValue(s);
    await finish(s);
    await showing;
    expectDelivered(s);
    expect(lookup(intent)).toBeUndefined();
    expect(deliver({ owner, value: { kind: 'text', text: 'must not replace first delivery' } })).toBe(false);
    expect(s.card().value).toEqual({ kind: 'text', text: rawValue });
    expectNoWrites(s);
  });

  it.each(['auth403', 'missing-body', 'fetch-error'] as const)(
    'fresh %s before delivery rejects without strict-mode fallback',
    async (failure) => {
      const s = await setupReveal({ autoHeaders: false });
      const showing = s.start();
      await settle();
      s.setCanonical();
      s.raw.resolve(rawValue);
      await settle();
      expectNoValue(s);
      expect(s.transport.requests).toHaveLength(2);
      const request = s.transport.requests[1]!;
      if (failure === 'auth403') request.respond(403);
      if (failure === 'missing-body') request.respond(200, false);
      if (failure === 'fetch-error') request.response.reject(new Error('FetchUnavailable'));
      await settle();
      await showing;
      expectNoValue(s);
      await s.session.refresh({ requiredSync: true }).catch(() => undefined);
      expectNoValue(s);
      expectNoWrites(s);
      expect(s.listeners.active).toBe(0);
    },
  );

  it.each(['auth403', 'missing-body'] as const)(
    'persistent parent loss ownership survives first publication and %s reconnect',
    async (failure) => {
      const s = await setupReveal();
      const showing = s.start();
      await settle();
      await finish(s);
      await showing;
      expectDelivered(s);
      await s.controller.secrets.copy('pass-a');
      s.transport.setAutoHeaders(false);
      s.transport.requests[1]!.close();
      await settle();
      expectNoValue(s);
      expect(s.session.state.value.ready).toBe(false);
      await vi.advanceTimersByTimeAsync(1_000);
      await settle();
      expect(s.transport.requests).toHaveLength(3);
      s.transport.requests[2]!.respond(failure === 'auth403' ? 403 : 200, failure !== 'missing-body');
      await settle();
      expectNoValue(s);
      expect(s.session.state.value.ready).toBe(false);
      expectNoWrites(s);
    },
  );

  it('isolated direct adapter mode retains the old strict epoch/primary fence with no handoff fallback', async () => {
    const context = contextFixture(JSON.stringify(coreState({ ...canonical(false) })));
    const result = deferred<string>();
    vi.mocked(context.manager.show_recovered).mockReturnValue(result.promise);
    const platform = {
      authenticate: vi.fn(async () => true),
      copy: vi.fn(async () => {}),
      readClipboard: vi.fn(async () => ''),
      reportError: vi.fn(),
    };
    const adapter = createCoreAdapter({ getContext: () => context, platform });
    const showing = adapter.secrets.showRecovered(intent).then(
      () => true,
      () => false,
    );
    context.epoch++;
    result.resolve(rawValue);
    expect(await showing).toBe(false);
    expect(context.manager.show_recovered).toHaveBeenCalledOnce();
    expect(platform.authenticate).not.toHaveBeenCalled();
  });
});
