import assert from 'node:assert/strict';
import test from 'node:test';

import { resetE2eNativeAppState } from './e2e-state-reset.mjs';

test('iOS app reset removes only the app sandbox and preserves the booted simulator', async () => {
  let installed = true;
  const calls = [];
  const events = [];
  const runCommand = async (file, args) => {
    calls.push([file, ...args]);
    if (args[0] === 'simctl' && args[1] === 'get_app_container') {
      if (!installed) throw new Error('Application is not installed');
      return { stdout: '/simulator/app-container' };
    }
    if (args[0] === 'simctl' && args[1] === 'uninstall') {
      installed = false;
      return { stdout: '' };
    }
    if (args[0] === 'simctl' && args[1] === 'list') {
      return { stdout: JSON.stringify({ devices: { runtime: [{ udid: 'ios-udid', state: 'Booted' }] } }) };
    }
    throw new Error(`Unexpected command: ${file} ${args.join(' ')}`);
  };

  const result = await resetE2eNativeAppState({
    scenario: { ios: { bundleId: 'org.metasecret.vault' } },
    platforms: [{ platform: 'iOS', simulatorName: 'iPhone 17', udid: 'ios-udid' }],
    onEvent: (event) => events.push(event),
    runCommand,
  });

  assert.deepEqual(result, { platformsReset: ['iOS'] });
  assert.equal(installed, false);
  assert.ok(events.includes('uninstalled and verified iOS app org.metasecret.vault'));
  assert.ok(events.includes('verified iOS simulator iPhone 17 remains healthy after app reset'));
  assert.equal(calls.some((call) => call.includes('erase') || call.includes('shutdown')), false);
});

test('Android app reset removes app and instrumentation packages from the same ready AVD', async () => {
  const installed = new Set(['metasecret.project.com', 'metasecret.project.com.test']);
  const calls = [];
  const events = [];
  const runCommand = async (_file, args) => {
    calls.push(args);
    if (args.length === 1 && args[0] === 'devices') {
      return { stdout: 'List of devices attached\nemulator-5554\tdevice\n' };
    }
    if (args.at(-2) === 'emu' && args.at(-1) === 'avd' || args.join(' ') === '-s emulator-5554 emu avd name') {
      return { stdout: 'Pixel_4a\nOK\n' };
    }
    if (args.join(' ') === '-s emulator-5554 get-state') return { stdout: 'device\n' };
    if (args.join(' ') === '-s emulator-5554 shell getprop sys.boot_completed') return { stdout: '1\n' };
    if (args.join(' ') === '-s emulator-5554 shell cmd activity get-started-user-state 0') {
      return { stdout: 'RUNNING_UNLOCKED\n' };
    }
    if (args.join(' ') === '-s emulator-5554 shell pm list packages --user 0') {
      return { stdout: [...installed].map((name) => `package:${name}`).join('\n') };
    }
    if (args[0] === '-s' && args[1] === 'emulator-5554' && args[2] === 'uninstall') {
      installed.delete(args[3]);
      return { stdout: 'Success\n', stderr: '' };
    }
    throw new Error(`Unexpected command: ${args.join(' ')}`);
  };

  const result = await resetE2eNativeAppState({
    scenario: { android: { bundleId: 'metasecret.project.com' } },
    platforms: [{ platform: 'Android', avdName: 'Pixel_4a', serial: 'emulator-5554' }],
    onEvent: (event) => events.push(event),
    runCommand,
    discoverSdk: () => ({ adbPath: 'adb' }),
  });

  assert.deepEqual(result, { platformsReset: ['Android'] });
  assert.deepEqual([...installed], []);
  assert.ok(events.includes('uninstalled and verified Android package metasecret.project.com'));
  assert.ok(events.includes('uninstalled and verified Android package metasecret.project.com.test'));
  assert.ok(events.includes('verified Android AVD Pixel_4a is healthy after app reset'));
  assert.equal(calls.some((args) => args.includes('emu') && args.includes('kill')), false);
  assert.equal(calls.some((args) => args.includes('-wipe-data')), false);
});
