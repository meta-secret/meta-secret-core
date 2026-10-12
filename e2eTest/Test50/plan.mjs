/** Pure F1 plan. No platform, network, build or business actions. */
export const platforms = ['web', 'android', 'ios', 'cli'];

export function buildDeletionPlan(runId) {
  if (!/^[a-zA-Z0-9_-]+$/.test(runId ?? '')) throw new Error('Unsafe/missing run ID');
  const definitions = [
    ...platforms.map((requester) => ({ id: `one-${requester}`, members: [requester], variants: [{ requester, outcome: 'accepted' }] })),
    ...[['web', 'android'], ['cli', 'ios']].map((members, index) => ({
      id: `two-${index + 1}`, members,
      variants: members.map((requester, actorIndex) => ({ requester, offline: members[1 - actorIndex], outcome: 'accepted' })),
    })),
    ...[['web', 'android', 'ios'], ['cli', 'android', 'ios']].map((members, index) => ({
      id: `three-${index + 1}`, members,
      variants: members.flatMap((requester, actorIndex) => ['accepted', 'declined'].map((outcome) => ({
        requester, responder: members[(actorIndex + 1) % 3], observer: members[(actorIndex + 2) % 3],
        outcome: outcome === 'accepted-offline' ? 'accepted' : outcome,
        ...(outcome === 'accepted-offline' ? { offline: members[(actorIndex + 2) % 3] } : {}),
      }))),
    })),
  ];
  let number = 0;
  const blocks = definitions.map((definition) => ({
    ...definition, runId: `${runId}-${definition.id}`,
    cycles: definition.variants.flatMap((variant, variantIndex) => Array.from({ length: 3 }, (_, index) => ({
      ...variant, number: ++number, repetition: index + 1, variant: variantIndex + 1,
      secretName: `${runId}-${definition.id}-v${variantIndex + 1}-r${index + 1}`,
      requirements: [
        'cancel-no-action', 'valid-show-survives-own-refresh', 'canonical-pass-id',
        'all-peers-ready-before-mutation', 'persisted-request-and-rendered-state',
        ...(definition.members.length === 3 ? ['different-current-member', 'first-response-wins', 'terminal-all-peers'] : ['local-confirmation-only']),
        ...(variant.offline ? ['offline-cached-id', 'startup-sync-overlay-blocks-read-copy', 'reconnect-tombstone-before-unblock'] : []),
        ...(variant.outcome === 'accepted' ? ['remove-revealed-and-cached-id', 'reject-late-reveal', 'same-name-new-id', 'reject-old-id-after-recreation'] : ['preserve-secret-after-decline']),
        'clean-boundary-all-peers',
      ],
    }))),
  }));
  return { runId, repetitions: 3, expectedBlocks: 8, expectedCycles: 60, blocks };
}

export function validateDeletionPlan(plan) {
  if (plan.blocks.length !== 8 || plan.blocks.flatMap((block) => block.cycles).length !== 60) throw new Error('Deletion matrix must contain 8 blocks / 60 cycles');
  const observed = new Map(platforms.map((platform) => [platform, new Set()]));
  const identifiers = new Set();
  for (const block of plan.blocks) {
    if (block.members.length < 1 || block.members.length > 3 || new Set(block.members).size !== block.members.length) throw new Error(`Invalid membership ${block.id}`);
    for (const cycle of block.cycles) {
      if (!block.members.includes(cycle.requester)) throw new Error('Requester is not a current member');
      observed.get(cycle.requester).add('requester');
      observed.get(cycle.requester).add(`policy-${block.members.length}`);
      if (cycle.responder) {
        if (cycle.requester === cycle.responder || !block.members.includes(cycle.responder)) throw new Error('Self/foreign decision actor');
        observed.get(cycle.responder).add(cycle.outcome === 'accepted' ? 'approver' : 'decliner');
      }
      if (cycle.offline) observed.get(cycle.offline).add('offline');
      if (identifiers.has(cycle.secretName)) throw new Error('Duplicate synthetic Secret selector');
      identifiers.add(cycle.secretName);
      if (![1, 2, 3].includes(cycle.repetition)) throw new Error('Missing three repetitions');
    }
  }
  for (const [platform, roles] of observed) for (const role of ['requester', 'approver', 'decliner', 'offline', 'policy-1', 'policy-2', 'policy-3']) {
    if (!roles.has(role)) throw new Error(`${platform} missing ${role}`);
  }
  return { blocks: plan.blocks.length, cycles: identifiers.size, roles: Object.fromEntries([...observed].map(([key, value]) => [key, [...value]])) };
}
