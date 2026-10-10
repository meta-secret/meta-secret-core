import assert from 'node:assert/strict';
import { chmodSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { discoverAndroidSdk, findOnlineAndroidAvd, withAndroidSdkPath } from './android-sdk.mjs';

test('discovers SDK tools from ANDROID_SDK_ROOT and builds a child PATH', (t) => {
  const root = mkdtempSync(join(tmpdir(), 'metasecret-android-sdk-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  for (const relativePath of ['platform-tools/adb', 'emulator/emulator']) {
    const executable = join(root, relativePath);
    mkdirSync(dirname(executable), { recursive: true });
    writeFileSync(executable, '');
    chmodSync(executable, 0o755);
  }

  const sdk = discoverAndroidSdk({ env: { ANDROID_SDK_ROOT: root }, home: '/not-used' });
  assert.equal(sdk.sdkRoot, root);
  assert.equal(sdk.adbPath, join(root, 'platform-tools/adb'));
  assert.equal(sdk.emulatorPath, join(root, 'emulator/emulator'));
  assert.equal(withAndroidSdkPath('/usr/bin', sdk), `${sdk.pathPrefix}${process.platform === 'win32' ? ';' : ':'}/usr/bin`);
});

test('reports a setup instruction when Android SDK tools are missing', () => {
  assert.throws(
    () => discoverAndroidSdk({ env: { ANDROID_SDK_ROOT: '/missing-sdk' }, home: '/missing-home' }),
    /install Android SDK Platform-Tools and Emulator/,
  );
});

test('selects only the requested online AVD and ignores another emulator', async () => {
  const calls = [];
  const sdk = { adbPath: '/sdk/platform-tools/adb' };
  const runAndCapture = async (command, args) => {
    calls.push(args);
    if (args[0] === 'devices') return { stdout: 'List of devices attached\nemulator-5554 device\nemulator-5556 device\n' };
    if (args[1] === 'emulator-5554') return { stdout: 'Other_AVD\nOK\n' };
    return { stdout: 'Pixel_4a\nOK\n' };
  };

  assert.equal(await findOnlineAndroidAvd(sdk, 'Pixel_4a', runAndCapture), 'emulator-5556');
  assert.equal(calls.length, 3);
});

test('does not treat an online emulator with an unresponsive identity console as absent', async () => {
  const sdk = { adbPath: '/sdk/platform-tools/adb' };
  const runAndCapture = async (_command, args) => {
    if (args[0] === 'devices') return { stdout: 'List of devices attached\nemulator-5554 device\n' };
    throw new Error('emulator console timed out');
  };

  await assert.rejects(
    findOnlineAndroidAvd(sdk, 'Pixel_4a', runAndCapture),
    (error) => error.code === 'ANDROID_AVD_IDENTITY_UNRESPONSIVE'
      && error.serial === 'emulator-5554'
      && /refusing to launch AVD Pixel_4a/.test(error.message),
  );
});

test('does not treat an empty online emulator identity response as absent', async () => {
  const sdk = { adbPath: '/sdk/platform-tools/adb' };
  const runAndCapture = async (_command, args) => {
    if (args[0] === 'devices') return { stdout: 'List of devices attached\nemulator-5554 device\n' };
    return { stdout: '' };
  };

  await assert.rejects(
    findOnlineAndroidAvd(sdk, 'Pixel_4a', runAndCapture),
    (error) => error.code === 'ANDROID_AVD_IDENTITY_UNRESPONSIVE',
  );
});
