import { vi } from 'vitest';
import { accepted, canonical, historical, type RevealSetup } from './liveboardRecoveredRevealFixtures';

export const invalidProofs = [
  'claim',
  'pass',
  'requester',
  'unaccepted',
  'wrong-primary',
  'nonready',
  'new-claim-earlier',
  'new-claim-later',
  'foreign-new-claim',
  'accepted-delete',
  'missing-history',
  'changed-history',
  'same-id-recreation',
  'unknown-provenance',
  'changed-responder',
] as const;

/** Explicit canonical test inputs; no history matching or eligibility implementation. */
export function invalidProof(reason: (typeof invalidProofs)[number]) {
  const value = canonical(true);
  const done = value.operations[1]!;
  if (reason === 'claim') done.requestId = 'wrong-claim';
  if (reason === 'pass') done.secretId = 'pass-b';
  if (reason === 'requester') done.requesterId = 'peer-a';
  if (reason === 'unaccepted') done.outcome = 'declined';
  if (reason === 'unknown-provenance') done.resolutionSource = 'unknown';
  if (reason === 'changed-responder') done.responderId = 'self';
  if (reason === 'wrong-primary') value.secrets[0]!.primaryAction = 'showLocal';
  if (reason === 'nonready') value.secrets[0]!.readiness = 'synchronizing';
  if (reason.startsWith('new-claim') || reason === 'foreign-new-claim')
    value.operations.push(
      accepted({
        requestId: reason === 'new-claim-earlier' ? '000-earlier-opaque' : 'zzz-later-opaque',
        requesterId: reason === 'foreign-new-claim' ? 'peer-a' : 'self',
      }),
    );
  if (reason === 'accepted-delete' || reason === 'same-id-recreation')
    value.operations.push(
      accepted({
        kind: 'deleteSecret',
        requestId: 'deleted-and-recreated-pass-a',
        clientStatus: 'done',
        resolutionSource: 'canonical',
        readiness: 'settled',
      }),
    );
  if (reason === 'accepted-delete') value.secrets[0]!.deletionRequestId = 'accepted-delete';
  if (reason === 'missing-history') value.operations = [done];
  if (reason === 'changed-history') value.operations[0] = { ...historical, requesterId: 'peer-a' };
  return value;
}

export const boundaries = [
  'route',
  'query-away-back',
  'Hide',
  'Create',
  'modal',
  'offline',
  'pagehide',
  'hidden',
  'auth',
  'key',
  'manager',
  'member',
  'reset',
  'dispose',
] as const;

/** Invoke actual public UI/lifecycle seams, without reimplementing their effects. */
export function hardBoundary(s: RevealSetup, reason: (typeof boundaries)[number]) {
  if (reason === 'route') {
    s.controller.navigate('home');
    s.controller.navigate('secrets');
  }
  if (reason === 'query-away-back') {
    s.controller.search('no match');
    s.controller.search('');
  }
  if (reason === 'Hide') s.controller.secrets.hide('pass-a');
  if (reason === 'Create') s.controller.create.open();
  if (reason === 'modal') s.controller.invite(document.createElement('button'));
  if (reason === 'offline') window.dispatchEvent(new Event('offline'));
  if (reason === 'pagehide') window.dispatchEvent(new Event('pagehide'));
  if (reason === 'hidden') {
    vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('hidden');
    document.dispatchEvent(new Event('visibilitychange'));
  }
  if (reason === 'auth') s.auth.isAuthenticated = false;
  if (reason === 'key') s.auth.masterKey = 'replacement-key';
  if (reason === 'manager') {
    s.store.invalidateSession();
    void s.store.appStateInit().catch(() => undefined);
  }
  if (reason === 'member') {
    s.setMember(false);
    void s.session.refresh({ requiredSync: true }).catch(() => undefined);
  }
  if (reason === 'reset') {
    s.controller.requestReset();
    void s.controller.confirmReset().catch(() => undefined);
  }
  if (reason === 'dispose') s.session.dispose();
}

export function restoreBoundary(s: RevealSetup, reason: (typeof boundaries)[number]) {
  if (reason === 'offline') window.dispatchEvent(new Event('online'));
  if (reason === 'pagehide') window.dispatchEvent(new Event('pageshow'));
  if (reason === 'hidden') {
    vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('visible');
    document.dispatchEvent(new Event('visibilitychange'));
  }
  if (reason === 'auth') s.auth.isAuthenticated = true;
  if (reason === 'member') s.setMember(true);
}
