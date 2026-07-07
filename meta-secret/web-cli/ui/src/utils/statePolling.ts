import { onBeforeUnmount } from 'vue';

export const STATE_POLL_INTERVAL_MS = 5000;

export type StateRefresh = () => Promise<unknown>;

export function createStatePoller(refresh: StateRefresh, intervalMs = STATE_POLL_INTERVAL_MS) {
  let timeoutId: ReturnType<typeof window.setTimeout> | null = null;
  let isStarted = false;
  let isRefreshing = false;

  const clearScheduledRefresh = () => {
    if (timeoutId === null) return;
    clearTimeout(timeoutId);
    timeoutId = null;
  };

  const scheduleNextRefresh = () => {
    if (!isStarted || timeoutId !== null) return;
    timeoutId = window.setTimeout(() => {
      timeoutId = null;
      void refreshNow();
    }, intervalMs);
  };

  const refreshNow = async () => {
    if (isRefreshing) return;
    clearScheduledRefresh();
    isRefreshing = true;
    try {
      await refresh();
    } catch (error) {
      console.warn('State polling refresh failed:', error);
    } finally {
      isRefreshing = false;
      scheduleNextRefresh();
    }
  };

  const start = () => {
    if (isStarted) return;
    isStarted = true;
    void refreshNow();
  };

  const stop = () => {
    isStarted = false;
    clearScheduledRefresh();
  };

  return {
    refreshNow,
    start,
    stop,
  };
}

export function useStatePolling(refresh: StateRefresh, intervalMs = STATE_POLL_INTERVAL_MS) {
  const poller = createStatePoller(refresh, intervalMs);
  onBeforeUnmount(() => poller.stop());
  return poller;
}
