/** Test-only lifecycle for one runner-owned XCTest/xcodebuild invocation. */
export function superviseIosSelectedSuite(options) {
  return createIosSelectedSuiteActor(options).result;
}

export function createIosSelectedSuiteActor({ watched, selector, timeoutMs,
  naturalExitMs = 5_000, closeTimeoutMs = 5_000, killTimeoutMs = 2_000, onPassed = () => {} }) {
  let firstFailure;
  let terminalSuite = false;
  let intentionallyTerminated = false;
  const preserve = (error) => firstFailure ??= error;
  // Intentional wrapper termination after a terminal suite line must not turn
  // its lower-level process rejection into an unhandled/false actor failure.
  void watched.result.catch(() => {});

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
    let outcome;
    try {
      // Arm both before any continuation. A failed selected suite is terminal
      // even when xcodebuild/testmanagerd remains alive after app termination.
      outcome = await Promise.race([
        watched.waitForMarker("Test Suite 'Selected tests' failed", timeoutMs).then(() => 'failed'),
        watched.waitForMarker("Test Suite 'Selected tests' passed", timeoutMs).then(() => 'passed'),
      ]);
      terminalSuite = true;
      if (outcome === 'failed') {
        const error = new Error(`XCTest selected suite failed: ${selector}; failure .xcresult retained`);
        error.code = 'IOS_SELECTED_SUITE_FAILED';
        throw error;
      }
    } catch (error) { preserve(error); }

    // Cancel all actor marker waits with the first selected-suite/process
    // failure before terminating the wrapper; SIGTERM cannot replace it.
    const terminal = firstFailure ?? new Error(`XCTest selected suite passed; owned wrapper closing: ${selector}`);
    watched.cancelPendingWaits(terminal);
    try { await stopAndJoin(); }
    catch (cleanupError) {
      if (firstFailure) throw new AggregateError([firstFailure, cleanupError],
        'XCTest failed and its owned wrapper did not close', { cause: firstFailure });
      throw cleanupError;
    }
    if (firstFailure) throw firstFailure;
    // A passing test cannot hide a natural result-writer/process failure.
    // Only our own termination after the flush deadline is intentional.
    if (!intentionallyTerminated) await watched.result;
    await onPassed();
    return { stdout: watched.outputTail(), stderr: '' };
  })();
  void result.catch(() => {});
  return { result, async cancelAndJoin() {
    // A sibling's failure may reach cancellation before the selected suite.
    // Once terminal, join the existing bounded flush instead of killing it early.
    if (!terminalSuite) watched.cancelPendingWaits(new Error(`iOS actor cancelled before its selected-suite result: ${selector}`));
    await result.catch((error) => {
      if (watched.child.exitCode == null && watched.child.signalCode == null) throw error;
    });
  } };
}
