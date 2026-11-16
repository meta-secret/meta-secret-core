import { describe, expect, it, vi } from 'vitest';
import { createDeviceController } from '@/features/liveboard/deviceController';
import type { DevicePort } from '@/features/liveboard/types';
import { deferred, devicePort, deviceSnapshot, session } from './liveboardFixtures';

function setup(patch: Partial<DevicePort> = {}) {
  const port = devicePort(patch),
    controller = createDeviceController(port);
  controller.reconcile(deviceSnapshot());
  return { controller, port };
}

describe('Liveboard Vault presentation never mutates membership', () => {
  it('uses exact canonical member data and supplied storage wording, without inferring policy', () => {
    const { controller } = setup();
    const snapshot = deviceSnapshot({ storageTitle: 'Opaque Core storage mode' });
    controller.reconcile(snapshot);
    expect(controller.state.value.devices).toEqual(snapshot.devices);
    expect(controller.state.value.storageTitle).toBe('Opaque Core storage mode');
  });
  it('preview uses a supplied forecast, preserving actual devices and sending no operation', () => {
    const { controller, port } = setup();
    controller.preview('peer-a');
    expect(controller.state.value.preview).toEqual(deviceSnapshot().forecasts[0]);
    expect(controller.state.value.devices).toHaveLength(3);
    expect(port.authenticate).not.toHaveBeenCalled();
    expect(port.remove).not.toHaveBeenCalled();
  });
  it('Current cancels only the forecast and never sends a request', () => {
    const { controller, port } = setup();
    controller.preview('peer-a');
    expect(controller.state.value.preview?.targetId).toBe('peer-a');
    controller.current();
    expect(controller.state.value.preview).toBeNull();
    expect(controller.state.value.devices).toHaveLength(3);
    expect(port.remove).not.toHaveBeenCalled();
  });
  it('cannot select/remove the current device or an unknown slot without a Core forecast', async () => {
    const { controller, port } = setup();
    expect(controller.state.value.devices.find((device) => device.current)?.deviceId).toBe('self');
    controller.preview('self');
    await controller.remove();
    controller.preview('outsider');
    await controller.remove();
    expect(controller.state.value.preview).toBeNull();
    expect(port.remove).not.toHaveBeenCalled();
    expect(port.authenticate).not.toHaveBeenCalled();
  });
  it('an add-device preview does not become a removal intent', async () => {
    const { controller, port } = setup();
    controller.reconcile(
      deviceSnapshot({
        forecasts: [
          { targetId: 'empty-03', mode: 'connect', storageTitle: 'Future', storageDetail: 'Presentation only' },
        ],
      }),
    );
    controller.preview('empty-03');
    expect(controller.state.value.preview?.mode).toBe('connect');
    await controller.remove();
    expect(port.authenticate).not.toHaveBeenCalled();
    expect(port.remove).not.toHaveBeenCalled();
  });
});

describe('Liveboard exact device removal lifecycle', () => {
  it('authenticates and submits only the selected target, preserving members until actual canonical removal', async () => {
    const { controller, port } = setup();
    controller.preview('peer-a');
    await controller.remove();
    expect(port.authenticate).toHaveBeenCalledExactlyOnceWith('peer-a');
    expect(port.remove).toHaveBeenCalledExactlyOnceWith('peer-a');
    expect(controller.state.value.devices).toHaveLength(3);
    await controller.remove();
    expect(port.remove).toHaveBeenCalledOnce();
  });
  it('authentication cancellation leaves the preview and sends no removal', async () => {
    const { controller, port } = setup({ authenticate: vi.fn(async () => false) });
    controller.preview('peer-a');
    await controller.remove();
    expect(port.authenticate).toHaveBeenCalledOnce();
    expect(port.remove).not.toHaveBeenCalled();
    expect(controller.state.value.preview?.targetId).toBe('peer-a');
  });
  it('freezes the target while authentication is outstanding and serializes duplicate Remove', async () => {
    const auth = deferred<boolean>(),
      { controller, port } = setup({ authenticate: vi.fn(() => auth.promise) });
    controller.preview('peer-a');
    const pending = controller.remove();
    expect(port.authenticate).toHaveBeenCalledExactlyOnceWith('peer-a');
    controller.preview('peer-b');
    controller.current();
    await controller.remove();
    expect(port.authenticate).toHaveBeenCalledOnce();
    expect(controller.state.value.preview?.targetId).toBe('peer-a');
    auth.resolve(true);
    await pending;
    expect(port.remove).toHaveBeenCalledExactlyOnceWith('peer-a');
  });
  it.each(['target', 'ready', 'session', 'dispose'] as const)(
    'does not submit a stale removal after %s changes during auth',
    async (lifecycle) => {
      const auth = deferred<boolean>(),
        { controller, port } = setup({ authenticate: vi.fn(() => auth.promise) });
      controller.preview('peer-a');
      const pending = controller.remove();
      expect(port.authenticate).toHaveBeenCalledOnce();
      if (lifecycle === 'dispose') controller.dispose();
      else
        controller.reconcile(
          deviceSnapshot({
            session: session({
              ready: lifecycle !== 'ready',
              identity: lifecycle === 'session' ? 'session-b' : 'session-a',
            }),
            devices:
              lifecycle === 'target'
                ? deviceSnapshot().devices.filter((device) => device.deviceId !== 'peer-a')
                : deviceSnapshot().devices,
          }),
        );
      auth.resolve(true);
      await pending;
      expect(port.remove).not.toHaveBeenCalled();
    },
  );
  it('renders only the actual request peers and responder and cannot cancel a pending forecast', () => {
    const { controller } = setup();
    controller.preview('peer-a');
    const operation = { requestId: 'exact-remove', targetId: 'peer-a', phase: 'waiting' as const, peers: ['peer-b'] };
    controller.reconcile(deviceSnapshot({ operation }));
    expect(controller.state.value.operation).toEqual(operation);
    controller.current();
    expect(controller.state.value.preview?.targetId).toBe('peer-a');
    controller.reconcile(deviceSnapshot({ operation: { ...operation, phase: 'declined', responderId: 'peer-b' } }));
    expect(controller.state.value.operation).toMatchObject({
      requestId: 'exact-remove',
      peers: ['peer-b'],
      responderId: 'peer-b',
      phase: 'declined',
    });
    expect(controller.state.value.devices).toHaveLength(3);
  });
  it('settles immediately to two actual devices, removing preview/actions/packets on canonical readiness', () => {
    const { controller } = setup();
    controller.preview('peer-a');
    controller.reconcile(
      deviceSnapshot({
        operation: { requestId: 'remove-a', targetId: 'peer-a', phase: 'settling', peers: ['peer-b'] },
      }),
    );
    expect(controller.state.value.operation?.phase).toBe('settling');
    controller.reconcile(
      deviceSnapshot({
        devices: deviceSnapshot().devices.filter((device) => device.deviceId !== 'peer-a'),
        storageTitle: 'Full encrypted copies',
        operation: { requestId: 'remove-a', targetId: 'peer-a', phase: 'settled', peers: [] },
      }),
    );
    expect(controller.state.value).toMatchObject({
      preview: null,
      operation: null,
      busy: false,
      storageTitle: 'Full encrypted copies',
    });
    expect(controller.state.value.devices.map((device) => device.deviceId)).toEqual(['self', 'peer-b']);
  });
});
