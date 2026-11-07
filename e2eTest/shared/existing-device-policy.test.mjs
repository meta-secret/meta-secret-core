import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, symlinkSync, rmSync, realpathSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { runInNewContext } from 'node:vm';
import * as supervisor from './platform-supervisor.mjs';
import { resetE2eNativeAppState } from './e2e-state-reset.mjs';
import { deletionEntryComplete } from '../Test50/plan.mjs';
import { assertAcceptedRunnerLineage } from '../run-ui-e2e-range.mjs';
import { E2E_BUILD_PROFILE, planPreparationBudget, createPreparationResourceBudget } from './e2e-resource-budget.mjs';
import { createPreparationMetrics } from './preparation-metrics.mjs';

const UUID = 'DB22263E-A8A3-49C5-ADD1-BAE4089E2E30';
const runtime = 'com.apple.CoreSimulator.SimRuntime.iOS-26-5';
const target = { name: 'iPhone 17', udid: UUID, deviceTypeIdentifier: 'com.apple.CoreSimulator.SimDeviceType.iPhone-17', state: 'Booted', isAvailable: true };
function iosFixture(devices = [target], runtimeIdentifier = runtime) {
  const calls = [];
  return { calls, options: {
    runCommand: async (_file, args) => {
      calls.push(args);
      if (args.includes('list')) return { stdout: JSON.stringify({ devices: { [runtimeIdentifier]: devices } }) };
      if (args.includes('bootstatus')) return { stdout: '' };
      if (args.includes('managerpid')) return { stdout: '96981\n' };
      assert.fail('unexpected mutation');
    },
    openDeviceUi: async ({ simulatorUdid }) => { assert.equal(simulatorUdid, UUID); return {}; },
  } };
}

test('existing-only iOS preparation refuses a foreign UUID before boot or install', async () => {
  const f = iosFixture([{ ...target, udid: '11111111-1111-1111-1111-111111111111' }]);
  await assert.rejects(supervisor.prepareIos('iPhone 17', () => assert.fail('no boot'), f.options));
  assert.equal(f.calls.length, 1);
});

test('existing-only iOS preparation binds the declared UUID and full physical tuple', async () => {
  const f = iosFixture([{ ...target, name: 'unrelated', udid: 'other' }, target]);
  const actual = await supervisor.prepareIos('iPhone 17', () => assert.fail('already Booted'), f.options);
  assert.equal(actual.udid, UUID);
  assert.deepEqual(f.calls.map(args => args[1]), ['list', 'bootstatus', 'spawn']);
  for (const device of [{ ...target, name: 'foreign' }, { ...target, isAvailable: false }, { ...target, deviceTypeIdentifier: 'foreign' }]) {
    const bad = iosFixture([device]);
    await assert.rejects(supervisor.prepareIos('iPhone 17', () => assert.fail('no boot'), bad.options));
    assert.equal(bad.calls.length, 1);
  }
  const badRuntime = iosFixture([target], 'foreign-runtime');
  await assert.rejects(supervisor.prepareIos('iPhone 17', () => {}, badRuntime.options));
  assert.equal(badRuntime.calls.length, 1);
});

test('new-run admission refuses all legacy creation ownership and opt-ins before effects', async () => {
  const admit = supervisor.assertExistingDeviceRun;
  assert.equal(typeof admit, 'function');
  const scenarios = [{ ios: { simulatorName: 'iPhone 17' }, android: { avdName: 'Pixel_4a' } }];
  assert.doesNotThrow(() => admit({ scenarios, manifest: {}, environment: {} }));
  for (const key of ['ownedIosProfile', 'ownedAndroidLease', 'ownedAndroidProfile', 'iosProfileCreationIntent']) {
    assert.throws(() => admit({ scenarios, manifest: { runtimeOwnership: [{ [key]: {} }] }, environment: {} }));
  }
  for (const runtimeOwnership of [null, {}, [{ platform: 'iOS', udid: UUID }], [{ platform: 'iOS', udid: 'foreign' }], [{ platform: 'Android', serial: 'foreign' }]]) {
    const manifest = { runtimeOwnership, tests: [] };
    const source = readFileSync(new URL('../run-ui-e2e-range.mjs', import.meta.url), 'utf8');
    const start = source.indexOf('async function executeRange('), end = source.indexOf('\n}\n', start) + 2;
    const enter = runInNewContext(source.slice(start, end) + '\nexecuteRange;', {
      assertExistingDeviceRun: options => admit({ ...options, environment: {} }),
      createBatchEnvironmentGuard: () => assert.fail('No guard preparation'),
      stopRunnerOwnedRuntimes: () => assert.fail('No cleanup authority from persisted metadata'),
    });
    await assert.rejects(enter({ entries: [{ testNumber: 1, scenarios: [{ value: scenarios[0] }] }], manifest }), /Persisted runtime ownership/);
  }
  for (const key of ['E2E_ANDROID_OWNED_PROFILE', 'E2E_IOS_OWNED_PROFILE']) {
    for (const value of ['1', '0', '']) assert.throws(() => admit({ scenarios, manifest: {}, environment: { [key]: value } }));
  }
  for (const key of ['ANDROID_AVD_HOME', 'ANDROID_USER_HOME', 'ANDROID_EMULATOR_HOME']) {
    assert.throws(() => admit({ scenarios, manifest: {}, environment: { [key]: '/foreign' } }));
  }
  for (const scenario of [{ ios: { simulatorName: 'foreign' } }, { android: { avdName: 'foreign' } }]) {
    assert.throws(() => admit({ scenarios: [scenario], manifest: {}, environment: {} }));
  }
  for (const number of [1, 2, 3, 4]) {
    const source = readFileSync(new URL(`../Test${number}/orchestrator.mjs`, import.meta.url), 'utf8');
    const start = source.indexOf('async function main() {'), end = source.indexOf('\n}\n', start) + 2;
    const main = runInNewContext(source.slice(start, end) + '\nmain;', { scenario: scenarios[0],
      assertExistingDeviceRun: options => admit({ ...options, environment: { E2E_ANDROID_OWNED_PROFILE: '1' } }) });
    await assert.rejects(main(), /forbidden/);
    const registrationFailure = new Error('Controlled canonical registration rejection');
    const identityRedactor = { addSensitiveValues: () => {} };
    const registered = runInNewContext(source.slice(start, end) + '\nmain;', { scenario: scenarios[0],
      prepareIosCallerIdentity: () => ({ scope: { namespace: 'controlled-admission' } }),
      scenarioPath: '/controlled/scenario.json', artifactsDirectory: '/controlled/artifacts', testNumber: number,
      namespaceRedactor: identityRedactor, errorRedactor: identityRedactor, diagnosticRedactor: identityRedactor,
      assertExistingDeviceRun: options => admit({ ...options, environment: {} }),
      readExistingAndroidAvd: name => { assert.equal(name, 'Pixel_4a'); throw registrationFailure; },
      loadBatchProducts: () => assert.fail('Registration must precede product/client preparation') });
    await assert.rejects(registered(), error => error === registrationFailure);
  }
});

test('scenario preparation rejects owned claims before even local environment validation', async () => {
  for (const claim of [{ ownedIosProfile: {} }, { ownedAndroidLease: {} }]) {
    await assert.rejects(supervisor.prepareScenarioPlatforms({ scenario: {}, ...claim,
      verifyEnvironment: () => assert.fail('ownership must fail before environment/SDK effects') }));
  }
});

test('borrowed-device reset refuses legacy ownership without uninstall or Keychain commands', async () => {
  for (const platform of [{ platform: 'iOS', udid: UUID, ownedIosProfile: {} }, { platform: 'Android', avdName: 'Pixel_4a', ownedAndroidProfile: {} }]) {
    const calls = [];
    await assert.rejects(resetE2eNativeAppState({ scenario: {}, platforms: [platform],
      runCommand: async (...args) => { calls.push(args); assert.fail('no reset mutation'); },
      discoverSdk: () => { calls.push('SDK'); assert.fail('no SDK'); } }));
    assert.deepEqual(calls, []);
  }
});

test('serial XCTest arguments bind one existing destination with parallel cloning disabled', () => {
  assert.equal(typeof supervisor.iosSerialTestArguments, 'function');
  assert.deepEqual(supervisor.iosSerialTestArguments(UUID), [
    '-destination', 'id=' + UUID, '-parallel-testing-enabled', 'NO',
    '-maximum-parallel-testing-workers', '1', '-maximum-concurrent-test-simulator-destinations', '1',
  ]);
  for (const id of ['', 'foreign', UUID.toLowerCase()]) assert.throws(() => supervisor.iosSerialTestArguments(id));
});

test('existing Android registration binds regular canonical config and data, never a private root', t => {
  const home = realpathSync(mkdtempSync(join(tmpdir(), 'existing-avd-control-')));
  t.after(() => rmSync(home, { recursive: true, force: true }));
  const avds = join(home, '.android', 'avd'), data = join(avds, 'Pixel_4a.avd');
  mkdirSync(data, { recursive: true });
  writeFileSync(join(avds, 'Pixel_4a.ini'), 'path=' + data + '\n');
  writeFileSync(join(data, 'config.ini'), 'AvdId=Pixel_4a\n');
  assert.equal(typeof supervisor.readExistingAndroidAvd, 'function');
  const actual = supervisor.readExistingAndroidAvd('Pixel_4a', { homeDirectory: home });
  assert.equal(actual.dataPath, data);
  assert.equal(actual.avdName, 'Pixel_4a');
  assert.throws(() => supervisor.readExistingAndroidAvd('foreign', { homeDirectory: home }));
  for (const config of ['AvdId=foreign\n', 'AvdId=Pixel_4a\nAvdId=Pixel_4a\n']) {
    writeFileSync(join(data, 'config.ini'), config);
    assert.throws(() => supervisor.readExistingAndroidAvd('Pixel_4a', { homeDirectory: home }));
  }
  writeFileSync(join(data, 'config.ini'), 'AvdId=Pixel_4a\n');
  writeFileSync(join(avds, 'Pixel_4a.ini'), 'path=' + join(home, 'foreign') + '\n');
  assert.throws(() => supervisor.readExistingAndroidAvd('Pixel_4a', { homeDirectory: home }));
  rmSync(join(avds, 'Pixel_4a.ini')); symlinkSync(join(data, 'config.ini'), join(avds, 'Pixel_4a.ini'));
  assert.throws(() => supervisor.readExistingAndroidAvd('Pixel_4a', { homeDirectory: home }));
});

test('low-disk AVD request is refused without starting or wiping any emulator', async () => {
  const cleaner = await import('../clean-ui-e2e-space.mjs');
  assert.equal(typeof cleaner.resetRequestedAndroidAvd, 'function');
  await assert.rejects(cleaner.resetRequestedAndroidAvd('Pixel_4a'), /forbidden|preserv|reuse/i);
});


// Named synthetic resource/audit I/O only. These controls exercise runtime
// ownership, not physical resource measurement or an actual F1 acceptance.
function rangePreparationPorts() {
  const GiB = 1024 ** 3;
  return {
    E2E_BUILD_PROFILE, planPreparationBudget, createPreparationResourceBudget, createPreparationMetrics, assertAcceptedRunnerLineage,
    measurePreparationInputs: () => ({ snapshotBytes: 1, androidConfigurationCount: 1, androidConfigurationKeys: ['defaultLocal'],
      iosRequired: false, wasmRequired: true, cliRequired: true, androidPairBytes: 1, wasmBytes: 1, cliBytes: 1,
      nativeBytes: 1, supportingBytes: 1, serverBytes: 1 }),
    readPreparationObservation: async () => ({ freeBytes: 64 * GiB, physicalBytes: 16 * GiB, availableBytes: 4 * GiB,
      swapUsedBytes: 0, swapTotalBytes: GiB }),
    writeFileSync: path => { assert.match(path, /phase-metrics-/); },
    createRunnerAudit: async () => ({ scope: 'SYNTHETIC_HOST_AUDIT_PORT' }),
    validateRunnerAudit: () => ({ noncompiled: {}, membership: {}, strict: {}, scenario: {} }),
    releaseHealthyRunnerOwnedRuntimes: async () => { throw Error('Controlled release unavailable; exercise owned cleanup fallback'); },
  };
}

test('range low-disk pause awaits current-owned cleanup and keeps failed ownership fail-closed', { timeout: 2000 }, async () => {
  const source = readFileSync(new URL('../run-ui-e2e-range.mjs', import.meta.url), 'utf8');
  const start = source.indexOf('async function executeRange('), end = source.indexOf('\n}\n', start) + 2;
  const cleanupStart = source.indexOf('async function finishRangeOwnership(');
  const body = source.slice(cleanupStart, end) + '\nexecuteRange;';
  for (const cleanupFails of [false, true]) {
    const runId = 'controlled-range-resume', events = [], saved = [];
    const manifest = { runId, range: { from: 1, to: 2 }, status: 'running', tests: [] };
    const entries = [1, 2].map(testNumber => ({ testNumber, scenarios: [{ value: { android: { avdName: 'Pixel_4a' } } }] }));
    const disks = [22, 22, 10]; let cleanupCalls = 0, finished = false, release, started;
    const barrier = new Promise(resolve => { release = resolve; });
    const cleanupStarted = new Promise(resolve => { started = resolve; });
    const collector = { writeEvent: () => {}, writeOutput: () => {}, redactor: { redactText: value => value }, close: async () => { events.push('collector-close'); } };
    const enter = runInNewContext(body, {
      ...rangePreparationPorts(),
      assertExistingDeviceRun: options => supervisor.assertExistingDeviceRun({ ...options, environment: {} }),
      createScenarioRuntimeProfile: ({ testNumber }) => ({ testNumber }),
      batchEnvironmentPaths: () => ({}), coreRoot: '/controlled/core', composeRoot: '/controlled/compose', resolve: join,
      createBatchEnvironmentGuard: () => ({ prepare: async () => { events.push('prepare'); }, restore: async () => { events.push('restore'); } }),
      createBatchProductPreparation: () => ({ nativePreparation: () => undefined }), runPreparationCommand: () => assert.fail('No real preparation command'),
      deletionEntryComplete, interruptedBy: null, diskThresholdGiB: 15, postCleanupMinimumGiB: 12,
      announce: () => {}, reportTestStage: () => {}, readFreeDiskGiB: () => { assert.ok(disks.length); return disks.shift(); },
      runDiskCleaner: () => ({ status: 0, afterGiB: 11 }), writeManifest: (_path, value) => { saved.push(JSON.parse(JSON.stringify(value))); },
      process: { stdout: { write: () => {} }, stderr: { write: () => {} } },
      runEntry: async (entry, run, preparation) => {
        assert.equal(entry.testNumber, 1, 'Second actor must not start below floor');
        await preparation.environmentGuard.prepare();
        run.onRuntimeStarted({ platform: 'Android', avdName: 'Pixel_4a', serial: 'emulator-5554' });
        events.push('actor-drained'); return { test: 1, status: 'passed', exitCode: 0 };
      },
      stopRunnerOwnedRuntimes: async runtimes => {
        cleanupCalls++; events.push('cleanup-start'); started();
        assert.equal(runtimes, manifest.runtimeOwnership); assert.equal(runtimes.length, 1);
        assert.equal(runtimes[0].platform, 'Android'); assert.equal(runtimes[0].avdName, 'Pixel_4a');
        assert.equal(runtimes[0].serial, 'emulator-5554');
        assert.ok(events.includes('actor-drained'), 'Actor must drain before cleanup');
        await barrier; events.push('cleanup-settled');
        if (cleanupFails) throw Error('controlled cleanup refusal');
      },
    });
    const input = { entries, options: { preserveEvidence: true }, runId, runDirectory: '/controlled/run', manifestPath: '/controlled/run/run.json', manifest, collector };
    const pending = enter(input).then(code => { finished = true; return code; }); void pending.catch(() => {});
    try {
      await Promise.race([cleanupStarted, pending.then(() => assert.fail('Range finished before owned cleanup started'))]);
      assert.equal(cleanupCalls, 1, 'A paused run must await its current-process runtime cleanup');
      assert.equal(finished, false); assert.equal(manifest.runtimeOwnership.length, 1);
      assert.equal(events.includes('restore'), false, 'Restore cannot overtake child cleanup');
      assert.equal(events.includes('collector-close'), false);
      release(); assert.equal(await pending, 75);
      assert.equal(manifest.status, 'paused-low-disk'); assert.equal(manifest.pendingTest, 2);
      assert.equal(manifest.tests.length, 1); assert.equal(manifest.tests[0].status, 'passed');
      assert.equal(manifest.environmentRestoration.status, 'passed');
      assert.match(manifest.runtimeRetentionWarning, /Healthy runtime release unavailable/);
      assert.ok(events.indexOf('cleanup-settled') < events.indexOf('restore'));
      assert.ok(events.indexOf('restore') < events.indexOf('collector-close'));
      assert.equal(manifest.runtimeOwnership.length, cleanupFails ? 1 : 0);
      assert.equal(saved.at(-1).runtimeOwnership.length, cleanupFails ? 1 : 0);
      if (cleanupFails) {
        assert.equal(manifest.runtimeCleanupWarning, 'controlled cleanup refusal');
        const resumed = JSON.parse(JSON.stringify(manifest)); resumed.status = 'running'; resumed.pendingTest = null;
        resumed.runtimeOwnership[0].emulatorPid = 4242; // Persisted PID never grants a new process authority.
        const refuseResume = runInNewContext(body, {
          assertExistingDeviceRun: options => supervisor.assertExistingDeviceRun({ ...options, environment: {} }),
          createBatchEnvironmentGuard: () => assert.fail('No guard from persisted ownership'),
          stopRunnerOwnedRuntimes: () => assert.fail('No foreign/persisted PID cleanup'),
        });
        await assert.rejects(refuseResume({ ...input, manifest: resumed }), /Persisted runtime ownership/);
      } else {
        assert.doesNotThrow(() => supervisor.assertExistingDeviceRun({ scenarios: entries.map(row => row.scenarios[0].value), manifest, environment: {} }));
      }
    } finally { release(); await pending.catch(() => {}); }
  }
});

async function actualTest4Preflight(t, devices = [target]) {
  const directory = realpathSync(mkdtempSync(join(tmpdir(), 'test4-preflight-control-')));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const readinessPath = join(directory, 'readiness.json'), runId = 'controlled-preflight';
  writeFileSync(readinessPath, JSON.stringify({ runId, platforms: [
    { platform: 'iOS', simulatorName: 'iPhone 17', udid: UUID },
  ] }));
  const source = readFileSync(new URL('../Test4/orchestrator.mjs', import.meta.url), 'utf8');
  const start = source.indexOf('async function preflightSelectedRuntimes() {');
  const end = source.indexOf('\n}\n', start) + 2;
  assert.ok(start >= 0 && end > start, 'Consume the actual complete preflight body');
  const { resolve } = await import('node:path');
  const calls = [], diagnostics = [], sdk = { sdkRoot: '/controlled/android-sdk', emulatorPath: '/controlled/emulator' };
  const developer = '/controlled/Xcode.app/Contents/Developer';
  const visibleUi = resolve(developer, '../Applications/DeviceHub.app');
  const capture = async (file, args) => {
    if (file === sdk.emulatorPath) {
      assert.deepEqual(Array.from(args), ['-list-avds']); calls.push('list-avds'); return { stdout: 'Pixel_4a\n' };
    }
    if (file === 'xcrun') {
      assert.deepEqual(Array.from(args), ['simctl', 'list', 'devices', 'available', '-j']); calls.push('list-ios');
      return { stdout: JSON.stringify({ devices: { [runtime]: devices } }) };
    }
    if (file === 'xcode-select') {
      assert.deepEqual(Array.from(args), ['-p']); calls.push('developer'); return { stdout: developer + '\n' };
    }
    assert.fail('No real SDK or mutation command in preflight control');
  };
  const context = {
    registrationOnly: false, registrationPlatforms: [], androidSdkTools: undefined,
    scenario: { android: { avdName: 'Pixel_4a' }, ios: { simulatorName: 'iPhone 17' } },
    discoverAndroidSdk: () => { calls.push('discover-sdk'); return sdk; }, runAndCapture: capture,
    resolveAndroidScenarioAvdName: supervisor.resolveAndroidScenarioAvdName,
    resolveIosScenarioSimulator: options => {
      calls.push('select-ios');
      return supervisor.resolveIosScenarioSimulator({ ...options, readinessPath, runId });
    },
    findOnlineAndroidAvd: async (actualSdk, avd, port) => {
      assert.equal(actualSdk, sdk); assert.equal(avd, 'Pixel_4a'); assert.equal(port, capture);
      calls.push('online-android'); return 'controlled-serial';
    },
    resolve, existsSync: path => { calls.push('visible-ui'); assert.equal(path, visibleUi); return true; },
    writeDiagnostic: text => diagnostics.push(text), console: { log: () => calls.push('success-log') },
  };
  const enter = () => runInNewContext(source.slice(start, end) + '\npreflightSelectedRuntimes;', context)();
  return { enter, context, calls, diagnostics, readinessPath, sdk, visibleUi };
}

test('actual Test4 preflight retains the strictly selected existing iOS device in its snapshot', async t => {
  const f = await actualTest4Preflight(t); let snapshot;
  await assert.doesNotReject(async () => { snapshot = await f.enter(); },
    'Actual preflight must retain its strictly resolved simulator binding');
  assert.deepEqual(JSON.parse(JSON.stringify(snapshot)), {
    android: { sdkRoot: f.sdk.sdkRoot, avd: 'Pixel_4a', onlineSerial: 'controlled-serial' },
    ios: { name: target.name, udid: target.udid, state: target.state, visibleUi: f.visibleUi },
  });
  assert.deepEqual(f.calls, ['discover-sdk', 'list-avds', 'list-ios', 'select-ios',
    'developer', 'visible-ui', 'online-android', 'success-log']);
  assert.equal(f.diagnostics.length, 1);
});

test('actual Test4 preflight rejects foreign missing and ambiguous existing iOS targets before later ports', async t => {
  for (const devices of [[{ ...target, udid: '11111111-1111-1111-1111-111111111111' }], [], [target, target]]) {
    const f = await actualTest4Preflight(t, devices);
    await assert.rejects(f.enter(), /iOS|simulator|UUID|device/i);
    assert.deepEqual(f.calls, ['discover-sdk', 'list-avds', 'list-ios', 'select-ios']);
    assert.equal(f.diagnostics.length, 0);
  }
});

test('actual Test4 Web-only registration preflight touches no native ports', async t => {
  const f = await actualTest4Preflight(t);
  f.context.registrationOnly = true; f.context.registrationPlatforms = ['web'];
  assert.deepEqual(JSON.parse(JSON.stringify(await f.enter())), {});
  assert.deepEqual(f.calls, []); assert.equal(f.diagnostics.length, 1);
});


test('actual range retains exact published runtime object until cleanup and clears only after it joins', async () => {
  const source = readFileSync(new URL('../run-ui-e2e-range.mjs', import.meta.url), 'utf8');
  const start = source.indexOf('async function executeRange('), end = source.indexOf('\n}\n', start) + 2;
  const cleanupStart = source.indexOf('async function finishRangeOwnership(');
  const body = source.slice(cleanupStart, end) + '\nexecuteRange;';
  const runtime = { platform: 'Android', avdName: 'Pixel_4a', serial: 'emulator-fixture' };
  const manifest = { runId: 'object-identity', range: { from: 1, to: 2 }, status: 'running', tests: [] };
  const events = [], disks = [22, 22, 10], saved = []; let cleaned;
  const enter = runInNewContext(body, {
    ...rangePreparationPorts(),
    assertExistingDeviceRun: options => supervisor.assertExistingDeviceRun({ ...options, environment: {} }),
    createScenarioRuntimeProfile: ({ testNumber }) => ({ testNumber }),
    batchEnvironmentPaths: () => ({}), coreRoot: '/controlled/core', composeRoot: '/controlled/compose', resolve: join,
    createBatchEnvironmentGuard: () => ({ prepare: async () => {}, restore: async () => { events.push('restore'); } }),
    createBatchProductPreparation: () => ({ nativePreparation: () => undefined }),
    runPreparationCommand: () => assert.fail('No external preparation command'),
    deletionEntryComplete, interruptedBy: null, diskThresholdGiB: 15, postCleanupMinimumGiB: 12,
    announce: () => {}, reportTestStage: () => {}, readFreeDiskGiB: () => { assert(disks.length); return disks.shift(); },
    runDiskCleaner: () => ({ status: 0, afterGiB: 11 }),
    writeManifest: (_path, value) => saved.push(JSON.parse(JSON.stringify(value))),
    process: { stdout: { write() {} }, stderr: { write() {} } },
    runEntry: async (entry, run) => { assert.equal(entry.testNumber, 1); run.onRuntimeStarted(runtime);
      events.push('actor-closed'); return { test: 1, status: 'passed', exitCode: 0 }; },
    stopRunnerOwnedRuntimes: async values => { cleaned = values[0]; assert(events.includes('actor-closed'));
      assert.equal(values.length, 1); events.push('cleanup-joined'); },
  });
  const code = await enter({ entries: [1, 2].map(testNumber => ({ testNumber,
    scenarios: [{ value: { android: { avdName: 'Pixel_4a' } } }] })), options: { preserveEvidence: true },
    runId: manifest.runId, runDirectory: '/controlled/run', manifestPath: '/controlled/run/run.json', manifest,
    collector: { writeEvent() {}, writeOutput() {}, redactor: { redactText: value => value }, close: async () => events.push('collector-closed') } });
  assert.equal(code, 75); assert.equal(cleaned, runtime, 'WeakMap authority requires original callback object');
  assert.equal(manifest.runtimeOwnership.length, 0); assert.equal(saved.at(-1).runtimeOwnership.length, 0);
  assert(events.indexOf('cleanup-joined') < events.indexOf('restore'));
  assert(events.indexOf('restore') < events.indexOf('collector-closed'));
});

test('actual Test51 callback retains runtime reference without launching its scenario', () => {
  const source = readFileSync(new URL('../Test51/local-run.mjs', import.meta.url), 'utf8');
  const matches = [...source.matchAll(/onRuntimeStarted:\s*(\([^)]*\)\s*=>\s*ownedRuntimes\.push\([^)]*\))/g)];
  assert.equal(matches.length, 1, 'One existing publication seam, no fallback source');
  const ownedRuntimes = [], runtime = { platform: 'Android', avdName: 'Pixel_4a', serial: 'emulator-fixture' };
  const callback = runInNewContext('(' + matches[0][1] + ')', { ownedRuntimes });
  callback(runtime); assert.equal(ownedRuntimes.length, 1); assert.equal(ownedRuntimes[0], runtime);
});
