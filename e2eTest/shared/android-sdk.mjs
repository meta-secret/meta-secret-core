import { existsSync } from 'node:fs';
import { delimiter, join, resolve } from 'node:path';
import { homedir } from 'node:os';

export function discoverAndroidSdk({ env = process.env, home = homedir() } = {}) {
  const candidates = [
    env.ANDROID_SDK_ROOT,
    env.ANDROID_HOME,
    resolve(home, 'Library/Android/sdk'),
  ].filter(Boolean);
  const sdkRoot = [...new Set(candidates)].find((candidate) =>
    existsSync(join(candidate, 'platform-tools', 'adb'))
      && existsSync(join(candidate, 'emulator', 'emulator')));

  if (!sdkRoot) {
    const roots = [...new Set(candidates)].join(', ') || '<none found>';
    throw new Error(
      `Android SDK tools are unavailable. Checked ${roots}; install Android SDK Platform-Tools and Emulator, `
        + 'or set ANDROID_SDK_ROOT to the SDK directory.',
    );
  }

  const platformTools = join(sdkRoot, 'platform-tools');
  const emulatorDirectory = join(sdkRoot, 'emulator');
  return {
    sdkRoot,
    adbPath: join(platformTools, 'adb'),
    emulatorPath: join(emulatorDirectory, 'emulator'),
    pathPrefix: [platformTools, emulatorDirectory].join(delimiter),
  };
}

export function withAndroidSdkPath(pathValue, sdk) {
  return [sdk.pathPrefix, pathValue].filter(Boolean).join(delimiter);
}

export async function findOnlineAndroidAvd(sdk, avdName, runAndCapture) {
  const { stdout } = await runAndCapture(sdk.adbPath, ['devices']);
  const serials = stdout.split(/\r?\n/)
    .map((line) => line.trim().split(/\s+/))
    .filter(([serial, state]) => serial?.startsWith('emulator-') && state === 'device')
    .map(([serial]) => serial);

  const unresolved = [];
  for (const serial of serials) {
    let response;
    try {
      response = await runAndCapture(sdk.adbPath, ['-s', serial, 'emu', 'avd', 'name']);
    } catch (error) {
      unresolved.push({ serial, cause: error });
      continue;
    }
    const firstResponseLine = response?.stdout.split(/\r?\n/)
      .map((line) => line.trim())
      .find((line) => line && line.toUpperCase() !== 'OK');
    const runningAvd = firstResponseLine?.replace(/^(?:Avd name|name of avd):\s*/i, '');
    if (!runningAvd) {
      unresolved.push({ serial, cause: new Error('AVD identity console returned no name') });
      continue;
    }
    if (runningAvd === avdName) return serial;
  }
  if (unresolved.length > 0) {
    const error = new Error(
      `Could not identify online Android emulator(s) ${unresolved.map(({ serial }) => serial).join(', ')}; `
        + `refusing to launch AVD ${avdName} because it may already be running`,
      { cause: unresolved[0].cause },
    );
    error.code = 'ANDROID_AVD_IDENTITY_UNRESPONSIVE';
    error.serial = unresolved.length === 1 ? unresolved[0].serial : undefined;
    error.unresolvedSerials = unresolved.map(({ serial }) => serial);
    throw error;
  }
  return null;
}
