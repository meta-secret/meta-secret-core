import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import {
  androidInstrumentationAdbArguments,
  androidInstrumentationDiagnosticArguments,
  serializePosixShellCommand,
  superviseAndroidInstrumentation,
} from './android-instrumentation.mjs';
import { createDiagnosticRedactor } from './diagnostic-redaction.mjs';

test('remote POSIX command round-trips spaces, quotes, commas, Unicode, and metacharacters', () => {
  const expected = [
    'Chrome on MacIntel',
    "single'quote",
    'double"quote',
    'comma,value',
    'Юникод 🚀',
    '; printf SHELL_INJECTION >&2',
    '$(printf COMMAND_SUBSTITUTION >&2)',
    '`printf BACKTICK_INJECTION >&2`',
    'line one\nline two',
  ];
  const command = serializePosixShellCommand([
    process.execPath,
    '-e',
    'process.stdout.write(JSON.stringify(process.argv.slice(1)))',
    '--',
    ...expected,
  ]);
  const completed = spawnSync('/bin/sh', ['-c', command], { encoding: 'utf8' });

  assert.equal(completed.status, 0);
  assert.equal(completed.stderr, '');
  assert.deepEqual(JSON.parse(completed.stdout), expected);
});

test('adb builder uses shell -- and round-trips through a fake adb executable', () => {
  const fixtureDirectory = mkdtempSync(join(tmpdir(), 'metasecret-fake-adb-'));
  try {
    const fakeAdb = join(fixtureDirectory, 'adb');
    const fakeAm = join(fixtureDirectory, 'am');
    writeFileSync(fakeAdb, `#!/bin/sh
test "$1" = "-s" || exit 21
shift 2
test "$1" = "shell" || exit 22
shift
test "$1" = "--" || exit 23
shift
test "$#" -eq 1 || exit 24
exec /bin/sh -c "$1"
`, { mode: 0o755 });
    writeFileSync(fakeAm, `#!${process.execPath}
process.stdout.write(JSON.stringify(process.argv.slice(2)));
`, { mode: 0o755 });
    const values = [
      'Chrome on MacIntel',
      "single'quote",
      'comma,value',
      'Юникод 🚀',
      '$(printf NOT_EXECUTED >&2)',
      'line one\nline two',
    ];
    const runner = 'metasecret.project.com/androidx.test.runner.AndroidJUnitRunner';
    const instrumentationArguments = values.flatMap((value, index) => [
      '-e', `fixture${index}`, value,
    ]);
    const adbArguments = androidInstrumentationAdbArguments(
      'emulator-5554',
      instrumentationArguments,
      runner,
    );

    assert.deepEqual(adbArguments.slice(0, 4), ['-s', 'emulator-5554', 'shell', '--']);
    assert.equal(adbArguments.length, 5);
    assert.match(adbArguments[4], /^'am' 'instrument' '-w' /);
    const completed = spawnSync(fakeAdb, adbArguments, {
      encoding: 'utf8',
      env: { ...process.env, PATH: `${fixtureDirectory}:${process.env.PATH ?? ''}` },
    });
    assert.equal(completed.status, 0, completed.stderr);
    assert.equal(completed.stderr, '');
    assert.deepEqual(
      JSON.parse(completed.stdout),
      ['instrument', '-w', ...instrumentationArguments, runner],
    );
  } finally {
    rmSync(fixtureDirectory, { recursive: true, force: true });
  }
});

test('failure diagnostic command contains only pre-serialization redactions', () => {
  const values = {
    secretValue: "SECRET_FRAGMENT_ALPHA ' spaces $(not-run)\nSECRET_FRAGMENT_OMEGA",
    privateKey: "PRIVATE_FRAGMENT ' $(not-run)",
    master_key: 'MASTER_FRAGMENT with spaces',
    keyShare: 'SHARE_FRAGMENT,$(not-run)',
  };
  const logicalArguments = Object.entries(values)
    .flatMap(([key, value]) => ['-e', key, value]);
  const runner = 'metasecret.project.com/androidx.test.runner.AndroidJUnitRunner';
  const redactor = createDiagnosticRedactor([values.secretValue]);
  const executionArguments = androidInstrumentationAdbArguments(
    'emulator-5554',
    logicalArguments,
    runner,
  );
  const diagnosticArguments = androidInstrumentationDiagnosticArguments(
    'emulator-5554',
    logicalArguments,
    runner,
    redactor.redactArguments,
  );
  const failureDiagnostic = `adb ${diagnosticArguments.join(' ')}`;

  assert.deepEqual(executionArguments.slice(0, 4), ['-s', 'emulator-5554', 'shell', '--']);
  assert.deepEqual(diagnosticArguments.slice(0, 4), ['-s', 'emulator-5554', 'shell', '--']);
  assert.match(executionArguments[4], /SECRET_FRAGMENT_ALPHA/);
  for (const sensitiveValue of Object.values(values)) {
    assert.doesNotMatch(failureDiagnostic, new RegExp(sensitiveValue.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
  }
  for (const fragment of [
    'SECRET_FRAGMENT_ALPHA',
    'SECRET_FRAGMENT_OMEGA',
    'PRIVATE_FRAGMENT',
    'MASTER_FRAGMENT',
    'SHARE_FRAGMENT',
    'not-run',
  ]) {
    assert.doesNotMatch(failureDiagnostic, new RegExp(fragment));
  }
  assert.equal(failureDiagnostic.match(/\[REDACTED\]/g)?.length, 4);
});

test('primary instrumentation failure wins over expected logcat termination', async () => {
  const primaryError = new Error('am instrument exited with 1: Invalid userId -2');
  let rejectInstrumentation;
  let rejectLogcat;
  const instrumentationResult = new Promise((_, reject) => {
    rejectInstrumentation = reject;
  });
  const logcatResult = new Promise((_, reject) => {
    rejectLogcat = reject;
  });
  const logcat = {
    child: {
      killed: false,
      kill(signal) {
        this.killed = true;
        rejectLogcat(new Error(`adb exited with ${signal}`));
      },
    },
    result: logcatResult,
    waitForMarker: () => logcatResult,
  };
  const supervised = superviseAndroidInstrumentation({
    instrumentationResult,
    logcat,
    selector: 'ExampleTest#example',
  });
  const marker = supervised.waitForMarker('E2E: NEVER_EMITTED', 1_000);

  rejectInstrumentation(primaryError);

  await assert.rejects(supervised.result, (error) => error === primaryError);
  await assert.rejects(marker, (error) => error === primaryError);
  assert.equal(logcat.child.killed, true);
});

test('successful instrumentation exit reports missing marker instead of logcat SIGTERM', async () => {
  let resolveInstrumentation;
  let rejectLogcat;
  const instrumentationResult = new Promise((resolve) => {
    resolveInstrumentation = resolve;
  });
  const logcatResult = new Promise((_, reject) => {
    rejectLogcat = reject;
  });
  const logcat = {
    child: {
      killed: false,
      kill(signal) {
        this.killed = true;
        rejectLogcat(new Error(`adb exited with ${signal}`));
      },
    },
    result: logcatResult,
    waitForMarker: () => logcatResult,
  };
  const supervised = superviseAndroidInstrumentation({
    instrumentationResult,
    logcat,
    selector: 'ExampleTest#example',
  });
  const marker = supervised.waitForMarker('E2E: NEVER_EMITTED', 1_000);
  const primaryOutput = { stdout: 'OK', stderr: '' };

  resolveInstrumentation(primaryOutput);

  assert.equal(await supervised.result, primaryOutput);
  await assert.rejects(
    marker,
    /Android instrumentation ExampleTest#example completed before marker E2E: NEVER_EMITTED/,
  );
  assert.equal(logcat.child.killed, true);
});
