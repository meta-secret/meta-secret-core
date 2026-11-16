import type { CheckedReceipt, CoreAdapter, CoreContext, CorePlatform } from './coreTypes';
import type { CreateReceipt, DecisionIntent, OperationReceipt, RevealedValue } from './types';
import { blockedProjection, readCoreProjection } from './coreProjection';
import { parseOperationReceipt } from './coreOperations';
import { createProducerRegistry, sameBinding } from './coreProducer';
import { invalid } from './coreValidation';
import { withOwnedClaim, withOwnedDevice, withOwnedPass, withOwnedPlainPass } from './wasmOwnership';
import type { RecoveredRevealInput } from './recoveredReveal';
import type { RecoveredRevealResult } from './types';

type Ticket = { context: CoreContext; valid(): boolean };
const decisionKey = (intent: DecisionIntent) => JSON.stringify([intent.kind, intent.requestId, intent.accepted]);
const requestKey = (intent: DecisionIntent) => JSON.stringify([intent.kind, intent.requestId]);
const value = (text: string): RevealedValue => {
  const words = text.trim().split(/\s+/u);
  return words.length === 12 || words.length === 24 ? { kind: 'seed', words } : { kind: 'text', text };
};
const presentation = (receipt: CheckedReceipt): { operationId: string; phase: 'submitted' | 'unknown' } => ({
  operationId: receipt.claimId ?? receipt.requestId ?? receipt.secretId!,
  phase: receipt.phase === 'queued' ? 'submitted' : 'unknown',
});

export function createCoreAdapter({
  getContext,
  platform,
  handoffRecovered,
}: {
  getContext(): CoreContext | null;
  platform: CorePlatform;
  handoffRecovered?(input: RecoveredRevealInput): Promise<RecoveredRevealResult>;
}): CoreAdapter {
  const tickets = new Map<string, Ticket>();
  const authenticating = new Set<string>();
  const incoming = new Map<string, { context: CoreContext; receipt: OperationReceipt }>();
  const recoveries = new Map<string, { context: CoreContext; receipt: CheckedReceipt }>();
  const recovering = new Map<string, CoreContext>();
  const uncertainRecovery = new Map<string, CoreContext>();
  const producers = createProducerRegistry({ getContext });
  let creation: { context: CoreContext; receipt: CreateReceipt } | null = null;
  let creating: CoreContext | null = null;
  let uncertainCreate: CoreContext | null = null;
  function project() {
    const context = getContext();
    if (!context) return blockedProjection();
    const projection = readCoreProjection({
      member: context.member,
      identity: context.identity,
    });
    projection.session.ready &&= context.ready;
    projection.canCreateSecret &&= projection.session.ready;
    producers.reconcile(projection);
    for (const [id, entry] of recoveries) {
      const op = projection.operations.find((op) => op.kind === 'recover' && op.requestId === entry.receipt.claimId);
      if (!sameBinding(entry.context, context, false) || op?.clientStatus === 'declined' || op?.clientStatus === 'done')
        recoveries.delete(id);
    }
    return projection;
  }
  function active() {
    const context = getContext();
    if (!context?.ready || !project().session.ready) throw invalid();
    return { ...context };
  }
  function actionable(intent: DecisionIntent) {
    return project().requests.some(
      (request) =>
        request.kind === intent.kind &&
        request.requestId === intent.requestId &&
        (intent.accepted ? request.canApprove : request.canDecline),
    );
  }
  async function authenticate(key: string, valid: () => boolean, lockKey = key) {
    const context = active();
    if (authenticating.has(lockKey) || !valid()) return false;
    authenticating.add(lockKey);
    tickets.delete(key);
    try {
      if (!(await platform.authenticate()) || !sameBinding(context, getContext()) || !getContext()?.ready || !valid())
        return false;
      tickets.set(key, { context, valid });
      return true;
    } catch (error) {
      if (error instanceof DOMException && ['NotAllowedError', 'AbortError'].includes(error.name)) return false;
      throw invalid();
    } finally {
      authenticating.delete(lockKey);
    }
  }
  function authorized(key: string) {
    const ticket = tickets.get(key);
    tickets.delete(key);
    if (!ticket || !sameBinding(ticket.context, getContext()) || !getContext()?.ready || !ticket.valid())
      throw invalid();
    return active();
  }
  function decisionCapability(context: CoreContext, intent: DecisionIntent) {
    const manager = context.manager;
    return (
      typeof (intent.kind === 'join'
        ? manager.decide_join_request
        : intent.kind === 'remove-device'
          ? manager.decide_remove_member
          : intent.kind === 'delete-secret'
            ? manager.decide_delete_secret
            : intent.accepted
              ? manager.accept_recover
              : manager.decline_recover) === 'function'
    );
  }
  const notifications: CoreAdapter['notifications'] = {
    authenticate: (intent) => {
      const key = decisionKey(intent);
      const previous = incoming.get(requestKey(intent));
      if (previous && sameBinding(previous.context, getContext(), false)) return Promise.resolve(false);
      return authenticate(
        key,
        () => actionable(intent) && Boolean(getContext() && decisionCapability(getContext()!, intent)),
        requestKey(intent),
      );
    },
    async decide(intent) {
      const key = decisionKey(intent),
        context = authorized(key);
      const request = requestKey(intent),
        previous = incoming.get(request);
      if (previous && sameBinding(previous.context, context, false)) throw invalid();
      if (!decisionCapability(context, intent)) throw invalid();
      // Both pre-issued choices lose their ticket before the first borrowed Core await.
      for (const accepted of [true, false]) tickets.delete(decisionKey({ ...intent, accepted }));
      const receipt: OperationReceipt = { operationId: intent.requestId, phase: 'unknown' };
      incoming.set(request, { context, receipt });
      const manager = context.manager;
      if (intent.kind === 'recover')
        await withOwnedClaim({ member: context.member, id: intent.requestId }, (claim) =>
          intent.accepted ? manager.accept_recover(claim) : manager.decline_recover(claim),
        );
      else if (intent.kind === 'delete-secret') await manager.decide_delete_secret(intent.requestId, intent.accepted);
      else {
        const json = JSON.stringify({ requestId: intent.requestId, accepted: intent.accepted });
        const checked = parseOperationReceipt(
          await (intent.kind === 'join' ? manager.decide_join_request(json) : manager.decide_remove_member(json)),
        );
        if (
          checked.requestId !== intent.requestId ||
          checked.kind !== (intent.kind === 'join' ? 'join' : 'removeDevice')
        )
          throw invalid();
        receipt.phase = presentation(checked).phase;
      }
      if (intent.kind === 'recover' || intent.kind === 'delete-secret') receipt.phase = 'submitted';
      return { ...receipt };
    },
  };
  function secret(id: string) {
    return project().secrets.find((secret) => secret.passId === id);
  }
  const secrets: CoreAdapter['secrets'] = {
    async showLocal(passId) {
      const context = active();
      if (secret(passId)?.primary.kind !== 'show-local' || typeof context.manager.show_local_secret !== 'function')
        throw invalid();
      const shown = await withOwnedPass({ member: context.member, id: passId }, (pass) =>
        context.manager.show_local_secret(pass),
      );
      if (!sameBinding(context, getContext()) || !getContext()?.ready || secret(passId)?.primary.kind !== 'show-local')
        throw invalid();
      return value(shown);
    },
    async showRecovered({ passId, claimId }) {
      const context = active();
      const eligible = () => {
        const action = secret(passId)?.primary;
        return action?.kind === 'show-recovered' && action.claimId === claimId;
      };
      if (!eligible() || typeof context.manager.show_recovered !== 'function') throw invalid();
      let called = false;
      const retrieveOnce = () => {
        if (called) return Promise.reject(invalid());
        called = true;
        return withOwnedClaim({ member: context.member, id: claimId }, (claim) =>
          context.manager.show_recovered(claim),
        );
      };
      if (handoffRecovered)
        return handoffRecovered({ intent: { passId, claimId }, binding: context, retrieveOnce, decode: value });
      const shown = await retrieveOnce();
      if (!sameBinding(context, getContext()) || !getContext()?.ready || !eligible()) throw invalid();
      return value(shown);
    },
    async recover(passId) {
      const context = active(),
        previous = recoveries.get(passId);
      if (previous && sameBinding(previous.context, context, false)) return presentation(previous.receipt);
      const pending = recovering.get(passId),
        unknown = uncertainRecovery.get(passId);
      if ((pending && sameBinding(pending, context, false)) || (unknown && sameBinding(unknown, context, false)))
        throw invalid();
      if (secret(passId)?.primary.kind !== 'recover' || typeof context.manager.recover_checked !== 'function')
        throw invalid();
      recovering.set(passId, context);
      let admitted = false;
      try {
        const receipt = parseOperationReceipt(
          await withOwnedPass({ member: context.member, id: passId }, (pass) => {
            admitted = true;
            return context.manager.recover_checked(pass);
          }),
        );
        if (receipt.kind !== 'recover' || receipt.secretId !== passId || !sameBinding(context, getContext(), false))
          throw invalid();
        recoveries.set(passId, { context, receipt });
        return presentation(receipt);
      } catch {
        if (admitted && sameBinding(context, getContext(), false)) uncertainRecovery.set(passId, context);
        throw invalid();
      } finally {
        if (recovering.get(passId) === context) recovering.delete(passId);
      }
    },
    authenticate: ({ passId }) => authenticate(`delete:${passId}`, () => Boolean(secret(passId)?.canDelete)),
    deleteSecret(passId) {
      const context = authorized(`delete:${passId}`);
      if (typeof context.manager.delete_secret_checked !== 'function') throw invalid();
      return producers.submit({
        context,
        kind: 'deleteSecret',
        subject: passId,
        action: (admitted) =>
          withOwnedPass({ member: context.member, id: passId }, (pass) => {
            admitted();
            return context.manager.delete_secret_checked(pass);
          }),
      });
    },
    copy: (text) => platform.copy(text),
    reportError: (message) => platform.reportError(message),
  };
  const create: CoreAdapter['create'] = {
    async create(draft) {
      const context = active();
      if (creation && sameBinding(creation.context, context, false)) return creation.receipt;
      if (
        (creating && sameBinding(creating, context, false)) ||
        (uncertainCreate && sameBinding(uncertainCreate, context, false)) ||
        !project().canCreateSecret ||
        typeof context.manager.cluster_distribution_checked !== 'function'
      )
        throw invalid();
      creating = context;
      let admitted = false;
      try {
        const text =
          draft.mode === 'seed'
            ? draft.words
                .slice(0, draft.wordCount)
                .map((word) => word.trim())
                .join(' ')
            : draft.value.trim();
        if (!draft.name.trim() || !text) throw invalid();
        const checked = parseOperationReceipt(
          await withOwnedPlainPass({ name: draft.name.trim(), text }, (input) => {
            admitted = true;
            return context.manager.cluster_distribution_checked(input);
          }),
        );
        if (checked.kind !== 'create' || !sameBinding(context, getContext(), false)) throw invalid();
        const receipt = presentation(checked);
        creation = { context, receipt };
        return receipt;
      } catch {
        if (admitted && sameBinding(context, getContext(), false)) uncertainCreate = context;
        throw invalid();
      } finally {
        if (creating === context) creating = null;
      }
    },
    async reconcile(id) {
      const context = active();
      if (!creation || creation.receipt.operationId !== id || !sameBinding(creation.context, context, false))
        throw invalid();
      const current = secret(id);
      if (current && !['blocked', 'waiting'].includes(current.primary.kind)) {
        creation = null;
        return { operationId: id, phase: 'confirmed', passId: id };
      }
      return creation.receipt;
    },
    readClipboard: () => platform.readClipboard(),
    reportError: (message) => platform.reportError(message),
  };
  const removable = (id: string) =>
    project().devices.some((device) => device.deviceId === id && !device.current && device.canRemove);
  const devices: CoreAdapter['devices'] = {
    authenticate: (id) => authenticate(`remove:${id}`, () => removable(id)),
    remove(id) {
      const context = authorized(`remove:${id}`);
      if (typeof context.manager.remove_member_checked !== 'function') throw invalid();
      return producers.submit({
        context,
        kind: 'removeDevice',
        subject: id,
        action: (admitted) =>
          withOwnedDevice({ member: context.member, id }, (device) => {
            admitted();
            return context.manager.remove_member_checked(device);
          }),
      });
    },
    reportError: (message) => platform.reportError(message),
  };
  return {
    project,
    notifications,
    secrets,
    create,
    devices,
    reconcileProducer: producers.reconcileProducer,
    continueRemove: producers.continueRemove,
  };
}
