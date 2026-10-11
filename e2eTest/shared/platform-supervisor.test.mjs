import test from 'node:test';
import assert from 'node:assert/strict';
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

const iosPlatform = { platform: 'iOS', simulatorName: 'selected-iPhone', udid: 'SELECTED-SIMULATOR-UDID' };

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
    args: ['simctl', 'spawn', 'SELECTED-SIMULATOR-UDID', 'launchctl', 'managerpid'],
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
        if (args.includes('list')) return { stdout: JSON.stringify({ devices: { runtime: [{ name: iosPlatform.simulatorName, udid: iosPlatform.udid, state: 'Booted' }] } }) };
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
