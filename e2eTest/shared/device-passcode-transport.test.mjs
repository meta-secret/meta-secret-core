import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import vm from 'node:vm';
import test from 'node:test';
import { createDiagnosticRedactor } from './diagnostic-redaction.mjs';
import { bindIosStepSecret } from './ios-step-secret.mjs';
import { resolveDeletionAuthProfile } from '../Test50/plan.mjs';
import { EXISTING_IOS_TARGET, iosSerialTestArguments } from './platform-supervisor.mjs';
import { superviseAndroidInstrumentation, assertAndroidInstrumentationPassed } from './android-instrumentation.mjs';

const pin = '240681'; // Controlled non-secret input, never a device observation.
const iosUdid = EXISTING_IOS_TARGET.udid; // Source-defined test input, not a fresh device observation.
const source = readFileSync(new URL('../Test4/orchestrator.mjs', import.meta.url), 'utf8');
function between(text, start, end) {
  const first = text.indexOf(start); const last = text.indexOf(end, first + start.length);
  assert.ok(first >= 0 && last > first, 'Actual caller boundary is missing'); return text.slice(first, last);
}
const observed = () => ({ child: { killed: false, kill() { this.killed = true; } },
  result: Promise.resolve({ stdout: '' }), waitForMarker: async () => true, cancelPendingWaits: () => {} });
const scenario = () => ({ mode: 'vault-secret-deletion', vault: { name: 'controlled-vault' },
  android: { bundleId: 'metasecret.project.com', testClass: 'ActualFixture', directInstrumentationSetup: true },
  ios: { bundleId: 'org.metasecret.vault', testClass: 'CaseEightIosBothOfflineUITest' } });
function loadActualDeclarations(context, android, ios) {
  context.process = { env: { E2E_ANDROID_DEVICE_PASSCODE: android, E2E_IOS_DEVICE_PASSCODE: ios } };
  if (source.includes('function readDeclaredDevicePasscodes(')) {
    vm.runInContext(between(source, 'function readDeclaredDevicePasscodes(', 'const sensitiveDiagnosticValues ='), context);
  }
}

test('actual reader binds distinct platform values, preserves leading zero and leaves missing input absent', () => {
  const context = vm.createContext({}); loadActualDeclarations(context, pin, '905714');
  assert.equal(typeof context.readDeclaredDevicePasscodes, 'function', 'Intentional baseline capability absence: declared-PIN reader is not implemented');
  const read = context.readDeclaredDevicePasscodes;
  const values = read({ E2E_ANDROID_DEVICE_PASSCODE: '0042', E2E_IOS_DEVICE_PASSCODE: '905714' });
  assert.ok(values.android === '0042' && values.ios === '905714', 'Platform binding or leading zero changed');
  const missing = read({}); assert.equal(missing.android, undefined); assert.equal(missing.ios, undefined);
});

test('actual reader rejects invalid declarations without echoing their values', () => {
  const context = vm.createContext({}); loadActualDeclarations(context, pin, '905714');
  assert.equal(typeof context.readDeclaredDevicePasscodes, 'function', 'Intentional baseline capability absence: declared-PIN reader is not implemented');
  for (const key of ['E2E_ANDROID_DEVICE_PASSCODE', 'E2E_IOS_DEVICE_PASSCODE']) {
    for (const invalid of ['', '123', '12x4', ' 240681', '1'.repeat(17)]) {
      assert.throws(() => context.readDeclaredDevicePasscodes({ [key]: invalid }), (error) =>
        /value omitted/.test(error.message) && (!invalid || !error.message.includes(invalid)));
    }
  }
});

function androidCaller({ declared = pin, direct = true } = {}) {
  const calls = []; const value = scenario(); value.android.directInstrumentationSetup = direct;
  const context = {
    scenario: value, testNumber: 50, declaredDevicePasscodes: { android: declared, ios: '905714' }, console: { log() {} },
    deletionAuthProfile: resolveDeletionAuthProfile({ E2E_ANDROID_DEVICE_PASSCODE: declared }, value),
    assertBatchProductsCurrent() {}, batchProducts: null, coreRoot: '/fixture/core', composeRoot: '/fixture/compose',
    waitForAndroidBoot: async () => {}, runAdbWithRetry: async (args) => { calls.push({ kind: 'adb', args }); },
    androidAppApkPath: '/fixture/app.apk', androidTestApkPath: '/fixture/test.apk', androidTestBundleId: 'fixture.test',
    diagnosticAndroidVerbose: false, watchProcessOutput: () => observed(), networkLossConfig: null,
    networkProxyUrl: () => 'http://10.0.2.2:5201', defaultSecret: { name: 'fixture-name', value: 'fixture-value' },
    secretConfigs: { only: { name: 'fixture-name', value: 'fixture-value' } }, recoveryCycles: [],
    cyclePlanJson: '[]', secretConfigJson: '{}', secretCreationPlan: {}, reuseProvenArtifacts: direct,
    writeDiagnostic() {}, verifyAndroidApkFfiProvenance: async () => {},
    captureInstalledAndroidAppProvenance: async () => {}, dismissAndroidCompatibilityDialogOnce: async () => {},
    buildAndroidFfiFromCurrentSource: async () => {}, recordFreshBuildProvenance: async () => {},
    runAndWait: async (file, args) => { calls.push({ kind: 'command', file, args }); return { stdout: '' }; },
    watchAndroidInstrumentation: (_serial, args) => { calls.push({ kind: 'instrumentation', args }); return observed(); },
    superviseAndroidInstrumentation, assertAndroidInstrumentationPassed,
  };
  vm.createContext(context);
  loadActualDeclarations(context, declared, '905714');
  vm.runInContext(between(source, 'async function startAndroidJoinTest(', 'async function startAndroidStepTest(')
    + between(source, 'async function startAndroidStepTest(', 'async function dismissAndroidCompatibilityDialog('), context);
  return { context, calls };
}
const hasDeclaredArgument = (args) => args.some((value, index) => value === 'devicePasscode' && args[index + 1] === pin);

test('actual Android setup caller forwards exactly one declared PIN to direct instrumentation', async () => {
  const { context, calls } = androidCaller(); const actor = await context.startAndroidJoinTest('owned-serial'); await actor.result;
  const args = calls.find((call) => call.kind === 'instrumentation').args;
  assert.ok(hasDeclaredArgument(args), 'Declared PIN did not reach actual setup instrumentation');
  assert.equal(args.filter((value) => value === 'devicePasscode').length, 1);
  assert.equal(args[args.indexOf('authProfile') + 1], 'real-os');
});

test('actual Android fallback caller forwards declared PIN to the existing Gradle instrumentation port', async () => {
  const { context, calls } = androidCaller({ direct: false });
  const actor = await context.startAndroidJoinTest('owned-serial'); await actor.result;
  const invocation = calls.find((call) => call.kind === 'command' && call.args.includes(':composeApp:connectedDebugAndroidTest'));
  assert.ok(invocation.args.includes(`-Pandroid.testInstrumentationRunnerArguments.devicePasscode=${pin}`), 'Gradle caller lost declared PIN');
  assert.ok(invocation.args.includes('-Pandroid.testInstrumentationRunnerArguments.authProfile=real-os'));
});

test('actual Android step caller forwards the same declared PIN without replacing role or selector', async () => {
  const { context, calls } = androidCaller();
  const actor = await context.startAndroidStepTest('owned-serial', { role: 'cancel', cycle: 16, step: 'run-cancel', testMethod: 'handleSecretDeletionStep' });
  await actor.result;
  const args = calls.find((call) => call.kind === 'instrumentation').args;
  assert.ok(hasDeclaredArgument(args), 'Declared PIN did not reach actual step instrumentation');
  assert.ok(args.includes('cancel') && args.includes('ActualFixture#handleSecretDeletionStep'));
  assert.equal(args[args.indexOf('authProfile') + 1], 'real-os');
});

function unlockCaller(declared, unlocked = false) {
  const calls = []; const context = { declaredDevicePasscodes: { android: declared }, console: { log() {} },
    androidUserIsUnlocked: async () => unlocked, waitForAndroidPinPrompt: async () => {},
    waitForAndroidUnlocked: async () => {}, runAdbWithRetry: async (args, options) => { calls.push({ args: Array.from(args), options }); } };
  vm.createContext(context);
  loadActualDeclarations(context, declared, '905714');
  vm.runInContext(between(source, 'async function unlockAndroidDevice(', 'async function androidUserIsUnlocked('), context);
  return { context, calls };
}

test('actual Android unlock enters declared digits rather than an assumed fixed PIN', async () => {
  const { context, calls } = unlockCaller(pin); await context.unlockAndroidDevice('owned-serial');
  const digits = calls.map((call) => call.args.at(-1)).filter((value) => /^KEYCODE_[0-9]$/.test(value));
  assert.ok(digits.join(',') === [...pin].map((value) => `KEYCODE_${value}`).join(','), 'Unlock used different digits');
  assert.ok(calls.filter((call) => /^KEYCODE_[0-9]$/.test(call.args.at(-1))).every((call) => call.options?.sensitiveInput === true),
    'Each credential digit needs private diagnostics, not only whole-PIN redaction');
});

test('actual Android locked-device caller rejects missing declaration before any input', async () => {
  const { context, calls } = unlockCaller(undefined);
  await assert.rejects(context.unlockAndroidDevice('owned-serial'), /declared PIN/);
  assert.equal(calls.length, 0, 'Missing PIN must not trigger speculative input');
});

test('actual Android unlocked-device caller needs no PIN or extra input', async () => {
  const { context, calls } = unlockCaller(undefined, true);
  await context.unlockAndroidDevice('owned-serial'); assert.equal(calls.length, 0);
});

function adbRetryCaller({ failAlways = false, reconnectFails = false } = {}) {
  const primary = new Error('controlled KEYCODE_2 failure'); const calls = []; const warnings = []; let attempt = 0;
  const context = { console: { warn: (message) => warnings.push(String(message)) }, wait: async () => {},
    runAndCapture: async (_file, args) => {
      calls.push(Array.from(args));
      if (args[0] === 'reconnect') {
        if (reconnectFails) throw new Error('controlled KEYCODE_4 reconnect failure');
        return { stdout: '' };
      }
      if (failAlways || ++attempt === 1) throw primary;
      return { stdout: 'completed' };
    } };
  vm.createContext(context);
  vm.runInContext(between(source, 'async function runAdbWithRetry(', 'async function captureInstalledAndroidAppProvenance('), context);
  return { context, primary, calls, warnings };
}

test('actual sensitive digit retry omits individual keycodes and reconnect error payload before warning persistence', async () => {
  const { context, calls, warnings } = adbRetryCaller({ reconnectFails: true });
  await context.runAdbWithRetry(['-s', 'owned-serial', 'shell', 'input', 'keyevent', 'KEYCODE_2'],
    { attempts: 2, delayMs: 0, sensitiveInput: true });
  assert.equal(calls.length, 3); assert.equal(warnings.length, 2);
  assert.ok(!warnings.join('\n').includes('KEYCODE_'), 'Per-digit warning leaks credential input');
  assert.ok(warnings.every((message) => message.includes('omitted')));
});

test('actual terminal sensitive-input failure preserves original cause behind a safe public stack', async () => {
  const { context, primary } = adbRetryCaller({ failAlways: true });
  await assert.rejects(context.runAdbWithRetry(['-s', 'owned-serial', 'shell', 'input', 'keyevent', 'KEYCODE_2'],
    { attempts: 1, sensitiveInput: true }), (error) => error.cause === primary
      && !error.message.includes('KEYCODE_') && !error.stack.includes('KEYCODE_'));
});

test('actual ordinary ADB retry retains its original error identity and existing diagnostic behavior', async () => {
  const { context, primary, warnings } = adbRetryCaller({ failAlways: true });
  await assert.rejects(context.runAdbWithRetry(['-s', 'owned-serial', 'shell', 'getprop', 'sys.boot_completed'],
    { attempts: 1 }), (error) => error === primary);
  assert.ok(warnings[0].includes('getprop sys.boot_completed'));
});

function iosCaller(options = {}) {
  const { renameError, scopedError } = options;
  const declared = Object.hasOwn(options, 'declared') ? options.declared : pin;
  const calls = []; const value = scenario(); let sequence = 0;
  // Explicit dependency tokens only: no physical namespace/template proof is fabricated.
  const identityContext = Object.freeze({ runRoot: '/fixture/ios-identity', testNumber: 50 });
  const templateProof = Object.freeze({ fixtureToken: 'SYNTHETIC_TEMPLATE_PORT' });
  const scopedXctestrun = '/fixture/ios-identity/ios-invocations/selected.xctestrun';
  const context = {
    scenario: value, declaredDevicePasscodes: { ios: declared, android: '905714' }, console: { log() {} }, registrationOnly: false,
    deletionAuthProfile: resolveDeletionAuthProfile({ E2E_IOS_DEVICE_PASSCODE: declared }, value),
    iosIdentity: { context: identityContext }, iosSerialTestArguments,
    iosGeneratedTemplateProof: (path) => { assert.equal(path, '/fixture/derived'); return templateProof; },
    prepareScopedIosInvocation: (input) => {
      calls.push({ kind: 'scoped', input });
      assert.equal(input.identityContext, identityContext); assert.equal(input.templateProof, templateProof);
      assert.equal(input.selector, 'iosAppUITests/CaseEightIosBothOfflineUITest/handleSecretDeletionStep');
      assert.equal(input.authPolicy, 'real-os');
      assert.equal(input.outputDirectory, resolve(identityContext.runRoot, 'ios-invocations'));
      assert.equal(input.privateConfiguration.requireCoreRouting, true);
      assert.equal(input.privateConfiguration.coreServerUrl, 'http://127.0.0.1:5202');
      assert.equal(input.privateConfiguration.authProfile, 'real-os');
      assert.equal(input.privateConfiguration.devicePasscode, declared);
      assert.equal(input.hostEnvironment.E2E_AUTH_PROFILE, 'real-os');
      assert.ok(!Object.keys(input.hostEnvironment).some((key) => /PASSCODE/.test(key)));
      if (scopedError) throw scopedError;
      return { xctestrun: scopedXctestrun };
    },
    defaultSecret: { name: 'fixture-name', value: 'fixture-value' }, secretConfigs: { only: { name: 'fixture-name', value: 'fixture-value' } },
    bindIosStepSecret, assertBatchProductsCurrent() {}, batchProducts: null, coreRoot: '/fixture/core', composeRoot: '/fixture/compose',
    verifyIosXcodeBuild() {}, iosDerivedDataPath: '/fixture/derived', iosProjectPath: '/fixture/ios.xcodeproj',
    iosStepConfigPath: '/fixture/private-step.json', iosGradleUserHome: '/fixture/gradle', iosTestMethod: 'handleSecretDeletionStep',
    artifactsDirectory: '/fixture/artifacts', testNumber: 50, process: { env: {} }, createHash, resolve, tmpdir: () => '/fixture/tmp',
    networkLossConfig: {}, networkProxyUrl: () => 'http://127.0.0.1:5202', recoveryCycles: [], secretConfigJson: '{}',
    cyclePlanJson: '[]', iosSenderCycles: '', iosApprovalSteps: '', secretCreationPlan: {}, reuseIosBuildForTesting: true,
    writeDiagnostic() {}, mkdtempSync: () => `/fixture/owned-${++sequence}`,
    writeFileSync: (file, text, options) => { calls.push({ kind: 'write', file, configuration: JSON.parse(text), options }); },
    renameSync: (from, to) => { calls.push({ kind: 'rename', from, to }); if (renameError) throw renameError; },
    rmSync: (file) => { calls.push({ kind: 'remove', file }); },
    watchProcessOutput: (_file, args, options) => { calls.push({ kind: 'actor', args: Array.from(args), options }); return observed(); },
    createIosSelectedSuiteActor: () => ({ result: Promise.resolve({ passed: true }), cancelAndJoin: async () => {} }),
    removePassedIosTestResults() {},
  };
  vm.createContext(context);
  loadActualDeclarations(context, '905714', declared);
  if (source.includes('function writeIosStepConfiguration(')) {
    vm.runInContext(between(source, 'function writeIosStepConfiguration(', 'function startIosJoinTest('), context);
  }
  vm.runInContext(between(source, 'function startIosJoinTest(', 'async function prepareAndroidEmulator('), context);
  return { context, calls };
}

test('actual iOS caller persists declared PIN privately before actor spawn without adding it to app launch environment', async () => {
  const { context, calls } = iosCaller(); const actor = context.startIosJoinTest(iosUdid); await actor.result;
  const written = calls.find((call) => call.kind === 'write');
  assert.ok(written.configuration.devicePasscode === pin, 'Actual persisted XCTest config lost declared PIN');
  assert.equal(written.options?.mode, 0o600); assert.equal(written.options?.flag, 'wx');
  assert.ok(calls.findIndex((call) => call.kind === 'rename') < calls.findIndex((call) => call.kind === 'actor'));
  const environment = calls.find((call) => call.kind === 'actor').options.env;
  assert.ok(!Object.keys(environment).some((key) => /PASSCODE/.test(key)), 'PIN must not become an app launch variable');
  assert.equal(environment.E2E_AUTH_PROFILE, 'real-os');
  const scoped = calls.find((call) => call.kind === 'scoped');
  assert.ok(scoped.input.privateConfiguration.devicePasscode === pin);
  assert.ok(calls.findIndex((call) => call.kind === 'scoped') < calls.findIndex((call) => call.kind === 'actor'));
  const args = calls.find((call) => call.kind === 'actor').args;
  assert.equal(args[args.indexOf('-xctestrun') + 1], '/fixture/ios-identity/ios-invocations/selected.xctestrun');
  assert.ok(args.includes(`id=${iosUdid}`));
  assert.equal(written.configuration.recoveryPhase, undefined);
  assert.equal(written.configuration.recoveryPhaseRequired, undefined);
});

test('actual iOS caller never synthesizes a default PIN in missing-input actor config', async () => {
  const { context, calls } = iosCaller({ declared: undefined });
  // Missing input must stay absent through the actual environment reader.
  const actor = context.startIosJoinTest(iosUdid); await actor.result;
  assert.equal(calls.find((call) => call.kind === 'write').configuration.devicePasscode, undefined);
});

test('actual iOS private-config failure preserves cause, cleans its own temporary directory and launches no actor', () => {
  const primary = new Error('controlled atomic replacement failure'); const { context, calls } = iosCaller({ renameError: primary });
  assert.throws(() => context.startIosJoinTest(iosUdid), (error) => error === primary);
  assert.equal(calls.filter((call) => call.kind === 'remove').length, 1);
  assert.equal(calls.filter((call) => call.kind === 'actor').length, 0);
});

test('actual iOS scoped transport failure preserves cause and launches no actor', () => {
  const primary = new Error('controlled scoped invocation failure');
  const { context, calls } = iosCaller({ scopedError: primary });
  assert.throws(() => context.startIosJoinTest(iosUdid), (error) => error === primary);
  assert.equal(calls.filter((call) => call.kind === 'scoped').length, 1);
  assert.equal(calls.filter((call) => call.kind === 'actor').length, 0);
});

test('actual Test4 redactor assembly registers declared PIN before handling arbitrary XCTest output', () => {
  const context = { secretConfigs: { only: { value: 'fixture-secret' } }, declaredDevicePasscodes: { android: pin, ios: pin }, createDiagnosticRedactor };
  const redactor = vm.runInNewContext(between(source, 'const sensitiveDiagnosticValues =', 'function appendBounded(') + '\ndiagnosticRedactor;', context);
  assert.ok(!redactor.redactText(`Type "${pin}" into SecureTextField`).includes(pin), 'Actual caller redactor leaked typed PIN');
});

test('native test-owned PIN consumption is bound before launch and drops the unconditional fixed fallback', () => {
  const swift = readFileSync(new URL('../../../meta-secret-compose/iosApp/iosAppUITests/CaseEightIosBothOfflineUITest.swift', import.meta.url), 'utf8');
  const authentication = readFileSync(new URL('../../../meta-secret-compose/iosApp/iosAppUITests/LiveboardAuthenticationTestActions.swift', import.meta.url), 'utf8');
  const android = readFileSync(new URL('../../../meta-secret-compose/composeApp/src/androidTest/kotlin/metasecret/project/com/LiveboardAuthenticationTestActions.kt', import.meta.url), 'utf8');
  const checkSwift = (text) => {
    const setup = between(text, 'override func setUpWithError()', 'func joinWebInitiatedVault()');
    assert.ok(setup.includes('app.lbSelection.devicePasscode = passcode'), 'Actual test selection is not bound');
    assert.ok(setup.indexOf('app.lbSelection.devicePasscode = passcode') < setup.indexOf('app.launch()'));
    assert.ok(setup.includes('app?.lbSelection.devicePasscode = nil'), 'Test-owned PIN reference is not released');
  };
  checkSwift(swift);
  assert.throws(() => checkSwift(swift.replace('app.lbSelection.devicePasscode = passcode', '')));
  assert.ok(!/typeText\("1111"\)|0\.\.<4/.test(authentication), 'Shared iOS helper still guesses a PIN');
  assert.ok(authentication.includes('lbSelection.devicePasscode') && authentication.includes('XCTFail('));
  assert.ok(!/\?: "1111"/.test(android), 'Android helper still guesses a PIN');
  assert.ok(android.includes('requireNotNull') && android.includes('getString("devicePasscode")'));
});
