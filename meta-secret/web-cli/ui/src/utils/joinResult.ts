import { resolveMetaSecretJoinEventsBaseUrl } from '@/config/metaSecretEnvironment';

export type JoinResultEvent = {
  requestId: string;
  outcome: 'accepted' | 'declined';
};

export type JoinResultControllerOptions = {
  refresh: () => Promise<unknown>;
  getAuthorization: () => Promise<string>;
  onError?: (message: string, terminal: boolean) => void;
  fetchImpl?: typeof fetch;
  resolveUrl?: (vaultName: string) => string;
  reconnectDelayMs?: number;
};

export type JoinResultController = {
  connect: (vaultName: string) => void;
  disconnect: () => void;
};

export function resolveJoinEventsUrl(vaultName: string) {
  const url = new URL(resolveMetaSecretJoinEventsBaseUrl());
  url.searchParams.set('vaultName', vaultName);
  return url.toString();
}

export function createJoinResultController({
  refresh,
  getAuthorization,
  onError = (message) => console.warn('[JoinResult] stream error:', message),
  fetchImpl = fetch,
  resolveUrl = resolveJoinEventsUrl,
  reconnectDelayMs = 1_000,
}: JoinResultControllerOptions): JoinResultController {
  let abortController: AbortController | null = null;
  let connectedVault = '';
  let generation = 0;

  const disconnect = () => {
    generation += 1;
    abortController?.abort();
    abortController = null;
    connectedVault = '';
  };

  const run = async (vaultName: string, currentGeneration: number, controller: AbortController) => {
    while (currentGeneration === generation && !controller.signal.aborted) {
      let terminalAuthorizationError = false;
      try {
        let token: string;
        try {
          token = await getAuthorization();
        } catch (error) {
          onError(error instanceof Error ? error.message : String(error), true);
          return;
        }
        const response = await fetchImpl(resolveUrl(vaultName), {
          headers: {
            Accept: 'text/event-stream',
            'Cache-Control': 'no-cache',
            Authorization: `Bearer ${token}`,
          },
          signal: controller.signal,
        });
        if (response.status === 401 || response.status === 403) {
          terminalAuthorizationError = true;
          onError(`join result authorization rejected (HTTP ${response.status})`, true);
        } else if (!response.ok) {
          throw new Error(`join result HTTP ${response.status}`);
        } else if (!response.body) {
          throw new Error('join result response has no body');
        } else {
          const reader = response.body.getReader();
          const decoder = new TextDecoder();
          let buffer = '';
          let eventName = '';
          let dataLines: string[] = [];
          try {
            while (!controller.signal.aborted && currentGeneration === generation) {
              const { done, value } = await reader.read();
              if (done) break;
              buffer += decoder.decode(value, { stream: true });
              const lines = buffer.split(/\r?\n/);
              buffer = lines.pop() ?? '';
              for (const line of lines) {
                if (line === '') {
                  if (eventName === 'join_result' && dataLines.length > 0) {
                    const result = JSON.parse(dataLines.join('\n')) as JoinResultEvent;
                    if (result.requestId && (result.outcome === 'accepted' || result.outcome === 'declined')) {
                      controller.abort();
                      await refresh();
                      return;
                    }
                    onError('invalid join result event', false);
                  }
                  eventName = '';
                  dataLines = [];
                } else if (line.startsWith('event:')) {
                  eventName = line.slice('event:'.length).trim();
                } else if (line.startsWith('data:')) {
                  dataLines.push(line.slice('data:'.length).trimStart());
                }
              }
            }
          } finally {
            reader.releaseLock();
          }
        }
      } catch (error) {
        if (controller.signal.aborted || currentGeneration !== generation) return;
        onError(error instanceof Error ? error.message : String(error), false);
      }
      if (terminalAuthorizationError) return;
      await new Promise<void>((resolve) => {
        const timer = window.setTimeout(resolve, reconnectDelayMs);
        controller.signal.addEventListener(
          'abort',
          () => {
            window.clearTimeout(timer);
            resolve();
          },
          { once: true },
        );
      });
    }
  };

  const connect = (vaultName: string) => {
    const nextVault = vaultName.trim();
    if (!nextVault) return disconnect();
    if (abortController && connectedVault === nextVault) return;
    disconnect();
    connectedVault = nextVault;
    abortController = new AbortController();
    void run(nextVault, generation, abortController);
  };

  return { connect, disconnect };
}
