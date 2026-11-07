/** Test-only lifecycle for one runner-owned XCTest/xcodebuild invocation. */
export function superviseIosSelectedSuite(options) {
  return createIosSelectedSuiteActor(options).result;
}

export function createIosSelectedSuiteActor({ watched, selector, timeoutMs,
  naturalExitMs = 5_000, closeTimeoutMs = 5_000, killTimeoutMs = 2_000, onPassed = () => {},
  deferCompletionTimeout = false }) {
  let firstFailure, rejectFailure, rejectDeadline, completionTimer;
  let selectedSuiteDeadline;
  let terminalSuite = false;
  let intentionallyTerminated = false;
  let completionStarted = false;
  const failure = new Promise((_, reject) => { rejectFailure = reject; });
  const deadline = new Promise((_, reject) => { rejectDeadline = reject; });
  void failure.catch(() => {});
  void deadline.catch(() => {});
  const preserve = error => {
    if (!firstFailure) { firstFailure = error; rejectFailure(error); }
    return firstFailure;
  };
  const assertBusinessActive = () => { if (firstFailure) throw firstFailure; };
  // Observe real process failure immediately, without turning our own bounded
  // post-terminal wrapper termination into another actor failure.
  void watched.result.catch(error => { if (!intentionallyTerminated) preserve(error); });

  const within = async (promise, deadlineMs) => {
    let timer;
    try {
      return await Promise.race([promise.then(() => true), new Promise((resolve) => {
        timer = setTimeout(() => resolve(false), deadlineMs);
      })]);
    } finally { clearTimeout(timer); }
  };
  const stopAndJoin = async () => {
    // Result writers can still be flushing attachments after the terminal line.
    if (terminalSuite && await within(watched.closed, naturalExitMs)) return;
    if (watched.child.exitCode == null && watched.child.signalCode == null) {
      intentionallyTerminated = watched.child.kill('SIGTERM') || intentionallyTerminated;
    }
    if (await within(watched.closed, closeTimeoutMs)) return;
    intentionallyTerminated = watched.child.kill('SIGKILL') || intentionallyTerminated;
    if (!await within(watched.closed, killTimeoutMs)) {
      throw new Error(`Owned xcodebuild did not close after terminal XCTest result: ${selector}`);
    }
  };

  const result = (async () => {
    try {
      // Explicit-mode observations have no timer. Their exact owner must call
      // complete or cancel; ordinary callers retain their original deadlines.
      const markerDeadline = deferCompletionTimeout ? null : timeoutMs;
      // Read-only receipt from immediately before the earliest original ordinary arm.
      if (markerDeadline !== null) selectedSuiteDeadline = Object.freeze({
        timeoutMs: markerDeadline, deadlineAt: performance.now() + markerDeadline,
      });
      const outcome = await Promise.race([
        watched.waitForMarker("Test Suite 'Selected tests' failed", markerDeadline).then(() => 'failed'),
        watched.waitForMarker("Test Suite 'Selected tests' passed", markerDeadline).then(() => 'passed'),
        failure,
        deadline,
      ]);
      terminalSuite = true;
      if (outcome === 'failed') {
        const error = new Error(`XCTest selected suite failed: ${selector}; failure .xcresult retained`);
        error.code = 'IOS_SELECTED_SUITE_FAILED';
        throw error;
      }
    } catch (error) { preserve(error); }
    finally { clearTimeout(completionTimer); }

    const terminal = firstFailure ?? new Error(`XCTest selected suite passed; owned wrapper closing: ${selector}`);
    watched.cancelPendingWaits(terminal);
    try { await stopAndJoin(); }
    catch (cleanupError) {
      if (firstFailure) throw new AggregateError([firstFailure, cleanupError],
        'XCTest failed and its owned wrapper did not close', { cause: firstFailure });
      throw cleanupError;
    }
    if (firstFailure) throw firstFailure;
    if (!intentionallyTerminated) await watched.result;
    await onPassed();
    return { stdout: watched.outputTail(), stderr: '' };
  })();
  void result.catch(error => preserve(error));
  return { result, failure, assertBusinessActive,
    get selectedSuiteDeadline() {
      assertBusinessActive();
      return terminalSuite || deferCompletionTimeout ? null : selectedSuiteDeadline ?? null;
    },
    complete() {
      if (deferCompletionTimeout && !completionStarted && !terminalSuite && !firstFailure) {
        completionStarted = true;
        completionTimer = setTimeout(() => rejectDeadline(new Error(
          `Timed out waiting for Test Suite 'Selected tests' passed: ${selector}`)), timeoutMs);
      }
      return result;
    },
    async cancelAndJoin(error) {
      if (error || !terminalSuite) {
        preserve(error ?? new Error(`iOS actor cancelled before its selected-suite result: ${selector}`));
        watched.cancelPendingWaits(firstFailure);
      }
      // Terminal flush belongs to the original actor, never to a sibling kill.
      await result.catch(failure => {
        if (failure instanceof AggregateError || (watched.child.exitCode == null && watched.child.signalCode == null)) throw failure;
      });
    },
  };
}
