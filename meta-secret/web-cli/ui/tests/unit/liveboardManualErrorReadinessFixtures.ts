import { expect, vi } from 'vitest';
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils';
import { markRaw, reactive } from 'vue';
import { createPinia, setActivePinia } from 'pinia';
import type { WasmApplicationManager } from 'meta-secret-web-cli';
import App from '@/App.vue';
import router from '@/router';
import { AppState } from '@/stores/app-state';
import type { provideLiveboardRouteSession } from '@/features/liveboard/routeContext';
import { registrationSurfaceActive } from '@/features/registration/registrationSurface';
import {
  coreState,
  deferred,
  managerFixture,
  memberFixture,
  operation,
  pass,
  stateFixture,
} from './liveboardCoreFixtures';

type RouteContext = ReturnType<typeof provideLiveboardRouteSession>;
export const ids = ['claim-first', 'claim-second'] as const;
export const values = { 'pass-a': 'Alpha local sample', 'pass-b': 'Beta local sample' };
export const copyError = 'Could not copy this secret.';
const hooks = vi.hoisted(() => ({
  auth: { isAuthenticated: true, masterKey: 'fixture-key' as string | null, authenticateWithPasskey: vi.fn() },
  initialize: vi.fn(),
  context: null as RouteContext | null,
  motions: [] as Array<{ readonly activeCount: number }>,
}));
vi.mock('@/stores/auth', () => ({ useAuthStore: () => hooks.auth }));
vi.mock('@/stores/theme', () => ({ useThemeStore: vi.fn() }));
vi.mock('meta-secret-web-cli', () => ({
  default: vi.fn(async () => {}),
  ApplicationStateInfo: { Local: 0, Member: 1, Outsider: 2, VaultNotExists: 3 },
  PlainPassInfo: class {
    free = vi.fn();
    [Symbol.dispose] = vi.fn();
    pass_id: ReturnType<typeof pass>;
    ['pass']: string;
    constructor(name: string, text: string) {
      this.pass = text;
      this.pass_id = pass('pass-new', name);
    }
  },
  MasterKeyManager: { from_pure_sk: vi.fn(() => ({})) },
  WasmApplicationManager: { init_wasm_with_device_and_environment: hooks.initialize },
}));
vi.mock('@/features/liveboard/routeContext', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/features/liveboard/routeContext')>();
  return {
    ...actual,
    provideLiveboardRouteSession(session: RouteContext['session']) {
      const context = actual.provideLiveboardRouteSession(session);
      hooks.context = context;
      return context;
    },
  };
});
vi.mock('@/features/liveboard/revealMotion', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/features/liveboard/revealMotion')>();
  return {
    ...actual,
    createRevealMotion() {
      const motion = actual.createRevealMotion();
      hooks.motions.push(motion);
      return motion;
    },
  };
});
vi.mock('@/components/registration/OrbitRegistration.vue', () => ({ default: { template: '<div data-orbit />' } }));
const descriptors = {
  animate: Object.getOwnPropertyDescriptor(Element.prototype, 'animate'),
  range: Object.getOwnPropertyDescriptor(Range.prototype, 'getBoundingClientRect'),
  showModal: Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, 'showModal'),
  close: Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, 'close'),
  clipboard: Object.getOwnPropertyDescriptor(navigator, 'clipboard'),
};
export const effects: Array<{ cancel: ReturnType<typeof vi.fn>; numeric: number[] }> = [];
const cleanups: Array<() => void> = [];
export const settle = async () => {
  await flushPromises();
  await flushPromises();
};
export async function tick() {
  await vi.advanceTimersByTimeAsync(150);
  await settle();
}
export function before() {
  vi.useFakeTimers();
  hooks.context = null;
  hooks.motions.length = effects.length = 0;
  hooks.initialize.mockReset();
  hooks.auth = reactive({
    isAuthenticated: true,
    masterKey: 'fixture-key',
    authenticateWithPasskey: vi.fn(async () => true),
  });
  registrationSurfaceActive.value = false;
  vi.stubGlobal('matchMedia', () => ({ matches: false, addEventListener: vi.fn(), removeEventListener: vi.fn() }));
  if (!descriptors.animate) Object.defineProperty(Element.prototype, 'animate', { configurable: true, value: vi.fn() });
  if (!descriptors.range)
    Object.defineProperty(Range.prototype, 'getBoundingClientRect', { configurable: true, value: vi.fn() });
  Object.defineProperty(HTMLDialogElement.prototype, 'showModal', {
    configurable: true,
    value(this: HTMLDialogElement) {
      this.open = true;
    },
  });
  Object.defineProperty(HTMLDialogElement.prototype, 'close', {
    configurable: true,
    value(this: HTMLDialogElement) {
      this.open = false;
    },
  });
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue(new DOMRect(0, 0, 8, 8));
  vi.spyOn(Range.prototype, 'getBoundingClientRect').mockReturnValue(new DOMRect(0, 0, 1, 1));
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockImplementation(
    () =>
      ({
        fillText: vi.fn(),
        getImageData: () => ({ data: new Uint8ClampedArray(256).fill(255) }),
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
}
export async function after() {
  cleanups
    .splice(0)
    .reverse()
    .forEach((fn) => fn());
  await settle();
  registrationSurfaceActive.value = false;
  vi.restoreAllMocks();
  vi.clearAllTimers();
  vi.useRealTimers();
  vi.unstubAllGlobals();
  for (const [target, key, descriptor] of [
    [Element.prototype, 'animate', descriptors.animate],
    [Range.prototype, 'getBoundingClientRect', descriptors.range],
    [HTMLDialogElement.prototype, 'showModal', descriptors.showModal],
    [HTMLDialogElement.prototype, 'close', descriptors.close],
    [navigator, 'clipboard', descriptors.clipboard],
  ] as const) {
    if (descriptor) Object.defineProperty(target, key, descriptor);
    else Reflect.deleteProperty(target, key);
  }
}
export const queue = () =>
  ids.map((requestId, index) => operation({ requestId, secretId: index ? 'pass-b' : 'pass-a' }));
export const localDTO = () =>
  coreState({
    storageMode: 'copies',
    secrets: Object.keys(values).map((secretId) => ({
      secretId,
      primaryAction: 'showLocal',
      canDelete: true,
      readiness: 'ready',
    })),
    operations: queue(),
  });
export async function setup(path = '/secrets') {
  const pinia = createPinia();
  setActivePinia(pinia);
  const store = AppState();
  const manager = managerFixture();
  const clipboard = {
    writeText: vi.fn<(text: string) => Promise<void>>().mockResolvedValue(),
    readText: vi.fn(async () => ''),
  };
  Object.defineProperty(navigator, 'clipboard', { configurable: true, value: clipboard });
  let dto = localDTO(),
    member = true,
    autoHeaders = true;
  const order: string[] = [],
    stages: string[] = [];
  const roots: ReturnType<typeof stateFixture>[] = [];
  const physical = { active: 0, maximum: 0 };
  const syncs: Array<ReturnType<typeof deferred<void>>> = [],
    reads: Array<ReturnType<typeof deferred<void>>> = [];
  const allGates: Array<ReturnType<typeof deferred<void>>> = [];
  const requests: Array<{ signal: AbortSignal; body: ReadableStream<Uint8Array>; respond(): void; close(): void }> = [];
  const fetcher = vi.fn<typeof fetch>().mockImplementation(async (_url, input) => {
    expect(new Headers(input?.headers).get('Authorization')).toBe('Bearer state-token');
    let control!: ReadableStreamDefaultController<Uint8Array>;
    const body = new ReadableStream<Uint8Array>({
      start(value) {
        control = value;
      },
    });
    const response = deferred<Response>();
    let closed = false;
    const request = {
      signal: input!.signal as AbortSignal,
      body,
      respond: () => response.resolve({ ok: true, status: 200, body } as Response),
      close() {
        if (!closed) {
          closed = true;
          control.close();
        }
      },
    };
    requests.push(request);
    if (autoHeaders) request.respond();
    return response.promise;
  });
  vi.stubGlobal('fetch', fetcher);
  const enter = () => {
    physical.active++;
    physical.maximum = Math.max(physical.maximum, physical.active);
  };
  const makeRoot = () => {
    const fixture = memberFixture(JSON.stringify(dto));
    vi.mocked(fixture.vaultData.secrets).mockImplementation(() => [pass('pass-a', 'Alpha'), pass('pass-b', 'Beta')]);
    const root = stateFixture(fixture.member);
    root.vault.is_member = () => member;
    if (!member) root.state.as_info = () => 2;
    roots.push(root);
    return root.state;
  };
  vi.mocked(manager.get_state).mockImplementation(async () => {
    enter();
    order.push('read');
    const gate = reads.shift();
    const root = makeRoot();
    try {
      if (gate) await gate.promise;
      return root;
    } finally {
      physical.active--;
    }
  });
  vi.mocked(manager.sync_now).mockImplementation(async () => {
    enter();
    order.push('sync');
    const gate = syncs.shift();
    try {
      if (gate) await gate.promise;
    } finally {
      physical.active--;
    }
  });
  vi.mocked(manager.show_local_secret).mockImplementation(async (id) => values[id.id_str() as keyof typeof values]);
  hooks.initialize.mockResolvedValue(manager);
  await router.push(path);
  let wrapper: VueWrapper | null = mount(App, {
    attachTo: document.body,
    global: {
      plugins: [pinia, router],
      stubs: {
        NavBar: true,
        PasskeyAuth: true,
        Toaster: true,
      },
    },
  });
  await router.isReady();
  await settle();
  await tick();
  const context = hooks.context!;
  expect(context).not.toBeNull();
  const session = context.session;
  expect(session.state.value.ready).toBe(true);
  expect(requests).toHaveLength(1);
  expect(requests[0]!.signal.aborted).toBe(false);
  const hold = (list: typeof syncs) => {
    const gate = deferred<void>();
    list.push(gate);
    allGates.push(gate);
    return gate;
  };
  const unmount = () => {
    wrapper?.unmount();
    wrapper = null;
  };
  cleanups.push(() => {
    unmount();
    requests.forEach((request) => {
      request.respond();
      request.close();
    });
    allGates.forEach((gate) => gate.resolve());
  });
  return {
    store,
    manager,
    clipboard,
    auth: hooks.auth,
    context,
    session,
    roots,
    order,
    physical,
    requests,
    fetcher,
    stages,
    get wrapper() {
      return wrapper!;
    },
    get controller() {
      return session.controller;
    },
    router,
    setDTO(next: ReturnType<typeof localDTO>) {
      dto = next;
    },
    holdSync: () => hold(syncs),
    holdRead: () => hold(reads),
    setAutoHeaders(value: boolean) {
      autoHeaders = value;
    },
    setMember(value: boolean) {
      member = value;
    },
    unmount,
    publish: () => store.updateStateWith(makeRoot()),
    replaceManager(next: WasmApplicationManager) {
      store.appManager = markRaw(next);
      store.updateStateWith(makeRoot());
    },
    stage(label: string) {
      stages.push(label);
      console.info('MANUAL_ERROR_STAGE', label);
    },
  };
}
export type Setup = Awaited<ReturnType<typeof setup>>;
export async function show(s: Setup, id = 'pass-a') {
  await s.wrapper.get(`[data-secret-id="${id}"] [data-secret-action="show"]`).trigger('click');
  await settle();
  expect(s.controller.secrets.state.value.cards[id]?.value).not.toBeNull();
}
export async function rejectedCopy(s: Setup, id = 'pass-a') {
  s.clipboard.writeText.mockRejectedValueOnce(new Error('clipboard-private-exception'));
  await s.wrapper.get(`[data-secret-id="${id}"] [data-secret-action="copy"]`).trigger('click');
  await settle();
  expect(s.controller.notifications.state.value.error).toBe(copyError);
}
export async function back(s: Setup) {
  await s.wrapper.get('a[aria-label="Back to Liveboard"]').trigger('click');
  await settle();
  expect(s.router.currentRoute.value.path).toBe('/');
}
export async function dismiss(s: Setup) {
  await s.wrapper.get('[data-testid="liveboard-notifications"] [aria-label="Dismiss error"]').trigger('click');
  await settle();
}
export function concealed(s: Setup) {
  expect
    .soft(Object.values(s.controller.secrets.state.value.cards).every((card) => card.value === null && !card.copied))
    .toBe(true);
  expect.soft(s.wrapper.find('[data-secret-action="copy"]').exists()).toBe(false);
  for (const value of Object.values(values)) expect.soft(s.wrapper.text()).not.toContain(value);
  expect.soft(hooks.motions.every((motion) => motion.activeCount === 0)).toBe(true);
  expect.soft(JSON.stringify(s.store.$state)).not.toContain('clipboard-private-exception');
}
export function liveMotion() {
  expect(hooks.motions.some((motion) => motion.activeCount > 0)).toBe(true);
  const live = effects.filter(
    (effect) => !effect.cancel.mock.calls.length && effect.numeric.some((n) => Number.isFinite(n) && n !== 0),
  );
  expect(live.length).toBeGreaterThan(0);
  expect(live.every((effect) => !effect.cancel.mock.calls.length)).toBe(true);
  return live;
}
export function noBusiness(s: Setup, allowed: string[] = []) {
  for (const name of [
    'accept_recover',
    'decline_recover',
    'decide_delete_secret',
    'decide_join_request',
    'decide_remove_member',
    'recover_checked',
    'recover_js',
    'delete_secret_checked',
    'delete_secret',
    'remove_member_checked',
    'remove_member_from_vault',
    'continue_local_remove_checked',
    'cluster_distribution_checked',
    'cluster_distribution',
    'generate_user_creds',
    'sign_up',
  ] as const)
    if (!allowed.includes(name)) expect(s.manager[name], name).not.toHaveBeenCalled();
}
export function retainedQueue(s: Setup) {
  expect
    .soft(s.controller.notifications.state.value.requests.map(({ kind, requestId }) => [kind, requestId]))
    .toEqual(ids.map((id) => ['recover', id]));
}
