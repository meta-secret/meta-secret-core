import { readonly, ref } from 'vue';
import { cancelled, createLifetime, sameSession, usable } from './lifecycle';
import type { DevicePort, DeviceSnapshot, DeviceView } from './types';

/** Forecasts are supplied by Core; preview never changes actual membership. */
export function createDeviceController(port: DevicePort) {
  const state = ref<DeviceView>({ devices: [], preview: null, operation: null, storageTitle: '', busy: false });
  const lifetime = createLifetime();
  let snapshot: DeviceSnapshot | null = null;
  let pending: string | null = null;
  let owner: object | null = null;
  const locked = () =>
    pending !== null || (state.value.operation !== null && state.value.operation.phase !== 'declined');
  function reconcile(next: DeviceSnapshot) {
    if (lifetime.disposed) return;
    if (!sameSession(snapshot?.session ?? null, next.session)) {
      lifetime.invalidate();
      pending = null;
      owner = null;
      state.value.preview = null;
    } else if (!usable(next.session)) lifetime.invalidate();
    snapshot = next;
    state.value.devices = next.devices.map((device) => ({ ...device }));
    state.value.storageTitle = next.storageTitle;
    state.value.operation =
      next.operation?.phase === 'settled'
        ? null
        : next.operation
          ? { ...next.operation, peers: [...next.operation.peers] }
          : null;
    const resolved =
      usable(next.session) &&
      next.operation &&
      (!pending || next.operation.targetId === pending) &&
      ['settled', 'declined'].includes(next.operation.phase);
    if (resolved && next.operation?.phase === 'settled') {
      state.value.preview = null;
      pending = null;
    }
    if (resolved && next.operation?.phase === 'declined') pending = null;
    if (
      !pending &&
      state.value.preview &&
      !next.forecasts.some((forecast) => forecast.targetId === state.value.preview?.targetId)
    )
      state.value.preview = null;
    state.value.busy = pending !== null || Boolean(state.value.operation && state.value.operation.phase !== 'declined');
  }
  function preview(targetId: string) {
    if (!snapshot || !usable(snapshot.session) || locked()) return;
    const forecast = snapshot.forecasts.find((item) => item.targetId === targetId);
    const device = snapshot.devices.find((item) => item.deviceId === targetId);
    if (!forecast || device?.current || (forecast.mode === 'remove' && (!device || !device.canRemove))) return;
    state.value.preview = { ...forecast };
  }
  async function remove() {
    const forecast = state.value.preview;
    if (!snapshot || !usable(snapshot.session) || locked() || !forecast || forecast.mode !== 'remove') return;
    const target = snapshot.devices.find((device) => device.deviceId === forecast.targetId);
    if (!target?.canRemove || target.current) return;
    const targetId = target.deviceId,
      token = lifetime.capture();
    const operationOwner = {};
    owner = operationOwner;
    pending = targetId;
    state.value.busy = true;
    let submitted = false;
    const valid = () =>
      lifetime.current(token) &&
      usable(snapshot?.session ?? null) &&
      Boolean(snapshot?.devices.some((device) => device.deviceId === targetId && device.canRemove && !device.current));
    try {
      if (!(await port.authenticate(targetId)) || !valid()) return;
      submitted = true;
      const receipt = await port.remove(targetId);
      if (valid() && receipt.phase === 'rejected') {
        submitted = false;
        port.reportError('Device removal was declined.');
      }
    } catch (error) {
      if (valid() && !cancelled(error)) port.reportError('Could not confirm device removal.');
    } finally {
      if (owner === operationOwner && !submitted) {
        pending = null;
        owner = null;
        state.value.busy = Boolean(state.value.operation && state.value.operation.phase !== 'declined');
      } else if (valid()) state.value.busy = pending !== null;
    }
  }
  return {
    state: readonly(state),
    hasPending: () => pending !== null,
    reconcile,
    preview,
    remove,
    current: () => {
      if (!locked()) state.value.preview = null;
    },
    dispose: () => {
      lifetime.dispose();
      snapshot = null;
      pending = null;
      state.value = { devices: [], preview: null, operation: null, storageTitle: '', busy: false };
    },
  };
}
