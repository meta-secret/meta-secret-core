import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { deferred, managerFixture, operation, producer, requestId } from './liveboardCoreFixtures';
import {
  after,
  back,
  before,
  concealed,
  dismiss,
  ids,
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
describe('Manual error current-parent boundaries and preserved operation owners', () => {
  it.each(['auth', 'key', 'manager', 'generation', 'member', 'reset', 'dispose'] as const)(
    '%s during a manual physical read fences all late publication and never sends a queue action',
    async (boundary) => {
      const s = await setup();
      await show(s);
      await rejectedCopy(s);
      await back(s);
      const sync = s.holdSync(),
        read = s.holdRead();
      const count = vi.mocked(s.manager.sync_now).mock.calls.length;
      await dismiss(s);
      expect.soft(s.manager.sync_now).toHaveBeenCalledTimes(count + 1);
      if (vi.mocked(s.manager.sync_now).mock.calls.length !== count + 1) {
        s.stage(`UNREACHED-${boundary}-late-fence-after-missing-sync`);
        return;
      }
      sync.resolve();
      await settle();
      expect(s.session.state.value.ready).toBe(false);
      const retiringRoot = s.roots.at(-1)!.state;
      const old = s.controller,
        oldIdentity = s.session.state.value.identity;
      const next = managerFixture();
      if (boundary === 'auth') s.auth.isAuthenticated = false;
      if (boundary === 'key') s.auth.masterKey = 'replacement-key';
      if (boundary === 'manager') s.replaceManager(next);
      if (boundary === 'generation') {
        s.store.sessionGeneration++;
        s.publish();
      }
      if (boundary === 'member') {
        s.setMember(false);
        s.publish();
      }
      let resetting: Promise<void> | undefined;
      if (boundary === 'reset') {
        old.requestReset();
        resetting = old.confirmReset();
      }
      if (boundary === 'dispose') s.session.dispose();
      await settle();
      read.resolve();
      await resetting;
      await settle();
      await tick();
      expect(s.store.currState).not.toBe(retiringRoot);
      const successor = s.controller;
      if (['key', 'manager', 'generation'].includes(boundary)) {
        expect.soft(s.session.state.value.identity).not.toBe(oldIdentity);
        successor.showError('Successor pinned error');
        await settle();
        expect(successor.notifications.state.value.error).toBe('Successor pinned error');
      }
      expect
        .soft(Object.values(old.secrets.state.value.cards).every((card) => card.value === null && !card.copied))
        .toBe(true);
      noBusiness(s, boundary === 'reset' ? ['clean_up_database'] : []);
      expect(s.manager.accept_recover).not.toHaveBeenCalled();
      expect(s.auth.authenticateWithPasskey).not.toHaveBeenCalled();
      expect(s.manager.show_local_secret).toHaveBeenCalledOnce();
      concealed(s);
      if (boundary === 'dispose' || boundary === 'auth') expect(s.session.state.value.ready).toBe(false);
      s.stage(`${boundary}-late-final-checked`);
    },
  );

  it.each(['offline', 'hidden'] as const)(
    '%s manual dismissal retains required work until the normal eligible resume',
    async (boundary) => {
      const s = await setup();
      await show(s);
      await rejectedCopy(s);
      await back(s);
      const visibility = boundary === 'hidden' ? vi.spyOn(document, 'visibilityState', 'get') : null;
      if (visibility) {
        visibility.mockReturnValue('hidden');
        document.dispatchEvent(new Event('visibilitychange'));
      } else window.dispatchEvent(new Event('offline'));
      const count = vi.mocked(s.manager.sync_now).mock.calls.length;
      await dismiss(s);
      await tick();
      expect(s.manager.sync_now).toHaveBeenCalledTimes(count);
      expect(s.session.state.value.ready).toBe(false);
      concealed(s);
      noBusiness(s);
      if (visibility) {
        visibility.mockReturnValue('visible');
        document.dispatchEvent(new Event('visibilitychange'));
      } else window.dispatchEvent(new Event('online'));
      await settle();
      await tick();
      expect(s.manager.sync_now).toHaveBeenCalledTimes(count + 1);
      expect(s.session.state.value.ready).toBe(true);
      expect(s.manager.show_local_secret).toHaveBeenCalledOnce();
      retainedQueue(s);
      concealed(s);
    },
  );

  it('a late local Show result is dropped after a real public error, with no automatic re-Show', async () => {
    const s = await setup();
    const value = deferred<string>();
    vi.mocked(s.manager.show_local_secret).mockReturnValueOnce(value.promise);
    await s.wrapper.get('[data-secret-id="pass-a"] [data-secret-action="show"]').trigger('click');
    await settle();
    expect(s.manager.show_local_secret).toHaveBeenCalledOnce();
    s.controller.showError('Current error');
    await settle();
    value.resolve('late private sample');
    await settle();
    concealed(s);
    expect(s.wrapper.text()).not.toContain('late private sample');
    expect(s.session.state.value.ready).toBe(false);
    expect(s.controller.notifications.state.value.error).toBe('Current error');
    expect(s.manager.show_local_secret).toHaveBeenCalledOnce();
    noBusiness(s);
  });

  it.each(['sdk', 'http'] as const)(
    'an unfinished recovered %s wait is revoked by error before any matching Done can restore it',
    async (phase) => {
      const s = await setup();
      const recover = operation({
        requestId: 'claim-second',
        requesterId: 'self',
        eligiblePeerIds: ['peer-a'],
        canApprove: false,
        canDecline: false,
        clientStatus: 'accepted',
        outcome: 'accepted',
        responderId: 'peer-a',
        resolutionSource: 'verified',
        readiness: 'ready',
      });
      const accepted = {
        ...localDTO(),
        secrets: [
          {
            secretId: 'pass-a',
            primaryAction: 'showRecovered',
            recoveryClaimId: 'claim-second',
            canDelete: true,
            readiness: 'ready',
          },
          localDTO().secrets[1]!,
        ],
        operations: [recover],
      };
      s.setDTO(accepted);
      await s.session.refresh({ requiredSync: true });
      await settle();
      const raw = deferred<string>();
      vi.mocked(s.manager.show_recovered).mockReturnValueOnce(raw.promise);
      s.setAutoHeaders(false);
      const showing = s.controller.secrets.primary('pass-a');
      await settle();
      expect(s.manager.show_recovered).toHaveBeenCalledOnce();
      if (phase === 'http') {
        raw.resolve('unfinished recovered sample');
        await settle();
        expect(s.requests).toHaveLength(2);
      }
      s.controller.showError('Current error');
      await settle();
      expect
        .soft(s.controller.secrets.pendingRecoveredReveal({ passId: 'pass-a', claimId: 'claim-second' }))
        .toBeUndefined();
      s.setDTO({
        ...accepted,
        secrets: [
          { secretId: 'pass-a', primaryAction: 'recover', canDelete: true, readiness: 'ready' },
          localDTO().secrets[1]!,
        ],
        operations: [{ ...recover, clientStatus: 'done', readiness: 'done' }],
      });
      raw.resolve('unfinished recovered sample');
      for (const request of s.requests) if (!request.signal.aborted) request.respond();
      await showing;
      await settle();
      expect(s.wrapper.text()).not.toContain('unfinished recovered sample');
      concealed(s);
      expect.soft(s.controller.state.value.unread['claim-second']).toBe('green');
      const owned = vi.mocked(s.manager.show_recovered).mock.calls[0]![0];
      expect(owned.free).toHaveBeenCalledOnce();
      expect(s.manager.show_recovered).toHaveBeenCalledOnce();
      noBusiness(s);
      s.stage(`${phase}-late-value-final-checked`);
    },
  );

  it('the same explicit error preserves a later reachable draft even while the original04 text stays visible', async () => {
    const s = await setup('/');
    s.controller.showError('Repeated public error');
    await s.session.refresh({ requiredSync: true });
    await settle();
    expect(s.session.state.value.ready).toBe(true);
    expect(s.controller.notifications.state.value.error).toBe('Repeated public error');
    s.controller.create.open();
    s.controller.create.change({ name: 'private name', value: 'private value', words: Array(24).fill('private') });
    expect(s.controller.create.state.value.open).toBe(true);
    s.controller.showError('Repeated public error');
    await settle();
    expect.soft(s.controller.create.state.value.open).toBe(false);
    expect
      .soft(s.controller.create.state.value.draft)
      .toMatchObject({ name: 'private name', value: 'private value', words: Array(24).fill('private') });
    expect.soft(s.session.state.value.ready).toBe(false);
    noBusiness(s);
  });

  it.each(['sdk', 'http'] as const)(
    'a repeated SAME error revokes a later unfinished recovered %s owner instead of publishing its late value',
    async (phase) => {
      const s = await setup();
      s.controller.showError('Repeated public error');
      const recover = operation({
        requestId: 'claim-second',
        requesterId: 'self',
        eligiblePeerIds: ['peer-a'],
        canApprove: false,
        canDecline: false,
        clientStatus: 'accepted',
        outcome: 'accepted',
        responderId: 'peer-a',
        resolutionSource: 'verified',
        readiness: 'ready',
      });
      const accepted = {
        ...localDTO(),
        secrets: [
          {
            secretId: 'pass-a',
            primaryAction: 'showRecovered',
            recoveryClaimId: 'claim-second',
            canDelete: true,
            readiness: 'ready',
          },
          localDTO().secrets[1]!,
        ],
        operations: [recover],
      };
      s.setDTO(accepted);
      await s.session.refresh({ requiredSync: true });
      await settle();
      expect(s.session.state.value.ready).toBe(true);
      expect(s.controller.notifications.state.value.error).toBe('Repeated public error');
      const raw = deferred<string>();
      vi.mocked(s.manager.show_recovered).mockReturnValueOnce(raw.promise);
      s.setAutoHeaders(false);
      const showing = s.controller.secrets.primary('pass-a');
      await settle();
      expect(s.controller.secrets.pendingRecoveredReveal({ passId: 'pass-a', claimId: 'claim-second' })).toBeDefined();
      if (phase === 'http') {
        raw.resolve('repeated late recovered sample');
        await settle();
        expect(s.requests).toHaveLength(2);
      }
      s.controller.showError('Repeated public error');
      await settle();
      expect
        .soft(s.controller.secrets.pendingRecoveredReveal({ passId: 'pass-a', claimId: 'claim-second' }))
        .toBeUndefined();
      expect.soft(s.session.state.value.ready).toBe(false);
      s.setDTO({
        ...accepted,
        secrets: [
          { secretId: 'pass-a', primaryAction: 'recover', canDelete: true, readiness: 'ready' },
          localDTO().secrets[1]!,
        ],
        operations: [{ ...recover, clientStatus: 'done', readiness: 'done' }],
      });
      raw.resolve('repeated late recovered sample');
      await settle();
      for (const request of s.requests) if (!request.signal.aborted) request.respond();
      await showing;
      await settle();
      concealed(s);
      expect.soft(s.wrapper.text()).not.toContain('repeated late recovered sample');
      expect.soft(s.controller.state.value.unread['claim-second']).toBe('green');
      expect(s.controller.notifications.state.value.error).toBe('Repeated public error');
      expect(s.manager.show_recovered).toHaveBeenCalledOnce();
      expect(vi.mocked(s.manager.show_recovered).mock.calls[0]![0].free).toHaveBeenCalledOnce();
      noBusiness(s);
      s.stage(`${phase}-same-event-late-final-checked`);
    },
  );

  it('Unknown Remove keeps the original actual R and pending owner through manual error read without resubmit or continuation', async () => {
    const s = await setup('/');
    vi.mocked(s.manager.remove_member_checked).mockResolvedValue(producer({ kind: 'removeDevice', request: false }));
    vi.mocked(s.manager.reconcile_producer_receipt).mockResolvedValue(
      producer({ kind: 'removeDevice', request: false }),
    );
    s.controller.devices.preview('peer-a');
    await s.controller.devices.remove();
    expect(s.manager.remove_member_checked).toHaveBeenCalledOnce();
    expect(s.controller.devices.hasPending()).toBe(true);
    const references = vi.mocked(s.manager.reconcile_producer_receipt).mock.calls.map(([reference]) => reference);
    expect(references.length).toBeGreaterThan(0);
    s.controller.showError('Current error');
    await settle();
    const count = vi.mocked(s.manager.sync_now).mock.calls.length;
    await dismiss(s);
    expect.soft(s.manager.sync_now).toHaveBeenCalledTimes(count + 1);
    expect(s.controller.devices.hasPending()).toBe(true);
    expect(s.controller.devices.state.value.busy).toBe(true);
    for (const [reference] of vi.mocked(s.manager.reconcile_producer_receipt).mock.calls)
      expect(reference).toBe(references[0]);
    await s.controller.devices.remove();
    expect(s.manager.remove_member_checked).toHaveBeenCalledOnce();
    expect(s.manager.continue_local_remove_checked).not.toHaveBeenCalled();
    expect(references[0]).toContain('18446744073709551615');
    expect(references[0]).not.toContain(requestId);
    noBusiness(s, ['remove_member_checked']);
  });

  it('Unknown incoming decision survives error dismissal and fresh read; another queue item is never selected or answered', async () => {
    const s = await setup('/');
    const decision = deferred<void>();
    vi.mocked(s.manager.accept_recover).mockReturnValueOnce(decision.promise);
    await s.wrapper.get('.notification-hit').trigger('click');
    await s.wrapper.get('[data-testid="approve-recover-request"]').trigger('click');
    await settle();
    expect(s.manager.accept_recover).toHaveBeenCalledOnce();
    s.controller.showError('Current error');
    await settle();
    const syncCount = vi.mocked(s.manager.sync_now).mock.calls.length;
    await dismiss(s);
    expect.soft(s.manager.sync_now).toHaveBeenCalledTimes(syncCount + 1);
    decision.reject(new Error('unknown-decision'));
    await settle();
    retainedQueue(s);
    expect(s.controller.notifications.state.value.busy).toBe(true);
    expect(s.controller.notifications.state.value.pendingDecision).toMatchObject({ requestId: ids[0], accepted: true });
    expect(s.manager.accept_recover).toHaveBeenCalledOnce();
    expect(s.manager.decline_recover).not.toHaveBeenCalled();
    noBusiness(s, ['accept_recover']);
  });

  it('auth cancellation stays a non-decision after error resume and never becomes Decline', async () => {
    const s = await setup('/');
    s.controller.showError('Current error');
    await settle();
    await dismiss(s);
    expect.soft(s.session.state.value.ready).toBe(true);
    if (!s.session.state.value.ready) {
      s.stage('UNREACHED-auth-cancel-after-missing-manual-read');
      return;
    }
    s.auth.authenticateWithPasskey.mockResolvedValueOnce(false);
    await s.wrapper.get('.notification-hit').trigger('click');
    await s.wrapper.get('[data-testid="decline-recover-request"]').trigger('click');
    await settle();
    expect(s.auth.authenticateWithPasskey).toHaveBeenCalledOnce();
    expect(s.controller.notifications.state.value.busy).toBe(false);
    retainedQueue(s);
    noBusiness(s);
  });
});
