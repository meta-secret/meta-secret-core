import { reactive, readonly, ref, shallowReactive } from 'vue';
import type { CoreAdapter, CoreOperation, CoreProjection, ProducerReceipt } from './coreTypes';
import type { OperationReceipt } from './types';
import { completedRemoval, currentDeviceId, provenResponse, resolvedRemoval } from './controllerPresentation';

type Producer = {
  operationId: string;
  kind: ProducerReceipt['kind'];
  subject: string;
  requestId?: string;
  previousTerminal?: string;
  previousTerminals: ReadonlyMap<string, string>;
};
export type SettledMutation = { passId: string; kind: 'recover' | 'delete' };
const key = (operation: CoreOperation) => JSON.stringify([operation.kind, operation.requestId]);
const terminal = (operation: CoreOperation) =>
  operation.outcome ? JSON.stringify([operation.kind, operation.requestId, operation.outcome]) : undefined;

/** Metadata-only observation. Never submits, continues, synchronizes or invents an identity. */
export function createControllerOperations(adapter: CoreAdapter) {
  const unread = ref<Record<string, 'green' | 'red'>>(Object.create(null));
  const consumed = reactive(new Set<string>());
  const observed = shallowReactive(new Map<string, CoreOperation>());
  const removalKeys = reactive(new Set<string>());
  const producers = new Map<string, Producer>();
  const admitted = new Set<string>();
  const recoveries = new Map<string, { claimId: string; previousTerminal?: string }>();
  let revision = 0;
  let session = '';
  let requester: string | undefined;
  let current: CoreProjection | null = null;
  const sameRemoval = (left: CoreOperation, right: CoreOperation) =>
    left.kind === 'removeDevice' &&
    right.kind === 'removeDevice' &&
    key(left) === key(right) &&
    left.requesterId === right.requesterId &&
    Boolean(left.targetDeviceId) &&
    left.targetDeviceId === right.targetDeviceId;
  function currentRemoval(snapshot: CoreProjection, remembered: CoreOperation) {
    const matches = snapshot.operations.filter((operation) => key(operation) === key(remembered));
    const operation = matches.length === 1 ? matches[0] : undefined;
    return operation &&
      snapshot.session.member &&
      operation.requesterId === currentDeviceId(snapshot) &&
      sameRemoval(remembered, operation)
      ? operation
      : undefined;
  }
  function retireRemoveDisplay(input: { subject: string; previousTerminals: ReadonlyMap<string, string> }) {
    if (!current?.session.member) return;
    for (const remembered of observed.values()) {
      const operation = currentRemoval(current, remembered);
      if (!operation || operation.targetDeviceId !== input.subject || operation.outcome !== 'declined') continue;
      const marker = terminal(operation);
      if (resolvedRemoval(operation) && marker && input.previousTerminals.get(key(operation)) === marker)
        consumed.add(key(operation));
    }
  }
  function retireDeleteDisplay(input: { subject: string; previousTerminals: ReadonlyMap<string, string> }) {
    if (!requester) return;
    for (const operation of observed.values()) {
      const marker = terminal(operation);
      const settled =
        (operation.outcome === 'declined' && operation.clientStatus === 'declined') ||
        (operation.outcome === 'accepted' && ['done', 'settled'].includes(operation.readiness));
      if (
        operation.kind === 'deleteSecret' &&
        operation.requesterId === requester &&
        operation.secretId === input.subject &&
        settled &&
        marker &&
        input.previousTerminals.get(key(operation)) === marker
      )
        consumed.add(key(operation));
    }
  }
  async function observe(entry: Producer) {
    const token = revision;
    try {
      const receipt = await adapter.reconcileProducer(entry.operationId);
      if (token === revision && producers.get(entry.operationId) === entry) {
        entry.requestId = receipt.requestId;
        if (entry.requestId)
          entry.previousTerminal = entry.previousTerminals.get(JSON.stringify([entry.kind, entry.requestId]));
      }
    } catch {
      // Unknown retains the original source and cannot release or replace its command.
    }
  }
  async function producer(input: {
    kind: Producer['kind'];
    subject: string;
    receipt: OperationReceipt;
    previousTerminals: ReadonlyMap<string, string>;
  }) {
    if (input.receipt.phase === 'rejected') return input.receipt;
    const entry: Producer = producers.get(input.receipt.operationId) ?? {
      kind: input.kind,
      subject: input.subject,
      operationId: input.receipt.operationId,
      previousTerminals: input.previousTerminals,
    };
    producers.set(entry.operationId, entry);
    if (!admitted.has(entry.operationId)) {
      admitted.add(entry.operationId);
      if (entry.kind === 'deleteSecret') retireDeleteDisplay(input);
      if (entry.kind === 'removeDevice') retireRemoveDisplay(input);
    }
    await observe(entry);
    return input.receipt;
  }
  function reconcile(snapshot: CoreProjection): SettledMutation[] {
    const identity = JSON.stringify([snapshot.session.identity, snapshot.session.vaultId, snapshot.session.member]);
    if (session && identity !== session) clear();
    session = identity;
    current = snapshot;
    if (!snapshot.session.member || !snapshot.session.ready) return [];
    const settled: SettledMutation[] = [];
    const ownId = currentDeviceId(snapshot);
    requester = ownId;
    for (const operation of snapshot.operations) {
      const remembered = observed.get(key(operation));
      if (operation.kind === 'removeDevice' && remembered && !sameRemoval(remembered, operation)) continue;
      observed.set(key(operation), { ...operation, eligiblePeerIds: [...operation.eligiblePeerIds] });
      if (operation.kind === 'removeDevice') removalKeys.add(key(operation));
      if (completedRemoval(snapshot, operation)) consumed.add(key(operation));
      if (
        operation.kind !== 'recover' ||
        operation.requesterId !== ownId ||
        !provenResponse(operation) ||
        consumed.has(key(operation))
      )
        continue;
      const secret = snapshot.secrets.find((item) => item.passId === operation.secretId);
      if (
        operation.outcome === 'accepted' &&
        secret?.primary.kind === 'show-recovered' &&
        secret.primary.claimId === operation.requestId
      )
        unread.value[operation.requestId] = 'green';
      if (operation.outcome === 'declined') unread.value[operation.requestId] = 'red';
    }
    for (const [passId, entry] of recoveries) {
      const operation = snapshot.operations.find(
        (item) =>
          item.kind === 'recover' &&
          item.requestId === entry.claimId &&
          item.secretId === passId &&
          item.requesterId === ownId,
      );
      const secret = snapshot.secrets.find((item) => item.passId === passId);
      const declined = operation?.clientStatus === 'declined' && operation.outcome === 'declined';
      const done = operation?.clientStatus === 'done' && secret?.primary.kind === 'recover';
      if (
        operation &&
        (!entry.previousTerminal || terminal(operation) !== entry.previousTerminal) &&
        (declined || done)
      ) {
        settled.push({ passId, kind: 'recover' });
        recoveries.delete(passId);
      }
    }
    for (const [id, entry] of producers) {
      if (!entry.requestId) continue;
      const operation = snapshot.operations.find(
        (item) =>
          item.kind === entry.kind &&
          item.requestId === entry.requestId &&
          item.requesterId === ownId &&
          (entry.kind === 'deleteSecret' ? item.secretId : item.targetDeviceId) === entry.subject,
      );
      if (operation && entry.previousTerminal && terminal(operation) === entry.previousTerminal) continue;
      if (entry.kind === 'removeDevice' && (!operation || !resolvedRemoval(operation))) continue;
      if (operation?.outcome === 'declined') {
        if (entry.kind === 'deleteSecret') settled.push({ passId: entry.subject, kind: 'delete' });
        producers.delete(id);
      } else if (operation?.outcome === 'accepted' && ['done', 'settled'].includes(operation.readiness))
        producers.delete(id);
    }
    return settled;
  }
  function consume(claimId: string) {
    const operation = observed.get(JSON.stringify(['recover', claimId]));
    if (!operation) return;
    consumed.add(key(operation));
    delete unread.value[claimId];
  }
  function consumeDeclines(passId: string) {
    for (const operation of observed.values())
      if (operation.kind === 'recover' && operation.secretId === passId && unread.value[operation.requestId] === 'red')
        consume(operation.requestId);
  }
  function clear() {
    revision++;
    session = '';
    requester = undefined;
    current = null;
    unread.value = Object.create(null);
    consumed.clear();
    removalKeys.clear();
    observed.clear();
    producers.clear();
    admitted.clear();
    recoveries.clear();
  }
  return {
    unread: readonly(unread),
    producer,
    captureTerminals: () =>
      new Map(
        [...observed].flatMap(([identity, operation]) => {
          const marker = terminal(operation);
          return marker ? [[identity, marker] as const] : [];
        }),
      ),
    recovered(input: { passId: string; receipt: OperationReceipt; previousTerminals: ReadonlyMap<string, string> }) {
      const { passId, receipt } = input;
      if (receipt.phase !== 'rejected') {
        recoveries.set(passId, {
          claimId: receipt.operationId,
          previousTerminal: input.previousTerminals.get(JSON.stringify(['recover', receipt.operationId])),
        });
        consumeDeclines(passId);
      }
      return receipt;
    },
    reconcile,
    observe: () => Promise.all([...producers.values()].map(observe)),
    consume,
    acknowledgeRemoval(input: { snapshot: CoreProjection; requestId: string; targetId: string }) {
      const remembered = observed.get(JSON.stringify(['removeDevice', input.requestId]));
      const operation = remembered && currentRemoval(input.snapshot, remembered);
      if (
        input.snapshot.session.ready &&
        operation?.targetDeviceId === input.targetId &&
        operation.outcome === 'declined' &&
        resolvedRemoval(operation)
      )
        consumed.add(key(operation));
    },
    visibleOperation(snapshot: CoreProjection) {
      const knownRemovals = new Set(removalKeys);
      return [...observed.values()]
        .flatMap((remembered) => {
          const operation =
            remembered.kind === 'removeDevice'
              ? knownRemovals.has(key(remembered))
                ? currentRemoval(snapshot, remembered)
                : undefined
              : remembered;
          return operation ? [operation] : [];
        })
        .find(
          (operation) =>
            operation.requesterId === currentDeviceId(snapshot) &&
            snapshot.operations.some((item) => key(item) === key(operation)) &&
            !consumed.has(key(operation)) &&
            (operation.kind === 'removeDevice'
              ? !completedRemoval(snapshot, operation)
              : operation.clientStatus !== 'done') &&
            operation.kind !== 'create' &&
            operation.kind !== 'join',
        );
    },
    removal: () => [...producers.values()].find((entry) => entry.kind === 'removeDevice'),
    clear,
  };
}
