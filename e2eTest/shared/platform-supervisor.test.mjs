import test from 'node:test';
import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { mkdtempSync, rmSync, mkdirSync, openSync, closeSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash, randomUUID } from 'node:crypto';
import { runInNewContext } from 'node:vm';
import {
  androidEmulatorLaunchArgs,
  shouldColdBootAndroidAvdOnLaunch,
  bindAndroidReadinessFailureToSerial,
  shouldColdBootOwnedOfflineAndroidAvd,
  shouldRestartAndroidAvdForReadinessFailure,
  verifyAndroidInstallReadiness,
  capturePlatformProbeDiagnostics,
  checkPlatform,
  prepareIos,
  prepareAndroid,
} from './platform-supervisor.mjs';

const androidPlatform = { platform: 'Android', avdName: 'Pixel_4a', serial: 'emulator-test' };
const androidLivenessDependencies = {
  androidSdk: () => ({ adbPath: '/sdk/adb' }),
  ownedAndroidProcesses: new Map(),
  findAndroidAvd: async (_sdk, avdName) => {
    assert.equal(avdName, androidPlatform.avdName);
    return androidPlatform.serial;
  },
};

const iosPlatform = { platform: 'iOS', simulatorName: 'iPhone 17', udid: 'DB22263E-A8A3-49C5-ADD1-BAE4089E2E30' };

test('iOS F4 executes a bounded guest heartbeat on the exact UDID without global inventory', async () => {
  const calls = [];
  await checkPlatform(iosPlatform, {
    runCommand: async (file, args, options) => {
      calls.push({ file, args, options });
      assert.equal(args.includes('list'), false);
      return { stdout: '96981\n' };
    },
  });
  assert.deepEqual(calls, [{
    file: 'xcrun',
    args: ['simctl', 'spawn', 'DB22263E-A8A3-49C5-ADD1-BAE4089E2E30', 'launchctl', 'managerpid'],
    options: { timeout: 15_000 },
  }]);
});

test('iOS F4 rejects wrong heartbeat, guest command failure and timeout', async () => {
  for (const invalid of ['', '0', '1', '-2', '2.5', '999999999999999999999', '96981 extra', 'E2E_IOS_RUNTIME_HEARTBEAT']) {
    await assert.rejects(checkPlatform(iosPlatform, {
      runCommand: async () => ({ stdout: invalid }),
    }), /runtime heartbeat failed/);
  }
  for (const error of [
    Object.assign(new Error('guest unavailable'), { code: 149 }),
    Object.assign(new Error('guest heartbeat timeout'), { code: 'ETIMEDOUT', killed: true }),
  ]) {
    await assert.rejects(checkPlatform(iosPlatform, {
      runCommand: async () => { throw error; },
    }), (actual) => actual === error);
  }
});

test('iOS F4 refuses a heartbeat without an exact selected UDID', async () => {
  await assert.rejects(checkPlatform({ platform: 'iOS' }, {
    runCommand: async () => assert.fail('must not select any default simulator'),
  }), /requires the selected simulator UDID/);
});

test('iOS F3 verifies the same guest health command after full inventory and bootstatus before handoff', async () => {
  for (const heartbeatWorks of [true, false]) {
    const calls = [];
    let uiOpened = false;
    const preparation = prepareIos(iosPlatform.simulatorName, () => assert.fail('already Booted'), {
      runCommand: async (_file, args, options) => {
        calls.push({ args, options });
        if (args.includes('list')) return { stdout: JSON.stringify({ devices: { 'com.apple.CoreSimulator.SimRuntime.iOS-26-5': [{ name: iosPlatform.simulatorName, udid: iosPlatform.udid, state: 'Booted', isAvailable: true, deviceTypeIdentifier: 'com.apple.CoreSimulator.SimDeviceType.iPhone-17' }] } }) };
        if (args.includes('bootstatus')) return { stdout: '' };
        assert.deepEqual(args, ['simctl', 'spawn', iosPlatform.udid, 'launchctl', 'managerpid']);
        assert.equal(options.timeout, 15_000);
        if (!heartbeatWorks) throw new Error('guest tool unavailable');
        return { stdout: '96981\n' };
      },
      openDeviceUi: async ({ simulatorUdid }) => { assert.equal(simulatorUdid, iosPlatform.udid); uiOpened = true; return {}; },
    });
    if (heartbeatWorks) {
      const result = await preparation;
      assert.equal(result.udid, iosPlatform.udid);
      assert.equal(uiOpened, true);
    } else {
      await assert.rejects(preparation, /guest tool unavailable/);
      assert.equal(uiOpened, false);
    }
    assert.deepEqual(calls.map(({ args }) => args[1]), ['list', 'bootstatus', 'spawn']);
  }
});

test('F4 checks exact AVD, boot and shell heartbeat without Binder/package inventories', async () => {
  const calls = [];
  await checkPlatform(androidPlatform, {
    ...androidLivenessDependencies,
    runCommand: async (file, args, options) => {
      calls.push({ file, args, options });
      if (args.includes('getprop')) return { stdout: '1\n' };
      if (args.includes('echo')) return { stdout: 'E2E_PLATFORM_PROBE\n' };
      throw new Error('F4 must not invoke installation inventories');
    },
  });
  assert.deepEqual(calls.map(({ args }) => args), [
    ['-s', 'emulator-test', 'shell', 'getprop', 'sys.boot_completed'],
    ['-s', 'emulator-test', 'shell', 'echo', 'E2E_PLATFORM_PROBE'],
  ]);
  assert.ok(calls.every(({ file, options }) => file === '/sdk/adb' && options.timeout === 10_000));
});

test('F4 rejects offline or mismatched AVD transport before shell checks', async () => {
  for (const serial of [null, 'other-emulator']) {
    await assert.rejects(checkPlatform(androidPlatform, {
      ...androidLivenessDependencies,
      findAndroidAvd: async () => serial,
      runCommand: async () => assert.fail('offline/foreign AVD must not receive a command'),
    }), /no longer online/);
  }
});

test('F4 rejects boot loss, invalid shell heartbeat and heartbeat timeout', async () => {
  await assert.rejects(checkPlatform(androidPlatform, {
    ...androidLivenessDependencies,
    runCommand: async () => ({ stdout: '0\n' }),
  }), /lost boot readiness/);
  await assert.rejects(checkPlatform(androidPlatform, {
    ...androidLivenessDependencies,
    runCommand: async (_file, args) => ({ stdout: args.includes('getprop') ? '1\n' : 'invalid\n' }),
  }), /shell heartbeat failed/);
  const timeout = Object.assign(new Error('shell heartbeat timed out'), { code: 'ETIMEDOUT' });
  await assert.rejects(checkPlatform(androidPlatform, {
    ...androidLivenessDependencies,
    runCommand: async (_file, args) => {
      if (args.includes('getprop')) return { stdout: '1\n' };
      throw timeout;
    },
  }), (error) => error === timeout);
});

test('F4 rejects a runner-owned emulator exit before invoking ADB', async () => {
  await assert.rejects(checkPlatform(androidPlatform, {
    ...androidLivenessDependencies,
    ownedAndroidProcesses: new Map([['Pixel_4a', {
      launch: { exit: { code: 1 }, logPath: '/tmp/metasecret-nonexistent-supervisor-test-log' },
    }]]),
    runCommand: async () => assert.fail('exited emulator must not receive a command'),
  }), /Emulator process exited/);
});

test('failure diagnostics bound commands and never log raw package inventory or error output', async () => {
  const events = [];
  await capturePlatformProbeDiagnostics({ platform: 'Android', serial: 'emulator-test' }, (line) => events.push(line), async (_file, args, options) => {
    assert.equal(options.timeout, 3_000);
    if (args.includes('echo')) return { stdout: 'E2E_PLATFORM_PROBE\n' };
    if (args.includes('service')) return { stdout: 'Found 5 services:\nprivate service contents' };
    if (args.includes('package')) return { stdout: 'package:private.package\n' };
    throw Object.assign(new Error('private stderr'), { code: 'ETIMEDOUT', killed: true });
  });
  assert.ok(events.some((line) => line.includes('shell-transport') && line.includes('responsive')));
  assert.ok(events.some((line) => line.includes('1 package rows')));
  assert.ok(events.some((line) => line.includes('code=ETIMEDOUT')));
  assert.equal(events.join('\n').includes('private'), false);
});

test('iOS failure diagnostics execute the verified guest command on the selected UDID', async () => {
  const events = [];
  let guestChecked = false;
  await capturePlatformProbeDiagnostics(iosPlatform, (line) => events.push(line), async (_file, args, options) => {
    assert.equal(options.timeout, 3_000);
    if (args.includes('spawn')) {
      assert.deepEqual(args, ['simctl', 'spawn', iosPlatform.udid, 'launchctl', 'managerpid']);
      guestChecked = true;
      return { stdout: '96981\n' };
    }
    if (args.includes('list')) return { stdout: JSON.stringify({ devices: { runtime: [{ udid: iosPlatform.udid, state: 'Booted' }] } }) };
    return { stdout: '' };
  });
  assert.equal(guestChecked, true);
  assert.ok(events.some((line) => line.includes('guest launchd manager PID=96981')));
});

test('Android install readiness checks shell service inventory and package manager', async () => {
  const calls = [];
  const result = await verifyAndroidInstallReadiness({
    adbPath: '/sdk/adb',
    serial: 'emulator-test',
    runCommand: async (_file, args, options) => {
      calls.push({ args, options });
      if (args.includes('service')) return { stdout: 'Found 174 services:\n 0 activity: [android.app.IActivityManager]\n' };
      return { stdout: 'package:android\npackage:com.android.settings\n' };
    },
  });

  assert.deepEqual(result, {
    adbTransport: 'online',
    shellServices: 'responsive',
    packageManager: 'responsive',
  });
  assert.deepEqual(calls.map(({ args }) => args.slice(3)), [
    ['service', 'list'],
    ['cmd', 'package', 'list', 'packages', '--user', '0'],
  ]);
  assert.ok(calls.every(({ options }) => options.timeout === 10_000));
});

test('Android service-list timeout is distinguished from an offline ADB transport', async () => {
  await assert.rejects(
    verifyAndroidInstallReadiness({
      adbPath: '/sdk/adb',
      serial: 'emulator-test',
      runCommand: async () => {
        throw Object.assign(new Error('probe timeout'), { code: 'ETIMEDOUT', killed: true });
      },
    }),
    (error) => error.code === 'ANDROID_AVD_SHELL_SERVICES_UNRESPONSIVE'
      && error.readinessProbe === 'shell-services'
      && error.message.includes('ADB transport is online')
      && error.message.includes('timed out'),
  );
});

test('Android package-manager timeout is reported as a separate readiness failure', async () => {
  let call = 0;
  await assert.rejects(
    verifyAndroidInstallReadiness({
      adbPath: '/sdk/adb',
      serial: 'emulator-test',
      runCommand: async () => {
        call += 1;
        if (call === 1) return { stdout: 'Found 174 services:\n' };
        throw Object.assign(new Error('probe timeout'), { code: 'ETIMEDOUT', killed: true });
      },
    }),
    (error) => error.code === 'ANDROID_AVD_PACKAGE_MANAGER_UNRESPONSIVE'
      && error.readinessProbe === 'package-manager'
      && error.message.includes('ADB transport is online'),
  );
});

test('only an install-readiness failure permits one exact-AVD recovery attempt', () => {
  assert.equal(shouldRestartAndroidAvdForReadinessFailure({ code: 'ANDROID_AVD_SHELL_SERVICES_UNRESPONSIVE' }, 0), true);
  assert.equal(shouldRestartAndroidAvdForReadinessFailure({ code: 'ANDROID_AVD_PACKAGE_MANAGER_UNRESPONSIVE' }, 0), true);
  assert.equal(shouldRestartAndroidAvdForReadinessFailure({ code: 'ANDROID_AVD_IDENTITY_UNRESPONSIVE' }, 0), false);
  assert.equal(shouldRestartAndroidAvdForReadinessFailure({ code: 'ANDROID_AVD_SHELL_SERVICES_UNRESPONSIVE' }, 1), false);
});

test('Android readiness recovery skips the saved Quick Boot snapshot', () => {
  assert.deepEqual(androidEmulatorLaunchArgs('Pixel_4a'), ['-avd', 'Pixel_4a']);
  assert.deepEqual(androidEmulatorLaunchArgs('Pixel_4a', { coldBoot: true }), [
    '-avd', 'Pixel_4a', '-no-snapshot-load',
  ]);
});

test('runner-owned Android launch cold-boots without a snapshot when no matching AVD is online', () => {
  assert.equal(shouldColdBootAndroidAvdOnLaunch(null), true);
  assert.equal(shouldColdBootAndroidAvdOnLaunch('emulator-5554'), false);
  assert.deepEqual(
    androidEmulatorLaunchArgs('Pixel_4a', {
      coldBoot: shouldColdBootAndroidAvdOnLaunch(null),
    }),
    ['-avd', 'Pixel_4a', '-no-snapshot-load'],
  );
});

test('Android readiness failure retains the exact ADB serial for safe recovery', () => {
  const error = { code: 'ANDROID_AVD_SHELL_SERVICES_UNRESPONSIVE' };
  assert.equal(bindAndroidReadinessFailureToSerial(error, 'emulator-test').serial, 'emulator-test');
  assert.equal(bindAndroidReadinessFailureToSerial(error, 'another-serial').serial, 'emulator-test');
});

test('offline Android recovery is limited to one exact runner-owned QEMU and serial', () => {
  const error = { code: 'ANDROID_AVD_OFFLINE_TIMEOUT' };
  const eligible = {
    error,
    recoveryAttempt: 0,
    startedBySupervisor: true,
    offlineSerials: ['emulator-5554'],
    qemuPids: [79202],
    launchPid: 79202,
  };
  assert.equal(shouldColdBootOwnedOfflineAndroidAvd(eligible), true);
  assert.equal(shouldColdBootOwnedOfflineAndroidAvd({ ...eligible, startedBySupervisor: false }), false);
  assert.equal(shouldColdBootOwnedOfflineAndroidAvd({ ...eligible, offlineSerials: ['emulator-5554', 'emulator-5556'] }), false);
  assert.equal(shouldColdBootOwnedOfflineAndroidAvd({ ...eligible, qemuPids: [79202, 79203] }), false);
  assert.equal(shouldColdBootOwnedOfflineAndroidAvd({ ...eligible, launchPid: 99999 }), false);
  assert.equal(shouldColdBootOwnedOfflineAndroidAvd({ ...eligible, recoveryAttempt: 1 }), false);
});

const lifecycleTurn = () => new Promise(resolve => setImmediate(resolve));
let lifecycleFixtureNumber = 0;
function androidLaunchFixture(t, mode = 'term', borrowed = false, avdName = 'Pixel_4a') {
  const directory = mkdtempSync(join(tmpdir(), 'metasecret-android-launch-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const sdk = { sdkRoot: '/controlled-sdk', adbPath: '/controlled-sdk/adb', emulatorPath: '/controlled-sdk/emulator', pathPrefix: '/controlled-sdk' };
  const events = [], commands = [], leases = [], child = new EventEmitter(), failure = new Error('controlled readiness rejection before serial');
  Object.assign(child, { pid: Number.MAX_SAFE_INTEGER, exitCode: null, signalCode: null, closed: false,
    unref() {}, kill(signal) { events.push(signal); if (mode === 'term' || (mode === 'kill' && signal === 'SIGKILL')) queueMicrotask(() => close(null, signal)); return true; } });
  function close(code, signal) { child.closed = true; child.exitCode = code; child.signalCode = signal; events.push('close'); child.emit('exit', code, signal); child.emit('close', code, signal); }
  const ports = { existingAvd: () => ({ avdName, dataPath: '/controlled/Pixel_4a.avd' }), androidSdk: () => sdk, runCommand: async (file, args) => {
    commands.push({ file, args });
    if (file === 'ps') return { stdout: '' };
    assert([sdk.adbPath, sdk.emulatorPath].includes(file), 'Controlled fixture must not dispatch a host tool');
    if (args[0] === 'version') return { stdout: 'Android Debug Bridge\nVersion controlled\n' };
    if (args[0] === '-list-avds') return { stdout: avdName + '\n' };
    if (args[0] === 'devices') return { stdout: 'List of devices attached\n' + (borrowed ? 'emulator-fixture\tdevice\n' : '') };
    if (args.includes('emu')) return { stdout: avdName + '\nOK\n' };
    assert.deepEqual(args, ['start-server']); return { stdout: '' };
  }, spawnProcess: (file, args, options) => {
    assert(!borrowed, 'A borrowed runtime must not launch another process'); assert.equal(file, sdk.emulatorPath);
    assert.deepEqual(args, ['-avd', avdName, '-no-snapshot-load']); assert.equal(options.detached, true); events.push('spawn');
    if (mode === 'closed') queueMicrotask(() => { child.emit('error', failure); close(1, null); }); return child;
  }, waitForAvd: async () => { events.push('readiness-reject'); throw failure; } };
  return { directory, avdName, child, failure, events, commands, leases, ports, close,
    start: () => prepareAndroid(avdName, lease => leases.push(lease), { runDirectory: directory, testNumber: 50, attempt: 1 }, ports) };
}

for (const mode of ['term', 'kill', 'never']) test(`owned Android early readiness failure drains exact launch (${mode})`, { timeout: 1000 }, async t => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const f = androidLaunchFixture(t, mode); let actual;
  f.failure.code = mode === 'term' ? 'ANDROID_AVD_SHELL_SERVICES_UNRESPONSIVE' : mode === 'kill' ? 'ANDROID_AVD_PACKAGE_MANAGER_UNRESPONSIVE' : undefined;
  const outcome = f.start().then(() => assert.fail('Readiness must reject'), error => { actual = error; f.events.push('rejected'); });
  await lifecycleTurn(); t.mock.timers.tick(5000); await lifecycleTurn(); t.mock.timers.tick(2000); await lifecycleTurn(); await outcome;
  assert.equal(f.leases.length, 0, 'Failure is before any serial callback'); assert(!f.commands.some(call => call.file === 'ps'), 'Drain uses the retained child without replacement discovery');
  assert.deepEqual(f.events.filter(event => event.startsWith('SIG')), mode === 'term' ? ['SIGTERM'] : ['SIGTERM', 'SIGKILL']);
  if (mode === 'never') { assert(actual instanceof AggregateError); assert.equal(actual.cause, f.failure); assert(actual.errors.includes(f.failure)); assert.equal(f.child.closed, false); }
  else { assert.equal(actual, f.failure); assert.equal(f.child.closed, true); assert(f.events.indexOf('close') < f.events.indexOf('rejected'), 'Rejection must await actual close'); }
});

test('borrowed Android readiness rejection never signals or replaces that runtime', async t => {
  for (const code of [undefined, 'ANDROID_AVD_SHELL_SERVICES_UNRESPONSIVE', 'ANDROID_AVD_PACKAGE_MANAGER_UNRESPONSIVE']) {
    const f = androidLaunchFixture(t, 'term', true); f.failure.code = code;
    await assert.rejects(f.start(), error => error === f.failure);
    assert.deepEqual(f.events, ['readiness-reject']); assert.equal(f.leases.length, 0); assert(!f.commands.some(call => call.file === 'ps'));
  }
  const f = androidLaunchFixture(t, 'term', true), run = f.ports.runCommand;
  f.ports.runCommand = (file, args) => args.includes('emu') ? Promise.resolve({ stdout: '' }) : run(file, args);
  await assert.rejects(f.start(), error => error.code === 'ANDROID_AVD_IDENTITY_UNRESPONSIVE');
  assert.deepEqual(f.events, []); assert(!f.commands.some(call => call.file === 'ps'));
});

test('already closed owned launch preserves readiness cause without a redundant signal', async t => {
  const f = androidLaunchFixture(t, 'closed'); await assert.rejects(f.start(), error => error === f.failure); await lifecycleTurn();
  assert.equal(f.child.closed, true); assert.equal(f.leases.length, 0); assert.deepEqual(f.events.filter(event => event.startsWith('SIG')), []);
});

test('successful owned readiness returns exact lease and never performs early cleanup', async t => {
  const f = androidLaunchFixture(t); f.ports.waitForAvd = async (_sdk, name, onSerial, timeout, launch) => {
    assert.equal(name, f.avdName); assert.equal(timeout, 180000); assert.equal(launch.emulator, f.child); onSerial('emulator-fixture'); return 'emulator-fixture';
  };
  const result = await f.start(); assert.equal(result.serial, 'emulator-fixture'); assert.equal(result.startedBySupervisor, true);
  assert.deepEqual(f.leases, [{ platform: 'Android', avdName: f.avdName, serial: 'emulator-fixture' }]);
  assert.equal(f.child.closed, false); assert.deepEqual(f.events, ['spawn']); f.close(0, null);
});

test('failed owned launch cleanup cannot delete a newer same-AVD owner', async t => {
  const first = androidLaunchFixture(t), second = androidLaunchFixture(t, 'term', false, first.avdName); let release;
  first.ports.waitForAvd = () => new Promise((_resolve, reject) => { release = () => reject(first.failure); });
  const firstOutcome = first.start().then(() => assert.fail('First readiness must fail'), error => { assert.equal(error, first.failure); });
  await lifecycleTurn(); assert.equal(typeof release, 'function');
  second.ports.waitForAvd = async (_sdk, _name, onSerial) => { onSerial('emulator-new-owner'); return 'emulator-new-owner'; };
  await second.start(); release(); await firstOutcome;
  assert.equal(first.child.closed, true); assert.equal(second.child.closed, false); second.close(37, null);
  await assert.rejects(checkPlatform({ platform: 'Android', avdName: first.avdName, serial: 'emulator-new-owner' }, {
    androidSdk: second.ports.androidSdk, findAndroidAvd: async () => assert.fail('New owner exit must be checked before ADB'),
    runCommand: async () => assert.fail('New owner lease must remain in the actual private map'),
  }), /Emulator process exited/);
});

// Proposal: execute original module bodies with only external ports closed.
function lifecycleSupervisor(t, f) {
  const path = new URL('./platform-supervisor.mjs', import.meta.url);
  const original = readFileSync(path, 'utf8');
  const body = original.replace(/^import .*;\r?$/gm, '').replace(/^export \{.*;\r?$/gm, '')
    .replace(/^export /gm, '').replace(/import\.meta\.url/g, JSON.stringify(path.href));
  return runInNewContext(body + '\n({ prepareAndroid, prepareIos, stopRunnerOwnedRuntimes, releaseHealthyRunnerOwnedRuntimes });', {
    resolve, fileURLToPath, mkdirSync, openSync, closeSync, readFileSync,
    promisify: fn => fn, execFile: (file, args, options) => f.ports.runCommand(file, Array.from(args), options),
    discoverAndroidSdk: f.ports.androidSdk, findOnlineAndroidAvd: async () => null,
    withAndroidSdkPath: value => value, homedir: () => '/controlled-home',
    process: { env: {}, stdout: { write() {} }, stderr: { write() {} } },
    setTimeout, clearTimeout, Buffer, URL, Date, AggregateError,
    createHash, randomUUID, spawn: () => assert.fail('Uninjected process launch'),
  });
}

for (const mode of ['term', 'kill', 'closed']) test(
  'published owned Android failure joins close and outer cleanup acknowledges once (' + mode + ')',
  { timeout: 1000 }, async t => {
    t.mock.timers.enable({ apis: ['setTimeout'] });
    const f = androidLaunchFixture(t, mode), api = lifecycleSupervisor(t, f), published = [];
    f.ports.waitForAvd = async (_sdk, _name, onSerial) => {
      onSerial('emulator-fixture'); throw f.failure;
    };
    let actual;
    const ports = { ...f.ports, runCommand: (file, args, options) => f.ports.runCommand(file, Array.from(args), options),
      spawnProcess: (file, args, options) => f.ports.spawnProcess(file, Array.from(args), options) };
    const first = api.prepareAndroid(f.avdName, runtime => published.push(runtime),
      { runDirectory: f.directory, testNumber: 1, attempt: 1 }, ports)
      .then(() => assert.fail('Readiness must reject'), error => { actual = error; });
    await lifecycleTurn(); t.mock.timers.tick(5000); await lifecycleTurn();
    t.mock.timers.tick(2000); await lifecycleTurn(); await first;
    assert.equal(actual, f.failure, 'Original readiness Error remains primary');
    assert.equal(published.length, 1); assert.equal(f.child.closed, true);
    const closedAt = f.events.indexOf('close'); assert(closedAt >= 0);
    await api.stopRunnerOwnedRuntimes(published);
    assert.equal(f.commands.filter(call => call.args.includes('kill')).length, 0,
      'Already joined exact runtime must not issue another adb emu kill');
    await api.stopRunnerOwnedRuntimes(published);
    assert.equal(f.commands.filter(call => call.args.includes('kill')).length, 0,
      'Repeated acknowledgement must also be inert');
    assert.equal(f.events.filter(event => event === 'spawn').length, 1);
  });


async function closedPublishedRuntime(t) {
  const f = androidLaunchFixture(t), api = lifecycleSupervisor(t, f), published = [];
  f.ports.waitForAvd = async (_sdk, _name, onSerial) => { onSerial('emulator-fixture'); throw f.failure; };
  const ports = { ...f.ports, runCommand: (file, args, options) => f.ports.runCommand(file, Array.from(args), options),
    spawnProcess: (file, args, options) => f.ports.spawnProcess(file, Array.from(args), options) };
  await assert.rejects(api.prepareAndroid(f.avdName, runtime => published.push(runtime),
    { runDirectory: f.directory, testNumber: 1, attempt: 1 }, ports), error => error === f.failure);
  assert.equal(published.length, 1); assert.equal(f.child.closed, true);
  return { f, api, runtime: published[0] };
}

for (const kind of ['clone', 'json', 'foreign', 'serial', 'avd', 'platform', 'extra', 'missing'])
  test('closed runtime acknowledgement refuses ' + kind + ' before any SDK call', async t => {
    const { f, api, runtime } = await closedPublishedRuntime(t);
    let candidate = runtime;
    if (kind === 'clone') candidate = { ...runtime };
    if (kind === 'json') candidate = JSON.parse(JSON.stringify(runtime));
    if (kind === 'foreign') candidate = { platform: 'Android', avdName: f.avdName, serial: 'emulator-foreign' };
    if (kind === 'serial') candidate.serial = 'emulator-changed';
    if (kind === 'avd') candidate.avdName = 'foreign-avd';
    if (kind === 'platform') candidate.platform = 'iOS';
    if (kind === 'extra') candidate.extraAuthority = true;
    if (kind === 'missing') delete candidate.serial;
    const before = f.commands.length, events = [];
    await assert.rejects(api.stopRunnerOwnedRuntimes([candidate], value => events.push(value)));
    assert.equal(f.commands.length, before, 'Refusal must precede every SDK command');
    assert.deepEqual(events, [], 'No inert acknowledgement for foreign or changed identity');
  });

test('old closed runtime cannot acknowledge or signal a newer same-AVD launch', async t => {
  const { f, api, runtime } = await closedPublishedRuntime(t);
  const newer = androidLaunchFixture(t, 'term', false, f.avdName), published = [];
  newer.ports.waitForAvd = async (_sdk, _name, onSerial) => { onSerial('emulator-new-owner'); return 'emulator-new-owner'; };
  const ports = { ...newer.ports, runCommand: (file, args, options) => newer.ports.runCommand(file, Array.from(args), options),
    spawnProcess: (file, args, options) => newer.ports.spawnProcess(file, Array.from(args), options) };
  await api.prepareAndroid(newer.avdName, value => published.push(value),
    { runDirectory: newer.directory, testNumber: 2, attempt: 1 }, ports);
  const before = f.commands.length, events = [];
  try {
    await assert.rejects(api.stopRunnerOwnedRuntimes([runtime], value => events.push(value)));
    assert.equal(f.commands.length, before); assert.deepEqual(events, []);
    assert.equal(newer.child.closed, false); assert.equal(published.length, 1);
  } finally { newer.close(0, null); }
});

test('original current active runtime retains exactly the existing normal ADB cleanup', async t => {
  const f = androidLaunchFixture(t), api = lifecycleSupervisor(t, f), published = [];
  f.ports.waitForAvd = async (_sdk, _name, onSerial) => { onSerial('emulator-fixture'); return 'emulator-fixture'; };
  const ports = { ...f.ports, runCommand: (file, args, options) => f.ports.runCommand(file, Array.from(args), options),
    spawnProcess: (file, args, options) => f.ports.spawnProcess(file, Array.from(args), options) };
  await api.prepareAndroid(f.avdName, value => published.push(value),
    { runDirectory: f.directory, testNumber: 1, attempt: 1 }, ports);
  const before = f.commands.length;
  try {
    await api.stopRunnerOwnedRuntimes(published);
    assert.equal(f.commands.length, before + 1);
    assert.deepEqual(f.commands.at(-1).args, ['-s', 'emulator-fixture', 'emu', 'kill']);
    assert.deepEqual(f.events.filter(value => value.startsWith('SIG')), [], 'Do not add host process signals');
  } finally { f.close(0, null); }
});


for (const kind of ['changing-platform-getter', 'serial-getter', 'symbol', 'hidden-data', 'setter-only'])
  test('runtime data-descriptor admission refuses ' + kind + ' without consulting accessors or SDK', async t => {
    const { f, api, runtime } = await closedPublishedRuntime(t); let getters = 0;
    if (kind === 'changing-platform-getter') Object.defineProperty(runtime, 'platform', {
      enumerable: true, configurable: true, get() { return ++getters === 1 ? 'Android' : 'iOS'; },
    });
    if (kind === 'serial-getter') Object.defineProperty(runtime, 'serial', {
      enumerable: true, configurable: true, get() { getters++; return 'emulator-fixture'; },
    });
    if (kind === 'symbol') runtime[Symbol('unowned-authority')] = true;
    if (kind === 'hidden-data') Object.defineProperty(runtime, 'hiddenAuthority', { value: true, enumerable: false });
    if (kind === 'setter-only') Object.defineProperty(runtime, 'serial', { enumerable: true, configurable: true, set(_value) {} });
    const commands = f.commands.length, events = []; let failure;
    try { await api.stopRunnerOwnedRuntimes([runtime], event => events.push(event)); } catch (error) { failure = error; }
    assert.equal(f.commands.length, commands, 'Descriptor refusal must precede all SDK dispatch');
    assert(failure, 'Only original own data descriptors may authenticate runtime identity');
    assert.equal(getters, 0, 'Admission cannot consult attacker-controlled getters');
    assert.deepEqual(events, [], 'No successful acknowledgement for changed descriptors or hidden keys');
  });

async function releasableAndroid(t) {
  const f = androidLaunchFixture(t), api = lifecycleSupervisor(t, f), published = [];
  f.ports.waitForAvd = async (_sdk, _name, onSerial) => { onSerial('emulator-fixture'); return 'emulator-fixture'; };
  await api.prepareAndroid(f.avdName, runtime => published.push(runtime),
    { runDirectory: f.directory, testNumber: 1, attempt: 1 }, {
      ...f.ports, runCommand: (file, args, options) => f.ports.runCommand(file, Array.from(args), options),
      spawnProcess: (file, args, options) => f.ports.spawnProcess(file, Array.from(args), options),
    });
  t.after(() => { if (!f.child.closed) f.close(0, null); });
  const healthCalls = [], ports = {
    androidSdk: f.ports.androidSdk, existingAvd: f.ports.existingAvd,
    findAndroidAvd: async () => 'emulator-fixture', now: () => '2026-10-28T00:00:00.000Z',
    runCommand: async (file, args, options) => {
      args = Array.from(args); healthCalls.push({ file, args, options });
      assert.equal(file, '/controlled-sdk/adb'); assert.equal(args[1], 'emulator-fixture');
      if (args.includes('get-state')) return { stdout: 'device\n' };
      if (args.includes('getprop')) return { stdout: '1\n' };
      if (args.includes('get-started-user-state')) return { stdout: 'RUNNING_UNLOCKED\n' };
      if (args.includes('service')) return { stdout: 'Found 174 services:\n' };
      if (args.includes('package')) return { stdout: 'package:android\n' };
      assert.fail('Unexpected release command');
    },
  };
  return { ...f, api, runtime: published[0], healthCalls, releasePorts: ports };
}

test('healthy exact Android release checks readiness once, retains guest and returns inert non-authority receipts', async t => {
  const f = await releasableAndroid(t), events = [], before = f.commands.length;
  const receipts = await f.api.releaseHealthyRunnerOwnedRuntimes([f.runtime], { reason: 'pending-test', onEvent: value => events.push(value) }, f.releasePorts);
  assert(Object.isFrozen(receipts)); assert(Object.isFrozen(receipts[0]));
  assert.equal(receipts[0].operation, 'released-healthy'); assert.equal(receipts[0].authorityTransferred, false);
  assert.equal(receipts[0].shutdown, false); assert.equal(receipts[0].reason, 'pending-test');
  assert.equal(f.healthCalls.length, 5); assert(f.healthCalls.every(call => call.options.timeout === 10000));
  assert.equal(f.child.closed, false); assert.equal(f.commands.length, before); assert.equal(events.length, 1);
  const again = await f.api.releaseHealthyRunnerOwnedRuntimes([f.runtime], { reason: 'pending-test', onEvent() { throw Error('observer'); } }, f.releasePorts);
  assert.equal(again[0], receipts[0]); assert.equal(f.healthCalls.length, 5);
  await f.api.stopRunnerOwnedRuntimes([f.runtime]); assert.equal(f.commands.length, before, 'Release revokes later shutdown authority');
  const otherProcess = lifecycleSupervisor(t, f);
  await assert.rejects(otherProcess.releaseHealthyRunnerOwnedRuntimes([f.runtime], { reason: 'pending-test' }, f.releasePorts), /exact current-process/);
  await assert.rejects(f.api.releaseHealthyRunnerOwnedRuntimes(receipts, { reason: 'pending-test' }, f.releasePorts), /exact current-process/);
  assert.equal(f.healthCalls.length, 5, 'A receipt or foreign process cannot adopt runtime authority');
});

for (const kind of ['clone', 'json', 'foreign', 'changed', 'getter', 'symbol', 'duplicate', 'reason'])
  test('healthy release rejects ' + kind + ' before health commands or authority revocation', async t => {
    const f = await releasableAndroid(t); let runtime = f.runtime, getters = 0;
    if (kind === 'clone') runtime = { ...runtime };
    if (kind === 'json') runtime = JSON.parse(JSON.stringify(runtime));
    if (kind === 'foreign') runtime = { ...runtime, serial: 'foreign' };
    if (kind === 'changed') runtime.serial = 'foreign';
    if (kind === 'getter') Object.defineProperty(runtime, 'serial', { enumerable: true, configurable: true, get() { getters++; return 'emulator-fixture'; } });
    if (kind === 'symbol') runtime[Symbol('authority')] = true;
    await assert.rejects(f.api.releaseHealthyRunnerOwnedRuntimes(kind === 'duplicate' ? [runtime, runtime] : [runtime],
      { reason: kind === 'reason' ? 'completed' : 'pending-test' }, f.releasePorts));
    assert.equal(getters, 0); assert.equal(f.healthCalls.length, 0); assert.equal(f.child.closed, false);
  });

test('unhealthy Android release preserves owned cleanup and rejects offline, changed registration and command failure', async t => {
  for (const kind of ['offline', 'boot', 'locked', 'binder', 'package', 'registration', 'command']) {
    const f = await releasableAndroid(t), primary = Error('controlled health failure'), original = f.releasePorts.runCommand;
    if (kind === 'offline') f.releasePorts.findAndroidAvd = async () => null;
    if (kind === 'registration') f.releasePorts.existingAvd = () => ({ avdName: f.avdName, dataPath: '/changed' });
    f.releasePorts.runCommand = async (file, args, options) => {
      if (kind === 'command') throw primary;
      if (kind === 'boot' && args.includes('getprop') || kind === 'locked' && args.includes('get-started-user-state')
        || kind === 'binder' && args.includes('service') || kind === 'package' && args.includes('package')) return { stdout: '' };
      return original(file, args, options);
    };
    await assert.rejects(f.api.releaseHealthyRunnerOwnedRuntimes([f.runtime], { reason: 'pending-test' }, f.releasePorts),
      kind === 'command' ? error => error === primary : undefined);
    await f.api.stopRunnerOwnedRuntimes([f.runtime]);
    assert.equal(f.commands.filter(call => call.args.includes('kill')).length, 1, 'Failed release must retain normal owned cleanup');
  }
});

test('release revalidates exact identity after awaited health before revoking any authority', async t => {
  const f = await releasableAndroid(t), original = f.releasePorts.runCommand;
  f.releasePorts.runCommand = async (file, args, options) => {
    const result = await original(file, args, options);
    if (args.includes('package')) f.runtime.serial = 'changed-during-health';
    return result;
  };
  await assert.rejects(f.api.releaseHealthyRunnerOwnedRuntimes([f.runtime], { reason: 'pending-test' }, f.releasePorts), /changed/);
  f.runtime.serial = 'emulator-fixture'; await f.api.stopRunnerOwnedRuntimes([f.runtime]);
  assert.equal(f.commands.filter(call => call.args.includes('kill')).length, 1);
});

test('closed launch and newer same-AVD owner cannot be released by an older runtime reference', async t => {
  const closed = await releasableAndroid(t); closed.close(0, null);
  await assert.rejects(closed.api.releaseHealthyRunnerOwnedRuntimes([closed.runtime], { reason: 'pending-test' }, closed.releasePorts), /not live/);
  assert.equal(closed.healthCalls.length, 0);
  const old = await releasableAndroid(t), newer = androidLaunchFixture(t, 'term', false, old.avdName);
  newer.ports.waitForAvd = async (_sdk, _name, onSerial) => { onSerial('new-current-serial'); return 'new-current-serial'; };
  try {
    await old.api.prepareAndroid(newer.avdName, () => {}, { runDirectory: newer.directory, testNumber: 2, attempt: 1 }, {
      ...newer.ports, runCommand: (file, args, options) => newer.ports.runCommand(file, Array.from(args), options),
      spawnProcess: (file, args, options) => newer.ports.spawnProcess(file, Array.from(args), options),
    });
    await assert.rejects(old.api.releaseHealthyRunnerOwnedRuntimes([old.runtime], { reason: 'pending-test' }, old.releasePorts), /owner changed/);
    assert.equal(old.healthCalls.length, 0); assert.equal(newer.child.closed, false);
  } finally { if (!newer.child.closed) newer.close(0, null); }
});

test('exact iOS boot release checks unchanged guest lifetime and mixed-array refusal is atomic', async t => {
  const f = await releasableAndroid(t), published = [], calls = [];
  const inventory = state => ({ devices: { 'com.apple.CoreSimulator.SimRuntime.iOS-26-5': [{ ...iosPlatform,
    name: iosPlatform.simulatorName, state, isAvailable: true, deviceTypeIdentifier: 'com.apple.CoreSimulator.SimDeviceType.iPhone-17' }] } });
  await f.api.prepareIos(iosPlatform.simulatorName, runtime => published.push(runtime), {
    runCommand: async (_file, args) => { if (args.includes('list')) return { stdout: JSON.stringify(inventory('Shutdown')) }; return { stdout: args.includes('spawn') ? '96981\n' : '' }; },
    openDeviceUi: async () => ({}),
  });
  assert.equal(published.length, 1);
  const ports = { ...f.releasePorts, readIosInventory: async () => inventory('Booted'), runCommand: async (file, args, options) => {
    if (file !== 'xcrun') return f.releasePorts.runCommand(file, args, options);
    calls.push(Array.from(args)); return { stdout: '96982\n' };
  } };
  await assert.rejects(f.api.releaseHealthyRunnerOwnedRuntimes([f.runtime, published[0]], { reason: 'pending-test' }, ports), /guest lifetime changed/);
  assert.equal(calls.length, 1); assert.equal(f.child.closed, false);
  ports.runCommand = async (file, args, options) => file === 'xcrun' ? { stdout: '96981\n' } : f.releasePorts.runCommand(file, args, options);
  const receipts = await f.api.releaseHealthyRunnerOwnedRuntimes([f.runtime, published[0]], { reason: 'pending-test' }, ports);
  assert.equal(receipts.length, 2); assert.equal(receipts[1].platform, 'iOS');
  const before = f.commands.length; await f.api.stopRunnerOwnedRuntimes([f.runtime, published[0]]);
  assert.equal(f.commands.length, before); assert.equal(f.child.closed, false);
});
