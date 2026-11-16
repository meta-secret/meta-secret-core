// Closed literal-profile synchronization. Harness IDs are not Core identities or approvals.
const platforms = Object.freeze(['web', 'android', 'ios']);
const opaque = /^[a-f0-9]{32}(?![\s\S])/;
const label = /^[A-Za-z0-9][A-Za-z0-9_.-]{0,79}(?![\s\S])/;
const events = new Set(['READY', 'REQUEST_SENT', 'INCOMING_VISIBLE']);
const demand = (value, message) => { if (!value) throw Error(message); };
function plain(value, keys) {
  demand(value && Object.getPrototypeOf(value) === Object.prototype, 'Plain phase data required');
  const descriptors = Object.getOwnPropertyDescriptors(value);
  demand(Object.keys(descriptors).sort().join() === [...keys].sort().join()
    && Object.values(descriptors).every(item => 'value' in item), 'Exact phase fields required');
}
const recoveryProfiles = Object.freeze({
  24: ['ios-multi-secret-lifecycle', 13, 39, 27],
  25: ['web-secondary-creator-lifecycle', 13, 39, 27],
  26: ['android-secondary-creator-lifecycle', 13, 39, 27],
  27: ['ios-secondary-creator-lifecycle', 13, 39, 27],
  28: ['web-secondary-creator-lifecycle', 1, 27, 27],
  29: ['android-distributed-creator-lifecycle', 1, 27, 27],
  30: ['ios-distributed-creator-lifecycle', 1, 27, 27],
  31: ['web-secondary-creator-lifecycle', 1, 27, 27],
  32: ['android-owner-creator-lifecycle', 1, 27, 27],
  33: ['ios-two-secret-owner-lifecycle', 1, 27, 27],
  34: ['web-secondary-creator-lifecycle', 1, 18, 18],
  35: ['android-secondary-creator-lifecycle', 1, 27, 27],
  36: ['ios-distributed-creator-lifecycle', 1, 27, 27],
  46: ['web-distributed-creator-removal-rejoin-recovery-lifecycle', 7, 33, 27],
  47: ['web-distributed-creator-declined-removal-recovery-lifecycle', 1, 27, 27],
  48: ['android-distributed-creator-declined-removal-recovery-lifecycle', 1, 27, 27],
  49: ['ios-distributed-creator-declined-removal-recovery-lifecycle', 1, 27, 27],
});
function selectedRecoveryProfile(value) {
  const entry = typeof value === 'string' && Object.entries(recoveryProfiles)
    .find(([number, spec]) => value === `test${number}:${spec[0]}`);
  demand(entry, 'Exact selected recovery profile required');
  return Number(entry[0]);
}
function freezeBinding(value, testNumber = 24) {
  plain(value, ['version', 'runId', 'testNumber', 'attempt', 'blockId', 'cycle', 'logicalStep',
    'mode', 'contextId', 'sender', 'participants']);
  demand(value.version === 1 && value.testNumber === testNumber && Number.isSafeInteger(value.attempt)
    && value.attempt > 0 && value.attempt <= 999999999 && Number.isSafeInteger(value.cycle) && value.cycle >= recoveryProfiles[testNumber][1] && value.cycle <= recoveryProfiles[testNumber][2]
    && value.logicalStep === 1 && value.mode === recoveryProfiles[testNumber][0]
    && typeof value.runId === 'string' && typeof value.blockId === 'string' && typeof value.contextId === 'string'
    && label.test(value.runId) && label.test(value.blockId) && opaque.test(value.contextId)
    && platforms.includes(value.sender), 'Exact selected recovery scope required');
  demand(Array.isArray(value.participants) && value.participants.length === 3, 'Three current participants required');
  const participants = value.participants.map(item => {
    plain(item, ['platform', 'role', 'invocationId']);
    demand(platforms.includes(item.platform) && typeof item.invocationId === 'string' && opaque.test(item.invocationId)
      && item.role === (item.platform === value.sender ? 'sender' : 'receiver'), 'Participant binding mismatch');
    return Object.freeze({...item});
  });
  demand(new Set(participants.map(p => p.platform)).size === 3
    && new Set([value.contextId, ...participants.map(p => p.invocationId)]).size === 4, 'Unique current scope required');
  return Object.freeze({...value, participants: Object.freeze(participants)});
}

export function createTest24RecoveryPhase(options) {
  return createBoundRecoveryPhase(options, 24);
}
function createBoundRecoveryPhase({binding: input, now = () => performance.now(),
  schedule = setTimeout, cancel = clearTimeout, onAbort = () => {}}, testNumber) {
  const binding = freezeBinding(input, testNumber), t0 = now();
  demand(Number.isFinite(t0) && t0 >= 0, 'Monotonic phase origin required');
  let deadlineAt = t0 + 600_000, sentAt, releaseDeadline, released = false, terminal, firstFailure, timer, previousTime = t0;
  const ready = new Set(), visible = new Set(), owners = new Map(), ownerPlatforms = new WeakMap(), waiters = new Set();
  let rejectFailure;
  const failure = new Promise((_, reject) => { rejectFailure = reject; });
  void failure.catch(() => {});
  const receivers = platforms.filter(p => p !== binding.sender);
  const time = () => {
    const value = now(); demand(Number.isFinite(value) && value >= previousTime, 'Invalid monotonic phase time'); previousTime = value; return value;
  };
  const badgeDeadline = () => sentAt === undefined ? undefined : sentAt + 180_000;
  const bound = () => sentAt !== undefined && visible.size !== 2 ? Math.min(deadlineAt, badgeDeadline())
    : released && sentAt === undefined ? Math.min(deadlineAt, releaseDeadline) : deadlineAt;
  const close = (kind, error) => {
    if (terminal) return;
    terminal = kind; cancel(timer);
    if (error) {
      firstFailure = error; rejectFailure(error);
      for (const waiter of waiters) { cancel(waiter.timer); waiter.reject(error); }
      waiters.clear();
      // The caller's normal outer failure path joins the same shutdown promise.
      try { const shutdown = onAbort(error); void shutdown?.catch?.(() => {}); } catch { /* Keep the original failure. */ }
    }
  };
  const expire = () => {
    const error = Object.assign(Error('Current recovery phase deadline expired'), {code: 'RECOVERY_PHASE_EXPIRED'});
    close('expired', error); return firstFailure ?? error;
  };
  const assertActive = () => {
    if (terminal) throw firstFailure ?? Error('Recovery phase is closed');
    if (time() >= bound()) throw expire();
  };
  const arm = () => {
    cancel(timer); if (terminal) return;
    const remaining = bound() - time();
    if (remaining <= 0) { expire(); return; }
    timer = schedule(() => { try { assertActive(); arm(); } catch { /* close already reports the first failure. */ } }, remaining);
    timer?.unref?.();
  };
  const abort = (error = Error('Current recovery phase aborted')) => close('aborted', error);
  const fail = message => { const error = Error(message); abort(error); throw error; };
  const satisfied = waiter => waiter.event === 'READY' ? ready.size === 3
    : waiter.event === 'REQUEST_SENT' ? sentAt !== undefined : visible.has(waiter.platform);
  const publish = (platform, event) => {
    assertActive();
    if (!events.has(event)) return fail('Unknown scoped recovery event');
    if (event === 'READY') {
      if (released || sentAt !== undefined || ready.has(platform)) return fail('Duplicate or out-of-order scoped READY');
      ready.add(platform);
    } else if (event === 'REQUEST_SENT') {
      if (platform !== binding.sender || !released || ready.size !== 3 || sentAt !== undefined) return fail('Unadmitted scoped SENT');
      sentAt = time();
    } else {
      if (!receivers.includes(platform) || sentAt === undefined || visible.has(platform)) return fail('Unadmitted scoped incoming proof');
      visible.add(platform);
    }
    arm();
    for (const waiter of [...waiters]) if (satisfied(waiter)) {
      waiters.delete(waiter); cancel(waiter.timer); waiter.resolve();
    }
  };
  const participant = platform => binding.participants.find(p => p.platform === platform);
  const contextFor = platform => {
    assertActive(); const entry = participant(platform); demand(entry, 'Unknown phase participant');
    return Object.freeze({version: 1, runId: binding.runId, testNumber: binding.testNumber, attempt: binding.attempt,
      blockId: binding.blockId, cycle: binding.cycle, logicalStep: 1, mode: binding.mode,
      contextId: binding.contextId, invocationId: entry.invocationId, platform, role: entry.role});
  };
  const observer = (platform, owner) => {
    assertActive(); demand(platform !== 'web' && participant(platform) && owner && typeof owner === 'object', 'Actual native output owner required');
    demand(!owners.has(platform) && !ownerPlatforms.has(owner), 'Native output owner is already bound');
    owners.set(platform, owner); ownerPlatforms.set(owner, platform);
    let pending = '', oversized = false;
    const marker = /^E2E: RECOVERY_PHASE_V1 context=([a-f0-9]{32}) invocation=([a-f0-9]{32}) platform=(ios|android) phase=(READY|REQUEST_SENT|INCOMING_VISIBLE)$/;
    const androidPrefix = /^[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]{3}\s+[0-9]+\s+[0-9]+\s+I MetaSecretE2E: /;
    return Object.freeze({push(sink, text) {
      demand(typeof text === 'string', 'Text-only live phase output required');
      if (sink !== 'stdout') { if (text.includes('RECOVERY_PHASE_V1')) fail('Phase evidence on non-authoritative stream'); return; }
      const lines = (pending + text).split('\n'); pending = lines.pop();
      for (let line of lines) {
        line = line.replace(/\r$/, '');
        if (line.includes('RECOVERY_PHASE_V1')) {
          if (oversized || line.length > 2048) fail('Oversized scoped phase evidence');
          if (platform === 'android') {
            const prefix = line.match(androidPrefix); if (!prefix) fail('Foreign Android phase framing');
            line = line.slice(prefix[0].length);
          }
          const match = line.match(marker);
          if (!match || match[1] !== binding.contextId || match[2] !== participant(platform).invocationId || match[3] !== platform) fail('Mismatched live scoped phase evidence');
          publish(platform, match[4]);
        }
        oversized = false;
      }
      if (pending.length > 2048) { oversized = true; pending = pending.slice(-2048); }
    }});
  };
  const waitFor = (event, maximumMs, platform) => {
    try {
      assertActive();
      demand(events.has(event) && Number.isSafeInteger(maximumMs) && maximumMs > 0
        && maximumMs <= (event === 'READY' ? 240_000 : 180_000), 'Original phase wait ceiling required');
      if (event === 'INCOMING_VISIBLE') demand(receivers.includes(platform), 'Registered receiver required');
      const waiter = {event, platform}; if (satisfied(waiter)) return Promise.resolve();
      return new Promise((resolve, reject) => {
        Object.assign(waiter, {resolve, reject}); waiters.add(waiter);
        waiter.timer = schedule(() => abort(Error('Current scoped phase proof timed out')),
          Math.min(maximumMs, bound() - time()));
      });
    } catch (error) { return Promise.reject(error); }
  };
  const snapshot = value => {
    let url;
    try { url = new URL(value, 'http://127.0.0.1'); } catch { return {status: 400}; }
    const keys = [...url.searchParams.keys()];
    if (url.pathname !== '/recovery-phase' || keys.length !== 4 || new Set(keys).size !== 4
      || !['v', 'context', 'participant', 'view'].every(key => keys.includes(key))
      || url.searchParams.get('v') !== '1' || !opaque.test(url.searchParams.get('context') ?? '')
      || !opaque.test(url.searchParams.get('participant') ?? '')) return {status: 400};
    const entry = binding.participants.find(p => p.invocationId === url.searchParams.get('participant'));
    if (url.searchParams.get('context') !== binding.contextId || !entry) return {status: 404};
    const view = url.searchParams.get('view');
    if (view !== (entry.role === 'sender' ? 'dispatch' : 'request-sent')) return {status: 400};
    try { assertActive(); } catch { return {status: terminal === 'expired' ? 408 : 410}; }
    const remaining = Math.floor(deadlineAt - time());
    const badgeRemaining = sentAt === undefined ? 0 : Math.floor(Math.min(deadlineAt, badgeDeadline()) - time());
    if (remaining <= 0) { expire(); return {status: 408}; }
    if (view === 'request-sent' && sentAt !== undefined && badgeRemaining <= 0) {
      if (visible.size === 2) return {status: 410};
      expire(); return {status: 408};
    }
    return {status: 200, body: {version: 1, contextId: binding.contextId, invocationId: entry.invocationId, view,
      state: view === 'dispatch' ? released ? 'DISPATCH_PERMITTED' : 'WAITING' : sentAt === undefined ? 'WAITING' : 'REQUEST_SENT',
      deadlineRemainingMs: remaining, badgeRemainingMs: view === 'request-sent' ? badgeRemaining : 0}};
  };
  const phase = {failure, contextFor, observer, waitFor, snapshot, assertActive, abort,
    get contextId() { return binding.contextId; }, get cycle() { return binding.cycle; },
    get deadlineAt() { return deadlineAt; }, get sentAt() { return sentAt; }, get badgeDeadlineAt() { return badgeDeadline(); },
    setIosDeadline(value) {
      assertActive(); demand(Number.isFinite(value) && value > time() && value <= time() + 600_000, 'Actual ordinary iOS deadline required');
      deadlineAt = Math.min(deadlineAt, value); arm();
    },
    releaseSender() { assertActive(); demand(ready.size === 3 && !released, 'All current READY precede one dispatch'); released = true; releaseDeadline = time() + 180_000; arm(); },
    web(event) { publish('web', event); },
    requireBothBadges() { assertActive(); demand(visible.size === 2, 'Both actual receiver proofs precede approval'); },
    async run(action) {
      try {
        assertActive(); demand(typeof action === 'function', 'A current action callback is required');
        const result = await Promise.race([Promise.resolve(action()), failure]); assertActive(); return result;
      } catch (error) { abort(error); throw firstFailure ?? error; }
    },
    complete() { assertActive(); demand(visible.size === 2, 'Incomplete phase cannot finish'); close('completed'); },
  };
  arm(); return Object.freeze(phase);
}

export function createRecoveryPhaseRegistry(options = {}) {
  const testNumber = selectedRecoveryProfile(Object.hasOwn(options, 'selectedProfile') ? options.selectedProfile : 'test24:ios-multi-secret-lifecycle');
  const phases = new Map(), cycles = new Set(); let frozen = false;
  return Object.freeze({
    register(binding, onAbort) {
      demand(!frozen && phases.size < recoveryProfiles[testNumber][3] && !phases.has(binding.contextId) && !cycles.has(binding.cycle), 'Fresh finite phase registry required');
      const phase = createBoundRecoveryPhase({...options, binding, onAbort}, testNumber);
      phases.set(phase.contextId, phase); cycles.add(phase.cycle); return phase;
    },
    snapshot(url) {
      let context;
      try { context = new URL(url, 'http://127.0.0.1').searchParams.get('context'); } catch { return {status: 400}; }
      const phase = phases.get(context);
      if (phase) return phase.snapshot(url);
      // Preserve strict query rejection before reporting an unknown context.
      const params = new URL(url, 'http://127.0.0.1').searchParams;
      const keys = [...params.keys()];
      return {status: keys.length === 4 && new Set(keys).size === 4 && ['v', 'context', 'participant', 'view'].every(k => keys.includes(k))
        && params.get('v') === '1' && opaque.test(context ?? '') && opaque.test(params.get('participant') ?? '')
        && ['dispatch', 'request-sent'].includes(params.get('view')) ? 404 : 400};
    },
    assertCycleActive(cycle) {
      const phase = [...phases.values()].find(item => item.cycle === cycle);
      demand(phase, 'Unregistered recovery cycle'); phase.assertActive();
    },
    freeze(error) { if (frozen) return; frozen = true; for (const phase of phases.values()) phase.abort(error); },
  });
}

export function handleRecoveryPhaseRequest(request, response, registry) {
  let url;
  try { url = new URL(request.url, 'http://127.0.0.1'); } catch { return false; }
  if (url.pathname !== '/recovery-phase') return false;
  const result = request.method === 'GET' ? registry.snapshot(request.url) : {status: 400};
  const text = result.status === 200 ? JSON.stringify(result.body) : '';
  demand(Buffer.byteLength(text, 'utf8') <= 2048, 'Bounded phase snapshot required');
  response.writeHead(result.status, {'content-type': 'application/json', 'cache-control': 'no-store'});
  response.end(text); return true;
}

export function abortRecoveryPhaseHarness({coordinator, shutdown}) {
  return error => { coordinator.freezeGrants(); return shutdown.finish({error, diagnostics: true}); };
}
