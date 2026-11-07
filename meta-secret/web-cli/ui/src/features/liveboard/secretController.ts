import { readonly, ref } from 'vue';
import { cancelled, createLifetime, sameSession, usable } from './lifecycle';
import type {
  LiveboardSession,
  RecoveredRevealOwner,
  RevealedValue,
  SecretCardView,
  SecretPort,
  SecretPresentation,
  SecretSnapshot,
  SecretView,
} from './types';

const instruction = (card: SecretCardView) => JSON.stringify(card.secret.primary);
const initialPhase = (card: SecretCardView) =>
  card.secret.deletionRequestId ? 'deleting' : card.secret.primary.kind === 'waiting' ? 'waiting' : 'masked';

/** Plaintext belongs only to the currently visible exact Pass and current ready session. */
export function createSecretController(port: SecretPort) {
  const state = ref<SecretView>({
    cards: {},
    presentation: { active: false, visibleIds: [], query: '', scrollTop: 0 },
    blocked: true,
  });
  const lifetime = createLifetime();
  const revisions = new Map<string, number>();
  const mutations = new Map<string, 'recover' | 'delete'>();
  const mutationOwners = new Map<string, object>();
  type Pending = {
    owner: RecoveredRevealOwner;
    card: SecretCardView;
    live: boolean;
    delivered: boolean;
    listeners: Set<() => void>;
  };
  const recovered = new Map<string, Pending>();
  let session: LiveboardSession | null = null;
  let syncSuspended = false;
  const bump = (id: string) => revisions.set(id, (revisions.get(id) ?? 0) + 1);
  const visible = (id: string) =>
    !state.value.blocked && state.value.presentation.active && state.value.presentation.visibleIds.includes(id);
  function revoke(id: string) {
    const pending = recovered.get(id);
    if (!pending) return;
    recovered.delete(id);
    pending.live = false;
    if (pending.card.phase === 'revealing') pending.card.phase = initialPhase(pending.card);
    for (const listener of pending.listeners) listener();
    pending.listeners.clear();
  }
  function mask(id: string, animate = false, preserveRecovered = false) {
    bump(id);
    if (!preserveRecovered) revoke(id);
    const card = state.value.cards[id];
    if (!card) return;
    card.conceal = animate && card.value !== null;
    card.value = null;
    card.copied = false;
    card.phase = mutations.get(id) === 'delete' ? 'deleting' : mutations.has(id) ? 'waiting' : initialPhase(card);
    if (preserveRecovered && recovered.has(id)) card.phase = 'revealing';
  }
  function suspendForSync() {
    syncSuspended = true;
    state.value.blocked = true;
    lifetime.invalidate();
    for (const id of Object.keys(state.value.cards)) mask(id, false, true);
  }
  function invalidate() {
    suspendForSync();
    syncSuspended = false;
    for (const id of [...recovered.keys()]) revoke(id);
  }
  function reconcile(snapshot: SecretSnapshot, { preserveRecovered = false } = {}) {
    if (lifetime.disposed) return;
    const changed = !sameSession(session, snapshot.session);
    if (changed || (!usable(snapshot.session) && !preserveRecovered)) invalidate();
    else if (!usable(snapshot.session)) suspendForSync();
    if (changed) {
      mutations.clear();
      mutationOwners.clear();
    }
    session = { ...snapshot.session };
    state.value.blocked = !usable(session);
    syncSuspended = preserveRecovered && state.value.blocked;
    const cards: Record<string, SecretCardView> = Object.create(null);
    for (const secret of snapshot.secrets) {
      const old = changed ? undefined : state.value.cards[secret.passId];
      const card: SecretCardView = old ?? { secret: { ...secret }, phase: 'masked', value: null, copied: false };
      const previous = instruction(card);
      const previousDeletion = card.secret.deletionRequestId;
      card.secret = { ...secret, primary: { ...secret.primary } };
      cards[secret.passId] = card;
      if (previous !== instruction(card) || previousDeletion !== secret.deletionRequestId || secret.deletionRequestId) {
        const pending = recovered.get(secret.passId);
        const completion =
          preserveRecovered &&
          pending?.card === card &&
          secret.primary.kind === 'recover' &&
          !secret.deletionRequestId &&
          (previous === JSON.stringify({ kind: 'show-recovered', claimId: pending.owner.claimId }) ||
            previous === JSON.stringify({ kind: 'recover' }));
        if (!completion) revoke(secret.passId);
        mutations.delete(secret.passId);
        mutationOwners.delete(secret.passId);
        bump(secret.passId);
        card.conceal = false;
        card.value = null;
        card.copied = false;
        card.phase = completion ? 'revealing' : initialPhase(card);
      } else if (!old) card.phase = initialPhase(card);
    }
    for (const id of Object.keys(state.value.cards))
      if (!cards[id]) {
        mask(id);
        mutations.delete(id);
        mutationOwners.delete(id);
      }
    state.value.cards = cards;
  }
  function present(presentation: SecretPresentation) {
    state.value.presentation = { ...presentation, visibleIds: [...presentation.visibleIds] };
    for (const id of Object.keys(state.value.cards))
      if (!visible(id)) mask(id, false, syncSuspended && presentation.active && presentation.visibleIds.includes(id));
  }
  function fence(id: string) {
    const token = lifetime.capture(),
      revision = revisions.get(id) ?? 0;
    return () =>
      lifetime.current(token) && revision === (revisions.get(id) ?? 0) && visible(id) && Boolean(state.value.cards[id]);
  }
  function pendingOwner(id: string, claimId: string, card: SecretCardView): Pending {
    const listeners = new Set<() => void>();
    const pending: Pending = {
      card,
      live: true,
      delivered: false,
      listeners,
      owner: {
        passId: id,
        claimId,
        isCurrent: () =>
          !lifetime.disposed &&
          pending.live &&
          recovered.get(id) === pending &&
          state.value.cards[id] === card &&
          state.value.presentation.active &&
          state.value.presentation.visibleIds.includes(id),
        onRevoke(listener) {
          if (!pending.live) listener();
          else listeners.add(listener);
          return () => listeners.delete(listener);
        },
      },
    };
    recovered.set(id, pending);
    return pending;
  }
  function deliverRecoveredReveal(input: {
    owner: unknown;
    value: RevealedValue;
  }): input is { owner: RecoveredRevealOwner; value: RevealedValue } {
    const { value } = input;
    const pending = [...recovered.values()].find((entry) => entry.owner === input.owner);
    const owner = pending?.owner;
    if (
      !pending ||
      !owner ||
      !owner.isCurrent() ||
      !visible(owner.passId) ||
      pending.card.secret.primary.kind !== 'recover' ||
      pending.card.secret.deletionRequestId
    )
      return false;
    pending.card.value =
      value.kind === 'text' ? { kind: 'text', text: value.text } : { kind: 'seed', words: [...value.words] };
    pending.card.phase = 'revealed';
    pending.card.conceal = false;
    pending.card.copied = false;
    pending.delivered = true;
    recovered.delete(owner.passId);
    return true;
  }
  async function primary(id: string) {
    const card = state.value.cards[id];
    if (
      !card ||
      !visible(id) ||
      recovered.has(id) ||
      mutations.has(id) ||
      ['revealing', 'waiting', 'deleting'].includes(card.phase)
    )
      return;
    const action = card.secret.primary;
    if (action.kind === 'blocked' || action.kind === 'waiting') return;
    if (card.phase === 'revealed') {
      mask(id);
      return;
    }
    const valid = fence(id);
    if (action.kind === 'recover') {
      const owner = {};
      mutationOwners.set(id, owner);
      mutations.set(id, 'recover');
      card.phase = 'waiting';
      try {
        const receipt = await port.recover(id);
        if (mutationOwners.get(id) === owner && receipt.phase === 'rejected') {
          mutations.delete(id);
          mutationOwners.delete(id);
          mask(id);
          port.reportError('Recovery request was declined.');
        }
      } catch {
        if (valid()) port.reportError('Could not confirm the recovery request.');
      }
      return;
    }
    card.phase = 'revealing';
    const pending = action.kind === 'show-recovered' ? pendingOwner(id, action.claimId, card) : undefined;
    try {
      const value =
        action.kind === 'show-local'
          ? await port.showLocal(id)
          : await port.showRecovered({ passId: id, claimId: action.claimId });
      if (value.kind === 'delivered') {
        if (pending?.owner !== value.owner || !pending.delivered || !visible(id) || !card.value) {
          if (recovered.get(id) === pending) mask(id);
        }
        return;
      }
      if (!valid()) return;
      card.value =
        value.kind === 'text' ? { kind: 'text', text: value.text } : { kind: 'seed', words: [...value.words] };
      card.conceal = false;
      card.phase = 'revealed';
      card.copied = false;
    } catch {
      if (valid()) {
        mask(id);
        port.reportError('Could not show this secret.');
      }
    } finally {
      if (pending && recovered.get(id) === pending) {
        revoke(id);
        if (card.phase === 'revealing') card.phase = initialPhase(card);
      }
    }
  }
  async function copy(id: string) {
    const card = state.value.cards[id];
    if (!card?.value || card.phase !== 'revealed' || !visible(id)) return;
    const valid = fence(id);
    try {
      await port.copy(card.value.kind === 'text' ? card.value.text : card.value.words.join(' '));
      if (valid() && card.phase === 'revealed') card.copied = true;
    } catch {
      if (valid()) port.reportError('Could not copy this secret.');
    }
  }
  async function remove(id: string) {
    const card = state.value.cards[id];
    if (!card || !visible(id) || !card.secret.canDelete || mutations.has(id) || card.secret.deletionRequestId) return;
    mask(id);
    const owner = {};
    mutationOwners.set(id, owner);
    mutations.set(id, 'delete');
    card.phase = 'deleting';
    const valid = fence(id);
    let submitted = false;
    try {
      if (!(await port.authenticate({ kind: 'delete-secret', passId: id })) || !valid() || !card.secret.canDelete)
        return;
      submitted = true;
      const receipt = await port.deleteSecret(id);
      if (mutationOwners.get(id) === owner && receipt.phase === 'rejected') {
        submitted = false;
        port.reportError('Deletion request was declined.');
      }
    } catch (error) {
      if (valid() && !cancelled(error)) port.reportError('Could not confirm the deletion request.');
    } finally {
      if (mutationOwners.get(id) === owner && !submitted) {
        mutations.delete(id);
        mutationOwners.delete(id);
        mask(id);
      }
    }
  }
  return {
    state: readonly(state),
    reconcile,
    present,
    primary,
    hide: (id: string) => mask(id, true),
    copy,
    remove,
    pendingRecoveredReveal({ passId, claimId }: { passId: string; claimId: string }) {
      const pending = recovered.get(passId);
      return pending?.owner.claimId === claimId && pending.owner.isCurrent() ? pending.owner : undefined;
    },
    deliverRecoveredReveal,
    suspendForSync,
    settleMutation({
      passId,
      kind,
      preserveRecovered = false,
    }: {
      passId: string;
      kind: 'recover' | 'delete';
      preserveRecovered?: boolean;
    }): void {
      if (lifetime.disposed || !state.value.cards[passId] || mutations.get(passId) !== kind) return;
      mutations.delete(passId);
      mutationOwners.delete(passId);
      mask(passId, false, preserveRecovered && kind === 'recover');
    },
    invalidate,
    dispose: () => {
      invalidate();
      lifetime.dispose();
      session = null;
      mutations.clear();
      mutationOwners.clear();
      state.value.cards = {};
      state.value.blocked = true;
    },
  };
}
