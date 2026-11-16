// Exact finite15 TEST procedure evidence; never approval, plaintext or actor success.
const scopeKeys = ['testNumber', 'role', 'nonce', 'cycle', 'step', 'runId', 'contextId', 'invocationId'];
const senderProfiles = Object.freeze({
  26: ['android-secondary-creator-lifecycle', [16,17,18,25,26,27,34,35,36]],
  27: ['ios-secondary-creator-lifecycle', [13,14,15,22,23,24,31,32,33]],
  28: ['web-secondary-creator-lifecycle', [7,8,9,16,17,18,25,26,27]],
  29: ['android-distributed-creator-lifecycle', [4,5,6,13,14,15,22,23,24]],
  30: ['ios-distributed-creator-lifecycle', [1,2,3,10,11,12,19,20,21]],
  31: ['web-secondary-creator-lifecycle', [7,8,9,16,17,18,25,26,27]],
  32: ['android-owner-creator-lifecycle', [4,5,6,13,14,15,22,23,24]],
  33: ['ios-two-secret-owner-lifecycle', [1,2,3,10,11,12,19,20,21]],
  34: ['web-secondary-creator-lifecycle', [5,6,11,12,17,18]],
  35: ['android-secondary-creator-lifecycle', [4,5,6,13,14,15,22,23,24]],
  36: ['ios-distributed-creator-lifecycle', [1,2,3,10,11,12,19,20,21]],
  46: ['web-distributed-creator-removal-rejoin-recovery-lifecycle', [25,26,27,28,29,30,31,32,33]],
  47: ['web-distributed-creator-declined-removal-recovery-lifecycle', [7,8,9,16,17,18,25,26,27]],
  48: ['android-distributed-creator-declined-removal-recovery-lifecycle', [7,8,9,16,17,18,25,26,27]],
  49: ['ios-distributed-creator-declined-removal-recovery-lifecycle', [7,8,9,16,17,18,25,26,27]],
});
const hex = value => typeof value === 'string' && /^[0-9a-f]{32}(?![\s\S])/.test(value);
const demand = (value, message) => { if (!value) throw new Error(message); };

export function assertRecoveryShowScope(scope, expected) {
  demand(scope && Object.getPrototypeOf(scope) === Object.prototype && Object.isFrozen(scope), 'Exact frozen Finite recovery Show scope required');
  const keys = Reflect.ownKeys(scope);
  demand(keys.length === scopeKeys.length && keys.every((key, index) => key === scopeKeys[index])
    && keys.every(key => {
      const field = Object.getOwnPropertyDescriptor(scope, key);
      return Object.hasOwn(field, 'value') && field.enumerable === true;
    }),
  'Exact Finite recovery Show data fields required');
  const profile = expected && Number.isSafeInteger(expected.testNumber) && Object.hasOwn(senderProfiles, expected.testNumber)
    ? senderProfiles[expected.testNumber] : null;
  demand(profile && expected.version === 1
    && expected.mode === profile[0] && expected.platform === 'ios'
    && expected.role === 'sender' && expected.logicalStep === 1
    && Number.isInteger(expected.cycle) && profile[1].includes(expected.cycle), 'Current finite recovery iOS sender required');
  demand(scope.testNumber === expected.testNumber && scope.role === 'sender' && hex(scope.nonce)
    && Number.isInteger(scope.cycle) && scope.cycle === expected.cycle && scope.step === 1
    && typeof scope.runId === 'string' && /^[A-Za-z0-9][A-Za-z0-9_.-]{0,79}(?![\s\S])/.test(scope.runId)
    && scope.runId === expected.runId && hex(scope.contextId) && hex(scope.invocationId)
    && scope.contextId === expected.contextId && scope.invocationId === expected.invocationId,
  'Finite recovery Show scope conflicts with the current independent invocation');
  return scope;
}

export async function runRecoveryIosShow({ actor, scope, phase, allowShow, peerFailure, now = () => performance.now() }) {
  const drain = async cause => {
    if (typeof actor?.cancelAndJoin === 'function') {
      try { await actor.cancelAndJoin(cause); }
      catch (cleanup) {
        throw cleanup === cause ? cause : new AggregateError([cause, cleanup],
          'Finite recovery Show failed and its actor did not drain', { cause });
      }
    }
    throw cause;
  };
  let receipt, initialPhaseDeadline, previousTime, prepDeadline;
  const time = () => {
    const at = now();
    demand(Number.isFinite(at) && (previousTime === undefined || at >= previousTime), 'Finite recovery Show clock is invalid');
    previousTime = at; return at;
  };
  try {
    demand(actor && phase && typeof allowShow === 'function', 'Current Finite recovery Show owner and phase required');
    phase.assertActive(); actor.assertBusinessActive();
    demand(scope === actor.recoveryShowPhase, 'Same owned Finite recovery Show scope required');
    assertRecoveryShowScope(scope, phase.contextFor('ios'));
    receipt = actor.selectedSuiteDeadline; initialPhaseDeadline = phase.deadlineAt;
    demand(receipt && receipt.timeoutMs === 600_000 && Number.isFinite(receipt.deadlineAt)
      && Number.isFinite(initialPhaseDeadline) && initialPhaseDeadline <= receipt.deadlineAt,
    'Finite recovery Show requires its armed ordinary600s and current phase deadline');
    const start = time();
    demand(start < Math.min(receipt.deadlineAt, initialPhaseDeadline), 'Finite recovery Show deadline exhausted');
    prepDeadline = Math.min(start + 180_000, receipt.deadlineAt, initialPhaseDeadline);
  } catch (failure) { return drain(failure); }

  const prefix = 'E2E: IOS_RECOVERY_SHOW_PREP_V1 ';
  const expectedPrefix = prefix + scope.testNumber + ' ' + scope.nonce + ' ' + scope.contextId + ' ' + scope.invocationId + ' ' + scope.cycle + ' ' + scope.step + ' ';
  const visible = 'E2E: IOS_RECOVERY_SECRET_VISIBLE_' + scope.cycle + '_' + scope.step;
  const closed = 'E2E: IOS_RECOVERY_CLOSED_' + scope.cycle + '_' + scope.step;
  const prepPattern = new RegExp('^' + expectedPrefix + '(?:INPUT_RETURNED|ALREADY_VISIBLE)\\r?\\n', 'm');
  const visiblePattern = new RegExp('^' + visible + '\\r?\\n', 'm');
  const closedPattern = new RegExp('^' + closed + '\\r?\\n', 'm');
  const pending = { stdout: '', stderr: '' }, discard = { stdout: false, stderr: false };
  let guardError, rejectGuard, disposed = false, released = false;
  let prepAt, valueAt, closeAt;
  const guard = new Promise((_, reject) => { rejectGuard = reject; });
  void guard.catch(() => {});
  const fail = error => { if (!disposed && !guardError) { guardError = error; rejectGuard(error); } };
  const ensure = () => {
    if (guardError) throw guardError;
    phase.assertActive(); actor.assertBusinessActive();
  };
  const current = () => {
    ensure();
    demand(actor.recoveryShowPhase === scope && actor.selectedSuiteDeadline === receipt,
      'Finite recovery Show actor scope or receipt ended or changed');
    demand(Number.isFinite(phase.deadlineAt) && phase.deadlineAt <= initialPhaseDeadline,
      'Finite recovery Show phase deadline changed or renewed');
    assertRecoveryShowScope(scope, phase.contextFor('ios'));
  };
  const bound = deadline => Math.min(deadline, receipt.deadlineAt, phase.deadlineAt);
  const error = message => fail(new Error('Finite recovery Show ' + message));
  const timestamp = deadline => {
    const at = time();
    if (at >= bound(deadline)) { error('phase deadline exhausted'); return undefined; }
    return at;
  };
  const line = (text, sink) => {
    text = text.replace(/\r$/, '');
    const preparation = text.startsWith(prefix);
    if (!preparation && text !== visible && text !== closed) return;
    try {
      current();
      if (sink !== 'stdout' || !released) { error('event is outside its issued stdout phase'); return; }
      if (Buffer.byteLength(text + '\n') > 512) { error('event exceeds its bound'); return; }
      if (preparation) {
        if (prepAt !== undefined || (text !== expectedPrefix + 'INPUT_RETURNED' && text !== expectedPrefix + 'ALREADY_VISIBLE')) {
          error('preparation scope or order is invalid'); return;
        }
        prepAt = timestamp(prepDeadline);
      } else if (text === visible) {
        if (prepAt === undefined || valueAt !== undefined) { error('value preceded preparation or repeated'); return; }
        valueAt = timestamp(prepAt + 45_000);
      } else {
        if (valueAt === undefined || closeAt !== undefined) { error('close preceded value or repeated'); return; }
        closeAt = timestamp(valueAt + 60_000);
      }
    } catch (failure) { fail(failure); }
  };
  const push = (sink, chunk) => {
    if (disposed) return;
    let text = chunk.toString();
    if (discard[sink]) {
      const end = text.indexOf('\n');
      if (end < 0) return;
      discard[sink] = false; text = text.slice(end + 1);
    }
    const lines = (pending[sink] + text).split('\n'); pending[sink] = lines.pop();
    for (const value of lines) line(value, sink);
    if (Buffer.byteLength(pending[sink]) > 512) {
      if (pending[sink].startsWith(prefix) || pending[sink].startsWith(visible) || pending[sink].startsWith(closed)) error('partial event exceeds its bound');
      pending[sink] = ''; discard[sink] = true;
    }
  };
  const stdout = chunk => push('stdout', chunk), stderr = chunk => push('stderr', chunk);
  const end = () => { if (closeAt === undefined) error('stream ended before its close'); };
  actor.failure.then(() => {}, fail); phase.failure.then(() => {}, fail);
  if (peerFailure) peerFailure.then(() => {}, fail);
  const awaitMarker = async (marker, deadline, observed, fresh = false) => {
    ensure();
    if (observed()) return;
    current();
    const remaining = bound(deadline) - time();
    demand(Number.isFinite(remaining) && remaining > 0, 'Finite recovery Show phase deadline exhausted');
    const wait = fresh ? actor.waitForNextMarker(marker, remaining) : actor.waitForMarker(marker, remaining);
    void wait.catch(fail);
    await Promise.race([wait, guard]); ensure();
    demand(observed(), 'Finite recovery Show marker lacked a current ordered observation');
  };

  actor.child.stdout.on('data', stdout); actor.child.stderr.on('data', stderr); actor.child.stdout.on('end', end);
  try {
    const prepared = awaitMarker(prepPattern, prepDeadline, () => prepAt !== undefined, true);
    void prepared.catch(fail);
    await Promise.resolve(); current();
    for (const sink of ['stdout', 'stderr']) {
      discard[sink] = discard[sink] || pending[sink].length > 0;
      pending[sink] = '';
    }
    released = true; allowShow();
    await prepared;
    await awaitMarker(visiblePattern, prepAt + 45_000, () => valueAt !== undefined);
    await awaitMarker(closedPattern, valueAt + 60_000, () => closeAt !== undefined);
    ensure();
  } catch (failure) {
    let cause = guardError ?? failure;
    if (!guardError) {
      try { phase.assertActive(); actor.assertBusinessActive(); } catch (original) { cause = original; }
    }
    return await drain(cause);
  } finally {
    disposed = true; actor.child.stdout.off('data', stdout); actor.child.stderr.off('data', stderr);
    actor.child.stdout.off('end', end); pending.stdout = ''; pending.stderr = '';
  }
}
