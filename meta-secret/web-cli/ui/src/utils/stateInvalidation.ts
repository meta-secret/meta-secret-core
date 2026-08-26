import { resolveMetaSecretStateEventsBaseUrl } from '@/config/metaSecretEnvironment';

export type StateInvalidationScope = 'vault' | 'devices' | 'ss_claims' | 'all';

export type StateInvalidationEvent = {
  type: 'state_invalidated';
  vaultName: string;
  scope?: StateInvalidationScope;
  revision?: string;
};

export type StateRefresh = () => Promise<unknown>;

export type StateInvalidationControllerOptions = {
  refresh: StateRefresh;
  debounceMs?: number;
  createEventSource?: (url: string) => EventSource;
  resolveUrl?: (vaultName: string) => string;
};

export type StateInvalidationController = {
  connect: (vaultName: string) => void;
  disconnect: () => void;
  refreshNow: () => Promise<void>;
};

export const STATE_INVALIDATION_DEBOUNCE_MS = 150;

export function resolveStateEventsUrl(vaultName: string) {
  const configuredUrl = import.meta.env.VITE_STATE_EVENTS_URL as string | undefined;
  const baseUrl = configuredUrl || resolveMetaSecretStateEventsBaseUrl();
  const url = new URL(baseUrl);
  url.searchParams.set('vaultName', vaultName);
  return url.toString();
}

export function createStateInvalidationController({
  refresh,
  debounceMs = STATE_INVALIDATION_DEBOUNCE_MS,
  createEventSource = (url) => new EventSource(url),
  resolveUrl = resolveStateEventsUrl,
}: StateInvalidationControllerOptions): StateInvalidationController {
  let eventSource: EventSource | null = null;
  let connectedVaultName = '';
  let refreshTimer: number | null = null;
  let isRefreshing = false;
  let pendingRefresh = false;
  let hasOpened = false;

  const clearRefreshTimer = () => {
    if (refreshTimer === null) return;
    window.clearTimeout(refreshTimer);
    refreshTimer = null;
  };

  const refreshNow = async () => {
    clearRefreshTimer();
    if (isRefreshing) {
      pendingRefresh = true;
      return;
    }

    isRefreshing = true;
    console.log('[StateEvents] getAppState refresh started');
    try {
      await refresh();
      console.log('[StateEvents] getAppState refresh finished');
    } catch (error) {
      console.warn('[StateEvents] getAppState refresh failed:', error);
    } finally {
      isRefreshing = false;
      if (pendingRefresh) {
        pendingRefresh = false;
        await refreshNow();
      }
    }
  };

  const scheduleRefresh = () => {
    if (isRefreshing) {
      pendingRefresh = true;
      return;
    }
    if (refreshTimer !== null) return;
    refreshTimer = window.setTimeout(() => {
      refreshTimer = null;
      void refreshNow();
    }, debounceMs);
  };

  const disconnect = () => {
    clearRefreshTimer();
    pendingRefresh = false;
    if (!eventSource) return;
    console.log('[StateEvents] socket disconnected', { vaultName: connectedVaultName });
    eventSource.close();
    eventSource = null;
  };

  const connect = (vaultName: string) => {
    const nextVaultName = vaultName.trim();
    if (!nextVaultName) {
      disconnect();
      connectedVaultName = '';
      return;
    }
    if (eventSource && connectedVaultName === nextVaultName) return;

    disconnect();
    connectedVaultName = nextVaultName;
    hasOpened = false;
    const source = createEventSource(resolveUrl(nextVaultName));
    eventSource = source;

    source.onopen = () => {
      const wasReconnect = hasOpened;
      hasOpened = true;
      console.log(wasReconnect ? '[StateEvents] socket reconnected' : '[StateEvents] socket connected', {
        vaultName: connectedVaultName,
      });
      scheduleRefresh();
    };

    source.onerror = () => {
      console.warn('[StateEvents] socket disconnected', { vaultName: connectedVaultName });
    };

    source.addEventListener('state_invalidated', (message) => {
      try {
        const event = JSON.parse(message.data) as StateInvalidationEvent;
        if (event.type !== 'state_invalidated' || event.vaultName !== connectedVaultName) return;
        console.log('[StateEvents] invalidation received', {
          vaultName: event.vaultName,
          scope: event.scope,
          revision: event.revision,
        });
        scheduleRefresh();
      } catch (error) {
        console.warn('[StateEvents] invalid invalidation payload:', error);
      }
    });
  };

  return {
    connect,
    disconnect,
    refreshNow,
  };
}
