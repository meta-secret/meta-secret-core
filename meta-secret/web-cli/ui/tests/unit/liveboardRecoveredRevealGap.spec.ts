import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { managerFixture, memberFixture, pass, stateFixture } from './liveboardCoreFixtures';
import {
  canonical,
  cleanupRevealHarness,
  debounce,
  expectNoWrites,
  intent,
  rawValue,
  resetRevealHarness,
  settle,
  setupReveal,
  type RevealSetup,
} from './liveboardRecoveredRevealFixtures';

const motion = vi.hoisted(() => ({ instances: [] as Array<{ readonly activeCount: number }> }));
vi.mock('@/features/liveboard/revealMotion', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/features/liveboard/revealMotion')>();
  return {
    ...actual,
    createRevealMotion() {
      const instance = actual.createRevealMotion();
      motion.instances.push(instance);
      return instance;
    },
  };
});
const animateBefore = Object.getOwnPropertyDescriptor(Element.prototype, 'animate');
const rangeBefore = Object.getOwnPropertyDescriptor(Range.prototype, 'getBoundingClientRect');
const effects: Array<{ cancel: ReturnType<typeof vi.fn>; numeric: number[] }> = [];
const names = { 'pass-a': 'Alpha', 'pass-b': 'Beta' };
const localValue = 'local gap fixture';
beforeEach(() => {
  resetRevealHarness();
  motion.instances.length = effects.length = 0;
  vi.stubGlobal('matchMedia', () => ({ matches: false, addEventListener: vi.fn(), removeEventListener: vi.fn() }));
  if (!animateBefore)
    Object.defineProperty(Element.prototype, 'animate', { configurable: true, writable: true, value: vi.fn() });
  if (!rangeBefore)
    Object.defineProperty(Range.prototype, 'getBoundingClientRect', {
      configurable: true,
      writable: true,
      value: vi.fn(),
    });
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue(new DOMRect(0, 0, 8, 8));
  vi.spyOn(Range.prototype, 'getBoundingClientRect').mockReturnValue(new DOMRect(0, 0, 1, 1));
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockImplementation(
    () =>
      ({
        fillText: vi.fn(),
        getImageData: () => ({ data: new Uint8ClampedArray(8 * 8 * 4).fill(255) }),
      }) as unknown as CanvasRenderingContext2D,
  );
  vi.spyOn(Element.prototype, 'animate').mockImplementation((frames) => {
    const numeric = Array.isArray(frames)
      ? frames.flatMap(
          (frame) =>
            String(frame.transform ?? '')
              .match(/-?\d+(?:\.\d+)?/gu)
              ?.map(Number) ?? [],
        )
      : [];
    const effect = { cancel: vi.fn(), numeric, finished: new Promise<Animation>(() => {}) };
    effects.push(effect);
    return effect as unknown as Animation;
  });
});
afterEach(async () => {
  await cleanupRevealHarness();
  if (!animateBefore) Reflect.deleteProperty(Element.prototype, 'animate');
  if (!rangeBefore) Reflect.deleteProperty(Range.prototype, 'getBoundingClientRect');
});

async function prepare() {
  const s = await setupReveal({ autoHeaders: false, names });
  vi.mocked(s.manager.show_local_secret).mockResolvedValue(localValue);
  await s.controller.secrets.primary('pass-b');
  await settle();
  await s.wrapper.get('[data-secret-id="pass-b"] [data-secret-action="copy"]').trigger('click');
  await settle();
  expect(s.controller.secrets.state.value.cards['pass-b']).toMatchObject({
    value: { kind: 'text', text: localValue },
    copied: true,
  });
  expect(s.wrapper.get('[data-secret-id="pass-b"]').text()).toContain(localValue);
  expect(s.platform.copy).toHaveBeenCalledExactlyOnceWith(localValue);
  const liveMotion = motion.instances.find((instance) => instance.activeCount > 0)!;
  expect(liveMotion).toBeDefined();
  expect(liveMotion.activeCount).toBeGreaterThan(0);
  const liveEffects = effects.filter((effect) => effect.numeric.some((value) => Number.isFinite(value) && value !== 0));
  expect(liveEffects.length).toBeGreaterThan(0);
  expect(liveEffects.every((effect) => effect.cancel.mock.calls.length === 0)).toBe(true);
  const showing = s.start();
  await settle();
  expect(s.manager.show_recovered).toHaveBeenCalledOnce();
  const retirement: Array<{ held: boolean; copied: boolean; ready: boolean; motion: number; oldAborted: boolean }> = [];
  return { s, showing, liveMotion, liveEffects, retirement };
}
type Gap = Awaited<ReturnType<typeof prepare>>;
async function openGap(g: Gap) {
  const actualRestart = vi.mocked(g.s.connection.restartReady).getMockImplementation()!;
  vi.mocked(g.s.connection.restartReady).mockImplementation((name) => {
    const cards = Object.values(g.s.controller.secrets.state.value.cards);
    g.retirement.push({
      held: cards.some((card) => card.value !== null),
      copied: cards.some((card) => card.copied),
      ready: g.s.session.state.value.ready,
      motion: motion.instances.reduce((sum, instance) => sum + instance.activeCount, 0),
      oldAborted: g.s.transport.requests[0]!.signal.aborted,
    });
    return actualRestart(name);
  });
  g.s.raw.resolve(rawValue);
  await settle();
  expect(g.s.transport.requests).toHaveLength(2);
  expect(g.s.transport.requests[0]!.signal.aborted).toBe(true);
  expect(g.s.transport.requests[1]!.subscribed).toBe(false);
  return g.s.transport.requests[1]!;
}
function preHeaderPrivacy(g: Gap) {
  expect.soft(g.retirement).toEqual([{ held: false, copied: false, ready: false, motion: 0, oldAborted: false }]);
  expect.soft(g.s.controller.secrets.state.value.cards['pass-b']).toMatchObject({ value: null, copied: false });
  expect.soft(g.s.wrapper.text()).not.toContain(localValue);
  expect.soft(g.s.wrapper.find('[data-secret-action="copy"]').exists()).toBe(false);
  expect.soft(g.liveMotion.activeCount).toBe(0);
  expect.soft(g.liveEffects.every((effect) => effect.cancel.mock.calls.length > 0)).toBe(true);
  expect.soft(g.s.session.state.value.ready).toBe(false);
  expect.soft(g.s.controller.secrets.state.value.blocked).toBe(true);
  const primary = g.s.wrapper.get('[data-secret-id="pass-b"] [data-testid="secret-primary-action-Beta"]');
  expect.soft(primary.attributes('disabled')).toBeDefined();
}
async function cancelOnlyA(s: RevealSetup) {
  s.controller.search('Beta');
  await settle();
  expect(s.controller.secrets.state.value.presentation.visibleIds).toEqual(['pass-b']);
  expect(s.wrapper.find('[data-secret-id="pass-a"]').exists()).toBe(false);
  expect(s.wrapper.find('[data-secret-id="pass-b"]').exists()).toBe(true);
  expect(s.controller.secrets.pendingRecoveredReveal(intent)).toBeUndefined();
}
const gapState = () => ({
  ...canonical(true),
  secrets: canonical(true).secrets.filter((secret) => secret.secretId !== 'pass-b'),
});
function safe(s: RevealSetup) {
  expect.soft(motion.instances.every((instance) => instance.activeCount === 0)).toBe(true);
  expect
    .soft(
      Object.values(s.session.controller.secrets.state.value.cards).every(
        (card) => card.value === null && !card.copied,
      ),
    )
    .toBe(true);
  expect.soft(s.wrapper.text()).not.toContain(rawValue);
  expect.soft(s.wrapper.text()).not.toContain(localValue);
  expect.soft(s.wrapper.find('[data-secret-action="copy"]').exists()).toBe(false);
  expect(s.manager.show_local_secret).toHaveBeenCalledOnce();
  expect(s.platform.copy).toHaveBeenCalledOnce();
  expectNoWrites(s);
  expect(s.physical.maximum).toBe(1);
}

describe('Parent owns the no-replay subscription gap independently from cancelled recovered Show', () => {
  it('masks all held value/Copy/real motion before headers, then automatically reads a gap deletion after partial filtering', async () => {
    const g = await prepare(),
      { s } = g;
    const http = await openGap(g);
    preHeaderPrivacy(g);
    await cancelOnlyA(s);
    s.setCanonical(gapState());
    const reads = vi.mocked(s.manager.get_state).mock.calls.length;
    s.order.push('fresh-headers');
    http.respond(); // Actual subscribed HTTP body, no replay frame and no manual refresh.
    await settle();
    await g.showing;
    await debounce();
    expect.soft(s.manager.sync_now).toHaveBeenCalledOnce();
    expect.soft(s.manager.get_state).toHaveBeenCalledTimes(reads + 1);
    expect.soft(s.order.slice(-3)).toEqual(['fresh-headers', 'sync', 'read']);
    expect.soft(s.controller.secrets.state.value.cards['pass-b']).toBeUndefined();
    expect.soft(s.wrapper.find('[data-secret-id="pass-b"]').exists()).toBe(false);
    expect.soft(s.session.state.value.ready).toBe(true);
    expect(s.transport.requests.filter((request) => !request.signal.aborted)).toHaveLength(1);
    expect(s.manager.state_events_auth_token).toHaveBeenCalledTimes(2);
    expect(s.claims).toHaveLength(1);
    expect(s.claims[0]!.free).toHaveBeenCalledOnce();
    expect(s.controller.state.value.unread[intent.claimId]).toBe('green');
    s.controller.search('Alpha');
    await settle();
    safe(s);
  });

  it('post-header sync failure stays required across error dismissal and an explicitly cached read', async () => {
    const g = await prepare(),
      { s } = g;
    const http = await openGap(g);
    await cancelOnlyA(s);
    s.setCanonical(gapState());
    vi.mocked(s.manager.sync_now).mockRejectedValueOnce(new Error('GapSyncUnavailable'));
    http.respond();
    await settle();
    await g.showing;
    expect.soft(s.manager.sync_now).toHaveBeenCalledOnce();
    expect.soft(s.session.state.value.ready).toBe(false);
    expect.soft(s.store.requiredSyncPending).toBe(true);
    safe(s);
    // Display-only control: parent dismissal now requests a genuine required read.
    s.controller.dismissError();
    await s.store.updateState(); // Permitted cached read only; it cannot satisfy the failed required target.
    await settle();
    expect.soft(s.session.state.value.ready).toBe(false);
    expect.soft(s.store.requiredSyncPending).toBe(true);
    expect.soft(s.manager.sync_now).toHaveBeenCalledOnce();
    safe(s);
  });

  it('cancelled delivery still queues a NEW post-ready physical target behind an actual pre-ready SSE flight', async () => {
    const g = await prepare(),
      { s } = g;
    const earlier = s.holdSync();
    s.transport.broadcast();
    await debounce();
    expect(s.manager.sync_now).toHaveBeenCalledOnce();
    const http = await openGap(g);
    await cancelOnlyA(s);
    s.setCanonical(gapState());
    const fresh = s.holdSync();
    http.respond();
    await settle();
    expect(s.manager.sync_now).toHaveBeenCalledOnce();
    earlier.resolve();
    await settle();
    expect.soft(s.manager.sync_now).toHaveBeenCalledTimes(2);
    expect.soft(s.session.state.value.ready).toBe(false);
    fresh.resolve();
    await settle();
    await g.showing;
    expect.soft(s.controller.secrets.state.value.cards['pass-b']).toBeUndefined();
    safe(s);
  });

  it.each(['generation', 'manager'] as const)(
    'retired HTTP cannot synchronize or satisfy the public %s replacement',
    async (boundary) => {
      const g = await prepare(),
        { s } = g;
      const http = await openGap(g);
      await cancelOnlyA(s);
      s.setCanonical(gapState());
      const previousIdentity = s.session.state.value.identity;
      const nextManager = boundary === 'manager' ? managerFixture() : s.manager;
      if (boundary === 'manager') {
        const member = memberFixture(JSON.stringify(gapState()));
        vi.mocked(member.vaultData.secrets).mockReturnValue([pass('pass-a', names['pass-a'])]);
        vi.mocked(nextManager.get_state).mockResolvedValue(stateFixture(member.member).state);
        const { WasmApplicationManager } = await import('meta-secret-web-cli');
        vi.mocked(WasmApplicationManager.init_wasm_with_device_and_environment).mockResolvedValueOnce(nextManager);
      }
      s.store.invalidateSession();
      await s.store.appStateInit();
      await settle();
      expect(s.store.appManager).toBe(nextManager);
      expect(s.session.state.value.identity).not.toBe(previousIdentity);
      expect(http.signal.aborted).toBe(true);
      const current = s.transport.requests.find((request) => !request.signal.aborted)!;
      current.respond();
      await debounce(); // Ordinary replacement bootstrap is drained before retiring headers resolve.
      const reads = vi.mocked(nextManager.get_state).mock.calls.length;
      const syncs = vi.mocked(nextManager.sync_now).mock.calls.length;
      http.respond();
      await settle();
      await g.showing;
      await debounce();
      expect(nextManager.get_state).toHaveBeenCalledTimes(reads);
      expect(nextManager.sync_now).toHaveBeenCalledTimes(syncs);
      if (boundary === 'manager')
        for (const method of [
          nextManager.show_recovered,
          nextManager.show_local_secret,
          nextManager.recover_checked,
          nextManager.delete_secret_checked,
          nextManager.remove_member_checked,
          nextManager.continue_local_remove_checked,
        ])
          expect(method).not.toHaveBeenCalled();
      expect.soft(s.session.state.value.ready).toBe(true);
      expect(vi.mocked(s.manager.sync_now).mock.calls).toHaveLength(0);
      safe(s);
    },
  );

  it.each(['offline', 'hidden', 'pagehide'] as const)(
    '%s defers gap closure until the normal eligible online/pageshow owner',
    async (boundary) => {
      const g = await prepare(),
        { s } = g;
      const http = await openGap(g);
      await cancelOnlyA(s);
      s.setCanonical(gapState());
      const visibility = boundary === 'hidden' ? vi.spyOn(document, 'visibilityState', 'get') : null;
      if (boundary === 'offline') window.dispatchEvent(new Event('offline'));
      else if (visibility) {
        visibility.mockReturnValue('hidden');
        document.dispatchEvent(new Event('visibilitychange'));
      } else window.dispatchEvent(new Event('pagehide'));
      http.respond();
      await settle();
      await g.showing;
      expect(s.manager.sync_now).not.toHaveBeenCalled();
      expect(s.session.state.value.ready).toBe(false);
      safe(s);
      if (boundary === 'offline') window.dispatchEvent(new Event('online'));
      else if (visibility) {
        visibility.mockReturnValue('visible');
        document.dispatchEvent(new Event('visibilitychange'));
      } else window.dispatchEvent(new Event('pageshow'));
      await settle();
      for (const request of s.transport.requests) if (!request.signal.aborted && !request.subscribed) request.respond();
      await debounce();
      expect.soft(s.manager.sync_now).toHaveBeenCalledOnce();
      expect.soft(s.session.state.value.ready).toBe(true);
      expect.soft(s.controller.secrets.state.value.cards['pass-b']).toBeUndefined();
      safe(s);
    },
  );

  it.each(['auth', 'key', 'dispose'] as const)(
    '%s fences old HTTP work without restoring either value',
    async (boundary) => {
      const g = await prepare(),
        { s } = g;
      const http = await openGap(g);
      await cancelOnlyA(s);
      s.setCanonical(gapState());
      if (boundary === 'auth') s.auth.isAuthenticated = false;
      if (boundary === 'key') s.auth.masterKey = 'replacement-fixture-key';
      if (boundary === 'dispose') s.session.dispose();
      await settle();
      if (boundary === 'key') {
        for (const request of s.transport.requests) if (!request.signal.aborted) request.respond();
        await debounce();
      }
      expect(http.signal.aborted).toBe(true);
      const reads = vi.mocked(s.manager.get_state).mock.calls.length;
      http.respond();
      await settle();
      await g.showing;
      await debounce();
      expect(s.manager.get_state).toHaveBeenCalledTimes(reads);
      expect(s.manager.sync_now).not.toHaveBeenCalled();
      safe(s);
    },
  );
});
