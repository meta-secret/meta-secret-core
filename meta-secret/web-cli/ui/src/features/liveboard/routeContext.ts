import { computed, inject, provide, type InjectionKey } from 'vue';
import type { useVaultSession } from './useVaultSession';

type Session = ReturnType<typeof useVaultSession>;
type Controller = Session['controller'];
type RouteContext = ReturnType<typeof createRouteContext>;
const routeContextKey: InjectionKey<RouteContext> = Symbol('liveboard-route-session');
const syncMessage = 'Could not synchronize Vault. Retry to apply pending changes.';

function createRouteContext(session: Session) {
  // Reset replaces the controller. Every view and action reads the current getter.
  const controller = computed(() => session.controller);
  const notices = computed(() => {
    const view = controller.value.notifications.state.value;
    const error = view.error ?? session.state.value.error;
    return error ? { ...view, error } : view;
  });
  async function runWrite(action: (owner: Controller) => Promise<void>) {
    if (!session.state.value.ready) return;
    const owner = controller.value;
    const identity = session.state.value.identity;
    const current = () => session.controller === owner && session.state.value.identity === identity;
    try {
      await action(owner);
      if (current()) await session.refresh({ requiredSync: true });
    } catch {
      if (current()) owner.showError(syncMessage);
    }
  }
  return { session, controller, notices, runWrite };
}

export function provideLiveboardRouteSession(session: Session) {
  const context = createRouteContext(session);
  provide(routeContextKey, context);
  return context;
}

export function useLiveboardRouteSession() {
  const context = inject(routeContextKey);
  if (!context) throw new Error('Liveboard parent session unavailable.');
  return context;
}
