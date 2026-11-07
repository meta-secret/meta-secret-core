import { readonly, ref } from 'vue';
import { cancelled, createLifetime, sameSession, usable } from './lifecycle';
import type {
  IncomingRequest,
  LiveboardSession,
  NotificationPort,
  NotificationSnapshot,
  NotificationView,
  RequestKey,
} from './types';

const key = (item: RequestKey) => JSON.stringify([item.kind, item.requestId]);

/** FIFO presentation only; every decision remains an exact authenticated Core intent. */
export function createNotificationController(port: NotificationPort) {
  const state = ref<NotificationView>({ requests: [], selected: null, expanded: false, busy: false, error: null });
  const lifetime = createLifetime();
  let session: LiveboardSession | null = null;
  let pending: string | null = null;
  let submission: 'auth' | 'submitted' | null = null;
  const selected = () =>
    state.value.requests.find((item) => state.value.selected && key(item) === key(state.value.selected));
  function reconcile(snapshot: NotificationSnapshot) {
    if (lifetime.disposed) return;
    const changed = !sameSession(session, snapshot.session);
    if (changed || !usable(snapshot.session)) {
      lifetime.invalidate();
      if (changed || submission === 'auth') {
        pending = null;
        submission = null;
      }
      state.value.selected = null;
      state.value.expanded = false;
      if (changed) state.value.error = null;
    }
    session = { ...snapshot.session };
    const incoming = new Map<string, IncomingRequest>();
    for (const item of snapshot.requests) if (!incoming.has(key(item))) incoming.set(key(item), { ...item });
    const retained = state.value.requests
      .filter((item) => incoming.has(key(item)))
      .map((item) => incoming.get(key(item))!);
    const seen = new Set(retained.map(key));
    state.value.requests = [...retained, ...[...incoming.values()].filter((item) => !seen.has(key(item)))];
    if (state.value.selected && !selected()) {
      lifetime.invalidate();
      state.value.selected = null;
      state.value.expanded = false;
    }
    if (pending && !incoming.has(pending)) {
      pending = null;
      submission = null;
    }
    state.value.busy = pending !== null;
    if (!pending) state.value.pendingDecision = null;
  }
  function open() {
    if (state.value.error) {
      state.value.error = null;
      return;
    }
    if (!usable(session) || pending || !state.value.requests[0]) return;
    const { kind, requestId } = state.value.requests[0];
    state.value.selected = { kind, requestId };
    state.value.expanded = true;
  }
  async function decide(accepted: boolean) {
    const item = selected();
    if (!usable(session) || pending || !item || (accepted ? !item.canApprove : !item.canDecline)) return;
    const intent = { kind: item.kind, requestId: item.requestId, accepted };
    const itemKey = key(item),
      token = lifetime.capture();
    pending = itemKey;
    submission = 'auth';
    state.value.pendingDecision = intent;
    state.value.busy = true;
    let submitted = false;
    try {
      if (!(await port.authenticate(intent))) return;
      const current = selected();
      if (
        !lifetime.current(token) ||
        !usable(session) ||
        !current ||
        key(current) !== itemKey ||
        (accepted ? !current.canApprove : !current.canDecline)
      )
        return;
      submitted = true;
      submission = 'submitted';
      const receipt = await port.decide(intent);
      if (!lifetime.current(token)) return;
      if (receipt.phase === 'rejected') {
        submitted = false;
        state.value.error = 'Request could not be completed.';
      }
    } catch (error) {
      if (lifetime.current(token) && !cancelled(error)) state.value.error = 'Request could not be completed.';
      // After submission an exception is an unknown outcome; reconciliation owns retry readiness.
    } finally {
      if (lifetime.current(token) && !submitted) {
        pending = null;
        submission = null;
      }
      if (lifetime.current(token)) state.value.busy = pending !== null;
      if (!pending) state.value.pendingDecision = null;
    }
  }
  return {
    state: readonly(state),
    reconcile,
    open,
    decide,
    showError: (message: string) => {
      if (!lifetime.disposed) state.value.error = message;
    },
    dismissError: () => {
      state.value.error = null;
    },
    dispose: () => {
      lifetime.dispose();
      pending = null;
      session = null;
      state.value = { requests: [], selected: null, expanded: false, busy: false, error: null };
    },
  };
}
