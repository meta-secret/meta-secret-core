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
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      const messages = buffer.split(/\r?\n\r?\n/);
      buffer = messages.pop() ?? '';
      for (const message of messages) {
        const parsed = parseSseMessage(message);
        if (parsed) onEvent(parsed);
      }
    }
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
    signal.addEventListener('abort', () => {
      window.clearTimeout(timer);
      resolve();
    }, { once: true });
  });
}

export function createStateInvalidationController({
  refresh,
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
  let streamGeneration = 0;

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
    streamGeneration += 1;
    if (!streamAbortController) return;
    console.log('[StateEvents] socket disconnected', { vaultName: connectedVaultName });
    streamAbortController.abort();
    streamAbortController = null;
  };

  const runStream = async (vaultName: string, generation: number, abortController: AbortController) => {
    let hasConnected = false;
    while (generation === streamGeneration && !abortController.signal.aborted) {
      try {
        console.log('[StateEvents] requesting authorization', { vaultName });
        const authorization = await getAuthorization();
        const response = await fetchImpl(resolveUrl(vaultName), {
          headers: {
            Accept: 'text/event-stream',
            'Cache-Control': 'no-cache',
            Authorization: `Bearer ${authorization}`,
          },
          signal: abortController.signal,
        });
        if (!response.ok) {
          console.warn('[StateEvents] authorization rejected', { vaultName, status: response.status });
          throw new Error(`state events HTTP ${response.status}`);
        }
        if (!response.body) throw new Error('state events response has no body');

        console.log(hasConnected ? '[StateEvents] socket reconnected' : '[StateEvents] socket connected', {
          vaultName,
        });
        hasConnected = true;
        scheduleRefresh();
        await consumeSseStream(response.body, (message) => {
          if (message.event !== 'state_invalidated') return;
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
        }, abortController.signal);
      } catch (error) {
        if (abortController.signal.aborted || generation !== streamGeneration) return;
        console.warn('[StateEvents] socket disconnected', { vaultName, error });
      }
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

  return { connect, disconnect, refreshNow };
}
