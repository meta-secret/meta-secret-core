import { afterEach, describe, expect, it, vi } from 'vitest';
import { createNotificationController } from '@/features/liveboard/notificationController';
import type { NotificationPort, OperationReceipt, RequestKind } from '@/features/liveboard/types';
import { deferred, notificationPort, request, session } from './liveboardFixtures';

const kinds: RequestKind[] = ['join', 'recover', 'delete-secret', 'remove-device'];
function setup(patch: Partial<NotificationPort> = {}) {
  const port = notificationPort(patch),
    controller = createNotificationController(port);
  controller.reconcile({ session: session(), requests: [request()] });
  return { controller, port };
}
afterEach(() => vi.useRealTimers());

describe('Liveboard 04 exact canonical queue', () => {
  it('keeps four kinds and same-name peers distinct, deduplicating only exact kind plus ID', () => {
    const { controller } = setup();
    controller.reconcile({
      session: session(),
      requests: [
        ...kinds.map((kind) => request(kind, 'shared-id')),
        request('join', 'other-id'),
        request('join', 'shared-id'),
      ],
    });
    expect(controller.state.value.requests).toHaveLength(5);
    expect(new Set(controller.state.value.requests.map((item) => `${item.kind}:${item.requestId}`)).size).toBe(5);
  });
  it('expands the exact current item while retaining its requester and subject', () => {
    const { controller } = setup();
    controller.reconcile({
      session: session(),
      requests: [
        request('recover', 'claim-a', {
          subjectId: 'pass-a',
          subjectName: '<script>not markup</script>',
        }),
      ],
    });
    controller.open();
    expect(controller.state.value).toMatchObject({
      expanded: true,
      selected: { kind: 'recover', requestId: 'claim-a' },
      requests: [{ subjectId: 'pass-a', subjectName: '<script>not markup</script>' }],
    });
  });
  it('keeps errors above a reconciled queue until explicit dismissal without reserving a receipt timer', async () => {
    vi.useFakeTimers();
    const { controller } = setup();
    controller.open();
    controller.showError('Could not sync Vault.');
    await vi.advanceTimersByTimeAsync(3_600_000);
    expect(controller.state.value.error).toBe('Could not sync Vault.');
    controller.reconcile({ session: session(), requests: [request(), request('recover', 'claim-b')] });
    expect(controller.state.value.requests).toHaveLength(2);
    controller.dismissError();
    expect(controller.state.value).toMatchObject({ error: null, expanded: true, selected: { requestId: 'request-a' } });
  });
});

describe('Liveboard decisions authenticate both branches', () => {
  it.each(kinds.flatMap((kind) => [true, false].map((accepted) => ({ kind, accepted }))))(
    '$kind accepted=$accepted: cancel authenticates but makes zero decisions and retains the item',
    async ({ kind, accepted }) => {
      const { controller, port } = setup({ authenticate: vi.fn(async () => false) });
      controller.reconcile({ session: session(), requests: [request(kind)] });
      controller.open();
      await controller.decide(accepted);
      expect(port.authenticate).toHaveBeenCalledWith({ kind, requestId: 'request-a', accepted });
      expect(port.decide).not.toHaveBeenCalled();
      expect(controller.state.value).toMatchObject({ busy: false, requests: [{ kind, requestId: 'request-a' }] });
    },
  );
  it.each([true, false])('treats NotAllowedError as no decision for accepted=%s', async (accepted) => {
    const { controller, port } = setup({
      authenticate: vi.fn(async () => {
        throw new DOMException('Cancelled', 'NotAllowedError');
      }),
    });
    controller.open();
    await controller.decide(accepted);
    expect(port.authenticate).toHaveBeenCalledOnce();
    expect(port.decide).not.toHaveBeenCalled();
    expect(controller.state.value.requests).toHaveLength(1);
    expect(controller.state.value.busy).toBe(false);
  });
  it('serializes repeated clicks while the real authentication is outstanding', async () => {
    const auth = deferred<boolean>(),
      { controller, port } = setup({ authenticate: vi.fn(() => auth.promise) });
    controller.open();
    const first = controller.decide(true);
    await controller.decide(false);
    expect(port.authenticate).toHaveBeenCalledOnce();
    expect(controller.state.value.busy).toBe(true);
    auth.resolve(true);
    await first;
    expect(port.decide).toHaveBeenCalledExactlyOnceWith({ kind: 'join', requestId: 'request-a', accepted: true });
  });
  it('never applies a late authentication result to the next queue head', async () => {
    const auth = deferred<boolean>(),
      { controller, port } = setup({ authenticate: vi.fn(() => auth.promise) });
    controller.open();
    const pending = controller.decide(true);
    expect(port.authenticate).toHaveBeenCalledOnce();
    controller.reconcile({ session: session(), requests: [request('join', 'request-b')] });
    auth.resolve(true);
    await pending;
    expect(port.decide).not.toHaveBeenCalled();
    expect(controller.state.value.requests[0]?.requestId).toBe('request-b');
  });
  it.each(['session', 'vault', 'member', 'ready', 'dispose'] as const)(
    'fences a pending auth result after %s changes',
    async (change) => {
      const auth = deferred<boolean>(),
        { controller, port } = setup({ authenticate: vi.fn(() => auth.promise) });
      controller.open();
      const pending = controller.decide(false);
      expect(port.authenticate).toHaveBeenCalledOnce();
      if (change === 'dispose') controller.dispose();
      else
        controller.reconcile({
          session: session({
            identity: change === 'session' ? 'session-b' : 'session-a',
            vaultId: change === 'vault' ? 'vault-b' : 'vault-a',
            member: change !== 'member',
            ready: change !== 'ready',
          }),
          requests: [request()],
        });
      auth.resolve(true);
      await pending;
      expect(port.decide).not.toHaveBeenCalled();
    },
  );
  it('does not authenticate a Core-disabled branch', async () => {
    const { controller, port } = setup();
    controller.reconcile({ session: session(), requests: [request('join', 'request-a', { canDecline: false })] });
    controller.open();
    expect(controller.state.value.selected).toEqual({ kind: 'join', requestId: 'request-a' });
    await controller.decide(false);
    expect(port.authenticate).not.toHaveBeenCalled();
    expect(port.decide).not.toHaveBeenCalled();
  });
  it('keeps a locally submitted decision disabled until canonical reconciliation, without dropping the request', async () => {
    const { controller, port } = setup();
    controller.open();
    await controller.decide(true);
    expect(port.decide).toHaveBeenCalledOnce();
    expect(controller.state.value.requests).toHaveLength(1);
    await controller.decide(false);
    expect(port.decide).toHaveBeenCalledOnce();
    controller.reconcile({ session: session(), requests: [request('recover', 'claim-next')] });
    expect(controller.state.value.requests[0]?.requestId).toBe('claim-next');
    expect(controller.state.value.busy).toBe(false);
  });
  it('does not duplicate an unknown or still in-flight submission on refresh/click', async () => {
    const submission = deferred<OperationReceipt>();
    const { controller, port } = setup({ decide: vi.fn(() => submission.promise) });
    controller.open();
    const first = controller.decide(true);
    await vi.waitFor(() => expect(port.decide).toHaveBeenCalledOnce());
    controller.reconcile({ session: session(), requests: [request()] });
    await controller.decide(true);
    expect(port.authenticate).toHaveBeenCalledOnce();
    submission.resolve({ operationId: 'exact-a', phase: 'unknown' });
    await first;
    await controller.decide(false);
    expect(port.decide).toHaveBeenCalledOnce();
  });
  it('preserves the exact queue and exposes a submission error for manual dismissal', async () => {
    const { controller, port } = setup({
      decide: vi.fn(async () => {
        throw new Error('Unavailable');
      }),
    });
    controller.open();
    await controller.decide(false);
    expect(port.decide).toHaveBeenCalledOnce();
    expect(controller.state.value.error).toBeTruthy();
    expect(controller.state.value.requests[0]?.requestId).toBe('request-a');
  });
});
