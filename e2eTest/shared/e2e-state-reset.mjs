import { prepareIosIdentityBoundary } from './ios-test-identity.mjs';
import { promisify } from 'node:util';
import { execFile } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';
import { discoverAndroidSdk, findOnlineAndroidAvd } from './android-sdk.mjs';
import { assertExistingDeviceRun, verifyOwnedIosResetTarget, verifyIosRuntimeHealth, verifyOwnedAndroidResetTarget } from './platform-supervisor.mjs';

const execFileAsync = promisify(execFile);
const E2E_SERVER_CONTAINER = /^meta-secret-e2e-server(?:-[a-zA-Z0-9_.-]+)?$/;

function commandFailureDetails(error) {
  return [
    error.message,
    error.code == null ? null : `exitCode=${error.code}`,
    error.signal ? `signal=${error.signal}` : null,
    error.stdout ? `stdout: ${String(error.stdout).trim()}` : null,
    error.stderr ? `stderr: ${String(error.stderr).trim()}` : null,
  ].filter(Boolean).join('\n');
}

async function command(file, args, options = {}) {
  return execFileAsync(file, args, {
    cwd: options.cwd,
    env: process.env,
    timeout: options.timeout ?? 30_000,
    maxBuffer: 4 * 1024 * 1024,
  });
}

async function resetServerContainer(server, onEvent, runCommand = command) {
  if (!server?.container) return;
  if (!E2E_SERVER_CONTAINER.test(server.container)) {
    throw new Error(`Refusing to reset non-E2E server container: ${server.container}`);
  }
  const url = new URL(server.url);
  if (!['localhost', '127.0.0.1', '::1'].includes(url.hostname)) {
    throw new Error(`Refusing to reset server for non-local endpoint: ${url.hostname}`);
  }

  let containerId;
  try {
    const { stdout } = await runCommand('docker', [
      'ps', '-aq', '--filter', `name=^/${server.container}$`,
    ]);
    containerId = stdout.trim().split(/\s+/)[0];
  } catch (error) {
    throw new Error(`Could not inspect local E2E server container ${server.container}: ${error.message}`);
  }
  if (containerId) {
    await runCommand('docker', ['rm', '-f', containerId]);
    onEvent(`removed prior E2E server container ${server.container}`);
  } else {
    onEvent(`no prior E2E server container ${server.container}`);
  }
  const { stdout: remaining } = await runCommand('docker', [
    'ps', '-aq', '--filter', `name=^/${server.container}$`,
  ]);
  if (remaining.trim()) throw new Error(`Prior E2E server container still exists: ${server.container}`);
  onEvent(`verified E2E server container ${server.container} is absent`);
}

async function waitForAndroidReady(platform, sdk, runCommand = command, timeoutMs = 90_000) {
  const deadline = Date.now() + timeoutMs;
  let lastError = 'Android AVD is offline or not in adb state=device';
  while (Date.now() < deadline) {
    try {
      const serial = await findOnlineAndroidAvd(sdk, platform.ownedAndroidProfile?.physicalAvdName ?? platform.avdName, (file, args) =>
        runCommand(file, args, { timeout: 10_000 }));
      if (serial !== platform.serial) {
        lastError = serial ? `AVD resolved to unexpected serial ${serial}` : 'Android AVD is offline';
      } else {
        const { stdout: state } = await runCommand(sdk.adbPath, ['-s', serial, 'get-state'], { timeout: 10_000 });
        if (state.trim() !== 'device') throw new Error(`adb get-state=${state.trim() || '<empty>'}`);
        const { stdout: bootCompleted } = await runCommand(sdk.adbPath, [
          '-s', serial, 'shell', 'getprop', 'sys.boot_completed',
        ], { timeout: 10_000 });
        if (bootCompleted.trim() !== '1') throw new Error(`sys.boot_completed=${bootCompleted.trim() || '<empty>'}`);
        const { stdout: userState } = await runCommand(sdk.adbPath, [
          '-s', serial, 'shell', 'cmd', 'activity', 'get-started-user-state', '0',
        ], { timeout: 10_000 });
        if (userState.trim() !== 'RUNNING_UNLOCKED') {
          throw new Error(`Android primary user state=${userState.trim() || '<empty>'}`);
        }
        const { stdout: packages } = await runCommand(sdk.adbPath, [
          '-s', serial, 'shell', 'pm', 'list', 'packages', '--user', '0',
        ], { timeout: 15_000 });
        return packages;
      }
    } catch (error) {
      lastError = commandFailureDetails(error);
    }
    await delay(1_000);
  }
  throw new Error(`Android AVD ${platform.avdName} did not recover before state reset: ${lastError}`);
}

async function androidPackageInstalled(sdk, platform, bundleId, onEvent, runCommand = command) {
  let lastError;
  for (let attempt = 1; attempt <= 3; attempt += 1) {
    try {
      const packageList = await waitForAndroidReady(platform, sdk, runCommand);
      return packageList.split(/\r?\n/).some((line) => line.trim() === `package:${bundleId}`);
    } catch (error) {
      lastError = error;
      const detail = commandFailureDetails(error);
      const transientTransport = /device offline|no devices\/emulators found|device .*not found|transport.*(?:error|closed)/i.test(detail);
      if (!transientTransport || attempt === 3) break;
      onEvent(`Android transport interrupted while checking ${bundleId}; waiting for the same AVD to recover (attempt ${attempt}/3)`);
    }
  }
  const detail = lastError
    ? commandFailureDetails(lastError)
    : 'unknown ADB failure';
  throw new Error(`Could not inspect Android package ${bundleId}: ${detail}`);
}

async function uninstallAndroid(platform, appBundleId, onEvent, runCommand = command, sdk = discoverAndroidSdk()) {
  if (!appBundleId) return;
  for (const bundleId of [appBundleId, `${appBundleId}.test`]) {
    if (!await androidPackageInstalled(sdk, platform, bundleId, onEvent, runCommand)) {
      onEvent(`Android package ${bundleId} was already absent`);
      continue;
    }

    let lastError;
    for (let attempt = 1; attempt <= 3; attempt += 1) {
      try {
        const { stdout, stderr } = await runCommand(sdk.adbPath, [
          '-s', platform.serial, 'uninstall', bundleId,
        ], { timeout: 30_000 });
        const output = `${stdout}\n${stderr}`;
        if (!/Success/i.test(output)) {
          throw new Error(output.trim() || `adb uninstall returned no success status for ${bundleId}`);
        }
        if (await androidPackageInstalled(sdk, platform, bundleId, onEvent, runCommand)) {
          throw new Error(`Android package ${bundleId} is still installed after adb uninstall`);
        }
        onEvent(`uninstalled and verified Android package ${bundleId}`);
        lastError = null;
        break;
      } catch (error) {
        lastError = error;
        const detail = commandFailureDetails(error);
        const transientTransport = /device offline|no devices\/emulators found|device .*not found|transport.*(?:error|closed)/i.test(detail);
        if (!transientTransport || attempt === 3) break;
        onEvent(`Android transport became unavailable while removing ${bundleId}; waiting for the same AVD to recover (attempt ${attempt}/3)`);
        await waitForAndroidReady(platform, sdk, runCommand);
      }
    }
    if (lastError) {
      const detail = commandFailureDetails(lastError);
      throw new Error(`Could not reset Android package ${bundleId}: ${detail.trim()}`);
    }
  }
  await waitForAndroidReady(platform, sdk, runCommand);
  onEvent(`verified Android AVD ${platform.avdName} is healthy after app reset`);
}

function prepareIosScope(scenario, platforms, context) {
  const ios = platforms.filter(item => item.platform === 'iOS');
  if (!ios.length) return null;
  if (!context || ios.length !== 1 || ios[0].udid !== context.udid || scenario.ios?.bundleId !== context.bundleId) {
    throw Error('Isolated iOS identity context is required before reset');
  }
  return prepareIosIdentityBoundary(context);
}
async function stopIosApp(platform, appBundleId, onEvent, runCommand) {
  try { await runCommand('xcrun', ['simctl', 'terminate', platform.udid, appBundleId]); }
  catch (error) {
    if (!/not running|not installed|No such process|found nothing to terminate/i.test(error.message)) throw error;
  }
  onEvent('selected isolated iOS identity; stopped only the exact app, all stored scopes preserved');
}

/**
 * Reset only the selected scenario's local E2E state. Runtimes must already be
 * booted; the scenario orchestrator installs and verifies clients afterward.
 */
export async function resetE2eScenarioState({ scenario, platforms, onEvent = () => {}, runCommand = command, discoverSdk = discoverAndroidSdk, runId = process.env.E2E_RUN_ID, readinessPath = process.env.E2E_READINESS_PATH, iosIdentityContext }) {
  assertExistingDeviceRun({ scenarios: [scenario], readiness: { platforms } });
  if (process.env.E2E_ANDROID_OWNED_PROFILE !== undefined && platforms.some(item => item.platform === 'Android' && !item.ownedAndroidProfile)) {
    throw new Error('Declared owned Android reset lost its profile handoff');
  }
  for (const platform of platforms.filter((item) => item.platform === 'Android' && item.ownedAndroidProfile)) {
    await verifyOwnedAndroidResetTarget(platform, { runId, readinessPath, runCommand });
  }
  for (const platform of platforms.filter((item) => item.platform === 'iOS' && item.ownedIosProfile)) {
    await verifyOwnedIosResetTarget(platform, { runId, readinessPath, runCommand });
  }
  if (scenario.server?.url && !scenario.server.container) {
    throw new Error('Scenario declares a server URL but no E2E container name; cannot prove a clean server state');
  }
  prepareIosScope(scenario, platforms, iosIdentityContext);
  await resetServerContainer(scenario.server, onEvent, runCommand);
  const reset = await resetE2eNativeAppState({ scenario, platforms, onEvent, runCommand, discoverSdk, runId, readinessPath, iosIdentityContext });
  return { serverReset: Boolean(scenario.server?.container), ...reset };
}

/** Rotate the iOS test scope or reset Android app packages; preserve device state. */
export async function resetE2eNativeAppState({
  scenario,
  platforms,
  onEvent = () => {},
  runCommand = command,
  discoverSdk = discoverAndroidSdk,
  runId = process.env.E2E_RUN_ID,
  readinessPath = process.env.E2E_READINESS_PATH,
  iosIdentityContext,
}) {
  assertExistingDeviceRun({ scenarios: [scenario], readiness: { platforms } });
  // A claimed owned target must be verified before any app or device mutation.
  if (process.env.E2E_ANDROID_OWNED_PROFILE !== undefined && platforms.some(item => item.platform === 'Android' && !item.ownedAndroidProfile)) {
    throw new Error('Declared owned Android reset lost its profile handoff');
  }
  for (const platform of platforms.filter((item) => item.platform === 'Android' && item.ownedAndroidProfile)) {
    await verifyOwnedAndroidResetTarget(platform, { runId, readinessPath, runCommand });
  }
  for (const platform of platforms.filter((item) => item.platform === 'iOS' && item.ownedIosProfile)) {
    await verifyOwnedIosResetTarget(platform, { runId, readinessPath, runCommand });
  }
  prepareIosScope(scenario, platforms, iosIdentityContext);
  const sdk = platforms.some(({ platform }) => platform === 'Android') ? discoverSdk() : null;
  for (const platform of platforms) {
    if (platform.platform === 'Android') {
      await uninstallAndroid(platform, scenario.android?.bundleId, onEvent, runCommand, sdk);
    } else if (platform.platform === 'iOS') {
      await stopIosApp(platform, scenario.ios?.bundleId, onEvent, runCommand);

    }
  }
  for (const platform of platforms.filter(({ platform: name }) => name === 'iOS')) {
    const { stdout } = await runCommand('xcrun', ['simctl', 'list', 'devices', 'available', '-j']);
    const devices = Object.values(JSON.parse(stdout).devices).flat();
    const simulator = devices.find((device) => device.udid === platform.udid);
    if (!simulator || simulator.state !== 'Booted') {
      throw new Error(`iOS simulator ${platform.simulatorName} did not remain Booted after reset`);
    }
    onEvent(`verified iOS simulator ${platform.simulatorName} remains healthy after app reset`);
  }
  return { platformsReset: platforms.map((platform) => platform.platform) };
}
