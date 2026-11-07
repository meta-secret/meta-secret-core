import { describe, expect, it, vi } from 'vitest';
import { createSecretController } from '@/features/liveboard/secretController';
import { deferred, secret, secretPort, session } from './liveboardFixtures';

describe('Liveboard pre-submit deletion lifecycle', () => {
  it.each(['leave', 'filter', 'hide'] as const)(
    'unlocks an unsubmitted deletion after %s invalidates authentication',
    async (lifecycle) => {
      const auth = deferred<boolean>();
      const port = secretPort({ authenticate: vi.fn(() => auth.promise) });
      const controller = createSecretController(port);
      controller.reconcile({ session: session(), secrets: [secret()] });
      controller.present({ active: true, visibleIds: ['pass-a'], query: '', scrollTop: 0 });
      const pending = controller.remove('pass-a');
      expect(port.authenticate).toHaveBeenCalledOnce();
      if (lifecycle === 'hide') controller.hide('pass-a');
      else
        controller.present({
          active: lifecycle !== 'leave',
          visibleIds: [],
          query: lifecycle === 'filter' ? 'other' : '',
          scrollTop: 0,
        });
      auth.resolve(true);
      await pending;
      expect(port.deleteSecret).not.toHaveBeenCalled();
      controller.present({ active: true, visibleIds: ['pass-a'], query: '', scrollTop: 0 });
      expect(controller.state.value.cards['pass-a']?.phase).toBe('masked');
      await controller.primary('pass-a');
      expect(port.showLocal).toHaveBeenCalledExactlyOnceWith('pass-a');
    },
  );
});

import { createDeviceController } from '@/features/liveboard/deviceController';
import { createNotificationController } from '@/features/liveboard/notificationController';
import { devicePort, deviceSnapshot, notificationPort, request } from './liveboardFixtures';

describe('Liveboard synchronization does not fabricate retry readiness', () => {
  it('retains an unknown submitted decision across a failed refresh', async () => {
    const port = notificationPort();
    const controller = createNotificationController(port);
    controller.reconcile({ session: session(), requests: [request()] });
    controller.open();
    await controller.decide(true);
    expect(port.decide).toHaveBeenCalledOnce();
    controller.reconcile({ session: session({ ready: false }), requests: [request()] });
    controller.reconcile({ session: session(), requests: [request()] });
    controller.open();
    await controller.decide(false);
    expect(port.decide).toHaveBeenCalledOnce();
    expect(controller.state.value.busy).toBe(true);
  });
  it('releases only an unsubmitted device intent after synchronization interrupts auth', async () => {
    const auth = deferred<boolean>();
    const port = devicePort({ authenticate: vi.fn(() => auth.promise) });
    const controller = createDeviceController(port);
    controller.reconcile(deviceSnapshot());
    controller.preview('peer-a');
    const pending = controller.remove();
    expect(port.authenticate).toHaveBeenCalledOnce();
    controller.reconcile(deviceSnapshot({ session: session({ ready: false }) }));
    auth.resolve(true);
    await pending;
    expect(port.remove).not.toHaveBeenCalled();
    controller.reconcile(deviceSnapshot());
    await controller.remove();
    expect(port.authenticate).toHaveBeenCalledTimes(2);
    expect(port.remove).toHaveBeenCalledExactlyOnceWith('peer-a');
  });
});
