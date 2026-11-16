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
  instrumentationArgumentsFromGradle,
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

test('F2 current: direct Android actor dispatch preserves original selector and quoted identity/route arguments', () => {
  const selector = 'metasecret.project.com.CaseEightAndroidBothOfflineTest#handleRecoveryStep';
  const values = [
    ['class', selector], ['role', 'receiver'], ['cycle', '18'], ['step', 'approve'],
    ['expectedIosDeviceName', 'iPhone18,3'], ['expectedWebDeviceName', 'Chrome on MacIntel'],
    ['vaultName', "fixture ' ; $(not-run) 🚀"], ['decision', 'approve'],
    ['requiredE2eServerUrl', 'http://10.0.2.2:5201'], ['approvalCoordinatorUrl', 'http://10.0.2.2:5180'],
  ];
  const args = values.flatMap(([key, value]) => ['-e', key, value]);
  assert.deepEqual(instrumentationArgumentsFromGradle([
    ':composeApp:installDebug', ':composeApp:connectedDebugAndroidTest',
    ...values.map(([key, value]) => `-Pandroid.testInstrumentationRunnerArguments.${key}=${value}`),
    '-PMETA_SECRET_ENV=local',
  ]), args, 'Existing Gradle actor arguments must map losslessly to direct instrumentation');
  const actual = androidInstrumentationAdbArguments('emulator-5554', args,
    'metasecret.project.com.test/androidx.test.runner.AndroidJUnitRunner');
  assert.deepEqual(actual.slice(0, 4), ['-s', 'emulator-5554', 'shell', '--']);
  assert.equal(actual.length, 5); assert.match(actual[4], /^'am' 'instrument' '-w' '-e' 'class'/);
  for (const [key, value] of values) {
    assert(actual[4].includes(`'-e' '${key}' '${value.replaceAll("'", `'"'"'`)}'`), `Argument ${key} changed in handoff`);
  }
  assert.doesNotMatch(actual[4], /connectedDebugAndroidTest|installDebug|gradlew/);
});

test('F2 deferred: shared authoritative output validator rejects original instrumentation failure forms', async () => {
  const { assertAndroidInstrumentationPassed } = await import('./android-instrumentation.mjs');
  for (const output of [
    { stdout: 'FAILURES!!!\nTests run: 1, Failures: 1' },
    { stderr: 'There was 1 failure' }, { stdout: 'There were 2 failures' },
    { stdout: 'INSTRUMENTATION_STATUS_CODE: -2' },
  ]) assert.throws(() => assertAndroidInstrumentationPassed(output, 'OriginalTest#originalMethod'), /OriginalTest#originalMethod/);
  assert.doesNotThrow(() => assertAndroidInstrumentationPassed({ stdout: 'OK (1 test)\nINSTRUMENTATION_CODE: -1' }, 'OriginalTest#originalMethod'));
});

async function actualAndroidJoinCapture(number = 4, block = 1, configure = () => {}) {
  const { readFileSync } = await import('node:fs');
  const { runInNewContext } = await import('node:vm');
  const { assertAndroidInstrumentationPassed } = await import('./android-instrumentation.mjs');
  const file = new URL(`../Test${number}/scenario${number === 4 ? '-block' + block : ''}.json`, import.meta.url);
  const scenario = JSON.parse(readFileSync(file, 'utf8')), input = { reuse: true, options: {}, auth: undefined, passcode: undefined };
  configure(scenario, input);
  const text = readFileSync(new URL('../Test4/orchestrator.mjs', import.meta.url), 'utf8');
  const first = text.indexOf('async function startAndroidJoinTest('), last = text.indexOf('\n}\n', first) + 2;
  const normalizers = text.slice(text.indexOf('function secretConfigFor('), text.indexOf('const projectRoot ='));
  const secretConfigs = scenario.secrets ?? { default: scenario.secret }, captured = { calls: [], direct: null, gradle: null };
  const logcat = { child: { killed: false, kill() { this.killed = true; } },
    result: new Promise(() => {}), cancelPendingWaits() {}, waitForMarker: () => new Promise(() => {}) };
  const output = { stdout: 'OK (1 test)', stderr: '' };
  const context = {
    scenario, secretConfigs, defaultSecret: Object.values(secretConfigs)[0], defaultSecretKey: Object.keys(secretConfigs)[0],
    batchProducts: {}, coreRoot: '/controlled/core', composeRoot: '/controlled/compose', console: { log() {} },
    reuseProvenArtifacts: input.reuse, diagnosticAndroidVerbose: false, networkLossConfig: null, deletionAuthProfile: input.auth,
    declaredDevicePasscodes: { android: input.passcode }, androidAppApkPath: '/controlled/app.apk',
    androidTestApkPath: '/controlled/test.apk', androidTestBundleId: scenario.android.bundleId + '.test',
    assertBatchProductsCurrent() { captured.calls.push('current-products'); },
    waitForAndroidBoot: async () => captured.calls.push('boot'),
    runAdbWithRetry: async args => { captured.calls.push(args.includes('install') ? 'install' : 'clear-logcat'); },
    watchProcessOutput: () => logcat,
    verifyAndroidApkFfiProvenance: async () => captured.calls.push('verify-apks'),
    buildAndroidFfiFromCurrentSource: async () => captured.calls.push('build'),
    recordFreshBuildProvenance: async () => captured.calls.push('record'),
    captureInstalledAndroidAppProvenance: async () => captured.calls.push('installed-proof'),
    dismissAndroidCompatibilityDialogOnce: async () => captured.calls.push('compatibility'),
    writeDiagnostic() {}, networkProxyUrl: () => 'http://controlled-route',
    runAndWait: async (_file, args) => {
      captured.calls.push('gradle');
      if (args.includes(':composeApp:connectedDebugAndroidTest')) captured.gradle = Array.from(args);
      return output;
    },
    watchAndroidInstrumentation: (serial, args, runner) => {
      assert.equal(serial, 'controlled-serial'); captured.direct = { args: Array.from(args), runner };
      return { result: Promise.resolve(output) };
    },
    superviseAndroidInstrumentation, assertAndroidInstrumentationPassed,
  };
  const actual = runInNewContext(normalizers + '\n' + text.slice(first, last)
    + '\n({ enter: startAndroidJoinTest, plan: cyclePlanJson, config: secretConfigJson, creation: JSON.stringify(secretCreationPlan) });', context);
  const actor = await actual.enter('controlled-serial', input.options); await actor.result;
  return { ...captured, expected: actual, scenario, logcat };
}

function decodedDirectRoundtrip(t, invocation) {
  const directory = mkdtempSync(join(tmpdir(), 'test4-plan-fake-adb-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const adb = join(directory, 'adb'), am = join(directory, 'am');
  writeFileSync(adb, '#!/bin/sh\ntest "$1" = "-s" || exit 21\nshift 2\ntest "$1" = "shell" || exit 22\nshift\ntest "$1" = "--" || exit 23\nshift\ntest "$#" -eq 1 || exit 24\nexec /bin/sh -c "$1"\n', { mode: 0o755 });
  writeFileSync(am, `#!${process.execPath}\nprocess.stdout.write(JSON.stringify(process.argv.slice(2)));\n`, { mode: 0o755 });
  const args = androidInstrumentationAdbArguments('controlled-serial', invocation.args, invocation.runner);
  const done = spawnSync(adb, args, { encoding: 'utf8', env: { ...process.env, PATH: directory + ':' + (process.env.PATH ?? '') } });
  assert.equal(done.status, 0, 'Owned fake adb/am transport must close naturally');
  assert.equal(done.stderr.length, 0, 'Owned fake transport must emit no error text');
  const decoded = JSON.parse(done.stdout), params = new Map();
  assert.equal(decoded[0], 'instrument'); assert.equal(decoded[1], '-w');
  assert.equal(decoded.at(-1) === invocation.runner, true, 'Runner identity must survive quoting');
  for (let i = 2; i < decoded.length - 1; i += 3) {
    assert.equal(decoded[i], '-e'); assert.equal(params.has(decoded[i + 1]), false, 'Each instrumentation key must be unique');
    params.set(decoded[i + 1], decoded[i + 2]);
  }
  return { params, args };
}

test('actual direct Test4 Android invocation carries six recovery cycles through the existing serializer', async t => {
  const f = await actualAndroidJoinCapture(), { params } = decodedDirectRoundtrip(t, f.direct);
  assert.equal(JSON.parse(params.get('cyclePlan') ?? '[]').length, 6,
    'Actual direct Android handoff must retain six canonical recovery cycles');
  assert.equal(params.get('cyclePlan') === f.expected.plan, true, 'Recovery plan must be byte-exact');
  assert.equal(params.get('class'), 'metasecret.project.com.CaseFourAndroidConcurrentRecoveryTest#createVaultThenHandleConcurrentRecovery');
  assert.deepEqual(f.calls, ['current-products', 'boot', 'clear-logcat', 'verify-apks',
    'install', 'install', 'installed-proof', 'compatibility']);
});

test('actual three Test4 blocks retain all canonical consumer fields and exact requester roles', async t => {
  for (const block of [1, 2, 3]) {
    const direct = await actualAndroidJoinCapture(4, block), { params } = decodedDirectRoundtrip(t, direct.direct);
    const legacy = await actualAndroidJoinCapture(4, block, (_scenario, input) => { input.reuse = false; });
    const gradle = new Map();
    for (const arg of legacy.gradle) if (arg.startsWith('-Pandroid.testInstrumentationRunnerArguments.')) {
      const pair = arg.slice('-Pandroid.testInstrumentationRunnerArguments.'.length), at = pair.indexOf('=');
      gradle.set(pair.slice(0, at), pair.slice(at + 1));
    }
    for (const key of ['cyclePlan', 'secretConfig', 'secretCreationPlan', 'recoveryCycles'])
      assert.equal(params.has(key) && params.get(key) === gradle.get(key), true, 'Canonical consumer field changed: ' + key);
    const plan = JSON.parse(params.get('cyclePlan'));
    assert.equal(plan.length, 6); assert.equal(plan.reduce((sum, cycle) => sum + cycle.senders.length, 0), 12);
    assert.equal(params.get('cyclePlan') === direct.expected.plan, true, 'Exact original role/approval plan must survive');
    assert.equal(params.get('secretValue') === Object.values(direct.scenario.secrets ?? { default: direct.scenario.secret })[0].value, true,
      'Direct scalar value must remain present; legacy Gradle does not supply this key');
    assert.equal(params.get('setupOnly'), 'false'); assert.equal(direct.logcat.child.killed, true);
  }
});

test('actual direct setup auth and quoted structured values preserve scalar compatibility and redaction', async t => {
  const setup = await actualAndroidJoinCapture(7, 1, (scenario, input) => { scenario.android.setupOnly = true; input.reuse = false; });
  const setupParams = decodedDirectRoundtrip(t, setup.direct).params;
  assert.equal(setupParams.get('setupOnly'), 'true');
  assert.equal(setupParams.get('class') === setup.scenario.android.testClass + '#' + setup.scenario.android.testMethod, true);
  assert.equal(setupParams.get('secretNames') === Object.values(setup.scenario.secrets ?? { default: setup.scenario.secret }).map(x => x.name).join(','), true);
  assert.equal(setupParams.get('secretValues') === Object.values(setup.scenario.secrets ?? { default: setup.scenario.secret }).map(x => x.value).join(','), true);
  const canary = "PRIVATE_TRANSPORT_CANARY ' \" , Юникод 🚀 $(not-run)\nend";
  const subject = 'CONTROLLED_SUBJECT_Юникод';
  const f = await actualAndroidJoinCapture(4, 1, (scenario, input) => {
    scenario.secret = { ...scenario.secret, name: subject, value: canary }; scenario.mode = 'vault-secret-deletion';
    scenario.secretCreation = { initial: { android: ['default'] } };
    input.passcode = 'controlled-passcode'; input.auth = 'controlled-business';
    input.options = { testClass: 'ControlledClass', testMethod: 'controlledMethod', joinPlatforms: ['web', 'ios'],
      expectedSecretNames: ['controlled-expected'], directShowSecretNames: ['controlled-show'] };
  });
  const { params } = decodedDirectRoundtrip(t, f.direct);
  for (const [key, value] of [['class', 'ControlledClass#controlledMethod'], ['secretValue', canary],
    ['devicePasscode', 'controlled-passcode'], ['authProfile', 'controlled-business'], ['requiredE2eServerUrl', 'http://controlled-route'],
    ['joinPlatforms', 'web,ios'], ['expectedSecretNames', 'controlled-expected'], ['directShowSecretNames', 'controlled-show']])
    assert.equal(params.get(key) === value, true, 'Existing direct option changed: ' + key);
  assert.equal(params.get('secretConfig') === f.expected.config, true, 'Structured canary must be exact');
  assert.equal(params.get('cyclePlan') === f.expected.plan, true, 'Quoted subjects must preserve the exact recovery plan');
  assert.equal(params.get('secretCreationPlan') === f.expected.creation, true, 'Non-null creation plan must be exact');
  const redactor = createDiagnosticRedactor([canary, subject]);
  const safe = androidInstrumentationDiagnosticArguments('controlled-serial', f.direct.args, f.direct.runner, redactor.redactArguments).join(' ');
  assert.equal(safe.includes('PRIVATE_TRANSPORT_CANARY'), false, 'No raw structured value in diagnostics');
  assert.equal(safe.includes('CONTROLLED_SUBJECT'), false, 'No raw subject in diagnostics');
});

// Controlled actual-source VM: no SDK/native child, UI/Core call, clock sleep or real authority.
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { EventEmitter } from 'node:events';
import { markerScan, markerScanStream } from './process-marker.mjs';
import { createMatchedMarkerChoices } from './matched-marker-choice.mjs';
import { buildFifoContext, createFifoPublication, nativeHeadChoices, validateFifoTerminal } from './recovery-fifo.mjs';
import { createHarnessShutdown } from './harness-shutdown.mjs';

const drainControllerSource = () => readFileSync(new URL('../Test4/orchestrator.mjs', import.meta.url), 'utf8');
const deferredDrain = () => { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; };
const drainPump = async () => { for (let n = 0; n < 50; n += 1) await Promise.resolve(); };
function drainFunction(text, name) {
  const start = text.indexOf(name), end = text.indexOf('\n}\n', start);
  assert(start >= 0 && end > start, 'Existing actual source seam must be present');
  return text.slice(start, end + 2);
}
function drainClock() {
  let now = 0, id = 0; const tasks = new Map();
  return { tasks, get now() { return now; }, set now(value) { now = value; },
    setTimeout(fn, delay) { const key = ++id; tasks.set(key, { fn, delay, due: now + Number(delay) }); return key; },
    clearTimeout(key) { tasks.delete(key); },
    fire(key, at) { const task = tasks.get(key); assert(task, 'Expected actual owned timer'); tasks.delete(key); now = at ?? task.due; task.fn(); },
  };
}
async function actualDrainFixture(configure = () => {}) {
  const text = drainControllerSource(), clock = drainClock(), owner = deferredDrain(), peer = deferredDrain();
  const trace = [], children = [], ownedErrors = []; let finishError, serverHandler, cancelled = 0;
  const identities = Object.fromEntries(['web', 'android', 'ios'].map(x => [x, { deviceId: x + '-id', deviceName: x + '-name' }]));
  const cycle = { number: 1, senders: ['web', 'android'], senderSecrets: { web: 'controlled-a', android: 'controlled-b' }, approvals: [{ platform: 'ios', secret: 'controlled-a', action: 'approve' }, { platform: 'ios', secret: 'controlled-b', action: 'approve' }] };
  const pending = [
    { kind: 'recover', requestId: 'controlled-request-a', requesterId: identities.web.deviceId, secretId: 'controlled-pass-a', eligiblePeerIds: [identities.ios.deviceId, identities.android.deviceId], clientStatus: 'needApprove' },
    { kind: 'recover', requestId: 'controlled-request-b', requesterId: identities.android.deviceId, secretId: 'controlled-pass-b', eligiblePeerIds: [identities.ios.deviceId, identities.web.deviceId], clientStatus: 'needApprove' },
  ];
  const terminal = new Set();
  const context = {
    URL, Buffer, AbortController, performance: { now: () => clock.now }, setTimeout: clock.setTimeout, clearTimeout: clock.clearTimeout,
    scenario: { runId: 'controlled-run', vault: { name: 'controlled-vault' } }, testNumber: 5, scenarioArtifactLabel: 'scenario-block1',
    recoveryCycles: [cycle], defaultSecret: { name: 'controlled-a' }, recoveryShowTimeoutMs: 45_000,
    requestedSignalExitCode: undefined, genericIosBusinessPromise: undefined, approvalCoordinator: undefined,
    createFifoPublication, buildFifoContext, nativeHeadChoices, validateFifoTerminal,
    createMatchedMarkerChoices, markerScan, markerScanStream, ssClaimsInvalidationMarker: /NEVER/,
    maxWatchedOutputChars: 128_000, appendBounded: (a, b, size = 128_000) => (a + b).slice(-size),
    diagnosticRedactor: { createStream: () => ({ push: x => x, flush: () => '' }) },
    scopedSecretIds: null, createScopedSecretIdCollector() { throw new Error('No scoped data in this fixture'); },
    filterVerboseWatchedOutput: (_a, _b, x) => x, appendFileSync() {}, diagnosticLogPath: '/not-written',
    redactCommandArguments: x => x, redactEnvironment: () => ({}), process: { stdout: { write() {} }, stderr: { write() {} } },
    watchedProcessOutputs: [], console: { log() {}, error() {} },
    writeDiagnostic: message => trace.push(message), formatHarnessShutdownError: error => { ownedErrors.push(error); return 'CONTROLLED_REDACTED_OWNER_ERROR'; }, redactSensitiveText: () => 'CONTROLLED_REDACTED',
    run() {
      const child = new EventEmitter(); child.stdout = new EventEmitter(); child.stderr = new EventEmitter(); child.killed = false; child.exitCode = null;
      child.kill = signal => { if (child.exitCode !== null) return false; child.killed = true; child.exitCode = 1; child.emit('exit', null, signal); child.stdout.emit('end'); child.stderr.emit('end'); child.emit('close', null, signal); return true; };
      children.push(child); return child;
    },
    createServer(handler) { serverHandler = handler; return { once() {}, listen(_p, _h, done) { done(); }, close(done) { done(); } }; },
    concurrentFifoIdentities: async () => { context.approvalCoordinator.publishFifoIdentities(identities); return identities; },
    openWebSecrets: async () => {}, webSecretCard: async () => {}, startWebRecovery: async () => {}, assertWebInboxEmpty: async () => {},
    revealAndCloseWebSecret: async () => trace.push('original-web-show'),
    waitForFifoCanonicalSnapshot() {}, setCurrentCycleForDiagnostics() {},
  };
  configure({ context, cycle, owner, peer, pending });
  const page = { evaluate: async (_fn, request) => {
    if (request.mode === 'snapshot') return { secrets: [{ name: 'controlled-a', passId: 'controlled-pass-a' }, { name: 'controlled-b', passId: 'controlled-pass-b' }], operations: [] };
    if (request.mode === 'pending') return { operations: pending };
    terminal.add(request.requestId); return { operations: pending.map(op => terminal.has(op.requestId) ? { ...op, outcome: 'accepted', resolutionSource: 'canonical' } : op) };
  } };
  const ios = { failure: peer.promise, assertBusinessActive() {}, waitForMarker: async () => {}, waitForMarkerChoice: async choices => choices[0].value };
  const extra = text.includes('let genericFirstFailure;') ? text.slice(text.indexOf('let genericFirstFailure;'), text.indexOf('async function runConcurrentRecoveryCycles(')) : '';
  const start = text.indexOf('  genericIosBusinessPromise = runGenericIosRecovery('), end = text.indexOf('\n\n  iosTest.assertBusinessActive();', start);
  assert(start >= 0 && end > start, 'Existing actual catch/cancellation seam must be present');
  const api = runInNewContext([
    drainFunction(text, 'function startApprovalCoordinator('), drainFunction(text, 'function watchProcessOutput('), extra,
    drainFunction(text, 'async function runConcurrentRecoveryCycles('), drainFunction(text, 'async function finishAfterSignal('),
    `async function executeActualCatch(page, iosTest, androidTest) { const androidSerial = 'controlled', simulatorUdid = 'controlled';\n${text.slice(start, end)}\n}`,
    `({ start: startApprovalCoordinator, watch: watchProcessOutput, enter: executeActualCatch, signal: finishAfterSignal,
      state: () => typeof genericFirstFailure === 'undefined' ? null : genericFirstFailure,
      prepare: typeof prepareAndroidShowDrain === 'undefined' ? null : prepareAndroidShowDrain,
      drain: typeof drainFailedAndroidShow === 'undefined' ? null : drainFailedAndroidShow })`,
  ].join('\n'), context);
  context.approvalCoordinator = await api.start();
  const raw = api.watch('adb', ['controlled-logcat']); void raw.result.catch(() => {});
  const actor = superviseAndroidInstrumentation({ instrumentationResult: owner.promise, logcat: raw, selector: 'controlled#same-original-method' });
  // Exact existing startAndroidJoinTest bridge; not a fabricated owner/result policy.
  const bridge = text.slice(text.indexOf('  void actor.result.catch((error) => logcat.cancelPendingWaits(error));'), text.indexOf('  return actor;', text.indexOf('  void actor.result.catch((error) => logcat.cancelPendingWaits(error));')));
  assert(bridge.includes('cancelPendingWaits')); runInNewContext(bridge, { actor, logcat: raw });
  context.runGenericIosRecovery = (p, i, a) => context.concurrent(p, i, a);
  context.concurrent = runInNewContext(drainFunction(text, 'async function runConcurrentRecoveryCycles(') + '\nrunConcurrentRecoveryCycles', context);
  context.harnessShutdown = createHarnessShutdown({ cancelActors(error) { cancelled += 1; finishError = error; trace.push('cancel'); raw.cancelPendingWaits(error); }, captureDiagnostics() { trace.push('diagnostics'); }, cleanup() { trace.push('cleanup'); } });
  const request = (platform, cycle = 2) => { let body; serverHandler({ url: '/approval?platform=' + platform + '&cycle=' + cycle, headers: { host: 'controlled' } }, { writeHead() {}, end(x) { body = x; } }); return body; };
  let run;
  return { api, context, clock, owner, peer, ios, raw, actor, trace, ownedErrors, children, cycle, pending, identities,
    canonical: () => buildFifoContext({ runId: context.scenario.runId, testNumber: 5, blockId: 'scenario-block1', cycle, identities, passes: { 'controlled-a': 'controlled-pass-a', 'controlled-b': 'controlled-pass-b' }, baselineRequestIds: [], projection: { operations: pending } }),
    get finishError() { return finishError; }, get cancelled() { return cancelled; }, request,
    async begin() { run = api.enter(page, ios, actor); void run.catch(() => {}); await drainPump(); children[0].stdout.emit('data', Buffer.from('E2E: ANDROID_RECOVERY_REQUEST_SENT_1\n')); await drainPump(); assert([...clock.tasks.values()].some(x => x.delay === 45_000), 'Actual original45s waiter reached: ' + (finishError?.stack ?? JSON.stringify(trace))); return undefined; },
    emit(phase, child = children[0], sink = 'stdout') { child[sink].emit('data', Buffer.from('E2E: ANDROID_RECOVERY_PHASE_' + phase + '\n')); },
    timeout(at) { const entry = [...clock.tasks].find(([, x]) => x.delay === 45_000); assert(entry); clock.fire(entry[0], at); },
    result: () => run,
  };
}

test('actual original45s failure observes its owner before cancellation without upgrading FAIL', async () => {
  const f = await actualDrainFixture(); await f.begin(); await drainPump(); f.emit('SHOW_ENTER'); f.timeout(); await drainPump();
  assert.equal(f.trace.filter(x => x.includes('DIAGNOSTIC_DRAIN] START')).length, 1,
    'Original failed owner must be observed before cancellation');
  assert.equal(f.cancelled, 0);
  const primary = f.api.state().error, ownerError = new Error('CONTROLLED_LATE_OWNER_FAILURE'); f.owner.reject(ownerError); await drainPump();
  await assert.rejects(f.result(), error => error === primary); assert.equal(f.finishError, primary);
  assert(f.ownedErrors.includes(ownerError)); assert.equal(f.cancelled, 1); assert.equal(f.clock.tasks.size, 0);
  assert(f.trace.findIndex(x => x.includes('DIAGNOSTIC_DRAIN] START')) < f.trace.indexOf('cancel'));
});

const drainPhases = ['SHOW_ENTER', 'SHOW_RETURNED_VALUE_CHECKED', 'HIDE_ENTER', 'HIDE_RETURNED', 'RECOVER_WAIT_ENTER', 'RECOVER_WAIT_COMPLETE'];
const drainLine = phase => 'E2E: ANDROID_RECOVERY_PHASE_' + phase + '\n';
const drainStarted = f => f.trace.some(line => line.includes('DIAGNOSTIC_DRAIN] START'));

test('actual timer synchronously freezes both grant paths and publication before any failure race', async () => {
  const f = await actualDrainFixture(); await f.begin(); f.emit('SHOW_ENTER'); f.timeout();
  const first = f.api.state(); assert(first.error.message.includes('ANDROID_RECOVERY_SECRET_VISIBLE_1'));
  assert.equal(first.observedAt, 45_000); assert.equal(f.cancelled, 0);
  assert.equal(f.request('android-show', 1), 'allowed'); assert.equal(f.request('android-sender', 2), 'waiting');
  assert.throws(() => f.context.approvalCoordinator.allow('android-sender', 2), /frozen/);
  assert.throws(() => f.context.approvalCoordinator.allowFifo('ios', 1, 1, f.pending[0].requestId), /frozen/);
  assert.throws(() => f.context.approvalCoordinator.publishFifo(f.canonical()), /frozen/);
  assert.throws(() => f.context.approvalCoordinator.publishFifoIdentities(f.identities), /frozen/);
  f.peer.reject(new Error('CONTROLLED_LATE_IOS_ERROR')); await drainPump();
  await assert.rejects(f.result(), error => error === first.error); assert.equal(f.finishError, first.error);
  assert.equal(f.cancelled, 1); assert.equal(f.clock.tasks.size, 0);
});

test('actual epoch observes only fresh same-watcher stdout and rejects malformed call sequences', async () => {
  const f = await actualDrainFixture(); f.emit('SHOW_ENTER');
  const token = {}, receipt = f.raw.armShowEpoch(token, 'selected', () => assert.fail('No timer armed'));
  assert.equal(receipt.state().count, 0); receipt.issued(token);
  const foreign = f.api.watch('adb', ['controlled-foreign-logcat']); void foreign.result.catch(() => {});
  for (const phase of drainPhases) f.emit(phase, f.children[1]); assert.equal(receipt.state().count, 0);
  const text = drainPhases.map(drainLine).join('');
  for (const character of text) f.children[0].stdout.emit('data', Buffer.from(character));
  assert.equal((await receipt.completed).kind, 'CALL_COMPLETED'); assert.equal(receipt.state().count, 6);
  receipt.dispose(); assert.equal(receipt.state().disposed, true); assert.equal(f.clock.tasks.size, 0);
  for (const variant of ['pregrant', 'duplicate', 'out-of-order', 'unknown', 'stderr', 'wrong-epoch', 'oversized']) {
    const g = await actualDrainFixture(); const epoch = {}, r = g.raw.armShowEpoch(epoch, 'selected', () => {});
    if (variant !== 'pregrant') r.issued(variant === 'wrong-epoch' ? {} : epoch);
    if (variant === 'duplicate') { g.emit('SHOW_ENTER'); g.emit('SHOW_ENTER'); }
    else if (variant === 'out-of-order') g.emit('HIDE_ENTER');
    else if (variant === 'unknown') g.emit('UNKNOWN');
    else if (variant === 'stderr') g.emit('SHOW_ENTER', g.children[0], 'stderr');
    else if (variant === 'oversized') g.children[0].stdout.emit('data', Buffer.from('x'.repeat(128_001)));
    else g.emit('SHOW_ENTER');
    assert.equal(r.state().invalid, true, variant); assert.equal((await r.invalidated).kind, 'OBSERVER_ENDED'); r.dispose();
    assert.equal(g.clock.tasks.size, 0);
  }
});

test('actual epoch keeps default next-only waiters untouched and rejects only its exact timer contract', async () => {
  const f = await actualDrainFixture(); const epoch = {}; let tagged = 0;
  const r = f.raw.armShowEpoch(epoch, 'E2E: SELECTED', () => { tagged += 1; }); r.issued(epoch);
  f.emit('SHOW_ENTER'); const old = f.raw.waitForNextMarker('E2E: UNRELATED', 1000);
  f.children[0].stdout.emit('data', Buffer.from('E2E: UNRELATED\n')); await old;
  for (const [marker, budget] of [['E2E: OTHER', 45_000], ['E2E: SELECTED', 44_999]]) {
    const waiting = f.raw.waitForMarker(marker, budget); void waiting.catch(() => {});
    const timer = [...f.clock.tasks].find(([, value]) => value.delay === budget); f.clock.fire(timer[0]); await assert.rejects(waiting);
  }
  assert.equal(tagged, 0); r.dispose();
  const replacement = f.raw.armShowEpoch({}, 'unused', () => {}); replacement.dispose(); assert.equal(f.clock.tasks.size, 0);
});

test('same-call completion and owner settlement between timer and catch remain latched without late PASS', async () => {
  for (const outcome of ['phases', 'owner-success', 'owner-error']) {
    const f = await actualDrainFixture(); await f.begin(); f.emit('SHOW_ENTER'); f.timeout();
    const primary = f.api.state().error;
    if (outcome === 'phases') for (const phase of drainPhases.slice(1)) f.emit(phase);
    else if (outcome === 'owner-success') f.owner.resolve({ stdout: 'CONTROLLED_SUCCESS' });
    else f.owner.reject(new Error('CONTROLLED_OWNER_FAILURE'));
    await drainPump(); await assert.rejects(f.result(), error => error === primary);
    assert.equal(f.finishError, primary); assert(drainStarted(f)); assert.equal(f.cancelled, 1);
    assert.equal(f.clock.tasks.size, 0); assert.equal(f.request('android-sender', 2), 'waiting');
    assert(f.trace.some(line => line.includes('END kind=' + ({ phases: 'CALL_COMPLETED', 'owner-success': 'OWNER_COMPLETED', 'owner-error': 'OWNER_FAILED' }[outcome]))));
  }
});

test('absolute diagnostic cap never resets and instrumentation result remains distinct from logcat close', async () => {
  const f = await actualDrainFixture(); await f.begin(); f.emit('SHOW_ENTER'); f.timeout(); await drainPump();
  assert(drainStarted(f)); const cap = [...f.clock.tasks].find(([, task]) => task.due === 225_000); assert(cap);
  f.emit('SHOW_RETURNED_VALUE_CHECKED'); f.emit('HIDE_ENTER');
  f.children[0].emit('exit', 0, null); f.children[0].emit('close', 0, null); await drainPump();
  assert.equal(f.cancelled, 0); assert.equal(f.clock.tasks.size, 1); assert.equal(f.api.state().drain.ownerSettled, false);
  assert.equal(f.clock.tasks.get(cap[0]).due, 225_000); f.clock.fire(cap[0]); await drainPump();
  const primary = f.api.state().error; await assert.rejects(f.result(), error => error === primary);
  assert(f.trace.some(line => line.includes('END kind=DIAGNOSTIC_CAP'))); assert.equal(f.cancelled, 1); assert.equal(f.clock.tasks.size, 0);
  for (const now of [225_000, 225_001, 44_999]) {
    const g = await actualDrainFixture(); await g.begin(); g.emit('SHOW_ENTER'); g.timeout(); g.clock.now = now;
    await drainPump(); await assert.rejects(g.result(), error => error === g.api.state().error);
    assert.equal(drainStarted(g), false); assert.equal(g.clock.tasks.size, 0); assert.equal(g.cancelled, 1);
  }
});

test('eligibility is scoped to exact Test5 first original show after both canonical requests settled', async () => {
  const variants = [
    f => { f.context.testNumber = 4; }, f => { f.context.scenarioArtifactLabel = 'scenario-block2'; },
    (_f, c) => { c.number = 2; }, (_f, c) => { c.senders = ['android']; },
    (_f, c) => { c.approvals[0].platform = 'android'; }, (_f, _c, x) => { x.runId = 'foreign'; },
    (_f, _c, x) => { x.testNumber = 4; }, (_f, _c, x) => { x.blockId = 'foreign'; },
    (_f, _c, x) => { x.cycle = 2; }, (_f, _c, x) => { x.requests.pop(); },
    (_f, _c, x) => { x.requests[1].requestId = x.requests[0].requestId; },
    (_f, _c, x) => { x.requests[1].subject = 'foreign'; }, (_f, _c, _x, ids) => { ids.pop(); },
    (_f, _c, _x, ids) => { ids[1] = ids[0]; }, (_f, _c, _x, ids) => { ids[1] = 'foreign'; },
    (_f, _c, x) => { x.approvals[0].approverPlatform = 'android'; },
    (_f, _c, x) => { x.approvals[0].action = 'decline'; },
  ];
  for (const change of variants) {
    const f = await actualDrainFixture(), cycle = structuredClone(f.cycle), canonical = structuredClone(f.canonical()), ids = canonical.requests.map(r => r.requestId);
    change(f, cycle, canonical, ids); assert.equal(f.api.prepare(f.actor, cycle, canonical, ids), null);
    assert.equal(f.clock.tasks.size, 0);
  }
  const f = await actualDrainFixture(); assert.equal(f.api.prepare({ result: f.actor.result }, f.cycle, f.canonical(), f.pending.map(r => r.requestId)), null);
  for (const variant of ['no-enter', 'returned', 'duplicate', 'pregrant', 'closed', 'owner-settled', 'peer-failed']) {
    const g = await actualDrainFixture();
    if (variant === 'pregrant') {
      const original = g.context.approvalCoordinator.allow;
      g.context.approvalCoordinator.allow = (...args) => { if (args[0] === 'android-show') g.emit('SHOW_ENTER'); return original(...args); };
    }
    await g.begin();
    if (variant !== 'no-enter' && variant !== 'pregrant') g.emit('SHOW_ENTER');
    if (variant === 'returned') g.emit('SHOW_RETURNED_VALUE_CHECKED');
    if (variant === 'duplicate') g.emit('SHOW_ENTER');
    if (variant === 'closed') g.children[0].emit('exit', 0, null);
    if (variant === 'owner-settled') g.owner.resolve({ stdout: 'CONTROLLED_SUCCESS' });
    if (variant === 'peer-failed') g.peer.reject(new Error('CONTROLLED_EARLIER_PEER'));
    if (['closed', 'owner-settled', 'peer-failed'].includes(variant)) { await drainPump(); await assert.rejects(g.result()); }
    else { g.timeout(); await drainPump(); await assert.rejects(g.result(), error => error === g.api.state().error); }
    assert.equal(drainStarted(g), false, variant); assert.equal(g.cancelled, 1); assert.equal(g.clock.tasks.size, 0);
  }
});

test('signal or peer ends diagnostic wait with one cleanup and retains original first Error', async () => {
  for (const mode of ['signal', 'peer', 'cancel', 'diagnostic-error']) {
    const f = await actualDrainFixture(); await f.begin(); f.emit('SHOW_ENTER'); f.timeout(); await drainPump();
    assert(drainStarted(f)); const primary = f.api.state().error;
    if (mode === 'signal') await f.api.signal('SIGINT', 130);
    else if (mode === 'peer') f.peer.reject(new Error('CONTROLLED_LATE_PEER'));
    else if (mode === 'cancel') f.raw.cancelPendingWaits(new Error('CONTROLLED_CANCEL'));
    else { f.context.writeDiagnostic = () => { throw new Error('CONTROLLED_DIAGNOSTIC_IO_FAILURE'); }; f.emit('SHOW_RETURNED_VALUE_CHECKED'); for (const phase of drainPhases.slice(2)) f.emit(phase); }
    await drainPump(); await assert.rejects(f.result(), error => error === primary);
    assert.equal(f.finishError, primary); assert.equal(f.cancelled, 1); assert.equal(f.clock.tasks.size, 0);
    assert.equal(f.trace.filter(line => line === 'cleanup').length, 1);
  }
});

test('ordinary success keeps original grants and marker sequence with no drain or receipt leak', async () => {
  const f = await actualDrainFixture(); await f.begin();
  for (const phase of drainPhases) f.emit(phase);
  f.children[0].stdout.emit('data', Buffer.from('E2E: ANDROID_RECOVERY_SECRET_VISIBLE_1\nE2E: ANDROID_RECOVERY_CLOSED_1\n'));
  await drainPump(); await f.result(); assert.equal(f.api.state(), null); assert.equal(drainStarted(f), false); assert.equal(f.cancelled, 0);
  assert.equal(f.clock.tasks.size, 0); const receipt = f.raw.armShowEpoch({}, 'unused', () => {}); receipt.dispose();
  f.context.approvalCoordinator.allow('android-sender', 2); assert.equal(f.request('android-sender', 2), 'allowed');
});

test('actual ordinary WebAuthn ready branch preserves create and stored credential login after drain changes', async () => {
  for (const stored of [false, true]) {
    const trace = [], window = { localStorage: { getItem: () => stored ? 'CONTROLLED_CREDENTIAL' : null } };
    const button = name => ({ isVisible: async () => true,
      waitFor: async options => trace.push(name + ':' + options.state + ':' + options.timeout),
      click: async () => trace.push(name + ':click') });
    const create = button('create'), authenticate = button('authenticate');
    const ready = { waitFor: async options => { assert.equal(options.state, 'visible'); assert.equal(options.timeout, 30_000); trace.push('ready'); } };
    const error = { first() { return this; }, waitFor: () => new Promise(() => {}), innerText: async () => 'CONTROLLED_AUTH_ERROR' };
    const page = {
      getByRole(role, options) { assert.equal(role, 'button'); assert(['Create Passkey', 'Authenticate with Passkey'].includes(options.name)); return options.name === 'Create Passkey' ? create : authenticate; },
      locator(selector) { assert.equal(selector, '[data-slot="dialog-content"] .text-destructive'); return error; },
      evaluate: async fn => runInNewContext('(' + fn.toString() + ')()', { window, document: { querySelector: () => null } }),
      url: () => 'https://controlled.invalid/', title: async () => 'CONTROLLED_TITLE',
    };
    const unlock = runInNewContext(drainFunction(drainControllerSource(), 'async function unlockWithPasskeyIfNeeded(') + '\nunlockWithPasskeyIfNeeded', {
      webEntryReady: actualPage => { assert.equal(actualPage, page); return ready; }, primaryVirtualAuthenticator: null,
      writeDiagnostic: message => trace.push('diagnostic'), console: { log() {} },
    });
    await unlock(page);
    const selected = stored ? 'authenticate' : 'create';
    assert.equal(window.__METASECRET_E2E_AUTHENTICATED, true);
    assert.deepEqual(trace, [selected + ':visible:30000', selected + ':click', 'ready', 'ready', selected + ':hidden:30000']);
  }
});
