import { expect, vi } from 'vitest';
import { flushPromises, mount } from '@vue/test-utils';
import { defineComponent, effectScope, h, reactive } from 'vue';
import { createPinia, setActivePinia } from 'pinia';
import type { ClaimId } from 'meta-secret-web-cli';
import SecretsPage from '@/components/liveboard/SecretsPage.vue';
import { AppState } from '@/stores/app-state';
import { useVaultSession } from '@/features/liveboard/useVaultSession';
import { createStateInvalidationController } from '@/utils/stateInvalidation';
import type { CoreOperation } from '@/features/liveboard/coreTypes';
import {
  claim,
  coreState,
  deferred,
  managerFixture,
  memberFixture,
  pass,
  platformFixture,
  stateFixture,
} from './liveboardCoreFixtures';

const sdk = vi.hoisted(() => ({
  initialize: vi.fn(),
  initializeWasm: vi.fn(),
  auth: { isAuthenticated: true, masterKey: 'fixture-key' as string | null },
}));
vi.mock('meta-secret-web-cli', () => ({
  default: sdk.initializeWasm,
  ApplicationStateInfo: { Local: 0, Member: 1, Outsider: 2, VaultNotExists: 3 },
  MasterKeyManager: { from_pure_sk: vi.fn(() => ({})) },
  WasmApplicationManager: { init_wasm_with_device_and_environment: sdk.initialize },
}));
vi.mock('@/stores/auth', () => ({ useAuthStore: () => sdk.auth }));

export const vaultName = 'семья@example.test';
export const rawValue = 'owned recovered sample';
export const intent = { passId: 'pass-a', claimId: 'claim-second' };
export const accepted = (patch: Partial<CoreOperation> = {}): CoreOperation => ({
  kind: 'recover',
  requestId: intent.claimId,
  secretId: intent.passId,
  requesterId: 'self',
  clientStatus: 'accepted',
  outcome: 'accepted',
  eligiblePeerIds: ['peer-a'],
  responderId: 'peer-a',
  canApprove: false,
  canDecline: false,
  resolutionSource: 'verified',
  readiness: 'ready',
  ...patch,
});
export const historical = accepted({
  requestId: 'claim-first',
  clientStatus: 'declined',
  outcome: 'declined',
  readiness: 'waiting',
});
type RawSecret = {
  secretId: string;
  primaryAction: string;
  canDelete: boolean;
  readiness: string;
  recoveryClaimId?: string;
  deletionRequestId?: string;
};
export type Canonical = Omit<ReturnType<typeof coreState>, 'secrets' | 'operations'> & {
  secrets: RawSecret[];
  operations: CoreOperation[];
};
export function canonical(done = false): Canonical {
  return {
    ...coreState(),
    storageMode: 'copies',
    secrets: [
      {
        secretId: 'pass-a',
        primaryAction: done ? 'recover' : 'showRecovered',
        recoveryClaimId: done ? undefined : intent.claimId,
        canDelete: true,
        readiness: 'ready',
      },
      { secretId: 'pass-b', primaryAction: 'showLocal', canDelete: true, readiness: 'ready' },
    ],
    operations: [historical, accepted(done ? { clientStatus: 'done', readiness: 'done' } : {})],
  };
}
export type ReadyLease = { isCurrent(): boolean; onLoss(listener: () => void): () => void };
export type Restartable = ReturnType<typeof createStateInvalidationController> & {
  restartReady(name: string): Promise<ReadyLease>;
};
export function controlledTransport(auto = false) {
  const requests: Array<{
    signal: AbortSignal;
    body: ReadableStream<Uint8Array>;
    response: ReturnType<typeof deferred<Response>>;
    subscribed: boolean;
    closed: boolean;
    push(): void;
    close(): void;
    fail(): void;
    respond(status?: number, body?: boolean): void;
  }> = [];
  const fetchImpl = vi.fn<typeof fetch>().mockImplementation(async (_url, init) => {
    let stream!: ReadableStreamDefaultController<Uint8Array>;
    const body = new ReadableStream<Uint8Array>({
      start(controller) {
        stream = controller;
      },
    });
    const response = deferred<Response>();
    const request = {
      signal: init!.signal as AbortSignal,
      body,
      response,
      subscribed: false,
      closed: false,
      push() {
        if (!request.closed)
          stream.enqueue(
            new TextEncoder().encode(
              `event: state_invalidated\ndata: ${JSON.stringify({ type: 'state_invalidated', vaultName, scope: 'ss_claims' })}\n\n`,
            ),
          );
      },
      close() {
        if (!request.closed) {
          request.closed = true;
          stream.close();
        }
      },
      fail() {
        if (!request.closed) {
          request.closed = true;
          stream.error(new Error('TransportUnavailable'));
        }
      },
      respond(status = 200, withBody = true) {
        request.subscribed = status === 200 && withBody;
        response.resolve({ ok: status === 200, status, body: withBody ? body : null } as Response);
      },
    };
    requests.push(request);
    if (auto) request.respond();
    return response.promise;
  });
  return {
    requests,
    fetchImpl,
    setAutoHeaders(value: boolean) {
      auto = value;
    },
    broadcast() {
      for (const request of requests) if (request.subscribed && !request.signal.aborted) request.push();
    },
    dispose() {
      for (const request of requests) request.close();
    },
  };
}
const disposals: Array<() => void> = [];
export function resetRevealHarness() {
  vi.useFakeTimers();
  vi.stubGlobal('matchMedia', () => ({ matches: true, addEventListener: vi.fn(), removeEventListener: vi.fn() }));
  sdk.initialize.mockReset();
  sdk.initializeWasm.mockReset().mockResolvedValue(undefined);
  setActivePinia(createPinia());
}
export async function cleanupRevealHarness() {
  disposals
    .splice(0)
    .reverse()
    .forEach((dispose) => dispose());
  await settle();
  vi.restoreAllMocks();
  vi.clearAllTimers();
  vi.useRealTimers();
  vi.unstubAllGlobals();
}
export const settle = async () => {
  await flushPromises();
  await flushPromises();
};
export const debounce = async () => {
  await vi.advanceTimersByTimeAsync(150);
  await settle();
};

export async function setupReveal(options: { autoHeaders?: boolean; names?: Readonly<Record<string, string>> } = {}) {
  const transport = controlledTransport(options.autoHeaders ?? true);
  const manager = managerFixture();
  const platform = platformFixture();
  const auth = reactive({ isAuthenticated: true, masterKey: 'fixture-key' as string | null });
  sdk.auth = auth;
  const store = AppState();
  const raw = deferred<string>();
  const claims: ClaimId[] = [];
  const roots: ReturnType<typeof stateFixture>[] = [];
  const order: string[] = [];
  const physical = { active: 0, maximum: 0 };
  const enter = () => {
    physical.active++;
    physical.maximum = Math.max(physical.maximum, physical.active);
  };
  let current: Canonical = canonical();
  let member = true;
  let syncGate: ReturnType<typeof deferred<void>> | undefined;
  let readGate: ReturnType<typeof deferred<void>> | undefined;
  const gates: Array<ReturnType<typeof deferred<void>>> = [];
  vi.mocked(manager.get_state).mockImplementation(async () => {
    enter();
    try {
      order.push('read');
      if (readGate) {
        const gate = readGate;
        readGate = undefined;
        await gate.promise;
      }
      const snapshot = current;
      const fixture = memberFixture(JSON.stringify(snapshot));
      vi.mocked(fixture.vaultData.secrets).mockImplementation(() =>
        snapshot.secrets.map((item) => pass(item.secretId, options.names?.[item.secretId])),
      );
      vi.mocked(fixture.member.recovery_claim_by_id).mockImplementation((id) => {
        const owned = claim(id);
        claims.push(owned);
        return owned;
      });
      const root = stateFixture(fixture.member);
      root.vault.is_member = () => member;
      roots.push(root);
      return root.state;
    } finally {
      physical.active--;
    }
  });
  vi.mocked(manager.sync_now).mockImplementation(async () => {
    enter();
    try {
      order.push('sync');
      if (syncGate) {
        const gate = syncGate;
        syncGate = undefined;
        await gate.promise;
      }
    } finally {
      physical.active--;
    }
  });
  vi.mocked(manager.show_recovered).mockImplementation(async () => raw.promise);
  sdk.initialize.mockResolvedValue(manager);
  const scope = effectScope();
  let connection!: Restartable;
  const leases: ReadyLease[] = [];
  const listeners = { attached: 0, active: 0 };
  const session = scope.run(() =>
    useVaultSession({
      store,
      auth,
      platform,
      createInvalidation(input) {
        connection = createStateInvalidationController({
          ...input,
          fetchImpl: transport.fetchImpl,
          resolveUrl: () => 'http://localhost/state-events',
        }) as Restartable;
        if (typeof connection.restartReady === 'function') {
          const restart = connection.restartReady.bind(connection);
          vi.spyOn(connection, 'restartReady').mockImplementation(async (name) => {
            const lease = await restart(name);
            leases.push(lease);
            const subscribe = lease.onLoss.bind(lease);
            vi.spyOn(lease, 'onLoss').mockImplementation((listener) => {
              listeners.attached++;
              listeners.active++;
              const unsubscribe = subscribe(listener);
              let active = true;
              return () => {
                if (active) {
                  active = false;
                  listeners.active--;
                }
                unsubscribe();
              };
            });
            return lease;
          });
        }
        return connection;
      },
    }),
  )!;
  const controller = session.controller;
  const wrapper = mount(
    defineComponent({
      setup: () => () =>
        h(SecretsPage, {
          view: session.controller.secrets.state.value,
          backHref: '/',
          onPrimary: session.controller.secrets.primary,
          onCopy: session.controller.secrets.copy,
          onSearch: session.controller.search,
        }),
    }),
    { global: { stubs: { RouterLink: { template: '<a><slot /></a>' } } } },
  );
  disposals.push(() => {
    wrapper.unmount();
    session.dispose();
    scope.stop();
    transport.dispose();
    raw.resolve(rawValue);
    gates.forEach((gate) => gate.resolve());
  });
  await settle();
  if (!options.autoHeaders && options.autoHeaders !== undefined) transport.requests[0]!.respond();
  await debounce();
  controller.navigate('secrets');
  await settle();
  expect(session.state.value.ready).toBe(true);
  expect(controller.secrets.state.value.cards['pass-a'].secret.primary).toEqual({
    kind: 'show-recovered',
    claimId: intent.claimId,
  });
  const setCanonical = (value = canonical(true)) => {
    current = value;
  };
  return {
    store,
    auth,
    manager,
    platform,
    session,
    controller,
    wrapper,
    raw,
    claims,
    roots,
    order,
    physical,
    connection,
    transport,
    leases,
    listeners,
    setCanonical,
    setMember(value: boolean) {
      member = value;
    },
    holdSync() {
      syncGate = deferred<void>();
      gates.push(syncGate);
      return syncGate;
    },
    holdRead() {
      readGate = deferred<void>();
      gates.push(readGate);
      return readGate;
    },
    start: () => controller.secrets.primary('pass-a'),
    card: () => controller.secrets.state.value.cards['pass-a'],
  };
}
export type RevealSetup = Awaited<ReturnType<typeof setupReveal>>;
export function expectNoWrites(s: RevealSetup) {
  expect(s.manager.show_recovered).toHaveBeenCalledOnce();
  expect(s.platform.authenticate).not.toHaveBeenCalled();
  for (const method of [
    s.manager.recover_checked,
    s.manager.recover_js,
    s.manager.delete_secret_checked,
    s.manager.continue_local_remove_checked,
    s.manager.cluster_distribution_checked,
    s.manager.remove_member_checked,
  ])
    expect(method).not.toHaveBeenCalled();
}
export function expectNoValue(s: RevealSetup) {
  expect(
    Object.values(s.controller.secrets.state.value.cards).every((card) => card.value === null && !card.copied),
  ).toBe(true);
  expect(s.wrapper.text()).not.toContain(rawValue);
  expect(s.wrapper.find('[data-secret-action="copy"]').exists()).toBe(false);
  expect(JSON.stringify(s.store.$state)).not.toContain(rawValue);
  expect(JSON.stringify(s.session.state.value)).not.toContain(rawValue);
}
