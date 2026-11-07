/** Test-only metadata and causal matching. No Core call, queue selection or policy implementation. */
const fail = (message) => { throw new Error(`FIFO ${message}`); };
const text = (value) => typeof value === 'string' && value.length > 0 && value.length <= 512 && !/[\r\n]/.test(value);
const pending = (op) => ['pending', 'needApprove'].includes(op.clientStatus) && op.outcome === undefined;
const freeze = (value) => { for (const item of Object.values(value)) if (item && typeof item === 'object') freeze(item); return Object.freeze(value); };
function memberMap(input) {
  const result = {};
  for (const platform of ['web', 'android', 'ios']) {
    const member = input?.[platform];
    if (!text(member?.deviceId) || !text(member?.deviceName)) fail('canonical member identity is missing');
    result[platform] = { deviceId: member.deviceId, deviceName: member.deviceName };
  }
  if (new Set(Object.values(result).map((x) => x.deviceId)).size !== 3) fail('canonical member identities are not unique');
  return result;
}

export function buildFifoContext({ runId, testNumber, blockId = 'scenario', cycle, identities, passes, baselineRequestIds, projection }) {
  if (!text(runId) || !text(blockId) || !Number.isInteger(testNumber) || !Number.isInteger(cycle?.number) || cycle.number < 1) fail('run/cycle binding is invalid');
  const members = memberMap(identities); const baseline = new Set(baselineRequestIds);
  const submissions = Object.entries(cycle.senderSecrets).map(([platform, subject]) => {
    const requester = members[platform], passId = passes[subject];
    if (!requester || !text(passId) || !text(subject)) fail('submission identity is missing');
    const found = projection.operations.filter((op) => op.kind === 'recover' && op.requesterId === requester.deviceId && op.secretId === passId && !baseline.has(op.requestId));
    if (found.length !== 1) fail('current-cycle submission must have one unique canonical Claim UUID');
    const op = found[0];
    if (!text(op.requestId) || !pending(op)) fail('submission is not canonically pending');
    if (!Array.isArray(op.eligiblePeerIds) || !op.eligiblePeerIds.length || new Set(op.eligiblePeerIds).size !== op.eligiblePeerIds.length || op.eligiblePeerIds.includes(requester.deviceId) || op.eligiblePeerIds.some((id) => !Object.values(members).some((m) => m.deviceId === id))) fail('submission addressed peers are invalid');
    return { kind: 'recover', requestId: op.requestId, passId, requesterId: requester.deviceId, requesterName: requester.deviceName, subject, eligiblePeerIds: [...op.eligiblePeerIds] };
  });
  if (new Set(submissions.map((x) => x.requestId)).size !== submissions.length) fail('submissions share a Claim UUID');
  const active = projection.operations.filter((op) => op.kind === 'recover' && pending(op));
  if (active.length !== submissions.length || active.some((op) => !submissions.some((r) => r.requestId === op.requestId))) fail('foreign or stale pending submission exists');
  const approvals = cycle.approvals.map((approval, index) => {
    const passId = passes[approval.secret], member = members[approval.platform], action = approval.action ?? 'approve';
    if (!text(passId) || !member || !['approve', 'decline'].includes(action) || !submissions.some((r) => r.passId === passId && r.eligiblePeerIds.includes(member.deviceId))) fail('approval has no addressed submission');
    return { logicalStep: index + 1, approverPlatform: approval.platform, passId, subject: approval.secret, action };
  });
  if (approvals.length !== submissions.length) fail('original approvals/submissions cardinality differs');
  return freeze({ version: 1, runId, testNumber, blockId, cycle: cycle.number, identities: members, requests: submissions, approvals });
}

export function matchFifoHead({ context, head, platform, remainingSteps, settledRequestIds, bindings = {} }) {
  const member = context.identities[platform];
  const request = context.requests.find((r) => r.requestId === head.requestId);
  if (!member || head.kind !== 'recover' || !request) fail('visible head is not a submitted recovery UUID');
  if (settledRequestIds.includes(request.requestId)) fail('visible head is already settled');
  if (!request.eligiblePeerIds.includes(member.deviceId)) fail('visible head is not addressed to this eligible receiver');
  if (head.requester !== request.requesterName || head.subject !== request.subject) fail('visible requester/subject does not match the canonical UUID/Pass identity');
  const expectedCount = context.requests.filter((r) => r.eligiblePeerIds.includes(member.deviceId) && !settledRequestIds.includes(r.requestId)).length;
  if (!Number.isInteger(head.count) || head.count !== expectedCount || expectedCount < 1) fail('visible total addressed queue count differs');
  const previouslyBound = Object.entries(bindings).filter(([, id]) => id === request.requestId).map(([step]) => Number(step));
  if (previouslyBound.some((step) => !remainingSteps.includes(step))) fail('visible UUID was already bound to a consumed logical step');
  const candidates = context.approvals.filter((a) => remainingSteps.includes(a.logicalStep) && a.approverPlatform === platform && a.passId === request.passId && (!bindings[a.logicalStep] || bindings[a.logicalStep] === request.requestId));
  if (!candidates.length || new Set(candidates.map((a) => a.action)).size !== 1) fail('visible head has no unambiguous original assigned approval');
  const approval = candidates.find((a) => previouslyBound.includes(a.logicalStep)) ?? candidates.reduce((a, b) => a.logicalStep < b.logicalStep ? a : b);
  return { ...request, ...approval, expectedCount };
}

export function validateFifoTerminal({ context, requestId, outcome, settledRequestIds, projection }) {
  const target = context.requests.find((r) => r.requestId === requestId);
  if (!target || settledRequestIds.includes(requestId)) fail('terminal UUID is foreign or already settled');
  for (const request of context.requests) {
    const found = projection.operations.filter((op) => op.kind === 'recover' && op.requestId === request.requestId && op.requesterId === request.requesterId && op.secretId === request.passId);
    if (found.length !== 1) fail('terminal/current pending UUID identity is missing or not unique');
    const op = found[0];
    if (request.requestId === requestId || settledRequestIds.includes(request.requestId)) {
      const expected = request.requestId === requestId ? outcome : 'accepted';
      if (op.outcome !== expected || !['canonical', 'verified'].includes(op.resolutionSource)) fail('exact canonical terminal is not observed');
    } else if (!pending(op)) fail('other outstanding request is no longer pending');
  }
}

export function nativeHeadChoices(context, platform, remainingSteps, settledRequestIds, bindings = {}) {
  const result = [];
  for (const request of context.requests) {
    if (settledRequestIds.includes(request.requestId) || !request.eligiblePeerIds.includes(context.identities[platform].deviceId)) continue;
    const count = context.requests.filter((r) => r.eligiblePeerIds.includes(context.identities[platform].deviceId) && !settledRequestIds.includes(r.requestId)).length;
    let matched;
    try { matched = matchFifoHead({ context, head: { kind: 'recover', requestId: request.requestId, requester: request.requesterName, subject: request.subject, count }, platform, remainingSteps, settledRequestIds, bindings }); } catch { continue; }
    result.push({ marker: `E2E: ${platform.toUpperCase()}_INCOMING_VISIBLE_${context.cycle}_${matched.logicalStep} claimId=${request.requestId}`, value: matched });
  }
  return result;
}

export function createFifoPublication({ runId, testNumber, blockId = 'scenario' }) {
  let identities = null; const contexts = new Map();
  return {
    publishIdentities(input) {
      const next = memberMap(input);
      if (identities && JSON.stringify(identities) !== JSON.stringify(next)) fail('identity publication is immutable');
      return identities ??= freeze(next);
    },
    publish(context) {
      if (!Object.isFrozen(context) || !Object.isFrozen(context.requests) || context.requests.some((request) => !Object.isFrozen(request) || !Object.isFrozen(request.eligiblePeerIds)) || !Object.isFrozen(context.approvals) || context.approvals.some((approval) => !Object.isFrozen(approval)) || !Object.isFrozen(context.identities)) fail('context publication must be immutable');
      if (!identities || context.version !== 1 || context.runId !== runId || context.testNumber !== testNumber || context.blockId !== blockId || JSON.stringify(context.identities) !== JSON.stringify(identities)) fail('context run/member binding differs');
      if (contexts.has(context.cycle)) fail('cycle context already published');
      contexts.set(context.cycle, context);
    },
    read: (cycle) => contexts.get(cycle) ?? null,
    require(cycle) { const value = contexts.get(cycle); if (!value) fail('cycle context not published'); return value; },
  };
}
