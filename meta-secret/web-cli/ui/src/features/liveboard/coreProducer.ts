import type { CoreContext, CoreProjection, ProducerReceipt, ProducerSource } from './coreTypes';
import { parseProducerReceipt, producerReferenceJson } from './coreOperations';
import { invalid } from './coreValidation';
import type { OperationReceipt } from './types';

export function sameBinding(left: CoreContext, right: CoreContext | null, includeEpoch = true) {
  return Boolean(
    right &&
    left.identity === right.identity &&
    left.manager === right.manager &&
    (!includeEpoch || left.epoch === right.epoch),
  );
}
function sameSource(left: ProducerSource, right: ProducerSource) {
  return (
    left.bodyCommitment === right.bodyCommitment &&
    left.eventId.id.curr === right.eventId.id.curr &&
    left.eventId.id.prev === right.eventId.id.prev &&
    left.eventId.fqdn.objType === right.eventId.fqdn.objType &&
    left.eventId.fqdn.objInstance === right.eventId.fqdn.objInstance
  );
}
const operationId = (receipt: ProducerReceipt) => {
  const { fqdn, id } = receipt.requestStage.source.eventId;
  return `${fqdn.objType}:${fqdn.objInstance}::${id.curr}`;
};
type Entry = { context: CoreContext; receipt: ProducerReceipt; subject: string };

/** Retains only actual producer correlation metadata; no consent or plaintext. */
export function createProducerRegistry({ getContext }: { getContext(): CoreContext | null }) {
  const entries = new Map<string, Entry>();
  const locks = new Map<string, string>();
  const inFlight = new Map<string, { context: CoreContext }>();
  const uncertain = new Map<string, CoreContext>();
  function presentation(receipt: ProducerReceipt): OperationReceipt {
    return { operationId: operationId(receipt), phase: receipt.phase === 'queued' ? 'submitted' : 'unknown' };
  }
  function clearOtherSession() {
    for (const [id, entry] of entries) if (!sameBinding(entry.context, getContext(), false)) entries.delete(id);
    for (const [key, id] of locks) if (!entries.has(id)) locks.delete(key);
    for (const [key, context] of uncertain) if (!sameBinding(context, getContext(), false)) uncertain.delete(key);
    for (const [key, owner] of inFlight) if (!sameBinding(owner.context, getContext(), false)) inFlight.delete(key);
  }
  async function submit(input: {
    context: CoreContext;
    kind: ProducerReceipt['kind'];
    subject: string;
    action(admitted: () => void): Promise<string>;
  }): Promise<OperationReceipt> {
    clearOtherSession();
    const key = JSON.stringify([input.kind, input.subject]);
    const previous = entries.get(locks.get(key) ?? '');
    if (previous) return presentation(previous.receipt);
    if (inFlight.has(key) || uncertain.has(key)) throw invalid();
    const owner = { context: input.context };
    inFlight.set(key, owner);
    let admitted = false;
    try {
      const receipt = parseProducerReceipt(
        await input.action(() => {
          admitted = true;
        }),
      );
      if (
        receipt.kind !== input.kind ||
        (input.kind === 'deleteSecret' ? receipt.secretId : receipt.targetDeviceId) !== input.subject
      )
        throw invalid();
      if (!sameBinding(input.context, getContext(), false)) throw invalid();
      const id = operationId(receipt),
        old = entries.get(id);
      if (old && !sameSource(old.receipt.requestStage.source, receipt.requestStage.source)) throw invalid();
      entries.set(id, { context: input.context, receipt, subject: input.subject });
      locks.set(key, id);
      return presentation(receipt);
    } catch {
      if (admitted && sameBinding(input.context, getContext(), false)) uncertain.set(key, input.context);
      throw invalid();
    } finally {
      if (inFlight.get(key) === owner) inFlight.delete(key);
    }
  }
  async function observe(id: string, continuing = false): Promise<ProducerReceipt> {
    clearOtherSession();
    const entry = entries.get(id),
      current = getContext();
    const context = current ? { ...current } : null;
    if (!entry || !context || !sameBinding(entry.context, context, false)) throw invalid();
    if (continuing && (entry.receipt.kind !== 'removeDevice' || !context.ready)) throw invalid();
    const method = continuing
      ? context.manager.continue_local_remove_checked
      : context.manager.reconcile_producer_receipt;
    if (typeof method !== 'function') throw invalid();
    const result = parseProducerReceipt(await method.call(context.manager, producerReferenceJson(entry.receipt)));
    const previous = entry.receipt;
    if (
      !sameBinding(context, getContext(), false) ||
      result.kind !== previous.kind ||
      result.secretId !== previous.secretId ||
      result.targetDeviceId !== previous.targetDeviceId ||
      !sameSource(result.requestStage.source, previous.requestStage.source) ||
      (previous.requestId && result.requestId !== previous.requestId)
    )
      throw invalid();
    if (
      previous.decisionStage?.state === 'attempted' &&
      (result.decisionStage?.state !== 'attempted' ||
        !sameSource(previous.decisionStage.source, result.decisionStage.source))
    )
      throw invalid();
    entry.receipt = result;
    return result;
  }
  function reconcile(snapshot: CoreProjection) {
    clearOtherSession();
    if (!snapshot.session.ready) return;
    for (const [key, id] of locks) {
      const entry = entries.get(id);
      if (!entry?.receipt.requestId) continue;
      const op = snapshot.operations.find(
        (op) => op.kind === entry.receipt.kind && op.requestId === entry.receipt.requestId,
      );
      // A disappearance, target count or a producer phase cannot release a write lock.
      if (op?.outcome === 'declined' || (op?.outcome === 'accepted' && ['done', 'settled'].includes(op.readiness)))
        locks.delete(key);
    }
  }
  return {
    submit,
    reconcile,
    reconcileProducer: (id: string) => observe(id),
    continueRemove: (id: string) => observe(id, true),
  };
}
