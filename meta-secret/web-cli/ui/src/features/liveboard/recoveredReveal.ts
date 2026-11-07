import type { StateEventsReadyLease } from '@/utils/stateInvalidation';
import type { CoreContext, CoreOperation, CoreProjection } from './coreTypes';
import type { RecoveredRevealOwner, RecoveredRevealResult, RevealedValue } from './types';

export type RecoveredRevealInput = {
  intent: { passId: string; claimId: string };
  binding: CoreContext;
  retrieveOnce(): Promise<string>;
  decode(text: string): RevealedValue;
};
type ControllerPort = {
  pendingRecoveredReveal(intent: RecoveredRevealInput['intent']): RecoveredRevealOwner | undefined;
  deliverRecoveredReveal(input: { owner: RecoveredRevealOwner; value: RevealedValue }): boolean;
  hide(passId: string): void;
};
type Pending = {
  owner: RecoveredRevealOwner;
  baseline: CoreProjection;
  current(): boolean;
  raw: string | null;
  stage: 'raw' | 'http' | 'sync';
  live: boolean;
  completed: boolean;
  cleanup: Array<() => void>;
};
const invalid = () => new Error('OperationUnavailable');
const identity = (operation: CoreOperation) => JSON.stringify([operation.kind, operation.requestId]);
const binding = (operation: CoreOperation) =>
  JSON.stringify([
    operation.kind,
    operation.requestId,
    operation.secretId,
    operation.requesterId,
    operation.responderId,
    operation.resolutionSource,
    operation.targetDeviceId,
    operation.outcome,
    operation.eligiblePeerIds,
    operation.canApprove,
    operation.canDecline,
  ]);
const rows = (projection: CoreProjection, passId: string) =>
  projection.operations.filter((operation) => operation.secretId === passId);
const own = (projection: CoreProjection, owner: RecoveredRevealOwner) =>
  projection.operations.find(
    (operation) =>
      operation.kind === 'recover' &&
      operation.requestId === owner.claimId &&
      operation.secretId === owner.passId &&
      operation.requesterId === projection.devices.find((device) => device.current)?.deviceId,
  );

type GapPorts = {
  context(): CoreContext | null;
  lifetime(): readonly [active: boolean, key: string | null, generation: number, revision: number];
  restartReady(): Promise<StateEventsReadyLease>;
  sync: { require(): void; refresh(): Promise<void>; ready(): boolean; target(): number };
};
/** Parent metadata only: cancellation of a value owner cannot abandon this obligation. */
async function closeSubscriptionGap(input: GapPorts) {
  const lifetime = input.lifetime();
  const identity = input.context()?.identity,
    manager = input.context()?.manager;
  const current = () => {
    const next = input.lifetime(),
      context = input.context();
    return Boolean(
      identity &&
      manager &&
      next[0] &&
      lifetime.every((value, index) => value === next[index]) &&
      context?.identity === identity &&
      context.manager === manager,
    );
  };
  if (!current()) throw invalid();
  input.sync.require(); // Conceal ALL held values before the no-replay receiver is retired.
  if (!current()) throw invalid();
  const lease = await input.restartReady();
  if (!current() || !lease.isCurrent()) throw invalid();
  input.sync.require();
  const target = input.sync.target();
  if (!current() || !lease.isCurrent()) throw invalid();
  await input.sync.refresh();
  if (!current() || !lease.isCurrent() || input.sync.target() !== target || !input.sync.ready()) throw invalid();
  return lease;
}
/** One unfinished, explicit delivery. No finished value is cached or restored. */
export function createRecoveredReveal(
  input: GapPorts & {
    controller(): ControllerPort;
    project(): CoreProjection;
  },
) {
  const pending = new Set<Pending>();
  function revoke(entry: Pending, conceal = true) {
    entry.raw = null;
    if (!entry.live) return;
    entry.live = false;
    pending.delete(entry);
    entry.cleanup.splice(0).forEach((cleanup) => cleanup());
    if (conceal && input.controller().pendingRecoveredReveal(entry.owner) === entry.owner)
      input.controller().hide(entry.owner.passId);
  }
  function continuous(entry: Pending, next: CoreProjection) {
    const previous = entry.baseline;
    if (
      !entry.current() ||
      !entry.owner.isCurrent() ||
      !next.session.member ||
      previous.session.identity !== next.session.identity ||
      previous.session.vaultId !== next.session.vaultId ||
      previous.devices.find((device) => device.current)?.deviceId !==
        next.devices.find((device) => device.current)?.deviceId ||
      previous.devices.length !== next.devices.length ||
      previous.devices.some((device) => !next.devices.some((current) => current.deviceId === device.deviceId))
    )
      return false;
    const secret = next.secrets.find((item) => item.passId === entry.owner.passId);
    if (
      !secret ||
      secret.deletionRequestId ||
      !['show-recovered', 'recover'].includes(secret.primary.kind) ||
      (secret.primary.kind === 'show-recovered' && secret.primary.claimId !== entry.owner.claimId)
    )
      return false;
    const before = rows(previous, entry.owner.passId),
      after = rows(next, entry.owner.passId);
    if (before.length !== after.length) return false;
    return before.every((operation) => {
      const current = after.find((item) => identity(item) === identity(operation));
      if (!current) return false;
      if (operation.kind !== 'recover' || operation.requestId !== entry.owner.claimId)
        return JSON.stringify(current) === JSON.stringify(operation);
      if (entry.completed && current.clientStatus !== 'done') return false;
      if (current.clientStatus === 'done') entry.completed = true;
      return binding(current) === binding(operation) && ['accepted', 'done'].includes(current.clientStatus);
    });
  }
  function observe(projection: CoreProjection) {
    for (const entry of [...pending]) if (!continuous(entry, projection)) revoke(entry);
  }
  async function handoff(request: RecoveredRevealInput): Promise<RecoveredRevealResult> {
    if (pending.size) throw invalid();
    const owner = input.controller().pendingRecoveredReveal(request.intent);
    const baseline = input.project();
    const original = owner && own(baseline, owner);
    if (
      !owner ||
      !original ||
      original.clientStatus !== 'accepted' ||
      original.outcome !== 'accepted' ||
      !original.responderId ||
      original.resolutionSource === 'unknown'
    )
      throw invalid();
    const lifetime = input.lifetime();
    const entry: Pending = {
      owner,
      baseline,
      current: () => {
        const current = input.lifetime(),
          context = input.context();
        return (
          current[0] &&
          lifetime.every((value, index) => value === current[index]) &&
          context?.manager === request.binding.manager &&
          context.identity === request.binding.identity
        );
      },
      raw: null,
      stage: 'raw',
      live: true,
      completed: false,
      cleanup: [],
    };
    pending.add(entry);
    entry.cleanup.push(owner.onRevoke(() => revoke(entry, false)));
    let published = false;
    try {
      await request.retrieveOnce().then((returned) => {
        if (entry.live) entry.raw = returned;
      });
      if (!entry.live || entry.raw === null) throw invalid();
      if (!continuous(entry, input.project())) throw invalid();
      entry.stage = 'http';
      // Gap closure belongs to the parent, even if this value owner is revoked while awaiting HTTP.
      const lease = await closeSubscriptionGap({
        context: input.context,
        lifetime: input.lifetime,
        restartReady: input.restartReady,
        sync: input.sync,
      });
      if (!entry.live) throw invalid();
      entry.cleanup.push(lease.onLoss(() => revoke(entry)));
      if (!lease.isCurrent() || !continuous(entry, input.project())) throw invalid();
      entry.stage = 'sync';
      const context = input.context(),
        projection = input.project();
      const completed = own(projection, owner);
      const secret = projection.secrets.find((item) => item.passId === owner.passId);
      if (
        !entry.live ||
        entry.raw === null ||
        !lease.isCurrent() ||
        !context?.ready ||
        !projection.session.ready ||
        !continuous(entry, projection) ||
        completed?.clientStatus !== 'done' ||
        completed.outcome !== 'accepted' ||
        completed.resolutionSource === 'unknown' ||
        secret?.primary.kind !== 'recover'
      )
        throw invalid();
      // No await between this new current fence, decoding and the one public delivery.
      const epoch = context.epoch;
      const value = request.decode(entry.raw);
      entry.raw = null;
      if (
        !entry.live ||
        !entry.current() ||
        !owner.isCurrent() ||
        !lease.isCurrent() ||
        input.context()?.epoch !== epoch ||
        !input.controller().deliverRecoveredReveal({ owner, value })
      )
        throw invalid();
      published = true;
      return { kind: 'delivered', owner };
    } finally {
      revoke(entry, !published);
    }
  }
  return {
    handoff,
    observe,
    allowsSync: () => pending.size > 0,
    invalidateStream() {
      // Only the retired stream may suspend an unfinished call. A successor event is always hard.
      for (const entry of [...pending]) if (entry.stage !== 'raw') revoke(entry);
    },
    revoke: () => {
      for (const entry of [...pending]) revoke(entry);
    },
  };
}
