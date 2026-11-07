// Test22-only phase boundary; original actor waits/timers/results remain the owners.
export async function runTest22IosShow({ actor, scope, allowShow, peerFailure, now = () => performance.now() }) {
  actor.assertBusinessActive();
  if (!scope || scope !== actor.test22ShowPhase || scope.testNumber !== 22 || scope.role !== 'sender'
      || !/^[0-9a-f]{32}$/.test(scope.nonce) || !Number.isInteger(scope.cycle)
      || scope.cycle < 1 || scope.cycle > 39 || scope.step !== 1) throw new Error('Invalid Test22 Show scope');
  actor.assertBusinessActive();
  const receipt = actor.selectedSuiteDeadline;
  if (!receipt || receipt.timeoutMs !== 600_000 || !Number.isFinite(receipt.deadlineAt)) {
    throw new Error('Test22 Show requires its armed ordinary600s actor');
  }
  const initialNow = now();
  if (!Number.isFinite(initialNow) || initialNow >= receipt.deadlineAt) throw new Error('Test22 Show actor deadline exhausted');
  const prepDeadline = Math.min(initialNow + 180_000, receipt.deadlineAt);
  const prefix = 'E2E: IOS_TEST22_SHOW_PREP_V1 ';
  const expectedPrefix = prefix + scope.nonce + ' ' + scope.cycle + ' ' + scope.step + ' ';
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
  const fail = error => {
    if (!disposed && !guardError) { guardError = error; rejectGuard(error); }
  };
  const ensure = () => { if (guardError) throw guardError; actor.assertBusinessActive(); };
  const current = () => {
    ensure();
    if (actor.selectedSuiteDeadline !== receipt) throw new Error('Test22 Show actor receipt ended or changed');
  };
  const error = message => fail(new Error('Test22 Show ' + message));
  const timestamp = deadline => {
    const at = now();
    if (!Number.isFinite(at) || at >= deadline) { error('phase deadline exhausted'); return undefined; }
    return at;
  };
  const line = (text, sink) => {
    text = text.replace(/\r$/, '');
    const preparation = text.startsWith(prefix);
    if (!preparation && text !== visible && text !== closed) return;
    try { current(); } catch (failure) { fail(failure); return; }
    if (sink !== 'stdout' || !released) { error('event is outside its issued stdout phase'); return; }
    if (Buffer.byteLength(text + '\n') > 512) { error('event exceeds its bound'); return; }
    if (preparation) {
      if (prepAt !== undefined || (text !== expectedPrefix + 'INPUT_RETURNED' && text !== expectedPrefix + 'ALREADY_VISIBLE')) {
        error('preparation scope or order is invalid'); return;
      }
      prepAt = timestamp(prepDeadline);
    } else if (text === visible) {
      if (prepAt === undefined || valueAt !== undefined) { error('value preceded preparation or repeated'); return; }
      valueAt = timestamp(Math.min(prepAt + 45_000, receipt.deadlineAt));
    } else {
      if (valueAt === undefined || closeAt !== undefined) { error('close preceded value or repeated'); return; }
      closeAt = timestamp(Math.min(valueAt + 60_000, receipt.deadlineAt));
    }
  };
  const push = (sink, chunk) => {
    if (disposed) return;
    let text = chunk.toString();
    if (discard[sink]) {
      const end = text.indexOf('\n');
      if (end < 0) return;
      discard[sink] = false; text = text.slice(end + 1);
    }
    const lines = (pending[sink] + text).split('\n');
    pending[sink] = lines.pop();
    for (const value of lines) line(value, sink);
    if (Buffer.byteLength(pending[sink]) > 512) {
      if (pending[sink].startsWith(prefix) || pending[sink].startsWith(visible) || pending[sink].startsWith(closed)) error('partial event exceeds its bound');
      pending[sink] = ''; discard[sink] = true;
    }
  };
  const stdout = chunk => push('stdout', chunk), stderr = chunk => push('stderr', chunk);
  const end = () => { if (closeAt === undefined) error('stream ended before its close'); };
  // Observe already-settled owner/peer failure before issuing the release.
  actor.failure.then(() => {}, fail);
  if (peerFailure) peerFailure.then(() => {}, fail);
  const awaitMarker = async (marker, deadline, observed, fresh = false) => {
    ensure();
    // A genuinely ordered line already delivered in this phase needs no new timer after terminal flush.
    if (observed()) return;
    current();
    const remaining = Math.min(deadline, receipt.deadlineAt) - now();
    if (!Number.isFinite(remaining) || remaining <= 0) throw new Error('Test22 Show phase deadline exhausted');
    const wait = fresh ? actor.waitForNextMarker(marker, remaining) : actor.waitForMarker(marker, remaining);
    await Promise.race([wait, guard]);
    ensure();
    if (!observed()) throw new Error('Test22 Show marker lacked a current ordered observation');
  };

  actor.child.stdout.on('data', stdout); actor.child.stderr.on('data', stderr);
  actor.child.stdout.on('end', end);
  try {
    const prepared = awaitMarker(prepPattern, prepDeadline, () => prepAt !== undefined, true);
    void prepared.catch(() => {});
    await Promise.resolve();
    current();
    // The original release occurs exactly once, after the fresh observer/wait is armed.
    released = true; allowShow();
    await prepared;
    await awaitMarker(visiblePattern, prepAt + 45_000, () => valueAt !== undefined);
    await awaitMarker(closedPattern, valueAt + 60_000, () => closeAt !== undefined);
    ensure();
  } catch (failure) {
    let cause = guardError ?? failure;
    if (!guardError) { try { actor.assertBusinessActive(); } catch (original) { cause = original; } }
    try { await actor.cancelAndJoin(cause); }
    catch (cleanup) {
      throw cleanup === cause ? cause : new AggregateError([cause, cleanup], 'Test22 Show failed and its actor did not drain', { cause });
    }
    throw cause;
  } finally {
    disposed = true;
    actor.child.stdout.off('data', stdout); actor.child.stderr.off('data', stderr);
    actor.child.stdout.off('end', end);
    pending.stdout = ''; pending.stderr = '';
  }
}
