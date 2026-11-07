import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  after,
  back,
  before,
  concealed,
  copyError,
  dismiss,
  effects,
  ids,
  liveMotion,
  localDTO,
  noBusiness,
  rejectedCopy,
  retainedQueue,
  settle,
  setup,
  show,
  tick,
} from './liveboardManualErrorReadinessFixtures';

beforeEach(before);
afterEach(after);
describe('Manual04 error recovery through actual App/router/VaultView/parent/Store lane', () => {
  it('Copy rejection conceals ALL values, then actual Back and manual tap request sync→fresh read and restore exact FIFO actions', async () => {
    const s = await setup();
    await show(s);
    await show(s, 'pass-b');
    await s.wrapper.get('[data-secret-id="pass-b"] [data-secret-action="copy"]').trigger('click');
    await settle();
    expect(s.clipboard.writeText).toHaveBeenCalledExactlyOnceWith('Beta local sample');
    expect(s.controller.secrets.state.value.cards['pass-b']?.copied).toBe(true);
    const activeEffects = liveMotion();
    await rejectedCopy(s);
    s.stage('principal-copy-rejected');
    concealed(s);
    retainedQueue(s);
    expect.soft(activeEffects.every((effect) => effect.cancel.mock.calls.length > 0)).toBe(true);
    expect(s.session.state.value.ready).toBe(false);
    await tick();
    expect(s.controller.notifications.state.value.error).toBe(copyError);
    await back(s);
    const sync = s.holdSync(),
      read = s.holdRead();
    const syncCount = vi.mocked(s.manager.sync_now).mock.calls.length,
      readCount = vi.mocked(s.manager.get_state).mock.calls.length;
    s.order.length = 0;
    s.setDTO({ ...localDTO(), operations: [...localDTO().operations].reverse() });
    await dismiss(s);
    expect.soft(s.manager.sync_now).toHaveBeenCalledTimes(syncCount + 1);
    if (vi.mocked(s.manager.sync_now).mock.calls.length !== syncCount + 1) {
      s.stage('UNREACHED-principal-held-sync-read-and-final-FIFO-after-missing-physical-sync');
      return;
    }
    s.stage('principal-physical-sync-entered');
    expect(s.order).toEqual(['sync']);
    expect(s.manager.get_state).toHaveBeenCalledTimes(readCount);
    expect(s.session.state.value.ready).toBe(false);
    await s.wrapper.get('.notification-hit').trigger('click');
    expect(s.wrapper.find('.notification-actions').exists()).toBe(false);
    noBusiness(s);
    sync.resolve();
    await settle();
    expect(s.order).toEqual(['sync', 'read']);
    expect(s.session.state.value.ready).toBe(false);
    await tick();
    expect(s.session.state.value.ready).toBe(false);
    read.resolve();
    await settle();
    expect.soft(s.session.state.value.ready).toBe(true);
    concealed(s);
    retainedQueue(s);
    expect(s.wrapper.get('.notification-count').text()).toBe('2');
    await s.wrapper.get('.notification-hit').trigger('click');
    expect(s.wrapper.get('.notification-actions').attributes('data-request-id')).toBe(ids[0]);
    await s.wrapper.get('[data-testid="approve-recover-request"]').trigger('click');
    await settle();
    expect(s.auth.authenticateWithPasskey).toHaveBeenCalledOnce();
    expect(s.manager.accept_recover).toHaveBeenCalledOnce();
    expect(vi.mocked(s.manager.accept_recover).mock.calls[0]![0].wasm_id_str()).toBe(ids[0]);
    expect(s.manager.decline_recover).not.toHaveBeenCalled();
    expect(s.manager.show_local_secret).toHaveBeenCalledTimes(2);
    expect(s.clipboard.writeText).toHaveBeenCalledTimes(2);
    expect(s.physical.maximum).toBe(1);
    noBusiness(s, ['accept_recover']);
    s.stage('principal-final-FIFO-reached');
  });

  it('a normal genuine required read may restore technical readiness while manual04 error remains pinned', async () => {
    const s = await setup();
    await show(s);
    await rejectedCopy(s);
    await s.session.refresh({ requiredSync: true });
    await settle(); // Named ordinary-read control, never principal rescue.
    expect(s.session.state.value.ready).toBe(true);
    expect(s.controller.notifications.state.value.error).toBe(copyError);
    retainedQueue(s);
    await tick();
    expect(s.controller.notifications.state.value.error).toBe(copyError);
    await back(s);
    expect(s.wrapper.get('[data-testid="liveboard-notifications"]').attributes('data-state')).toBe('error');
    expect(s.wrapper.find('.notification-actions').exists()).toBe(false);
    noBusiness(s);
    concealed(s);
  });

  it('the SAME sanitized Copy report is a new hard event after ordinary fresh readiness, with no error auto-clear', async () => {
    const s = await setup();
    await show(s);
    await rejectedCopy(s);
    await s.session.refresh({ requiredSync: true });
    await settle();
    expect(s.session.state.value.ready).toBe(true);
    expect(s.controller.notifications.state.value.error).toBe(copyError);
    await show(s, 'pass-b');
    const active = liveMotion();
    await rejectedCopy(s, 'pass-b');
    concealed(s);
    expect.soft(active.every((effect) => effect.cancel.mock.calls.length)).toBe(true);
    expect.soft(s.session.state.value.ready).toBe(false);
    expect.soft(s.controller.secrets.state.value.blocked).toBe(true);
    expect(s.controller.notifications.state.value.error).toBe(copyError);
    retainedQueue(s);
    expect(s.manager.show_local_secret).toHaveBeenCalledTimes(2);
    expect(s.clipboard.writeText).toHaveBeenCalledTimes(2);
    noBusiness(s);
    s.stage('same-text-final-privacy-checked');
  });

  it('a failed manual required sync stays blocked through cache-only publication and recovers only after a later manual successful sync/read', async () => {
    const s = await setup();
    await show(s);
    await rejectedCopy(s);
    await back(s);
    const count = vi.mocked(s.manager.sync_now).mock.calls.length;
    vi.mocked(s.manager.sync_now).mockRejectedValueOnce(new Error('private-sync-error'));
    await dismiss(s);
    expect.soft(s.manager.sync_now).toHaveBeenCalledTimes(count + 1);
    if (vi.mocked(s.manager.sync_now).mock.calls.length !== count + 1) {
      s.stage('UNREACHED-failed-sync-cache-after-missing-sync');
      return;
    }
    expect(s.session.state.value.ready).toBe(false);
    expect(s.store.requiredSyncPending).toBe(true);
    const requiredError = s.controller.notifications.state.value.error;
    expect(requiredError).not.toBeNull();
    await s.store.updateState();
    await settle();
    await tick();
    expect(s.session.state.value.ready).toBe(false);
    expect(s.store.requiredSyncPending).toBe(true);
    expect(s.wrapper.find('.notification-actions').exists()).toBe(false);
    noBusiness(s);
    concealed(s);
    expect(s.manager.sync_now).toHaveBeenCalledTimes(count + 1);
    await dismiss(s);
    await settle();
    expect(s.manager.sync_now).toHaveBeenCalledTimes(count + 2);
    expect(s.session.state.value.ready).toBe(true);
    expect(s.controller.notifications.state.value.error).toBeNull();
    retainedQueue(s);
    s.stage('failed-sync-final-current-success');
  });

  it('a newer required error target queues behind an older physical read and repeated manual clicks coalesce in the same lane', async () => {
    const s = await setup('/');
    const oldRead = s.holdRead(),
      freshRead = s.holdRead(),
      sync = s.holdSync();
    const older = s.store.updateState();
    await settle();
    s.controller.showError('Current public error');
    await settle();
    const button = s.wrapper.get('[aria-label="Dismiss error"]').element as HTMLButtonElement;
    const syncCount = vi.mocked(s.manager.sync_now).mock.calls.length;
    button.click();
    button.click();
    await settle();
    expect(s.manager.sync_now).toHaveBeenCalledTimes(syncCount);
    expect(s.session.state.value.ready).toBe(false);
    oldRead.resolve();
    await older;
    await settle();
    expect.soft(s.manager.sync_now).toHaveBeenCalledTimes(syncCount + 1);
    if (vi.mocked(s.manager.sync_now).mock.calls.length !== syncCount + 1) {
      s.stage('UNREACHED-new-target-after-old-read');
      return;
    }
    expect(s.session.state.value.ready).toBe(false);
    sync.resolve();
    await settle();
    expect(s.session.state.value.ready).toBe(false);
    freshRead.resolve();
    await settle();
    expect(s.session.state.value.ready).toBe(true);
    expect(s.physical.maximum).toBe(1);
    expect(s.manager.sync_now).toHaveBeenCalledTimes(syncCount + 1);
    retainedQueue(s);
    noBusiness(s);
  });

  it('completion of a pre-existing dismiss read cannot certify a newer error or clear its pinned message', async () => {
    const s = await setup('/');
    s.controller.showError('First error');
    await settle();
    const first = s.holdSync();
    const count = vi.mocked(s.manager.sync_now).mock.calls.length;
    await dismiss(s);
    expect.soft(s.manager.sync_now).toHaveBeenCalledTimes(count + 1);
    if (vi.mocked(s.manager.sync_now).mock.calls.length !== count + 1) {
      s.stage('UNREACHED-new-error-target');
      return;
    }
    s.controller.showError('Newer error');
    first.resolve();
    await settle();
    expect.soft(s.session.state.value.ready).toBe(false);
    expect(s.controller.notifications.state.value.error).toBe('Newer error');
    await dismiss(s);
    await settle();
    expect(s.session.state.value.ready).toBe(true);
    expect(s.manager.sync_now).toHaveBeenCalledTimes(count + 2);
    noBusiness(s);
  });

  it('internal notification decision rejection shares the manual parent read path and retains submitted Unknown ownership', async () => {
    const s = await setup('/');
    vi.mocked(s.manager.accept_recover).mockRejectedValueOnce(new Error('private-decision-error'));
    await s.wrapper.get('.notification-hit').trigger('click');
    await s.wrapper.get('[data-testid="approve-recover-request"]').trigger('click');
    await settle();
    expect(s.controller.notifications.state.value.error).toBe('Request could not be completed.');
    const count = vi.mocked(s.manager.sync_now).mock.calls.length;
    await dismiss(s);
    expect.soft(s.manager.sync_now).toHaveBeenCalledTimes(count + 1);
    expect(s.controller.notifications.state.value.pendingDecision).toMatchObject({
      kind: 'recover',
      requestId: ids[0],
      accepted: true,
    });
    expect(s.controller.notifications.state.value.busy).toBe(true);
    retainedQueue(s);
    expect(s.manager.accept_recover).toHaveBeenCalledOnce();
    expect(s.auth.authenticateWithPasskey).toHaveBeenCalledOnce();
    expect(s.manager.decline_recover).not.toHaveBeenCalled();
    noBusiness(s, ['accept_recover']);
  });

  it('public controller errors preserve a reachable creation draft and use the same manual read without submitting it', async () => {
    const s = await setup('/');
    s.controller.create.open();
    s.controller.create.change({ name: 'private draft name', value: 'private draft text' });
    expect(s.controller.create.state.value.draft.value).toBe('private draft text');
    s.controller.showError('QR unavailable');
    await settle();
    expect(s.controller.create.state.value.draft.value).toBe('private draft text');
    expect(s.controller.create.state.value.open).toBe(false);
    const count = vi.mocked(s.manager.sync_now).mock.calls.length;
    await dismiss(s);
    expect.soft(s.manager.sync_now).toHaveBeenCalledTimes(count + 1);
    expect.soft(s.session.state.value.ready).toBe(true);
    noBusiness(s);
    concealed(s);
  });

  it('a successful Copy remains a non-write action with no physical sync or extra Show', async () => {
    const s = await setup();
    await show(s);
    const syncs = vi.mocked(s.manager.sync_now).mock.calls.length;
    await s.wrapper.get('[data-secret-action="copy"]').trigger('click');
    await settle();
    expect(s.clipboard.writeText).toHaveBeenCalledExactlyOnceWith('Alpha local sample');
    expect(s.manager.sync_now).toHaveBeenCalledTimes(syncs);
    expect(s.manager.show_local_secret).toHaveBeenCalledOnce();
    expect(s.controller.notifications.state.value.error).toBeNull();
    noBusiness(s);
    expect(effects.length).toBeGreaterThan(0);
  });
});
