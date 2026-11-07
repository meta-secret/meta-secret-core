/** Pure F1 plan. No platform, network, build or business actions. */
export const platforms = ['web', 'android', 'ios', 'cli'];

/** Test50 admission is independent of its mandatory real Core routing. No ports run here. */
export function resolveDeletionAuthProfile(environment = {}, configuration = {}) {
  const supplied = Object.hasOwn(environment, 'E2E_TEST50_AUTH_PROFILE');
  const configured = Object.hasOwn(configuration, 'authProfile');
  const profile = supplied ? environment.E2E_TEST50_AUTH_PROFILE : configured ? configuration.authProfile : 'real-os';
  if (!['real-os', 'controlled-business'].includes(profile) || (configured && configuration.authProfile !== profile)) throw Error('Invalid or conflicting Test50 auth profile');
  const pending = [environment, configuration]; let count = 0;
  const modes = ['METASECRET_UI_TEST_MODE', 'SIMCTL_CHILD_METASECRET_UI_TEST_MODE'];
  const credentials = ['E2E_ANDROID_DEVICE_PASSCODE', 'E2E_IOS_DEVICE_PASSCODE', 'devicePasscode'];
  const forces = ['metasecret.force_system_auth', 'metasecret.test.force_system_auth'];
  while (pending.length) {
    if (++count > 8192) throw Error('Test50 profile input exceeds inspection bound');
    const item = pending.pop();
    if (!item || typeof item !== 'object') continue;
    if (modes.some(key => Object.hasOwn(item, key))) throw Error('Test50 host MODE must be absent');
    if (profile === 'controlled-business' && [...credentials, ...forces].some(key => Object.hasOwn(item, key))) throw Error('Controlled business forbids credential or force-auth input');
    if (profile === 'controlled-business' && ['JAVA_TOOL_OPTIONS', 'JDK_JAVA_OPTIONS', '_JAVA_OPTIONS'].some(key =>
      typeof item[key] === 'string' && /-Dmetasecret\.(?:test\.)?force_system_auth(?:=|\s|$)/.test(item[key]))) throw Error('Controlled business forbids host JVM force-auth input');
    pending.push(...Object.values(item).filter(value => value && typeof value === 'object'));
  }
  return profile;
}

export function deletionCoverage(authProfile, pendingNativeCancelSamples = 36) {
  if (!['real-os', 'controlled-business'].includes(authProfile)) throw Error('Invalid Test50 coverage profile');
  return authProfile === 'controlled-business'
    ? { coverageScope: 'business-only', manualOs: { status: 'unrun', confirmation: 'unrun', cancel: 'unrun', platforms: ['android', 'ios'], pendingNativeCancelSamples }, fullAcceptance: false }
    : { coverageScope: 'original-real-os', manualOs: null, fullAcceptance: true };
}

/** The same receipt oracle is used by block parent, range caller and controlled tests. */
export function validateDeletionResult(result, expected) {
  if (JSON.stringify(result)?.length > 65536) throw Error('Test50 receipt exceeds its bound');
  if (!Number.isSafeInteger(expected.attempt) || expected.attempt < 1) throw Error('Invalid Test50 result attempt');
  const plan = buildDeletionPlan(expected.runId);
  const pendingSamples = expected.blocks.reduce((sum, block) => sum + (plan.blocks.find(item => item.id === block.id)?.cycles
    .filter(cycle => block.cycles.includes(cycle.number) && ['android', 'ios'].includes(cycle.requester)).length ?? 0), 0);
  const coverage = deletionCoverage(expected.authProfile, pendingSamples);
  const cycles = expected.blocks.reduce((sum, block) => sum + block.cycles.length, 0);
  if (result?.version !== 1 || result.runId !== expected.runId || result.attempt !== expected.attempt ||
      result.authProfile !== expected.authProfile || result.stage !== 'F4' ||
      JSON.stringify(result.blocks) !== JSON.stringify(expected.blocks) ||
      result.business?.status !== 'passed' || result.business.blocks !== expected.blocks.length || result.business.cycles !== cycles ||
      result.outcome?.code !== 0 || result.outcome.signal !== null || !Array.isArray(result.outcome.cleanupErrors) || result.outcome.cleanupErrors.length ||
      Object.keys(coverage).some(key => JSON.stringify(result[key]) !== JSON.stringify(coverage[key]))) throw Error('Missing, partial or mismatched Test50 result receipt');
  return result;
}

export function requireDeletionResumeProfile(previous, current) {
  if ((previous ?? 'real-os') !== current) throw Error('Test50 resume auth profile mismatch');
  return current;
}

export function deletionEntryComplete(result, authProfile) {
  if (result.test !== 50) return result.status === 'passed';
  if (authProfile === 'real-os') return result.status === 'passed' && (result.authProfile ?? 'real-os') === authProfile;
  if (result.status !== 'business-passed-manual-pending' || result.authProfile !== authProfile) return false;
  try {
    const receipt = result.acceptance;
    const blocks = buildDeletionPlan(receipt.runId).blocks.map((block, index) => {
      const configurationSha256 = receipt.blocks[index]?.configurationSha256;
      if (!/^[a-f0-9]{64}$/.test(configurationSha256)) throw Error('Invalid Test50 config hash');
      return { id: block.id, cycles: block.cycles.map(cycle => cycle.number), configurationSha256 };
    });
    validateDeletionResult(receipt, { runId: receipt.runId, attempt: result.attempt, authProfile, blocks });
    return true;
  } catch { return false; }
}

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
