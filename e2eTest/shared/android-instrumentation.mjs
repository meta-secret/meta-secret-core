export function quotePosixShellArgument(rawValue) {
  const value = String(rawValue);
  if (value.includes('\0')) {
    throw new Error('Android shell arguments cannot contain NUL bytes');
  }
  return `'${value.replaceAll("'", `'"'"'`)}'`;
}

export function serializePosixShellCommand(argumentsList) {
  if (!Array.isArray(argumentsList) || argumentsList.length === 0) {
    throw new Error('A non-empty remote shell argument list is required');
  }
  return argumentsList.map(quotePosixShellArgument).join(' ');
}

/**
 * `adb shell` reparses its command through the remote POSIX shell. Passing
 * values as separate host argv entries therefore does not preserve spaces or
 * metacharacters. Send one fully quoted remote command instead.
 */
export function androidInstrumentationAdbArguments(
  serial,
  instrumentationArguments,
  runnerComponent,
) {
  if (!serial || !runnerComponent) {
    throw new Error('Android instrumentation requires a serial and runner component');
  }
  return [
    '-s',
    String(serial),
    'shell',
    '--',
    serializePosixShellCommand([
      'am',
      'instrument',
      '-w',
      ...instrumentationArguments.map(String),
      String(runnerComponent),
    ]),
  ];
}

export function androidInstrumentationDiagnosticArguments(
  serial,
  instrumentationArguments,
  runnerComponent,
  redactArguments,
) {
  if (typeof redactArguments !== 'function') {
    throw new Error('Android instrumentation diagnostics require an argument redactor');
  }
  const logicalRemoteArguments = [
    'am',
    'instrument',
    '-w',
    ...instrumentationArguments.map(String),
    String(runnerComponent),
  ];
  const safeRemoteArguments = redactArguments(logicalRemoteArguments).map(String);
  return [
    '-s',
    String(serial),
    'shell',
    '--',
    serializePosixShellCommand(safeRemoteArguments),
  ];
}

/**
 * Android E2E markers arrive through logcat, while pass/fail belongs to the
 * instrumentation process. Once instrumentation settles, logcat is stopped
 * deliberately. Keep that expected SIGTERM from replacing the authoritative
 * instrumentation result for marker waiters or public callers.
 */
export function superviseAndroidInstrumentation({
  instrumentationResult,
  logcat,
  selector,
  validateOutput = () => {},
}) {
  let primaryOutcome;
  let resolvePrimaryOutcome;
  const primaryOutcomePromise = new Promise((resolve) => {
    resolvePrimaryOutcome = resolve;
  });

  const result = Promise.resolve(instrumentationResult)
    .then((output) => {
      validateOutput(output, selector);
      return output;
    })
    .then(
      (output) => {
        primaryOutcome = { ok: true, output };
        resolvePrimaryOutcome(primaryOutcome);
        return output;
      },
      (error) => {
        primaryOutcome = { ok: false, error };
        resolvePrimaryOutcome(primaryOutcome);
        throw error;
      },
    )
    .finally(() => {
      if (!logcat.child.killed) logcat.child.kill('SIGTERM');
    });

  // Callers frequently wait for markers before awaiting `result`. Observe the
  // rejection now while retaining the same authoritative promise for them.
  void result.catch(() => {});
  void logcat.result.catch(() => {});

  const waitForMarker = async (marker, timeoutMs) => {
    try {
      return await Promise.race([
        logcat.waitForMarker(marker, timeoutMs),
        primaryOutcomePromise.then((outcome) => {
          if (!outcome.ok) throw outcome.error;
          throw new Error(`Android instrumentation ${selector} completed before marker ${marker}`);
        }),
      ]);
    } catch (error) {
      if (primaryOutcome && !primaryOutcome.ok) throw primaryOutcome.error;
      throw error;
    }
  };

  return { ...logcat, waitForMarker, result };
}
