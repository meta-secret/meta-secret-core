import assert from 'node:assert/strict';
import * as fs from 'node:fs';
import { createHash, randomUUID } from 'node:crypto';
import { EventEmitter } from 'node:events';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import test from 'node:test';
import { findOnlineAndroidAvd, withAndroidSdkPath } from './android-sdk.mjs';
import { createScenarioRuntimeProfile, coreHttpProofsReady } from './scenario-runtime-profile.mjs';
import { runEntry } from '../run-ui-e2e-range.mjs';

// Existing-seam RED file: missing new modules are optional ports, never an import/collection failure.
const profileUrl = new URL('./android-owned-profile.mjs', import.meta.url);
const handoffUrl = new URL('./android-owned-handoff.mjs', import.meta.url);
const profileApi = fs.existsSync(profileUrl) ? await import(profileUrl) : {};
const handoffApi = fs.existsSync(handoffUrl) ? await import(handoffUrl) : {};
const sourceUrl = new URL('./platform-supervisor.mjs', import.meta.url);
const source = fs.readFileSync(sourceUrl, 'utf8');
const hash = bytes => createHash('sha256').update(bytes).digest('hex');

function fixture(t, options = {}) {
  const directory = fs.realpathSync(fs.mkdtempSync(resolve(tmpdir(), 'owned-avd-runtime-control-')));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const root = resolve(directory, 'owned'), content = resolve(root, 'content'), avdHome = resolve(root, 'user/.android/avd');
  const sdk = { sdkRoot: resolve(directory, 'sdk'), emulatorPath: resolve(directory, 'sdk/emulator/emulator'), adbPath: resolve(directory, 'sdk/adb'), pathPrefix: '/controlled' };
  const runId = 'controlled-range', avdName = 'Pixel_4a', physicalAvdName = 'MetaSecret-e2e-12345678-1234-1234-1234-123456789abc';
  const events = [], children = [], ready = [], calls = []; let borrowed = options.borrowed, waitCalls = 0, removed = false;
  function write(path, text) { fs.mkdirSync(resolve(path, '..'), { recursive: true }); fs.writeFileSync(path, text); }
  const encode = values => Object.entries(values).map(([key, value]) => `${key}=${value}\n`).join('');
  const config = { AvdId: physicalAvdName, 'abi.type': 'arm64-v8a', 'hw.cpu.arch': 'arm64', target: 'android-36',
    'image.sysdir.1': 'system-images/android-36/google_apis_ps16k/arm64-v8a/', 'hw.sdCard': 'yes', 'sdcard.size': '512M' };
  const hardware = { 'avd.id': physicalAvdName, 'avd.name': physicalAvdName, 'android.avd.home': avdHome, 'android.sdk.root': sdk.sdkRoot,
    'hw.cpu.arch': 'arm64', 'hw.sdCard': 'true', 'disk.dataPartition.path': resolve(content, 'userdata-qemu.img'),
    'disk.cachePartition.path': resolve(content, 'cache.img'), 'disk.encryptionKeyPartition.path': resolve(content, 'encryptionkey.img'),
    'hw.sdCard.path': resolve(content, 'sdcard.img') };
  const qemu = resolve(sdk.sdkRoot, 'emulator/qemu/darwin-aarch64/qemu-system-aarch64');
  write(qemu, 'controlled executable metadata stub, never executed');
  write(resolve(content, 'config.ini'), encode(config));
  write(resolve(avdHome, physicalAvdName + '.ini'), 'path=' + content + '\n');
  write(resolve(content, 'hardware-qemu.ini'), encode(hardware));
  for (const name of ['userdata-qemu.img', 'cache.img', 'encryptionkey.img']) write(resolve(content, name), 'controlled image metadata stub');
  const sd = fs.openSync(resolve(content, 'sdcard.img'), 'wx'); fs.ftruncateSync(sd, 512 * 1024 ** 2); fs.closeSync(sd);
  const stat = fs.lstatSync(root);
  const privateEnv = { ANDROID_HOME: sdk.sdkRoot, ANDROID_SDK_ROOT: sdk.sdkRoot, ANDROID_USER_HOME: resolve(root, 'user/.android'),
    ANDROID_AVD_HOME: avdHome, ANDROID_EMULATOR_HOME: resolve(root, 'user/.android'), ANDROID_SDK_HOME: resolve(root, 'user'), XDG_CACHE_HOME: resolve(root, 'cache') };
  const profile = { version: 1, runId, avdName, physicalAvdName, root, content, avdHome, identity: { dev: stat.dev, ino: stat.ino }, privateEnv,
    configPath: resolve(content, 'config.ini'), configSha256: hash(fs.readFileSync(resolve(content, 'config.ini'))), sdkRoot: sdk.sdkRoot,
    launchArgs: ['-avd', physicalAvdName, '-datadir', content, '-data', resolve(content, 'userdata-qemu.img'), '-cache', resolve(content, 'cache.img'), '-no-snapshot', '-no-snapstorage'] };
  const lease = { intent: profile, get removed() { return removed; }, verify(scope = {}) {
    if (removed || scope.avdName && scope.avdName !== avdName || scope.runId && scope.runId !== runId) throw new Error('controlled lease mismatch'); return profile;
  }, remove() { events.push('remove'); assert(children.every(child => child.closed), 'Every actual child must close before root deletion'); fs.rmSync(root, { recursive: true }); removed = true; } };
  const env = { PATH: '', E2E_ANDROID_OWNED_PROFILE: '1', E2E_RANGE_RUN: '1', E2E_RUN_ID: runId };
  const runCommand = async (file, args) => {
    calls.push({ file, args: [...args] });
    if (file === 'ps') return { stdout: [
      ...(borrowed ? [`9876 ${qemu} -avd ${physicalAvdName} -android-hw ${content}/hardware-qemu.ini`] : []),
      ...children.filter(child => !child.closed).map(child => `${child.pid} ${qemu} -avd ${physicalAvdName} -android-hw ${content}/hardware-qemu.ini`),
    ].join('\n') };
    assert([sdk.emulatorPath, sdk.adbPath].includes(file), 'No real host/SDK command port');
    if (file === sdk.emulatorPath) return { stdout: physicalAvdName + '\n' };
    if (args[0] === 'version') return { stdout: 'Version controlled\n' };
    if (args[0] === 'start-server') return { stdout: '' };
    if (args[0] === 'devices') return { stdout: 'List of devices attached\n' + (borrowed || children.some(child => !child.closed) ? 'emulator-6550\tdevice\n' : '') };
    if (args.includes('name')) return { stdout: physicalAvdName + '\nOK\n' };
    if (args.includes('sys.boot_completed')) return { stdout: '1' };
    if (args.includes('E2E_PLATFORM_PROBE')) return { stdout: 'E2E_PLATFORM_PROBE' };
    if (args.includes('kill')) { events.push('adb-kill'); return { stdout: '' }; }
    throw new Error('Unexpected controlled command');
  };
  const spawnProcess = (file, args, spawnOptions) => {
    assert.equal(file, sdk.emulatorPath); events.push('spawn');
    assert.equal(spawnOptions.env.ANDROID_AVD_HOME, avdHome);
    const child = new EventEmitter();
    Object.assign(child, { pid: 81000 + children.length, closed: false, args: [...args], unref() {}, kill(signal) {
      events.push(signal);
      if (options.closeMode !== 'never' && (options.closeMode !== 'kill' || signal === 'SIGKILL')) queueMicrotask(() => {
        child.closed = true; events.push('close');
        if (options.replaceOnClose) api.runnerOwnedAndroidProcesses.set(physicalAvdName, { lease: {}, launch: { closed: false } });
        child.emit('exit', 0, null); child.emit('close', 0, null);
      }); return true;
    } }); children.push(child); return child;
  };
  const waitForAvd = async (_sdk, name, callback = () => {}) => {
    assert.equal(name, physicalAvdName); events.push('readiness'); waitCalls += 1;
    if (options.failure && (options.alwaysFail || waitCalls === 1)) throw options.failure;
    callback('emulator-6550'); return 'emulator-6550';
  };
  const context = { ...fs, ...profileApi, ...handoffApi, Buffer, resolve, randomUUID, URL, fileURLToPath, process: { env, platform: 'darwin', stdout: { write() {} }, kill() { assert.fail('No global PID signal'); } },
    promisify: () => runCommand, execFile() { assert.fail('No process'); }, spawn: spawnProcess, discoverAndroidSdk: () => sdk,
    findOnlineAndroidAvd, withAndroidSdkPath, createScenarioRuntimeProfile, coreHttpProofsReady, openVisibleIosDeviceUi() { assert.fail('No iOS'); },
    monitorEventLoopDelay() { assert.fail('No monitor'); }, delay: async () => {}, setTimeout: (callback, ms) => setTimeout(callback, Math.min(ms, 5)), clearTimeout, setInterval, clearInterval };
  const body = source.replace(/^import[\s\S]*?;\n/gm, '').replace(/^export \{.*\};\n/gm, '').replace(/\bexport /g, '').replaceAll('import.meta.url', JSON.stringify(sourceUrl.href));
  const api = runInNewContext(body + '\n({ prepareAndroid, prepareScenarioPlatforms, stopRunnerOwnedRuntimes, checkPlatform, runnerOwnedAndroidProcesses });', context, { timeout: 1000 });
  context.waitForAndroidAvd = waitForAvd;
  const scope = { runDirectory: directory, testNumber: 50, attempt: 1, ownedAndroidLease: lease, runId };
  return { directory, profile, lease, sdk, runId, avdName, env, events, calls, children, ready, api, runCommand, write, config, hardware, qemu,
    saveHardware: () => write(resolve(content, 'hardware-qemu.ini'), encode(hardware)),
    saveConfig() { write(profile.configPath, encode(config)); profile.configSha256 = hash(fs.readFileSync(profile.configPath)); },
    start: () => api.prepareAndroid(avdName, runtime => ready.push(runtime), scope, { androidSdk: () => sdk, runCommand, spawnProcess, waitForAvd }),
    stop: () => api.stopRunnerOwnedRuntimes([{ platform: 'Android', avdName, ownedAndroidLease: lease }]),
    claim(platform) { const path = resolve(directory, 'readiness.json'); write(path, JSON.stringify({ runId, test: 50, attempt: 1, platforms: [platform] })); env.E2E_READINESS_PATH = path; return path; },
    setBorrowed(value) { borrowed = value; } };
}

test('actual supervisor uses physical official profile and reuses the same current lease without another writer', async t => {
  const f = fixture(t), first = await f.start(), second = await f.start();
  assert.equal(first.avdName, 'Pixel_4a'); assert.equal(first.ownedAndroidProfile, f.profile);
  assert.equal(first.emulatorPid, second.emulatorPid); assert.equal(f.children.length, 1);
  assert.deepEqual(f.children[0].args, f.profile.launchArgs); assert(!f.children[0].args.includes('-wipe-data'));
  await f.stop(); assert(!fs.existsSync(f.profile.root)); assert(f.events.indexOf('close') < f.events.indexOf('remove'));
});
test('actual supervisor rejects already running private target without current lease before spawn or restart', async t => {
  const f = fixture(t, { borrowed: true }); await assert.rejects(f.start());
  assert.equal(f.children.length, 0); assert(!f.events.some(event => event.startsWith('SIG'))); assert(fs.existsSync(f.profile.root));
});
test('actual supervisor never adopts JSON as a deletion or launch capability', async t => {
  const f = fixture(t);
  await assert.rejects(f.api.prepareAndroid(f.avdName, () => {}, { runDirectory: f.directory, ownedAndroidLease: JSON.parse(JSON.stringify(f.lease)) }));
  await assert.rejects(f.api.stopRunnerOwnedRuntimes([{ platform: 'Android', ownedAndroidProfile: f.profile }]));
  assert.equal(f.calls.length, 0); assert(fs.existsSync(f.profile.root));
});
test('actual supervisor refuses foreign run before ADB, inventory, spawn or cleanup mutation', async t => {
  const f = fixture(t);
  await assert.rejects(f.api.prepareAndroid(f.avdName, () => {}, { runDirectory: f.directory, runId: 'foreign', ownedAndroidLease: f.lease }));
  assert.equal(f.calls.length, 0); assert.equal(f.children.length, 0); assert(fs.existsSync(f.profile.root));
});
test('actual owned recovery drains exact child then reuses same profile without deleting it', async t => {
  const error = Object.assign(new Error('controlled services readiness'), { code: 'ANDROID_AVD_SHELL_SERVICES_UNRESPONSIVE' });
  const f = fixture(t, { failure: error }), platform = await f.start();
  assert.equal(f.children.length, 2); assert.equal(platform.ownedAndroidProfile, f.profile);
  assert(f.events.indexOf('close') < f.events.lastIndexOf('spawn')); assert(!f.events.includes('remove'));
  await f.stop(); assert.equal(f.events.filter(event => event === 'remove').length, 1);
});
for (const closeMode of ['kill', 'never']) test(`owned close barrier ${closeMode} keeps deletion after actual close only`, async t => {
  const f = fixture(t, { closeMode }); await f.start();
  if (closeMode === 'never') { await assert.rejects(f.stop()); assert(fs.existsSync(f.profile.root)); assert(!f.events.includes('remove')); }
  else { await f.stop(); assert(f.events.includes('SIGKILL')); assert(f.events.indexOf('close') < f.events.indexOf('remove')); }
});
test('replacement launch mapping during actual close cannot authorize root deletion', async t => {
  const f = fixture(t, { replaceOnClose: true }); await f.start(); await assert.rejects(f.stop());
  assert(fs.existsSync(f.profile.root)); assert(!f.events.includes('remove'));
  assert.notEqual(f.api.runnerOwnedAndroidProcesses.get(f.profile.physicalAvdName).lease, f.lease);
});
test('actual wrapper refuses a legacy owned Android lease before readiness or launch', async t => {
  const f = fixture(t);
  await assert.rejects(f.api.prepareScenarioPlatforms({ scenario: {
    server: { url: 'http://127.0.0.1:3000', container: 'meta-secret-e2e-server-control', port: 3000 },
    web: { url: 'http://127.0.0.1:5181' }, android: { avdName: f.avdName } },
    runId: f.runId, runDirectory: f.directory, testNumber: 50, attempt: 1, ownedAndroidLease: f.lease,
    verifyEnvironment: () => assert.fail('No environment mutation for forbidden ownership') }), /forbidden/);
  assert.equal(f.ready.length, 0);
});

function child(number, f, serial) {
  const text = fs.readFileSync(new URL(`../Test${number}/orchestrator.mjs`, import.meta.url), 'utf8');
  const start = text.indexOf('async function prepareAndroidEmulator() {');
  const end = text.indexOf(number === 4 ? 'async function waitForOnlineAndroidAvd(' : 'async function waitForConfiguredAndroidAvd(', start);
  const events = [], sdk = f.sdk;
  const prepare = runInNewContext(text.slice(start, end) + '\nprepareAndroidEmulator;', {
    process: { env: f.env }, console: { log() {}, warn() {} }, scenario: { android: { avdName: f.avdName, bundleId: 'controlled.app' } }, androidSdkTools: sdk,
    androidEmulatorPath: sdk.emulatorPath, findOnlineAndroidAvd: async (_sdk, name) => { assert.equal(name, f.profile.physicalAvdName); return serial; },
    resolveAndroidScenarioAvdName: name => handoffApi.resolveAndroidScenarioAvdName(name, { env: f.env }),
    verifyOwnedAndroidScenarioRuntime: (name, value, ports) => handoffApi.verifyOwnedAndroidScenarioRuntime(name, value, { ...ports, env: f.env }),
    runAndCapture: f.runCommand, run: () => events.push('legacy-launch'), waitForConfiguredAndroidAvd: async () => 'emulator-6550', waitForOnlineAndroidAvd: async () => 'emulator-6550',
    waitForAndroidBoot: async () => events.push('boot'), waitForOnlineAndroidDevice: async () => {}, runAdbWithRetry: async () => {}, unlockAndroidDevice: async () => {},
    waitForAndroidStorage: async () => {}, resetE2eNativeAppState: async options => { events.push('reset'); assert.equal(options.platforms[0].emulatorPid, 81000); },
    runAndWait: async () => events.push('uninstall'), writeDiagnostic() {}, androidSerialForCleanup: null,
  }, { timeout: 1000 });
  return { prepare, events };
}
for (const number of [1, 2, 3, 4]) for (const serial of [null, 'emulator-6552']) {
  test(`actual Test${number} rejects ${serial ?? 'lost'} owned serial before any legacy launch/boot/reset`, async t => {
    const f = fixture(t), platform = await f.start(); f.claim(platform);
    const actual = child(number, f, serial); await assert.rejects(actual.prepare()); assert.deepEqual(actual.events, []); await f.stop();
  });
}
for (const number of [1, 2, 3, 4]) test(`actual Test${number} consumes exact physical profile/serial/current QEMU`, async t => {
  const f = fixture(t), platform = await f.start(); f.claim(platform);
  const actual = child(number, f, platform.serial); assert.equal(await actual.prepare(), platform.serial);
  assert(actual.events.includes('boot')); assert(!actual.events.includes('legacy-launch'));
  if (number === 4) assert(actual.events.includes('reset')); await f.stop();
});
for (const mutation of ['foreign-run', 'foreign-config', 'foreign-writable', 'foreign-hardware-metadata']) test(`child handoff rejects ${mutation} before further work`, async t => {
  const f = fixture(t), platform = await f.start(); f.claim(platform);
  if (mutation === 'foreign-run') f.env.E2E_RUN_ID = 'foreign';
  if (mutation === 'foreign-config') f.write(f.profile.configPath, 'changed');
  if (mutation === 'foreign-writable') f.write(resolve(f.profile.content, 'hardware-qemu.ini'), 'disk.dataPartition.path=/foreign/userdata\n');
  if (mutation === 'foreign-hardware-metadata') {
    const hardware = resolve(f.profile.content, 'hardware-qemu.ini'), external = resolve(f.directory, 'foreign-metadata');
    f.write(external, fs.readFileSync(hardware)); fs.unlinkSync(hardware); fs.symlinkSync(external, hardware);
  }
  const actual = child(4, f, platform.serial); await assert.rejects(actual.prepare()); assert.deepEqual(actual.events, []); await f.stop();
});
test('actual range runEntry obtains explicit lease before platform/reset/child work', async t => {
  const f = fixture(t), events = [], logPath = resolve(f.directory, 'range.log'); fs.writeFileSync(logPath, '');
  const collector = { runDirectory: f.directory, startTest: () => logPath, writeEvent() {}, finishTest: async () => {}, redactor: { redactText: value => value } };
  const stop = new Error('controlled platform stop');
  const result = await runEntry({ testNumber: 50, script: 'test-50', scenarios: [{ value: { server: { url: 'http://127.0.0.1:3000', container: 'meta-secret-e2e-server-control', port: 3000 }, web: { url: 'http://127.0.0.1:5181' }, android: { avdName: f.avdName } } }] },
    { attempt: 1, collector, runId: f.runId, onRuntimeStarted() {} },
    { getAndroidProfile: async name => { assert.equal(name, f.avdName); events.push('lease'); return f.lease; },
      environmentGuard: { prepare: async () => events.push('environment') }, batchProducts: { hasPersistedProof: true, prepare: async () => { events.push('products'); return '/controlled/index'; }, nativePreparation: () => ({}) } },
    { prepareScenarioPlatforms: async options => { assert.equal(options.ownedAndroidLease, f.lease); events.push('platform'); throw stop; },
      resetE2eScenarioState: () => assert.fail('No reset'), spawn: () => assert.fail('No actor'), discoverAttemptArtifacts: () => [] });
  assert.deepEqual(events, ['environment', 'products', 'lease', 'platform']); assert.equal(result.error, stop.message); assert.equal(result.status, 'readiness-failed');
});

test('actual range refuses legacy private handoff and current existing-device child retains its primary exit without private homes', async t => {
  const f = fixture(t), platform = await f.start(), reportPath = f.claim(platform), logPath = resolve(f.directory, 'range.log');
  fs.writeFileSync(logPath, ''); let childEnv, resetCalls = 0, spawnCalls = 0;
  const collector = { runDirectory: f.directory, startTest: () => logPath, writeEvent() {}, writeOutput() {}, finishTest: async () => {}, redactor: { redactText: value => value } };
  const entry = { testNumber: 50, script: 'test-50', scenarios: [{ value: { server: { url: 'http://127.0.0.1:3000', container: 'meta-secret-e2e-server-control', port: 3000 }, web: { url: 'http://127.0.0.1:5181' }, android: { avdName: f.avdName } } }] };
  const run = { attempt: 1, collector, runId: f.runId, onRuntimeStarted() {} };
  const preparation = { environmentGuard: { prepare: async () => {} }, batchProducts: { hasPersistedProof: true, prepare: async () => '/controlled/index', nativePreparation: () => ({}) } };
  let currentPlatform = platform;
  const services = { prepareScenarioPlatforms: async () => ({ platforms: [currentPlatform], reportPath }),
      resetE2eScenarioState: async () => { resetCalls++; return {}; },
      startPlatformLivenessMonitor: () => () => {}, discoverAttemptArtifacts: () => [],
      spawn: (_file, _args, options) => { spawnCalls++; childEnv = options.env; const child = new EventEmitter(); child.stdout = new EventEmitter(); child.stderr = new EventEmitter();
        queueMicrotask(() => child.emit('close', 7, null)); return child; } };
  const refused = await runEntry(entry, run, preparation, services);
  assert.equal(refused.status, 'readiness-failed'); assert.match(refused.error, /Legacy owned profile handoff\/resume is forbidden/);
  assert.equal(childEnv, undefined); assert.equal(resetCalls, 0); assert.equal(spawnCalls, 0);
  // Current public range admits only an existing target, never private profile JSON.
  currentPlatform = { platform: 'Android', avdName: f.avdName, serial: 'emulator-fixture' }; f.claim(currentPlatform);
  const result = await runEntry(entry, run, preparation, services);
  assert.equal(resetCalls, 1); assert.equal(spawnCalls, 1);
  assert.equal(childEnv.E2E_ANDROID_OWNED_PROFILE, undefined);
  assert.equal(childEnv.ANDROID_AVD_HOME, process.env.ANDROID_AVD_HOME);
  assert.notEqual(childEnv.ANDROID_AVD_HOME, f.profile.avdHome);
  assert.equal(childEnv.JAVA_HOME, process.env.JAVA_HOME); assert.equal(childEnv.E2E_READINESS_PATH, reportPath);
  assert.equal(result.exitCode, 7); assert.notEqual(result.status, 'passed'); await f.stop();
});

for (const scope of ['missing', 'foreign']) test(`actual reset rejects ${scope} owned handoff before server/app mutation`, async t => {
  const f = fixture(t), platform = await f.start(); f.claim(platform); const mutations = [];
  const url = new URL('./e2e-state-reset.mjs', import.meta.url), text = fs.readFileSync(url, 'utf8');
  const reset = runInNewContext(text.replace(/^import[\s\S]*?;\n/gm, '').replace(/\bexport /g, '') + '\nresetE2eScenarioState;', {
    ...handoffApi, process: { env: f.env }, promisify: () => () => assert.fail('No command default'), execFile() {}, delay: async () => {},
    verifyOwnedIosResetTarget() { assert.fail('No iOS'); }, verifyIosRuntimeHealth() { assert.fail('No iOS'); },
    findOnlineAndroidAvd, discoverAndroidSdk: () => f.sdk,
  }, { filename: fileURLToPath(url), timeout: 1000 });
  const target = scope === 'missing' ? { ...platform, ownedAndroidProfile: undefined }
    : { ...platform, ownedAndroidProfile: { ...platform.ownedAndroidProfile, runId: 'foreign' } };
  await assert.rejects(reset({ scenario: { server: { container: 'meta-secret-e2e-server-control', url: 'http://127.0.0.1:3000' }, android: { bundleId: 'controlled.app' } },
    platforms: [target], runId: f.runId, readinessPath: f.env.E2E_READINESS_PATH,
    runCommand: async () => { mutations.push('command'); throw new Error('controlled stop on first forbidden mutation port'); }, discoverSdk: () => f.sdk }));
  assert.deepEqual(mutations, []); await f.stop();
});

test('actual liveness checks the current physical target and preserves serial/boot/heartbeat guards', async t => {
  const f = fixture(t), platform = await f.start();
  await f.api.checkPlatform(platform, { runCommand: f.runCommand, androidSdk: () => f.sdk,
    findAndroidAvd: async (_sdk, name) => { assert.equal(name, f.profile.physicalAvdName); return platform.serial; } });
  assert(f.calls.some(call => call.args.includes('sys.boot_completed')));
  assert(f.calls.some(call => call.args.includes('E2E_PLATFORM_PROBE'))); await f.stop();
});

// The real exported verifier consumes measured argv/metadata through a finite, non-executing ps port.
function measuredRuntime(t) {
  const f = fixture(t), state = { ...f, linePid: 81000, duplicate: false, tokens: [f.qemu, ...f.profile.launchArgs] };
  state.platform = { platform: 'Android', avdName: f.avdName, state: 'Booted', serial: 'emulator-6550',
    emulatorPid: 81000, ownedAndroidProfile: f.profile };
  delete f.hardware['hw.sdCard.path']; f.saveHardware();
  state.check = () => handoffApi.verifyOwnedAndroidRuntime(state.platform, { runId: f.runId,
    runCommand: async (file, args, options) => {
      assert.equal(file, 'ps'); assert.deepEqual(args, process.platform === 'darwin' ? ['-axo', 'pid=,command='] : ['-eo', 'pid=,args=']);
      assert.equal(options.timeout, 10_000); f.calls.push({ file, args });
      const line = `${state.linePid} ${state.tokens.join(' ')}`;
      return { stdout: state.duplicate ? line + '\n' + line : line };
    } });
  return state;
}
async function rejectMeasured(t, amend) {
  const f = measuredRuntime(t); assert.equal(await f.check(), f.platform);
  await amend(f); await assert.rejects(f.check()); f.lease.remove(); assert(!fs.existsSync(f.profile.root));
}

test('measured frontend exact launch vector accepts qualified ARM36 automatic SD omission', async t => {
  const f = measuredRuntime(t), paths = [f.profile.configPath, resolve(f.profile.content, 'hardware-qemu.ini'), f.qemu];
  const before = paths.map(path => fs.readFileSync(path));
  assert.equal(await f.check(), f.platform); assert.equal(f.calls.length, 1);
  assert.deepEqual(paths.map(path => fs.readFileSync(path)), before);
  assert.equal(fs.lstatSync(resolve(f.profile.content, 'sdcard.img')).size, 512 * 1024 ** 2);
  f.lease.remove(); assert(!fs.existsSync(f.profile.root));
});
test('frontend and retained legacy routes accept the exact present owned SD image', async t => {
  for (const legacy of [false, true]) {
    const f = measuredRuntime(t); f.hardware['hw.sdCard.path'] = resolve(f.profile.content, 'sdcard.img'); f.saveHardware();
    if (legacy) f.tokens = [f.qemu, '-avd', f.profile.physicalAvdName, '-android-hw', resolve(f.profile.content, 'hardware-qemu.ini')];
    assert.equal(await f.check(), f.platform); f.lease.remove(); assert(!fs.existsSync(f.profile.root));
  }
});
test('frontend rejects missing duplicate conflicting or extra launch flags', async t => {
  for (const flag of ['-avd', '-datadir', '-data', '-cache']) {
    for (const mode of ['missing', 'duplicate', 'conflict']) await rejectMeasured(t, f => {
      const at = f.tokens.indexOf(flag);
      if (mode === 'missing') f.tokens.splice(at, 2);
      else if (mode === 'duplicate') f.tokens.push(flag, f.tokens[at + 1]);
      else f.tokens[at + 1] = resolve(f.profile.root, 'different');
    });
  }
  for (const extra of [['-snapshot', 'foreign'], ['-snapstorage', '/foreign/snapshot.img'], ['-sdcard', '/foreign/sd.img'], ['-wipe-data'], ['-unknown']]) {
    await rejectMeasured(t, f => { f.tokens.push(...extra); });
  }
  for (const flag of ['-datadir', '-data', '-cache']) {
    await rejectMeasured(t, f => {
      const foreign = resolve(f.profile.root, 'different');
      f.tokens[f.tokens.indexOf(flag) + 1] = foreign;
      f.profile.launchArgs[f.profile.launchArgs.indexOf(flag) + 1] = foreign;
    });
    for (const mode of ['foreign', 'duplicate', 'conflicting']) {
      const f = measuredRuntime(t), hardware = resolve(f.profile.content, 'hardware-qemu.ini');
      f.hardware['hw.sdCard.path'] = resolve(f.profile.content, 'sdcard.img'); f.saveHardware();
      f.tokens = [f.qemu, '-avd', f.profile.physicalAvdName, '-android-hw', hardware];
      assert.equal(await f.check(), f.platform);
      const exact = flag === '-datadir' ? f.profile.content : resolve(f.profile.content, flag === '-data' ? 'userdata-qemu.img' : 'cache.img');
      f.tokens.push(flag, mode === 'foreign' ? '/foreign/image' : exact);
      if (mode !== 'foreign') f.tokens.push(flag, mode === 'duplicate' ? exact : '/foreign/image');
      await assert.rejects(f.check()); f.lease.remove(); assert(!fs.existsSync(f.profile.root));
    }
  }
});
test('invalid or duplicate present android-hw cannot fall back to the frontend route', async t => {
  for (const mode of ['foreign', 'duplicate', 'conflicting']) await rejectMeasured(t, f => {
    const own = resolve(f.profile.content, 'hardware-qemu.ini');
    f.tokens.push('-android-hw', mode === 'foreign' ? '/foreign/hardware.ini' : own);
    if (mode !== 'foreign') f.tokens.push('-android-hw', mode === 'duplicate' ? own : '/foreign/hardware.ini');
  });
});
test('measured runtime binds exact PID binary physical identity and hardware homes', async t => {
  for (const mode of ['pid', 'duplicate-process', 'binary']) await rejectMeasured(t, f => {
    if (mode === 'pid') f.linePid += 1;
    else if (mode === 'duplicate-process') f.duplicate = true;
    else f.tokens[0] = '/foreign/qemu-system-aarch64';
  });
  for (const key of ['avd.id', 'avd.name', 'android.avd.home', 'android.sdk.root']) for (const missing of [false, true]) {
    await rejectMeasured(t, f => { if (missing) delete f.hardware[key]; else f.hardware[key] = '/foreign'; f.saveHardware(); });
  }
});
test('canonical guest images and SD refuse alternate paths symlinks and nonregular files', async t => {
  for (const name of ['userdata-qemu.img', 'cache.img', 'encryptionkey.img', 'sdcard.img']) {
    for (const mode of ['symlink', 'directory', 'missing']) await rejectMeasured(t, f => {
      const target = resolve(f.profile.content, name); fs.unlinkSync(target);
      if (mode === 'directory') fs.mkdirSync(target);
      if (mode === 'symlink') { const foreign = resolve(f.directory, 'foreign.img'); f.write(foreign, 'untouched'); fs.symlinkSync(foreign, target); }
    });
  }
  for (const key of ['disk.dataPartition.path', 'disk.cachePartition.path', 'disk.encryptionKeyPartition.path', 'hw.sdCard.path']) {
    for (const path of ['/foreign/image.img', 'different.img']) await rejectMeasured(t, f => {
      f.hardware[key] = path.startsWith('/') ? path : resolve(f.profile.content, path); f.saveHardware();
    });
  }
});
test('automatic SD omission rejects other ABI API package malformed metadata or SD size', async t => {
  for (const [key, value] of [['abi.type', 'x86_64'], ['hw.cpu.arch', 'x86_64'], ['target', 'android-35'],
    ['image.sysdir.1', 'system-images/android-36/foreign/arm64-v8a/']]) for (const missing of [false, true]) {
    await rejectMeasured(t, f => { if (missing) delete f.config[key]; else f.config[key] = value; f.saveConfig(); });
  }
  for (const key of ['hw.cpu.arch', 'hw.sdCard']) for (const missing of [false, true]) {
    await rejectMeasured(t, f => { if (missing) delete f.hardware[key]; else f.hardware[key] = 'foreign'; f.saveHardware(); });
  }
  await rejectMeasured(t, f => fs.truncateSync(resolve(f.profile.content, 'sdcard.img'), 1));
});
test('unowned scenario resolution and verification remain no-probe default routes', async () => {
  let probes = 0;
  assert.equal(handoffApi.resolveAndroidScenarioAvdName('Pixel_4a', { env: {} }), 'Pixel_4a');
  assert.equal(await handoffApi.verifyOwnedAndroidScenarioRuntime('Pixel_4a', null, { env: {},
    runCommand: async () => { probes += 1; assert.fail('Unowned route must not probe'); } }), undefined);
  assert.equal(probes, 0);
});
