// Exact25 TEST procedure evidence; never approval, plaintext or actor success.
const scopeKeys = ['testNumber', 'role', 'nonce', 'cycle', 'step', 'runId', 'contextId', 'invocationId'];
const senderCycles = [19, 20, 21, 28, 29, 30, 37, 38, 39];
const hex = value => typeof value === 'string' && /^[0-9a-f]{32}(?![\s\S])/.test(value);
const demand = (value, message) => { if (!value) throw new Error(message); };

export function assertTest25ShowScope(scope, expected) {
  demand(scope && Object.getPrototypeOf(scope) === Object.prototype && Object.isFrozen(scope), 'Exact frozen Test25 Show scope required');
  const keys = Reflect.ownKeys(scope);
  demand(keys.length === scopeKeys.length && keys.every((key, index) => key === scopeKeys[index])
    && keys.every(key => {
      const field = Object.getOwnPropertyDescriptor(scope, key);
      return Object.hasOwn(field, 'value') && field.enumerable === true;
    }),
  'Exact Test25 Show data fields required');
  demand(expected && expected.version === 1 && expected.testNumber === 25
    && expected.mode === 'web-secondary-creator-lifecycle' && expected.platform === 'ios'
    && expected.role === 'sender' && expected.logicalStep === 1
    && Number.isInteger(expected.cycle) && senderCycles.includes(expected.cycle), 'Current Test25 iOS sender required');
  demand(scope.testNumber === 25 && scope.role === 'sender' && hex(scope.nonce)
    && Number.isInteger(scope.cycle) && scope.cycle === expected.cycle && scope.step === 1
    && typeof scope.runId === 'string' && /^[A-Za-z0-9][A-Za-z0-9_.-]{0,79}(?![\s\S])/.test(scope.runId)
    && scope.runId === expected.runId && hex(scope.contextId) && hex(scope.invocationId)
    && scope.contextId === expected.contextId && scope.invocationId === expected.invocationId,
  'Test25 Show scope conflicts with the current independent invocation');
  return scope;
}

export async function runTest25IosShow({ actor, scope, phase, allowShow, peerFailure, now = () => performance.now() }) {
  const drain = async cause => {
    if (typeof actor?.cancelAndJoin === 'function') {
      try { await actor.cancelAndJoin(cause); }
      catch (cleanup) {
        throw cleanup === cause ? cause : new AggregateError([cause, cleanup],
          'Test25 Show failed and its actor did not drain', { cause });
      }
    }
    throw cause;
  };
  let receipt, initialPhaseDeadline, previousTime, prepDeadline;
  const time = () => {
    const at = now();
    demand(Number.isFinite(at) && (previousTime === undefined || at >= previousTime), 'Test25 Show clock is invalid');
    previousTime = at; return at;
  };
  try {
    demand(actor && phase && typeof allowShow === 'function', 'Current Test25 Show owner and phase required');
    phase.assertActive(); actor.assertBusinessActive();
    demand(scope === actor.test25ShowPhase, 'Same owned Test25 Show scope required');
    assertTest25ShowScope(scope, phase.contextFor('ios'));
    receipt = actor.selectedSuiteDeadline; initialPhaseDeadline = phase.deadlineAt;
    demand(receipt && receipt.timeoutMs === 600_000 && Number.isFinite(receipt.deadlineAt)
      && Number.isFinite(initialPhaseDeadline) && initialPhaseDeadline <= receipt.deadlineAt,
    'Test25 Show requires its armed ordinary600s and current phase deadline');
    const start = time();
    demand(start < Math.min(receipt.deadlineAt, initialPhaseDeadline), 'Test25 Show deadline exhausted');
    prepDeadline = Math.min(start + 180_000, receipt.deadlineAt, initialPhaseDeadline);
  } catch (failure) { return drain(failure); }

  const prefix = 'E2E: IOS_TEST25_SHOW_PREP_V1 ';
  const expectedPrefix = prefix + scope.nonce + ' ' + scope.contextId + ' ' + scope.invocationId + ' ' + scope.cycle + ' ' + scope.step + ' ';
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
    demand(actor.test25ShowPhase === scope && actor.selectedSuiteDeadline === receipt,
      'Test25 Show actor scope or receipt ended or changed');
    demand(Number.isFinite(phase.deadlineAt) && phase.deadlineAt <= initialPhaseDeadline,
      'Test25 Show phase deadline changed or renewed');
    assertTest25ShowScope(scope, phase.contextFor('ios'));
  };
  const bound = deadline => Math.min(deadline, receipt.deadlineAt, phase.deadlineAt);
  const error = message => fail(new Error('Test25 Show ' + message));
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
    demand(Number.isFinite(remaining) && remaining > 0, 'Test25 Show phase deadline exhausted');
    const wait = fresh ? actor.waitForNextMarker(marker, remaining) : actor.waitForMarker(marker, remaining);
    void wait.catch(fail);
    await Promise.race([wait, guard]); ensure();
    demand(observed(), 'Test25 Show marker lacked a current ordered observation');
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
