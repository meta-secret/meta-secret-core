import { spawn } from 'node:child_process';
import { closeSync, existsSync, mkdirSync, openSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import { promisify } from 'node:util';
import { execFile } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';
import { monitorEventLoopDelay } from 'node:perf_hooks';
import { discoverAndroidSdk, findOnlineAndroidAvd, withAndroidSdkPath } from './android-sdk.mjs';
import { openVisibleIosDeviceUi } from './visible-ios-ui.mjs';

const execFileAsync = promisify(execFile);
const e2eRoot = resolve(fileURLToPath(new URL('..', import.meta.url)));
const coreRoot = resolve(e2eRoot, '..');
const workspaceRoot = resolve(coreRoot, '..');
const runnerOwnedAndroidProcesses = new Map();
async function command(file, args, options = {}) {
  return execFileAsync(file, args, {
    cwd: options.cwd,
    env: { ...process.env, ...options.env },
    timeout: options.timeout ?? 60_000,
    maxBuffer: 16 * 1024 * 1024,
  });
}

export async function verifyAndroidInstallReadiness({ adbPath, serial, runCommand = command, timeoutMs = 10_000 }) {
  const probes = [
    {
      id: 'shell-services',
      code: 'ANDROID_AVD_SHELL_SERVICES_UNRESPONSIVE',
      args: ['-s', serial, 'shell', 'service', 'list'],
      isReady: (stdout) => /^Found\s+\d+\s+services:?\s*$/m.test(stdout),
    },
    {
      id: 'package-manager',
      code: 'ANDROID_AVD_PACKAGE_MANAGER_UNRESPONSIVE',
      args: ['-s', serial, 'shell', 'cmd', 'package', 'list', 'packages', '--user', '0'],
      isReady: (stdout) => stdout.split(/\r?\n/).some((line) => line.startsWith('package:')),
    },
  ];

  for (const probe of probes) {
    try {
      const { stdout } = await runCommand(adbPath, probe.args, { timeout: timeoutMs });
      if (!probe.isReady(stdout)) {
        throw Object.assign(new Error('probe returned no expected inventory'), { code: 'INVALID_PROBE_OUTPUT' });
      }
    } catch (cause) {
      const timedOut = cause.code === 'ETIMEDOUT' || cause.killed;
      const error = new Error(
        `Android ADB transport is online, but the ${probe.id} probe ${timedOut ? 'timed out' : 'failed'} for ${serial}`,
        { cause },
      );
      error.code = probe.code;
      error.readinessProbe = probe.id;
      throw error;
    }
  }

  return { adbTransport: 'online', shellServices: 'responsive', packageManager: 'responsive' };
}

export function shouldRestartAndroidAvdForReadinessFailure(error, recoveryAttempt) {
  return recoveryAttempt === 0
    && (error?.code === 'ANDROID_AVD_SHELL_SERVICES_UNRESPONSIVE'
      || error?.code === 'ANDROID_AVD_PACKAGE_MANAGER_UNRESPONSIVE');
}

export function shouldColdBootOwnedOfflineAndroidAvd({ error, recoveryAttempt, startedBySupervisor, offlineSerials, qemuPids, launchPid }) {
  return recoveryAttempt === 0
    && startedBySupervisor
    && error?.code === 'ANDROID_AVD_OFFLINE_TIMEOUT'
    && Array.isArray(offlineSerials)
    && offlineSerials.length === 1
    && Array.isArray(qemuPids)
    && qemuPids.length === 1
    && qemuPids[0] === launchPid;
}

export function shouldColdBootAndroidAvdOnLaunch(existingOnlineSerial) {
  return !existingOnlineSerial;
}

export function androidEmulatorLaunchArgs(avdName, { coldBoot = false } = {}) {
  return ['-avd', avdName, ...(coldBoot ? ['-no-snapshot-load'] : [])];
}

export function bindAndroidReadinessFailureToSerial(error, serial) {
  error.serial ??= serial;
  return error;
}

function waitForAndroidAvd(sdk, avdName, onSerial = () => {}, timeoutMs = 180_000, launch = null) {
  const deadline = Date.now() + timeoutMs;
  return (async () => {
    let lastError = 'AVD has not appeared in adb devices';
    let identifiedSerial = null;
    let offlineSince = null;
    let offlineSerials = [];
    while (Date.now() < deadline) {
      try {
        if (launch?.exit) {
          const output = existsSync(launch.logPath) ? readFileSync(launch.logPath, 'utf8').slice(-12_000) : '';
          throw new Error(
            `Android Emulator process exited (code=${launch.exit.code ?? 'none'}, signal=${launch.exit.signal ?? 'none'}); `
              + `log=${launch.logPath}${output ? `\n${output}` : ''}`,
          );
        }
        const serial = await findOnlineAndroidAvd(sdk, avdName, (file, args) => command(file, args, { timeout: 10_000 }));
        if (serial) {
          offlineSince = null;
          offlineSerials = [];
          identifiedSerial = serial;
          onSerial(serial);
          const { stdout: deviceState } = await command(sdk.adbPath, ['-s', serial, 'get-state'], { timeout: 10_000 });
          if (deviceState.trim() !== 'device') {
            throw new Error(`adb get-state=${deviceState.trim() || '<empty>'}`);
          }
          const { stdout } = await command(sdk.adbPath, ['-s', serial, 'shell', 'getprop', 'sys.boot_completed'], { timeout: 10_000 });
          if (stdout.trim() !== '1') {
            throw new Error(`sys.boot_completed=${stdout.trim() || '<empty>'}`);
          }
          const { stdout: userState } = await command(sdk.adbPath, [
            '-s', serial, 'shell', 'cmd', 'activity', 'get-started-user-state', '0',
          ], { timeout: 10_000 });
          if (userState.trim() !== 'RUNNING_UNLOCKED') {
            throw new Error(`Android primary user state=${userState.trim() || '<empty>'}`);
          }
          await command(sdk.adbPath, ['-s', serial, 'shell', 'pm', 'list', 'packages', '--user', '0'], { timeout: 15_000 });
          await verifyAndroidInstallReadiness({ adbPath: sdk.adbPath, serial });
          return serial;
        }
        if (launch) {
          const { stdout: devices } = await command(sdk.adbPath, ['devices'], { timeout: 10_000 });
          offlineSerials = devices.split(/\r?\n/)
            .map((line) => line.trim().split(/\s+/))
            .filter(([candidate, state]) => candidate?.startsWith('emulator-') && state === 'offline')
            .map(([candidate]) => candidate);
          if (offlineSerials.length > 0) {
            offlineSince ??= Date.now();
            lastError = `ADB emulator serial(s) remain offline: ${offlineSerials.join(', ')}`;
            if (Date.now() - offlineSince >= 30_000) {
              const error = new Error(`Android AVD ${avdName} remained offline in ADB for 30 seconds`);
              error.code = 'ANDROID_AVD_OFFLINE_TIMEOUT';
              error.offlineSerials = offlineSerials;
              throw error;
            }
          } else {
            offlineSince = null;
          }
        }
      } catch (error) {
        if (error.code === 'ANDROID_AVD_OFFLINE_TIMEOUT') throw error;
        lastError = error.message;
        if (error.code === 'ANDROID_AVD_SHELL_SERVICES_UNRESPONSIVE'
            || error.code === 'ANDROID_AVD_PACKAGE_MANAGER_UNRESPONSIVE') {
          throw bindAndroidReadinessFailureToSerial(error, identifiedSerial);
        }
      }
      await delay(1_000);
    }
    throw new Error(`Android AVD ${avdName} did not become ready: ${lastError}`);
  })();
}

async function ensureAndroidAdbServer(sdk) {
  const { stdout: versionOutput } = await command(sdk.adbPath, ['version'], { timeout: 10_000 });
  await command(sdk.adbPath, ['start-server'], { timeout: 15_000 });
  const { stdout: deviceOutput } = await command(sdk.adbPath, ['devices'], { timeout: 10_000 });
  return {
    adbPath: sdk.adbPath,
    version: versionOutput.split(/\r?\n/).find((line) => /Version\s/i.test(line))?.trim() ?? 'unknown',
    serverAvailable: true,
    devicesAtCheck: deviceOutput.split(/\r?\n/).slice(1).map((line) => line.trim()).filter(Boolean),
  };
}

async function findAvdQemuPids(avdName) {
  const args = process.platform === 'darwin' ? ['-axo', 'pid=,command='] : ['-eo', 'pid=,args='];
  const { stdout } = await command('ps', args, { timeout: 10_000 });
  const matches = [];
  for (const line of stdout.split(/\r?\n/)) {
    const match = line.trim().match(/^(\d+)\s+(.+)$/);
    if (!match) continue;
    const [, pidText, commandLine] = match;
    const tokens = commandLine.match(/"[^"]*"|'[^']*'|\S+/g)?.map((token) => token.replace(/^['"]|['"]$/g, '')) ?? [];
    const executable = tokens[0]?.split('/').pop() ?? '';
    const avdIndex = tokens.indexOf('-avd');
    if (/^qemu-system(?:-|$)/.test(executable) && avdIndex >= 0 && tokens[avdIndex + 1] === avdName) {
      matches.push(Number(pidText));
    }
  }
  return matches;
}

async function restartUnresponsiveAvd(sdk, avdName, serial, runDirectory, testNumber, attempt, reason = 'identity console') {
  if (!serial) {
    throw new Error(`An online Android emulator failed its ${reason} probe, but its serial is ambiguous; refusing to restart AVD ${avdName}`);
  }
  const pids = await findAvdQemuPids(avdName);
  if (pids.length !== 1) {
    throw new Error(
      `Android emulator ${serial} failed its ${reason} probe and ${pids.length} matching ${avdName} QEMU processes were found; `
        + 'refusing an ambiguous restart',
    );
  }
  const logsDirectory = resolve(runDirectory, 'platforms');
  mkdirSync(logsDirectory, { recursive: true });
  const logPath = resolve(logsDirectory, `android-${avdName}-recovery-test-${String(testNumber).padStart(2, '0')}-attempt-${attempt}.log`);
  const record = (message) => writeFileSync(logPath, `${new Date().toISOString()} ${message}\n`, { flag: 'a', mode: 0o600 });
  record(`unresponsive online serial=${serial}; failed probe=${reason}; restarting matching QEMU pid=${pids[0]}`);
  process.stdout.write(`⚠️ Android AVD ${avdName} is online in ADB but its ${reason} probe is unresponsive; restarting its matching stale QEMU process\n`);
  try {
    process.kill(pids[0], 'SIGTERM');
  } catch (error) {
    if (error.code !== 'ESRCH') throw new Error(`Could not stop stale Android emulator process: ${error.message}`);
  }

  const deadline = Date.now() + 30_000;
  let lastState = 'ADB serial has not disconnected';
  while (Date.now() < deadline) {
    let stillRunning = false;
    try { process.kill(pids[0], 0); stillRunning = true; } catch (error) { if (error.code !== 'ESRCH') throw error; }
    const { stdout } = await command(sdk.adbPath, ['devices'], { timeout: 10_000 });
    const stateLine = stdout.split(/\r?\n/).map((line) => line.trim().split(/\s+/))
      .find(([candidate]) => candidate === serial);
    lastState = stateLine?.[1] ?? 'disconnected';
    if (!stillRunning && lastState !== 'device') {
      record(`stale emulator stopped; prior serial state=${lastState}`);
      return logPath;
    }
    await delay(500);
  }
  throw new Error(`Stale Android AVD ${avdName} did not stop cleanly (serial state=${lastState}); see ${logPath}`);
}

async function prepareAndroid(avdName, onRuntimeStarted, runDirectory, testNumber, attempt) {
  const sdk = discoverAndroidSdk();
  const adbServer = await ensureAndroidAdbServer(sdk);
  process.stdout.write(`🔌 Android ADB server ready: ${adbServer.version} (${adbServer.adbPath})\n`);
  const { stdout } = await command(sdk.emulatorPath, ['-list-avds']);
  const available = stdout.split(/\r?\n/).map((value) => value.trim()).filter(Boolean);
  if (!available.includes(avdName)) {
    throw new Error(`Android AVD ${avdName} is not installed; available: ${available.join(', ') || '<none>'}`);
  }
  let serial = null;
  let startedBySupervisor = false;
  let launch = null;
  let forceColdBoot = false;
  try {
    serial = await findOnlineAndroidAvd(sdk, avdName, (file, args) => command(file, args, { timeout: 10_000 }));
  } catch (error) {
    if (error.code !== 'ANDROID_AVD_IDENTITY_UNRESPONSIVE') throw error;
    await restartUnresponsiveAvd(sdk, avdName, error.serial, runDirectory, testNumber, attempt);
    forceColdBoot = true;
  }
  // Quick Boot can leave a device that later loses its ADB transport during
  // instrumentation. When this run owns the launch, start the same AVD from
  // a clean boot without loading a snapshot; keep its user data intact.
  forceColdBoot ||= shouldColdBootAndroidAvdOnLaunch(serial);
  const launchAndroid = async () => {
    if (forceColdBoot) {
      process.stdout.write(`♻️ Cold-booting Android AVD ${avdName} without the saved emulator snapshot\n`);
    }
    process.stdout.write(`📱 Starting visible Android emulator ${avdName}\n`);
    const logsDirectory = resolve(runDirectory, 'platforms');
    mkdirSync(logsDirectory, { recursive: true });
    const logPath = resolve(logsDirectory, `android-${avdName}-test-${String(testNumber).padStart(2, '0')}-attempt-${attempt}.log`);
    const logFd = openSync(logPath, 'a', 0o600);
    const emulator = spawn(sdk.emulatorPath, androidEmulatorLaunchArgs(avdName, { coldBoot: forceColdBoot }), {
      detached: true,
      stdio: ['ignore', logFd, logFd],
      env: {
        ...process.env,
        PATH: withAndroidSdkPath(process.env.PATH, sdk),
        ANDROID_ADB_SERVER_PORT: process.env.ANDROID_ADB_SERVER_PORT ?? '5037',
      },
    });
    closeSync(logFd);
    launch = { logPath, exit: null, emulator };
    emulator.once('error', (error) => { launch.exit = { code: null, signal: null, error: error.message }; });
    emulator.once('exit', (code, signal) => { launch.exit = { code, signal }; });
    emulator.unref();
    runnerOwnedAndroidProcesses.set(avdName, { emulator, launch });
    startedBySupervisor = true;
    return waitForAndroidAvd(sdk, avdName, (discoveredSerial) => {
      onRuntimeStarted({ platform: 'Android', avdName, serial: discoveredSerial });
    }, 180_000, launch);
  };

  for (let recovery = 0; recovery < 2; recovery += 1) {
    try {
      if (serial) serial = await waitForAndroidAvd(sdk, avdName);
      else serial = await launchAndroid();
      break;
    } catch (error) {
      const matchingQemuPids = startedBySupervisor ? await findAvdQemuPids(avdName) : [];
      if (shouldColdBootOwnedOfflineAndroidAvd({
        error,
        recoveryAttempt: recovery,
        startedBySupervisor,
        offlineSerials: error.offlineSerials,
        qemuPids: matchingQemuPids,
        launchPid: launch?.emulator?.pid,
      })) {
        const qemuPid = launch.emulator.pid;
        process.stdout.write(`♻️ Android ADB stayed offline for ${avdName}; stopping only its runner-owned QEMU process ${qemuPid} and cold-booting without snapshot or data wipe\n`);
        try {
          process.kill(qemuPid, 'SIGTERM');
        } catch (killError) {
          if (killError.code !== 'ESRCH') throw new Error(`Could not stop exact runner-owned Android QEMU ${qemuPid}: ${killError.message}`);
        }
        const stopDeadline = Date.now() + 30_000;
        while (Date.now() < stopDeadline) {
          const remaining = await findAvdQemuPids(avdName);
          if (!remaining.includes(qemuPid)) break;
          await delay(500);
        }
        if ((await findAvdQemuPids(avdName)).includes(qemuPid)) {
          throw new Error(`Runner-owned Android QEMU ${qemuPid} did not stop within 30 seconds`);
        }
        const recoveryLog = resolve(runDirectory, 'platforms', `android-${avdName}-recovery-test-${String(testNumber).padStart(2, '0')}-attempt-${attempt}.log`);
        writeFileSync(recoveryLog, `${new Date().toISOString()} stopped exact runner-owned QEMU pid=${qemuPid} after ADB remained offline; retrying without snapshot/data wipe\n`, { flag: 'a', mode: 0o600 });
        runnerOwnedAndroidProcesses.delete(avdName);
        forceColdBoot = true;
        serial = null;
        launch = null;
        continue;
      }
      if (!shouldRestartAndroidAvdForReadinessFailure(error, recovery)) throw error;
      process.stdout.write(`⚠️ Android ADB transport is online but ${error.readinessProbe} is unresponsive; restarting exact AVD ${avdName}\n`);
      await restartUnresponsiveAvd(sdk, avdName, serial ?? error.serial, runDirectory, testNumber, attempt, error.readinessProbe);
      runnerOwnedAndroidProcesses.delete(avdName);
      serial = null;
      launch = null;
      forceColdBoot = true;
    }
  }
  return { platform: 'Android', avdName, serial, state: 'Booted', sdkRoot: sdk.sdkRoot, adbServer, startedBySupervisor, emulatorLogPath: launch?.logPath ?? null };
}

function iosRuntimeHealthArgs(udid) {
  if (!udid) throw new Error('iOS runtime heartbeat requires the selected simulator UDID');
  return ['simctl', 'spawn', udid, 'launchctl', 'managerpid'];
}

function iosRuntimeManagerPid(stdout) {
  const value = stdout.trim();
  const pid = Number(value);
  if (!/^[0-9]+$/.test(value) || !Number.isSafeInteger(pid) || pid <= 1) {
    throw new Error('iOS runtime heartbeat failed: invalid guest launchd manager PID');
  }
  return pid;
}

export async function verifyIosRuntimeHealth({ udid, runCommand = command }) {
  // Runtime resolution is required: absolute host paths are not executable
  // guest paths on the selected iOS runtime. Verified Booted/shutdown behavior
  // is recorded in ios-guest-health-executable-verification.json.
  const { stdout } = await runCommand('xcrun', iosRuntimeHealthArgs(udid), { timeout: 15_000 });
  return { guestManagerPid: iosRuntimeManagerPid(stdout) };
}

export async function prepareIos(simulatorName, onRuntimeStarted, {
  runCommand = command,
  openDeviceUi = openVisibleIosDeviceUi,
} = {}) {
  const { stdout } = await runCommand('xcrun', ['simctl', 'list', 'devices', 'available', '-j']);
  const devices = Object.values(JSON.parse(stdout).devices).flat();
  const simulator = devices.find((device) => device.name === simulatorName);
  if (!simulator) throw new Error(`iOS simulator ${simulatorName} is not installed`);
  const startedBySupervisor = simulator.state !== 'Booted';
  if (simulator.state !== 'Booted') {
    process.stdout.write(`📱 Starting visible iOS simulator ${simulator.name}\n`);
    await runCommand('xcrun', ['simctl', 'boot', simulator.udid]);
    onRuntimeStarted({ platform: 'iOS', simulatorName, udid: simulator.udid });
  }
  await runCommand('xcrun', ['simctl', 'bootstatus', simulator.udid, '-b'], { timeout: 300_000 });
  await verifyIosRuntimeHealth({ udid: simulator.udid, runCommand });
  const runAndCapture = async (file, args, options) => runCommand(file, args, options);
  const runAndWait = async (file, args, options) => runCommand(file, args, options);
  const ui = await openDeviceUi({ runAndCapture, runAndWait, simulatorUdid: simulator.udid });
  return { platform: 'iOS', simulatorName, udid: simulator.udid, state: 'Booted', visibleUi: ui, startedBySupervisor };
}

export async function prepareScenarioPlatforms({ scenario, testNumber, attempt, runId, runDirectory, onRuntimeStarted = () => {} }) {
  const environment = verifyLocalEnvironment(scenario);
  const platforms = [];
  if (scenario.android?.avdName) {
    const platform = await prepareAndroid(scenario.android.avdName, onRuntimeStarted, runDirectory, testNumber, attempt);
    platforms.push(platform);
  }
  if (scenario.ios?.simulatorName) {
    const platform = await prepareIos(scenario.ios.simulatorName, onRuntimeStarted);
    platforms.push(platform);
  }
  const directory = resolve(runDirectory, 'readiness');
  mkdirSync(directory, { recursive: true });
  const reportPath = resolve(directory, `test-${String(testNumber).padStart(2, '0')}-attempt-${attempt}.json`);
  const report = {
    test: testNumber,
    runId,
    scenario: scenario.name ?? `UI E2E Test ${testNumber}`,
    checkedAt: new Date().toISOString(),
    environment,
    platforms,
    status: platforms.length === 0 ? 'ready' : 'runtime-ready',
    note: 'App and test-runner installation is verified by the scenario orchestrator before its first business mutation.',
  };
  writeFileSync(reportPath, `${JSON.stringify(report, null, 2)}\n`, { mode: 0o600, flag: 'wx' });
  return { platforms, reportPath };
}

export async function stopRunnerOwnedRuntimes(runtimes, onEvent = () => {}) {
  const failures = [];
  for (const runtime of runtimes) {
    try {
      if (runtime.platform === 'Android') {
        const sdk = discoverAndroidSdk();
        await command(sdk.adbPath, ['-s', runtime.serial, 'emu', 'kill']);
      } else if (runtime.platform === 'iOS') {
        await command('xcrun', ['simctl', 'shutdown', runtime.udid]);
      }
      onEvent(`stopped runner-owned ${runtime.platform} runtime ${runtime.avdName ?? runtime.simulatorName}`);
    } catch (error) {
      failures.push(`${runtime.platform} ${runtime.avdName ?? runtime.simulatorName}: ${error.message}`);
    }
  }
  if (failures.length > 0) throw new Error(`Could not stop runner-owned runtimes: ${failures.join('; ')}`);
}

function verifyLocalEnvironment(scenario) {
  const composeRoot = resolve(workspaceRoot, 'meta-secret-compose');
  const selectors = [
    {
      label: 'Core',
      path: resolve(coreRoot, 'meta-secret/core/src/node/app/sync/environment.rs'),
      mode: (text) => text.match(/SELECTED_SERVER_ENVIRONMENT[^\n]*ServerEnvironment::(Local|Remote)/)?.[1]?.toLowerCase(),
    },
    {
      label: 'Web',
      path: resolve(coreRoot, 'meta-secret/web-cli/ui/.env.local'),
      mode: (text) => text.match(/^VITE_META_SECRET_ENV=(local|remote)$/m)?.[1],
    },
    {
      label: 'Android',
      path: resolve(composeRoot, 'gradle.properties'),
      mode: (text) => text.match(/^META_SECRET_ENV=(local|remote)$/m)?.[1],
    },
    {
      label: 'iOS',
      path: resolve(composeRoot, 'iosApp/Configuration/Config.xcconfig'),
      mode: (text) => text.match(/^META_SECRET_ENV=(local|remote)$/m)?.[1],
    },
  ];
  const statuses = selectors.map((selector) => {
    if (!existsSync(selector.path)) throw new Error(`Local environment selector is missing (${selector.label}): ${selector.path}`);
    const mode = selector.mode(readFileSync(selector.path, 'utf8'));
    if (mode !== 'local') throw new Error(`Refusing E2E on non-local ${selector.label} selector (${mode ?? 'unknown'}); run task set-env -- local through run_ui_e2e first`);
    return { component: selector.label, mode, path: selector.path };
  });
  for (const [label, endpoint] of [['server', scenario.server?.url], ['web', scenario.web?.url]]) {
    if (!endpoint) continue;
    const hostname = new URL(endpoint).hostname;
    if (!['localhost', '127.0.0.1', '::1'].includes(hostname)) {
      throw new Error(`Refusing E2E with non-local ${label} endpoint: ${hostname}`);
    }
  }
  return { mode: 'local', selectors: statuses, verifiedAt: new Date().toISOString() };
}

export function updateReadinessReport({ platform, checks, requiredNonPlatformParticipants }) {
  const reportPath = process.env.E2E_READINESS_PATH;
  if (!reportPath) return;
  const report = JSON.parse(readFileSync(reportPath, 'utf8'));
  if (requiredNonPlatformParticipants) {
    report.requiredNonPlatformParticipants = [...new Set(requiredNonPlatformParticipants)];
  }
  const platformReport = report.platforms.find((entry) => entry.platform === platform);
  if (platformReport) platformReport.installation = checks;
  else {
    report.nonPlatformParticipants ??= [];
    const participant = report.nonPlatformParticipants.find((entry) => entry.participant === platform);
    if (participant) participant.checks = checks;
    else report.nonPlatformParticipants.push({ participant: platform, checks });
  }
  const nativePlatformsReady = report.platforms.every((entry) => entry.installation);
  const nonPlatformParticipantsReady = (report.requiredNonPlatformParticipants ?? []).every((required) =>
    (report.nonPlatformParticipants ?? []).some((entry) => entry.participant === required && entry.checks),
  );
  if (nativePlatformsReady && nonPlatformParticipantsReady) {
    report.status = 'ready';
    report.appsReadyAt = new Date().toISOString();
  }
  report.updatedAt = new Date().toISOString();
  const temporary = `${reportPath}.${process.pid}.tmp`;
  writeFileSync(temporary, `${JSON.stringify(report, null, 2)}\n`, { mode: 0o600, flag: 'wx' });
  renameSync(temporary, reportPath);
}

export async function checkPlatform(platform, {
  runCommand = command,
  androidSdk = discoverAndroidSdk,
  findAndroidAvd = findOnlineAndroidAvd,
  ownedAndroidProcesses = runnerOwnedAndroidProcesses,
} = {}) {
  if (platform.platform === 'Android') {
    const ownedProcess = ownedAndroidProcesses.get(platform.avdName);
    if (ownedProcess?.launch.exit) {
      const output = existsSync(ownedProcess.launch.logPath)
        ? readFileSync(ownedProcess.launch.logPath, 'utf8').slice(-12_000)
        : '';
      throw new Error(
        `Android Emulator process exited (code=${ownedProcess.launch.exit.code ?? 'none'}, signal=${ownedProcess.launch.exit.signal ?? 'none'}); `
          + `log=${ownedProcess.launch.logPath}${output ? `\n${output}` : ''}`,
      );
    }
    const sdk = androidSdk();
    const serial = await findAndroidAvd(sdk, platform.avdName, (file, args) => runCommand(file, args, { timeout: 10_000 }));
    if (!serial || serial !== platform.serial) throw new Error(`Android AVD ${platform.avdName} is no longer online`);
    const { stdout } = await runCommand(sdk.adbPath, ['-s', serial, 'shell', 'getprop', 'sys.boot_completed'], { timeout: 10_000 });
    if (stdout.trim() !== '1') throw new Error(`Android AVD ${platform.avdName} lost boot readiness`);
    // F3 verifies Binder/package services before reset/install. During F4,
    // inventory dumps compete with active instrumentation and are not an app
    // health signal. Keep transport/identity/boot plus a bounded shell heartbeat;
    // the scenario's actor result, exact markers and crash checks judge the app.
    const heartbeat = await runCommand(sdk.adbPath, ['-s', serial, 'shell', 'echo', 'E2E_PLATFORM_PROBE'], { timeout: 10_000 });
    if (heartbeat.stdout.trim() !== 'E2E_PLATFORM_PROBE') {
      throw new Error(`Android AVD ${platform.avdName} shell heartbeat failed`);
    }
  } else if (platform.platform === 'iOS') {
    // F3 resolves inventory and verifies Booted/install state. F4 asks the
    // selected running guest to execute a harmless command, avoiding global
    // CoreSimulator discovery while XCTest is actively using this guest.
    await verifyIosRuntimeHealth({ udid: platform.udid, runCommand });
  }
}

export async function capturePlatformProbeDiagnostics(platform, onEvent, runCommand = command) {
  // OS-only evidence: never collect package names, application state, or payloads.
  const probes = [
    { id: 'host-memory', file: 'memory_pressure', args: ['-Q'], summary: (out) => out.match(/System-wide memory free percentage:.*$/m)?.[0] ?? 'unavailable' },
  ];
  if (platform.platform === 'Android') {
    const { adbPath } = discoverAndroidSdk();
    const args = (remote) => ['-s', platform.serial, 'shell', ...remote];
    probes.push(
      { id: 'shell-transport', file: adbPath, args: args(['echo', 'E2E_PLATFORM_PROBE']), summary: (out) => out.trim() === 'E2E_PLATFORM_PROBE' ? 'responsive' : 'unexpected response' },
      { id: 'binder-services', file: adbPath, args: args(['service', 'list']), summary: (out) => out.match(/^Found\s+\d+\s+services:?\s*$/m)?.[0] ?? 'no service inventory' },
      { id: 'package-manager', file: adbPath, args: args(['cmd', 'package', 'list', 'packages', '--user', '0']), summary: (out) => `${out.split(/\r?\n/).filter((line) => line.startsWith('package:')).length} package rows` },
    );
    const owned = runnerOwnedAndroidProcesses.get(platform.avdName);
    if (owned?.emulator?.pid) probes.push({ id: 'owned-qemu-resource', file: 'ps', args: ['-p', String(owned.emulator.pid), '-o', 'pid=,etime=,%cpu=,%mem=,state='], summary: (out) => out.trim() });
  } else if (platform.platform === 'iOS') {
    probes.push(
      { id: 'selected-runtime-inventory', file: 'xcrun', args: ['simctl', 'list', 'devices', platform.udid, '-j'], summary: (out) => {
        const device = Object.values(JSON.parse(out).devices).flat().find((entry) => entry.udid === platform.udid);
        return device ? `selected runtime state=${device.state}` : 'selected runtime absent';
      } },
      { id: 'selected-runtime-process', file: 'xcrun', args: iosRuntimeHealthArgs(platform.udid), summary: (out) => `guest launchd manager PID=${iosRuntimeManagerPid(out)}` },
    );
  }
  await Promise.all(probes.map(async (probe) => {
    const started = Date.now();
    try {
      const { stdout } = await runCommand(probe.file, probe.args, { timeout: 3_000 });
      onEvent(`platform probe diagnostic ${platform.platform} ${probe.id}: elapsedMs=${Date.now() - started}; ${probe.summary(stdout)}`);
    } catch (error) {
      onEvent(`platform probe diagnostic ${platform.platform} ${probe.id}: elapsedMs=${Date.now() - started}; code=${error.code ?? 'unknown'}; killed=${Boolean(error.killed)}`);
    }
  }));
}

async function checkPlatformWithRetry(platform, onRetry, attempts = 3) {
  let lastError;
  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    try {
      await checkPlatform(platform);
      return;
    } catch (error) {
      lastError = error;
      if (attempt === 1) {
        // Bounded diagnostics run independently; they do not extend readiness
        // deadlines or change the failure/retry decision.
        void capturePlatformProbeDiagnostics(platform, onRetry).catch(() => {});
      }
      if (attempt < attempts) {
        onRetry(`${platform.platform} liveness command failed (${attempt}/${attempts}); retrying: ${error.message}`);
        await delay(1_000);
      }
    }
  }
  throw lastError;
}

export function startPlatformLivenessMonitor({
  platforms,
  testNumber,
  onFailure,
  onEvent,
  intervalMs = 15_000,
  unavailableGraceMs = 0,
}) {
  if (platforms.length === 0) return () => {};
  let active = true;
  let checking = false;
  const loopDelay = monitorEventLoopDelay({ resolution: 20 });
  loopDelay.enable();
  const unavailableSince = new Map();
  const timer = setInterval(async () => {
    if (!active || checking) return;
    checking = true;
    try {
      const now = Date.now();
      const results = await Promise.all(platforms.map(async (platform) => {
        try {
          await checkPlatformWithRetry(platform, (message) => {
            onEvent(message);
            if (message.includes('liveness command failed')) {
              onEvent(`platform probe parent event-loop delay: maxMs=${Math.round(loopDelay.max / 1e6)}; meanMs=${Number.isFinite(loopDelay.mean) ? Math.round(loopDelay.mean / 1e6) : 'unavailable'}`);
            }
          });
          unavailableSince.delete(platform.platform);
          return { platform, error: null };
        } catch (error) {
          const since = unavailableSince.get(platform.platform) ?? now;
          unavailableSince.set(platform.platform, since);
          return { platform, error, since };
        }
      }));

      const terminalFailure = results.find(({ error, since }) =>
        error && Date.now() - since >= unavailableGraceMs,
      );
      if (terminalFailure) {
        active = false;
        onEvent(`platform liveness failure for TEST #${testNumber}: ${terminalFailure.error.message}`);
        onFailure(terminalFailure.error);
        return;
      }

      const transitioning = results.filter(({ error }) => error);
      if (transitioning.length > 0) {
        onEvent(
          `platform transition in TEST #${testNumber}; allowing bounded recovery: `
            + transitioning.map(({ platform, error, since }) =>
              `${platform.platform} unavailable ${Math.round((Date.now() - since) / 1_000)}s (${error.message})`,
            ).join('; '),
        );
      } else {
        onEvent(`platform liveness check passed for TEST #${testNumber}`);
      }
    } finally {
      checking = false;
      loopDelay.reset();
    }
  }, intervalMs);
  timer.unref?.();
  return () => {
    active = false;
    clearInterval(timer);
    loopDelay.disable();
  };
}
