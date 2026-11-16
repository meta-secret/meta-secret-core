import { resolveMetaSecretStateEventsBaseUrl } from '@/config/metaSecretEnvironment';

export type StateInvalidationScope = 'vault' | 'devices' | 'ss_claims' | 'all';

export type StateInvalidationEvent = {
  type: 'state_invalidated';
  vaultName: string;
  scope?: StateInvalidationScope;
  revision?: string;
  e2eSignal?: {
    type: 'recovery_decision';
    claimId: string;
    outcome: 'accepted' | 'declined';
  };
};

export type StateRefresh = (options?: { requiredSync?: boolean }) => Promise<unknown>;

export type StateInvalidationControllerOptions = {
  refresh: StateRefresh;
  syncBeforeRefresh?: StateRefresh;
  onInvalidate?: () => void;
  onUnavailable?: () => void;
  getAuthorization: () => Promise<string>;
  debounceMs?: number;
  reconnectDelayMs?: number;
  fetchImpl?: typeof fetch;
  resolveUrl?: (vaultName: string) => string;
};

export type StateInvalidationController = {
  connect: (vaultName: string) => void;
  disconnect: () => void;
  refreshNow: () => Promise<void>;
  restartReady: (vaultName: string) => Promise<StateEventsReadyLease>;
};
export type StateEventsReadyLease = {
  isCurrent(): boolean;
  onLoss(listener: () => void): () => void;
};

export const STATE_INVALIDATION_DEBOUNCE_MS = 150;
const DEFAULT_RECONNECT_DELAY_MS = 1_000;

export function resolveStateEventsUrl(vaultName: string) {
  const configuredUrl = import.meta.env.VITE_STATE_EVENTS_URL as string | undefined;
  const baseUrl = configuredUrl || resolveMetaSecretStateEventsBaseUrl();
  const url = new URL(baseUrl);
  url.searchParams.set('vaultName', vaultName);
  return url.toString();
}

type ParsedSseEvent = { event: string; data: string };

function parseSseMessage(message: string): ParsedSseEvent | null {
  let event = 'message';
  const dataLines: string[] = [];
  for (const line of message.split(/\r?\n/)) {
    if (line.startsWith('event:')) event = line.slice('event:'.length).trim();
    if (line.startsWith('data:')) dataLines.push(line.slice('data:'.length).trimStart());
  }
  if (dataLines.length === 0) return null;
  return { event, data: dataLines.join('\n') };
}

async function consumeSseStream(
  body: ReadableStream<Uint8Array>,
  onEvent: (event: ParsedSseEvent) => void,
  signal: AbortSignal,
) {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';
  try {
    while (true) {
      if (signal.aborted) return;
      const { done, value } = await reader.read();
      if (signal.aborted) return;
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      const messages = buffer.split(/\r?\n\r?\n/);
      buffer = messages.pop() ?? '';
      for (const message of messages) {
        const parsed = parseSseMessage(message);
        if (parsed) onEvent(parsed);
      }
    }
    if (signal.aborted) return;
    buffer += decoder.decode();
    const parsed = parseSseMessage(buffer);
    if (parsed) onEvent(parsed);
  } finally {
    reader.releaseLock();
  }
}

function waitForReconnect(delayMs: number, signal: AbortSignal): Promise<void> {
  if (delayMs <= 0 || signal.aborted) return Promise.resolve();
  return new Promise((resolve) => {
    const timer = window.setTimeout(resolve, delayMs);
    signal.addEventListener(
      'abort',
      () => {
        window.clearTimeout(timer);
        resolve();
      },
      { once: true },
    );
  });
}

export function createStateInvalidationController({
  refresh,
  syncBeforeRefresh,
  onInvalidate,
  onUnavailable,
  getAuthorization,
  debounceMs = STATE_INVALIDATION_DEBOUNCE_MS,
  reconnectDelayMs = DEFAULT_RECONNECT_DELAY_MS,
  fetchImpl = fetch,
  resolveUrl = resolveStateEventsUrl,
}: StateInvalidationControllerOptions): StateInvalidationController {
  let streamAbortController: AbortController | null = null;
  let connectedVaultName = '';
  let refreshTimer: number | null = null;
  let isRefreshing = false;
  let pendingRefresh = false;
  let pendingSync = false;
  let scheduledSync = false;
  let streamGeneration = 0;
  let loseLease: (() => void) | null = null;
  let rejectReady: (() => void) | null = null;

  const clearRefreshTimer = () => {
    if (refreshTimer === null) return;
    window.clearTimeout(refreshTimer);
    refreshTimer = null;
  };

  const refreshNow = async (syncFirst = false) => {
    clearRefreshTimer();
    if (isRefreshing) {
      pendingRefresh = true;
      pendingSync ||= syncFirst;
      return;
    }

    isRefreshing = true;
    const generation = streamGeneration;
    try {
      if (syncFirst && syncBeforeRefresh) {
        await syncBeforeRefresh();
      }
      if (generation !== streamGeneration) return;
      await refresh({ requiredSync: syncFirst && !syncBeforeRefresh });
    } catch {
      console.warn('[StateEvents] state synchronization unavailable');
    } finally {
      isRefreshing = false;
      if (pendingRefresh) {
        pendingRefresh = false;
        const shouldSync = pendingSync;
        pendingSync = false;
        await refreshNow(shouldSync);
      }
    }
  };

  const scheduleRefresh = (syncFirst = false) => {
    if (isRefreshing) {
      pendingRefresh = true;
      pendingSync ||= syncFirst;
      return;
    }
    scheduledSync ||= syncFirst;
    if (refreshTimer !== null) return;
    refreshTimer = window.setTimeout(() => {
      refreshTimer = null;
      const shouldSync = scheduledSync;
      scheduledSync = false;
      void refreshNow(shouldSync);
    }, debounceMs);
  };

  const disconnect = () => {
    clearRefreshTimer();
    pendingRefresh = false;
    pendingSync = false;
    scheduledSync = false;
    streamGeneration += 1;
    rejectReady?.();
    rejectReady = null;
    loseLease?.();
    loseLease = null;
    if (!streamAbortController) return;
    console.log('[StateEvents] socket disconnected', { vaultName: connectedVaultName });
    streamAbortController.abort();
    streamAbortController = null;
  };

  const runStream = async (
    vaultName: string,
    generation: number,
    abortController: AbortController,
    ready?: { resolve(lease: StateEventsReadyLease): void; reject(): void },
  ) => {
    let hasConnected = false;
    let hasLost = false;
    const current = () =>
      generation === streamGeneration && !abortController.signal.aborted && connectedVaultName === vaultName;
    while (generation === streamGeneration && !abortController.signal.aborted) {
      try {
        console.log('[StateEvents] requesting authorization', { vaultName });
        const authorization = await getAuthorization();
        if (generation !== streamGeneration || abortController.signal.aborted || connectedVaultName !== vaultName)
          return;
        const response = await fetchImpl(resolveUrl(vaultName), {
          headers: {
            Accept: 'text/event-stream',
            'Cache-Control': 'no-cache',
            Authorization: `Bearer ${authorization}`,
          },
          signal: abortController.signal,
        });
        if (generation !== streamGeneration || abortController.signal.aborted || connectedVaultName !== vaultName)
          return;
        if (!response.ok) {
          console.warn('[StateEvents] authorization rejected', { vaultName, status: response.status });
          throw new Error(`state events HTTP ${response.status}`);
        }
        if (!response.body) throw new Error('state events response has no body');
        let live = true;
        const listeners = new Set<() => void>();
        loseLease = () => {
          if (!live) return;
          live = false;
          for (const listener of listeners) listener();
          listeners.clear();
        };
        const lease: StateEventsReadyLease = {
          isCurrent: () => live && current(),
          onLoss(listener) {
            if (!live || !current()) listener();
            else listeners.add(listener);
            return () => listeners.delete(listener);
          },
        };
        console.log(hasConnected ? '[StateEvents] socket reconnected' : '[StateEvents] socket connected', {
          vaultName,
        });
        // HTTP headers/body follow the server's subscription. There is no initial ready frame.
        if (ready) {
          ready.resolve(lease);
          ready = undefined;
          rejectReady = null;
        } else scheduleRefresh(hasLost);
        hasConnected = true;
        await consumeSseStream(
          response.body,
          (message) => {
            if (generation !== streamGeneration || abortController.signal.aborted || connectedVaultName !== vaultName)
              return;
            if (message.event !== 'state_invalidated') return;
            try {
              const event = JSON.parse(message.data) as StateInvalidationEvent;
              if (event.type !== 'state_invalidated' || event.vaultName !== connectedVaultName) return;
              console.log('[StateEvents] invalidation received', {
                vaultName: event.vaultName,
                scope: event.scope,
                revision: event.revision,
              });
              if (event.e2eSignal?.type === 'recovery_decision') {
                console.log(
                  `E2E_SSE_RECOVERY_DECISION claimId=${event.e2eSignal.claimId} ` +
                    `outcome=${event.e2eSignal.outcome}`,
                );
              }
              onInvalidate?.();
              if (generation !== streamGeneration || abortController.signal.aborted || connectedVaultName !== vaultName)
                return;
              scheduleRefresh(true);
            } catch {
              console.warn('[StateEvents] invalid invalidation payload');
            }
          },
          abortController.signal,
        );
      } catch {
        if (abortController.signal.aborted || generation !== streamGeneration) return;
        console.warn('[StateEvents] socket disconnected', { vaultName });
      }
      if (!current()) return;
      loseLease?.();
      loseLease = null;
      ready?.reject();
      ready = undefined;
      rejectReady = null;
      hasLost = true;
      onUnavailable?.();
      await waitForReconnect(reconnectDelayMs, abortController.signal);
    }
  };

  const connect = (vaultName: string) => {
    const nextVaultName = vaultName.trim();
    if (!nextVaultName) {
      disconnect();
      connectedVaultName = '';
      return;
    }
    if (streamAbortController && connectedVaultName === nextVaultName) return;

    disconnect();
    connectedVaultName = nextVaultName;
    const abortController = new AbortController();
    streamAbortController = abortController;
    const generation = streamGeneration;
    void runStream(nextVaultName, generation, abortController);
  };
  const restartReady = (vaultName: string): Promise<StateEventsReadyLease> => {
    disconnect(); // Retire the old generation/timers before any authorization await.
    connectedVaultName = vaultName.trim();
    if (!connectedVaultName) return Promise.reject(new Error('StateEventsUnavailable'));
    const abortController = new AbortController();
    streamAbortController = abortController;
    return new Promise((resolve, reject) => {
      const fail = () => reject(new Error('StateEventsUnavailable'));
      rejectReady = fail;
      void runStream(connectedVaultName, streamGeneration, abortController, { resolve, reject: fail });
    });
  };
  return { connect, disconnect, refreshNow, restartReady };
}
