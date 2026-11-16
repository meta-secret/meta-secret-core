import { createRunOwnedAndroidProfile } from './android-owned-profile.mjs';
import { verifyOwnedAndroidRuntime, verifyOwnedAndroidScenarioRuntime, verifyOwnedAndroidResetTarget, resolveAndroidScenarioAvdName } from './android-owned-handoff.mjs';
export { createRunOwnedAndroidProfile, verifyOwnedAndroidScenarioRuntime, verifyOwnedAndroidResetTarget, resolveAndroidScenarioAvdName };
import { createScenarioRuntimeProfile, coreHttpProofsReady } from './scenario-runtime-profile.mjs';
import { spawn } from 'node:child_process';
import { closeSync, existsSync, mkdirSync, openSync, readFileSync, renameSync, writeFileSync, lstatSync, realpathSync } from 'node:fs';
import { promisify } from 'node:util';
import { execFile } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';
import { monitorEventLoopDelay } from 'node:perf_hooks';
import { randomUUID, createHash } from 'node:crypto';
import { homedir } from 'node:os';
import { discoverAndroidSdk, findOnlineAndroidAvd, withAndroidSdkPath } from './android-sdk.mjs';
import { openVisibleIosDeviceUi } from './visible-ios-ui.mjs';

const execFileAsync = promisify(execFile);
const e2eRoot = resolve(fileURLToPath(new URL('..', import.meta.url)));
const coreRoot = resolve(e2eRoot, '..');
const workspaceRoot = resolve(coreRoot, '..');
const runnerOwnedAndroidProcesses = new Map();
const runnerOwnedAndroidRuntimeObjects = new WeakMap();
const runnerOwnedIosRuntimeObjects = new WeakMap();
const runnerOwnedIosBoots = new Map();

export const EXISTING_IOS_TARGETS = Object.freeze({
  'iPhone 17': Object.freeze({ simulatorName: 'iPhone 17', udid: 'DB22263E-A8A3-49C5-ADD1-BAE4089E2E30',
    runtimeIdentifier: 'com.apple.CoreSimulator.SimRuntime.iOS-26-5', deviceTypeIdentifier: 'com.apple.CoreSimulator.SimDeviceType.iPhone-17' }),
  'iPhone 17 Pro Max': Object.freeze({ simulatorName: 'iPhone 17 Pro Max', udid: '60BEF44A-3487-4AD4-B7D8-AA18CCF41368',
    runtimeIdentifier: 'com.apple.CoreSimulator.SimRuntime.iOS-27-0', deviceTypeIdentifier: 'com.apple.CoreSimulator.SimDeviceType.iPhone-17-pro-max' }),
});
export const EXISTING_IOS_TARGET = EXISTING_IOS_TARGETS['iPhone 17'];

export function assertExistingDeviceRun({ scenarios = [], manifest = {}, readiness = {}, environment = process.env } = {}) {
  if (manifest.runtimeOwnership !== undefined && (!Array.isArray(manifest.runtimeOwnership) || manifest.runtimeOwnership.length !== 0)) throw new Error('Persisted runtime ownership is forbidden; only this process may record a newly started runtime');
  if (Object.keys(environment).some(key => /^E2E_(?:ANDROID|IOS)_OWNED_/.test(key))) throw new Error('New owned device profiles are forbidden; reuse existing devices');
  for (const key of ['ANDROID_AVD_HOME', 'ANDROID_USER_HOME', 'ANDROID_EMULATOR_HOME']) {
    const expected = resolve(homedir(), key === 'ANDROID_AVD_HOME' ? '.android/avd' : '.android');
    if (environment[key] !== undefined && resolve(environment[key]) !== expected) throw new Error('Foreign/private Android home is forbidden for existing-device reuse');
  }
  const forbidden = new Set(['ownedIosProfile', 'ownedAndroidProfile', 'ownedAndroidLease', 'iosProfileCreationIntent']);
  const pending = [manifest, readiness, ...scenarios];
  while (pending.length) {
    const item = pending.pop();
    if (!item || typeof item !== 'object') continue;
    if (Object.keys(item).some(key => forbidden.has(key))) throw new Error('Legacy owned profile handoff/resume is forbidden before mutation or cleanup');
    pending.push(...Object.values(item));
  }
  for (const scenario of scenarios) {
    const allowedIosName = scenario.testNumber === 50 ? new Set(Object.keys(EXISTING_IOS_TARGETS)) : new Set([EXISTING_IOS_TARGET.simulatorName]);
    if (scenario.ios?.simulatorName && !allowedIosName.has(scenario.ios.simulatorName)) throw new Error('Only the declared existing iOS target is admitted');
    if (scenario.android?.avdName && scenario.android.avdName !== 'Pixel_4a') throw new Error('Only the existing Pixel_4a AVD is admitted');
  }
  for (const platform of readiness.platforms ?? []) {
    if (platform.platform === 'iOS' && !Object.values(EXISTING_IOS_TARGETS).some(target => target.udid === platform.udid)) throw new Error('Borrowed iOS reset/readiness UUID mismatch');
    if (platform.platform === 'Android' && platform.avdName !== 'Pixel_4a') throw new Error('Borrowed Android reset/readiness target mismatch');
  }
}

export function iosSerialTestArguments(udid) {
  if (!Object.values(EXISTING_IOS_TARGETS).some(target => target.udid === udid)) throw new Error('XCTest requires the exact existing destination');
  return ['-destination', `id=${udid}`, '-parallel-testing-enabled', 'NO',
    '-maximum-parallel-testing-workers', '1', '-maximum-concurrent-test-simulator-destinations', '1'];
}

export function readExistingAndroidAvd(avdName, { homeDirectory = homedir() } = {}) {
  if (avdName !== 'Pixel_4a') throw new Error('Only the existing Pixel_4a AVD is admitted');
  const root = resolve(homeDirectory, '.android/avd'), registration = resolve(root, 'Pixel_4a.ini'), dataPath = resolve(root, 'Pixel_4a.avd');
  const regular = name => { if (lstatSync(name).isSymbolicLink() || !lstatSync(name).isFile() || realpathSync(name) !== name) throw new Error('Existing AVD config must be a canonical regular file'); };
  regular(registration);
  const paths = readFileSync(registration, 'utf8').split(/\r?\n/).filter(line => line.startsWith('path='));
  if (paths.length !== 1 || paths[0].slice(5) !== dataPath || lstatSync(dataPath).isSymbolicLink() || !lstatSync(dataPath).isDirectory() || realpathSync(dataPath) !== dataPath) throw new Error('Existing registered AVD data path mismatch');
  const configPath = resolve(dataPath, 'config.ini'); regular(configPath);
  const config = readFileSync(configPath), ids = config.toString('utf8').split(/\r?\n/).filter(line => /^AvdId\s*=/.test(line));
  if (ids.length !== 1 || ids[0].split('=')[1].trim() !== avdName) throw new Error('Existing AVD config identity mismatch');
  return { avdName, registration, dataPath, configPath, configSha256: createHash('sha256').update(config).digest('hex') };
}

export function selectExistingIosSimulator(simulatorName, inventory, targetUdid) {
  const target = EXISTING_IOS_TARGETS[simulatorName];
  if (!target || (targetUdid !== undefined && targetUdid !== target.udid)) throw new Error('Existing iOS target declaration mismatch');
  targetUdid ??= target.udid;
  const device = selectIosSimulator({ simulatorName, inventory, targetUdid });
  if (device.runtimeIdentifier !== target.runtimeIdentifier || device.deviceTypeIdentifier !== target.deviceTypeIdentifier || device.isAvailable !== true) throw new Error('Existing iOS target runtime/type/availability mismatch');
  return device;
}
async function command(file, args, options = {}) {
  return execFileAsync(file, args, {
    cwd: options.cwd,
    env: { ...process.env, ...options.env },
    timeout: options.timeout ?? 60_000,
    maxBuffer: 16 * 1024 * 1024,
  });
}

function createAndroidReadinessDiagnostics(onDiagnostic) {
  if (typeof onDiagnostic !== 'function') return () => {};
  let count = 0;
  let probeStartedAt;
  return (event, facts = {}) => {
    try {
      if (count > 12) return;
      if (!['PROBE_ENTER', 'PROBE_PASS', 'PROBE_FAIL', 'CHILD_ERROR', 'CHILD_EXIT', 'CHILD_CLOSE',
        'DRAIN_ENTER', 'DRAIN_CLOSED', 'DRAIN_UNJOINED'].includes(event)) return;
      if (count++ === 12) { event = 'CAPPED'; facts = {}; }
      const now = Date.now();
      if (event === 'PROBE_ENTER') probeStartedAt = now;
      const elapsed = ['PROBE_PASS', 'PROBE_FAIL'].includes(event) ? now - probeStartedAt : null;
      const record = {
        event, at: new Date(now).toISOString(),
        probe: ['shell-services', 'package-manager'].includes(facts.probe) ? facts.probe : null,
        matched: typeof facts.matched === 'boolean' ? facts.matched : null,
        elapsedMs: Number.isSafeInteger(elapsed) && elapsed >= 0 ? elapsed : null,
        cause: facts.cause === undefined ? null : ['ETIMEDOUT', 'INVALID_PROBE_OUTPUT',
          'ERR_CHILD_PROCESS_STDIO_MAXBUFFER'].includes(facts.cause) ? facts.cause : 'OTHER',
        killed: typeof facts.killed === 'boolean' ? facts.killed : null,
        exitCode: Number.isSafeInteger(facts.exitCode) ? facts.exitCode : null,
        signal: facts.signal == null ? null : ['SIGTERM', 'SIGKILL', 'SIGINT', 'SIGHUP',
          'SIGABRT', 'SIGBUS', 'SIGSEGV', 'SIGILL', 'SIGQUIT'].includes(facts.signal) ? facts.signal : 'OTHER',
        closed: typeof facts.closed === 'boolean' ? facts.closed : null,
      };
      const line = `ANDROID_READINESS_DIAG ${JSON.stringify(record)}`;
      if (Buffer.byteLength(line) <= 512) onDiagnostic(line);
    } catch { /* Diagnostics never replace readiness or cleanup results. */ }
  };
}

export async function verifyAndroidInstallReadiness({ adbPath, serial, runCommand = command, timeoutMs = 10_000,
  diagnostic = () => {} }) {
  const emit = (event, facts) => { try { diagnostic(event, facts); } catch { /* Observer only. */ } };
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
    let matched = null;
    emit('PROBE_ENTER', { probe: probe.id });
    try {
      const { stdout } = await runCommand(adbPath, probe.args, { timeout: timeoutMs });
      matched = probe.isReady(stdout);
      if (!matched) {
        throw Object.assign(new Error('probe returned no expected inventory'), { code: 'INVALID_PROBE_OUTPUT' });
      }
      emit('PROBE_PASS', { probe: probe.id, matched });
    } catch (cause) {
      const code = cause.code;
      const killed = code === 'ETIMEDOUT' ? null : cause.killed;
      const timedOut = code === 'ETIMEDOUT' || killed;
      const error = new Error(
        `Android ADB transport is online, but the ${probe.id} probe ${timedOut ? 'timed out' : 'failed'} for ${serial}`,
        { cause },
      );
      error.code = probe.code;
      error.readinessProbe = probe.id;
      emit('PROBE_FAIL', { probe: probe.id, matched, cause: code ?? 'OTHER', killed });
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
          await verifyAndroidInstallReadiness({ adbPath: sdk.adbPath, serial, diagnostic: launch?.diagnostic });
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

async function ensureAndroidAdbServer(sdk, runCommand = command) {
  const { stdout: versionOutput } = await runCommand(sdk.adbPath, ['version'], { timeout: 10_000 });
  await runCommand(sdk.adbPath, ['start-server'], { timeout: 15_000 });
  const { stdout: deviceOutput } = await runCommand(sdk.adbPath, ['devices'], { timeout: 10_000 });
  return {
    adbPath: sdk.adbPath,
    version: versionOutput.split(/\r?\n/).find((line) => /Version\s/i.test(line))?.trim() ?? 'unknown',
    serverAvailable: true,
    devicesAtCheck: deviceOutput.split(/\r?\n/).slice(1).map((line) => line.trim()).filter(Boolean),
  };
}

async function findAvdQemuPids(avdName, runCommand = command) {
  const args = process.platform === 'darwin' ? ['-axo', 'pid=,command='] : ['-eo', 'pid=,args='];
  const { stdout } = await runCommand('ps', args, { timeout: 10_000 });
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

function observeAndroidLaunch(emulator, logPath, diagnostic = () => {}) {
  const launch = { logPath, exit: null, emulator, closed: false, diagnostic };
  launch.closePromise = new Promise((resolveClose) => {
    emulator.once('close', (code, signal) => {
      launch.closed = true;
      diagnostic('CHILD_CLOSE', { exitCode: code, signal, closed: true });
      resolveClose();
    });
  });
  emulator.once('error', (error) => {
    launch.exit = { code: null, signal: null, error: error.message };
    diagnostic('CHILD_ERROR');
  });
  emulator.once('exit', (code, signal) => {
    launch.exit = { code, signal };
    diagnostic('CHILD_EXIT', { exitCode: code, signal, closed: launch.closed });
  });
  return launch;
}

async function waitForAndroidLaunchClose(launch, timeoutMs) {
  if (launch.closed) return true;
  let timer;
  try {
    return await Promise.race([
      launch.closePromise.then(() => true),
      new Promise((resolveTimeout) => { timer = setTimeout(() => resolveTimeout(false), timeoutMs); }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}

async function drainFailedAndroidLaunch(avdName, launch) {
  if (!launch) return;
  launch.diagnostic('DRAIN_ENTER', { closed: launch.closed });
  const errors = [];
  for (const [signal, timeoutMs] of [['SIGTERM', 5_000], ['SIGKILL', 2_000]]) {
    if (launch.closed) break;
    try { launch.emulator.kill(signal); } catch (error) { errors.push(error); }
    if (await waitForAndroidLaunchClose(launch, timeoutMs)) break;
  }
  if (!launch.closed) errors.push(new Error('Runner-owned Android launch did not close after TERM/KILL'));
  if (launch.closed && runnerOwnedAndroidProcesses.get(avdName)?.launch === launch) {
    runnerOwnedAndroidProcesses.delete(avdName);
  }
  launch.diagnostic(launch.closed ? 'DRAIN_CLOSED' : 'DRAIN_UNJOINED', { closed: launch.closed });
  if (errors.length) throw new AggregateError(errors, 'Runner-owned Android launch cleanup failed');
}

export async function prepareAndroid(avdName, onRuntimeStarted, { runDirectory, testNumber, attempt, ownedAndroidLease, runId, onDiagnostic }, {
  androidSdk = discoverAndroidSdk, runCommand = command, spawnProcess = spawn, waitForAvd = waitForAndroidAvd, existingAvd = readExistingAndroidAvd,
} = {}) {
  if (ownedAndroidLease) return prepareOwnedAndroid(avdName, onRuntimeStarted, { runDirectory, testNumber, attempt, ownedAndroidLease, runId }, { androidSdk, runCommand, spawnProcess, waitForAvd });
  assertExistingDeviceRun({ scenarios: [{ android: { avdName } }] });
  const registeredAvd = existingAvd(avdName);
  const sdk = androidSdk();
  const adbServer = await ensureAndroidAdbServer(sdk, runCommand);
  process.stdout.write(`🔌 Android ADB server ready: ${adbServer.version} (${adbServer.adbPath})\n`);
  const { stdout } = await runCommand(sdk.emulatorPath, ['-list-avds']);
  const available = stdout.split(/\r?\n/).map((value) => value.trim()).filter(Boolean);
  if (!available.includes(avdName)) {
    throw new Error(`Android AVD ${avdName} is not installed; available: ${available.join(', ') || '<none>'}`);
  }
  let serial = null;
  let startedBySupervisor = false;
  let launch = null;
  let forceColdBoot = false;
  const diagnostic = createAndroidReadinessDiagnostics(onDiagnostic);
  serial = await findOnlineAndroidAvd(sdk, avdName, (file, args) => runCommand(file, args, { timeout: 10_000 }));
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
    const emulator = spawnProcess(sdk.emulatorPath, androidEmulatorLaunchArgs(avdName, { coldBoot: forceColdBoot }), {
      detached: true,
      stdio: ['ignore', logFd, logFd],
      env: {
        ...process.env,
        PATH: withAndroidSdkPath(process.env.PATH, sdk),
        ANDROID_ADB_SERVER_PORT: process.env.ANDROID_ADB_SERVER_PORT ?? '5037',
      },
    });
    closeSync(logFd);
    launch = observeAndroidLaunch(emulator, logPath, diagnostic);
    emulator.unref();
    runnerOwnedAndroidProcesses.set(avdName, { emulator, launch });
    startedBySupervisor = true;
    let runtime;
    return waitForAvd(sdk, avdName, (discoveredSerial) => {
      if (!runtime || runtime.serial !== discoveredSerial) {
        runtime = { platform: 'Android', avdName, serial: discoveredSerial };
        runnerOwnedAndroidRuntimeObjects.set(runtime, { launch, avdName, serial: discoveredSerial,
          registeredAvd: Object.freeze({ ...registeredAvd }) });
      }
      onRuntimeStarted(runtime);
    }, 180_000, launch);
  };

  try {
    // One existing-device start only; failure drains our child without a replacement launch.
    if (serial) serial = await waitForAvd(sdk, avdName);
    else serial = await launchAndroid();
  } catch (error) {
    try {
      await drainFailedAndroidLaunch(avdName, launch);
    } catch (cleanupError) {
      throw new AggregateError([error, cleanupError], 'Android preparation failed and its owned launch could not drain', { cause: error });
    }
    throw error;
  }
  return { platform: 'Android', avdName, registeredAvd, serial, state: 'Booted', sdkRoot: sdk.sdkRoot, adbServer, startedBySupervisor, emulatorLogPath: launch?.logPath ?? null };
}

async function stopOwnedAndroid(lease, { runCommand = command } = {}) {
  if (lease?.removed) return;
  if (typeof lease?.verify !== 'function' || typeof lease?.remove !== 'function') throw new Error('Owned Android lease cannot be adopted from metadata');
  const name = lease.intent.physicalAvdName;
  const current = runnerOwnedAndroidProcesses.get(name);
  if (current && current.lease !== lease) throw new Error('Owned Android launch lease changed');
  if (current) await drainFailedAndroidLaunch(name, current.launch);
  if ((await findAvdQemuPids(name, runCommand)).length) throw new Error('Owned Android QEMU still writes; retaining private root');
  const replacement = runnerOwnedAndroidProcesses.get(name);
  if (replacement && (replacement.lease !== lease || !replacement.launch.closed)) throw new Error('Owned Android launch changed during close; retaining private root');
  lease.remove();
}

async function prepareOwnedAndroid(avdName, onRuntimeStarted, scope, ports) {
  const { ownedAndroidLease: lease, runDirectory, testNumber, attempt, runId } = scope;
  if (typeof lease?.verify !== 'function') throw new Error('Owned Android lease is not current-process authority');
  const profile = lease.verify({ avdName, runId });
  if (!runId || profile.runId !== runId) throw new Error('Owned Android run scope mismatch');
  const name = profile.physicalAvdName, sdk = ports.androidSdk();
  if (profile.sdkRoot !== sdk.sdkRoot) throw new Error('Owned Android SDK changed');
  const localCommand = (file, args, options = {}) => ports.runCommand(file, args, { ...options, env: { ...options.env, ...profile.privateEnv } });
  const adbServer = await ensureAndroidAdbServer(sdk, localCommand);
  const available = (await localCommand(sdk.emulatorPath, ['-list-avds'])).stdout.split(/\r?\n/).map(value => value.trim()).filter(Boolean);
  if (available.length !== 1 || available[0] !== name) throw new Error('Private Android AVD inventory is ambiguous');
  let current = runnerOwnedAndroidProcesses.get(name);
  const pids = await findAvdQemuPids(name, localCommand);
  const serial = await findOnlineAndroidAvd(sdk, name, localCommand);
  if (current) {
    if (current.lease !== lease || current.launch.closed || current.launch.exit
        || pids.length !== 1 || pids[0] !== current.emulator.pid || serial !== current.serial) {
      throw new Error('Owned Android lease is lost; refusing borrowed restart');
    }
    await ports.waitForAvd(sdk, name);
    const platform = { ...current.platform, adbServer };
    await verifyOwnedAndroidRuntime(platform, { runId: profile.runId, runCommand: localCommand });
    onRuntimeStarted({ ...platform, ownedAndroidLease: lease }); return platform;
  }
  if (pids.length || serial) throw new Error('Private Android target is already running without its exact lease');
  let launch;
  try {
    for (let recovery = 0; recovery < 2; recovery += 1) {
      lease.verify({ avdName });
      const logPath = resolve(runDirectory, 'platforms', `android-owned-test-${testNumber}-attempt-${attempt}-launch-${recovery}.log`);
      mkdirSync(resolve(logPath, '..'), { recursive: true });
      const fd = openSync(logPath, 'a', 0o600);
      let emulator;
      try {
        emulator = ports.spawnProcess(sdk.emulatorPath, profile.launchArgs, { detached: true, stdio: ['ignore', fd, fd],
          env: { ...process.env, ...profile.privateEnv, PATH: withAndroidSdkPath(process.env.PATH, sdk) } });
      } finally { closeSync(fd); }
      launch = observeAndroidLaunch(emulator, logPath); emulator.unref();
      current = { emulator, launch, lease }; runnerOwnedAndroidProcesses.set(name, current);
      try {
        const actualSerial = await ports.waitForAvd(sdk, name, discoveredSerial => {
          current.serial = discoveredSerial;
          onRuntimeStarted({ platform: 'Android', avdName, serial: discoveredSerial, ownedAndroidLease: lease, ownedAndroidProfile: profile });
        }, 180_000, launch);
        const platform = { platform: 'Android', avdName, serial: actualSerial, state: 'Booted', sdkRoot: sdk.sdkRoot, adbServer,
          startedBySupervisor: true, emulatorPid: emulator.pid, emulatorLogPath: logPath, ownedAndroidProfile: profile };
        current.serial = actualSerial; current.platform = platform;
        await verifyOwnedAndroidRuntime(platform, { runId: profile.runId, runCommand: localCommand });
        onRuntimeStarted({ ...platform, ownedAndroidLease: lease }); return platform;
      } catch (error) {
        const matching = await findAvdQemuPids(name, localCommand);
        const retry = matching.length === 1 && matching[0] === emulator.pid && (shouldRestartAndroidAvdForReadinessFailure(error, recovery)
          || shouldColdBootOwnedOfflineAndroidAvd({ error, recoveryAttempt: recovery, startedBySupervisor: true,
            offlineSerials: error.offlineSerials, qemuPids: matching, launchPid: emulator.pid }));
        if (!retry) throw error;
        await drainFailedAndroidLaunch(name, launch);
        if ((await findAvdQemuPids(name, localCommand)).length) throw new Error('Owned Android writer did not leave after actual close');
      }
    }
    throw new Error('Owned Android readiness did not complete');
  } catch (error) {
    try { await drainFailedAndroidLaunch(name, launch); }
    catch (cleanupError) { throw new AggregateError([error, cleanupError], 'Owned Android preparation failed and launch did not close', { cause: error }); }
    throw error;
  }
}

export const OWNED_IOS_PROFILE_SPEC = Object.freeze({
  simulatorName: 'iPhone 17',
  runtimeIdentifier: 'com.apple.CoreSimulator.SimRuntime.iOS-26-5',
  deviceTypeIdentifier: 'com.apple.CoreSimulator.SimDeviceType.iPhone-17',
});

export async function readIosSimulatorInventory(runCommand = command) {
  const { stdout } = await runCommand('xcrun', ['simctl', 'list', 'devices', '-j']);
  return JSON.parse(stdout);
}

export function validateOwnedIosProfile(profile, { runId, simulatorName = OWNED_IOS_PROFILE_SPEC.simulatorName } = {}) {
  if (!runId || profile?.runId !== runId || profile.simulatorName !== simulatorName ||
      profile.createdByRun !== true || profile.absentBeforeCreate !== true ||
      !/^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/i.test(profile.udid ?? '') ||
      !profile.name?.startsWith(`MetaSecret-e2e-${runId}-`) ||
      profile.runtimeIdentifier !== OWNED_IOS_PROFILE_SPEC.runtimeIdentifier ||
      profile.deviceTypeIdentifier !== OWNED_IOS_PROFILE_SPEC.deviceTypeIdentifier) {
    throw new Error('Invalid or foreign owned iOS profile');
  }
}

export function selectIosSimulator({ simulatorName, inventory, ownedIosProfile, runId, targetUdid, allowUnavailable = false }) {
  const devices = Object.entries(inventory.devices).flatMap(([runtimeIdentifier, values]) =>
    values.map((device) => ({ ...device, runtimeIdentifier })));
  if (ownedIosProfile) validateOwnedIosProfile(ownedIosProfile, { runId, simulatorName });
  const udid = ownedIosProfile?.udid ?? targetUdid;
  const matches = devices.filter((device) => udid ? device.udid === udid : device.name === simulatorName);
  if (matches.length !== 1) throw new Error('Selected iOS simulator is unavailable or ambiguous');
  const device = matches[0];
  if (device.isAvailable === false && !allowUnavailable) throw new Error('Selected owned iOS profile is unavailable');
  if (ownedIosProfile && (device.name !== ownedIosProfile.name ||
      device.runtimeIdentifier !== ownedIosProfile.runtimeIdentifier || device.deviceTypeIdentifier !== ownedIosProfile.deviceTypeIdentifier)) {
    throw new Error('Inventory does not match exact owned iOS profile');
  }
  if (!ownedIosProfile && device.name !== simulatorName) throw new Error('Selected iOS simulator name/UUID mismatch');
  return { ...device, ...(ownedIosProfile ? { ownedIosProfile } : {}) };
}

export function resolveIosScenarioSimulator({ simulatorName, inventory, readinessPath = process.env.E2E_READINESS_PATH, runId = process.env.E2E_RUN_ID }) {
  if (!readinessPath) {
    if (process.env.E2E_RANGE_RUN === '1') throw new Error('Range iOS target requires parent readiness');
    return selectExistingIosSimulator(simulatorName, inventory);
  }
  const report = JSON.parse(readFileSync(readinessPath, 'utf8'));
  assertExistingDeviceRun({ readiness: report });
  const matches = report.platforms.filter((entry) => entry.platform === 'iOS');
  if (report.runId !== runId || matches.length !== 1 || matches[0].simulatorName !== simulatorName ||
      (matches[0].ownedIosProfile && matches[0].udid !== matches[0].ownedIosProfile.udid)) {
    throw new Error('Selected iOS target does not match parent readiness');
  }
  return selectExistingIosSimulator(simulatorName, inventory, matches[0].udid);
}

export async function verifyOwnedIosResetTarget(platform, { runId, readinessPath, runCommand = command }) {
  const profile = platform.ownedIosProfile;
  validateOwnedIosProfile(profile, { runId });
  if (platform.udid !== profile.udid || !readinessPath) throw new Error('Reset requires exact owned iOS profile readiness');
  const report = JSON.parse(readFileSync(readinessPath, 'utf8'));
  assertExistingDeviceRun({ readiness: report });
  const matches = report.platforms.filter((entry) => entry.platform === 'iOS');
  if (report.runId !== runId || matches.length !== 1 || matches[0].udid !== platform.udid ||
      JSON.stringify(matches[0].ownedIosProfile) !== JSON.stringify(profile)) throw new Error('Reset owned iOS profile differs from parent readiness');
  const device = selectIosSimulator({ simulatorName: profile.simulatorName, inventory: await readIosSimulatorInventory(runCommand), ownedIosProfile: profile, runId });
  if (device.state !== 'Booted') throw new Error('Reset owned iOS profile must remain Booted');
}

export async function verifyOwnedIosProfilePrerequisites(runCommand = command) {
  const runtimes = JSON.parse((await runCommand('xcrun', ['simctl', 'list', 'runtimes', '-j'])).stdout).runtimes;
  const types = JSON.parse((await runCommand('xcrun', ['simctl', 'list', 'devicetypes', '-j'])).stdout).devicetypes;
  if (runtimes.filter((runtime) => runtime.identifier === OWNED_IOS_PROFILE_SPEC.runtimeIdentifier && runtime.isAvailable === true).length !== 1 ||
      types.filter((type) => type.identifier === OWNED_IOS_PROFILE_SPEC.deviceTypeIdentifier).length !== 1) {
    throw new Error('Exact owned iOS runtime/device type is unavailable or ambiguous');
  }
}

export async function deleteRunOwnedIosProfile(profile, { runId, runCommand = command, now = Date.now, wait = delay }) {
  validateOwnedIosProfile(profile, { runId });
  let inventory = await readIosSimulatorInventory(runCommand);
  if (!Object.values(inventory.devices).flat().some((device) => device.udid === profile.udid)) return;
  const device = selectIosSimulator({ simulatorName: profile.simulatorName, inventory, ownedIosProfile: profile, runId, allowUnavailable: true });
  let shutdownError;
  if (device.state !== 'Shutdown') {
    try { await runCommand('xcrun', ['simctl', 'shutdown', profile.udid], { timeout: 30_000 }); }
    catch (error) { shutdownError = error; }
  }
  const deadline = now() + 30_000;
  while (true) {
    inventory = await readIosSimulatorInventory(runCommand);
    if (!Object.values(inventory.devices).flat().some((item) => item.udid === profile.udid)) return;
    if (selectIosSimulator({ simulatorName: profile.simulatorName, inventory, ownedIosProfile: profile, runId, allowUnavailable: true }).state === 'Shutdown') break;
    if (now() >= deadline) throw new Error('Owned iOS profile did not shut down before deletion', { cause: shutdownError });
    await wait(250);
  }
  let deleteError;
  try { await runCommand('xcrun', ['simctl', 'delete', profile.udid], { timeout: 30_000 }); }
  catch (error) { deleteError = error; }
  if (Object.values((await readIosSimulatorInventory(runCommand)).devices).flat().some((item) => item.udid === profile.udid)) {
    throw new Error('Owned iOS profile still exists after deletion', { cause: deleteError });
  }
}

export async function createRunOwnedIosProfile({ runId, simulatorName = OWNED_IOS_PROFILE_SPEC.simulatorName, onCreated = () => {}, onCreateIntent = () => {}, runCommand = command }) {
  if (!/^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$/.test(runId ?? '') || simulatorName !== OWNED_IOS_PROFILE_SPEC.simulatorName) throw new Error('Invalid owned iOS profile run/target');
  await verifyOwnedIosProfilePrerequisites(runCommand);
  const before = Object.values((await readIosSimulatorInventory(runCommand)).devices).flat();
  const name = `MetaSecret-e2e-${runId}-${randomUUID()}`;
  if (before.some((device) => device.name === name)) throw new Error('Owned iOS profile name already exists');
  const intent = { ...OWNED_IOS_PROFILE_SPEC, runId, name, baselineUdids: before.map((device) => device.udid) };
  const claimedProfile = (udid) => ({ ...OWNED_IOS_PROFILE_SPEC, runId, name, udid, createdByRun: true, absentBeforeCreate: true });
  let profile; let createAttempted = false;
  try {
    onCreateIntent(intent);
    createAttempted = true;
    const { stdout } = await runCommand('xcrun', ['simctl', 'create', name, OWNED_IOS_PROFILE_SPEC.deviceTypeIdentifier, OWNED_IOS_PROFILE_SPEC.runtimeIdentifier], { timeout: 60_000 });
    const candidate = claimedProfile(stdout.trim());
    validateOwnedIosProfile(candidate, { runId, simulatorName });
    if (before.some((device) => device.udid === candidate.udid)) throw new Error('Creation returned a pre-existing iOS UUID');
    profile = candidate;
    onCreated(profile);
    selectIosSimulator({ simulatorName, inventory: await readIosSimulatorInventory(runCommand), ownedIosProfile: profile, runId });
    return profile;
  } catch (error) {
    const cleanupErrors = [];
    if (!profile && createAttempted) {
      try {
        const inventory = await readIosSimulatorInventory(runCommand);
        const matches = Object.entries(inventory.devices).flatMap(([runtimeIdentifier, devices]) =>
          devices.map((device) => ({ ...device, runtimeIdentifier }))).filter((device) => device.name === name);
        if (matches.length > 1) throw new Error('Uncertain iOS creation has ambiguous nonce profiles');
        if (matches.length === 1) {
          const device = matches[0];
          if (before.some((prior) => prior.udid === device.udid) || device.runtimeIdentifier !== intent.runtimeIdentifier ||
              device.deviceTypeIdentifier !== intent.deviceTypeIdentifier) throw new Error('Uncertain iOS creation does not match its exact intent');
          profile = claimedProfile(device.udid);
          validateOwnedIosProfile(profile, { runId, simulatorName });
          onCreated(profile);
        }
      } catch (recoveryError) { cleanupErrors.push(recoveryError); }
    }
    if (profile) {
      try { await deleteRunOwnedIosProfile(profile, { runId, runCommand }); }
      catch (cleanupError) { cleanupErrors.push(cleanupError); }
    }
    if (cleanupErrors.length) throw new AggregateError([error, ...cleanupErrors], 'Owned iOS creation and cleanup failed', { cause: error });
    throw error;
  }
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
  ownedIosProfile,
  runId,
} = {}) {
  const inventory = await readIosSimulatorInventory(runCommand);
  const simulator = ownedIosProfile ? selectIosSimulator({ simulatorName, inventory, ownedIosProfile, runId }) : selectExistingIosSimulator(simulatorName, inventory);
  const startedBySupervisor = simulator.state !== 'Booted';
  let ownedBoot;
  if (simulator.state !== 'Booted') {
    process.stdout.write(`📱 Starting visible iOS simulator ${simulator.name}\n`);
    await runCommand('xcrun', ['simctl', 'boot', simulator.udid]);
    const runtime = { platform: 'iOS', simulatorName, udid: simulator.udid, ...(ownedIosProfile ? { ownedIosProfile } : {}) };
    if (!ownedIosProfile) {
      ownedBoot = { simulatorName, udid: simulator.udid, guestManagerPid: null };
      runnerOwnedIosRuntimeObjects.set(runtime, ownedBoot);
      runnerOwnedIosBoots.set(simulator.udid, ownedBoot);
    }
    onRuntimeStarted(runtime);
  }
  await runCommand('xcrun', ['simctl', 'bootstatus', simulator.udid, '-b'], { timeout: 300_000 });
  const health = await verifyIosRuntimeHealth({ udid: simulator.udid, runCommand });
  if (ownedBoot) ownedBoot.guestManagerPid = health.guestManagerPid;
  const runAndCapture = async (file, args, options) => runCommand(file, args, options);
  const runAndWait = async (file, args, options) => runCommand(file, args, options);
  const ui = await openDeviceUi({ runAndCapture, runAndWait, simulatorUdid: simulator.udid });
  return { platform: 'iOS', simulatorName, udid: simulator.udid, state: 'Booted', visibleUi: ui, startedBySupervisor, ...(ownedIosProfile ? { ownedIosProfile } : {}) };
}

export async function prepareScenarioPlatforms({ scenario, testNumber, attempt, runId, runDirectory, onRuntimeStarted = () => {}, onDiagnostic, runtimeProfile = createScenarioRuntimeProfile({ testNumber, scenario }), verifyEnvironment = verifyLocalEnvironment, ownedIosProfile, ownedAndroidLease }) {
  if (ownedIosProfile !== undefined || ownedAndroidLease !== undefined) throw new Error('Owned profile preparation is forbidden; reuse existing devices');
  assertExistingDeviceRun({ scenarios: [scenario] });
  const environment = verifyEnvironment(scenario);
  const platforms = [];
  if (scenario.android?.avdName) {
    const platform = await prepareAndroid(scenario.android.avdName, onRuntimeStarted, { runDirectory, testNumber, attempt, ownedAndroidLease, runId, onDiagnostic });
    platforms.push(platform);
  }
  if (scenario.ios?.simulatorName) {
    const platform = await prepareIos(scenario.ios.simulatorName, onRuntimeStarted, { ownedIosProfile, runId });
    platforms.push(platform);
  }
  const directory = resolve(runDirectory, 'readiness');
  mkdirSync(directory, { recursive: true });
  const reportPath = resolve(directory, `test-${String(testNumber).padStart(2, '0')}-attempt-${attempt}.json`);
  const report = {
    test: testNumber,
    ...(ownedAndroidLease ? { attempt } : {}),
    runId,
    scenario: scenario.name ?? `UI E2E Test ${testNumber}`,
    checkedAt: new Date().toISOString(),
    environment,
    platforms,
    runtimeProfile,
    ...(runtimeProfile.kind === 'core-http' ? { requiredNonPlatformParticipants: [...runtimeProfile.requiredNonPlatformParticipants], nonPlatformParticipants: [] } : {}),
    status: runtimeProfile.kind === 'core-http' ? 'participants-pending' : platforms.length === 0 ? 'ready' : 'runtime-ready',
    note: 'App and test-runner installation is verified by the scenario orchestrator before its first business mutation.',
  };
  writeFileSync(reportPath, `${JSON.stringify(report, null, 2)}\n`, { mode: 0o600, flag: 'wx' });
  return { platforms, reportPath };
}

function assertRuntimeData(runtime, expected) {
  const keys = Reflect.ownKeys(runtime), descriptors = Object.getOwnPropertyDescriptors(runtime);
  if (keys.length !== Object.keys(expected).length || keys.some(key => !Object.hasOwn(expected, key))
    || Object.entries(expected).some(([key, value]) => {
      const field = descriptors[key];
      return !field || !Object.hasOwn(field, 'value') || field.value !== value
        || !field.enumerable || !field.configurable || !field.writable;
    })) throw new Error('Runtime object changed before ownership disposition');
}

/** Called only after the range has joined every actor; receipts never transfer authority. */
export async function releaseHealthyRunnerOwnedRuntimes(runtimes, { reason, onEvent = () => {} } = {}, {
  runCommand = command, androidSdk = discoverAndroidSdk, findAndroidAvd = findOnlineAndroidAvd,
  existingAvd = readExistingAndroidAvd, readIosInventory = readIosSimulatorInventory,
  now = () => new Date().toISOString(),
} = {}) {
  if (reason !== 'pending-test' || !Array.isArray(runtimes) || new Set(runtimes).size !== runtimes.length) throw Error('Exact pending-test runtime disposition required');
  const records = runtimes.map(runtime => {
    const android = runnerOwnedAndroidRuntimeObjects.get(runtime), ios = runnerOwnedIosRuntimeObjects.get(runtime);
    if (!android && !ios) throw Error('Healthy release requires the exact current-process runtime object');
    return { runtime, owner: android ?? ios, platform: android ? 'Android' : 'iOS' };
  });
  const validate = ({ runtime, owner, platform }) => {
    if (platform === 'Android') {
      assertRuntimeData(runtime, { platform, avdName: owner.avdName, serial: owner.serial });
      const current = runnerOwnedAndroidProcesses.get(owner.avdName);
      if (current && current.launch !== owner.launch) throw Error('Android runtime owner changed before release');
      if (!owner.released && (current?.launch !== owner.launch || owner.launch.closed || owner.launch.exit)) throw Error('Android owned launch is not live');
      if (JSON.stringify(existingAvd(owner.avdName)) !== JSON.stringify(owner.registeredAvd)) throw Error('Registered Android AVD changed before release');
    } else {
      assertRuntimeData(runtime, { platform, simulatorName: owner.simulatorName, udid: owner.udid });
      const current = runnerOwnedIosBoots.get(owner.udid);
      if (current && current !== owner || !owner.released && (current !== owner || !owner.guestManagerPid)) throw Error('iOS boot owner changed before release');
    }
  };
  records.forEach(validate);
  for (const record of records) {
    const { owner, platform } = record;
    if (owner.released) continue;
    if (platform === 'Android') {
      const sdk = androidSdk();
      const serial = await findAndroidAvd(sdk, owner.avdName, (file, args) => runCommand(file, args, { timeout: 10_000 }));
      if (serial !== owner.serial) throw Error('Exact Android runtime no longer online');
      for (const [args, expected] of [
        [['get-state'], 'device'], [['shell', 'getprop', 'sys.boot_completed'], '1'],
        [['shell', 'cmd', 'activity', 'get-started-user-state', '0'], 'RUNNING_UNLOCKED'],
      ]) if ((await runCommand(sdk.adbPath, ['-s', serial, ...args], { timeout: 10_000 })).stdout.trim() !== expected) throw Error('Android release health failed');
      await verifyAndroidInstallReadiness({ adbPath: sdk.adbPath, serial, runCommand });
    } else {
      const inventory = await readIosInventory((file, args) => runCommand(file, args, { timeout: 15_000 }));
      const device = selectExistingIosSimulator(owner.simulatorName, inventory, owner.udid);
      if (device.state !== 'Booted') throw Error('Exact iOS runtime is not Booted');
      const health = await verifyIosRuntimeHealth({ udid: owner.udid, runCommand });
      if (health.guestManagerPid !== owner.guestManagerPid) throw Error('iOS guest lifetime changed before release');
    }
    records.forEach(validate);
  }
  records.forEach(validate);
  const at = now();
  if (typeof at !== 'string' || !/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z$/.test(at)) throw Error('Runtime release timestamp unavailable');
  const receipts = records.map(({ owner, platform }) => owner.released ?? Object.freeze({ operation: 'released-healthy', reason,
    platform, ...(platform === 'Android' ? { avdName: owner.avdName, serial: owner.serial } : { simulatorName: owner.simulatorName, udid: owner.udid }),
    at, shutdown: false, authorityTransferred: false }));
  records.forEach(validate);
  records.forEach(({ owner, platform }, index) => {
    owner.released = receipts[index];
    if (platform === 'Android') runnerOwnedAndroidProcesses.delete(owner.avdName);
    else runnerOwnedIosBoots.delete(owner.udid);
  });
  for (const receipt of receipts) try { onEvent(`released healthy runner-owned ${receipt.platform} runtime; guest remains running`); } catch { /* Returned receipts remain authoritative. */ }
  return Object.freeze(receipts);
}

export async function stopRunnerOwnedRuntimes(runtimes, onEvent = () => {}) {
  const failures = [];
  for (const runtime of runtimes) {
    const owned = runnerOwnedAndroidRuntimeObjects.get(runtime);
    const ownedIos = runnerOwnedIosRuntimeObjects.get(runtime);
    let platform = owned ? 'Android' : ownedIos ? 'iOS' : undefined;
    let runtimeName = owned?.avdName ?? ownedIos?.simulatorName;
    try {
      if (owned) {
        const expected = { platform: 'Android', avdName: owned.avdName, serial: owned.serial };
        const keys = Reflect.ownKeys(runtime);
        const descriptors = Object.getOwnPropertyDescriptors(runtime);
        if (keys.length !== 3 || keys.some(key => !Object.hasOwn(expected, key))
          || Object.entries(expected).some(([key, value]) => {
            const descriptor = descriptors[key];
            return !descriptor || !Object.hasOwn(descriptor, 'value') || descriptor.value !== value
              || !descriptor.enumerable || !descriptor.configurable || !descriptor.writable;
          })) throw new Error('Android runtime object changed before cleanup');
      } else if (ownedIos) {
        assertRuntimeData(runtime, { platform, simulatorName: ownedIos.simulatorName, udid: ownedIos.udid });
      } else {
        platform = runtime.platform;
        const descriptors = Object.getOwnPropertyDescriptors(runtime);
        runtimeName = descriptors.avdName?.value ?? descriptors.simulatorName?.value;
      }
      if (platform === 'Android') {
        if (!owned) {
          if (runtime.ownedAndroidLease) { await stopOwnedAndroid(runtime.ownedAndroidLease); continue; }
          if (runtime.ownedAndroidProfile) throw new Error('Owned Android cleanup lost its current-process lease');
          throw new Error('Android cleanup requires its exact current-process runtime object');
        }
        const current = runnerOwnedAndroidProcesses.get(owned.avdName);
        if (current && current.launch !== owned.launch) throw new Error('Android runtime owner changed before cleanup');
        if (owned.released) { try { onEvent('acknowledged released Android lifetime; guest remains running'); } catch {} continue; }
        if (owned.launch.closed) {
          onEvent(`acknowledged closed runner-owned Android runtime ${owned.avdName}`);
          continue;
        }
        if (current?.launch !== owned.launch) throw new Error('Android cleanup lost its retained launch');
        const sdk = discoverAndroidSdk();
        await command(sdk.adbPath, ['-s', owned.serial, 'emu', 'kill']);
      } else if (platform === 'iOS') {
        if (ownedIos) {
          assertRuntimeData(runtime, { platform, simulatorName: ownedIos.simulatorName, udid: ownedIos.udid });
          const current = runnerOwnedIosBoots.get(ownedIos.udid);
          if (current && current !== ownedIos) throw Error('iOS boot owner changed before cleanup');
          if (ownedIos.released) { try { onEvent('acknowledged released iOS lifetime; guest remains running'); } catch {} continue; }
        } else if (!runtime.ownedIosProfile) throw Error('iOS cleanup requires its exact current-process runtime object');
        if (ownedIos) await command('xcrun', ['simctl', 'shutdown', ownedIos.udid]);
        else if (runtime.ownedIosProfile) {
          if (runtime.udid !== runtime.ownedIosProfile.udid) throw new Error('Owned iOS cleanup UUID mismatch');
          await deleteRunOwnedIosProfile(runtime.ownedIosProfile, { runId: runtime.ownedIosProfile.runId });
        }
        else await command('xcrun', ['simctl', 'shutdown', runtime.udid]);
      }
      onEvent(`stopped runner-owned ${platform} runtime ${runtimeName}`);
    } catch (error) {
      failures.push(`${platform} ${runtimeName}: ${error.message}`);
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
  if (report.runtimeProfile?.kind === 'core-http') {
    report.requiredNonPlatformParticipants = [...report.runtimeProfile.requiredNonPlatformParticipants];
  } else if (requiredNonPlatformParticipants) {
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
  if (report.runtimeProfile?.kind === 'core-http') {
    report.status = coreHttpProofsReady(report) ? 'ready' : 'participants-pending';
    if (report.status !== 'ready') delete report.appsReadyAt;
  }
  if (nativePlatformsReady && nonPlatformParticipantsReady && (report.runtimeProfile?.kind !== 'core-http' || coreHttpProofsReady(report))) {
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
    const physicalName = platform.ownedAndroidProfile?.physicalAvdName ?? platform.avdName;
    if (platform.ownedAndroidProfile) await verifyOwnedAndroidRuntime(platform, { runId: platform.ownedAndroidProfile.runId, runCommand });
    const ownedProcess = ownedAndroidProcesses.get(physicalName);
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
    const serial = await findAndroidAvd(sdk, physicalName, (file, args) => runCommand(file, args, { timeout: 10_000 }));
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
