import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import NotificationTile from '@/components/liveboard/NotificationTile.vue';
import type { NotificationView } from '@/features/liveboard/types';
import { request } from './liveboardFixtures';
import {
  after,
  back,
  before,
  concealed,
  deferred,
  dismiss,
  firstId,
  fresh,
  head,
  marker,
  mixed,
  noBusiness,
  open,
  ownId,
  rejectedCopy,
  response,
  secondId,
  settle,
  setup,
  show,
  tick,
  vault,
} from './liveboardResponseReadyFixtures';

beforeEach(before);
afterEach(after);
const hit = (s: Awaited<ReturnType<typeof setup>>) => s.wrapper.get<HTMLButtonElement>('.notification-hit');
const view = (): NotificationView => ({
  requests: [request('recover', firstId), request('delete-secret', secondId)],
  selected: null,
  expanded: false,
  busy: false,
  error: null,
});
async function copiedFailureWithMixedQueue() {
  const s = await setup('/secrets');
  const accepted = response();
  accepted.operations.push(...mixed().operations);
  accepted.secrets[2] = mixed().secrets[2]!;
  await fresh(s, accepted);
  const done = {
    ...accepted,
    operations: accepted.operations.map((op) =>
      op.requestId === ownId ? { ...op, clientStatus: 'done', readiness: 'done' } : op,
    ),
    secrets: accepted.secrets.map((secret) =>
      secret.secretId === 'pass-a' ? { ...secret, primaryAction: 'recover', recoveryClaimId: undefined } : secret,
    ),
  };
  vi.mocked(s.manager.show_recovered).mockImplementation(async () => {
    s.setDTO(done);
    return 'Explicit own recovered sample';
  });
  await show(s);
  await rejectedCopy(s);
  await back(s);
  return { s, done };
}

describe('Native availability from actual current parent readiness, with exact FIFO', () => {
  it('Copy error manual tap holds physical sync/read, then one Open/Approve and next normal-read Open/Decline reach exact FIFO heads', async () => {
    const { s, done } = await copiedFailureWithMixedQueue();
    concealed(s);
    expect(s.controller.notifications.state.value.requests.map((item) => item.requestId)).toEqual([firstId, secondId]);
    const sync = s.holdSync(),
      read = s.holdRead();
    const count = vi.mocked(s.manager.sync_now).mock.calls.length;
    await dismiss(s);
    expect(s.manager.sync_now).toHaveBeenCalledTimes(count + 1);
    expect(s.session.state.value.ready).toBe(false);
    expect.soft(hit(s).element.disabled).toBe(true);
    await hit(s).trigger('click');
    expect(s.wrapper.find('.notification-actions').exists()).toBe(false);
    expect(s.auth.authenticateWithPasskey).not.toHaveBeenCalled();
    noBusiness(s);
    marker(s, 'FIFO-held-physical-sync');
    sync.resolve();
    await settle();
    expect(s.session.state.value.ready).toBe(false);
    expect.soft(hit(s).element.disabled).toBe(true);
    expect(s.wrapper.get('.notification-count').text()).toBe('2');
    concealed(s);
    marker(s, 'FIFO-held-fresh-read');
    read.resolve();
    await settle();
    expect(s.session.state.value.ready).toBe(true);
    expect(hit(s).element.disabled).toBe(false);
    await open(s);
    head(s, 'recover', firstId, 2);
    const remaining = { ...done, operations: done.operations.filter((op) => op.requestId !== firstId) };
    vi.mocked(s.manager.accept_recover).mockImplementation(async (claim) => {
      expect(claim.wasm_id_str()).toBe(firstId);
      s.setDTO(remaining);
    });
    await s.wrapper.get('[data-testid="approve-recover-request"]').trigger('click');
    await settle();
    expect(s.manager.accept_recover).toHaveBeenCalledOnce();
    expect(s.auth.authenticateWithPasskey).toHaveBeenCalledOnce();
    expect(s.wrapper.get('.notification-count').text()).toBe('1');
    expect(s.controller.notifications.state.value.requests[0]?.requestId).toBe(secondId);
    await s.wrapper.get('a[aria-label="Open Profile"]').trigger('click');
    await settle();
    const nextRead = s.holdRead();
    await back(s); // Existing route refresh, not a synthetic readiness rescue.
    expect(s.session.state.value.ready).toBe(false);
    expect.soft(hit(s).element.disabled).toBe(true);
    await hit(s).trigger('click');
    expect(s.wrapper.find('.notification-actions').exists()).toBe(false);
    expect(s.manager.decide_delete_secret).not.toHaveBeenCalled();
    marker(s, 'FIFO-next-Delete-normal-read-held');
    nextRead.resolve();
    await settle();
    expect(s.session.state.value.ready).toBe(true);
    await open(s);
    head(s, 'delete-secret', secondId, 1);
    vi.mocked(s.manager.decide_delete_secret).mockImplementation(async () => {
      s.setDTO({
        ...remaining,
        operations: remaining.operations.filter((op) => op.requestId !== secondId),
        secrets: remaining.secrets.map((secret) =>
          secret.secretId === 'pass-c' ? { ...secret, canDelete: true, deletionRequestId: undefined } : secret,
        ),
      });
    });
    await s.wrapper.get('[data-testid="decline-delete-secret-request"]').trigger('click');
    await settle();
    expect(s.manager.decide_delete_secret).toHaveBeenCalledExactlyOnceWith(secondId, false);
    expect(s.auth.authenticateWithPasskey).toHaveBeenCalledTimes(2);
    expect(s.controller.notifications.state.value.requests).toEqual([]);
    expect(s.wrapper.get('.tile-notifications').attributes('data-state')).toBe('empty');
    expect(s.manager.show_recovered).toHaveBeenCalledOnce();
    expect(s.clipboard.writeText).toHaveBeenCalledOnce();
    expect(s.physical.maximum).toBe(1);
    noBusiness(s, ['accept_recover', 'decide_delete_secret']);
    concealed(s);
    marker(s, 'FIFO-final-both-exact-decisions-reached');
  });

  it('a failed actual manual sync plus a cached publication never enables Open', async () => {
    const { s } = await copiedFailureWithMixedQueue();
    const failedCount = vi.mocked(s.manager.sync_now).mock.calls.length;
    vi.mocked(s.manager.sync_now).mockRejectedValueOnce(new Error('private-sync-error'));
    await dismiss(s);
    expect(s.manager.sync_now).toHaveBeenCalledTimes(failedCount + 1);
    expect(s.session.state.value.ready).toBe(false);
    expect(s.store.requiredSyncPending).toBe(true);
    await s.store.updateState();
    await settle();
    await tick();
    expect(s.session.state.value.ready).toBe(false);
    expect(s.store.requiredSyncPending).toBe(true);
    expect(s.wrapper.get('.tile-notifications').attributes('data-state')).toBe('error');
    const errorDismiss = s.wrapper.get<HTMLButtonElement>('.notification-hit[aria-label="Dismiss error"]');
    expect(errorDismiss.element.disabled).toBe(false);
    expect(s.wrapper.find('.notification-hit[aria-label^="Open "]').exists()).toBe(false);
    expect(s.wrapper.find('.notification-actions').exists()).toBe(false);
    expect(s.auth.authenticateWithPasskey).not.toHaveBeenCalled();
    expect(s.controller.notifications.state.value.requests.map(({ kind, requestId }) => [kind, requestId])).toEqual([
      ['recover', firstId],
      ['delete-secret', secondId],
    ]);
    noBusiness(s);
    concealed(s);
    marker(s, 'failed-cache-final-blocked');
    const retrySync = s.holdSync(),
      retryRead = s.holdRead();
    const syncCount = vi.mocked(s.manager.sync_now).mock.calls.length,
      readCount = vi.mocked(s.manager.get_state).mock.calls.length,
      orderStart = s.order.length;
    await dismiss(s); // Actual enabled ERROR tile tap requests the existing physical retry.
    expect(s.manager.sync_now).toHaveBeenCalledTimes(syncCount + 1);
    expect(s.manager.get_state).toHaveBeenCalledTimes(readCount);
    expect(s.order.slice(orderStart)).toEqual(['sync']);
    expect(s.session.state.value.ready).toBe(false);
    expect(s.store.requiredSyncPending).toBe(true);
    expect(hit(s).attributes('aria-label')).toMatch(/^Open RECOVER SECRET from /);
    expect(hit(s).element.disabled).toBe(true);
    expect(s.wrapper.get('.notification-count').text()).toBe('2');
    await hit(s).trigger('click');
    expect(s.wrapper.find('.notification-actions').exists()).toBe(false);
    expect(s.auth.authenticateWithPasskey).not.toHaveBeenCalled();
    noBusiness(s);
    concealed(s);
    marker(s, 'failed-cache-retry-physical-sync-held');
    retrySync.resolve();
    await settle();
    expect(s.order.slice(orderStart)).toEqual(['sync', 'read']);
    expect(s.manager.get_state).toHaveBeenCalledTimes(readCount + 1);
    expect(s.session.state.value.ready).toBe(false);
    expect(s.store.requiredSyncPending).toBe(true);
    expect(hit(s).element.disabled).toBe(true);
    expect(s.wrapper.find('.notification-actions').exists()).toBe(false);
    expect(s.controller.notifications.state.value.requests.map((item) => item.requestId)).toEqual([firstId, secondId]);
    marker(s, 'failed-cache-retry-fresh-read-held');
    retryRead.resolve();
    await settle();
    expect(s.session.state.value.ready).toBe(true);
    expect(s.store.requiredSyncPending).toBe(false);
    expect(hit(s).attributes('aria-label')).toMatch(/^Open RECOVER SECRET from /);
    expect(hit(s).element.disabled).toBe(false);
    expect(s.wrapper.get('.notification-count').text()).toBe('2');
    expect(s.controller.notifications.state.value.requests.map((item) => item.requestId)).toEqual([firstId, secondId]);
    expect(s.wrapper.find('.notification-actions').exists()).toBe(false);
    expect(s.auth.authenticateWithPasskey).not.toHaveBeenCalled();
    expect(s.manager.sync_now).toHaveBeenCalledTimes(syncCount + 1);
    expect(s.manager.get_state).toHaveBeenCalledTimes(readCount + 1);
    expect(s.physical.maximum).toBe(1);
    noBusiness(s);
    concealed(s);
    marker(s, 'failed-cache-retry-final-actual-ready');
  });

  it('a real unrelated pending Device removal does not disable an otherwise ready incoming request', async () => {
    const s = await setup();
    await fresh(s, mixed());
    await s.wrapper.get('[data-device-id="peer-a"]').trigger('click');
    await s.wrapper.get('[data-testid="remove-device"]').trigger('click');
    await settle();
    expect(s.manager.remove_member_checked).toHaveBeenCalledOnce();
    expect(s.session.state.value.ready).toBe(true);
    expect(vault(s).storage.busy).toBe(true);
    expect(hit(s).element.disabled).toBe(false);
    await open(s);
    head(s, 'recover', firstId, 2);
    expect(s.wrapper.get<HTMLButtonElement>('[data-testid="approve-recover-request"]').element.disabled).toBe(false);
    expect(s.wrapper.get<HTMLButtonElement>('[data-testid="decline-recover-request"]').element.disabled).toBe(false);
    expect(s.auth.authenticateWithPasskey).toHaveBeenCalledOnce(); // Device removal only.
    noBusiness(s, ['remove_member_checked']);
    marker(s, 'unrelated-device-pending-final-actionable');
  });

  it('cancelling authentication leaves the same exact actionable head and sends no decision', async () => {
    const s = await setup();
    await fresh(s, mixed());
    await open(s);
    head(s, 'recover', firstId, 2);
    s.auth.authenticateWithPasskey.mockResolvedValueOnce(false);
    await s.wrapper.get('[data-testid="approve-recover-request"]').trigger('click');
    await settle();
    expect(s.auth.authenticateWithPasskey).toHaveBeenCalledOnce();
    expect(s.manager.accept_recover).not.toHaveBeenCalled();
    await open(s);
    head(s, 'recover', firstId, 2);
    noBusiness(s);
    marker(s, 'cancelled-auth-final-same-head');
  });

  it('a key boundary during real authentication never decides or restores old actions', async () => {
    const s = await setup();
    await fresh(s, mixed());
    await open(s);
    const auth = deferred<boolean>();
    s.auth.authenticateWithPasskey.mockReturnValueOnce(auth.promise);
    await s.wrapper.get('[data-testid="approve-recover-request"]').trigger('click');
    await settle();
    const old = s.controller;
    s.auth.masterKey = 'next-owner-key';
    await settle();
    auth.resolve(true);
    await settle();
    expect(s.manager.accept_recover).not.toHaveBeenCalled();
    expect(old.notifications.state.value.expanded).toBe(false);
    expect(Object.values(old.secrets.state.value.cards).every((card) => card.value === null)).toBe(true);
    noBusiness(s);
    marker(s, 'stale-auth-final-no-decision');
  });
});

describe('Actual NotificationTile native controls complement the real parent regressions', () => {
  it.each([true, false])(
    'unready expanded controls never emit %s decision despite retained Core permission',
    async (accepted) => {
      const next = { ...view(), expanded: true };
      const wrapper = mount(NotificationTile, { props: { view: next, ...{ ready: false } } });
      const control = wrapper.get<HTMLButtonElement>(
        `[data-testid="${accepted ? 'approve' : 'decline'}-recover-request"]`,
      );
      expect.soft(control.element.disabled).toBe(true);
      await control.trigger('click');
      expect.soft(wrapper.emitted('decide')).toBeUndefined();
      wrapper.unmount();
    },
  );

  it('error Dismiss stays native-clickable while unready and busy, emitting only dismissal without queue mutation', async () => {
    const next = { ...view(), busy: true, error: 'Public failure' };
    const wrapper = mount(NotificationTile, { props: { view: next, ...{ ready: false } } });
    const button = wrapper.get<HTMLButtonElement>('.notification-hit');
    expect(button.element.disabled).toBe(false);
    await button.trigger('click');
    expect(wrapper.emitted('dismissError')).toHaveLength(1);
    expect(wrapper.emitted('open')).toBeUndefined();
    expect(wrapper.emitted('decide')).toBeUndefined();
    expect(next.requests).toHaveLength(2);
    expect(next.error).toBe('Public failure');
    wrapper.unmount();
  });

  it('ready still respects each exact Core permission instead of granting an unavailable decision', async () => {
    const item = { ...request('recover', firstId), canApprove: false, canDecline: true };
    const wrapper = mount(NotificationTile, {
      props: { view: { ...view(), requests: [item], expanded: true }, ...{ ready: true } },
    });
    expect(wrapper.get<HTMLButtonElement>('[data-testid="approve-recover-request"]').element.disabled).toBe(true);
    expect(wrapper.get<HTMLButtonElement>('[data-testid="decline-recover-request"]').element.disabled).toBe(false);
    await wrapper.get('[data-testid="approve-recover-request"]').trigger('click');
    expect(wrapper.emitted('decide')).toBeUndefined();
    wrapper.unmount();
  });
});
