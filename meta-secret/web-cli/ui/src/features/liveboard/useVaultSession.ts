import { computed, getCurrentScope, onScopeDispose, readonly, ref, shallowRef, watch } from 'vue';
import { AppState } from '@/stores/app-state';
import { useAuthStore } from '@/stores/auth';
import {
  createStateInvalidationController,
  type StateInvalidationControllerOptions,
  type StateInvalidationController,
} from '@/utils/stateInvalidation';
import { createCoreAdapter } from './coreAdapter';
import { createLiveboardController } from './controller';
import { createSessionContext } from './sessionContext';
import { createSessionError } from './sessionError';
import { createSessionRefresh, type SessionRefreshRequest } from './sessionRefresh';
import type { CorePlatform } from './coreTypes';
import { createRecoveredReveal } from './recoveredReveal';
type Auth = { isAuthenticated: boolean; masterKey: string | null; authenticateWithPasskey?(): Promise<boolean> };
type Controller = ReturnType<typeof createLiveboardController>;
type Options = {
  store?: ReturnType<typeof AppState>;
  auth?: Auth;
  platform?: CorePlatform;
  version?: string;
  createInvalidation?(
    input: StateInvalidationControllerOptions,
  ): Omit<StateInvalidationController, 'restartReady'> & Partial<Pick<StateInvalidationController, 'restartReady'>>;
};
const unavailable = () => new Error('OperationUnavailable');
const syncMessage = 'Could not synchronize Vault. Retry to apply pending changes.';
/** One parent owner. Child navigation only talks to the same presentation controller. */
export function useVaultSession(options: Options = {}) {
  const store = options.store ?? AppState();
  const auth = options.auth ?? useAuthStore();
  const owner = createSessionContext();
  const initialized = ref(false),
    initializing = ref(false),
    resetBusy = ref(false);
  const localFailure = ref(false),
    dismissed = ref(false);
  let disposed = false,
    revision = 0,
    required = 0,
    satisfied = 0;
  let pageActive = typeof document === 'undefined' || document.visibilityState !== 'hidden';
  let online = typeof navigator === 'undefined' || navigator.onLine;
  let connectedIdentity = '';
  const controller = shallowRef<Controller>();
  let resetOwner: { controller: Controller } | null = null;
  const eligible = () =>
    !disposed && !resetBusy.value && auth.isAuthenticated && Boolean(auth.masterKey) && initialized.value;
  const active = () => eligible() && online && pageActive;
  const errors = createSessionError({
    context: () => (eligible() ? [controller.value, revision] : null),
    active,
    revoke: () => recovered.revoke(),
    require: markRequired,
    read: () => refresh({ requiredSync: true }),
  });
  function block() {
    owner.block();
    controller.value?.beginSync({ preserveRecovered: recovered.allowsSync() });
  }
  function markRequired() {
    required++;
    dismissed.value = false;
    block();
  }
  const platform: CorePlatform = options.platform ?? {
    authenticate: () => {
      if (typeof auth.authenticateWithPasskey !== 'function') return Promise.reject(unavailable());
      return auth.authenticateWithPasskey();
    },
    copy: (text) => navigator.clipboard.writeText(text),
    readClipboard: () => navigator.clipboard.readText(),
    reportError: () => undefined,
  };
  const adapter = createCoreAdapter({
    getContext: () => owner.context.value,
    handoffRecovered: (request) => recovered.handoff(request),
    platform: {
      authenticate: () => platform.authenticate(),
      copy: (text) => platform.copy(text),
      readClipboard: () => platform.readClipboard(),
      reportError: (message) => errors.report(message, (error) => platform.reportError(error)),
    },
  });
  const makeController = () =>
    errors.bind((onError) =>
      createLiveboardController({ adapter, resetLocal, version: options.version ?? __WEB_UI_VERSION__, onError }),
    );
  controller.value = makeController();
  const connection = (options.createInvalidation ?? createStateInvalidationController)({
    refresh: (request) => refresh(request),
    onInvalidate: () => {
      recovered.invalidateStream();
      if (eligible()) markRequired();
    },
    onUnavailable: () => {
      if (!eligible()) return;
      recovered.revoke();
      localFailure.value = true;
      markRequired();
      controller.value!.showError(syncMessage);
    },
    getAuthorization: () => {
      if (!eligible() || !online || !owner.context.value) return Promise.reject(unavailable());
      return store.getStateEventsAuthToken(owner.vaultName());
    },
  } satisfies StateInvalidationControllerOptions);
  const recovered = createRecoveredReveal({
    controller: () => controller.value!.secrets,
    project: adapter.project,
    context: () => owner.context.value,
    lifetime: () => [active(), auth.masterKey, store.sessionGeneration, revision],
    restartReady: () =>
      connection.restartReady ? connection.restartReady(owner.vaultName()) : Promise.reject(unavailable()),
    sync: {
      require: markRequired,
      refresh: () => refresh({ requiredSync: true }),
      ready: () => ready() && Boolean(owner.context.value?.ready),
      target: () => required,
    },
  });
  function disconnect() {
    connectedIdentity = '';
    connection.disconnect();
  }
  function connect() {
    const context = owner.context.value;
    if (!eligible() || !online || !context) return;
    if (connectedIdentity === context.identity) return;
    disconnect();
    connectedIdentity = context.identity;
    connection.connect(owner.vaultName());
  }
  const ready = () =>
    active() &&
    !initializing.value &&
    required === satisfied &&
    !localFailure.value &&
    !store.requiredSyncPending &&
    !store.syncError &&
    !store.isSynchronizing;
  async function adopt() {
    if (disposed || initializing.value || resetBusy.value || !pageActive) return;
    let token = revision;
    try {
      const previousIdentity = owner.context.value?.identity;
      const changed = owner.publish({
        root: store.currState,
        manager: store.appManager,
        generation: store.sessionGeneration,
        ready: ready(),
      });
      if (changed && previousIdentity) {
        recovered.revoke();
        revision++;
        token = revision;
        required = satisfied = 0;
        localFailure.value = false;
        dismissed.value = false;
        owner.publish({
          root: store.currState,
          manager: store.appManager,
          generation: store.sessionGeneration,
          ready: ready(),
        });
      }
      if (changed || !owner.context.value) disconnect();
      if (owner.context.value && !adapter.project().session.member) {
        localFailure.value = true;
        block();
        controller.value!.showError(syncMessage);
      }
      recovered.observe(adapter.project());
      await controller.value!.refresh({ preserveRecovered: recovered.allowsSync() });
      if (!disposed && token === revision) connect();
    } catch {
      if (!disposed && token === revision) {
        recovered.revoke();
        localFailure.value = true;
        block();
        disconnect();
        owner.clear();
        controller.value!.showError(syncMessage);
      }
    }
  }
  function boundary() {
    recovered.revoke();
    revision++;
    block();
    disconnect();
    owner.clear();
    required = satisfied = 0;
    localFailure.value = false;
    dismissed.value = false;
  }
  const coordinator = createSessionRefresh({
    active: () => active() && Boolean(owner.context.value),
    revision: () => revision,
    target: () => required,
    require: () => {
      if (required === satisfied) markRequired();
      else block();
    },
    async run(request: SessionRefreshRequest) {
      const manager = store.appManager,
        generation = store.sessionGeneration;
      const key = auth.masterKey;
      const current = () =>
        active() &&
        request.revision === revision &&
        manager === store.appManager &&
        generation === store.sessionGeneration &&
        key === auth.masterKey;
      block();
      try {
        const result = await store.refreshState({ requiredSync: request.requiredSync });
        if (!current() || !result) return;
        if (request.requiredSync && request.target === required && !store.requiredSyncPending)
          satisfied = request.target;
        if ((!request.requiredSync || required === satisfied) && !store.syncError) {
          localFailure.value = false;
          dismissed.value = false;
        }
        await adopt();
      } catch {
        if (current()) {
          recovered.revoke();
          localFailure.value = true;
          dismissed.value = false;
          block();
          controller.value!.showError(syncMessage);
        }
        throw unavailable();
      }
    },
  });
  function refresh(request: { requiredSync?: boolean } = {}) {
    return coordinator.refresh(request);
  }
  async function initialize(token: number) {
    const key = auth.masterKey;
    initializing.value = true;
    try {
      await store.appStateInit();
      if (disposed || token !== revision || key !== auth.masterKey || !auth.isAuthenticated) return;
      initialized.value = true;
    } catch {
      if (!disposed && token === revision) localFailure.value = true;
    } finally {
      if (!disposed && token === revision) {
        initializing.value = false;
        await adopt();
      }
    }
  }
  async function resetLocal() {
    if (disposed || resetBusy.value) return;
    boundary();
    const reset = { controller: controller.value! };
    resetOwner = reset;
    resetBusy.value = true;
    initialized.value = false;
    const token = revision,
      key = auth.masterKey;
    try {
      await store.cleanDatabase();
      if (disposed || token !== revision || key !== auth.masterKey || !auth.isAuthenticated) throw unavailable();
      await store.appStateInit();
      if (disposed || token !== revision || key !== auth.masterKey || !auth.isAuthenticated) throw unavailable();
      initialized.value = true;
    } catch {
      if (!disposed && token === revision) localFailure.value = true;
      throw unavailable();
    } finally {
      if (resetOwner === reset) {
        const replacement = !disposed && controller.value === reset.controller ? makeController() : null;
        if (replacement) {
          controller.value = replacement;
          reset.controller.dispose();
        }
        resetOwner = null;
        resetBusy.value = false;
        if (replacement && resetOwner === null && controller.value === replacement) {
          await adopt();
          if (!disposed && resetOwner === null && controller.value === replacement && localFailure.value)
            replacement.showError(syncMessage);
        }
      }
    }
  }
  const stopAuth = watch(
    () => [auth.isAuthenticated, auth.masterKey] as const,
    () => {
      boundary();
      initialized.value = false;
      initializing.value = false;
      store.invalidateSession();
      if (auth.isAuthenticated && auth.masterKey) void initialize(revision);
    },
    { immediate: true, flush: 'sync' },
  );
  const stopStore = watch(
    () => [store.appManager, store.currState, store.sessionGeneration] as const,
    ([manager, root]) => {
      if (disposed || initializing.value || resetBusy.value) return;
      if (owner.manager() && manager !== owner.manager()) {
        const previousRoot = owner.root();
        boundary();
        if (root === previousRoot) return;
      }
      block();
      void adopt();
    },
    { flush: 'sync' },
  );
  const stopSync = watch(
    () => [store.isSynchronizing, store.requiredSyncPending, store.syncError] as const,
    ([busy, barrier, error]) => {
      if (disposed || initializing.value || resetBusy.value) return;
      if (error) recovered.revoke();
      if (busy || barrier || error) block();
      else void adopt();
    },
    { flush: 'sync' },
  );
  const handleOnline = () => {
    online = true;
    if (active()) void refresh({ requiredSync: true }).catch(() => undefined);
  };
  const handleOffline = () => {
    online = false;
    if (!disposed) {
      recovered.revoke();
      markRequired();
      disconnect();
    }
  };
  const pagehide = () => {
    recovered.revoke();
    pageActive = false;
    if (!disposed) block();
  };
  const pageshow = () => {
    pageActive = true;
    if (active()) void refresh({ requiredSync: required !== satisfied }).catch(() => undefined);
  };
  const visibility = () => {
    if (document.visibilityState === 'hidden') pagehide();
    else pageshow();
  };
  window.addEventListener('online', handleOnline);
  window.addEventListener('offline', handleOffline);
  window.addEventListener('pagehide', pagehide);
  window.addEventListener('pageshow', pageshow);
  document.addEventListener('visibilitychange', visibility);
  function dispose() {
    if (disposed) return;
    disposed = true;
    recovered.revoke();
    revision++;
    initialized.value = false;
    stopAuth();
    stopStore();
    stopSync();
    disconnect();
    owner.clear();
    controller.value!.dispose();
    store.invalidateSession();
    window.removeEventListener('online', handleOnline);
    window.removeEventListener('offline', handleOffline);
    window.removeEventListener('pagehide', pagehide);
    window.removeEventListener('pageshow', pageshow);
    document.removeEventListener('visibilitychange', visibility);
  }
  if (getCurrentScope()) onScopeDispose(dispose);
  const state = readonly(
    computed(() => ({
      ready: ready() && Boolean(owner.context.value?.ready && controller.value?.state.value.ready),
      initialized: initialized.value,
      member: Boolean(owner.context.value),
      loading: initializing.value || store.isSynchronizing,
      resetting: resetBusy.value,
      error: !dismissed.value && (localFailure.value || store.syncError) ? syncMessage : null,
      vaultName: owner.vaultName(),
      identity: owner.context.value?.identity ?? '',
      epoch: owner.context.value?.epoch ?? 0,
    })),
  );
  return {
    state,
    get controller() {
      return controller.value!;
    },
    refresh,
    dismissError: () => errors.dismiss(() => (dismissed.value = true)),
    dispose,
  };
}
