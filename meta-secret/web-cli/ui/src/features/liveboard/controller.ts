import { computed, readonly, ref, shallowRef, watch } from 'vue';
import type { CoreAdapter, CoreProjection } from './coreTypes';
import type { NotificationReceiptPresentation, ProfilePresentation } from './presentation';
import { blockedProjection } from './coreProjection';
import { createSecretController } from './secretController';
import { createDeviceController } from './deviceController';
import { createNotificationController } from './notificationController';
import { createControllerCreate } from './controllerCreate';
import { createControllerOperations } from './controllerOperations';
import {
  deviceForecasts,
  deviceOperation,
  provenResponse,
  requestVisual,
  storageTitle,
  vaultPresentation,
} from './controllerPresentation';
import { sameSession } from './lifecycle';
export type LiveboardRoute = 'home' | 'secrets' | 'profile';
export function createLiveboardController(input: {
  adapter: CoreAdapter;
  resetLocal(): Promise<void>;
  version: string;
  onError?(): void;
}) {
  const { adapter } = input;
  const snapshot = shallowRef<CoreProjection>(blockedProjection());
  const ui = ref({
    route: 'home' as LiveboardRoute,
    inviteOpen: false,
    resetOpen: false,
    resetBusy: false,
  });
  let disposed = false;
  let resetting = false;
  let revision = 0;
  let pageActive = true;
  let reportingError = false;
  let createBoundary = 0;
  const inviteAnchor = shallowRef<HTMLElement | null>(null);
  const notificationReceipt = shallowRef<NotificationReceiptPresentation | null>(null);
  const operations = createControllerOperations(adapter);
  const notifications = createNotificationController(adapter.notifications);
  const devices = createDeviceController({
    ...adapter.devices,
    async remove(targetId) {
      const token = revision;
      const previousTerminals = operations.captureTerminals();
      const receipt = await adapter.devices.remove(targetId);
      if (token === revision && !disposed && !resetting)
        await operations.producer({ kind: 'removeDevice', subject: targetId, receipt, previousTerminals });
      return receipt;
    },
    reportError,
  });
  const secretController = createSecretController({
    ...adapter.secrets,
    async recover(passId) {
      const token = revision;
      const previousTerminals = operations.captureTerminals();
      const receipt = await adapter.secrets.recover(passId);
      if (token === revision && !disposed && !resetting) operations.recovered({ passId, receipt, previousTerminals });
      return receipt;
    },
    async deleteSecret(passId) {
      const token = revision;
      const previousTerminals = operations.captureTerminals();
      const receipt = await adapter.secrets.deleteSecret(passId);
      if (token === revision && !disposed && !resetting)
        await operations.producer({ kind: 'deleteSecret', subject: passId, receipt, previousTerminals });
      return receipt;
    },
    reportError,
  });
  const creation = createControllerCreate({
    port: { ...adapter.create, reportError },
    allowed: () => {
      if (createBoundary) return false;
      return !disposed && !resetting && pageActive && snapshot.value.canCreateSecret && snapshot.value.session.ready;
    },
  });
  const create = {
    ...creation,
    open() {
      secretController.invalidate();
      creation.open();
    },
  };
  const secrets = {
    ...secretController,
    deliverRecoveredReveal(input: Parameters<typeof secretController.deliverRecoveredReveal>[0]) {
      if (!secretController.deliverRecoveredReveal(input)) return false;
      operations.consume(input.owner.claimId);
      return true;
    },
    async primary(passId: string) {
      const before = secretController.state.value.cards[passId];
      const claimId = before?.secret.primary.kind === 'show-recovered' ? before.secret.primary.claimId : null;
      const token = revision;
      await secretController.primary(passId);
      const after = secretController.state.value.cards[passId];
      if (
        token === revision &&
        claimId &&
        after?.phase === 'revealed' &&
        after.value &&
        after.secret.primary.kind === 'show-recovered' &&
        after.secret.primary.claimId === claimId
      )
        operations.consume(claimId);
    },
  };
  const profile = computed<ProfilePresentation>(() => ({
    vaultName: snapshot.value.vaultName,
    deviceCount: snapshot.value.devices.length,
    secretCount: snapshot.value.secrets.length,
    version: input.version,
    alpha: true,
    resetDescription: 'Clear local data on this device. Your Vault and other devices remain available.',
  }));
  const state = computed(() => ({
    ...ui.value,
    ready: snapshot.value.session.ready && !disposed && !resetting,
    canCreateSecret: snapshot.value.canCreateSecret && snapshot.value.session.ready && !resetting,
    profile: profile.value,
    unread: operations.unread.value,
    inviteAnchor: inviteAnchor.value,
    notificationReceipt: notificationReceipt.value,
    vault: vaultPresentation({
      snapshot: snapshot.value,
      devices: devices.state.value,
      request: requestVisual(snapshot.value, operations.visibleOperation(snapshot.value)),
    }),
  }));
  function present() {
    const presentation = secretController.state.value.presentation;
    const query = presentation.query.trim().toLocaleLowerCase();
    secretController.present({
      ...presentation,
      active: ui.value.route === 'secrets' && pageActive && !disposed && !resetting,
      visibleIds: snapshot.value.secrets
        .filter((secret) => secret.name.toLocaleLowerCase().includes(query))
        .map((secret) => secret.passId),
    });
  }
  function conceal(preserveRecovered = false, reason: Parameters<typeof create.conceal>[0] = 'hard') {
    if (preserveRecovered) secretController.suspendForSync();
    else secretController.invalidate();
    create.conceal(reason);
    ui.value.inviteOpen = false;
    inviteAnchor.value = null;
  }
  function invalidateError() {
    fenceCreate(() => {
      create.conceal('error');
      input.onError?.();
      beginSync();
    });
  }
  function reportError(message: string) {
    if (disposed || resetting) return;
    const previous = reportingError;
    reportingError = true;
    try {
      invalidateError();
      notifications.showError(message);
    } finally {
      reportingError = previous;
    }
  }
  function reconcileOperations(next: CoreProjection, preserveRecovered: boolean) {
    const correlation = operations.removal();
    const localPending = devices.hasPending();
    const settledMutations = operations.reconcile(next);
    for (const mutation of settledMutations) secretController.settleMutation({ ...mutation, preserveRecovered });
    devices.reconcile({
      session: next.session,
      devices: next.devices,
      forecasts: deviceForecasts(next),
      storageTitle: storageTitle(next),
      operation: deviceOperation(next, correlation, localPending),
    });
  }
  function currentDevice() {
    if (disposed || resetting || !snapshot.value.session.ready) return;
    const token = revision;
    const preview = devices.state.value.preview;
    const operation = operations.visibleOperation(snapshot.value);
    devices.current();
    if (
      token === revision &&
      preview?.mode === 'remove' &&
      !devices.state.value.preview &&
      operation?.kind === 'removeDevice' &&
      operation.outcome === 'declined' &&
      operation.targetDeviceId === preview.targetId
    )
      operations.acknowledgeRemoval({
        snapshot: snapshot.value,
        requestId: operation.requestId,
        targetId: preview.targetId,
      });
  }
  function fenceCreate(effect: () => void) {
    createBoundary++;
    try {
      effect();
    } finally {
      createBoundary--;
    }
  }
  function reconcile(next: CoreProjection, preserveRecovered = false) {
    fenceCreate(() => {
      if (disposed || resetting) return;
      const changed = !sameSession(snapshot.value.session, next.session);
      const previousVault = snapshot.value.vaultName;
      const decision = notifications.state.value.pendingDecision;
      const selected = notifications.state.value.requests.find(
        (request) => decision?.kind === request.kind && decision.requestId === request.requestId,
      );
      // Every published material snapshot fences old held cards and late Show/Copy.
      if (preserveRecovered && !changed) secretController.suspendForSync();
      else secretController.invalidate();
      if (changed) revision++;
      if (changed || !next.session.ready || previousVault !== next.vaultName) {
        create.conceal(changed || previousVault !== next.vaultName ? 'hard' : 'sync');
        ui.value.inviteOpen = false;
        inviteAnchor.value = null;
      }
      if (changed) notificationReceipt.value = null;
      if (!changed && next.session.ready && decision && selected) {
        const kind =
          decision.kind === 'delete-secret'
            ? 'deleteSecret'
            : decision.kind === 'remove-device'
              ? 'removeDevice'
              : decision.kind;
        const result = next.operations.find(
          (operation) => operation.kind === kind && operation.requestId === decision.requestId,
        );
        if (result && provenResponse(result))
          notificationReceipt.value = {
            identity: JSON.stringify([decision.kind, decision.requestId]),
            outcome: result.outcome === 'accepted' ? 'approved' : 'declined',
            requesterName: selected.requesterName,
            subjectName: selected.subjectName,
          };
      }
      snapshot.value = next;
      notifications.reconcile({ session: next.session, requests: next.requests });
      if (next.requests.length) notificationReceipt.value = null;
      create.reconcile(next.session);
      secretController.reconcile({ session: next.session, secrets: next.secrets }, { preserveRecovered });
      reconcileOperations(next, preserveRecovered);
      if (!next.vaultName || !next.session.ready) ui.value.inviteOpen = false;
      present();
    });
  }
  async function refresh({ preserveRecovered = false } = {}) {
    if (disposed || resetting) return;
    reconcile(adapter.project(), preserveRecovered);
    if (!snapshot.value.session.ready) return;
    const token = revision;
    await operations.observe();
    if (token !== revision || disposed || resetting) return;
    // A late canonical UUID is used only after observation of the original R/U.
    reconcileOperations(snapshot.value, preserveRecovered);
    await create.refreshReceipt();
  }
  function beginSync({ preserveRecovered = false } = {}, reason: 'sync' | 'hard' = 'sync') {
    if (disposed || resetting) return;
    fenceCreate(() => {
      conceal(preserveRecovered, reason);
      reconcile(
        { ...snapshot.value, canCreateSecret: false, session: { ...snapshot.value.session, ready: false } },
        preserveRecovered,
      );
    });
  }
  function navigate(route: LiveboardRoute) {
    if (disposed || resetting || ui.value.route === route) return;
    conceal(false, 'route');
    ui.value.route = route;
    secretController.reconcile({ session: snapshot.value.session, secrets: snapshot.value.secrets });
    present();
  }
  function search(query: string) {
    if (disposed || resetting) return;
    secretController.present({ ...secretController.state.value.presentation, query });
    present();
  }
  function scroll(scrollTop: number) {
    if (!disposed && !resetting) secretController.present({ ...secretController.state.value.presentation, scrollTop });
  }
  function pagehide() {
    pageActive = false;
    beginSync({}, 'hard');
    present();
  }
  function pageshow() {
    pageActive = true;
  }
  function visibilitychange() {
    if (document.visibilityState === 'hidden') pagehide();
    else pageshow();
  }
  function detach() {
    if (typeof window !== 'undefined') {
      window.removeEventListener('pagehide', pagehide);
      window.removeEventListener('pageshow', pageshow);
    }
    if (typeof document !== 'undefined') document.removeEventListener('visibilitychange', visibilitychange);
  }
  const stopError = watch(
    () => notifications.state.value.error,
    (error) => {
      if (error && !reportingError) invalidateError();
    },
    { flush: 'sync' },
  );
  if (typeof window !== 'undefined') {
    window.addEventListener('pagehide', pagehide);
    window.addEventListener('pageshow', pageshow);
  }
  if (typeof document !== 'undefined') document.addEventListener('visibilitychange', visibilitychange);
  async function confirmReset() {
    if (disposed || resetting || !ui.value.resetOpen) return;
    resetting = true;
    revision++;
    ui.value.resetBusy = true;
    conceal();
    create.dispose();
    secretController.dispose();
    devices.dispose();
    notifications.reconcile({ session: blockedProjection().session, requests: [] });
    operations.clear();
    notificationReceipt.value = null;
    snapshot.value = blockedProjection();
    detach();
    try {
      await input.resetLocal();
    } catch {
      notifications.showError('Could not reset local data.');
    } finally {
      ui.value.resetBusy = false;
      ui.value.resetOpen = false;
    }
  }
  return {
    state: readonly(state),
    secrets,
    devices: { ...devices, current: currentDevice },
    notifications,
    create,
    refresh,
    beginSync,
    navigate,
    search,
    scroll,
    showError: reportError,
    dismissError: notifications.dismissError,
    invite(anchor: HTMLElement) {
      if (!disposed && !resetting && snapshot.value.session.ready && snapshot.value.vaultName) {
        secretController.invalidate();
        inviteAnchor.value = anchor;
        ui.value.inviteOpen = true;
      }
    },
    closeInvite() {
      ui.value.inviteOpen = false;
      inviteAnchor.value = null;
    },
    requestReset() {
      if (!disposed && !resetting) {
        secretController.invalidate();
        ui.value.resetOpen = true;
      }
    },
    cancelReset() {
      if (!resetting) ui.value.resetOpen = false;
    },
    confirmReset,
    dispose() {
      if (disposed) return;
      disposed = true;
      revision++;
      conceal();
      create.dispose();
      secretController.dispose();
      devices.dispose();
      notifications.dispose();
      operations.clear();
      notificationReceipt.value = null;
      snapshot.value = blockedProjection();
      ui.value.resetOpen = false;
      detach();
      stopError();
    },
  };
}
