import { describe, expect, it, vi } from 'vitest';
import { createCoreAdapter } from '@/features/liveboard/coreAdapter';
import {
  parseOperationReceipt,
  parseProducerReceipt,
  producerReferenceJson,
} from '@/features/liveboard/coreOperations';
import { createNotificationController } from '@/features/liveboard/notificationController';
import {
  checked,
  contextFixture,
  coreState,
  deferred,
  operation,
  platformFixture,
  producer,
  requestId,
} from './liveboardCoreFixtures';
import type { RequestKind } from '@/features/liveboard/types';

const kinds: RequestKind[] = ['join', 'recover', 'delete-secret', 'remove-device'];
const coreKind = { join: 'join', recover: 'recover', 'delete-secret': 'deleteSecret', 'remove-device': 'removeDevice' };
function setup(kind: RequestKind = 'recover') {
  let context = contextFixture(
    JSON.stringify(
      coreState({
        operations: [
          operation({
            kind: coreKind[kind],
            requestId: kind === 'recover' ? 'claim-second' : requestId,
            targetDeviceId: 'peer-a',
          }),
        ],
      }),
    ),
  );
  const platform = platformFixture();
  const adapter = createCoreAdapter({ getContext: () => context, platform });
  const controller = createNotificationController(adapter.notifications);
  const refresh = () => {
    const projected = adapter.project();
    controller.reconcile({ session: projected.session, requests: projected.requests });
  };
  refresh();
  controller.open();
  return {
    adapter,
    controller,
    platform,
    refresh,
    get context() {
      return context;
    },
    replace: (next: typeof context) => {
      context = next;
    },
  };
}

describe('Liveboard exact incoming operations through actual typed manager ports', () => {
  it.each(kinds.flatMap((kind) => [true, false].map((accepted) => ({ kind, accepted }))))(
    'authenticates $kind accepted=$accepted and submits only that exact operation',
    async ({ kind, accepted }) => {
      const s = setup(kind);
      await s.controller.decide(accepted);
      expect(s.platform.authenticate).toHaveBeenCalledOnce();
      const m = s.context.manager;
      if (kind === 'join')
        expect(m.decide_join_request).toHaveBeenCalledExactlyOnceWith(JSON.stringify({ requestId, accepted }));
      if (kind === 'remove-device')
        expect(m.decide_remove_member).toHaveBeenCalledExactlyOnceWith(JSON.stringify({ requestId, accepted }));
      if (kind === 'delete-secret') expect(m.decide_delete_secret).toHaveBeenCalledExactlyOnceWith(requestId, accepted);
      if (kind === 'recover') {
        expect(s.context.member.recovery_claim_by_id).toHaveBeenCalledWith('claim-second');
        const resolve = vi.mocked(s.context.member.recovery_claim_by_id);
        const exact = resolve.mock.results.at(-1)?.value;
        expect(accepted ? m.accept_recover : m.decline_recover).toHaveBeenCalledExactlyOnceWith(exact);
        expect(exact?.free).toHaveBeenCalledOnce();
      }
      expect(m.update_membership).not.toHaveBeenCalled();
      expect(m.approve_remove_member).not.toHaveBeenCalled();
      expect(m.decline_remove_member).not.toHaveBeenCalled();
      expect(s.context.member.find_pending_incoming_recovery_claim).not.toHaveBeenCalled();
      s.controller.dispose();
    },
  );

  it.each(['false', 'cancel', 'error', 'key-manager', 'epoch', 'removed', 'capability'] as const)(
    'performs zero writes when operation authentication becomes %s',
    async (reason) => {
      const s = setup('join');
      const originalManager = s.context.manager;
      const originalChecked = originalManager.decide_join_request;
      const replacementManager = contextFixture().manager;
      const auth = deferred<boolean>();
      s.platform.authenticate.mockReturnValue(auth.promise);
      const deciding = s.controller.decide(true);
      if (reason === 'key-manager') s.replace({ ...s.context, manager: replacementManager, identity: 'session-b' });
      if (reason === 'epoch') s.replace({ ...s.context, epoch: 2 });
      if (reason === 'removed') {
        vi.mocked(s.context.member.liveboard_state_json).mockReturnValue(JSON.stringify(coreState({ operations: [] })));
        s.refresh();
      }
      if (reason === 'capability')
        Object.defineProperty(s.context.manager, 'decide_join_request', { value: undefined });
      if (reason === 'cancel') auth.reject(new DOMException('private-marker', 'NotAllowedError'));
      else if (reason === 'error') auth.reject(new Error('private-marker'));
      else auth.resolve(reason !== 'false');
      await deciding;
      expect(originalChecked).not.toHaveBeenCalled();
      expect(originalManager.update_membership).not.toHaveBeenCalled();
      for (const write of [
        replacementManager.decide_join_request,
        replacementManager.update_membership,
        replacementManager.cluster_distribution_checked,
        replacementManager.recover_checked,
        replacementManager.delete_secret_checked,
        replacementManager.remove_member_checked,
        replacementManager.decide_remove_member,
        replacementManager.decide_delete_secret,
        replacementManager.accept_recover,
        replacementManager.decline_recover,
      ])
        expect(write).not.toHaveBeenCalled();
      expect(JSON.stringify(s.controller.state.value)).not.toContain('private-marker');
      s.controller.dispose();
    },
  );

  it('keeps a submitted Unknown disabled across reconciliation and repeated clicks', async () => {
    const s = setup('join');
    vi.mocked(s.context.manager.decide_join_request).mockResolvedValue(
      checked({ kind: 'join', requestId, secretId: undefined, claimId: undefined, phase: 'unknown' }),
    );
    await s.controller.decide(true);
    s.refresh();
    await s.controller.decide(true);
    expect(s.context.manager.decide_join_request).toHaveBeenCalledOnce();
    expect(s.platform.authenticate).toHaveBeenCalledOnce();
    s.controller.dispose();
  });

  it.each(kinds.flatMap((kind) => [true, false].map((firstAccepted) => ({ kind, firstAccepted }))))(
    'admits $kind only once with opposing pre-issued tickets, first accepted=$firstAccepted',
    async ({ kind, firstAccepted }) => {
      const s = setup(kind);
      const m = s.context.manager;
      const exactId = kind === 'recover' ? 'claim-second' : requestId;
      const firstIntent = { kind, requestId: exactId, accepted: firstAccepted };
      const oppositeIntent = { ...firstIntent, accepted: !firstAccepted };
      const checkedCall = deferred<string>();
      const voidCall = deferred<void>();
      const unknownReceipt = checked({
        kind: coreKind[kind],
        requestId: exactId,
        secretId: undefined,
        claimId: undefined,
        phase: 'unknown',
      });
      const selectedRecover = firstAccepted ? m.accept_recover : m.decline_recover;
      if (kind === 'join')
        vi.mocked(m.decide_join_request).mockReturnValueOnce(checkedCall.promise).mockResolvedValue(unknownReceipt);
      if (kind === 'remove-device')
        vi.mocked(m.decide_remove_member).mockReturnValueOnce(checkedCall.promise).mockResolvedValue(unknownReceipt);
      if (kind === 'delete-secret')
        vi.mocked(m.decide_delete_secret).mockReturnValueOnce(voidCall.promise).mockResolvedValue();
      if (kind === 'recover') vi.mocked(selectedRecover).mockReturnValueOnce(voidCall.promise).mockResolvedValue();
      const mutations = [
        m.decide_join_request,
        m.decide_remove_member,
        m.decide_delete_secret,
        m.accept_recover,
        m.decline_recover,
      ];
      const mutationCount = () => mutations.reduce((total, method) => total + vi.mocked(method).mock.calls.length, 0);
      const observe = (pending: ReturnType<typeof s.adapter.notifications.decide>) =>
        pending.then(
          (receipt) => ({ status: 'fulfilled' as const, receipt }),
          () => ({ status: 'rejected' as const }),
        );

      expect(await s.adapter.notifications.authenticate(firstIntent)).toBe(true);
      // A safer implementation may reject this second authentication before issuing its ticket.
      await s.adapter.notifications.authenticate(oppositeIntent);
      const first = observe(s.adapter.notifications.decide(firstIntent));
      await Promise.resolve();
      const ownedClaim = kind === 'recover' ? vi.mocked(selectedRecover).mock.calls[0]?.[0] : undefined;
      const freedWhileHeld = ownedClaim ? vi.mocked(ownedClaim.free).mock.calls.length : 0;
      const oppositeWhileHeld = observe(s.adapter.notifications.decide(oppositeIntent));
      await Promise.resolve();
      await Promise.resolve();
      const writesWhileHeld = mutationCount();

      // Both rejections are observed from creation, before an admitted void call is rejected.
      if (kind === 'join' || kind === 'remove-device') checkedCall.resolve(unknownReceipt);
      else voidCall.reject(new Error('OperationUnavailable'));
      const [firstOutcome] = await Promise.all([first, oppositeWhileHeld]);
      s.refresh();
      const writesAfterUnknown = mutationCount();
      await observe(s.adapter.notifications.decide(oppositeIntent));
      const writesAfterOppositeRetry = mutationCount();
      try {
        expect(writesWhileHeld).toBe(1);
        expect(writesAfterUnknown).toBe(1);
        expect(writesAfterOppositeRetry).toBe(1);
        expect(s.adapter.project().requests.some((item) => item.kind === kind && item.requestId === exactId)).toBe(
          true,
        );
        if (kind === 'join' || kind === 'remove-device') {
          expect(firstOutcome).toMatchObject({
            status: 'fulfilled',
            receipt: { operationId: exactId, phase: 'unknown' },
          });
          expect(kind === 'join' ? m.decide_join_request : m.decide_remove_member).toHaveBeenCalledExactlyOnceWith(
            JSON.stringify({ requestId: exactId, accepted: firstAccepted }),
          );
        } else {
          expect(firstOutcome.status).toBe('rejected');
          if (kind === 'delete-secret')
            expect(m.decide_delete_secret).toHaveBeenCalledExactlyOnceWith(exactId, firstAccepted);
          else {
            expect(ownedClaim?.wasm_id_str()).toBe(exactId);
            expect(selectedRecover).toHaveBeenCalledExactlyOnceWith(ownedClaim);
            expect(firstAccepted ? m.decline_recover : m.accept_recover).not.toHaveBeenCalled();
            expect(freedWhileHeld).toBe(0);
            expect(ownedClaim?.free).toHaveBeenCalledOnce();
          }
        }
        for (const method of [
          m.update_membership,
          m.approve_remove_member,
          m.decline_remove_member,
          m.delete_secret,
          m.remove_member_from_vault,
          m.cluster_distribution,
          m.recover_js,
          m.cluster_distribution_checked,
          m.recover_checked,
          m.delete_secret_checked,
          m.remove_member_checked,
          m.continue_local_remove_checked,
          m.reconcile_producer_receipt,
          m.sync_now,
          m.get_state,
        ])
          expect(method).not.toHaveBeenCalled();
        expect(m.free).not.toHaveBeenCalled();
      } finally {
        s.controller.dispose();
      }
    },
  );
});

describe('Liveboard bare checked receipt and distinct producer parsing', () => {
  it.each([
    { kind: 'create', secretId: 'pass-a' },
    { kind: 'recover', secretId: 'pass-a', claimId: 'claim-second' },
    { kind: 'join', requestId },
    { kind: 'removeDevice', requestId },
  ])('retains actual checked identities for $kind without asserting canonical completion', (receipt) => {
    const parsed = parseOperationReceipt(
      JSON.stringify({ ...receipt, phase: 'unknown', errorCode: 'RecoverCanonicalUnknown' }),
    );
    expect(parsed).toMatchObject({ ...receipt, phase: 'unknown' });
    expect(parsed.phase).not.toBe('confirmed');
  });
  it.each([
    JSON.stringify({ success: true, message: JSON.parse(checked()) }),
    producer(),
    '{"kind":"create","phase":"queued"}',
    'private-marker',
  ])('rejects wrong envelopes, missing actual IDs and raw errors without exposing input', (raw) => {
    expect(() => parseOperationReceipt(raw)).toThrow();
    try {
      parseOperationReceipt(raw);
    } catch (error) {
      expect(String(error)).not.toContain('private-marker');
    }
  });

  it.each(['9007199254740993', '18446744073709551615'])(
    'preserves source sequence %s as exact numeric tokens in the strict permitted reference',
    (curr) => {
      const parsed = parseProducerReceipt(producer({ kind: 'removeDevice', curr, decision: true, request: false }));
      const reference = producerReferenceJson(parsed);
      expect(reference).toContain(`"curr":${curr}`);
      expect(reference).toContain(`"prev":${BigInt(curr) - 1n}`);
      expect(reference).toContain('"curr":9007199254740993');
      expect(reference).toContain('"bodyCommitment":"' + 'a'.repeat(64) + '"');
      expect(reference).toContain('"bodyCommitment":"' + 'b'.repeat(64) + '"');
      const fields = JSON.parse(reference);
      expect(Object.keys(fields).sort()).toEqual(['decisionStage', 'kind', 'requestStage', 'version']);
      expect(Object.keys(fields.requestStage).sort()).toEqual(['bodyCommitment', 'eventId']);
      expect(Object.keys(fields.decisionStage).sort()).toEqual(['bodyCommitment', 'eventId']);
      for (const forbidden of ['observation', 'readiness', 'phase', 'materialStage', 'targetDeviceId'])
        expect(reference).not.toContain(`"${forbidden}"`);
    },
  );
  it('retains the actual Delete UUID and never emits a decision source for Delete', () => {
    const reference = producerReferenceJson(parseProducerReceipt(producer()));
    expect(reference).toContain(`"requestId":"${requestId}"`);
    expect(reference).not.toContain('decisionStage');
  });
  it.each([checked(), '{"version":2}', '{"version":1,"kind":"removeDevice"}'])(
    'rejects ordinary receipts or malformed producer records',
    (raw) => expect(() => parseProducerReceipt(raw)).toThrow(),
  );
});
