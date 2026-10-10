import { ensureIosFfi, prepareIosXcodeBuild, recordIosXcodeBuild, verifyIosXcodeBuild } from '../shared/ios-ffi-provenance.mjs';
import {
  appendFileSync,
  cpSync,
  existsSync,
  lstatSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  readlinkSync,
  rmSync,
  statfsSync,
  writeFileSync,
} from 'node:fs';
import { resolve, dirname, basename } from 'node:path';
import { execFileSync, spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createServer, request as httpRequest } from 'node:http';
import { createConnection } from 'node:net';
import { tmpdir } from 'node:os';
import process from 'node:process';
import { setTimeout as wait } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import {
  discoverAndroidSdk,
  findOnlineAndroidAvd,
  withAndroidSdkPath,
} from '../shared/android-sdk.mjs';
import { createDiagnosticRedactor } from '../shared/diagnostic-redaction.mjs';
import {
  androidInstrumentationAdbArguments,
  androidInstrumentationDiagnosticArguments,
  superviseAndroidInstrumentation,
} from '../shared/android-instrumentation.mjs';
import { openVisibleIosDeviceUi } from '../shared/visible-ios-ui.mjs';
import { installAndVerifyAndroidE2eApps, installBuiltIosTestApps } from '../shared/platform-readiness.mjs';
import { resetE2eNativeAppState } from '../shared/e2e-state-reset.mjs';
import { markerScan, markerScanStream, ssClaimsInvalidationMarker } from '../shared/process-marker.mjs';
import { recoveryClaimIds, selectNewPendingRecoveryClaim } from '../shared/test15-recovery-claims.mjs';
import { updateReadinessReport } from '../shared/platform-supervisor.mjs';
import { createHarnessShutdown, formatHarnessShutdownError } from '../shared/harness-shutdown.mjs';
import { recycleWebPage, webPageRefreshCycles } from '../shared/web-page-recycle.mjs';
import { createPostCycleActions } from '../shared/post-cycle-actions.mjs';
import { bindIosStepSecret, createStepEnvironment } from '../shared/ios-step-secret.mjs';

const root = dirname(fileURLToPath(import.meta.url));
const e2eRoot = resolve(root, '..');
const artifactsDirectory = resolve(e2eRoot, '.artifacts');
const scenarioArgument = process.argv.slice(2).find((argument) => !argument.startsWith('--'));
const exitOnSuccess = process.argv.includes('--exit-on-success') || process.env.E2E_EXIT_ON_SUCCESS === '1';
const diagnosticStopAfterCycle = Number.parseInt(
  process.env.E2E_DIAGNOSTIC_STOP_AFTER_CYCLE ?? '',
  10,
);
const diagnosticAndroidVerbose = process.env.E2E_DIAGNOSTIC_ANDROID_VERBOSE === '1';
const scenarioPath = resolve(root, scenarioArgument ?? 'scenario.json');
const scenario = JSON.parse(readFileSync(scenarioPath, 'utf8'));
const testNumber = scenario.testNumber ?? 4;
if (process.env.E2E_RANGE_RUN === '1') scenario.runId = process.env.E2E_RUN_ID;
const scenarioArtifactLabel = basename(scenarioPath, '.json');
const secretConfigs = scenario.secrets ?? { default: scenario.secret };
const defaultSecretKey = Object.keys(secretConfigs)[0];
const defaultSecret = secretConfigs[defaultSecretKey];
const diagnosticFileName = scenario.runId
  ? `test-${testNumber}-${scenario.runId}-attempt-${process.env.E2E_TEST_ATTEMPT ?? '1'}-${scenarioArtifactLabel}.log`
  : `test-${testNumber}.log`;
const diagnosticLogPath = resolve(artifactsDirectory, diagnosticFileName);
const iosStepConfigPath = '/tmp/metasecret-e2e-ios-step.json';
const buildProvenancePath = resolve(artifactsDirectory, 'ui-e2e-build-provenance.json');
const reuseProvenArtifacts = process.env.E2E_REUSE_PROVEN_ARTIFACTS === '1';
const bootstrapBuildProvenance = process.env.E2E_BOOTSTRAP_BUILD_PROVENANCE === '1';
let buildProvenanceVerified = false;

function secretConfigFor(keyOrName) {
  if (keyOrName && secretConfigs[keyOrName]) return secretConfigs[keyOrName];
  return Object.values(secretConfigs).find((config) => config?.name === keyOrName) ?? defaultSecret;
}

function secretNameFor(keyOrName) {
  const config = secretConfigFor(keyOrName);
  if (!config?.name) throw new Error(`Secret configuration is missing a name for ${keyOrName}`);
  return config.name;
}

function secretValueForName(secretName) {
  return Object.values(secretConfigs).find((config) => config?.name === secretName)?.value
    ?? defaultSecret?.value;
}

function normalizeSecretCreationPlan(rawPlan) {
  if (!rawPlan) return null;
  return Object.fromEntries(Object.entries(rawPlan).map(([stage, platformSecrets]) => {
    if (!platformSecrets || typeof platformSecrets !== 'object') {
      throw new Error(`Invalid secretCreation.${stage}: expected platform map`);
    }
    return [stage, Object.fromEntries(Object.entries(platformSecrets).map(([platform, keys]) => {
      if (!Array.isArray(keys)) {
        throw new Error(`Invalid secretCreation.${stage}.${platform}: expected array`);
      }
      return [platform, keys.map(secretNameFor)];
    }))];
  }));
}

const secretCreationPlan = normalizeSecretCreationPlan(scenario.secretCreation);
let joinExpectedSecretNames = [];

function normalizeSender(sender) {
  if (typeof sender === 'string') {
    return { platform: sender, secret: defaultSecret?.name };
  }
  return {
    platform: sender.platform,
    secret: secretNameFor(sender.secret ?? sender.secretKey),
  };
}

function normalizeApproval(approval, fallbackSecret = defaultSecret?.name) {
  if (typeof approval === 'string') {
    return { platform: approval, secret: fallbackSecret, action: 'approve' };
  }
  return {
    platform: approval.platform ?? approval.approver,
    secret: secretNameFor(approval.secret ?? approval.secretKey ?? fallbackSecret),
    action: approval.action ?? approval.decision ?? 'approve',
  };
}

const rawRecoveryCycles = scenario.mode === 'both-receivers-offline'
  ? []
  : scenario.recovery.cyclePlan
  ? scenario.recovery.cyclePlan.flatMap((group) => Array.from({ length: group.count }, (_, offset) => {
      const resolveValue = (value) => Array.isArray(value) ? value[offset % value.length] : value;
      const senderEntries = group.senders.map(normalizeSender);
      const defaultApprovalSecret = senderEntries[0]?.secret ?? defaultSecret?.name;
      const approvals = group.mode === 'direct-show'
        ? []
        : group.approvals
        ? group.approvals.map((approval) => normalizeApproval(approval, defaultApprovalSecret))
        : [
            normalizeApproval(resolveValue(group.firstApprover), defaultApprovalSecret),
            normalizeApproval(resolveValue(group.secondApprover), defaultApprovalSecret),
          ];
      return {
        block: group.block,
        kind: group.mode,
        offlineReceiver: group.offlineReceiver ?? scenario.offlineReceiver?.platform,
        observer: resolveValue(group.observer),
        senders: senderEntries.map((sender) => sender.platform),
        senderSecrets: Object.fromEntries(senderEntries.map((sender) => [sender.platform, sender.secret])),
        approvals,
        expectedOutcome: group.expectedOutcome,
      };
    }))
  : (scenario.recovery.groups ?? []).flatMap((group) => {
      if (group.senders) {
      return Array.from({ length: group.count }, () => ({
        group: group.name,
        offlineReceiver: group.offlineReceiver ?? scenario.offlineReceiver?.platform,
          senders: group.senders,
          approvals: group.approvals,
          followUpApprovals: group.followUpApprovals ?? [],
        }));
      }
      const alternatingApprovers = group.approver.startsWith('alternate-')
        ? group.approver.split('-').slice(1)
        : null;
      return Array.from({ length: group.count }, (_, offset) => ({
        group: group.approver,
        offlineReceiver: group.offlineReceiver ?? scenario.offlineReceiver?.platform,
        approver: alternatingApprovers
          ? alternatingApprovers[offset % alternatingApprovers.length]
          : group.approver,
      }));
    });

const recoveryCycles = rawRecoveryCycles.map((cycle, index) => {
  const approvals = cycle.approvals?.map((approval) => normalizeApproval(approval))
    ?? [normalizeApproval(cycle.approver), ...((cycle.followUpApprovals ?? []).map((approval) => normalizeApproval(approval)))];
  const senderSecrets = cycle.senderSecrets ?? Object.fromEntries(
    (cycle.senders ?? []).map((sender) => [sender, defaultSecret?.name]),
  );
  return {
    ...cycle,
    kind: cycle.kind ?? cycle.mode ?? 'recovery',
    number: index + 1,
    senders: cycle.senders ?? Object.keys(senderSecrets),
    senderSecrets,
    approvals,
    expectedOutcome: cycle.expectedOutcome,
    firstApprover: approvals[0]?.platform,
    secondApprover: approvals[1]?.platform,
  };
});
// The native UI actor's showAcceptedRecoveryAfterOffline assertion waits up to
// 180 seconds for the revealed-secret UI. Keep the parent marker deadline
// beyond that child deadline so the coordinator does not cancel XCTest before
// it can report its own result.
const recoveryShowTimeoutMs = scenario.recovery.showTimeoutMs ?? 195_000;
const cyclePlanJson = JSON.stringify(recoveryCycles);
const secretConfigJson = JSON.stringify(secretConfigs);
const iosSenderCycles = recoveryCycles
  .filter((cycle) => cycle.senders.includes('ios'))
  .map((cycle) => cycle.number)
  .join(',');
const iosApprovalSteps = recoveryCycles
  .flatMap((cycle) => cycle.approvals.map((approval, index) => [index + 1, approval.platform])
    .filter(([, approver]) => approver === 'ios')
    .map(([step]) => `${cycle.number}:${step}`))
  .join(',');
const projectRoot = resolve(e2eRoot, '..');
const coreRoot = projectRoot;
const webDirectory = resolve(e2eRoot, scenario.web.directory);
const composeRoot = resolve(e2eRoot, scenario.ios.composeRoot);
const iosProjectPath = resolve(composeRoot, 'iosApp/iosApp.xcodeproj');
const iosDerivedDataPath = resolve(e2eRoot, '.derivedData/iosApp');
const iosTestLogsPath = resolve(iosDerivedDataPath, 'Logs/Test');
const iosGradleUserHome = resolve(composeRoot, '.gradle-home');
const reuseIosBuildForTesting = true; // Every path uses the verified build-for-testing bundle.
let androidSdkTools;
const metaCliBinaryPath = resolve(projectRoot, 'meta-secret/target/debug/meta-cli');
const androidTestBundleId = `${scenario.android.bundleId}.test`;
const iosTestMethod = scenario.ios.testMethod
  ?? scenario.ios.joinTestMethod
  ?? 'testJoinAndroidInitiatedVaultAndHandleConcurrentRecovery';
const serverContainer = scenario.server.container;
const serverImage = scenario.server.image;
const processes = [];
const watchedProcessOutputs = [];
const browserDiagnostics = [];
let approvalCoordinator;
let activeServerProcess;
let currentCycleForDiagnostics = null;
let lastAnnouncedRoleAssignment = null;
let browserInstance;
let primaryVirtualAuthenticator;
let simulatorUdidForCleanup;
let androidSerialForCleanup;
let requestedSignalExitCode;
let cancelledActorPids = [];
let environmentGuard;
let serverContainerOwned = false;
let primaryWebClientReady = false;

function setCurrentCycleForDiagnostics(cycleValue) {
  const cycle = typeof cycleValue === 'object'
    ? cycleValue
    : recoveryCycles.find((candidate) => candidate.number === cycleValue);
  currentCycleForDiagnostics = cycle?.number ?? cycleValue;
  if (!cycle) return;

  const platformKeys = ['web', 'ios', 'android', 'cli'].filter((platform) => scenario[platform]);
  const roles = Object.fromEntries(platformKeys.map((platform) => [platform, 'observer']));
  for (const sender of cycle.senders ?? []) roles[sender] = cycle.kind === 'direct-show' ? 'showing' : 'requester';
  for (const approval of cycle.approvals ?? []) {
    const platform = typeof approval === 'string' ? approval : approval.platform;
    if (roles[platform]) roles[platform] = 'approver';
  }
  if (cycle.offlineReceiver && roles[cycle.offlineReceiver]) roles[cycle.offlineReceiver] = 'offline';

  const signature = platformKeys.map((platform) => `${platform}:${roles[platform]}`).join('|');
  if (signature === lastAnnouncedRoleAssignment) return;
  lastAnnouncedRoleAssignment = signature;
  const deviceRoles = platformKeys.map((platform, index) =>
    `Device ${String.fromCharCode(65 + index)} — ${platform.toUpperCase() === 'IOS' ? 'iOS' : platform[0].toUpperCase() + platform.slice(1)} ${roles[platform]}`);
  console.log(`📱 Roles changed: Cycle ${cycle.number} — ${deviceRoles.join('; ')}`);
}
// Android 16 shows the native compatibility dialog only on the first launch
// of a freshly installed APK. Re-launching and force-stopping the app before
// every instrumentation step can interrupt the persisted UI/native state.
// Handle the dialog once per orchestrator run, then let each test launch the
// activity directly.
let androidCompatibilityDialogHandled = false;
const webSenderRevealCompletedCycles = new Set();
const networkGateProcesses = new Map();

function availableDiskGiB() {
  const stats = statfsSync(e2eRoot);
  return (stats.bavail * stats.bsize) / (1024 ** 3);
}

async function reclaimBuildCacheForVisibleRun() {
  const beforeGiB = availableDiskGiB();
  if (beforeGiB >= 12) {
    writeDiagnostic(`[RESOURCE] BuildKit prune skipped freeDiskGiB=${beforeGiB.toFixed(2)}`);
    return;
  }

  writeDiagnostic(
    `[RESOURCE] free disk below visible-run boundary after server build: ${beforeGiB.toFixed(2)} GiB; `
      + 'pruning unused BuildKit cache while preserving the built server image',
  );
  await runAndWait('docker', ['builder', 'prune', '--all', '--force']);
  const afterGiB = availableDiskGiB();
  writeDiagnostic(
    `[RESOURCE] BuildKit prune completed freeDiskGiB=${afterGiB.toFixed(2)} `
      + `reclaimedGiB=${(afterGiB - beforeGiB).toFixed(2)}`,
  );
  if (afterGiB < 10) {
    throw new Error(
      `Insufficient disk for visible UI phase after safe BuildKit cleanup: ${afterGiB.toFixed(2)} GiB`,
    );
  }
}

function removePassedIosTestResults() {
  if (process.env.E2E_PRESERVE_EVIDENCE === '1') {
    writeDiagnostic('[RESOURCE] retained iOS xcresult bundle(s) by --preserve-evidence');
    return;
  }
  if (!existsSync(iosTestLogsPath)) return;
  let removed = 0;
  for (const entry of readdirSync(iosTestLogsPath)) {
    if (!entry.endsWith('.xcresult')) continue;
    rmSync(resolve(iosTestLogsPath, entry), { recursive: true, force: true });
    removed += 1;
  }
  if (removed > 0) {
    writeDiagnostic(`[RESOURCE] removed ${removed} passed iOS xcresult bundle(s)`);
  }
}

const networkLossConfig = scenario.networkLoss ?? null;

function networkProxyUrl(platform) {
  if (!networkLossConfig) return '';
  const port = platform === 'android'
    ? networkLossConfig.androidProxyPort
    : networkLossConfig.iosProxyPort;
  const host = platform === 'android' ? '10.0.2.2' : '127.0.0.1';
  return `http://${host}:${port}`;
}

// Test #4 intentionally exercises many recovery claims. Keeping every byte of
// server/UI output in live JavaScript strings makes memory usage grow with
// every cycle and eventually crashes the Web target. Markers are short, so a
// bounded tail is sufficient for synchronization and failure diagnostics.
const maxWatchedOutputChars = 128_000;
const maxBrowserDiagnosticEntries = 200;
const maxBrowserDiagnosticLineChars = 8_000;

const sensitiveDiagnosticValues = [...new Set(
  Object.values(secretConfigs)
    .map((config) => config?.value)
    .filter((value) => typeof value === 'string' && value.length > 0),
)].sort((left, right) => right.length - left.length);
const diagnosticRedactor = createDiagnosticRedactor(sensitiveDiagnosticValues);
const redactSensitiveText = diagnosticRedactor.redactText;
const redactCommandArguments = diagnosticRedactor.redactArguments;
const redactEnvironment = diagnosticRedactor.redactEnvironment;

function appendBounded(previous, next, limit = maxWatchedOutputChars) {
  const combined = `${previous}${next}`;
  return combined.length > limit ? combined.slice(-limit) : combined;
}

function filterVerboseWatchedOutput(command, sink, text) {
  // The local server defaults to DEBUG and prints the complete claim map on
  // each state read. Keep useful info/warn/error lines without creating a
  // multi-hundred-megabyte diagnostic file and terminal stream.
  if (command === 'docker' && sink === 'stdout') {
    const lines = text.split(/\r?\n/).filter((line) => !line.includes(' DEBUG '));
    const filtered = lines.join('\n');
    return text.endsWith('\n') ? `${filtered}\n` : filtered;
  }
  return text;
}

function recordBrowserDiagnostic(line, { important = true } = {}) {
  if (!important) return;
  const safeLine = redactSensitiveText(line);
  const boundedLine = safeLine.length > maxBrowserDiagnosticLineChars
    ? `${safeLine.slice(0, maxBrowserDiagnosticLineChars)}… [truncated]`
    : safeLine;
  browserDiagnostics.push(boundedLine);
  if (browserDiagnostics.length > maxBrowserDiagnosticEntries) {
    browserDiagnostics.splice(0, browserDiagnostics.length - maxBrowserDiagnosticEntries);
  }
  appendFileSync(diagnosticLogPath, `[browser] ${boundedLine}\n`);
}

function writeDiagnostic(message) {
  const line = `[${new Date().toISOString()}] ${redactSensitiveText(message)}\n`;
  process.stderr.write(line);
  appendFileSync(diagnosticLogPath, line);
}

function waitForBrowserConsole(page, predicate, timeoutMs = 120_000) {
  return new Promise((resolve, reject) => {
    let timer;
    const cleanup = () => {
      page.off('console', onConsole);
      clearTimeout(timer);
    };
    const onConsole = (message) => {
      try {
        if (!predicate(message)) return;
        cleanup();
        resolve(message.text());
      } catch (error) {
        cleanup();
        reject(error);
      }
    };
    page.on('console', onConsole);
    timer = setTimeout(() => {
      cleanup();
      reject(new Error(
        `Timed out waiting for browser console marker; recent diagnostics: ${browserDiagnostics.slice(-8).join(' | ')}`,
      ));
    }, timeoutMs);
  });
}

function startApprovalCoordinator(port = 5180) {
  const allowedApprovals = new Set();
  const server = createServer((request, response) => {
    const url = new URL(request.url, `http://${request.headers.host}`);
    if (url.pathname === '/scenario') {
      response.writeHead(200, { 'content-type': 'application/json' });
      response.end(JSON.stringify({
        vaultName: scenario.vault.name,
        secretConfig: secretConfigs,
        recoveryPlan: recoveryCycles,
        secretCreation: secretCreationPlan,
        joinExpectedSecretNames,
      }));
      return;
    }
    if (url.pathname !== '/approval') {
      response.writeHead(404).end();
      return;
    }

    const key = `${url.searchParams.get('platform')}:${url.searchParams.get('cycle')}`;
    response.writeHead(200, { 'content-type': 'text/plain' });
    response.end(allowedApprovals.has(key) ? 'allowed' : 'waiting');
  });

  return new Promise((resolvePromise, reject) => {
    server.once('error', reject);
    server.listen(port, '0.0.0.0', () => resolvePromise({
      allow(platform, cycle) {
        allowedApprovals.add(`${platform}:${cycle}`);
      },
      close() {
        return new Promise((resolveClose) => server.close(resolveClose));
      },
    }));
  });
}

function startNetworkGateProxy(platform, port, upstreamPort = 3000) {
  let blocked = false;
  const server = createServer((request, response) => {
    if (blocked) {
      writeDiagnostic(`[NETWORK] ${platform} blocked ${request.method} ${request.url}`);
      request.destroy();
      return;
    }

    const upstream = httpRequest({
      hostname: '127.0.0.1',
      port: upstreamPort,
      method: request.method,
      path: request.url,
      headers: {
        ...request.headers,
        host: `127.0.0.1:${upstreamPort}`,
      },
    }, (upstreamResponse) => {
      response.writeHead(
        upstreamResponse.statusCode ?? 502,
        upstreamResponse.statusMessage,
        upstreamResponse.headers,
      );
      upstreamResponse.pipe(response);
    });

    upstream.on('error', (error) => {
      writeDiagnostic(`[NETWORK] ${platform} upstream error: ${error.message}`);
      if (!response.headersSent) response.writeHead(502);
      response.end();
    });
    request.on('aborted', () => upstream.destroy());
    response.on('close', () => upstream.destroy());
    request.pipe(upstream);
  });

  return new Promise((resolvePromise, reject) => {
    server.once('error', reject);
    server.listen(port, '0.0.0.0', () => {
      const gate = {
        block() {
          blocked = true;
          writeDiagnostic(`[NETWORK] ${platform} -> OFFLINE`);
        },
        unblock() {
          blocked = false;
          writeDiagnostic(`[NETWORK] ${platform} -> ONLINE`);
        },
        close() {
          return new Promise((resolveClose) => server.close(resolveClose));
        },
      };
      networkGateProcesses.set(platform, gate);
      resolvePromise(gate);
    });
  });
}

async function startNetworkLossGates() {
  if (!networkLossConfig) return;
  await Promise.all([
    startNetworkGateProxy('android', networkLossConfig.androidProxyPort),
    startNetworkGateProxy('ios', networkLossConfig.iosProxyPort),
  ]);
  console.log(
    `Network-loss gates ready: Android=${networkProxyUrl('android')} `
      + `iOS=${networkProxyUrl('ios')}`,
  );
}

async function setNetworkGate(platform, online) {
  const gate = networkGateProcesses.get(platform);
  if (!gate) throw new Error(`Network-loss gate is not configured for ${platform}`);
  if (online) gate.unblock();
  else gate.block();
}
function run(command, args, options = {}) {
  harnessShutdown.assertCanSpawn();
  const child = spawn(command, args, {
    cwd: options.cwd ?? projectRoot,
    env: {
      ...process.env,
      PATH: withAndroidSdkPath(
        `/opt/homebrew/bin:/usr/local/bin:${process.env.PATH ?? ''}`,
        androidSdkTools ?? { pathPrefix: '' },
      ),
      ...(options.env ?? {}),
    },
    stdio: options.stdio ?? 'inherit',
    shell: false,
  });
  processes.push(child);
  return child;
}

function runAndWait(command, args, options = {}) {
  return new Promise((resolvePromise, reject) => {
    const child = run(command, args, { ...options, stdio: options.stdio ?? 'inherit' });
    child.once('error', reject);
    child.once('exit', (code, signal) => {
      if (code === 0) resolvePromise();
      else reject(new Error(`${command} exited with ${code ?? signal}`));
    });
  });
}

function runAndCapture(command, args, options = {}) {
  return new Promise((resolvePromise, reject) => {
    const child = run(command, args, { ...options, stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (chunk) => {
      stdout += chunk.toString();
    });
    child.stderr.on('data', (chunk) => {
      stderr += chunk.toString();
    });
    child.once('error', reject);
    child.once('exit', (code, signal) => {
      if (code === 0) resolvePromise({ stdout, stderr });
      else reject(new Error(`${command} exited with ${code ?? signal}\n${stderr || stdout}`));
    });
  });
}

function runAndCaptureWithInput(command, args, input, options = {}) {
  return new Promise((resolvePromise, reject) => {
    const child = run(command, args, { ...options, stdio: ['pipe', 'pipe', 'pipe'] });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (chunk) => { stdout += chunk.toString(); });
    child.stderr.on('data', (chunk) => { stderr += chunk.toString(); });
    child.once('error', reject);
    child.once('exit', (code, signal) => {
      if (code === 0) resolvePromise({ stdout, stderr });
      else reject(new Error(`${command} exited with ${code ?? signal}\n${stderr || stdout}`));
    });
    child.stdin.end(input);
  });
}

const environmentConfigPaths = [
  resolve(coreRoot, 'meta-secret/core/src/node/app/sync/environment.rs'),
  resolve(coreRoot, 'meta-secret/web-cli/ui/.env.local'),
  resolve(composeRoot, 'gradle.properties'),
  resolve(composeRoot, 'iosApp/Configuration/Config.xcconfig'),
];
const environmentArtifactRoots = [
  resolve(composeRoot, 'iosApp/Libs'),
  resolve(composeRoot, 'composeApp/src/androidMain/jniLibs'),
  resolve(composeRoot, 'iosApp/iosApp/UniffiGenerated'),
  resolve(composeRoot, 'iosApp/iosApp/MetaSecretCoreService/UniffiGenerated'),
  resolve(composeRoot, 'composeApp/src/androidMain/kotlin/com/metasecret/core/uniffi'),
];
const wasmArtifactRoot = resolve(coreRoot, 'meta-secret/web-cli/ui/pkg');
const iosBuildArtifactRoot = resolve(iosDerivedDataPath, 'Build');
const androidAppApkPath = resolve(
  composeRoot,
  'composeApp/build/outputs/apk/debug/composeApp-debug.apk',
);
const androidTestApkPath = resolve(
  composeRoot,
  'composeApp/build/outputs/apk/androidTest/debug/composeApp-debug-androidTest.apk',
);
const androidJniLibsRoot = resolve(composeRoot, 'composeApp/src/androidMain/jniLibs');
const androidArm64FfiPath = resolve(androidJniLibsRoot, 'arm64-v8a/libmetasecret_mobile.so');
const androidArm64PackagedFfiPath = resolve(
  composeRoot,
  'composeApp/build/intermediates/stripped_native_libs/debug/stripDebugDebugSymbols/out/lib/arm64-v8a/libmetasecret_mobile.so',
);
const androidFfiBuildScript = resolve(coreRoot, 'meta-secret/mobile/scripts/build-android.sh');
const androidFfiBuildProvenancePath = resolve(artifactsDirectory, 'android-ffi-build-provenance.json');
const androidFfiDiagnosticMarker = '[CORE_DIAG] sync_vault response events=';
let androidFfiPreparedForScenario = false;

function snapshotFile(filePath) {
  return existsSync(filePath)
    ? { path: filePath, existed: true, contents: readFileSync(filePath) }
    : { path: filePath, existed: false, contents: null };
}

function restoreFile(snapshot) {
  if (snapshot.existed) {
    writeFileSync(snapshot.path, snapshot.contents);
  } else {
    rmSync(snapshot.path, { force: true });
  }
}

function environmentModeFromSnapshot(snapshot) {
  if (!snapshot.existed) return null;
  const contents = snapshot.contents.toString('utf8');
  if (snapshot.path.endsWith('environment.rs')) {
    return contents.match(/SELECTED_SERVER_ENVIRONMENT[^\n]*ServerEnvironment::(Local|Remote)/)?.[1]?.toLowerCase()
      ?? null;
  }
  const key = snapshot.path.endsWith('.env.local') ? 'VITE_META_SECRET_ENV' : 'META_SECRET_ENV';
  return contents.match(new RegExp(`^${key}=(local|remote)$`, 'm'))?.[1] ?? null;
}

async function captureRepoDirtyState(repoRoot) {
  const [{ stdout: status }, { stdout: diff }, { stdout: untrackedRaw }] = await Promise.all([
    runAndCapture('git', ['status', '--porcelain=v1', '-z', '--untracked-files=all'], { cwd: repoRoot }),
    runAndCapture('git', ['diff', '--binary', '--no-ext-diff', 'HEAD', '--'], { cwd: repoRoot }),
    runAndCapture('git', ['ls-files', '--others', '--exclude-standard', '-z'], { cwd: repoRoot }),
  ]);
  const digest = createHash('sha256');
  digest.update(diff);
  const untracked = untrackedRaw.split('\0').filter(Boolean).sort();
  for (const relativePath of untracked) {
    digest.update(relativePath);
    const absolutePath = resolve(repoRoot, relativePath);
    if (existsSync(absolutePath)) digest.update(readFileSync(absolutePath));
  }
  return { status, digest: digest.digest('hex') };
}

function hashArtifactTree(rootPath) {
  const digest = createHash('sha256');
  if (!existsSync(rootPath)) {
    digest.update('absent');
    return digest.digest('hex');
  }

  const visit = (absolutePath, relativePath) => {
    const stats = lstatSync(absolutePath);
    const type = stats.isDirectory() ? 'directory' : stats.isSymbolicLink() ? 'symlink' : 'file';
    digest.update(`${relativePath}\0${type}\0${stats.mode & 0o7777}\0`);
    if (stats.isSymbolicLink()) {
      digest.update(readlinkSync(absolutePath));
      return;
    }
    if (stats.isDirectory()) {
      for (const entry of readdirSync(absolutePath).sort()) {
        visit(resolve(absolutePath, entry), relativePath ? `${relativePath}/${entry}` : entry);
      }
      return;
    }
    digest.update(readFileSync(absolutePath));
  };
  visit(rootPath, '.');
  return digest.digest('hex');
}

async function hashGitWorkingFiles(repoRoot, scopes) {
  const { stdout } = await runAndCapture(
    'git',
    ['ls-files', '-co', '--exclude-standard', '-z', '--', ...scopes],
    { cwd: repoRoot },
  );
  const digest = createHash('sha256');
  for (const relativePath of stdout.split('\0').filter(Boolean).sort()) {
    const absolutePath = resolve(repoRoot, relativePath);
    if (!existsSync(absolutePath)) continue;
    digest.update(`${relativePath}\0`);
    digest.update(readFileSync(absolutePath));
  }
  return digest.digest('hex');
}

async function currentAndroidFfiSourceFingerprint() {
  return hashGitWorkingFiles(coreRoot, ['meta-secret']);
}

function androidFfiSha256(buffer) {
  return createHash('sha256').update(buffer).digest('hex');
}

function extractAndroidApkFfi(apkPath) {
  return execFileSync('unzip', ['-p', apkPath, 'lib/arm64-v8a/libmetasecret_mobile.so'], {
    encoding: null,
    maxBuffer: 64 * 1024 * 1024,
  });
}

async function buildAndroidFfiFromCurrentSource() {
  if (androidFfiPreparedForScenario) return;
  if (process.env.E2E_ANDROID_FFI_PREBUILT === '1') {
    if (!existsSync(androidFfiBuildProvenancePath)) {
      throw new Error(`Android Core FFI preflight provenance not found: ${androidFfiBuildProvenancePath}`);
    }
    const prebuilt = JSON.parse(readFileSync(androidFfiBuildProvenancePath, 'utf8'));
    const sourceFingerprint = await currentAndroidFfiSourceFingerprint();
    if (prebuilt.sourceFingerprint !== sourceFingerprint) {
      throw new Error('Android Core FFI preflight source fingerprint does not match current Core source');
    }
    for (const [abi, folder] of [['arm64-v8a', 'arm64-v8a'], ['armeabi-v7a', 'armeabi-v7a'], ['x86_64', 'x86_64']]) {
      const path = resolve(androidJniLibsRoot, folder, 'libmetasecret_mobile.so');
      if (!existsSync(path)) throw new Error(`Android Core FFI preflight output missing for ABI ${abi}`);
      const library = readFileSync(path);
      if (!library.includes(Buffer.from(prebuilt.diagnosticMarker))
        || androidFfiSha256(library) !== prebuilt.abiSha256[abi]) {
        throw new Error(`Android Core FFI preflight library does not match recorded source provenance for ABI ${abi}`);
      }
    }
    androidFfiPreparedForScenario = true;
    writeDiagnostic(`[BUILD] using Core FFI built before simulator startup source=${sourceFingerprint} diagnosticMarker=true`);
    return;
  }
  if (!existsSync(androidFfiBuildScript)) {
    throw new Error(`Official Android FFI build script not found: ${androidFfiBuildScript}`);
  }

  writeDiagnostic('[BUILD] rebuilding Android Core FFI from current source with official build-android.sh');
  await runAndWait('bash', [androidFfiBuildScript], { cwd: coreRoot });

  if (!existsSync(androidArm64FfiPath)) {
    throw new Error(`Android Core FFI output missing after build: ${androidArm64FfiPath}`);
  }
  const builtNative = readFileSync(androidArm64FfiPath);
  if (!builtNative.includes(Buffer.from(androidFfiDiagnosticMarker))) {
    throw new Error('Fresh Android Core FFI is missing the required sync_vault diagnostic marker');
  }

  const sourceFingerprint = await currentAndroidFfiSourceFingerprint();
  const abiHashes = {};
  for (const abi of ['arm64-v8a', 'armeabi-v7a', 'x86_64']) {
    const libraryPath = resolve(androidJniLibsRoot, abi, 'libmetasecret_mobile.so');
    if (!existsSync(libraryPath)) throw new Error(`Android Core FFI output missing for ABI ${abi}`);
    const library = readFileSync(libraryPath);
    if (!library.includes(Buffer.from(androidFfiDiagnosticMarker))) {
      throw new Error(`Android Core FFI for ABI ${abi} is missing the sync_vault diagnostic marker`);
    }
    abiHashes[abi] = androidFfiSha256(library);
  }
  writeFileSync(androidFfiBuildProvenancePath, `${JSON.stringify({
    version: 1,
    builtAt: new Date().toISOString(),
    sourceFingerprint,
    diagnosticMarker: androidFfiDiagnosticMarker,
    abiSha256: abiHashes,
  }, null, 2)}\n`);
  androidFfiPreparedForScenario = true;
  writeDiagnostic(
    `[BUILD] Android Core FFI built from current source source=${sourceFingerprint} `
      + `arm64-v8a=${abiHashes['arm64-v8a']} armeabi-v7a=${abiHashes['armeabi-v7a']} `
      + `x86_64=${abiHashes.x86_64} diagnosticMarker=true`,
  );
}

async function verifyAndroidApkFfiProvenance() {
  if (!existsSync(androidAppApkPath) || !existsSync(androidArm64PackagedFfiPath)) {
    throw new Error('Android app/APK Core FFI outputs are missing; complete the Gradle debug build first');
  }
  if (!existsSync(androidFfiBuildProvenancePath)) {
    throw new Error(`Android Core FFI source provenance not found: ${androidFfiBuildProvenancePath}`);
  }

  const expected = JSON.parse(readFileSync(androidFfiBuildProvenancePath, 'utf8'));
  const currentSourceFingerprint = await currentAndroidFfiSourceFingerprint();
  if (expected.sourceFingerprint !== currentSourceFingerprint) {
    throw new Error('Android Core FFI source fingerprint changed after its native build');
  }
  const apkNative = extractAndroidApkFfi(androidAppApkPath);
  const strippedNative = readFileSync(androidArm64PackagedFfiPath);
  const apkNativeSha256 = androidFfiSha256(apkNative);
  const strippedNativeSha256 = androidFfiSha256(strippedNative);
  const diagnosticMarkerPresent = apkNative.includes(Buffer.from(expected.diagnosticMarker));
  const matchesGradleOutput = apkNativeSha256 === strippedNativeSha256;
  const matchesFreshFfi = apkNativeSha256 === expected.abiSha256['arm64-v8a'];

  writeDiagnostic(
    `[BUILD] Android packaged Core FFI source=${currentSourceFingerprint} `
      + `apkSha256=${apkNativeSha256} gradleStrippedSha256=${strippedNativeSha256} `
      + `freshJniLibSha256=${expected.abiSha256['arm64-v8a']} `
      + `matchesGradleOutput=${matchesGradleOutput} matchesFreshFfi=${matchesFreshFfi} `
      + `diagnosticMarker=${diagnosticMarkerPresent}`,
  );
  if (!matchesGradleOutput || !matchesFreshFfi || !diagnosticMarkerPresent) {
    throw new Error('Android APK does not contain the freshly built Core FFI for the current source');
  }
}

async function captureBuildProvenance() {
  const [coreSource, composeSource, androidFfiSource, image] = await Promise.all([
    hashGitWorkingFiles(coreRoot, ['Taskfile.yml', 'docker-bake.hcl', 'meta-secret']),
    hashGitWorkingFiles(
      composeRoot,
      ['settings.gradle.kts', 'build.gradle.kts', 'gradle.properties', 'gradle', 'composeApp', 'iosApp'],
    ),
    currentAndroidFfiSourceFingerprint(),
    runAndCapture('docker', ['image', 'inspect', serverImage, '--format', '{{.Id}}']),
  ]);
  const sourceFingerprint = createHash('sha256')
    .update(`core:${coreSource}\ncompose:${composeSource}\n`)
    .digest('hex');
  return {
    version: 1,
    sourceFingerprint,
    artifacts: {
      native: Object.fromEntries(
        environmentArtifactRoots.map((artifactPath) => [artifactPath, hashArtifactTree(artifactPath)]),
      ),
      androidFfiSourceFingerprint: androidFfiSource,
      androidJniLibs: hashArtifactTree(androidJniLibsRoot),
      wasm: hashArtifactTree(wasmArtifactRoot),
      iosBuild: hashArtifactTree(iosBuildArtifactRoot),
      androidAppApk: hashArtifactTree(androidAppApkPath),
      androidTestApk: hashArtifactTree(androidTestApkPath),
      serverImageId: image.stdout.trim(),
    },
  };
}

async function verifyOrBootstrapBuildProvenance() {
  const current = await captureBuildProvenance();
  if (bootstrapBuildProvenance) {
    const evidence = process.env.E2E_BUILD_PROVENANCE_EVIDENCE?.trim();
    if (!evidence) {
      throw new Error('E2E_BUILD_PROVENANCE_EVIDENCE is required to bootstrap artifact provenance');
    }
    writeFileSync(buildProvenancePath, `${JSON.stringify({
      ...current,
      capturedAt: new Date().toISOString(),
      evidence,
    }, null, 2)}\n`);
    writeDiagnostic(
      `[BUILD] bootstrapped exact artifact provenance source=${current.sourceFingerprint} evidence=${evidence}`,
    );
  } else if (!existsSync(buildProvenancePath)) {
    throw new Error(`Proven build artifact manifest not found: ${buildProvenancePath}`);
  }

  const expected = JSON.parse(readFileSync(buildProvenancePath, 'utf8'));
  if (
    expected.version !== current.version
    || expected.sourceFingerprint !== current.sourceFingerprint
    || JSON.stringify(expected.artifacts) !== JSON.stringify(current.artifacts)
  ) {
    throw new Error(
      `Build artifact provenance mismatch: expected source=${expected.sourceFingerprint ?? '<missing>'} `
        + `current source=${current.sourceFingerprint}`,
    );
  }
  buildProvenanceVerified = true;
  writeDiagnostic(
    `[BUILD] exact current-source artifact provenance verified source=${current.sourceFingerprint} `
      + `serverImage=${current.artifacts.serverImageId}`,
  );
}

async function recordFreshBuildProvenance() {
  const current = await captureBuildProvenance();
  writeFileSync(buildProvenancePath, `${JSON.stringify({
    ...current,
    capturedAt: new Date().toISOString(),
    evidence: 'full local Core/Compose/WASM/server/iOS/Android build completed by UI E2E runner',
  }, null, 2)}\n`);
  writeDiagnostic(`[BUILD] fresh artifact provenance recorded source=${current.sourceFingerprint}`);
}

function snapshotEnvironmentArtifactTrees() {
  const backupRoot = mkdtempSync(resolve(tmpdir(), 'metasecret-e2e-environment-'));
  try {
    const snapshots = environmentArtifactRoots.map((artifactPath, index) => {
      const existed = existsSync(artifactPath);
      const backupPath = resolve(backupRoot, String(index));
      if (existed) {
        cpSync(artifactPath, backupPath, {
          recursive: true,
          preserveTimestamps: true,
          verbatimSymlinks: true,
        });
      }
      return {
        path: artifactPath,
        backupPath,
        existed,
        hash: hashArtifactTree(artifactPath),
      };
    });
    return { backupRoot, snapshots };
  } catch (error) {
    rmSync(backupRoot, { recursive: true, force: true });
    throw error;
  }
}

function restoreEnvironmentArtifactTrees(artifactBackup) {
  for (const snapshot of artifactBackup.snapshots) {
    const normalizedRoot = `${resolve(composeRoot)}/`;
    if (!resolve(snapshot.path).startsWith(normalizedRoot)) {
      throw new Error(`Refusing to restore artifact path outside Compose: ${snapshot.path}`);
    }
    rmSync(snapshot.path, { recursive: true, force: true });
    if (snapshot.existed) {
      mkdirSync(dirname(snapshot.path), { recursive: true });
      cpSync(snapshot.backupPath, snapshot.path, {
        recursive: true,
        preserveTimestamps: true,
        verbatimSymlinks: true,
      });
    }
  }

  const mismatch = artifactBackup.snapshots.find((snapshot) => (
    existsSync(snapshot.path) !== snapshot.existed
      || hashArtifactTree(snapshot.path) !== snapshot.hash
  ));
  if (mismatch) {
    throw new Error(
      `native artifact restore hash mismatch: ${mismatch.path}; backup=${artifactBackup.backupRoot}`,
    );
  }
}

function assertPortAvailable(host, port, label) {
  return new Promise((resolvePromise, reject) => {
    const socket = createConnection({ host, port });
    const finishAvailable = () => {
      socket.destroy();
      resolvePromise();
    };
    socket.setTimeout(1_500, finishAvailable);
    socket.once('error', finishAvailable);
    socket.once('connect', () => {
      socket.destroy();
      reject(new Error(`${label} port ${host}:${port} is already owned by another process`));
    });
  });
}

async function assertLocalServicesAvailableForOwnership() {
  await runAndCapture('docker', ['info']);
  const inspectSucceeded = await runAndCapture('docker', ['inspect', serverContainer])
    .then(() => true)
    .catch((error) => {
      if (/No such object|No such container/i.test(error.message)) return false;
      throw error;
    });
  if (inspectSucceeded) {
    throw new Error(`Local server container ${serverContainer} already exists; refusing to take ownership`);
  }

  const webUrl = new URL(scenario.web.url);
  await Promise.all([
    assertPortAvailable('127.0.0.1', scenario.server.port, 'MetaSecret server'),
    assertPortAvailable(webUrl.hostname, Number(webUrl.port || 80), 'MetaSecret Web'),
    assertPortAvailable('127.0.0.1', 5180, 'approval coordinator'),
  ]);
  return {
    container: { name: serverContainer, existedBeforeRun: false },
    ports: [
      `127.0.0.1:${scenario.server.port}`,
      `${webUrl.hostname}:${Number(webUrl.port || 80)}`,
      '127.0.0.1:5180',
    ],
  };
}

async function preflightSelectedRuntimes() {
  androidSdkTools = discoverAndroidSdk();
  const { stdout: avdOutput } = await runAndCapture(androidSdkTools.emulatorPath, ['-list-avds']);
  const avds = avdOutput.split(/\r?\n/).map((name) => name.trim()).filter(Boolean);
  if (!avds.includes(scenario.android.avdName)) {
    throw new Error(
      `Android AVD "${scenario.android.avdName}" is unavailable. Installed AVDs: ${avds.join(', ') || '<none>'}. `
        + `Create it in Android Studio Device Manager or install it with ${androidSdkTools.emulatorPath} tooling.`,
    );
  }

  const { stdout: simulatorOutput } = await runAndCapture(
    'xcrun', ['simctl', 'list', 'devices', 'available', '-j'],
  );
  const simulators = Object.values(JSON.parse(simulatorOutput).devices).flat();
  const simulator = simulators.find((device) => device.name === scenario.ios.simulatorName);
  if (!simulator) {
    const availableNames = [...new Set(simulators.map((device) => device.name))];
    throw new Error(
      `iOS Simulator "${scenario.ios.simulatorName}" is unavailable. Available simulators: `
        + `${availableNames.join(', ') || '<none>'}. Install the required Xcode simulator runtime/device.`,
    );
  }

  const { stdout: developerDirectoryOutput } = await runAndCapture('xcode-select', ['-p']);
  const developerDirectory = developerDirectoryOutput.trim();
  const visibleUi = [
    resolve(developerDirectory, '../Applications/DeviceHub.app'),
    resolve(developerDirectory, 'Applications/Simulator.app'),
  ].find((candidate) => existsSync(candidate));
  if (!visibleUi) {
    throw new Error(`Neither DeviceHub.app nor Simulator.app exists for Xcode at ${developerDirectory}`);
  }

  const runtimeSnapshot = {
    android: {
      sdkRoot: androidSdkTools.sdkRoot,
      avd: scenario.android.avdName,
      onlineSerial: await findOnlineAndroidAvd(androidSdkTools, scenario.android.avdName, runAndCapture),
    },
    ios: { name: simulator.name, udid: simulator.udid, state: simulator.state, visibleUi },
  };
  writeDiagnostic(`[PREFLIGHT] runtimes ${JSON.stringify(runtimeSnapshot)}`);
  console.log(
    `✅ Local runtimes ready: Android AVD ${scenario.android.avdName}; `
      + `iOS ${simulator.name} (${simulator.state})`,
  );
  return runtimeSnapshot;
}

async function prepareLocalEnvironmentGuard() {
  const configSnapshots = environmentConfigPaths.map(snapshotFile);
  const modes = configSnapshots.map(environmentModeFromSnapshot).filter(Boolean);
  const uniqueModes = [...new Set(modes)];
  if (uniqueModes.length !== 1) {
    throw new Error(`Cannot snapshot a mixed MetaSecret environment: ${uniqueModes.join(',') || '<unknown>'}`);
  }
  const artifactBackup = snapshotEnvironmentArtifactTrees();
  let coreState;
  let composeState;
  try {
    [coreState, composeState] = await Promise.all([
      captureRepoDirtyState(coreRoot),
      captureRepoDirtyState(composeRoot),
    ]);
  } catch (error) {
    rmSync(artifactBackup.backupRoot, { recursive: true, force: true });
    throw error;
  }
  environmentGuard = {
    previousMode: uniqueModes[0],
    configSnapshots,
    artifactBackup,
    coreState,
    composeState,
    localApplied: false,
    restored: false,
  };
  writeDiagnostic(
    `[ENV] snapshot mode=${environmentGuard.previousMode}; `
      + `coreDirty=${coreState.status.length > 0}; composeDirty=${composeState.status.length > 0}; `
      + `nativeArtifactBackup=${artifactBackup.backupRoot}`,
  );

  environmentGuard.runtimeSnapshot = await preflightSelectedRuntimes();
  environmentGuard.serviceSnapshot = await assertLocalServicesAvailableForOwnership();
  writeDiagnostic(
    `[ENV] service ownership snapshot container=${serverContainer}:absent `
      + `ports=${environmentGuard.serviceSnapshot.ports.join(',')}:available`,
  );
  if (process.env.E2E_RANGE_RUN === '1') {
    if (uniqueModes[0] !== 'local') {
      throw new Error(`The range coordinator must set local mode before launching Test #${testNumber}; found ${uniqueModes[0]}`);
    }
    writeDiagnostic('[ENV] range coordinator already verified local mode; skipping duplicate task set-env rebuild');
    return;
  }
  if (reuseProvenArtifacts) {
    if (uniqueModes[0] !== 'local') {
      throw new Error(`Proven artifact reuse requires an already-local environment, got ${uniqueModes[0]}`);
    }
    await verifyOrBootstrapBuildProvenance();
    writeDiagnostic(
      '[ENV] local selectors and exact current-source artifact hashes proven; '
        + 'skipping redundant task set-env -- local rebuild',
    );
    return;
  }
  // Mark the transition before invoking the command: even a partial failure
  // can have rewritten config or native artifacts and therefore requires the
  // previous-mode restoration path.
  environmentGuard.localApplied = true;
  await runAndWait('/opt/homebrew/bin/task', ['set-env', '--', 'local'], { cwd: coreRoot });
  const localModes = environmentConfigPaths.map(snapshotFile).map(environmentModeFromSnapshot).filter(Boolean);
  if (localModes.length !== environmentConfigPaths.length || localModes.some((mode) => mode !== 'local')) {
    throw new Error(`Local environment verification failed: ${localModes.join(',')}`);
  }
  writeDiagnostic('[ENV] local mode applied and verified through task set-env -- local');
}

async function restoreEnvironmentGuard() {
  if (!environmentGuard || environmentGuard.restored) return;
  environmentGuard.restored = true;
  let rebuildError;
  let artifactRestoreError;
  try {
    if (environmentGuard.localApplied && environmentGuard.previousMode === 'remote') {
      await runAndWait('/opt/homebrew/bin/task', ['set-env', '--', 'remote'], { cwd: coreRoot });
    }
  } catch (error) {
    rebuildError = error;
  } finally {
    for (const snapshot of environmentGuard.configSnapshots) restoreFile(snapshot);
    try {
      restoreEnvironmentArtifactTrees(environmentGuard.artifactBackup);
    } catch (error) {
      artifactRestoreError = error;
    }
    rmSync(iosStepConfigPath, { force: true });
  }

  const mismatchedConfig = environmentGuard.configSnapshots.find((snapshot) => {
    if (snapshot.existed !== existsSync(snapshot.path)) return true;
    return snapshot.existed && !readFileSync(snapshot.path).equals(snapshot.contents);
  });
  const [coreState, composeState] = await Promise.all([
    captureRepoDirtyState(coreRoot),
    captureRepoDirtyState(composeRoot),
  ]);
  const stateMismatch = coreState.status !== environmentGuard.coreState.status
    || coreState.digest !== environmentGuard.coreState.digest
    || composeState.status !== environmentGuard.composeState.status
    || composeState.digest !== environmentGuard.composeState.digest;
  if (rebuildError || artifactRestoreError || mismatchedConfig || stateMismatch) {
    const reasons = [
      rebuildError ? `mode rebuild failed: ${rebuildError.message}` : null,
      artifactRestoreError ? artifactRestoreError.message : null,
      mismatchedConfig ? `config mismatch: ${mismatchedConfig.path}` : null,
      stateMismatch ? 'dirty working-tree fingerprint changed' : null,
    ].filter(Boolean);
    throw new Error(`MetaSecret environment restoration blocker: ${reasons.join('; ')}`);
  }
  rmSync(environmentGuard.artifactBackup.backupRoot, { recursive: true, force: true });
  writeDiagnostic(`[ENV] restored exact mode=${environmentGuard.previousMode}; repository state preserved`);
}

async function runMetaCli(cliDirectory, args, {
  label = args.join(' '),
  suppressOutput = false,
} = {}) {
  if (!existsSync(metaCliBinaryPath)) {
    throw new Error(
      `meta-cli binary not found at ${metaCliBinaryPath}. `
      + 'Build it with: cargo build -p meta-cli',
    );
  }

  writeDiagnostic(`[CLI] ${label}; cwd=${cliDirectory}`);
  const result = await runAndCapture(
    metaCliBinaryPath,
    ['--output-format', 'json', ...args],
    { cwd: cliDirectory },
  );
  if (!suppressOutput && result.stderr.trim()) {
    writeDiagnostic(`[CLI][stderr] ${result.stderr.trim().slice(-4_000)}`);
  }
  if (!suppressOutput && result.stdout.trim()) {
    writeDiagnostic(`[CLI][stdout] ${result.stdout.trim().slice(-4_000)}`);
  }
  return result;
}

async function runMetaCliWithInput(cliDirectory, args, input, { label = args.join(' ') } = {}) {
  if (!existsSync(metaCliBinaryPath)) {
    throw new Error(
      `meta-cli binary not found at ${metaCliBinaryPath}. `
      + 'Build it with: cargo build -p meta-cli',
    );
  }

  writeDiagnostic(`[CLI] ${label}; cwd=${cliDirectory}`);
  const result = await runAndCaptureWithInput(
    metaCliBinaryPath,
    ['--output-format', 'json', ...args],
    input,
    { cwd: cliDirectory },
  );
  if (result.stderr.trim()) writeDiagnostic(`[CLI][stderr] ${result.stderr.trim().slice(-4_000)}`);
  if (result.stdout.trim()) writeDiagnostic(`[CLI][stdout] ${result.stdout.trim().slice(-4_000)}`);
  return result;
}

async function createMetaCliRootDevice() {
  const cliDirectory = resolve(
    artifactsDirectory,
    `test21-${scenario.runId ?? 'run'}-cli-${process.pid}-${Date.now()}`,
  );
  mkdirSync(cliDirectory, { recursive: true });
  const deviceName = `test21-cli-${scenario.runId ?? 'run'}`;
  const deviceInit = await runMetaCli(
    cliDirectory,
    ['init', 'device', '--device-name', deviceName],
    { label: `init root CLI device ${deviceName}` },
  );
  const deviceId = deviceInit.stdout.match(/Device ID:\s*([^\s]+)/)?.[1];
  if (!deviceId) throw new Error(`Could not parse CLI root device ID: ${deviceInit.stdout}`);
  await runMetaCli(
    cliDirectory,
    ['init', 'user', '--vault-name', scenario.vault.name],
    { label: `init root CLI vault=${scenario.vault.name}` },
  );
  await runMetaCli(cliDirectory, ['auth', 'sign-up'], { label: 'create CLI root vault' });
  const secret = defaultSecret;
  if (secret?.name && secret?.value) {
    await runMetaCliWithInput(
      cliDirectory,
      ['secret', 'split', '--pass-name', secret.name, '--stdin'],
      `${secret.value}\n`,
      { label: `create CLI secret ${secret.name}` },
    );
  }
  writeDiagnostic(`[CLI] root vault ready device=${deviceId} name=${deviceName}`);
  return { cliDirectory, deviceId, deviceName };
}

async function acceptNextCliJoin(cliDevice, platform, timeoutMs = 180_000) {
  const deadline = Date.now() + timeoutMs;
  let lastOutput = '';
  while (Date.now() < deadline) {
    const result = await runMetaCli(cliDevice.cliDirectory, ['auth', 'accept-all-join-requests'], {
      label: `accept pending ${platform} join request`,
    });
    lastOutput = `${result.stdout}\n${result.stderr}`;
    if (/Accepted join request|Successfully accepted\s+[1-9]/i.test(lastOutput)) {
      writeDiagnostic(`[CLI] accepted ${platform} join request`);
      // The CLI writes the membership update to its local log first. Opening
      // the client once more flushes that event through the sync gateway so
      // the joined device can observe the accepted membership immediately.
      await runMetaCli(cliDevice.cliDirectory, ['info', 'secrets'], {
        label: `flush ${platform} membership update`,
      });
      return;
    }
    await wait(500);
  }
  throw new Error(`Timed out waiting for CLI to accept ${platform} join request: ${lastOutput.slice(-2_000)}`);
}

async function createMetaCliDevice(cycle) {
  const cliDirectory = resolve(
    artifactsDirectory,
    `test15-${scenario.runId ?? 'run'}-cli-${process.pid}-${Date.now()}-${cycle.number}`,
  );
  mkdirSync(cliDirectory, { recursive: true });
  const deviceName = `test15-cli-${cycle.number}`;
  const deviceInit = await runMetaCli(
    cliDirectory,
    ['init', 'device', '--device-name', deviceName],
    { label: `init device ${deviceName}` },
  );
  const deviceId = deviceInit.stdout.match(/Device ID:\s*([^\s]+)/)?.[1];
  if (!deviceId) {
    throw new Error(`Could not parse CLI device ID from init output: ${deviceInit.stdout}`);
  }

  await runMetaCli(
    cliDirectory,
    ['init', 'user', '--vault-name', scenario.vault.name],
    { label: `init user vault=${scenario.vault.name}` },
  );
  await runMetaCli(cliDirectory, ['auth', 'sign-up'], { label: 'submit CLI join request' });
  writeDiagnostic(`[CLI] join request submitted device=${deviceId} name=${deviceName}`);
  return { cliDirectory, deviceId, deviceName };
}

function parseCliJson(result, label) {
  try {
    // Core diagnostic markers may precede CLI JSON on stdout in local E2E
    // builds. The command's structured payload starts at its first JSON object.
    const jsonStart = result.stdout.indexOf('{');
    if (jsonStart < 0) throw new Error('no JSON object in stdout');
    return JSON.parse(result.stdout.slice(jsonStart));
  } catch (error) {
    throw new Error(
      `CLI ${label} returned invalid JSON: ${error.message}; `
      + `stdout=${result.stdout.slice(-4_000)}`,
    );
  }
}

async function readMetaCliInfo(cliDevice, command = ['info', 'default'], options = {}) {
  const result = await runMetaCli(cliDevice.cliDirectory, command, {
    label: command.join(' '),
    ...options,
  });
  return parseCliJson(result, command.join(' '));
}

async function assertMetaCliMember(cliDevice, expectedMemberCount) {
  // `info default` is intentionally not used here: its JSON template still
  // expects a legacy claim.status field and fails as soon as recovery claims
  // are present. A redistributed Split claim is the protocol-level proof
  // that the new member was accepted and received a vault copy.
  const claimsInfo = await readMetaCliInfo(cliDevice, ['info', 'recovery-claims']);
  const redistributed = (claimsInfo.claims ?? []).find((claim) => (
    claim.type === 'Split'
      && claim.receivers?.some((receiver) => receiver.id === cliDevice.deviceId)
  ));
  if (!redistributed) {
    throw new Error(
      `CLI did not observe accepted membership: no Split claim contains device=${cliDevice.deviceId}; `
      + `claims=${JSON.stringify(claimsInfo).slice(0, 2_000)}`,
    );
  }
  writeDiagnostic(
    `[CLI] member state observed device=${cliDevice.deviceId} `
    + `redistributedClaim=${redistributed.id}`,
  );
  return claimsInfo;
}

function watchProcessOutput(command, args, options = {}) {
  const {
    displayArgs: requestedDisplayArgs,
    diagnosticRole,
    ...processOptions
  } = options;
  let stdoutTail = '';
  let stderrTail = '';
  const markerScanTails = { stdout: '', stderr: '' };
  const redactionStreams = {
    stdout: diagnosticRedactor.createStream(),
    stderr: diagnosticRedactor.createStream(),
  };
  const markerWaiters = new Set();
  let markerWaiterSequence = 0;

  const child = run(command, args, { ...processOptions, stdio: ['ignore', 'pipe', 'pipe'] });
  const emitSafeOutput = (sink, text, flush = false) => {
    const safeText = flush
      ? redactionStreams[sink].flush()
      : redactionStreams[sink].push(text);
    if (!safeText) return;
    if (sink === 'stdout') stdoutTail = appendBounded(stdoutTail, safeText);
    else stderrTail = appendBounded(stderrTail, safeText);
    const diagnosticText = filterVerboseWatchedOutput(command, sink, safeText);
    if (diagnosticText) {
      process[sink].write(diagnosticText);
      appendFileSync(diagnosticLogPath, `[${command} ${sink}] ${diagnosticText}`);
    }
  };
  const processOutput = (chunk, sink) => {
    const text = chunk.toString();
    const previousMarkerScanTail = markerScanTails[sink];
    markerScanTails[sink] = appendBounded(previousMarkerScanTail, text);
    emitSafeOutput(sink, text);
    for (const waiter of markerWaiters) {
      const scan = waiter.nextOnly
        ? markerScanStream(
          waiter.pendingByStream,
          sink,
          text,
          waiter.marker,
          maxWatchedOutputChars,
        )
        : markerScan(previousMarkerScanTail, text, waiter.marker, maxWatchedOutputChars);
      if (waiter.nextOnly) waiter.pendingByStream = scan.pendingByStream;
      const matches = scan.matched;
      if (diagnosticRole === 'e2e-server'
          && String(waiter.marker) === String(ssClaimsInvalidationMarker)
          && (text.includes('Publishing state invalidation') || text.includes('scope=Some('))) {
        writeDiagnostic(
          `SERVER_MARKER_SCAN waiter=${waiter.id} stream=${sink} `
          + `bytes=${Buffer.byteLength(text)} invalidation_prefix=${text.includes('Publishing state invalidation')} `
          + `claims_scope=${text.includes('scope=Some(SsClaims)')} candidate_match=${matches}`,
        );
      }
      if (matches) {
        if (diagnosticRole === 'e2e-server'
            && String(waiter.marker) === String(ssClaimsInvalidationMarker)) {
          writeDiagnostic(`SERVER_MARKER_RESOLVED waiter=${waiter.id} stream=${sink}`);
        }
        clearTimeout(waiter.timeout);
        markerWaiters.delete(waiter);
        waiter.resolve();
      }
    }
  };

  child.stdout.on('data', (chunk) => processOutput(chunk, 'stdout'));
  child.stderr.on('data', (chunk) => processOutput(chunk, 'stderr'));
  child.stdout.on('end', () => emitSafeOutput('stdout', '', true));
  child.stderr.on('end', () => emitSafeOutput('stderr', '', true));

  let processError;
  const result = new Promise((resolvePromise, reject) => {
    child.once('error', reject);
    child.once('exit', (code, signal) => {
      processError = code === 0 ? new Error(`${command} ended before expected E2E event`) : new Error(`${command} exited with ${code ?? signal}`);
      for (const waiter of markerWaiters) {
        clearTimeout(waiter.timeout);
        waiter.reject(processError);
      }
      markerWaiters.clear();
      if (code === 0) resolvePromise({ stdout: stdoutTail, stderr: stderrTail });
      else reject(processError);
    });
  });
  // `exit` can be emitted before all stdio handles are closed. Consumers that
  // immediately launch another native build need the stronger `close`
  // boundary so a terminated wrapper cannot leave nested build work racing
  // with the next invocation.
  const closed = new Promise((resolvePromise) => {
    child.once('close', (code, signal) => resolvePromise({ code, signal }));
  });

  const waitForMarker = (marker, timeoutMs = 300_000) => new Promise((resolvePromise, reject) => {
    const matches = [markerScanTails.stdout, markerScanTails.stderr].some((output) => (
      marker instanceof RegExp
        ? (marker.lastIndex = 0, marker.test(output))
        : output.includes(marker)
    ));
    if (matches) return resolvePromise();
    if (processError) return reject(processError);
    const waiter = {
      marker,
      resolve: resolvePromise,
      reject: reject,
      timeout: setTimeout(() => {
        markerWaiters.delete(waiter);
        reject(new Error(`Timed out waiting for ${String(marker)}`));
      }, timeoutMs),
    };
    markerWaiters.add(waiter);
  });

  const waitForNextMarker = (marker, timeoutMs = 300_000) => new Promise((resolvePromise, reject) => {
    if (processError) return reject(processError);
    const waiterId = ++markerWaiterSequence;
    const armedAt = Date.now();
    const waiter = {
      id: waiterId,
      armedAt,
      marker,
      nextOnly: true,
      pendingByStream: { stdout: '', stderr: '' },
      resolve: resolvePromise,
      reject,
      timeout: setTimeout(() => {
        markerWaiters.delete(waiter);
        if (diagnosticRole === 'e2e-server'
            && String(marker) === String(ssClaimsInvalidationMarker)) {
          writeDiagnostic(
            `SERVER_MARKER_TIMEOUT waiter=${waiterId} elapsedMs=${Date.now() - armedAt} `
            + `stdout_pending_bytes=${Buffer.byteLength(waiter.pendingByStream.stdout)} `
            + `stderr_pending_bytes=${Buffer.byteLength(waiter.pendingByStream.stderr)}`,
          );
        }
        reject(new Error(`Timed out waiting for new output matching ${String(marker)}`));
      }, timeoutMs),
    };
    markerWaiters.add(waiter);
    if (diagnosticRole === 'e2e-server'
        && String(marker) === String(ssClaimsInvalidationMarker)) {
      writeDiagnostic(
        `SERVER_MARKER_ARMED waiter=${waiterId} timeoutMs=${timeoutMs} `
        + `stdout_seen_bytes=${Buffer.byteLength(markerScanTails.stdout)} `
        + `stderr_seen_bytes=${Buffer.byteLength(markerScanTails.stderr)}`,
      );
    }
  });

  const watched = {
    command,
    displayArgs: redactCommandArguments(requestedDisplayArgs ?? args),
    displayEnvironment: redactEnvironment(processOptions.env),
    child,
    waitForMarker,
    waitForNextMarker,
    result,
    closed,
    cancelPendingWaits(error) {
      processError ??= error;
      for (const waiter of markerWaiters) {
        clearTimeout(waiter.timeout);
        waiter.reject(processError);
      }
      markerWaiters.clear();
    },
    outputTail: () => `${stdoutTail}\n${stderrTail}`.slice(-12_000),
  };
  watchedProcessOutputs.push(watched);
  return watched;
}

function watchAndroidInstrumentation(serial, instrumentationArguments, runnerComponent) {
  return watchProcessOutput(
    'adb',
    androidInstrumentationAdbArguments(serial, instrumentationArguments, runnerComponent),
    {
      // Raw values are used only for process execution. Diagnostics receive a
      // separately redacted logical argv serialized after values are removed,
      // so POSIX quote fragments can never reconstruct sensitive material.
      displayArgs: androidInstrumentationDiagnosticArguments(
        serial,
        instrumentationArguments,
        runnerComponent,
        redactCommandArguments,
      ),
    },
  );
}

function printFailureDiagnostics() {
  for (const watched of watchedProcessOutputs) {
    const tail = watched.outputTail();
    if (tail.trim()) {
      const environment = Object.keys(watched.displayEnvironment).length > 0
        ? ` env=${JSON.stringify(watched.displayEnvironment)}`
        : '';
      const heading = `${watched.command} ${watched.displayArgs.join(' ')}${environment}`;
      console.error(`\n=== E2E diagnostic: ${heading} ===\n${tail}`);
      appendFileSync(diagnosticLogPath, `\n=== E2E diagnostic: ${heading} ===\n${tail}\n`);
    }
  }
  if (browserDiagnostics.length > 0) {
    console.error(`\n=== E2E diagnostic: Web browser ===\n${browserDiagnostics.slice(-200).join('\n')}`);
    appendFileSync(diagnosticLogPath, `\n=== E2E diagnostic: Web browser ===\n${browserDiagnostics.slice(-200).join('\n')}\n`);
  }
}

async function waitForHttp(url, timeoutMs = 120_000) {
  const deadline = Date.now() + timeoutMs;
  let lastError;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url);
      if (response.ok || response.status < 500) return;
    } catch (error) {
      lastError = error;
    }
    await wait(500);
  }
  throw new Error(`Timed out waiting for ${url}: ${lastError?.message ?? 'server unavailable'}`);
}

function processIsAlive(pid) {
  if (!Number.isInteger(pid) || pid <= 1 || pid === process.pid) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return error.code !== 'ESRCH';
  }
}

function readProcessTable() {
  return new Promise((resolvePromise, reject) => {
    const child = spawn('ps', ['-axo', 'pid=,ppid='], {
      cwd: projectRoot,
      env: { ...process.env, PATH: `/opt/homebrew/bin:/usr/local/bin:${process.env.PATH ?? ''}` },
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (chunk) => { stdout += chunk.toString(); });
    child.stderr.on('data', (chunk) => { stderr += chunk.toString(); });
    child.once('error', reject);
    child.once('close', (code, signal) => {
      if (code !== 0) {
        reject(new Error(`ps exited with ${code ?? signal}: ${stderr}`));
        return;
      }
      resolvePromise(stdout.split('\n').flatMap((line) => {
        const [pid, parentPid] = line.trim().split(/\s+/).map(Number);
        return Number.isInteger(pid) && Number.isInteger(parentPid) ? [{ pid, parentPid }] : [];
      }));
    });
  });
}

async function ownedProcessTreePids() {
  const rootPids = new Set(
    processes
      .filter((child) => child.pid && child.exitCode == null && child.signalCode == null)
      .map((child) => child.pid)
      .filter((pid) => processIsAlive(pid)),
  );
  if (rootPids.size === 0) return [];
  const table = await readProcessTable();
  const depthByPid = new Map([...rootPids].map((pid) => [pid, 0]));
  let changed = true;
  while (changed) {
    changed = false;
    for (const { pid, parentPid } of table) {
      if (depthByPid.has(pid) || !depthByPid.has(parentPid) || pid === process.pid) continue;
      depthByPid.set(pid, depthByPid.get(parentPid) + 1);
      changed = true;
    }
  }
  return [...depthByPid.entries()]
    .sort((left, right) => right[1] - left[1])
    .map(([pid]) => pid);
}

async function waitForPidsGone(pids, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  let remaining = pids.filter(processIsAlive);
  while (remaining.length > 0 && Date.now() < deadline) {
    await wait(Math.min(100, Math.max(1, deadline - Date.now())));
    remaining = remaining.filter(processIsAlive);
  }
  return remaining;
}

async function terminateOwnedProcessTree() {
  // A cancelled wrapper may exit before its descendants; retain their owned
  // identities so reparenting cannot hide a still-running native test/build.
  const pids = [...new Set([
    ...cancelledActorPids.filter(processIsAlive),
    ...await ownedProcessTreePids(),
  ])];
  for (const pid of pids) {
    try {
      process.kill(pid, 'SIGTERM');
    } catch (error) {
      if (error.code !== 'ESRCH') throw error;
    }
  }
  let remaining = await waitForPidsGone(pids, 10_000);
  for (const pid of remaining) {
    try {
      process.kill(pid, 'SIGKILL');
    } catch (error) {
      if (error.code !== 'ESRCH') throw error;
    }
  }
  remaining = await waitForPidsGone(remaining, 10_000);
  if (remaining.length > 0) {
    throw new Error(`Harness process tree did not terminate: pids=${remaining.join(',')}`);
  }

  const stillOpen = processes.filter((child) => (
    child.pid && child.exitCode == null && child.signalCode == null && processIsAlive(child.pid)
  ));
  if (stillOpen.length > 0) {
    throw new Error(`Tracked child handles remained active: pids=${stillOpen.map((child) => child.pid).join(',')}`);
  }
  writeDiagnostic(`[CLEANUP] process tree terminated roots=${pids.length}`);
}

async function performCleanup() {
  // Stop/wait for every owned platform actor before closing the visible Web
  // session or shutting down simulators. On success their result promises
  // have already settled; on failure this cancels siblings and waits for the
  // owned process tree to exit before runtime teardown.
  await terminateOwnedProcessTree();
  await browserInstance?.close().catch(() => {});
  browserInstance = undefined;
  for (const gate of networkGateProcesses.values()) await gate.close().catch(() => {});
  networkGateProcesses.clear();
  await approvalCoordinator?.close().catch(() => {});
  approvalCoordinator = undefined;
  if (exitOnSuccess) {
    await runAndWait('./gradlew', ['--stop'], {
      cwd: composeRoot,
      stdio: 'ignore',
    }).catch(() => {});
  }
  if (serverContainerOwned) {
    await runAndWait('docker', ['rm', '-f', serverContainer], { stdio: 'ignore' }).catch(() => {});
    serverContainerOwned = false;
  }
  await restoreEnvironmentGuard();
}

const harnessShutdown = createHarnessShutdown({
  async cancelActors(error = new Error('Scenario actors finished')) {
    for (const watched of watchedProcessOutputs) watched.cancelPendingWaits(error);
    cancelledActorPids = await ownedProcessTreePids();
    for (const pid of cancelledActorPids) {
      try { process.kill(pid, 'SIGTERM'); }
      catch (failure) { if (failure.code !== 'ESRCH') throw failure; }
    }
  },
  async captureDiagnostics() {
    // Capture complete output before sanitized screenshots touch the apps.
    printFailureDiagnostics();
    await captureFailureScreenshots();
  },
  cleanup: performCleanup,
});

function stopProcesses() {
  return harnessShutdown.finish();
}

async function captureIosScreenshot(simulatorUdid, filePath) {
  await runAndWait('xcrun', ['simctl', 'io', simulatorUdid, 'screenshot', filePath]);
  writeDiagnostic(`[SCREENSHOT] iOS saved ${filePath}`);
}

function captureAndroidScreenshot(serial, filePath) {
  return new Promise((resolvePromise, reject) => {
    const child = run('adb', ['-s', serial, 'exec-out', 'screencap', '-p'], {
      cwd: projectRoot,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    const chunks = [];
    let stderr = '';
    child.stdout.on('data', (chunk) => chunks.push(chunk));
    child.stderr.on('data', (chunk) => { stderr += chunk.toString(); });
    child.once('error', reject);
    child.once('exit', (code, signal) => {
      if (code !== 0) {
        reject(new Error(`adb screenshot exited with ${code ?? signal}: ${stderr}`));
        return;
      }
      writeFileSync(filePath, Buffer.concat(chunks));
      writeDiagnostic(`[SCREENSHOT] Android saved ${filePath}`);
      resolvePromise();
    });
  });
}

async function sanitizeIosForScreenshot(simulatorUdid) {
  await runAndCapture(
    'xcrun',
    ['simctl', 'terminate', simulatorUdid, scenario.ios.bundleId],
  ).catch(() => {});
  await runAndCapture(
    'xcrun',
    ['simctl', 'launch', simulatorUdid, 'com.apple.Preferences'],
  );
  // A still-running XCTest may race the first termination. Terminate the
  // target once more after Settings owns the foreground; a not-running result
  // is safe and expected.
  await runAndCapture(
    'xcrun',
    ['simctl', 'terminate', simulatorUdid, scenario.ios.bundleId],
  ).catch(() => {});
  writeDiagnostic('[SCREENSHOT] iOS sanitized: target terminated; Settings foregrounded');
  return 'target-terminated-and-system-settings-foregrounded';
}

async function sanitizeAndroidForScreenshot(serial) {
  await runAndCapture('adb', [
    '-s', serial, 'shell', 'am', 'force-stop', scenario.android.bundleId,
  ]);
  await runAndCapture('adb', [
    '-s', serial, 'shell', 'am', 'start', '-W', '-a', 'android.settings.SETTINGS',
  ]);
  const deadline = Date.now() + 15_000;
  let lastFocus = '<unknown>';
  while (Date.now() < deadline) {
    const { stdout } = await runAndCapture('adb', [
      '-s', serial, 'shell', 'dumpsys', 'window', 'windows',
    ]);
    lastFocus = stdout.match(/mCurrentFocus[^\n]*/)?.[0] ?? '<missing-current-focus>';
    if (/com\.android\.settings/i.test(lastFocus) && !lastFocus.includes(scenario.android.bundleId)) {
      writeDiagnostic('[SCREENSHOT] Android sanitized: target force-stopped; Settings foregrounded');
      return 'target-force-stopped-and-system-settings-foregrounded';
    }
    await wait(100);
  }
  throw new Error(`Android screenshot sanitization did not reach Settings: ${lastFocus}`);
}

async function captureFailureScreenshots() {
  mkdirSync(artifactsDirectory, { recursive: true });
  const cycle = currentCycleForDiagnostics ?? 'setup';
  const baseName = `test-${testNumber}-${scenario.runId ?? 'run'}-attempt-${process.env.E2E_TEST_ATTEMPT ?? '1'}-${scenarioArtifactLabel}-cycle-${cycle}-failure`;
  const captures = [];

  const pages = browserInstance?.contexts().flatMap((context) => context.pages()) ?? [];
  const page = [...pages].reverse().find((candidate) => !candidate.isClosed());
  if (page) {
    const filePath = resolve(artifactsDirectory, `${baseName}-web.png`);
    rmSync(filePath, { force: true });
    try {
      await page.screenshot({
        path: filePath,
        fullPage: true,
        mask: sensitiveDiagnosticValues.map((value) => page.getByText(value, { exact: true })),
      });
      captures.push({
        platform: 'web',
        path: filePath,
        status: 'captured-and-masked',
        sanitizationMethod: 'playwright-exact-secret-value-mask',
      });
      writeDiagnostic(`[SCREENSHOT] Web saved ${filePath}`);
    } catch (error) {
      captures.push({ platform: 'web', status: 'failed', error: redactSensitiveText(error.message) });
    }
  } else {
    captures.push({ platform: 'web', status: 'unavailable' });
  }

  if (simulatorUdidForCleanup) {
    const filePath = resolve(artifactsDirectory, `${baseName}-ios.png`);
    rmSync(filePath, { force: true });
    try {
      const sanitizationMethod = await sanitizeIosForScreenshot(simulatorUdidForCleanup);
      await captureIosScreenshot(simulatorUdidForCleanup, filePath);
      captures.push({
        platform: 'ios',
        path: filePath,
        status: 'captured-sanitized',
        sanitizationMethod,
      });
    } catch (error) {
      captures.push({ platform: 'ios', status: 'failed', error: redactSensitiveText(error.message) });
    }
  } else {
    captures.push({ platform: 'ios', status: 'unavailable' });
  }

  if (androidSerialForCleanup) {
    const filePath = resolve(artifactsDirectory, `${baseName}-android.png`);
    rmSync(filePath, { force: true });
    try {
      const sanitizationMethod = await sanitizeAndroidForScreenshot(androidSerialForCleanup);
      await captureAndroidScreenshot(androidSerialForCleanup, filePath);
      captures.push({
        platform: 'android',
        path: filePath,
        status: 'captured-sanitized',
        sanitizationMethod,
      });
    } catch (error) {
      captures.push({ platform: 'android', status: 'failed', error: redactSensitiveText(error.message) });
    }
  } else {
    captures.push({ platform: 'android', status: 'unavailable' });
  }

  const manifestPath = resolve(artifactsDirectory, `${baseName}-screenshots.json`);
  writeFileSync(manifestPath, `${JSON.stringify({ testNumber, cycle, captures }, null, 2)}\n`);
  writeDiagnostic(`[SCREENSHOT] manifest saved ${manifestPath}`);
  return captures;
}

async function findIosSimulator() {
  const { stdout } = await runAndCapture('xcrun', ['simctl', 'list', 'devices', 'available', '-j']);
  const devicesByRuntime = JSON.parse(stdout).devices;
  const devices = Object.values(devicesByRuntime).flat();
  const device = devices.find((candidate) => candidate.name === scenario.ios.simulatorName);
  if (!device) throw new Error(`iOS simulator not found: ${scenario.ios.simulatorName}`);
  return device;
}

async function openIosDeviceUi(simulatorUdid) {
  const applicationPath = await openVisibleIosDeviceUi({ runAndCapture, runAndWait, simulatorUdid });
  writeDiagnostic(`[IOS] visible device UI opened path=${applicationPath}`);
}

async function prepareIosSimulator() {
  const simulator = await findIosSimulator();
  simulatorUdidForCleanup = simulator.udid;
  console.log(`8. Preparing iOS simulator: ${simulator.name} (${simulator.udid})`);
  if (simulator.state !== 'Booted') {
    await runAndWait('xcrun', ['simctl', 'boot', simulator.udid]);
  }
  await runAndWait('xcrun', ['simctl', 'bootstatus', simulator.udid, '-b']);
  await resetE2eNativeAppState({
    scenario,
    platforms: [{ platform: 'iOS', simulatorName: simulator.name, udid: simulator.udid }],
    onEvent: (message) => {
      console.log(`🧹 ${message}`);
      writeDiagnostic(`[BLOCK APP RESET] ${message}`);
    },
  });
  await openIosDeviceUi(simulator.udid);
  return simulator.udid;
}

async function prepareIosTestBundle(simulatorUdid) {
  const nativeInputs = await prepareIosXcodeBuild({ coreRoot, composeRoot, derivedDataPath: iosDerivedDataPath, runAndCapture });
  console.log('8a. Building iOS app and UI tests once for the full scenario');
  const xcodebuildArgs = [
    'build-for-testing',
    '-project',
    iosProjectPath,
    '-scheme',
    'iosApp',
    '-configuration',
    'Debug',
    '-destination',
    `id=${simulatorUdid}`,
    '-derivedDataPath',
    iosDerivedDataPath,
  ];
  if (networkLossConfig) {
    xcodebuildArgs.push(
      `META_SECRET_SOCKET_URL=${networkProxyUrl('ios')}`,
      'META_SECRET_ENV=local',
    );
  }
  await runAndWait('xcodebuild', xcodebuildArgs, {
    cwd: resolve(composeRoot, 'iosApp'),
    env: { GRADLE_USER_HOME: iosGradleUserHome },
  });
  recordIosXcodeBuild({ coreRoot, composeRoot, derivedDataPath: iosDerivedDataPath, inputs: nativeInputs });
  writeDiagnostic('[IOS] build-for-testing completed; subsequent steps use test-without-building');
}

function startIosJoinTest(simulatorUdid, {
  testClass = scenario.ios.testClass ?? 'CaseFourIosConcurrentRecoveryUITest',
  testMethod = iosTestMethod,
  label = 'iOS UI test',
  environment = {},
  secretName = '',
  secretValue = null,
  expectedDeviceNames = null,
} = {}) {
  const selectedSecret = bindIosStepSecret({
    secretName, secretValue, environment, defaultSecret,
    valueForName: (name) => Object.values(secretConfigs).find((config) => config.name === name)?.value,
  });
  environment = selectedSecret.environment;
  verifyIosXcodeBuild({ derivedDataPath: iosDerivedDataPath });
  console.log(`9. Starting ${label}`);
  // xcodebuild does not reliably propagate per-invocation E2E_* variables to
  // the XCTest process. Persist the compact step context on the host so the
  // iOS UI test can read the exact cycle/step without timing-based defaults.
  if (environment.E2E_ROLE || environment.E2E_NETWORK_ROLE) {
    writeDiagnostic(`[IOS_STEP_BINDING] role=${environment.E2E_ROLE ?? environment.E2E_NETWORK_ROLE} `
      + `cycle=${environment.E2E_CYCLE ?? '?'} step=${environment.E2E_STEP ?? '?'} `
      + `selectorHash=${createHash('sha256').update(selectedSecret.name).digest('hex').slice(0, 12)} `
      + 'configAndLaunchSelectionEqual=true');
    writeFileSync(iosStepConfigPath, JSON.stringify({
      role: environment.E2E_ROLE ?? environment.E2E_NETWORK_ROLE,
      cycle: environment.E2E_CYCLE,
      step: environment.E2E_STEP,
      approvalPlatform: environment.E2E_APPROVAL_PLATFORM,
      sender: environment.E2E_SENDER,
      action: environment.E2E_ACTION,
      expectedOutcome: environment.E2E_EXPECTED_OUTCOME,
      secretName: selectedSecret.name,
      secretValue: selectedSecret.value,
      repeatApprove: environment.E2E_REPEAT_APPROVE === '1',
      duplicateRecovery: environment.E2E_DUPLICATE_RECOVERY === '1',
      verifyReceiverAlert: environment.E2E_VERIFY_RECEIVER_ALERT === '1',
      receiverRequestAlreadySent: environment.E2E_RECEIVER_REQUEST_ALREADY_SENT === '1',
      networkRole: environment.E2E_NETWORK_ROLE,
      networkCycles: environment.E2E_NETWORK_CYCLES,
      expectedWebDeviceName: expectedDeviceNames?.web,
      expectedAndroidDeviceName: expectedDeviceNames?.android,
      expectedIosDeviceName: expectedDeviceNames?.ios,
      joinTarget: environment.E2E_JOIN_TARGET,
    }));
  }
  const xcodebuildArgs = [
    reuseIosBuildForTesting ? 'test-without-building' : 'test',
    '-project',
    iosProjectPath,
    '-scheme',
    'iosApp',
    '-configuration',
    'Debug',
    '-destination',
    `id=${simulatorUdid}`,
    '-derivedDataPath',
    iosDerivedDataPath,
    `-only-testing:iosAppUITests/${testClass}/${testMethod}`,
  ];
  if (networkLossConfig) {
    xcodebuildArgs.push(
      `META_SECRET_SOCKET_URL=${networkProxyUrl('ios')}`,
      'META_SECRET_ENV=local',
    );
  }
  const watched = watchProcessOutput(
    'xcodebuild',
    xcodebuildArgs,
    {
      cwd: resolve(composeRoot, 'iosApp'),
      env: {
        GRADLE_USER_HOME: iosGradleUserHome,
        E2E_VAULT_NAME: scenario.vault.name,
        E2E_SECRET_NAME: selectedSecret.name,
        E2E_SECRET_VALUE: selectedSecret.value,
        E2E_SECRET_CONFIG: secretConfigJson,
        E2E_RECOVERY_PLAN: cyclePlanJson,
        E2E_RECOVERY_CYCLES: String(recoveryCycles.length),
        // Keep iOS launch variables compact. XCTest's launch environment
        // truncates the full 18-cycle JSON payload before the test starts.
        E2E_IOS_SENDER_CYCLES: iosSenderCycles,
        E2E_IOS_APPROVAL_STEPS: iosApprovalSteps,
        E2E_SECRET_CREATION_PLAN: JSON.stringify(secretCreationPlan),
        E2E_APPROVAL_COORDINATOR_URL: 'http://127.0.0.1:5180',
        ...(networkLossConfig ? { E2E_CORE_SERVER_URL: networkProxyUrl('ios') } : {}),
        ...environment,
      },
    },
  );
  // XCTest can finish the selected test successfully while xcodebuild keeps
  // its testmanagerd session open indefinitely. The selected-suite result is
  // the authoritative pass/fail boundary for this one-test invocation; close
  // the stuck wrapper so the next recovery cycle can start.
  // Full 18-cycle suites can spend ~353s in XCTest before teardown completes.
  // Keep a separate bounded result-flush allowance after the business work;
  // six-cycle blocks retain the shorter deadline.
  const selectedSuiteTimeoutMs = recoveryCycles.length >= 18 ? 600_000 : 360_000;
  const selectedSuitePassed = watched.waitForMarker(
    "Test Suite 'Selected tests' passed",
    selectedSuiteTimeoutMs,
  )
    .then(async () => {
      if (!watched.child.killed) watched.child.kill('SIGTERM');
      await watched.closed;
      removePassedIosTestResults();
      return { stdout: watched.outputTail(), stderr: '' };
    });
  // The wrapper is deliberately terminated after the authoritative XCTest
  // pass marker. Its low-level process promise therefore rejects with the
  // expected SIGTERM even though the UI step passed; consume that internal
  // lifecycle result while `selectedSuitePassed` remains the public result.
  void watched.result.catch(() => {});
  // waitForMarker already rejects if xcodebuild exits before the XCTest pass
  // marker. Once the marker is seen, wait for the actual process close before
  // allowing the next iOS step to start.
  const result = selectedSuitePassed;
  void result.catch(() => {});
  return { ...watched, result };
}

async function prepareAndroidEmulator() {
  let serial = await findOnlineAndroidAvd(
    androidSdkTools,
    scenario.android.avdName,
    runAndCapture,
  );

  if (!serial) {
    console.log(`12. Starting Android emulator: ${scenario.android.avdName}`);
    // A stale named snapshot can report boot completed while leaving the
    // emulated shared-storage volume unmounted. Start from the current AVD
    // userdata without loading that snapshot; readiness is checked below.
    run(androidSdkTools.emulatorPath, ['-avd', scenario.android.avdName, '-no-snapshot-load']);
    // `adb wait-for-device` also returns for an offline emulator. Wait for
    // the transport to become usable before starting Gradle.
    serial = await waitForOnlineAndroidAvd(scenario.android.avdName);
  }

  if (!serial) throw new Error(`Android emulator ${scenario.android.avdName} did not start`);
  androidSerialForCleanup = serial;
  console.log(`13. Preparing Android emulator: ${serial}`);
  await waitForAndroidBoot(serial);
  // sys.boot_completed can be published before the adb transport finishes
  // reconnecting after a snapshot restore. Verify the serial is online again
  // immediately before sending unlock input, otherwise adb may report the
  // device as not found/offline even though Android already reports booted.
  await waitForOnlineAndroidDevice(serial);
  // The page-size-16kb debug AVD rejects shell input injection while adbd is
  // running as the shell user. Rooting this debuggable emulator is reversible
  // for the run and lets the semantic unlock below deliver the configured PIN.
  await runAdbWithRetry(['-s', serial, 'root']).catch((error) => {
    console.warn(`ADB root unavailable; continuing with existing adbd user: ${error.message}`);
  });
  await waitForOnlineAndroidDevice(serial);
  await unlockAndroidDevice(serial);
  await waitForAndroidStorage(serial);
  await resetE2eNativeAppState({
    scenario,
    platforms: [{ platform: 'Android', avdName: scenario.android.avdName, serial }],
    onEvent: (message) => {
      console.log(`🧹 ${message}`);
      writeDiagnostic(`[BLOCK APP RESET] ${message}`);
    },
  });
  return serial;
}

async function waitForOnlineAndroidAvd(avdName, timeoutMs = 120_000) {
  const deadline = Date.now() + timeoutMs;
  let lastError = `AVD ${avdName} is not listed as an online Android emulator`;
  while (Date.now() < deadline) {
    try {
      const serial = await findOnlineAndroidAvd(androidSdkTools, avdName, runAndCapture);
      if (serial) return serial;
    } catch (error) {
      lastError = error.message;
    }
    await wait(1_000);
  }
  throw new Error(`Android AVD ${avdName} did not become online: ${lastError}`);
}

async function unlockAndroidDevice(serial) {
  console.log(`13a. Unlocking Android emulator: ${serial}`);
  if (await androidUserIsUnlocked(serial)) return;
  await runAdbWithRetry(['-s', serial, 'shell', 'input', 'keyevent', 'KEYCODE_WAKEUP']);
  await waitForAndroidPinPrompt(serial);
  // The AVD is configured with PIN 1111. Key events are used instead of
  // `input text`: secure PIN fields reject text injection on Android 36.
  for (let digit = 0; digit < 4; digit += 1) {
    await runAdbWithRetry(['-s', serial, 'shell', 'input', 'keyevent', 'KEYCODE_1']);
  }
  await runAdbWithRetry(['-s', serial, 'shell', 'input', 'keyevent', 'KEYCODE_ENTER']);
  await waitForAndroidUnlocked(serial);
}

async function androidUserIsUnlocked(serial) {
  try {
    const { stdout } = await runAndCapture('adb', ['-s', serial, 'shell', 'dumpsys', 'trust']);
    return /deviceLocked=0/.test(stdout);
  } catch {
    return false;
  }
}

async function waitForAndroidPinPrompt(serial, timeoutMs = 120_000) {
  const deadline = Date.now() + timeoutMs;
  const dumpPath = '/data/local/tmp/metasecret-e2e-window.xml';
  let lastState = 'PIN prompt not visible';
  while (Date.now() < deadline) {
    // Android 16 can publish boot_completed before adbd finishes its final
    // reconnect. Re-check the transport here instead of treating one
    // transient offline/not-found response as a PIN failure.
    const onlineWindow = Math.min(deadline - Date.now(), 5_000);
    await waitForOnlineAndroidDevice(serial, onlineWindow).catch((error) => {
      lastState = error.message;
    });
    if (Date.now() >= deadline) break;
    if (await androidUserIsUnlocked(serial)) return;
    try {
      await runAdbWithRetry([
        '-s', serial, 'shell', 'uiautomator', 'dump', dumpPath,
      ], { attempts: 2, delayMs: 200 });
      await runAdbWithRetry([
        '-s', serial, 'shell', 'grep', '-q', 'keyguard_pin_view', dumpPath,
      ], { attempts: 2, delayMs: 200 });
      return;
    } catch (error) {
      lastState = error.message;
      // A cold boot may still be showing the lock-screen artwork. Repeating
      // the gesture until the PIN view exists is state-driven, not a fixed
      // sleep, and also covers a display that woke between adb reconnects.
      await runAdbWithRetry([
        '-s', serial, 'shell', 'input', 'swipe', '540', '2200', '540', '300', '500',
      ], { attempts: 2, delayMs: 200 }).catch(() => {});
      await wait(250);
    }
  }
  throw new Error(`Android PIN prompt did not appear: ${lastState}`);
}

async function waitForAndroidUnlocked(serial, timeoutMs = 30_000) {
  const deadline = Date.now() + timeoutMs;
  let lastState = 'deviceLocked=1';
  while (Date.now() < deadline) {
    if (await androidUserIsUnlocked(serial)) return;
    try {
      const { stdout } = await runAndCapture('adb', ['-s', serial, 'shell', 'dumpsys', 'trust']);
      lastState = stdout.match(/deviceLocked=[^, ]+/)?.[0] ?? lastState;
    } catch (error) {
      lastState = error.message;
    }
    await wait(250);
  }
  throw new Error(`Android emulator remained locked after PIN entry: ${lastState}`);
}

async function waitForOnlineAndroidDevice(expectedSerial = null, timeoutMs = 120_000) {
  const deadline = Date.now() + timeoutMs;
  let lastState = 'no emulator listed';
  while (Date.now() < deadline) {
    try {
      const { stdout } = await runAndCapture('adb', ['devices']);
      const rows = stdout.split('\n').map((line) => line.trim().split(/\s+/));
      const online = rows.find(([id, state]) =>
        id?.startsWith('emulator-') && state === 'device' &&
        (expectedSerial == null || id === expectedSerial));
      if (online?.[0]) {
        // `adb devices` can briefly report `device` while adbd is still
        // restarting (especially after `adb root`). Verify a real shell
        // round-trip before allowing Gradle or UI input to start.
        try {
          await runAndCapture('adb', ['-s', online[0], 'shell', 'true']);
          return online[0];
        } catch (error) {
          lastState = `${online[0]} shell unavailable: ${error.message}`;
        }
      }
      const emulator = rows.find(([id]) =>
        id?.startsWith('emulator-') &&
        (expectedSerial == null || id === expectedSerial));
      lastState = emulator ? `${emulator[0]} ${emulator[1] || '<empty>'}` : 'no emulator listed';
    } catch (error) {
      lastState = error.message;
    }
    await wait(1_000);
  }
  throw new Error(`Android emulator did not become online: ${lastState}`);
}

async function waitForAndroidBoot(serial, timeoutMs = 240_000) {
  const deadline = Date.now() + timeoutMs;
  let lastError = 'Android did not report sys.boot_completed=1';

  while (Date.now() < deadline) {
    try {
      const { stdout } = await runAndCapture('adb', ['-s', serial, 'shell', 'getprop', 'sys.boot_completed']);
      if (stdout.trim() === '1') return;
      lastError = `sys.boot_completed=${stdout.trim() || '<empty>'}`;
    } catch (error) {
      lastError = error.message;
    }
    await wait(1_000);
  }

  throw new Error(`Android emulator ${serial} did not finish booting: ${lastError}`);
}

async function waitForAndroidStorage(serial, timeoutMs = 120_000) {
  const deadline = Date.now() + timeoutMs;
  let lastError = 'Android shared storage is not mounted';
  let lastMountState = '<unknown>';

  while (Date.now() < deadline) {
    try {
      await runAndCapture('adb', ['-s', serial, 'shell', 'test', '-d', '/storage/self/primary']);
      // The Android emulator does not always pre-create this directory after
      // a clean boot. Creating the same parent that UTP will use verifies that
      // the FUSE-backed shared-storage mount is writable and makes the later
      // instrumentation mkdir deterministic.
      await runAndCapture(
        'adb',
        ['-s', serial, 'shell', 'mkdir', '-p', '/sdcard/Android/media'],
      );
      await runAndCapture('adb', ['-s', serial, 'shell', 'test', '-d', '/sdcard/Android/media']);
      console.log(`Android shared storage is ready: ${serial}`);
      return;
    } catch (error) {
      lastError = error.message;
      try {
        const { stdout } = await runAndCapture(
          'adb',
          [
            '-s', serial, 'shell', 'ls', '-ld',
            '/sdcard', '/storage/self', '/storage/self/primary', '/storage/emulated/0',
          ],
        );
        lastMountState = stdout.trim().replace(/\s+/g, ' ');
      } catch (mountError) {
        lastMountState = mountError.message;
      }
    }
    await wait(1_000);
  }

  throw new Error(`Android shared storage did not become ready: ${lastError}; mount=${lastMountState}`);
}

async function runAdbWithRetry(args, { attempts = 5, delayMs = 1_000 } = {}) {
  let lastError;
  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    try {
      return await runAndCapture('adb', args);
    } catch (error) {
      lastError = error;
      console.warn(`ADB command failed (${attempt}/${attempts}): adb ${args.join(' ')}\n${error.message}`);
      await runAndCapture('adb', ['reconnect', 'device']).catch((reconnectError) => {
        console.warn(`ADB reconnect failed: ${reconnectError.message}`);
      });
      if (attempt < attempts) await wait(delayMs);
    }
  }
  throw lastError;
}

async function captureInstalledAndroidAppProvenance(serial) {
  const packageName = scenario.android.bundleId;
  const { stdout } = await runAdbWithRetry(['-s', serial, 'shell', 'pm', 'path', packageName]);
  const remoteApkPath = stdout
    .split(/\r?\n/)
    .map((line) => line.trim().replace(/^package:/, ''))
    .find(Boolean);
  if (!remoteApkPath) throw new Error(`Installed APK path not found for ${packageName}`);

  const tempDirectory = mkdtempSync(resolve(tmpdir(), 'metasecret-installed-apk-'));
  try {
    const localApkPath = resolve(tempDirectory, 'installed-base.apk');
    await runAdbWithRetry(['-s', serial, 'pull', remoteApkPath, localApkPath]);
    const installedApkHash = createHash('sha256').update(readFileSync(localApkPath)).digest('hex');
    const builtApkHash = createHash('sha256').update(readFileSync(androidAppApkPath)).digest('hex');
    const matchesBuiltArtifact = installedApkHash === builtApkHash;
    writeDiagnostic(
      `[BUILD] installed Android app provenance package=${packageName} installedApkSha256=${installedApkHash} builtApkSha256=${builtApkHash} matchesBuiltArtifact=${matchesBuiltArtifact}`,
    );
    if (!matchesBuiltArtifact) {
      throw new Error(`Installed Android APK hash does not match built artifact for ${packageName}`);
    }
  } finally {
    rmSync(tempDirectory, { recursive: true, force: true });
  }
}

async function startAndroidJoinTest(
  serial,
  {
    testClass = scenario.android.testClass,
    testMethod = scenario.android.testMethod ?? scenario.android.joinTestMethod,
    expectedSecretNames = [],
    directShowSecretNames = [],
    joinPlatforms = null,
  } = {},
) {
  console.log('14. Starting Android UI test');
  // An emulator can briefly switch from `device` to `offline` after boot
  // while its services settle. AGP fails immediately in that state, so make
  // the state check immediately before handing control to Gradle.
  await waitForAndroidBoot(serial);
  // connectedDebugAndroidTest installs both the app and its instrumentation
  // APK, then launches the target activity itself. A separate `am start`
  // preflight is redundant and has proved flaky on the Android 16 emulator.
  await runAdbWithRetry(['-s', serial, 'logcat', '-c']);
  const appApkPath = androidAppApkPath;
  const testApkPath = androidTestApkPath;
  const logcat = watchProcessOutput('adb', [
    '-s', serial, 'logcat',
    ...(diagnosticAndroidVerbose ? ['-v', 'threadtime', 'System.out:I', 'System.err:W'] : []),
    'MetaSecretE2E:I',
    'AndroidRuntime:E',
    'ActivityManager:E',
    'linker:E',
    'DEBUG:F',
    'crash_dump64:F',
    'tombstoned:E',
    'libc:F',
    diagnosticAndroidVerbose ? '*:E' : '*:S',
  ]);
  void logcat.result.catch(() => {}); // logcat is stopped deliberately when the instrumentation test ends
  const androidTestClass = testClass
    ?? 'metasecret.project.com.CaseFourAndroidConcurrentRecoveryTest';
  const androidTestMethod = testMethod
    ?? (androidTestClass === 'metasecret.project.com.CaseFourAndroidConcurrentRecoveryTest'
      ? 'createVaultThenHandleConcurrentRecovery'
      : undefined);
  const androidTestSelector = androidTestMethod
    ? `${androidTestClass}#${androidTestMethod}`
    : androidTestClass;
  const runnerArguments = [
    '-e', 'class', androidTestSelector,
    '-e', 'vaultName', scenario.vault.name,
    '-e', 'secretName', defaultSecret?.name ?? '',
    '-e', 'secretValue', defaultSecret?.value ?? '',
    '-e', 'secretConfig', secretConfigJson,
    '-e', 'recoveryCycles', String(recoveryCycles.length),
    '-e', 'cyclePlan', cyclePlanJson,
    '-e', 'secretCreationPlan', JSON.stringify(secretCreationPlan),
    ...(expectedSecretNames.length > 0
      ? ['-e', 'expectedSecretNames', expectedSecretNames.join(',')]
      : []),
    ...(directShowSecretNames.length > 0
      ? ['-e', 'directShowSecretNames', directShowSecretNames.join(',')]
      : []),
    '-e', 'approvalCoordinatorUrl', 'http://10.0.2.2:5180',
    '-e', 'setupOnly', scenario.android.setupOnly ? 'true' : 'false',
    ...(joinPlatforms ? ['-e', 'joinPlatforms', joinPlatforms.join(',')] : []),
  ];
  // Android's `am` parser treats JSON punctuation as shell syntax when a
  // long value is passed through `adb shell`. The Test #7 setup only needs
  // these scalar values; keeping the JSON plan for the Gradle path avoids a
  // fragile command-line encoding and does not reduce scenario coverage.
  const directRunnerArguments = [
    '-e', 'class', androidTestSelector,
    '-e', 'vaultName', scenario.vault.name,
    '-e', 'secretName', defaultSecret?.name ?? '',
    '-e', 'secretValue', defaultSecret?.value ?? '',
    '-e', 'secretNames', Object.values(secretConfigs).map((config) => config.name).join(','),
    '-e', 'secretValues', Object.values(secretConfigs).map((config) => config.value).join(','),
    ...(expectedSecretNames.length > 0
      ? ['-e', 'expectedSecretNames', expectedSecretNames.join(',')]
      : []),
    ...(directShowSecretNames.length > 0
      ? ['-e', 'directShowSecretNames', directShowSecretNames.join(',')]
      : []),
    '-e', 'approvalCoordinatorUrl', 'http://10.0.2.2:5180',
    '-e', 'setupOnly', scenario.android.setupOnly ? 'true' : 'false',
    ...(joinPlatforms ? ['-e', 'joinPlatforms', joinPlatforms.join(',')] : []),
  ];
  const androidNetworkGradleArgs = networkLossConfig
    ? [
        `-PMETA_SECRET_ENV=local`,
        `-PMETA_SECRET_SOCKET_URL=${networkProxyUrl('android')}`,
        `-PMETA_SECRET_E2E_SERVER_URL=${networkProxyUrl('android')}`,
      ]
    : [];

  let test;
  let directInstrumentation = false;
  if (scenario.android.directInstrumentationSetup) {
    // Gradle's connected test task may uninstall the target application after
    // the test. Test #7 must keep its vault between the setup and assertion
    // instrumentation, so build/install both APKs and invoke the runner
    // directly instead of using connectedDebugAndroidTest.
    if (reuseProvenArtifacts) {
      writeDiagnostic('[ANDROID] reusing exact fingerprint-verified app and instrumentation APKs');
      await verifyAndroidApkFfiProvenance();
    } else {
      await buildAndroidFfiFromCurrentSource();
      await runAndWait('./gradlew', [
        ':composeApp:assembleDebug',
        ':composeApp:assembleDebugAndroidTest',
        ...androidNetworkGradleArgs,
      ], { cwd: composeRoot, env: { ANDROID_SERIAL: serial } });
      await verifyAndroidApkFfiProvenance();
      // Direct instrumentation reuses the assembled APKs for every remaining
      // step and does not need Gradle again. Release the daemon before the
      // visible multi-client phase so it cannot compete with both simulators
      // for memory throughout long scenarios such as Test #46.
      await runAndWait('./gradlew', ['--stop'], {
        cwd: composeRoot,
        stdio: 'ignore',
      }).catch(() => {});
      writeDiagnostic('[ANDROID] Gradle daemon stopped after one-time APK assembly');
      await recordFreshBuildProvenance();
    }
    await runAdbWithRetry(['-s', serial, 'install', '-r', appApkPath]);
    await runAdbWithRetry(['-s', serial, 'install', '-r', testApkPath]);
    await captureInstalledAndroidAppProvenance(serial);
    await dismissAndroidCompatibilityDialogOnce(serial);
    test = watchAndroidInstrumentation(
      serial,
      directRunnerArguments,
      `${androidTestBundleId}/androidx.test.runner.AndroidJUnitRunner`,
    );
    directInstrumentation = true;
  } else {
    // The orchestrator uninstalls the app before each run, while Gradle may
    // consider installDebug up-to-date and skip reinstalling it. Install the
    // freshly assembled target APK explicitly so ActivityScenario can resolve
    // MainActivity even when the Gradle install task is cached. Build both
    // artifacts here as well: a clean workspace may not have APKs yet.
    await buildAndroidFfiFromCurrentSource();
    await runAndWait('./gradlew', [
      ':composeApp:assembleDebug',
      ':composeApp:assembleDebugAndroidTest',
      ...androidNetworkGradleArgs,
    ], { cwd: composeRoot, env: { ANDROID_SERIAL: serial } });
    await verifyAndroidApkFfiProvenance();
    await recordFreshBuildProvenance();
    await runAdbWithRetry(['-s', serial, 'install', '-r', appApkPath]);
    await captureInstalledAndroidAppProvenance(serial);
    // Android 16 may show the 16 KB compatibility warning only when the
    // freshly installed target is first launched by instrumentation. Dismiss
    // it before the UI test reaches a screenshot marker so the captured frame
    // contains the application error state rather than the system dialog.
    await dismissAndroidCompatibilityDialogOnce(serial);
    test = runAndWait(
      './gradlew',
      [
        ':composeApp:installDebug',
        ':composeApp:connectedDebugAndroidTest',
        `-Pandroid.testInstrumentationRunnerArguments.class=${androidTestSelector}`,
        `-Pandroid.testInstrumentationRunnerArguments.vaultName=${scenario.vault.name}`,
        `-Pandroid.testInstrumentationRunnerArguments.secretName=${defaultSecret?.name ?? ''}`,
        `-Pandroid.testInstrumentationRunnerArguments.secretConfig=${secretConfigJson}`,
        `-Pandroid.testInstrumentationRunnerArguments.recoveryCycles=${recoveryCycles.length}`,
        `-Pandroid.testInstrumentationRunnerArguments.cyclePlan=${cyclePlanJson}`,
        `-Pandroid.testInstrumentationRunnerArguments.secretCreationPlan=${JSON.stringify(secretCreationPlan)}`,
        ...(expectedSecretNames.length > 0
          ? [`-Pandroid.testInstrumentationRunnerArguments.expectedSecretNames=${expectedSecretNames.join(',')}`]
          : []),
        ...(directShowSecretNames.length > 0
          ? [`-Pandroid.testInstrumentationRunnerArguments.directShowSecretNames=${directShowSecretNames.join(',')}`]
          : []),
        '-Pandroid.testInstrumentationRunnerArguments.approvalCoordinatorUrl=http://10.0.2.2:5180',
        `-Pandroid.testInstrumentationRunnerArguments.setupOnly=${scenario.android.setupOnly ? 'true' : 'false'}`,
        ...androidNetworkGradleArgs,
      ],
      { cwd: composeRoot, env: { ANDROID_SERIAL: serial } },
    );
  }

  return superviseAndroidInstrumentation({
    instrumentationResult: directInstrumentation ? test.result : test,
    logcat,
    selector: androidTestSelector,
    validateOutput: directInstrumentation ? assertAndroidInstrumentationPassed : () => {},
  });
}

function assertAndroidInstrumentationPassed(output, selector) {
  const text = `${output?.stdout ?? ''}\n${output?.stderr ?? ''}`;
  if (/FAILURES!!!|There was\s+\d+ failure|There were\s+\d+ failures|INSTRUMENTATION_STATUS_CODE:\s*-\d+/.test(text)) {
    throw new Error(`Android instrumentation failed for ${selector}\n${text.slice(-12_000)}`);
  }
}

async function startAndroidStepTest(serial, {
  role,
  cycle,
  step,
  approvalPlatform = '',
  sender = '',
  decision = '',
  expectedOutcome = '',
  repeatApprove = false,
  duplicateRecovery = false,
  networkRole = '',
  networkBlock = '',
  networkCycles = '',
  joinTarget = '',
  secretValue = '',
  initializeShow = false,
  verifyReceiverAlert = false,
  receiverRequestAlreadySent = false,
  expectedSecretNames = [],
  expectedDeviceNames = null,
  testClass = scenario.android.testClass,
  secretName = defaultSecret?.name ?? '',
  testMethod = scenario.android.stepTestMethod ?? 'handleRecoveryStep',
  label = 'Android recovery step',
} = {}) {
  console.log(`Starting ${label}: role=${role} cycle=${cycle} step=${step}`);
  const appApkPath = resolve(composeRoot, 'composeApp/build/outputs/apk/debug/composeApp-debug.apk');
  const testApkPath = resolve(composeRoot, 'composeApp/build/outputs/apk/androidTest/debug/composeApp-debug-androidTest.apk');
  await runAdbWithRetry(['-s', serial, 'install', '-r', appApkPath]);
  await runAdbWithRetry(['-s', serial, 'install', '-r', testApkPath]);
  await captureInstalledAndroidAppProvenance(serial);
  await dismissAndroidCompatibilityDialogOnce(serial);
  // Every step owns a fresh synchronization marker set. Clear the emulator
  // buffer before attaching logcat so old system ANRs are not replayed once
  // per cycle and retained by the Node/terminal pipeline.
  await runAdbWithRetry(['-s', serial, 'logcat', '-c']);
  const logcat = watchProcessOutput('adb', [
    '-s', serial, 'logcat',
    ...(diagnosticAndroidVerbose ? ['-v', 'threadtime', 'System.out:I', 'System.err:W'] : []),
    'MetaSecretE2E:I',
    'AndroidRuntime:E',
    'ActivityManager:E',
    'libc:F',
    diagnosticAndroidVerbose ? '*:E' : '*:S',
  ]);
  void logcat.result.catch(() => {});
  const androidTestClass = testClass
    ?? 'metasecret.project.com.CaseEightAndroidBothOfflineTest';
  const androidTestMethod = testMethod;
  const selector = `${androidTestClass}#${androidTestMethod}`;
  const runnerArguments = [
    '-e', 'class', selector,
    '-e', 'vaultName', scenario.vault.name,
    '-e', 'secretName', secretName,
    '-e', 'role', role,
    '-e', 'cycle', String(cycle),
    '-e', 'step', String(step),
    '-e', 'repeatApprove', repeatApprove ? 'true' : 'false',
    '-e', 'duplicateRecovery', duplicateRecovery ? 'true' : 'false',
    '-e', 'approvalCoordinatorUrl', 'http://10.0.2.2:5180',
  ];
  const optionalRunnerArguments = [
    ['approvalPlatform', approvalPlatform],
    ['sender', sender],
    ['expectedOutcome', expectedOutcome],
    ['networkRole', networkRole],
    ['networkBlock', networkBlock],
    ['networkCycles', networkCycles],
    ['joinTarget', joinTarget],
    ['secretValue', secretValue],
    ['initializeShow', initializeShow ? 'true' : 'false'],
    ['verifyReceiverAlert', verifyReceiverAlert ? 'true' : 'false'],
    ['receiverRequestAlreadySent', receiverRequestAlreadySent ? 'true' : 'false'],
    ['expectedSecretNames', expectedSecretNames.join(',')],
    ['expectedWebDeviceName', expectedDeviceNames?.web],
    ['expectedAndroidDeviceName', expectedDeviceNames?.android],
    ['expectedIosDeviceName', expectedDeviceNames?.ios],
  ];
  const runnerInsertAt = runnerArguments.length;
  runnerArguments.splice(
    runnerInsertAt,
    0,
    ...optionalRunnerArguments
      .filter(([, value]) => value !== undefined && value !== null && String(value).length > 0)
      .flatMap(([key, value]) => ['-e', key, String(value)]),
  );
  if (decision) {
    const coordinatorIndex = runnerArguments.indexOf('approvalCoordinatorUrl');
    // Insert before the existing `-e approvalCoordinatorUrl` pair. Inserting
    // at the value index leaves the pair's `-e` in place and produces the
    // invalid `-e -e decision ...` command line for `am instrument`.
    runnerArguments.splice(coordinatorIndex - 1, 0, '-e', 'decision', decision);
  }
  const test = watchAndroidInstrumentation(
    serial,
    runnerArguments,
    `${androidTestBundleId}/androidx.test.runner.AndroidJUnitRunner`,
  );
  // Android E2E markers are emitted with Log.i and therefore arrive on the
  // dedicated logcat watcher, not on `am instrument` stdout. Keep the
  // instrumentation result for pass/fail validation, but expose the logcat
  // marker waiter to the orchestrator.
  return superviseAndroidInstrumentation({
    instrumentationResult: test.result,
    logcat,
    selector,
    validateOutput: assertAndroidInstrumentationPassed,
  });
}

async function dismissAndroidCompatibilityDialog(serial, timeoutMs = 30_000) {
  // Android 16 shows this system dialog the first time the debug APK is
  // launched. It is outside Compose's semantics tree, so dismiss it through
  // accessibility only when its text is actually present.
  await runAdbWithRetry([
    '-s', serial, 'shell', 'monkey', '-p', scenario.android.bundleId, '1',
  ]);
  const dumpPath = '/data/local/tmp/metasecret-e2e-window.xml';
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      await runAdbWithRetry([
        '-s', serial, 'shell', 'uiautomator', 'dump', dumpPath,
      ], { attempts: 2, delayMs: 200 });
      const { stdout } = await runAndCapture('adb', ['-s', serial, 'shell', 'cat', dumpPath]);
      if (stdout.includes('16 KB compatible') || stdout.includes('ELF alignment check failed')) {
        const ok = stdout.match(/<node[^>]*text="OK"[^>]*bounds="\[(\d+),(\d+)\]\[(\d+),(\d+)\]"/);
        if (ok) {
          const [, left, top, right, bottom] = ok.map(Number);
          await runAdbWithRetry([
            '-s', serial,
            'shell',
            'input',
            'tap',
            String(Math.round((left + right) / 2)),
            String(Math.round((top + bottom) / 2)),
          ]);
        } else {
          await runAdbWithRetry(['-s', serial, 'shell', 'input', 'keyevent', 'KEYCODE_ENTER']);
        }
        break;
      }
    } catch {
      // The target may still be starting; the next accessibility snapshot is authoritative.
    }
    await wait(250);
  }
  await runAdbWithRetry(['-s', serial, 'shell', 'am', 'force-stop', scenario.android.bundleId]);
}

async function dismissAndroidCompatibilityDialogOnce(serial) {
  if (androidCompatibilityDialogHandled) return;
  await dismissAndroidCompatibilityDialog(serial);
  androidCompatibilityDialogHandled = true;
}

async function stopAndroidApplication(serial) {
  console.log(`15a. Stopping offline Android application: ${scenario.android.bundleId}`);
  await runAdbWithRetry([
    '-s', serial, 'shell', 'am', 'force-stop', scenario.android.bundleId,
  ]);
  const deadline = Date.now() + 30_000;
  let lastPid = '<unknown>';
  while (Date.now() < deadline) {
    const { stdout } = await runAndCapture(
      'adb',
      ['-s', serial, 'shell', 'pidof', scenario.android.bundleId],
    ).catch(() => ({ stdout: '' }));
    lastPid = stdout.trim() || '<none>';
    if (!stdout.trim()) {
      console.log('Android application is stopped');
      return;
    }
    await wait(500);
  }
  throw new Error(`Android application did not stop: pid=${lastPid}`);
}

async function stopIosApplication(simulatorUdid) {
  console.log(`15a. Stopping offline iOS application: ${scenario.ios.bundleId}`);
  await runAndWait('xcrun', ['simctl', 'terminate', simulatorUdid, scenario.ios.bundleId])
    .catch(() => {});
  console.log('iOS application is stopped');
}

async function startAndroidRemainingRecoveryTest(serial) {
  console.log('15c. Starting Android recovery cycles after offline restart');
  await waitForOnlineAndroidDevice(serial, 120_000);
  const appApkPath = resolve(
    composeRoot,
    'composeApp/build/outputs/apk/debug/composeApp-debug.apk',
  );
  const testApkPath = resolve(
    composeRoot,
    'composeApp/build/outputs/apk/androidTest/debug/composeApp-debug-androidTest.apk',
  );
  await runAdbWithRetry(['-s', serial, 'install', '-r', appApkPath]);
  await runAdbWithRetry(['-s', serial, 'install', '-r', testApkPath]);
  const selector = `${scenario.android.testClass}#handleRemainingRecoveryCycles`;
  const logcat = watchProcessOutput('adb', ['-s', serial, 'logcat', 'MetaSecretE2E:I', '*:S']);
  void logcat.result.catch(() => {});
  const test = watchAndroidInstrumentation(
    serial,
    [
      '-e', 'class', selector,
      '-e', 'vaultName', scenario.vault.name,
      '-e', 'secretName', defaultSecret?.name ?? '',
      '-e', 'approvalCoordinatorUrl', 'http://10.0.2.2:5180',
    ],
    `${androidTestBundleId}/androidx.test.runner.AndroidJUnitRunner`,
  );
  return superviseAndroidInstrumentation({
    instrumentationResult: test.result,
    logcat,
    selector,
    validateOutput: assertAndroidInstrumentationPassed,
  });
}

async function runIosStaleAlertAssertion(simulatorUdid) {
  // The first iOS UI test deliberately returns after the offline boundary.
  // A fresh XCTest invocation relaunches the persisted app state and checks
  // that no old incoming request remains in the UI.
  const assertion = startIosJoinTest(simulatorUdid, {
    testClass: scenario.ios.assertionClass ?? scenario.ios.testClass,
    testMethod: scenario.ios.assertionMethod ?? 'assertNoStaleRecoveryAlertAfterOfflineRestart',
    label: 'iOS stale-alert assertion',
  });
  await assertion.result;
  console.log('✅ Offline iOS receiver did not show a stale recovery alert');
}

async function runOfflineReceiverRecovery(page, iosTest, androidTest, androidSerial, simulatorUdid) {
  const cycleFor = (platform) => recoveryCycles.find((cycle) => cycle.offlineReceiver === platform);
  const androidCycle = cycleFor('android');
  const webCycle = cycleFor('web');
  const iosCycle = cycleFor('ios');
  if (!androidCycle || !webCycle || !iosCycle) {
    throw new Error('Test #7 requires exactly one offline cycle for android, web, and ios');
  }

  // Cycle 1 — Android is offline: Web sends, iOS approves.
  setCurrentCycleForDiagnostics(androidCycle);
  const androidSecret = androidCycle.senderSecrets.web ?? defaultSecret?.name;
  const androidApprovalStep = androidCycle.approvals.findIndex((approval) => approval.platform === 'ios') + 1;
  if (androidApprovalStep <= 0) throw new Error('Android-offline cycle requires an iOS approval');
  await androidTest.waitForMarker('E2E: ANDROID_OFFLINE_READY', 180_000);
  await androidTest.result;
  await stopAndroidApplication(androidSerial);
  console.log(`15.${androidCycle.number} Web requests recovery while Android is offline`);
  approvalCoordinator.allow('web-sender', androidCycle.number);
  await startWebRecovery(page, androidSecret);
  approvalCoordinator.allow(`ios-approve-${androidApprovalStep}`, androidCycle.number);
  await iosTest.waitForMarker(
    `E2E: IOS_APPROVED_INCOMING_${androidCycle.number}_${androidApprovalStep}`,
    180_000,
  );
  await revealAndCloseWebSecret(page, androidSecret, false);
  await runAndroidStaleAlertAssertion(androidSerial);
  console.log('✅ Offline Android receiver did not show a stale recovery alert');

  // Cycle 2 — Web is offline: Android sends, iOS approves.
  setCurrentCycleForDiagnostics(webCycle);
  const webSecret = webCycle.senderSecrets.android ?? defaultSecret?.name;
  await stopWebApplication(page);
  const androidRemaining = await startAndroidRemainingRecoveryTest(androidSerial);
  approvalCoordinator.allow('android-sender', webCycle.number);
  await androidRemaining.waitForMarker(`E2E: ANDROID_RECOVERY_REQUEST_SENT_${webCycle.number}`, 180_000);
  const webApprovalStep = webCycle.approvals.findIndex((approval) => approval.platform === 'ios') + 1;
  if (webApprovalStep <= 0) throw new Error('Web-offline cycle requires an iOS approval');
  approvalCoordinator.allow(`ios-approve-${webApprovalStep}`, webCycle.number);
  await iosTest.waitForMarker(
    `E2E: IOS_APPROVED_INCOMING_${webCycle.number}_${webApprovalStep}`,
    180_000,
  );
  approvalCoordinator.allow('android-show', webCycle.number);
  await androidRemaining.waitForMarker(
    `E2E: ANDROID_RECOVERY_CLOSED_${webCycle.number}`,
    180_000,
  );
  await reopenWebAfterOffline(page);
  await assertNoStaleWebRecoveryAlert(page, webSecret);
  console.log('✅ Offline Web receiver did not show a stale recovery alert');

  // Cycle 3 — iOS is offline: Android sends, Web approves. iOS exits its
  // first UI-test invocation at the explicit offline boundary, then a fresh
  // invocation checks the persisted state after the request is completed.
  setCurrentCycleForDiagnostics(iosCycle);
  const iosSecret = iosCycle.senderSecrets.android ?? defaultSecret?.name;
  await iosTest.waitForMarker(`E2E: IOS_OFFLINE_READY_${iosCycle.number}`, 180_000);
  await stopIosApplication(simulatorUdid);
  approvalCoordinator.allow('ios-offline-stop', iosCycle.number);
  await iosTest.result;
  await androidRemaining.waitForMarker('E2E: ANDROID_REMAINING_RECOVERY_READY', 180_000);
  approvalCoordinator.allow('android-sender', iosCycle.number);
  await androidRemaining.waitForMarker(
    `E2E: ANDROID_RECOVERY_REQUEST_SENT_${iosCycle.number}`,
    180_000,
  );
  const webApproverSecret = iosCycle.approvals.find((approval) => approval.platform === 'web')?.secret
    ?? iosSecret;
  await approveIncomingRecoveryOnWeb(page, webApproverSecret, 1);
  approvalCoordinator.allow('android-show', iosCycle.number);
  await androidRemaining.waitForMarker(
    `E2E: ANDROID_RECOVERY_CLOSED_${iosCycle.number}`,
    180_000,
  );
  await androidRemaining.result;
  await runIosStaleAlertAssertion(simulatorUdid);
}

async function createWebInitiatedVault(page) {
  await preparePrimaryWebClient(page);
  // F3 authenticates Web before the native build. Xcode/Gradle builds can
  // take long enough for the authenticated browser session to lock again,
  // so re-check its live UI state at the actual first scenario action.
  await unlockWithPasskeyIfNeeded(page);
  await page.getByPlaceholder('vault name').fill(scenario.vault.name);
  await page.getByRole('button', { name: 'Set Vault Name' }).click();
  await page.getByText('Vault name is free!').waitFor({ timeout: 120_000 });
  await page.getByRole('button', { name: 'Create', exact: true }).click();
  await page.getByRole('button', { name: '+ Add Secret', exact: true }).waitFor({ timeout: 180_000 });
  const initialSecretNames = secretCreationPlan?.initial?.web
    ?? Object.values(secretConfigs).map((config) => config.name);
  for (const secretName of initialSecretNames) {
    await createWebSecret(page, secretName);
  }
  await waitForWebSecretNames(page, initialSecretNames);
  console.log('✅ Web vault and initial secret ready');
}

async function preparePrimaryWebClient(page) {
  if (primaryWebClientReady) return;
  await page.goto(scenario.web.url, { waitUntil: 'domcontentloaded' });
  await unlockWithPasskeyIfNeeded(page);
  await Promise.race([
    page.getByPlaceholder('vault name').waitFor({ state: 'visible', timeout: 30_000 }),
    page.getByRole('link', { name: 'Secrets', exact: true }).waitFor({ state: 'visible', timeout: 30_000 }),
  ]);
  primaryWebClientReady = true;
  writeDiagnostic('[F3] primary Web client loaded, authenticated, and left Init before native clients boot');
}

async function runWebInitiatedSetup(page, simulatorUdid, androidSerial, { joinPlatforms = null } = {}) {
  joinPlatforms ??= scenario.mode === 'new-device-during-recovery'
    ? ['ios', 'android'].filter((platform) => scenario.newDevice?.oldReceivers?.includes(platform))
    : ['ios', 'android'];
  await createWebInitiatedVault(page);

  let iosTest = null;
  if (joinPlatforms.includes('ios')) {
  iosTest = startIosJoinTest(simulatorUdid, {
    testClass: scenario.ios.testClass,
    testMethod: scenario.ios.joinTestMethod,
    label: 'iOS Web-initiated join',
  });
  void iosTest.result.catch(() => {});
  // A cold Xcode/Kotlin framework build can exceed three minutes after the
  // native libraries were rebuilt. This is an event timeout, not a scenario
  // sleep: the test continues immediately when the marker is emitted.
  await iosTest.waitForMarker('E2E: IOS_JOIN_REQUEST_SENT', 300_000);
  await approveJoinRequestOnWeb(page, 'iOS');
  await iosTest.waitForMarker('E2E: IOS_JOIN_READY', 180_000);
  await iosTest.result;
  }

  let androidTest = null;
  if (joinPlatforms.includes('android')) {
  androidTest = await startAndroidJoinTest(androidSerial);
  await androidTest.waitForMarker('E2E: ANDROID_JOIN_REQUEST_SENT', 180_000);
  await approveJoinRequestOnWeb(page, 'Android');
  await androidTest.waitForMarker('E2E: ANDROID_JOIN_READY', 180_000);
  await androidTest.result;
  }
  await waitForWebSecrets(page);
  return { iosTest, androidTest };
}

function setJoinExpectedSecretNames(secretNames) {
  joinExpectedSecretNames = secretNames.map(secretNameFor);
  writeDiagnostic(
    `[TEST${testNumber}] joinExpectedSecretNames=${joinExpectedSecretNames.join(',')}`,
  );
}

async function runWebMultiSecretLifecycleSetup(page, simulatorUdid, androidSerial) {
  const initialNames = secretCreationPlan?.initial?.web ?? [defaultSecret?.name];
  const joinOrder = scenario.setup?.joinOrder ?? ['android', 'ios'];
  if (joinOrder.join(',') !== 'android,ios') {
    throw new Error(
      `Test #${testNumber} requires setup.joinOrder=[android,ios], got ${joinOrder.join(',')}`,
    );
  }

  setJoinExpectedSecretNames(initialNames);
  await createWebInitiatedVault(page);

  const androidTest = await startAndroidJoinTest(androidSerial, {
    testClass: scenario.android.joinTestClass
      ?? scenario.android.testClass
      ?? 'metasecret.project.com.CaseEightAndroidBothOfflineTest',
    testMethod: scenario.android.directShowTestMethod
      ?? scenario.android.joinTestMethod
      ?? 'joinWebInitiatedVault',
    expectedSecretNames: initialNames,
    directShowSecretNames: [
      ...(secretCreationPlan?.initial?.web ?? []),
      ...(secretCreationPlan?.afterAndroidJoin?.web ?? []),
    ].map(secretNameFor),
  });
  await androidTest.waitForMarker('E2E: ANDROID_JOIN_REQUEST_SENT', 180_000);
  await approveJoinRequestOnWeb(page, 'Android');
  await androidTest.waitForMarker('E2E: ANDROID_JOIN_READY', 180_000);
  await waitForWebSecretNames(page, initialNames);
  writeDiagnostic(
    `[TEST${testNumber}] setup complete members=web,android secrets=${initialNames.join(',')}`,
  );
  return { androidTest };
}

async function runWebMultiSecretLifecycleIosJoin(
  page,
  simulatorUdid,
  namesBeforeIos,
  androidSerial = null,
) {
  setJoinExpectedSecretNames(namesBeforeIos);
  const secondJoinApprover = scenario.setup?.secondJoinApprover ?? 'web';
  const iosTest = startIosJoinTest(simulatorUdid, {
    testClass: scenario.ios.joinTestClass
      ?? scenario.ios.testClass
      ?? 'CaseEightIosBothOfflineUITest',
    testMethod: scenario.ios.joinTestMethod ?? 'joinWebInitiatedVault',
    label: 'iOS Web-initiated join after Secret2',
  });
  void iosTest.result.catch(() => {});
  await iosTest.waitForMarker('E2E: IOS_JOIN_REQUEST_SENT', 300_000);
  if (secondJoinApprover === 'web') {
    await approveJoinRequestOnWeb(page, 'iOS');
  } else if (secondJoinApprover === 'android') {
    if (!androidSerial) {
      throw new Error(`Test #${testNumber} requires an Android serial for the iOS Join approval`);
    }
    const approval = await startAndroidStepTest(androidSerial, {
      testClass: scenario.android.lifecycleTestClass
        ?? scenario.android.creatorTestClass
        ?? scenario.android.testClass,
      testMethod: scenario.android.approveJoinTestMethod,
      role: 'join-approver',
      cycle: 0,
      step: 1,
      joinTarget: 'ios',
      label: `Android Test #${testNumber} approves iOS Join`,
    });
    await approval.waitForMarker(
      `E2E: ANDROID_TEST${testNumber}_JOIN_APPROVED_IOS`,
      180_000,
    );
    await approval.result;
  } else {
    throw new Error(
      `Test #${testNumber} has unsupported secondJoinApprover=${secondJoinApprover}`,
    );
  }
  await iosTest.waitForMarker('E2E: IOS_JOIN_READY', 180_000);
  await iosTest.result;
  await waitForWebSecretNames(page, namesBeforeIos);
  writeDiagnostic(
    `[TEST${testNumber}] iOS joined after Secret2; `
      + `approver=${secondJoinApprover} members=web,android,ios `
      + `secrets=${namesBeforeIos.join(',')}`,
  );
}

async function assertNoStaleWebMembershipUi(page, secretNames) {
  await page.getByTestId('remove-device-dialog').waitFor({ state: 'hidden', timeout: 30_000 });
  await page.getByTestId('pending-device-row').waitFor({ state: 'hidden', timeout: 30_000 });
  await page.locator('[data-slot="alert-dialog-content"]').waitFor({ state: 'hidden', timeout: 30_000 });
  await page.getByRole('link', { name: 'Secrets', exact: true }).click();
  await waitForWebSecretNames(page, secretNames);
  for (const secretName of secretNames) {
    const action = page.getByTestId(`secret-primary-action-${secretName}`);
    const row = page.locator('li').filter({ has: action });
    await row.waitFor({ state: 'visible', timeout: 30_000 });
    await row.getByTestId('recovery-request-badge').waitFor({ state: 'hidden', timeout: 30_000 });
    await row.getByTestId(`open-recovery-request-${secretName}`).waitFor({ state: 'hidden', timeout: 30_000 });
  }
}

async function inspectWebMemberRow(page, category, {
  current,
  expectedDeviceName = null,
  label,
}) {
  const categoryLabel = page.getByText(category, { exact: true });
  await categoryLabel.first().waitFor({ state: 'visible', timeout: 120_000 });
  const categoryCount = await categoryLabel.count();
  if (categoryCount !== 1) {
    throw new Error(`${label}: expected exactly one ${category} participant row, got ${categoryCount}`);
  }
  const row = categoryLabel.locator('xpath=ancestor::div[.//*[@data-slot="badge"]][1]');
  await row.waitFor({ state: 'visible', timeout: 30_000 });
  const rowCount = await row.count();
  if (rowCount !== 1) throw new Error(`${label}: ${category} row is ambiguous: ${rowCount}`);

  const nameElement = row.locator('p.font-semibold');
  await nameElement.waitFor({ state: 'visible', timeout: 30_000 });
  const nameCount = await nameElement.count();
  const actualDeviceName = (await nameElement.textContent())?.trim() ?? '';
  if (nameCount !== 1 || actualDeviceName.length === 0) {
    throw new Error(
      `${label}: ${category} row must expose exactly one non-empty nested device name; `
        + `count=${nameCount}`,
    );
  }
  if (expectedDeviceName && actualDeviceName !== expectedDeviceName) {
    throw new Error(
      `${label}: ${category} device name mismatch expected=${JSON.stringify(expectedDeviceName)} `
        + `actual=${JSON.stringify(actualDeviceName)}`,
    );
  }

  const expectedStatus = current ? 'Current' : 'Member';
  const unexpectedStatus = current ? 'Member' : 'Current';
  const expectedBadge = row.locator('[data-slot="badge"]').filter({
    hasText: new RegExp(`^\\s*${expectedStatus}\\s*$`),
  });
  await expectedBadge.waitFor({ state: 'visible', timeout: 30_000 });
  const [expectedBadgeCount, unexpectedBadgeCount, removeCount] = await Promise.all([
    expectedBadge.count(),
    row.locator('[data-slot="badge"]').filter({
      hasText: new RegExp(`^\\s*${unexpectedStatus}\\s*$`),
    }).count(),
    row.getByTestId('remove-device').count(),
  ]);
  if (expectedBadgeCount !== 1 || unexpectedBadgeCount !== 0) {
    throw new Error(
      `${label}: ${category} row status mismatch expected=${expectedStatus} `
        + `expectedCount=${expectedBadgeCount} unexpectedCount=${unexpectedBadgeCount}`,
    );
  }
  const expectedRemoveCount = current ? 0 : 1;
  if (removeCount !== expectedRemoveCount) {
    throw new Error(
      `${label}: ${category} row expected scoped trash count=${expectedRemoveCount}, got ${removeCount}`,
    );
  }
  return { row, deviceName: actualDeviceName };
}

async function assertWebMembership(
  page,
  expectedCategories,
  secretNames,
  label,
  expectedDeviceNames = {},
) {
  await page.getByRole('link', { name: 'Devices', exact: true }).click();
  await page.locator('[data-slot="card-title"]').getByText('Devices', { exact: true })
    .waitFor({ state: 'visible', timeout: 60_000 });
  const allCategories = ['Web', 'Android', 'iPhone'];
  const categoryToPlatform = { Web: 'web', Android: 'android', iPhone: 'ios' };
  const observedDeviceNames = {};
  for (const category of expectedCategories) {
    const platform = categoryToPlatform[category];
    const member = await inspectWebMemberRow(page, category, {
      current: platform === 'web',
      expectedDeviceName: expectedDeviceNames[platform],
      label,
    });
    observedDeviceNames[platform] = member.deviceName;
  }
  for (const category of allCategories.filter((candidate) => !expectedCategories.includes(candidate))) {
    await page.getByText(category, { exact: true }).first()
      .waitFor({ state: 'hidden', timeout: 120_000 });
  }
  const removeCount = await page.getByTestId('remove-device').count();
  if (removeCount !== Math.max(0, expectedCategories.length - 1)) {
    throw new Error(
      `${label}: expected ${expectedCategories.length - 1} removable member rows, got ${removeCount}`,
    );
  }
  const currentCount = await page.getByText('Current', { exact: true }).count();
  const memberCount = await page.getByText('Member', { exact: true }).count();
  if (currentCount !== 1 || memberCount !== expectedCategories.length - 1) {
    throw new Error(
      `${label}: member badge mismatch current=${currentCount} member=${memberCount} `
        + `expected=${expectedCategories.length}`,
    );
  }
  await assertNoStaleWebMembershipUi(page, secretNames);
  writeDiagnostic(
    `[TEST${testNumber}] ${label} memberCount=${expectedCategories.length} `
      + `participants=${expectedCategories.join(',')} staleUi=false`,
  );
  return observedDeviceNames;
}

async function initiateWebDeviceRemoval(page, {
  targetCategory,
  expectedTargetDeviceName,
  waitForTargetGone,
  label,
  requestVisibleMarker,
  completionMarker,
}) {
  if (typeof expectedTargetDeviceName !== 'string' || expectedTargetDeviceName.length === 0) {
    throw new Error(`${label}: exact observed registered target name is required`);
  }
  await page.getByRole('link', { name: 'Devices', exact: true }).click();
  const target = await inspectWebMemberRow(page, targetCategory, {
    current: false,
    expectedDeviceName: expectedTargetDeviceName,
    label,
  });
  const categoryText = page.getByText(targetCategory, { exact: true });
  const targetRow = target.row;
  const removeButton = targetRow.getByTestId('remove-device');
  await removeButton.waitFor({ state: 'visible', timeout: 30_000 });
  await removeButton.click();

  const dialog = page.getByTestId('remove-device-dialog');
  await dialog.waitFor({ state: 'visible', timeout: 30_000 });
  await dialog.getByText(targetCategory, { exact: true }).waitFor({ state: 'visible', timeout: 30_000 });
  const dialogDeviceName = dialog.locator('[data-slot="dialog-title"] span.text-primary');
  await dialogDeviceName.waitFor({ state: 'visible', timeout: 30_000 });
  const [dialogNameCount, dialogNameText] = await Promise.all([
    dialogDeviceName.count(),
    dialogDeviceName.textContent(),
  ]);
  if (dialogNameCount !== 1 || dialogNameText?.trim() !== target.deviceName) {
    throw new Error(
      `${label}: removal confirmation target mismatch row=${JSON.stringify(target.deviceName)} `
        + `dialog=${JSON.stringify(dialogNameText?.trim() ?? null)} count=${dialogNameCount}`,
    );
  }
  if (requestVisibleMarker) console.log(`E2E: ${requestVisibleMarker}`);
  await dialog.getByTestId('confirm-remove-device').click();
  await dialog.waitFor({ state: 'hidden', timeout: 120_000 });
  writeDiagnostic(
    `[TEST${testNumber}] ${label} visible Web trash+confirmation completed target=${targetCategory}`,
  );

  if (waitForTargetGone) {
    await categoryText.waitFor({ state: 'hidden', timeout: 180_000 });
    writeDiagnostic(`[TEST${testNumber}] ${label} target row removed from Web`);
  }
  if (completionMarker) console.log(`E2E: ${completionMarker}`);
  return { row: targetRow, deviceName: target.deviceName, category: targetCategory };
}

async function declinePendingWebDeviceRemoval(page, {
  targetCategory,
  expectedTargetDeviceName,
  label,
  requestVisibleMarker,
  completionMarker,
}) {
  if (typeof expectedTargetDeviceName !== 'string' || expectedTargetDeviceName.length === 0) {
    throw new Error(`${label}: exact observed registered target name is required`);
  }
  // The requester's completion marker proves persistence. Reload fetches that
  // authoritative Core state before the Web decliner interacts with the row,
  // avoiding any timing assumption about SSE delivery.
  await page.reload({ waitUntil: 'domcontentloaded' });
  await unlockWithPasskeyIfNeeded(page);
  await page.getByRole('link', { name: 'Devices', exact: true }).click();
  const target = await inspectWebMemberRow(page, targetCategory, {
    current: false,
    expectedDeviceName: expectedTargetDeviceName,
    label,
  });
  const removeButton = target.row.getByTestId('remove-device');
  await removeButton.waitFor({ state: 'visible', timeout: 30_000 });
  await removeButton.click();

  const dialog = page.getByTestId('remove-device-dialog');
  await dialog.waitFor({ state: 'visible', timeout: 30_000 });
  await dialog.getByText(targetCategory, { exact: true }).waitFor({ state: 'visible', timeout: 30_000 });
  const dialogDeviceName = dialog.locator('[data-slot="dialog-title"] span.text-primary');
  await dialogDeviceName.waitFor({ state: 'visible', timeout: 30_000 });
  const [dialogNameCount, dialogNameText] = await Promise.all([
    dialogDeviceName.count(),
    dialogDeviceName.textContent(),
  ]);
  if (dialogNameCount !== 1 || dialogNameText?.trim() !== target.deviceName) {
    throw new Error(
      `${label}: decline target mismatch row=${JSON.stringify(target.deviceName)} `
        + `dialog=${JSON.stringify(dialogNameText?.trim() ?? null)} count=${dialogNameCount}`,
    );
  }
  if (requestVisibleMarker) console.log(`E2E: ${requestVisibleMarker}`);
  const decline = dialog.getByTestId('decline-remove-device');
  await decline.waitFor({ state: 'visible', timeout: 30_000 });
  await decline.click();
  await dialog.waitFor({ state: 'hidden', timeout: 120_000 });

  // Fetch the terminal event, rather than treating a closed dialog as proof
  // that the signed Decline reached Core.
  await page.reload({ waitUntil: 'domcontentloaded' });
  await unlockWithPasskeyIfNeeded(page);
  writeDiagnostic(
    `[TEST${testNumber}] ${label} visible Web decline completed target=${targetCategory}`,
  );
  if (completionMarker) console.log(`E2E: ${completionMarker}`);
}

async function runWebMemberRemovalLifecycle(page, simulatorUdid, androidSerial) {
  const allSecretNames = ['secret1', 'secret2', 'secret3'].map(secretNameFor);
  const firstRemoval = scenario.membership?.firstRemoval;
  const secondRemoval = scenario.membership?.secondRemoval;
  if ((scenario.membership?.initialPlatforms ?? []).join(',') !== 'web,android,ios') {
    throw new Error('Test #37 requires the Web, Android, iOS three-member boundary');
  }
  if (
    firstRemoval?.requester !== 'web'
    || firstRemoval?.approver !== 'android'
    || firstRemoval?.target !== 'ios'
    || firstRemoval?.expectedMemberCount !== 2
  ) {
    throw new Error('Test #37 first removal must be Web -> Android approval -> iOS target');
  }
  if (
    secondRemoval?.requester !== 'web'
    || secondRemoval?.approver !== null
    || secondRemoval?.target !== 'android'
    || secondRemoval?.expectedMemberCount !== 1
  ) {
    throw new Error('Test #37 second removal must be Web local two-to-one removal of Android');
  }
  if (recoveryCycles.length !== 9 || recoveryCycles.some((cycle) => (
    cycle.kind !== 'direct-show' || cycle.approvals.length !== 0 || cycle.senders.length !== 1
  ))) {
    throw new Error('Test #37 requires exactly nine single-sender direct-Show cycles and zero recovery');
  }
  const firstBlock = recoveryCycles.slice(0, 6);
  const finalBlock = recoveryCycles.slice(6);
  const actualShowMatrix = recoveryCycles.map((cycle) => (
    `${cycle.block}:${cycle.senders[0]}:${cycle.senderSecrets[cycle.senders[0]]}`
  ));
  const expectedShowMatrix = [
    ...allSecretNames.map((name) => `after-ios-removal:web:${name}`),
    ...allSecretNames.map((name) => `after-ios-removal:android:${name}`),
    ...allSecretNames.map((name) => `after-android-removal:web:${name}`),
  ];
  if (actualShowMatrix.join('|') !== expectedShowMatrix.join('|')) {
    throw new Error('Test #37 direct-Show matrix does not match the two removal boundaries');
  }

  const setup = await runWebMultiSecretLifecycleSetup(page, simulatorUdid, androidSerial);
  await setup.androidTest.result;

  const secondName = secretNameFor('secret2');
  await createWebSecret(page, secondName);
  await waitForWebSecretNames(page, allSecretNames.slice(0, 2));
  const androidSecret2Check = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.lifecycleTestClass,
    testMethod: 'assertSecretsVisibleForTest37',
    role: 'redistribution-check',
    cycle: 2,
    step: 1,
    secretName: secondName,
    expectedSecretNames: allSecretNames.slice(0, 2),
    label: 'Android Test #37 verifies Secret2 before iOS Join',
  });
  await androidSecret2Check.waitForMarker('E2E: ANDROID_TEST37_SECRETS_VISIBLE_2', 180_000);
  await androidSecret2Check.result;

  await runWebMultiSecretLifecycleIosJoin(
    page,
    simulatorUdid,
    allSecretNames.slice(0, 2),
    androidSerial,
  );

  const thirdName = secretNameFor('secret3');
  await createWebSecret(page, thirdName);
  await waitForWebSecretNames(page, allSecretNames);
  setJoinExpectedSecretNames(allSecretNames);
  const expectedDeviceNames = await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'three-device boundary',
  );
  writeDiagnostic(
    '[TEST37] canonical registered device identities discovered from exact Web rows '
      + `web=${JSON.stringify(expectedDeviceNames.web)} `
      + `android=${JSON.stringify(expectedDeviceNames.android)} `
      + `ios=${JSON.stringify(expectedDeviceNames.ios)}`,
  );

  const androidRemovalApproval = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.lifecycleTestClass,
    testMethod: scenario.android.approveRemovalTestMethod,
    role: 'removal-approver',
    cycle: 1,
    step: 1,
    joinTarget: expectedDeviceNames.ios,
    expectedSecretNames: allSecretNames,
    expectedDeviceNames,
    label: 'Android Test #37 approves iOS removal',
  });
  const iosRemovalObserver = startIosJoinTest(simulatorUdid, {
    testClass: scenario.ios.testClass,
    testMethod: scenario.ios.removalObserverTestMethod,
    label: 'iOS Test #37 observes own removal',
    environment: stepEnvironment({ role: 'removal-target', cycle: 1, step: 1, sender: 'web' }),
    secretName: thirdName,
    secretValue: secretValueForName(thirdName),
    expectedDeviceNames,
  });
  void iosRemovalObserver.result.catch(() => {});
  await Promise.all([
    androidRemovalApproval.waitForMarker('E2E: ANDROID_TEST37_REMOVAL_RECEIVER_READY', 180_000),
    // XCTest owns a 180-second bounded UI predicate. Leave a full minute for
    // its assertion/result diagnostics instead of terminating at the same edge.
    iosRemovalObserver.waitForMarker('E2E: IOS_TEST37_REMOVAL_TARGET_READY', 240_000),
  ]);

  await initiateWebDeviceRemoval(page, {
    targetCategory: 'iPhone',
    expectedTargetDeviceName: expectedDeviceNames.ios,
    waitForTargetGone: false,
    label: 'three-to-two removal request',
  });
  await androidRemovalApproval.waitForMarker(
    'E2E: ANDROID_TEST37_REMOVAL_REQUEST_VISIBLE',
    180_000,
  );
  approvalCoordinator.allow('android-remove-ios-approve', 1);
  await androidRemovalApproval.waitForMarker(
    'E2E: ANDROID_TEST37_IOS_REMOVAL_APPROVED',
    180_000,
  );
  await Promise.all([
    androidRemovalApproval.result,
    iosRemovalObserver.waitForMarker('E2E: IOS_TEST37_REMOVED_RESET', 180_000),
  ]);
  await iosRemovalObserver.result;
  await assertWebMembership(
    page,
    ['Web', 'Android'],
    allSecretNames,
    'two-device reshared boundary',
    expectedDeviceNames,
  );

  for (const cycle of firstBlock) {
    setCurrentCycleForDiagnostics(cycle);
    const sender = cycle.senders[0];
    const secretName = cycle.senderSecrets[sender];
    if (sender === 'web') {
      await showAndCloseWebSecret(page, secretName);
    } else {
      const show = await startAndroidStepTest(androidSerial, {
        testClass: scenario.android.lifecycleTestClass,
        testMethod: scenario.android.showSecretTestMethod,
        role: 'direct-show',
        cycle: cycle.number,
        step: 1,
        secretName,
        secretValue: secretValueForName(secretName),
        expectedSecretNames: allSecretNames,
        label: `Android Test #37 direct Show cycle ${cycle.number}`,
      });
      await show.waitForMarker(
        `E2E: ANDROID_TEST37_LOCAL_SECRET_VALUE_VERIFIED_${cycle.number}`,
        180_000,
      );
      await show.waitForMarker(`E2E: ANDROID_TEST37_LOCAL_SECRET_CLOSED_${cycle.number}`, 60_000);
      await show.result;
    }
    await assertNoStaleWebMembershipUi(page, allSecretNames);
    writeDiagnostic(
      `[TEST37][after-ios-removal][cycle=${cycle.number}] sender=${sender} `
        + `secret=${secretName} directShow=true exactValue=verified closed=true`,
    );
  }

  await assertWebMembership(
    page,
    ['Web', 'Android'],
    allSecretNames,
    'pre-two-to-one boundary',
    expectedDeviceNames,
  );
  const androidRemovalTarget = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.lifecycleTestClass,
    testMethod: scenario.android.removalObserverTestMethod,
    role: 'removal-target',
    cycle: 2,
    step: 1,
    expectedSecretNames: allSecretNames,
    expectedDeviceNames,
    label: 'Android Test #37 observes own removal',
  });
  await androidRemovalTarget.waitForMarker('E2E: ANDROID_TEST37_REMOVAL_TARGET_READY', 180_000);
  await initiateWebDeviceRemoval(page, {
    targetCategory: 'Android',
    expectedTargetDeviceName: expectedDeviceNames.android,
    waitForTargetGone: true,
    label: 'two-to-one local removal',
  });
  await androidRemovalTarget.waitForMarker('E2E: ANDROID_TEST37_REMOVED_RESET', 180_000);
  await androidRemovalTarget.result;
  await assertWebMembership(
    page,
    ['Web'],
    allSecretNames,
    'single-device boundary',
    expectedDeviceNames,
  );

  for (const cycle of finalBlock) {
    setCurrentCycleForDiagnostics(cycle);
    const secretName = cycle.senderSecrets.web;
    await showAndCloseWebSecret(page, secretName);
    await assertNoStaleWebMembershipUi(page, allSecretNames);
    writeDiagnostic(
      `[TEST37][after-android-removal][cycle=${cycle.number}] sender=web `
        + `secret=${secretName} directShow=true exactValue=verified closed=true`,
    );
  }
  await assertWebMembership(
    page,
    ['Web'],
    allSecretNames,
    'final single-device boundary',
    expectedDeviceNames,
  );
  return { completedCycles: recoveryCycles.length };
}

async function runWebSecondaryCreatorMemberRemovalLifecycle(page, simulatorUdid, androidSerial) {
  const allSecretNames = ['secret1', 'secret2', 'secret3'].map(secretNameFor);
  const [initialName, secondName, thirdName] = allSecretNames;
  const firstRemoval = scenario.membership?.firstRemoval;
  const secondRemoval = scenario.membership?.secondRemoval;
  if ((scenario.membership?.initialPlatforms ?? []).join(',') !== 'web,android,ios') {
    throw new Error('Test #40 requires the Web, Android, iOS three-member boundary');
  }
  if ((scenario.setup?.joinOrder ?? []).join(',') !== 'android,ios') {
    throw new Error('Test #40 requires Android then iOS Join order');
  }
  if (scenario.setup?.secondJoinApprover !== 'web') {
    throw new Error('Test #40 requires Web as the sole approver of both Joins');
  }
  if (
    (secretCreationPlan?.initial?.web ?? []).join(',') !== initialName
    || (secretCreationPlan?.afterAndroidJoin?.android ?? []).join(',') !== secondName
    || (secretCreationPlan?.afterIosJoin?.android ?? []).join(',') !== thirdName
  ) {
    throw new Error('Test #40 requires Web Secret1, then Android Secret2 and Secret3');
  }
  if (
    firstRemoval?.requester !== 'web'
    || firstRemoval?.approver !== 'ios'
    || firstRemoval?.target !== 'android'
    || firstRemoval?.expectedMemberCount !== 2
  ) {
    throw new Error('Test #40 first removal must be Web -> iOS approval -> Android target');
  }
  if (
    secondRemoval?.requester !== 'ios'
    || secondRemoval?.approver !== null
    || secondRemoval?.target !== 'web'
    || secondRemoval?.expectedMemberCount !== 1
  ) {
    throw new Error('Test #40 second removal must be iOS local two-to-one removal of Web');
  }
  if (recoveryCycles.length !== 9 || recoveryCycles.some((cycle) => (
    cycle.kind !== 'direct-show' || cycle.approvals.length !== 0 || cycle.senders.length !== 1
  ))) {
    throw new Error('Test #40 requires exactly nine single-sender direct-Show cycles and zero recovery');
  }
  const firstBlock = recoveryCycles.slice(0, 6);
  const finalBlock = recoveryCycles.slice(6);
  const actualShowMatrix = recoveryCycles.map((cycle) => (
    `${cycle.block}:${cycle.senders[0]}:${cycle.senderSecrets[cycle.senders[0]]}`
  ));
  const expectedShowMatrix = [
    ...allSecretNames.map((name) => `after-android-removal:web:${name}`),
    ...allSecretNames.map((name) => `after-android-removal:ios:${name}`),
    ...allSecretNames.map((name) => `after-web-removal:ios:${name}`),
  ];
  if (actualShowMatrix.join('|') !== expectedShowMatrix.join('|')) {
    throw new Error('Test #40 direct-Show matrix does not match the two removal boundaries');
  }

  const startIosAction = ({
    testMethod,
    role,
    cycle = 0,
    secretName,
    sender = '',
    expectedDeviceNames,
    label,
  }) => startIosJoinTest(simulatorUdid, {
    testClass: scenario.ios.testClass,
    testMethod,
    label,
    environment: stepEnvironment({ role, cycle, step: 1, sender, secretName }),
    secretName,
    secretValue: secretValueForName(secretName),
    expectedDeviceNames,
  });

  setJoinExpectedSecretNames([initialName]);
  await createWebInitiatedVault(page);
  console.log('E2E: WEB_TEST40_SECRET1_READY');
  console.log('E2E: WEB_TEST40_INITIATOR_SETUP_READY');

  const androidJoin = await startAndroidJoinTest(androidSerial, {
    testClass: scenario.android.testClass,
    testMethod: scenario.android.joinTestMethod,
    expectedSecretNames: [initialName],
  });
  await androidJoin.waitForMarker('E2E: ANDROID_TEST40_JOIN_REQUEST_SENT', 180_000);
  await approveJoinRequestOnWeb(page, 'Android', { strictCompletion: true });
  console.log('E2E: WEB_TEST40_ANDROID_JOIN_APPROVED');
  await androidJoin.waitForMarker('E2E: ANDROID_TEST40_JOIN_READY', 180_000);
  await androidJoin.waitForMarker('E2E: ANDROID_TEST40_SECRETS_READY', 60_000);
  await androidJoin.result;
  await waitForWebSecretNames(page, [initialName]);

  const createSecond = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.creatorTestClass,
    testMethod: scenario.android.createSecretTestMethod,
    role: 'creator',
    cycle: 2,
    step: 1,
    secretName: secondName,
    secretValue: secretValueForName(secondName),
    expectedSecretNames: [initialName, secondName],
    label: 'Android Test #40 creates Secret2',
  });
  await createSecond.waitForMarker('E2E: ANDROID_TEST40_SECRET_CREATED_2', 180_000);
  await createSecond.result;
  setJoinExpectedSecretNames([initialName, secondName]);
  await waitForWebSecretNames(page, [initialName, secondName]);

  const iosJoin = startIosJoinTest(simulatorUdid, {
    testClass: scenario.ios.testClass,
    testMethod: scenario.ios.joinTestMethod,
    label: 'iOS Test #40 joins Web-owned Vault',
  });
  void iosJoin.result.catch(() => {});
  await iosJoin.waitForMarker('E2E: IOS_TEST40_JOIN_REQUEST_SENT', 300_000);
  await approveJoinRequestOnWeb(page, 'iOS', { strictCompletion: true });
  console.log('E2E: WEB_TEST40_IOS_JOIN_APPROVED');
  await iosJoin.waitForMarker('E2E: IOS_TEST40_JOIN_READY', 180_000);
  await iosJoin.waitForMarker('E2E: IOS_TEST40_SECRETS_READY', 60_000);
  await iosJoin.result;
  await waitForWebSecretNames(page, [initialName, secondName]);

  const createThird = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.creatorTestClass,
    testMethod: scenario.android.createSecretTestMethod,
    role: 'creator',
    cycle: 3,
    step: 1,
    secretName: thirdName,
    secretValue: secretValueForName(thirdName),
    expectedSecretNames: allSecretNames,
    label: 'Android Test #40 creates Secret3',
  });
  await createThird.waitForMarker('E2E: ANDROID_TEST40_SECRET_CREATED_3', 180_000);
  await createThird.result;
  setJoinExpectedSecretNames(allSecretNames);
  await waitForWebSecretNames(page, allSecretNames);

  const expectedDeviceNames = await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #40 three-device boundary',
  );
  writeDiagnostic(
    '[TEST40] canonical registered device identities discovered from exact Web rows '
      + `web=${JSON.stringify(expectedDeviceNames.web)} `
      + `android=${JSON.stringify(expectedDeviceNames.android)} `
      + `ios=${JSON.stringify(expectedDeviceNames.ios)}`,
  );

  const androidRemovalTarget = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.lifecycleTestClass,
    testMethod: scenario.android.removalObserverTestMethod,
    role: 'removal-target',
    cycle: 1,
    step: 1,
    expectedSecretNames: allSecretNames,
    expectedDeviceNames,
    label: 'Android Test #40 observes own removal',
  });
  const iosRemovalApproval = startIosAction({
    testMethod: scenario.ios.approveRemovalTestMethod,
    role: 'removal-approver',
    cycle: 1,
    secretName: thirdName,
    sender: 'web',
    expectedDeviceNames,
    label: 'iOS Test #40 approves Android removal',
  });
  void iosRemovalApproval.result.catch(() => {});
  await Promise.all([
    androidRemovalTarget.waitForMarker('E2E: ANDROID_TEST40_REMOVAL_TARGET_READY', 180_000),
    iosRemovalApproval.waitForMarker('E2E: IOS_TEST40_REMOVAL_RECEIVER_READY', 240_000),
  ]);
  await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #40 Web removal requester boundary',
    expectedDeviceNames,
  );
  console.log('E2E: WEB_TEST40_ANDROID_REMOVAL_REQUESTER_READY');

  await initiateWebDeviceRemoval(page, {
    targetCategory: 'Android',
    expectedTargetDeviceName: expectedDeviceNames.android,
    waitForTargetGone: false,
    label: 'Test #40 Web request to remove Android',
  });
  await iosRemovalApproval.waitForMarker('E2E: IOS_TEST40_REMOVAL_REQUEST_VISIBLE', 180_000);
  // The receiver's exact pending row/dialog is the persistence boundary for
  // the Web request. Do not emit a completed request marker for the click.
  console.log('E2E: WEB_TEST40_ANDROID_REMOVAL_REQUESTED');
  approvalCoordinator.allow('ios-remove-android-approve-test40', 1);
  await Promise.all([
    iosRemovalApproval.waitForMarker('E2E: IOS_TEST40_ANDROID_REMOVAL_APPROVED', 180_000),
    androidRemovalTarget.waitForMarker('E2E: ANDROID_TEST40_REMOVED_RESET', 180_000),
  ]);
  await Promise.all([iosRemovalApproval.result, androidRemovalTarget.result]);
  await assertWebMembership(
    page,
    ['Web', 'iPhone'],
    allSecretNames,
    'Test #40 two-device reshared boundary',
    expectedDeviceNames,
  );
  console.log('E2E: WEB_TEST40_ANDROID_REMOVAL_COMPLETED');

  for (const cycle of firstBlock) {
    setCurrentCycleForDiagnostics(cycle);
    const sender = cycle.senders[0];
    const secretName = cycle.senderSecrets[sender];
    if (sender === 'web') {
      await showAndCloseWebSecret(page, secretName);
      console.log(`E2E: WEB_TEST40_LOCAL_SECRET_VALUE_VERIFIED_${cycle.number}`);
      console.log(`E2E: WEB_TEST40_LOCAL_SECRET_CLOSED_${cycle.number}`);
    } else if (sender === 'ios') {
      const show = startIosAction({
        testMethod: scenario.ios.showSecretTestMethod,
        role: 'direct-show',
        cycle: cycle.number,
        secretName,
        label: `iOS Test #40 direct Show cycle ${cycle.number}`,
      });
      void show.result.catch(() => {});
      await show.waitForMarker(
        `E2E: IOS_TEST40_LOCAL_SECRET_VALUE_VERIFIED_${cycle.number}`,
        180_000,
      );
      await show.waitForMarker(`E2E: IOS_TEST40_LOCAL_SECRET_VISIBLE_${cycle.number}`, 60_000);
      await show.waitForMarker(`E2E: IOS_TEST40_LOCAL_SECRET_CLOSED_${cycle.number}`, 60_000);
      await show.result;
    } else {
      throw new Error(`Test #40 first direct-Show block has unsupported sender=${sender}`);
    }
    await assertNoStaleWebMembershipUi(page, allSecretNames);
    writeDiagnostic(
      `[TEST40][after-android-removal][cycle=${cycle.number}] sender=${sender} `
        + `secret=${secretName} directShow=true exactValue=verified closed=true`,
    );
  }

  await assertWebMembership(
    page,
    ['Web', 'iPhone'],
    allSecretNames,
    'Test #40 pre-two-to-one boundary',
    expectedDeviceNames,
  );
  console.log('E2E: WEB_TEST40_REMOVAL_TARGET_READY');
  const iosLocalRemoval = startIosAction({
    testMethod: scenario.ios.localRemovalTestMethod,
    role: 'removal-requester',
    cycle: 2,
    secretName: thirdName,
    sender: 'ios',
    expectedDeviceNames,
    label: 'iOS Test #40 removes Web locally',
  });
  void iosLocalRemoval.result.catch(() => {});
  await iosLocalRemoval.waitForMarker('E2E: IOS_TEST40_WEB_LOCAL_REMOVAL_READY', 240_000);
  approvalCoordinator.allow('ios-remove-web-local-test40', 2);
  await iosLocalRemoval.waitForMarker(
    'E2E: IOS_TEST40_WEB_LOCAL_REMOVAL_REQUESTED',
    180_000,
  );
  await Promise.all([
    assertWebResetAfterOwnRemoval(page, allSecretNames),
    iosLocalRemoval.waitForMarker(
      'E2E: IOS_TEST40_WEB_LOCAL_REMOVAL_COMPLETED',
      180_000,
    ),
  ]);
  await iosLocalRemoval.result;

  for (const cycle of finalBlock) {
    setCurrentCycleForDiagnostics(cycle);
    const secretName = cycle.senderSecrets.ios;
    const show = startIosAction({
      testMethod: scenario.ios.showSecretTestMethod,
      role: 'direct-show',
      cycle: cycle.number,
      secretName,
      label: `iOS Test #40 single-member Show cycle ${cycle.number}`,
    });
    void show.result.catch(() => {});
    await show.waitForMarker(
      `E2E: IOS_TEST40_LOCAL_SECRET_VALUE_VERIFIED_${cycle.number}`,
      180_000,
    );
    await show.waitForMarker(`E2E: IOS_TEST40_LOCAL_SECRET_VISIBLE_${cycle.number}`, 60_000);
    await show.waitForMarker(`E2E: IOS_TEST40_LOCAL_SECRET_CLOSED_${cycle.number}`, 60_000);
    await show.result;
    writeDiagnostic(
      `[TEST40][after-web-removal][cycle=${cycle.number}] sender=ios `
        + `secret=${secretName} directShow=true exactValue=verified closed=true`,
    );
  }
  return { completedCycles: recoveryCycles.length };
}

async function runWebDistributedCreatorMemberRemovalLifecycle(page, simulatorUdid, androidSerial) {
  const allSecretNames = ['secret1', 'secret2', 'secret3'].map(secretNameFor);
  const [initialName, secondName, thirdName] = allSecretNames;
  const firstRemoval = scenario.membership?.firstRemoval;
  const secondRemoval = scenario.membership?.secondRemoval;
  if (testNumber !== 43) {
    throw new Error(`Distributed-creator member-removal lifecycle is not defined for Test #${testNumber}`);
  }
  if ((scenario.membership?.initialPlatforms ?? []).join(',') !== 'web,android,ios') {
    throw new Error('Test #43 requires the Web, Android, iOS three-member boundary');
  }
  if ((scenario.setup?.joinOrder ?? []).join(',') !== 'android,ios') {
    throw new Error('Test #43 requires Android then iOS Join order');
  }
  if (scenario.setup?.secondJoinApprover !== 'web') {
    throw new Error('Test #43 requires Web as the sole approver of both Joins');
  }
  if (
    (secretCreationPlan?.initial?.web ?? []).join(',') !== initialName
    || (secretCreationPlan?.afterAndroidJoin?.android ?? []).join(',') !== secondName
    || (secretCreationPlan?.afterIosJoin?.ios ?? []).join(',') !== thirdName
  ) {
    throw new Error('Test #43 requires Web Secret1, Android Secret2, then iOS Secret3');
  }
  if (
    firstRemoval?.requester !== 'ios'
    || firstRemoval?.approver !== 'android'
    || firstRemoval?.target !== 'web'
    || firstRemoval?.expectedMemberCount !== 2
  ) {
    throw new Error('Test #43 first removal must be iOS -> Android approval -> Web target');
  }
  if (
    secondRemoval?.requester !== 'android'
    || secondRemoval?.approver !== null
    || secondRemoval?.target !== 'ios'
    || secondRemoval?.expectedMemberCount !== 1
  ) {
    throw new Error('Test #43 second removal must be Android local two-to-one removal of iOS');
  }
  if (recoveryCycles.length !== 9 || recoveryCycles.some((cycle) => (
    cycle.kind !== 'direct-show' || cycle.approvals.length !== 0 || cycle.senders.length !== 1
  ))) {
    throw new Error('Test #43 requires exactly nine single-sender direct-Show cycles and zero recovery');
  }
  const firstBlock = recoveryCycles.slice(0, 6);
  const finalBlock = recoveryCycles.slice(6);
  const actualShowMatrix = recoveryCycles.map((cycle) => (
    `${cycle.block}:${cycle.senders[0]}:${cycle.senderSecrets[cycle.senders[0]]}`
  ));
  const expectedShowMatrix = [
    ...allSecretNames.map((name) => `after-web-removal:android:${name}`),
    ...allSecretNames.map((name) => `after-web-removal:ios:${name}`),
    ...allSecretNames.map((name) => `after-ios-removal:android:${name}`),
  ];
  if (actualShowMatrix.join('|') !== expectedShowMatrix.join('|')) {
    throw new Error('Test #43 direct-Show matrix does not match the two removal boundaries');
  }

  const startIosAction = ({
    testMethod,
    role,
    cycle = 0,
    secretName,
    sender = '',
    expectedDeviceNames,
    label,
  }) => startIosJoinTest(simulatorUdid, {
    testClass: scenario.ios.testClass,
    testMethod,
    label,
    environment: stepEnvironment({ role, cycle, step: 1, sender, secretName }),
    secretName,
    secretValue: secretValueForName(secretName),
    expectedDeviceNames,
  });

  setJoinExpectedSecretNames([initialName]);
  await createWebInitiatedVault(page);
  console.log('E2E: WEB_TEST43_SECRET1_READY');
  console.log('E2E: WEB_TEST43_INITIATOR_SETUP_READY');

  const androidJoin = await startAndroidJoinTest(androidSerial, {
    testClass: scenario.android.testClass,
    testMethod: scenario.android.joinTestMethod,
    expectedSecretNames: [initialName],
  });
  await androidJoin.waitForMarker('E2E: ANDROID_TEST43_JOIN_REQUEST_SENT', 180_000);
  await approveJoinRequestOnWeb(page, 'Android', { strictCompletion: true });
  console.log('E2E: WEB_TEST43_ANDROID_JOIN_APPROVED');
  await androidJoin.waitForMarker('E2E: ANDROID_TEST43_JOIN_READY', 180_000);
  await androidJoin.waitForMarker('E2E: ANDROID_TEST43_SECRETS_READY', 60_000);
  await androidJoin.result;
  await waitForWebSecretNames(page, [initialName]);

  const createSecond = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.creatorTestClass,
    testMethod: scenario.android.createSecretTestMethod,
    role: 'creator',
    cycle: 2,
    step: 1,
    secretName: secondName,
    secretValue: secretValueForName(secondName),
    expectedSecretNames: [initialName, secondName],
    label: 'Android Test #43 creates Secret2',
  });
  await createSecond.waitForMarker('E2E: ANDROID_TEST43_SECRET_CREATED_2', 180_000);
  await createSecond.result;
  setJoinExpectedSecretNames([initialName, secondName]);
  await waitForWebSecretNames(page, [initialName, secondName]);

  const iosJoin = startIosJoinTest(simulatorUdid, {
    testClass: scenario.ios.testClass,
    testMethod: scenario.ios.joinTestMethod,
    label: 'iOS Test #43 joins Web-owned Vault',
  });
  void iosJoin.result.catch(() => {});
  await iosJoin.waitForMarker('E2E: IOS_TEST43_JOIN_REQUEST_SENT', 300_000);
  await approveJoinRequestOnWeb(page, 'iOS', { strictCompletion: true });
  console.log('E2E: WEB_TEST43_IOS_JOIN_APPROVED');
  await iosJoin.waitForMarker('E2E: IOS_TEST43_JOIN_READY', 180_000);
  await iosJoin.waitForMarker('E2E: IOS_TEST43_SECRETS_READY', 60_000);
  await iosJoin.result;
  await waitForWebSecretNames(page, [initialName, secondName]);

  const createThird = startIosAction({
    testMethod: scenario.ios.createSecretTestMethod,
    role: 'creator',
    cycle: 3,
    secretName: thirdName,
    label: 'iOS Test #43 creates Secret3',
  });
  void createThird.result.catch(() => {});
  await createThird.waitForMarker('E2E: IOS_TEST43_SECRET_CREATED_3', 180_000);
  await createThird.result;
  setJoinExpectedSecretNames(allSecretNames);
  await waitForWebSecretNames(page, allSecretNames);

  const expectedDeviceNames = await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #43 three-device boundary',
  );
  writeDiagnostic(
    '[TEST43] canonical registered device identities discovered from exact Web rows '
      + `web=${JSON.stringify(expectedDeviceNames.web)} `
      + `android=${JSON.stringify(expectedDeviceNames.android)} `
      + `ios=${JSON.stringify(expectedDeviceNames.ios)}`,
  );

  const androidRemovalApproval = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.lifecycleTestClass,
    testMethod: scenario.android.approveRemovalTestMethod,
    role: 'removal-approver',
    cycle: 1,
    step: 1,
    joinTarget: expectedDeviceNames.web,
    expectedSecretNames: allSecretNames,
    expectedDeviceNames,
    label: 'Android Test #43 approves Web removal',
  });
  const iosRemovalRequester = startIosAction({
    testMethod: scenario.ios.requestRemovalTestMethod,
    role: 'removal-requester',
    cycle: 1,
    secretName: thirdName,
    sender: 'ios',
    expectedDeviceNames,
    label: 'iOS Test #43 requests Web removal',
  });
  void iosRemovalRequester.result.catch(() => {});
  await Promise.all([
    androidRemovalApproval.waitForMarker('E2E: ANDROID_TEST43_REMOVAL_RECEIVER_READY', 180_000),
    iosRemovalRequester.waitForMarker('E2E: IOS_TEST43_WEB_REMOVAL_REQUESTER_READY', 240_000),
  ]);
  await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #43 Web passive-target boundary',
    expectedDeviceNames,
  );
  console.log('E2E: WEB_TEST43_REMOVAL_TARGET_READY');

  approvalCoordinator.allow('ios-remove-web-start-test43', 1);
  await iosRemovalRequester.waitForMarker('E2E: IOS_TEST43_WEB_REMOVAL_REQUESTED', 180_000);
  await androidRemovalApproval.waitForMarker('E2E: ANDROID_TEST43_REMOVAL_REQUEST_VISIBLE', 180_000);
  const webReset = assertWebResetAfterOwnRemoval(page, allSecretNames);
  approvalCoordinator.allow('android-remove-web-approve-test43', 1);
  await Promise.all([
    androidRemovalApproval.waitForMarker('E2E: ANDROID_TEST43_WEB_REMOVAL_APPROVED', 180_000),
    iosRemovalRequester.waitForMarker('E2E: IOS_TEST43_WEB_REMOVAL_COMPLETED', 180_000),
    webReset,
  ]);
  await Promise.all([androidRemovalApproval.result, iosRemovalRequester.result]);

  for (const cycle of firstBlock) {
    setCurrentCycleForDiagnostics(cycle);
    const sender = cycle.senders[0];
    const secretName = cycle.senderSecrets[sender];
    if (sender === 'android') {
      const show = await startAndroidStepTest(androidSerial, {
        testClass: scenario.android.lifecycleTestClass,
        testMethod: scenario.android.showSecretTestMethod,
        role: 'direct-show',
        cycle: cycle.number,
        step: 1,
        secretName,
        secretValue: secretValueForName(secretName),
        expectedSecretNames: allSecretNames,
        label: `Android Test #43 direct Show cycle ${cycle.number}`,
      });
      await show.waitForMarker(
        `E2E: ANDROID_TEST43_LOCAL_SECRET_VALUE_VERIFIED_${cycle.number}`,
        180_000,
      );
      await show.waitForMarker(`E2E: ANDROID_TEST43_LOCAL_SECRET_CLOSED_${cycle.number}`, 60_000);
      await show.result;
    } else if (sender === 'ios') {
      const show = startIosAction({
        testMethod: scenario.ios.showSecretTestMethod,
        role: 'direct-show',
        cycle: cycle.number,
        secretName,
        label: `iOS Test #43 direct Show cycle ${cycle.number}`,
      });
      void show.result.catch(() => {});
      await show.waitForMarker(
        `E2E: IOS_TEST43_LOCAL_SECRET_VALUE_VERIFIED_${cycle.number}`,
        180_000,
      );
      await show.waitForMarker(`E2E: IOS_TEST43_LOCAL_SECRET_VISIBLE_${cycle.number}`, 60_000);
      await show.waitForMarker(`E2E: IOS_TEST43_LOCAL_SECRET_CLOSED_${cycle.number}`, 60_000);
      await show.result;
    } else {
      throw new Error(`Test #43 first direct-Show block has unsupported sender=${sender}`);
    }
    writeDiagnostic(
      `[TEST43][after-web-removal][cycle=${cycle.number}] sender=${sender} `
        + `secret=${secretName} directShow=true exactValue=verified closed=true`,
    );
  }

  const iosRemovalTarget = startIosAction({
    testMethod: scenario.ios.removalObserverTestMethod,
    role: 'removal-target',
    cycle: 2,
    secretName: thirdName,
    sender: 'android',
    expectedDeviceNames,
    label: 'iOS Test #43 observes own local removal',
  });
  void iosRemovalTarget.result.catch(() => {});
  const androidLocalRemoval = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.lifecycleTestClass,
    testMethod: scenario.android.localRemovalTestMethod,
    role: 'removal-requester',
    cycle: 2,
    step: 1,
    joinTarget: expectedDeviceNames.ios,
    expectedSecretNames: allSecretNames,
    expectedDeviceNames,
    label: 'Android Test #43 removes iOS locally',
  });
  await Promise.all([
    iosRemovalTarget.waitForMarker('E2E: IOS_TEST43_REMOVAL_TARGET_READY', 240_000),
    androidLocalRemoval.waitForMarker('E2E: ANDROID_TEST43_IOS_LOCAL_REMOVAL_READY', 180_000),
  ]);
  approvalCoordinator.allow('android-remove-ios-local-test43', 2);
  await androidLocalRemoval.waitForMarker(
    'E2E: ANDROID_TEST43_IOS_LOCAL_REMOVAL_REQUESTED',
    180_000,
  );
  await Promise.all([
    iosRemovalTarget.waitForMarker('E2E: IOS_TEST43_REMOVED_RESET', 180_000),
    androidLocalRemoval.waitForMarker(
      'E2E: ANDROID_TEST43_IOS_LOCAL_REMOVAL_COMPLETED',
      180_000,
    ),
  ]);
  await Promise.all([iosRemovalTarget.result, androidLocalRemoval.result]);

  for (const cycle of finalBlock) {
    setCurrentCycleForDiagnostics(cycle);
    const secretName = cycle.senderSecrets.android;
    const show = await startAndroidStepTest(androidSerial, {
      testClass: scenario.android.lifecycleTestClass,
      testMethod: scenario.android.showSecretTestMethod,
      role: 'direct-show',
      cycle: cycle.number,
      step: 1,
      secretName,
      secretValue: secretValueForName(secretName),
      expectedSecretNames: allSecretNames,
      label: `Android Test #43 single-member Show cycle ${cycle.number}`,
    });
    await show.waitForMarker(
      `E2E: ANDROID_TEST43_LOCAL_SECRET_VALUE_VERIFIED_${cycle.number}`,
      180_000,
    );
    await show.waitForMarker(`E2E: ANDROID_TEST43_LOCAL_SECRET_CLOSED_${cycle.number}`, 60_000);
    await show.result;
    writeDiagnostic(
      `[TEST43][after-ios-removal][cycle=${cycle.number}] sender=android `
        + `secret=${secretName} directShow=true exactValue=verified closed=true`,
    );
  }
  return { completedCycles: recoveryCycles.length };
}

async function runWebDistributedCreatorRemovalRejoinRecoveryLifecycle(
  page,
  simulatorUdid,
  androidSerial,
) {
  const allSecretNames = ['secret1', 'secret2', 'secret3'].map(secretNameFor);
  const [initialName, secondName, thirdName] = allSecretNames;
  const directShowCycles = recoveryCycles.filter((cycle) => cycle.kind === 'direct-show');
  const recoveryOnlyCycles = recoveryCycles.filter((cycle) => cycle.kind !== 'direct-show');
  const firstRemoval = scenario.membership?.firstRemoval;
  const secondRemoval = scenario.membership?.secondRemoval;
  const rejoins = scenario.membership?.rejoins ?? [];
  const verifyBothReceiverAlerts = scenario.recovery.verifyBothReceiverAlerts === true;

  if (testNumber !== 46) {
    throw new Error(`Removal/re-Join/recovery lifecycle is not defined for Test #${testNumber}`);
  }
  if ((scenario.membership?.initialPlatforms ?? []).join(',') !== 'web,android,ios') {
    throw new Error('Test #46 requires the Web, Android, iOS three-member boundary');
  }
  if ((scenario.setup?.joinOrder ?? []).join(',') !== 'android,ios') {
    throw new Error('Test #46 requires Android then iOS Join order');
  }
  if (scenario.setup?.secondJoinApprover !== 'android') {
    throw new Error('Test #46 requires Android B to approve iOS C initial Join');
  }
  if (
    (secretCreationPlan?.initial?.web ?? []).join(',') !== initialName
    || (secretCreationPlan?.afterAndroidJoin?.android ?? []).join(',') !== secondName
    || (secretCreationPlan?.afterIosJoin?.ios ?? []).join(',') !== thirdName
  ) {
    throw new Error('Test #46 requires Web Secret1, Android Secret2, then iOS Secret3');
  }
  if (
    firstRemoval?.requester !== 'android'
    || firstRemoval?.approver !== 'ios'
    || firstRemoval?.target !== 'web'
    || firstRemoval?.expectedMemberCount !== 2
  ) {
    throw new Error('Test #46 first removal must be Android -> iOS approval -> Web target');
  }
  if (
    secondRemoval?.requester !== 'android'
    || secondRemoval?.approver !== null
    || secondRemoval?.target !== 'ios'
    || secondRemoval?.expectedMemberCount !== 1
  ) {
    throw new Error('Test #46 second removal must be Android local two-to-one removal of iOS');
  }
  const expectedRejoins = [
    'web:android:2',
    'ios:web:3',
  ];
  const actualRejoins = rejoins.map((entry) => (
    `${entry.requester}:${entry.approver}:${entry.expectedMemberCount}`
  ));
  if (actualRejoins.join('|') !== expectedRejoins.join('|')) {
    throw new Error('Test #46 requires Web re-Join via Android, then iOS re-Join via Web');
  }
  if (
    recoveryCycles.length !== 33
    || directShowCycles.length !== 6
    || recoveryOnlyCycles.length !== 27
  ) {
    throw new Error(
      `Test #46 requires 6 direct Show plus 27 recovery cycles; got `
        + `${directShowCycles.length}+${recoveryOnlyCycles.length}`,
    );
  }
  if (!verifyBothReceiverAlerts) {
    throw new Error('Test #46 must verify both receiver alerts before each approval');
  }

  const actualDirectShowMatrix = directShowCycles.map((cycle) => {
    const sender = cycle.senders[0];
    return `${cycle.block}:${sender}:${cycle.senderSecrets[sender]}`;
  });
  const expectedDirectShowMatrix = [
    ...allSecretNames.map((name) => `after-web-rejoin:web:${name}`),
    ...allSecretNames.map((name) => `after-web-rejoin:android:${name}`),
  ];
  if (actualDirectShowMatrix.join('|') !== expectedDirectShowMatrix.join('|')) {
    throw new Error('Test #46 direct-Show matrix must cover Web and Android for all Secrets');
  }

  const roleCounts = Object.fromEntries(
    ['web', 'android', 'ios'].map((platform) => [platform, { sender: 0, approver: 0, observer: 0 }]),
  );
  for (const cycle of recoveryOnlyCycles) {
    if (cycle.senders.length !== 1 || cycle.approvals.length !== 1) {
      throw new Error(`Test #46 recovery cycle ${cycle.number} must have one sender and approver`);
    }
    const sender = cycle.senders[0];
    const approver = cycle.approvals[0]?.platform;
    const observer = ['web', 'android', 'ios']
      .find((platform) => platform !== sender && platform !== approver);
    if (!observer || sender === approver || cycle.observer !== observer) {
      throw new Error(`Test #46 recovery cycle ${cycle.number} has an invalid role triple`);
    }
    roleCounts[sender].sender += 1;
    roleCounts[approver].approver += 1;
    roleCounts[observer].observer += 1;
  }
  for (const [platform, counts] of Object.entries(roleCounts)) {
    for (const [role, count] of Object.entries(counts)) {
      if (count !== 9) {
        throw new Error(`Test #46 expected ${platform} ${role}=9, got ${count}`);
      }
    }
  }

  const emitRecoveryMarker = (cycle, action) => {
    const block = cycle.block.toUpperCase().replace(/[^A-Z0-9]+/g, '_');
    console.log(`E2E: TEST46_${block}_CYCLE_${cycle.number}_${action}`);
  };
  const startIosAction = ({
    testMethod,
    role,
    cycle = 0,
    secretName,
    sender = '',
    approvalPlatform = '',
    expectedDeviceNames,
    label,
  }) => startIosJoinTest(simulatorUdid, {
    testClass: scenario.ios.testClass,
    testMethod,
    label,
    environment: stepEnvironment({
      role,
      cycle,
      step: 1,
      sender,
      approvalPlatform,
      verifyReceiverAlert: role === 'receiver' && verifyBothReceiverAlerts,
      secretName,
    }),
    secretName,
    secretValue: secretValueForName(secretName),
    expectedDeviceNames,
  });

  setJoinExpectedSecretNames([initialName]);
  await createWebInitiatedVault(page);
  console.log('E2E: WEB_TEST46_SECRET1_READY');
  console.log('E2E: WEB_TEST46_INITIATOR_SETUP_READY');

  const androidJoin = await startAndroidJoinTest(androidSerial, {
    testClass: scenario.android.testClass,
    testMethod: scenario.android.joinTestMethod,
    expectedSecretNames: [initialName],
  });
  await androidJoin.waitForMarker('E2E: ANDROID_TEST46_INITIAL_JOIN_REQUEST_SENT', 180_000);
  await approveJoinRequestOnWeb(page, 'Android', { strictCompletion: true });
  console.log('E2E: WEB_TEST46_ANDROID_JOIN_APPROVED');
  await androidJoin.waitForMarker('E2E: ANDROID_TEST46_INITIAL_JOIN_READY', 180_000);
  await androidJoin.waitForMarker('E2E: ANDROID_TEST46_INITIAL_SECRETS_READY', 60_000);
  await androidJoin.result;
  await waitForWebSecretNames(page, [initialName]);

  const createSecond = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.creatorTestClass,
    testMethod: scenario.android.createSecretTestMethod,
    role: 'creator',
    cycle: 2,
    step: 1,
    secretName: secondName,
    secretValue: secretValueForName(secondName),
    expectedSecretNames: [initialName, secondName],
    label: 'Android Test #46 creates Secret2',
  });
  await createSecond.waitForMarker('E2E: ANDROID_TEST46_SECRET_CREATED_2', 180_000);
  await createSecond.result;
  setJoinExpectedSecretNames([initialName, secondName]);
  await waitForWebSecretNames(page, [initialName, secondName]);

  const initialIosJoin = startIosJoinTest(simulatorUdid, {
    testClass: scenario.ios.testClass,
    testMethod: scenario.ios.joinTestMethod,
    label: 'iOS Test #46 initial Join approved by Android',
  });
  void initialIosJoin.result.catch(() => {});
  await initialIosJoin.waitForMarker('E2E: IOS_TEST46_INITIAL_JOIN_REQUEST_SENT', 300_000);
  const approveInitialIosJoin = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.joinApproverTestClass,
    testMethod: scenario.android.approveJoinTestMethod,
    role: 'join-approver',
    cycle: 1,
    step: 1,
    joinTarget: 'ios',
    label: 'Android Test #46 approves iOS initial Join',
  });
  await approveInitialIosJoin.waitForMarker('E2E: ANDROID_TEST46_JOIN_APPROVED_IOS_1', 180_000);
  await initialIosJoin.waitForMarker('E2E: IOS_TEST46_INITIAL_JOIN_READY', 180_000);
  await initialIosJoin.waitForMarker('E2E: IOS_TEST46_INITIAL_SECRETS_READY', 60_000);
  await Promise.all([approveInitialIosJoin.result, initialIosJoin.result]);

  const createThird = startIosAction({
    testMethod: scenario.ios.createSecretTestMethod,
    role: 'creator',
    cycle: 3,
    secretName: thirdName,
    label: 'iOS Test #46 creates Secret3',
  });
  void createThird.result.catch(() => {});
  await createThird.waitForMarker('E2E: IOS_TEST46_SECRET_CREATED_3', 180_000);
  await createThird.result;
  setJoinExpectedSecretNames(allSecretNames);
  await waitForWebSecretNames(page, allSecretNames);

  const initialDeviceNames = await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #46 initial three-device boundary',
  );
  writeDiagnostic(
    '[TEST46] initial canonical identities '
      + `web=${JSON.stringify(initialDeviceNames.web)} `
      + `android=${JSON.stringify(initialDeviceNames.android)} `
      + `ios=${JSON.stringify(initialDeviceNames.ios)}`,
  );

  const iosRemovalApproval = startIosAction({
    testMethod: scenario.ios.approveRemovalTestMethod,
    role: 'removal-approver',
    cycle: 1,
    secretName: thirdName,
    sender: 'android',
    expectedDeviceNames: initialDeviceNames,
    label: 'iOS Test #46 approves Web removal',
  });
  void iosRemovalApproval.result.catch(() => {});
  const androidRemovalRequester = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.lifecycleTestClass,
    testMethod: scenario.android.requestRemovalTestMethod,
    role: 'removal-requester',
    cycle: 1,
    step: 1,
    joinTarget: initialDeviceNames.web,
    expectedSecretNames: allSecretNames,
    expectedDeviceNames: initialDeviceNames,
    label: 'Android Test #46 requests Web removal',
  });
  await Promise.all([
    iosRemovalApproval.waitForMarker('E2E: IOS_TEST46_WEB_REMOVAL_RECEIVER_READY', 240_000),
    androidRemovalRequester.waitForMarker(
      'E2E: ANDROID_TEST46_WEB_REMOVAL_REQUESTER_READY',
      180_000,
    ),
  ]);
  await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #46 Web passive-target boundary',
    initialDeviceNames,
  );
  console.log('E2E: WEB_TEST46_REMOVAL_TARGET_READY');

  const webReset = assertWebResetAfterOwnRemoval(page, allSecretNames);
  approvalCoordinator.allow('android-remove-web-start-test46', 1);
  await androidRemovalRequester.waitForMarker(
    'E2E: ANDROID_TEST46_WEB_REMOVAL_REQUESTED',
    180_000,
  );
  await iosRemovalApproval.waitForMarker(
    'E2E: IOS_TEST46_WEB_REMOVAL_REQUEST_VISIBLE',
    180_000,
  );
  approvalCoordinator.allow('ios-remove-web-approve-test46', 1);
  await Promise.all([
    iosRemovalApproval.waitForMarker('E2E: IOS_TEST46_WEB_REMOVAL_APPROVED', 180_000),
    androidRemovalRequester.waitForMarker('E2E: ANDROID_TEST46_WEB_REMOVAL_COMPLETED', 180_000),
    webReset,
  ]);
  await Promise.all([iosRemovalApproval.result, androidRemovalRequester.result]);

  const iosRemovalTarget = startIosAction({
    testMethod: scenario.ios.removalObserverTestMethod,
    role: 'removal-target',
    cycle: 2,
    secretName: thirdName,
    sender: 'android',
    expectedDeviceNames: initialDeviceNames,
    label: 'iOS Test #46 observes its local removal',
  });
  void iosRemovalTarget.result.catch(() => {});
  const androidLocalRemoval = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.lifecycleTestClass,
    testMethod: scenario.android.localRemovalTestMethod,
    role: 'removal-requester',
    cycle: 2,
    step: 1,
    joinTarget: initialDeviceNames.ios,
    expectedSecretNames: allSecretNames,
    expectedDeviceNames: initialDeviceNames,
    label: 'Android Test #46 removes iOS locally',
  });
  await Promise.all([
    iosRemovalTarget.waitForMarker('E2E: IOS_TEST46_REMOVAL_TARGET_READY', 240_000),
    androidLocalRemoval.waitForMarker('E2E: ANDROID_TEST46_IOS_LOCAL_REMOVAL_READY', 180_000),
  ]);
  approvalCoordinator.allow('android-remove-ios-local-test46', 2);
  await androidLocalRemoval.waitForMarker(
    'E2E: ANDROID_TEST46_IOS_LOCAL_REMOVAL_REQUESTED',
    180_000,
  );
  await Promise.all([
    iosRemovalTarget.waitForMarker('E2E: IOS_TEST46_REMOVED_RESET', 180_000),
    androidLocalRemoval.waitForMarker(
      'E2E: ANDROID_TEST46_IOS_LOCAL_REMOVAL_COMPLETED',
      180_000,
    ),
  ]);
  await Promise.all([iosRemovalTarget.result, androidLocalRemoval.result]);

  await submitWebJoinRequest(page);
  console.log('E2E: WEB_TEST46_REJOIN_REQUEST_SENT');
  const approveWebRejoin = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.joinApproverTestClass,
    testMethod: scenario.android.approveJoinTestMethod,
    role: 'join-approver',
    cycle: 2,
    step: 1,
    joinTarget: 'web',
    label: 'Android Test #46 approves Web re-Join',
  });
  await approveWebRejoin.waitForMarker('E2E: ANDROID_TEST46_JOIN_APPROVED_WEB_2', 180_000);
  await approveWebRejoin.result;
  await page.reload({ waitUntil: 'domcontentloaded' });
  await unlockWithPasskeyIfNeeded(page);
  await waitForWebSecretNames(page, allSecretNames);
  const webRejoinDeviceNames = await assertWebMembership(
    page,
    ['Web', 'Android'],
    allSecretNames,
    'Test #46 Web re-Join two-device boundary',
    { android: initialDeviceNames.android },
  );
  console.log('E2E: WEB_TEST46_REJOIN_READY');
  writeDiagnostic(
    '[TEST46] Web re-Join canonical identities '
      + `web=${JSON.stringify(webRejoinDeviceNames.web)} `
      + `android=${JSON.stringify(webRejoinDeviceNames.android)}`,
  );

  for (const cycle of directShowCycles) {
    setCurrentCycleForDiagnostics(cycle);
    const sender = cycle.senders[0];
    const secretName = cycle.senderSecrets[sender];
    if (sender === 'web') {
      await showAndCloseWebSecret(page, secretName);
      console.log(`E2E: WEB_TEST46_LOCAL_SECRET_VALUE_VERIFIED_${cycle.number}`);
      console.log(`E2E: WEB_TEST46_LOCAL_SECRET_CLOSED_${cycle.number}`);
    } else if (sender === 'android') {
      const show = await startAndroidStepTest(androidSerial, {
        testClass: scenario.android.lifecycleTestClass,
        testMethod: scenario.android.showSecretTestMethod,
        role: 'direct-show',
        cycle: cycle.number,
        step: 1,
        secretName,
        secretValue: secretValueForName(secretName),
        expectedSecretNames: allSecretNames,
        label: `Android Test #46 direct Show cycle ${cycle.number}`,
      });
      await show.waitForMarker(
        `E2E: ANDROID_TEST46_LOCAL_SECRET_VALUE_VERIFIED_${cycle.number}`,
        180_000,
      );
      await show.waitForMarker(
        `E2E: ANDROID_TEST46_LOCAL_SECRET_CLOSED_${cycle.number}`,
        60_000,
      );
      await show.result;
    } else {
      throw new Error(`Test #46 direct Show does not support sender=${sender}`);
    }
    await assertNoStaleWebMembershipUi(page, allSecretNames);
    writeDiagnostic(
      `[TEST46][after-web-rejoin][cycle=${cycle.number}] sender=${sender} `
        + `secret=${secretName} exactValue=verified closed=true`,
    );
    page = await refreshWebForKnownNames(page, cycle.number, allSecretNames);
  }

  setJoinExpectedSecretNames(allSecretNames);
  const iosRejoin = startIosJoinTest(simulatorUdid, {
    testClass: scenario.ios.testClass,
    testMethod: scenario.ios.rejoinTestMethod,
    label: 'iOS Test #46 re-Join approved by Web',
  });
  void iosRejoin.result.catch(() => {});
  await iosRejoin.waitForMarker('E2E: IOS_TEST46_REJOIN_JOIN_REQUEST_SENT', 300_000);
  await approveJoinRequestOnWeb(page, 'iOS', { strictCompletion: true });
  console.log('E2E: WEB_TEST46_IOS_REJOIN_APPROVED');
  await iosRejoin.waitForMarker('E2E: IOS_TEST46_REJOIN_JOIN_READY', 180_000);
  await iosRejoin.waitForMarker('E2E: IOS_TEST46_REJOIN_SECRETS_READY', 60_000);
  await iosRejoin.result;
  await page.reload({ waitUntil: 'domcontentloaded' });
  await unlockWithPasskeyIfNeeded(page);
  await waitForWebSecretNames(page, allSecretNames);
  const finalDeviceNames = await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #46 final three-device recovery boundary',
    webRejoinDeviceNames,
  );
  console.log('E2E: WEB_TEST46_FINAL_THREE_DEVICE_READY');
  writeDiagnostic(
    '[TEST46] final canonical identities '
      + `web=${JSON.stringify(finalDeviceNames.web)} `
      + `android=${JSON.stringify(finalDeviceNames.android)} `
      + `ios=${JSON.stringify(finalDeviceNames.ios)}`,
  );

  const startNativeRecoveryStep = async (
    platform,
    role,
    cycle,
    secretName,
    approvalPlatform,
    sender,
  ) => {
    if (platform === 'ios') {
      const test = startIosAction({
        testMethod: scenario.ios.stepTestMethod,
        role,
        cycle: cycle.number,
        secretName,
        approvalPlatform,
        sender,
        label: `iOS Test #46 ${role} cycle ${cycle.number}`,
      });
      void test.result.catch(() => {});
      return test;
    }
    return startAndroidStepTest(androidSerial, {
      testClass: scenario.android.stepTestClass,
      testMethod: scenario.android.stepTestMethod,
      role,
      cycle: cycle.number,
      step: 1,
      approvalPlatform,
      sender,
      verifyReceiverAlert: role === 'receiver' && verifyBothReceiverAlerts,
      secretName,
      secretValue: secretValueForName(secretName),
      label: `Android Test #46 ${role} cycle ${cycle.number}`,
    });
  };

  for (const cycle of recoveryOnlyCycles) {
    setCurrentCycleForDiagnostics(cycle);
    const sender = cycle.senders[0];
    const approver = cycle.approvals[0].platform;
    const observer = cycle.observer;
    const secretName = cycle.senderSecrets[sender];
    let senderTest;
    let approverTest;
    let observerTest;

    if (approver !== 'web') {
      approverTest = await startNativeRecoveryStep(
        approver,
        'receiver',
        cycle,
        secretName,
        approver,
        sender,
      );
    }
    if (observer !== 'web') {
      observerTest = await startNativeRecoveryStep(
        observer,
        'receiver',
        cycle,
        secretName,
        approver,
        sender,
      );
    }
    if (sender !== 'web') {
      senderTest = await startNativeRecoveryStep(
        sender,
        'sender',
        cycle,
        secretName,
        approver,
        sender,
      );
    }

    await waitForRecoveryParticipantsReady(page, {
      cycleNumber: cycle.number,
      secretName,
      participants: [
        { platform: approver, role: 'receiver', test: approverTest },
        { platform: observer, role: 'receiver', test: observerTest },
        { platform: sender, role: 'sender', test: senderTest },
      ],
      label: `TEST46:${cycle.block}`,
    });
    emitRecoveryMarker(cycle, 'PARTICIPANTS_READY');

    if (sender === 'web') {
      approvalCoordinator.allow('web-sender', cycle.number);
      await startWebRecovery(page, secretName);
    } else {
      approvalCoordinator.allow(`${sender}-sender-1`, cycle.number);
      await senderTest.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_REQUEST_SENT_${cycle.number}_1`,
        180_000,
      );
    }
    emitRecoveryMarker(cycle, `${sender.toUpperCase()}_REQUEST_PERSISTED`);

    const waitForReceiverVisible = async (platform, test) => {
      if (platform === 'web') {
        await openStrictWebIncomingAlert(page, 1, secretName, {
          dismissWithoutDecision: platform === observer,
        });
        return;
      }
      await test.waitForMarker(
        `E2E: ${platform.toUpperCase()}_INCOMING_ALERT_VISIBLE_${cycle.number}_1`,
        180_000,
      );
      if (platform === observer) {
        await test.waitForMarker(
          `E2E: ${platform.toUpperCase()}_INCOMING_ALERT_DISMISSED_WITHOUT_DECISION_${cycle.number}_1`,
          60_000,
        );
      }
    };
    await Promise.all([
      waitForReceiverVisible(approver, approverTest),
      waitForReceiverVisible(observer, observerTest),
    ]);
    emitRecoveryMarker(cycle, 'BOTH_RECEIVERS_VISIBLE');

    if (approver === 'web') {
      await approveOpenIncomingRecoveryOnWeb(page, secretName);
    } else {
      approvalCoordinator.allow(`${approver}-approve-1`, cycle.number);
      await approverTest.waitForMarker(
        `E2E: ${approver.toUpperCase()}_APPROVED_INCOMING_${cycle.number}_1`,
        180_000,
      );
    }
    emitRecoveryMarker(cycle, `${approver.toUpperCase()}_APPROVAL_COMPLETED`);

    if (observer === 'web') {
      await waitForWebIncomingRecoveryGone(page, secretName);
    } else {
      approvalCoordinator.allow(`${observer}-dismiss-1`, cycle.number);
      await observerTest.waitForMarker(
        `E2E: ${observer.toUpperCase()}_DISMISSED_INCOMING_${cycle.number}_1`,
        180_000,
      );
    }
    emitRecoveryMarker(cycle, `${observer.toUpperCase()}_OBSERVER_CLEARED`);

    if (sender === 'web') {
      await revealAndCloseWebSecret(page, secretName, false);
    } else {
      approvalCoordinator.allow(`${sender}-show-1`, cycle.number);
      await senderTest.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_SECRET_VISIBLE_${cycle.number}_1`,
        recoveryShowTimeoutMs,
      );
      await senderTest.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_CLOSED_${cycle.number}_1`,
        60_000,
      );
    }
    emitRecoveryMarker(cycle, `${sender.toUpperCase()}_VALUE_VERIFIED`);
    emitRecoveryMarker(cycle, `${sender.toUpperCase()}_DIALOG_CLOSED`);

    if (senderTest) await senderTest.result;
    if (approverTest) await approverTest.result;
    if (observerTest) await observerTest.result;
    emitRecoveryMarker(cycle, 'CYCLE_COMPLETED');
    writeDiagnostic(
      `[TEST46][${cycle.block}][cycle=${cycle.number}] sender=${sender} `
        + `approver=${approver} observer=${observer} secret=${secretName} `
        + 'bothReceiversVisible=true observerGone=true exactValue=verified closed=true',
    );
    page = await refreshWebForKnownNames(page, cycle.number, allSecretNames);
  }

  return { completedCycles: recoveryCycles.length };
}

async function runWebDistributedCreatorDeclinedRemovalRecoveryLifecycle(
  page,
  simulatorUdid,
  androidSerial,
) {
  const allSecretNames = ['secret1', 'secret2', 'secret3'].map(secretNameFor);
  const [initialName, secondName, thirdName] = allSecretNames;
  const declinedRemovals = scenario.membership?.declinedRemovals ?? [];
  const verifyBothReceiverAlerts = scenario.recovery.verifyBothReceiverAlerts === true;

  if (testNumber !== 47) {
    throw new Error(`Declined-removal/recovery lifecycle is not defined for Test #${testNumber}`);
  }
  if ((scenario.membership?.initialPlatforms ?? []).join(',') !== 'web,android,ios') {
    throw new Error('Test #47 requires the Web, Android, iOS three-member boundary');
  }
  if ((scenario.setup?.joinOrder ?? []).join(',') !== 'android,ios') {
    throw new Error('Test #47 requires Android then iOS Join order');
  }
  if (scenario.setup?.secondJoinApprover !== 'web') {
    throw new Error('Test #47 requires Web A to approve iOS C initial Join');
  }
  if (
    (secretCreationPlan?.initial?.web ?? []).join(',') !== initialName
    || (secretCreationPlan?.afterAndroidJoin?.android ?? []).join(',') !== secondName
    || (secretCreationPlan?.afterIosJoin?.ios ?? []).join(',') !== thirdName
  ) {
    throw new Error('Test #47 requires Web Secret1, Android Secret2, then iOS Secret3');
  }
  const expectedRemovalMatrix = [
    '1:ios:android:web:3',
    '2:android:web:ios:3',
    '3:web:ios:android:3',
  ];
  const actualRemovalMatrix = declinedRemovals.map((entry) => (
    `${entry.phase}:${entry.requester}:${entry.decliner}:${entry.target}:${entry.expectedMemberCount}`
  ));
  if (actualRemovalMatrix.join('|') !== expectedRemovalMatrix.join('|')) {
    throw new Error('Test #47 declined-removal role matrix is invalid');
  }
  if (
    recoveryCycles.length !== 27
    || recoveryCycles.some((cycle) => (
      cycle.kind !== 'recovery'
      || cycle.senders.length !== 1
      || cycle.approvals.length !== 1
    ))
  ) {
    throw new Error('Test #47 requires exactly 27 single-sender recovery cycles');
  }
  if (!verifyBothReceiverAlerts) {
    throw new Error('Test #47 must verify both receiver alerts before each approval');
  }

  const roleCounts = Object.fromEntries(
    ['web', 'android', 'ios'].map((platform) => [platform, { sender: 0, approver: 0, observer: 0 }]),
  );
  for (const phase of [1, 2, 3]) {
    const phaseCycles = recoveryCycles.filter((cycle) => cycle.block.startsWith(`phase${phase}-`));
    if (phaseCycles.length !== 9) {
      throw new Error(`Test #47 phase ${phase} must contain exactly nine recovery cycles`);
    }
    const phaseMatrix = new Set();
    for (const cycle of phaseCycles) {
      const sender = cycle.senders[0];
      const approver = cycle.approvals[0]?.platform;
      const observer = cycle.observer;
      const secretName = cycle.senderSecrets[sender];
      if (
        !sender
        || !approver
        || !observer
        || new Set([sender, approver, observer]).size !== 3
      ) {
        throw new Error(`Test #47 recovery cycle ${cycle.number} has an invalid role triple`);
      }
      phaseMatrix.add(`${sender}:${secretName}`);
      roleCounts[sender].sender += 1;
      roleCounts[approver].approver += 1;
      roleCounts[observer].observer += 1;
    }
    const expectedPhaseMatrix = new Set(
      ['web', 'android', 'ios'].flatMap((platform) => (
        allSecretNames.map((secretName) => `${platform}:${secretName}`)
      )),
    );
    if (
      phaseMatrix.size !== expectedPhaseMatrix.size
      || [...expectedPhaseMatrix].some((entry) => !phaseMatrix.has(entry))
    ) {
      throw new Error(`Test #47 phase ${phase} does not cover every sender/Secret pair exactly once`);
    }
  }
  for (const [platform, counts] of Object.entries(roleCounts)) {
    for (const [role, count] of Object.entries(counts)) {
      if (count !== 9) {
        throw new Error(`Test #47 expected ${platform} ${role}=9, got ${count}`);
      }
    }
  }

  const emitRecoveryMarker = (cycle, action) => {
    const block = cycle.block.toUpperCase().replace(/[^A-Z0-9]+/g, '_');
    console.log(`E2E: TEST47_${block}_CYCLE_${cycle.number}_${action}`);
  };
  const startIosAction = ({
    testMethod,
    role,
    cycle = 0,
    secretName,
    sender = '',
    approvalPlatform = '',
    expectedDeviceNames,
    label,
  }) => startIosJoinTest(simulatorUdid, {
    testClass: scenario.ios.testClass,
    testMethod,
    label,
    environment: stepEnvironment({
      role,
      cycle,
      step: 1,
      sender,
      approvalPlatform,
      verifyReceiverAlert: role === 'receiver' && verifyBothReceiverAlerts,
      secretName,
    }),
    secretName,
    secretValue: secretValueForName(secretName),
    expectedDeviceNames,
  });
  const startNativeRecoveryStep = async (
    platform,
    role,
    cycle,
    secretName,
    approvalPlatform,
    sender,
  ) => {
    if (platform === 'ios') {
      const test = startIosAction({
        testMethod: scenario.ios.stepTestMethod,
        role,
        cycle: cycle.number,
        secretName,
        approvalPlatform,
        sender,
        label: `iOS Test #47 ${role} cycle ${cycle.number}`,
      });
      void test.result.catch(() => {});
      return test;
    }
    return startAndroidStepTest(androidSerial, {
      testClass: scenario.android.stepTestClass,
      testMethod: scenario.android.stepTestMethod,
      role,
      cycle: cycle.number,
      step: 1,
      approvalPlatform,
      sender,
      verifyReceiverAlert: role === 'receiver' && verifyBothReceiverAlerts,
      secretName,
      secretValue: secretValueForName(secretName),
      label: `Android Test #47 ${role} cycle ${cycle.number}`,
    });
  };
  const runRecoveryPhase = async (phase) => {
    const phaseCycles = recoveryCycles.filter((cycle) => cycle.block.startsWith(`phase${phase}-`));
    for (const cycle of phaseCycles) {
      setCurrentCycleForDiagnostics(cycle);
      const sender = cycle.senders[0];
      const approver = cycle.approvals[0].platform;
      const observer = cycle.observer;
      const secretName = cycle.senderSecrets[sender];
      let senderTest;
      let approverTest;
      let observerTest;

      if (approver !== 'web') {
        approverTest = await startNativeRecoveryStep(
          approver,
          'receiver',
          cycle,
          secretName,
          approver,
          sender,
        );
      }
      if (observer !== 'web') {
        observerTest = await startNativeRecoveryStep(
          observer,
          'receiver',
          cycle,
          secretName,
          approver,
          sender,
        );
      }
      if (sender !== 'web') {
        senderTest = await startNativeRecoveryStep(
          sender,
          'sender',
          cycle,
          secretName,
          approver,
          sender,
        );
      }

      await waitForRecoveryParticipantsReady(page, {
        cycleNumber: cycle.number,
        secretName,
        participants: [
          { platform: approver, role: 'receiver', test: approverTest },
          { platform: observer, role: 'receiver', test: observerTest },
          { platform: sender, role: 'sender', test: senderTest },
        ],
        label: `TEST47:${cycle.block}`,
      });
      emitRecoveryMarker(cycle, 'PARTICIPANTS_READY');

      if (sender === 'web') {
        approvalCoordinator.allow('web-sender', cycle.number);
        await startWebRecovery(page, secretName);
      } else {
        approvalCoordinator.allow(`${sender}-sender-1`, cycle.number);
        await senderTest.waitForMarker(
          `E2E: ${sender.toUpperCase()}_RECOVERY_REQUEST_SENT_${cycle.number}_1`,
          180_000,
        );
      }
      emitRecoveryMarker(cycle, `${sender.toUpperCase()}_REQUEST_PERSISTED`);

      const waitForReceiverVisible = async (platform, test) => {
        if (platform === 'web') {
          await openStrictWebIncomingAlert(page, 1, secretName, {
            dismissWithoutDecision: platform === observer,
          });
          return;
        }
        await test.waitForMarker(
          `E2E: ${platform.toUpperCase()}_INCOMING_ALERT_VISIBLE_${cycle.number}_1`,
          180_000,
        );
        if (platform === observer) {
          await test.waitForMarker(
            `E2E: ${platform.toUpperCase()}_INCOMING_ALERT_DISMISSED_WITHOUT_DECISION_${cycle.number}_1`,
            60_000,
          );
        }
      };
      await Promise.all([
        waitForReceiverVisible(approver, approverTest),
        waitForReceiverVisible(observer, observerTest),
      ]);
      emitRecoveryMarker(cycle, 'BOTH_RECEIVERS_VISIBLE');

      if (approver === 'web') {
        await approveOpenIncomingRecoveryOnWeb(page, secretName);
      } else {
        approvalCoordinator.allow(`${approver}-approve-1`, cycle.number);
        await approverTest.waitForMarker(
          `E2E: ${approver.toUpperCase()}_APPROVED_INCOMING_${cycle.number}_1`,
          180_000,
        );
      }
      emitRecoveryMarker(cycle, `${approver.toUpperCase()}_APPROVAL_COMPLETED`);

      if (observer === 'web') {
        await waitForWebIncomingRecoveryGone(page, secretName);
      } else {
        approvalCoordinator.allow(`${observer}-dismiss-1`, cycle.number);
        await observerTest.waitForMarker(
          `E2E: ${observer.toUpperCase()}_DISMISSED_INCOMING_${cycle.number}_1`,
          180_000,
        );
      }
      emitRecoveryMarker(cycle, `${observer.toUpperCase()}_OBSERVER_CLEARED`);

      if (sender === 'web') {
        await revealAndCloseWebSecret(page, secretName, false);
      } else {
        approvalCoordinator.allow(`${sender}-show-1`, cycle.number);
        await senderTest.waitForMarker(
          `E2E: ${sender.toUpperCase()}_RECOVERY_SECRET_VISIBLE_${cycle.number}_1`,
          recoveryShowTimeoutMs,
        );
        await senderTest.waitForMarker(
          `E2E: ${sender.toUpperCase()}_RECOVERY_CLOSED_${cycle.number}_1`,
          60_000,
        );
      }
      emitRecoveryMarker(cycle, `${sender.toUpperCase()}_VALUE_VERIFIED`);
      emitRecoveryMarker(cycle, `${sender.toUpperCase()}_DIALOG_CLOSED`);

      if (senderTest) await senderTest.result;
      if (approverTest) await approverTest.result;
      if (observerTest) await observerTest.result;
      emitRecoveryMarker(cycle, 'CYCLE_COMPLETED');
      writeDiagnostic(
        `[TEST47][phase=${phase}][${cycle.block}][cycle=${cycle.number}] sender=${sender} `
          + `approver=${approver} observer=${observer} secret=${secretName} `
          + 'bothReceiversVisible=true observerGone=true exactValue=verified closed=true',
      );
      page = await refreshWebForKnownNames(page, cycle.number, allSecretNames);
    }
  };

  setJoinExpectedSecretNames([initialName]);
  await createWebInitiatedVault(page);
  console.log('E2E: WEB_TEST47_SECRET1_READY');
  console.log('E2E: WEB_TEST47_INITIATOR_SETUP_READY');

  const androidJoin = await startAndroidJoinTest(androidSerial, {
    testClass: scenario.android.testClass,
    testMethod: scenario.android.joinTestMethod,
    expectedSecretNames: [initialName],
  });
  await androidJoin.waitForMarker('E2E: ANDROID_TEST47_INITIAL_JOIN_REQUEST_SENT', 180_000);
  await approveJoinRequestOnWeb(page, 'Android', { strictCompletion: true });
  console.log('E2E: WEB_TEST47_ANDROID_JOIN_APPROVED');
  await androidJoin.waitForMarker('E2E: ANDROID_TEST47_INITIAL_JOIN_READY', 180_000);
  await androidJoin.waitForMarker('E2E: ANDROID_TEST47_INITIAL_SECRETS_READY', 60_000);
  await androidJoin.result;
  await waitForWebSecretNames(page, [initialName]);

  const createSecond = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.creatorTestClass,
    testMethod: scenario.android.createSecretTestMethod,
    role: 'creator',
    cycle: 2,
    step: 1,
    secretName: secondName,
    secretValue: secretValueForName(secondName),
    expectedSecretNames: [initialName, secondName],
    label: 'Android Test #47 creates Secret2',
  });
  await createSecond.waitForMarker('E2E: ANDROID_TEST47_SECRET_CREATED_2', 180_000);
  await createSecond.result;
  setJoinExpectedSecretNames([initialName, secondName]);
  await waitForWebSecretNames(page, [initialName, secondName]);

  const iosJoin = startIosJoinTest(simulatorUdid, {
    testClass: scenario.ios.testClass,
    testMethod: scenario.ios.joinTestMethod,
    label: 'iOS Test #47 initial Join approved by Web',
  });
  void iosJoin.result.catch(() => {});
  await iosJoin.waitForMarker('E2E: IOS_TEST47_JOIN_REQUEST_SENT', 300_000);
  await approveJoinRequestOnWeb(page, 'iOS', { strictCompletion: true });
  console.log('E2E: WEB_TEST47_IOS_JOIN_APPROVED');
  await iosJoin.waitForMarker('E2E: IOS_TEST47_JOIN_READY', 180_000);
  await iosJoin.waitForMarker('E2E: IOS_TEST47_SECRETS_READY', 60_000);
  await iosJoin.result;

  const createThird = startIosAction({
    testMethod: scenario.ios.createSecretTestMethod,
    role: 'creator',
    cycle: 3,
    secretName: thirdName,
    label: 'iOS Test #47 creates Secret3',
  });
  void createThird.result.catch(() => {});
  await createThird.waitForMarker('E2E: IOS_TEST47_SECRET_CREATED_3', 180_000);
  await createThird.result;
  setJoinExpectedSecretNames(allSecretNames);
  await waitForWebSecretNames(page, allSecretNames);

  const expectedDeviceNames = await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #47 initial three-device boundary',
  );
  console.log('E2E: WEB_TEST47_INITIAL_THREE_DEVICE_READY');
  writeDiagnostic(
    '[TEST47] canonical registered device identities '
      + `web=${JSON.stringify(expectedDeviceNames.web)} `
      + `android=${JSON.stringify(expectedDeviceNames.android)} `
      + `ios=${JSON.stringify(expectedDeviceNames.ios)}`,
  );

  // Phase 1: iOS requests Web removal; Android declines.
  const androidPhase1Decliner = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.lifecycleTestClass,
    testMethod: scenario.android.declineWebRemovalTestMethod,
    role: 'removal-decliner',
    cycle: 1,
    step: 1,
    joinTarget: expectedDeviceNames.web,
    expectedSecretNames: allSecretNames,
    expectedDeviceNames,
    label: 'Android Test #47 declines iOS request to remove Web',
  });
  const iosPhase1Requester = startIosAction({
    testMethod: scenario.ios.requestWebRemovalTestMethod,
    role: 'removal-requester',
    cycle: 1,
    secretName: thirdName,
    sender: 'ios',
    expectedDeviceNames,
    label: 'iOS Test #47 requests Web removal',
  });
  void iosPhase1Requester.result.catch(() => {});
  await Promise.all([
    androidPhase1Decliner.waitForMarker('E2E: ANDROID_TEST47_PHASE1_DECLINER_READY', 180_000),
    iosPhase1Requester.waitForMarker('E2E: IOS_TEST47_PHASE1_REQUESTER_READY', 240_000),
  ]);
  await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #47 phase-1 passive target boundary',
    expectedDeviceNames,
  );
  console.log('E2E: WEB_TEST47_PHASE1_TARGET_READY');
  approvalCoordinator.allow('ios-remove-web-start-test47', 1);
  await iosPhase1Requester.waitForMarker('E2E: IOS_TEST47_PHASE1_REMOVAL_REQUESTED', 180_000);
  await androidPhase1Decliner.waitForMarker(
    'E2E: ANDROID_TEST47_PHASE1_REMOVAL_REQUEST_VISIBLE',
    180_000,
  );
  approvalCoordinator.allow('android-decline-web-test47', 1);
  await androidPhase1Decliner.waitForMarker(
    'E2E: ANDROID_TEST47_PHASE1_REMOVAL_DECLINED',
    180_000,
  );
  approvalCoordinator.allow('ios-phase1-decline-observed-test47', 1);
  await iosPhase1Requester.waitForMarker('E2E: IOS_TEST47_PHASE1_DECLINE_OBSERVED', 180_000);
  await Promise.all([androidPhase1Decliner.result, iosPhase1Requester.result]);
  await page.reload({ waitUntil: 'domcontentloaded' });
  await unlockWithPasskeyIfNeeded(page);
  await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #47 phase-1 terminal Decline boundary',
    expectedDeviceNames,
  );
  console.log('E2E: WEB_TEST47_PHASE1_DECLINE_OBSERVED');
  await runRecoveryPhase(1);

  // Phase 2: Android requests iOS removal; Web declines.
  const iosPhase2Target = startIosAction({
    testMethod: scenario.ios.observeIosDeclineTestMethod,
    role: 'removal-target',
    cycle: 2,
    secretName: thirdName,
    sender: 'android',
    expectedDeviceNames,
    label: 'iOS Test #47 remains a member after Web Decline',
  });
  void iosPhase2Target.result.catch(() => {});
  const androidPhase2Requester = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.lifecycleTestClass,
    testMethod: scenario.android.requestIosRemovalTestMethod,
    role: 'removal-requester',
    cycle: 2,
    step: 1,
    joinTarget: expectedDeviceNames.ios,
    expectedSecretNames: allSecretNames,
    expectedDeviceNames,
    label: 'Android Test #47 requests iOS removal',
  });
  await Promise.all([
    iosPhase2Target.waitForMarker('E2E: IOS_TEST47_PHASE2_TARGET_READY', 240_000),
    androidPhase2Requester.waitForMarker('E2E: ANDROID_TEST47_PHASE2_REQUESTER_READY', 180_000),
  ]);
  await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #47 phase-2 Web decliner boundary',
    expectedDeviceNames,
  );
  console.log('E2E: WEB_TEST47_PHASE2_DECLINER_READY');
  approvalCoordinator.allow('android-remove-ios-start-test47', 2);
  await androidPhase2Requester.waitForMarker(
    'E2E: ANDROID_TEST47_PHASE2_REMOVAL_REQUESTED',
    180_000,
  );
  await declinePendingWebDeviceRemoval(page, {
    targetCategory: 'iPhone',
    expectedTargetDeviceName: expectedDeviceNames.ios,
    label: 'Test #47 phase-2 Web Decline',
    requestVisibleMarker: 'WEB_TEST47_PHASE2_REMOVAL_REQUEST_VISIBLE',
    completionMarker: 'WEB_TEST47_PHASE2_REMOVAL_DECLINED',
  });
  approvalCoordinator.allow('android-phase2-decline-observed-test47', 2);
  approvalCoordinator.allow('ios-phase2-decline-observed-test47', 2);
  await Promise.all([
    androidPhase2Requester.waitForMarker('E2E: ANDROID_TEST47_PHASE2_DECLINE_OBSERVED', 180_000),
    iosPhase2Target.waitForMarker('E2E: IOS_TEST47_PHASE2_DECLINE_OBSERVED', 180_000),
  ]);
  await Promise.all([androidPhase2Requester.result, iosPhase2Target.result]);
  await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #47 phase-2 terminal Decline boundary',
    expectedDeviceNames,
  );
  await runRecoveryPhase(2);

  // Phase 3: Web requests Android removal; iOS declines.
  const androidPhase3Target = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.lifecycleTestClass,
    testMethod: scenario.android.observeAndroidDeclineTestMethod,
    role: 'removal-target',
    cycle: 3,
    step: 1,
    expectedSecretNames: allSecretNames,
    expectedDeviceNames,
    label: 'Android Test #47 remains a member after iOS Decline',
  });
  const iosPhase3Decliner = startIosAction({
    testMethod: scenario.ios.declineAndroidRemovalTestMethod,
    role: 'removal-decliner',
    cycle: 3,
    secretName: thirdName,
    sender: 'web',
    expectedDeviceNames,
    label: 'iOS Test #47 declines Web request to remove Android',
  });
  void iosPhase3Decliner.result.catch(() => {});
  await Promise.all([
    androidPhase3Target.waitForMarker('E2E: ANDROID_TEST47_PHASE3_TARGET_READY', 180_000),
    iosPhase3Decliner.waitForMarker('E2E: IOS_TEST47_PHASE3_DECLINER_READY', 240_000),
  ]);
  await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #47 phase-3 Web requester boundary',
    expectedDeviceNames,
  );
  console.log('E2E: WEB_TEST47_PHASE3_REQUESTER_READY');
  await initiateWebDeviceRemoval(page, {
    targetCategory: 'Android',
    expectedTargetDeviceName: expectedDeviceNames.android,
    waitForTargetGone: false,
    label: 'Test #47 phase-3 Web removal request',
    requestVisibleMarker: 'WEB_TEST47_PHASE3_REMOVAL_DIALOG_VISIBLE',
    completionMarker: 'WEB_TEST47_PHASE3_REMOVAL_REQUESTED',
  });
  await iosPhase3Decliner.waitForMarker(
    'E2E: IOS_TEST47_PHASE3_REMOVAL_REQUEST_VISIBLE',
    180_000,
  );
  approvalCoordinator.allow('ios-decline-android-test47', 3);
  await iosPhase3Decliner.waitForMarker(
    'E2E: IOS_TEST47_PHASE3_REMOVAL_DECLINED',
    180_000,
  );
  approvalCoordinator.allow('android-phase3-decline-observed-test47', 3);
  await androidPhase3Target.waitForMarker(
    'E2E: ANDROID_TEST47_PHASE3_DECLINE_OBSERVED',
    180_000,
  );
  await Promise.all([androidPhase3Target.result, iosPhase3Decliner.result]);
  await page.reload({ waitUntil: 'domcontentloaded' });
  await unlockWithPasskeyIfNeeded(page);
  await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #47 phase-3 terminal Decline boundary',
    expectedDeviceNames,
  );
  console.log('E2E: WEB_TEST47_PHASE3_DECLINE_OBSERVED');
  await runRecoveryPhase(3);

  return { completedCycles: recoveryCycles.length };
}

async function runAndroidDistributedCreatorDeclinedRemovalRecoveryLifecycle(
  page,
  simulatorUdid,
  androidSerial,
) {
  const allSecretNames = ['secret1', 'secret2', 'secret3'].map(secretNameFor);
  const [initialName, secondName, thirdName] = allSecretNames;
  const declinedRemovals = scenario.membership?.declinedRemovals ?? [];
  const verifyBothReceiverAlerts = scenario.recovery.verifyBothReceiverAlerts === true;

  if (testNumber !== 48) {
    throw new Error(`Android declined-removal/recovery lifecycle is not defined for Test #${testNumber}`);
  }
  if ((scenario.membership?.initialPlatforms ?? []).join(',') !== 'android,ios,web') {
    throw new Error('Test #48 requires the Android, iOS, Web three-member boundary');
  }
  if ((scenario.setup?.joinOrder ?? []).join(',') !== 'ios,web') {
    throw new Error('Test #48 requires iOS then Web Join order');
  }
  if (scenario.setup?.secondJoinApprover !== 'android') {
    throw new Error('Test #48 requires Android A to approve both initial Joins');
  }
  if (
    (secretCreationPlan?.initial?.android ?? []).join(',') !== initialName
    || (secretCreationPlan?.afterIosJoin?.ios ?? []).join(',') !== secondName
    || (secretCreationPlan?.afterWebJoin?.web ?? []).join(',') !== thirdName
  ) {
    throw new Error('Test #48 requires Android Secret1, iOS Secret2, then Web Secret3');
  }
  const expectedRemovalMatrix = [
    '1:web:ios:android:3',
    '2:ios:android:web:3',
    '3:android:web:ios:3',
  ];
  const actualRemovalMatrix = declinedRemovals.map((entry) => (
    `${entry.phase}:${entry.requester}:${entry.decliner}:${entry.target}:${entry.expectedMemberCount}`
  ));
  if (actualRemovalMatrix.join('|') !== expectedRemovalMatrix.join('|')) {
    throw new Error('Test #48 declined-removal role matrix is invalid');
  }
  if (
    recoveryCycles.length !== 27
    || recoveryCycles.some((cycle) => (
      cycle.kind !== 'recovery'
      || cycle.senders.length !== 1
      || cycle.approvals.length !== 1
    ))
  ) {
    throw new Error('Test #48 requires exactly 27 single-sender recovery cycles');
  }
  if (!verifyBothReceiverAlerts) {
    throw new Error('Test #48 must verify both receiver alerts before each approval');
  }

  const roleCounts = Object.fromEntries(
    ['web', 'android', 'ios'].map((platform) => [platform, { sender: 0, approver: 0, observer: 0 }]),
  );
  for (const phase of [1, 2, 3]) {
    const phaseCycles = recoveryCycles.filter((cycle) => cycle.block.startsWith(`phase${phase}-`));
    if (phaseCycles.length !== 9) {
      throw new Error(`Test #48 phase ${phase} must contain exactly nine recovery cycles`);
    }
    const phaseMatrix = new Set();
    for (const cycle of phaseCycles) {
      const sender = cycle.senders[0];
      const approver = cycle.approvals[0]?.platform;
      const observer = cycle.observer;
      const secretName = cycle.senderSecrets[sender];
      if (!sender || !approver || !observer || new Set([sender, approver, observer]).size !== 3) {
        throw new Error(`Test #48 recovery cycle ${cycle.number} has an invalid role triple`);
      }
      phaseMatrix.add(`${sender}:${secretName}`);
      roleCounts[sender].sender += 1;
      roleCounts[approver].approver += 1;
      roleCounts[observer].observer += 1;
    }
    const expectedPhaseMatrix = new Set(
      ['web', 'android', 'ios'].flatMap((platform) => (
        allSecretNames.map((secretName) => `${platform}:${secretName}`)
      )),
    );
    if (
      phaseMatrix.size !== expectedPhaseMatrix.size
      || [...expectedPhaseMatrix].some((entry) => !phaseMatrix.has(entry))
    ) {
      throw new Error(`Test #48 phase ${phase} does not cover every sender/Secret pair exactly once`);
    }
  }
  for (const [platform, counts] of Object.entries(roleCounts)) {
    for (const [role, count] of Object.entries(counts)) {
      if (count !== 9) {
        throw new Error(`Test #48 expected ${platform} ${role}=9, got ${count}`);
      }
    }
  }

  const emitRecoveryMarker = (cycle, action) => {
    const block = cycle.block.toUpperCase().replace(/[^A-Z0-9]+/g, '_');
    console.log(`E2E: TEST48_${block}_CYCLE_${cycle.number}_${action}`);
  };
  const startIosAction = ({
    testMethod,
    role,
    cycle = 0,
    secretName,
    sender = '',
    approvalPlatform = '',
    expectedDeviceNames,
    label,
  }) => startIosJoinTest(simulatorUdid, {
    testClass: scenario.ios.testClass,
    testMethod,
    label,
    environment: stepEnvironment({
      role,
      cycle,
      step: 1,
      sender,
      approvalPlatform,
      verifyReceiverAlert: role === 'receiver' && verifyBothReceiverAlerts,
      secretName,
    }),
    secretName,
    secretValue: secretValueForName(secretName),
    expectedDeviceNames,
  });
  const startNativeRecoveryStep = async (
    platform,
    role,
    cycle,
    secretName,
    approvalPlatform,
    sender,
  ) => {
    if (platform === 'ios') {
      const test = startIosAction({
        testMethod: scenario.ios.stepTestMethod,
        role,
        cycle: cycle.number,
        secretName,
        approvalPlatform,
        sender,
        label: `iOS Test #48 ${role} cycle ${cycle.number}`,
      });
      void test.result.catch(() => {});
      return test;
    }
    return startAndroidStepTest(androidSerial, {
      testClass: scenario.android.stepTestClass,
      testMethod: scenario.android.stepTestMethod,
      role,
      cycle: cycle.number,
      step: 1,
      approvalPlatform,
      sender,
      verifyReceiverAlert: role === 'receiver' && verifyBothReceiverAlerts,
      secretName,
      secretValue: secretValueForName(secretName),
      label: `Android Test #48 ${role} cycle ${cycle.number}`,
    });
  };
  const runRecoveryPhase = async (phase) => {
    const phaseCycles = recoveryCycles.filter((cycle) => cycle.block.startsWith(`phase${phase}-`));
    for (const cycle of phaseCycles) {
      setCurrentCycleForDiagnostics(cycle);
      const sender = cycle.senders[0];
      const approver = cycle.approvals[0].platform;
      const observer = cycle.observer;
      const secretName = cycle.senderSecrets[sender];
      let senderTest;
      let approverTest;
      let observerTest;

      if (approver !== 'web') {
        approverTest = await startNativeRecoveryStep(
          approver,
          'receiver',
          cycle,
          secretName,
          approver,
          sender,
        );
      }
      if (observer !== 'web') {
        observerTest = await startNativeRecoveryStep(
          observer,
          'receiver',
          cycle,
          secretName,
          approver,
          sender,
        );
      }
      if (sender !== 'web') {
        senderTest = await startNativeRecoveryStep(
          sender,
          'sender',
          cycle,
          secretName,
          approver,
          sender,
        );
      }

      await waitForRecoveryParticipantsReady(page, {
        cycleNumber: cycle.number,
        secretName,
        participants: [
          { platform: approver, role: 'receiver', test: approverTest },
          { platform: observer, role: 'receiver', test: observerTest },
          { platform: sender, role: 'sender', test: senderTest },
        ],
        label: `TEST48:${cycle.block}`,
      });
      emitRecoveryMarker(cycle, 'PARTICIPANTS_READY');

      if (sender === 'web') {
        approvalCoordinator.allow('web-sender', cycle.number);
        await startWebRecovery(page, secretName);
      } else {
        approvalCoordinator.allow(`${sender}-sender-1`, cycle.number);
        await senderTest.waitForMarker(
          `E2E: ${sender.toUpperCase()}_RECOVERY_REQUEST_SENT_${cycle.number}_1`,
          180_000,
        );
      }
      emitRecoveryMarker(cycle, `${sender.toUpperCase()}_REQUEST_PERSISTED`);

      const waitForReceiverVisible = async (platform, test) => {
        if (platform === 'web') {
          await openStrictWebIncomingAlert(page, 1, secretName, {
            dismissWithoutDecision: platform === observer,
          });
          return;
        }
        await test.waitForMarker(
          `E2E: ${platform.toUpperCase()}_INCOMING_ALERT_VISIBLE_${cycle.number}_1`,
          180_000,
        );
        if (platform === observer) {
          await test.waitForMarker(
            `E2E: ${platform.toUpperCase()}_INCOMING_ALERT_DISMISSED_WITHOUT_DECISION_${cycle.number}_1`,
            60_000,
          );
        }
      };
      await Promise.all([
        waitForReceiverVisible(approver, approverTest),
        waitForReceiverVisible(observer, observerTest),
      ]);
      emitRecoveryMarker(cycle, 'BOTH_RECEIVERS_VISIBLE');

      if (approver === 'web') {
        await approveOpenIncomingRecoveryOnWeb(page, secretName);
      } else {
        approvalCoordinator.allow(`${approver}-approve-1`, cycle.number);
        await approverTest.waitForMarker(
          `E2E: ${approver.toUpperCase()}_APPROVED_INCOMING_${cycle.number}_1`,
          180_000,
        );
      }
      emitRecoveryMarker(cycle, `${approver.toUpperCase()}_APPROVAL_COMPLETED`);

      if (observer === 'web') {
        await waitForWebIncomingRecoveryGone(page, secretName);
      } else {
        approvalCoordinator.allow(`${observer}-dismiss-1`, cycle.number);
        await observerTest.waitForMarker(
          `E2E: ${observer.toUpperCase()}_DISMISSED_INCOMING_${cycle.number}_1`,
          180_000,
        );
      }
      emitRecoveryMarker(cycle, `${observer.toUpperCase()}_OBSERVER_CLEARED`);

      if (sender === 'web') {
        await revealAndCloseWebSecret(page, secretName, false);
      } else {
        approvalCoordinator.allow(`${sender}-show-1`, cycle.number);
        await senderTest.waitForMarker(
          `E2E: ${sender.toUpperCase()}_RECOVERY_SECRET_VISIBLE_${cycle.number}_1`,
          recoveryShowTimeoutMs,
        );
        await senderTest.waitForMarker(
          `E2E: ${sender.toUpperCase()}_RECOVERY_CLOSED_${cycle.number}_1`,
          60_000,
        );
      }
      emitRecoveryMarker(cycle, `${sender.toUpperCase()}_VALUE_VERIFIED`);
      emitRecoveryMarker(cycle, `${sender.toUpperCase()}_DIALOG_CLOSED`);

      if (senderTest) await senderTest.result;
      if (approverTest) await approverTest.result;
      if (observerTest) await observerTest.result;
      emitRecoveryMarker(cycle, 'CYCLE_COMPLETED');
      writeDiagnostic(
        `[TEST48][phase=${phase}][${cycle.block}][cycle=${cycle.number}] sender=${sender} `
          + `approver=${approver} observer=${observer} secret=${secretName} `
          + 'bothReceiversVisible=true observerGone=true exactValue=verified closed=true',
      );
      page = await refreshWebForKnownNames(page, cycle.number, allSecretNames);
    }
  };

  setJoinExpectedSecretNames([initialName]);
  const androidSetup = await startAndroidJoinTest(androidSerial, {
    testClass: scenario.android.initiatorTestClass,
    testMethod: scenario.android.initiatorSetupTestMethod,
    expectedSecretNames: [initialName],
  });
  await androidSetup.waitForMarker('E2E: ANDROID_TEST48_SECRET1_READY', 180_000);
  await androidSetup.waitForMarker('E2E: ANDROID_TEST48_INITIATOR_SETUP_READY', 60_000);
  await androidSetup.result;

  const iosJoin = startIosJoinTest(simulatorUdid, {
    testClass: scenario.ios.testClass,
    testMethod: scenario.ios.joinTestMethod,
    label: 'iOS Test #48 joins Android-owned Vault',
  });
  void iosJoin.result.catch(() => {});
  await iosJoin.waitForMarker('E2E: IOS_TEST48_JOIN_REQUEST_SENT', 300_000);
  const approveIosJoin = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.initiatorTestClass,
    testMethod: scenario.android.approveJoinTestMethod,
    role: 'join-approver',
    cycle: 0,
    step: 1,
    joinTarget: 'ios',
    label: 'Android Test #48 approves iOS Join',
  });
  await approveIosJoin.waitForMarker('E2E: ANDROID_TEST48_JOIN_APPROVED_IOS', 180_000);
  await iosJoin.waitForMarker('E2E: IOS_TEST48_JOIN_READY', 180_000);
  await iosJoin.waitForMarker('E2E: IOS_TEST48_SECRETS_READY', 60_000);
  await Promise.all([approveIosJoin.result, iosJoin.result]);

  const createSecond = startIosAction({
    testMethod: scenario.ios.createSecretTestMethod,
    role: 'creator',
    cycle: 2,
    secretName: secondName,
    label: 'iOS Test #48 creates Secret2',
  });
  void createSecond.result.catch(() => {});
  await createSecond.waitForMarker('E2E: IOS_TEST48_SECRET_CREATED_2', 180_000);
  await createSecond.result;
  setJoinExpectedSecretNames([initialName, secondName]);

  await submitWebJoinRequest(page);
  console.log('E2E: WEB_TEST48_JOIN_REQUEST_SENT');
  const approveWebJoin = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.initiatorTestClass,
    testMethod: scenario.android.approveJoinTestMethod,
    role: 'join-approver',
    cycle: 0,
    step: 1,
    joinTarget: 'web',
    label: 'Android Test #48 approves Web Join',
  });
  await approveWebJoin.waitForMarker('E2E: ANDROID_TEST48_JOIN_APPROVED_WEB', 180_000);
  await approveWebJoin.result;
  await page.reload({ waitUntil: 'domcontentloaded' });
  await unlockWithPasskeyIfNeeded(page);
  await waitForWebSecretNames(page, [initialName, secondName]);
  console.log('E2E: WEB_TEST48_JOIN_READY');
  console.log('E2E: WEB_TEST48_SECRETS_READY');

  await createWebSecret(page, thirdName);
  await waitForWebSecretNames(page, allSecretNames);
  console.log('E2E: WEB_TEST48_SECRET_CREATED_3');
  setJoinExpectedSecretNames(allSecretNames);

  const expectedDeviceNames = await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #48 initial three-device boundary',
  );
  console.log('E2E: WEB_TEST48_INITIAL_THREE_DEVICE_READY');
  writeDiagnostic(
    '[TEST48] canonical registered device identities '
      + `web=${JSON.stringify(expectedDeviceNames.web)} `
      + `android=${JSON.stringify(expectedDeviceNames.android)} `
      + `ios=${JSON.stringify(expectedDeviceNames.ios)}`,
  );

  // Phase 1: Web requests Android removal; iOS declines.
  const androidPhase1Target = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.lifecycleTestClass,
    testMethod: scenario.android.observeAndroidDeclineTestMethod,
    role: 'removal-target',
    cycle: 1,
    step: 1,
    expectedSecretNames: allSecretNames,
    expectedDeviceNames,
    label: 'Android Test #48 remains a member after iOS Decline',
  });
  const iosPhase1Decliner = startIosAction({
    testMethod: scenario.ios.declineAndroidRemovalTestMethod,
    role: 'removal-decliner',
    cycle: 1,
    secretName: secondName,
    sender: 'web',
    expectedDeviceNames,
    label: 'iOS Test #48 declines Web request to remove Android',
  });
  void iosPhase1Decliner.result.catch(() => {});
  await Promise.all([
    androidPhase1Target.waitForMarker('E2E: ANDROID_TEST48_PHASE1_TARGET_READY', 180_000),
    iosPhase1Decliner.waitForMarker('E2E: IOS_TEST48_PHASE1_DECLINER_READY', 240_000),
  ]);
  await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #48 phase-1 Web requester boundary',
    expectedDeviceNames,
  );
  console.log('E2E: WEB_TEST48_PHASE1_REQUESTER_READY');
  await initiateWebDeviceRemoval(page, {
    targetCategory: 'Android',
    expectedTargetDeviceName: expectedDeviceNames.android,
    waitForTargetGone: false,
    label: 'Test #48 phase-1 Web removal request',
    requestVisibleMarker: 'WEB_TEST48_PHASE1_REMOVAL_DIALOG_VISIBLE',
    completionMarker: 'WEB_TEST48_PHASE1_REMOVAL_REQUESTED',
  });
  await iosPhase1Decliner.waitForMarker(
    'E2E: IOS_TEST48_PHASE1_REMOVAL_REQUEST_VISIBLE',
    180_000,
  );
  approvalCoordinator.allow('ios-decline-android-test48', 1);
  await iosPhase1Decliner.waitForMarker(
    'E2E: IOS_TEST48_PHASE1_REMOVAL_DECLINED',
    180_000,
  );
  approvalCoordinator.allow('android-phase1-decline-observed-test48', 1);
  await androidPhase1Target.waitForMarker(
    'E2E: ANDROID_TEST48_PHASE1_DECLINE_OBSERVED',
    180_000,
  );
  await Promise.all([androidPhase1Target.result, iosPhase1Decliner.result]);
  await page.reload({ waitUntil: 'domcontentloaded' });
  await unlockWithPasskeyIfNeeded(page);
  await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #48 phase-1 terminal Decline boundary',
    expectedDeviceNames,
  );
  console.log('E2E: WEB_TEST48_PHASE1_DECLINE_OBSERVED');
  await runRecoveryPhase(1);

  // Phase 2: iOS requests Web removal; Android declines.
  const androidPhase2Decliner = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.lifecycleTestClass,
    testMethod: scenario.android.declineWebRemovalTestMethod,
    role: 'removal-decliner',
    cycle: 2,
    step: 1,
    joinTarget: expectedDeviceNames.web,
    expectedSecretNames: allSecretNames,
    expectedDeviceNames,
    label: 'Android Test #48 declines iOS request to remove Web',
  });
  const iosPhase2Requester = startIosAction({
    testMethod: scenario.ios.requestWebRemovalTestMethod,
    role: 'removal-requester',
    cycle: 2,
    secretName: secondName,
    sender: 'ios',
    expectedDeviceNames,
    label: 'iOS Test #48 requests Web removal',
  });
  void iosPhase2Requester.result.catch(() => {});
  await Promise.all([
    androidPhase2Decliner.waitForMarker('E2E: ANDROID_TEST48_PHASE2_DECLINER_READY', 180_000),
    iosPhase2Requester.waitForMarker('E2E: IOS_TEST48_PHASE2_REQUESTER_READY', 240_000),
  ]);
  await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #48 phase-2 passive target boundary',
    expectedDeviceNames,
  );
  console.log('E2E: WEB_TEST48_PHASE2_TARGET_READY');
  approvalCoordinator.allow('ios-remove-web-start-test48', 2);
  await iosPhase2Requester.waitForMarker('E2E: IOS_TEST48_PHASE2_REMOVAL_REQUESTED', 180_000);
  await androidPhase2Decliner.waitForMarker(
    'E2E: ANDROID_TEST48_PHASE2_REMOVAL_REQUEST_VISIBLE',
    180_000,
  );
  approvalCoordinator.allow('android-decline-web-test48', 2);
  await androidPhase2Decliner.waitForMarker(
    'E2E: ANDROID_TEST48_PHASE2_REMOVAL_DECLINED',
    180_000,
  );
  approvalCoordinator.allow('ios-phase2-decline-observed-test48', 2);
  await iosPhase2Requester.waitForMarker('E2E: IOS_TEST48_PHASE2_DECLINE_OBSERVED', 180_000);
  await Promise.all([androidPhase2Decliner.result, iosPhase2Requester.result]);
  await page.reload({ waitUntil: 'domcontentloaded' });
  await unlockWithPasskeyIfNeeded(page);
  await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #48 phase-2 terminal Decline boundary',
    expectedDeviceNames,
  );
  console.log('E2E: WEB_TEST48_PHASE2_DECLINE_OBSERVED');
  await runRecoveryPhase(2);

  // Phase 3: Android requests iOS removal; Web declines.
  const iosPhase3Target = startIosAction({
    testMethod: scenario.ios.observeIosDeclineTestMethod,
    role: 'removal-target',
    cycle: 3,
    secretName: secondName,
    sender: 'android',
    expectedDeviceNames,
    label: 'iOS Test #48 remains a member after Web Decline',
  });
  void iosPhase3Target.result.catch(() => {});
  const androidPhase3Requester = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.lifecycleTestClass,
    testMethod: scenario.android.requestIosRemovalTestMethod,
    role: 'removal-requester',
    cycle: 3,
    step: 1,
    joinTarget: expectedDeviceNames.ios,
    expectedSecretNames: allSecretNames,
    expectedDeviceNames,
    label: 'Android Test #48 requests iOS removal',
  });
  await Promise.all([
    iosPhase3Target.waitForMarker('E2E: IOS_TEST48_PHASE3_TARGET_READY', 240_000),
    androidPhase3Requester.waitForMarker('E2E: ANDROID_TEST48_PHASE3_REQUESTER_READY', 180_000),
  ]);
  await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #48 phase-3 Web decliner boundary',
    expectedDeviceNames,
  );
  console.log('E2E: WEB_TEST48_PHASE3_DECLINER_READY');
  approvalCoordinator.allow('android-remove-ios-start-test48', 3);
  await androidPhase3Requester.waitForMarker(
    'E2E: ANDROID_TEST48_PHASE3_REMOVAL_REQUESTED',
    180_000,
  );
  await declinePendingWebDeviceRemoval(page, {
    targetCategory: 'iPhone',
    expectedTargetDeviceName: expectedDeviceNames.ios,
    label: 'Test #48 phase-3 Web Decline',
    requestVisibleMarker: 'WEB_TEST48_PHASE3_REMOVAL_REQUEST_VISIBLE',
    completionMarker: 'WEB_TEST48_PHASE3_REMOVAL_DECLINED',
  });
  approvalCoordinator.allow('android-phase3-decline-observed-test48', 3);
  approvalCoordinator.allow('ios-phase3-decline-observed-test48', 3);
  await Promise.all([
    androidPhase3Requester.waitForMarker('E2E: ANDROID_TEST48_PHASE3_DECLINE_OBSERVED', 180_000),
    iosPhase3Target.waitForMarker('E2E: IOS_TEST48_PHASE3_DECLINE_OBSERVED', 180_000),
  ]);
  await Promise.all([androidPhase3Requester.result, iosPhase3Target.result]);
  await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #48 phase-3 terminal Decline boundary',
    expectedDeviceNames,
  );
  await runRecoveryPhase(3);

  return { completedCycles: recoveryCycles.length };
}

async function runIosDistributedCreatorDeclinedRemovalRecoveryLifecycle(
  page,
  simulatorUdid,
  androidSerial,
) {
  const allSecretNames = ['secret1', 'secret2', 'secret3'].map(secretNameFor);
  const [initialName, secondName, thirdName] = allSecretNames;
  const declinedRemovals = scenario.membership?.declinedRemovals ?? [];
  const verifyBothReceiverAlerts = scenario.recovery.verifyBothReceiverAlerts === true;

  if (testNumber !== 49) {
    throw new Error(`iOS declined-removal/recovery lifecycle is not defined for Test #${testNumber}`);
  }
  if ((scenario.membership?.initialPlatforms ?? []).join(',') !== 'ios,web,android') {
    throw new Error('Test #49 requires the iOS, Web, Android three-member boundary');
  }
  if ((scenario.setup?.joinOrder ?? []).join(',') !== 'web,android') {
    throw new Error('Test #49 requires Web then Android Join order');
  }
  if (scenario.setup?.secondJoinApprover !== 'ios') {
    throw new Error('Test #49 requires iOS A to approve both initial Joins');
  }
  if (
    (secretCreationPlan?.initial?.ios ?? []).join(',') !== initialName
    || (secretCreationPlan?.afterWebJoin?.web ?? []).join(',') !== secondName
    || (secretCreationPlan?.afterAndroidJoin?.android ?? []).join(',') !== thirdName
  ) {
    throw new Error('Test #49 requires iOS Secret1, Web Secret2, then Android Secret3');
  }
  const expectedRemovalMatrix = [
    '1:android:web:ios:3',
    '2:web:ios:android:3',
    '3:ios:android:web:3',
  ];
  const actualRemovalMatrix = declinedRemovals.map((entry) => (
    `${entry.phase}:${entry.requester}:${entry.decliner}:${entry.target}:${entry.expectedMemberCount}`
  ));
  if (actualRemovalMatrix.join('|') !== expectedRemovalMatrix.join('|')) {
    throw new Error('Test #49 declined-removal role matrix is invalid');
  }
  if (
    recoveryCycles.length !== 27
    || recoveryCycles.some((cycle) => (
      cycle.kind !== 'recovery'
      || cycle.senders.length !== 1
      || cycle.approvals.length !== 1
    ))
  ) {
    throw new Error('Test #49 requires exactly 27 single-sender recovery cycles');
  }
  if (!verifyBothReceiverAlerts) {
    throw new Error('Test #49 must verify both receiver alerts before each approval');
  }

  const roleCounts = Object.fromEntries(
    ['web', 'android', 'ios'].map((platform) => [platform, { sender: 0, approver: 0, observer: 0 }]),
  );
  for (const phase of [1, 2, 3]) {
    const phaseCycles = recoveryCycles.filter((cycle) => cycle.block.startsWith(`phase${phase}-`));
    if (phaseCycles.length !== 9) {
      throw new Error(`Test #49 phase ${phase} must contain exactly nine recovery cycles`);
    }
    const phaseMatrix = new Set();
    for (const cycle of phaseCycles) {
      const sender = cycle.senders[0];
      const approver = cycle.approvals[0]?.platform;
      const observer = cycle.observer;
      const secretName = cycle.senderSecrets[sender];
      if (!sender || !approver || !observer || new Set([sender, approver, observer]).size !== 3) {
        throw new Error(`Test #49 recovery cycle ${cycle.number} has an invalid role triple`);
      }
      phaseMatrix.add(`${sender}:${secretName}`);
      roleCounts[sender].sender += 1;
      roleCounts[approver].approver += 1;
      roleCounts[observer].observer += 1;
    }
    const expectedPhaseMatrix = new Set(
      ['web', 'android', 'ios'].flatMap((platform) => (
        allSecretNames.map((secretName) => `${platform}:${secretName}`)
      )),
    );
    if (
      phaseMatrix.size !== expectedPhaseMatrix.size
      || [...expectedPhaseMatrix].some((entry) => !phaseMatrix.has(entry))
    ) {
      throw new Error(`Test #49 phase ${phase} does not cover every sender/Secret pair exactly once`);
    }
  }
  for (const [platform, counts] of Object.entries(roleCounts)) {
    for (const [role, count] of Object.entries(counts)) {
      if (count !== 9) {
        throw new Error(`Test #49 expected ${platform} ${role}=9, got ${count}`);
      }
    }
  }

  const emitRecoveryMarker = (cycle, action) => {
    const block = cycle.block.toUpperCase().replace(/[^A-Z0-9]+/g, '_');
    console.log(`E2E: TEST49_${block}_CYCLE_${cycle.number}_${action}`);
  };
  const startIosAction = ({
    testMethod,
    role,
    cycle = 0,
    secretName,
    sender = '',
    approvalPlatform = '',
    expectedDeviceNames,
    label,
  }) => startIosJoinTest(simulatorUdid, {
    testClass: scenario.ios.testClass,
    testMethod,
    label,
    environment: stepEnvironment({
      role,
      cycle,
      step: 1,
      sender,
      approvalPlatform,
      verifyReceiverAlert: role === 'receiver' && verifyBothReceiverAlerts,
      secretName,
    }),
    secretName,
    secretValue: secretValueForName(secretName),
    expectedDeviceNames,
  });
  const startNativeRecoveryStep = async (
    platform,
    role,
    cycle,
    secretName,
    approvalPlatform,
    sender,
  ) => {
    if (platform === 'ios') {
      const test = startIosAction({
        testMethod: scenario.ios.stepTestMethod,
        role,
        cycle: cycle.number,
        secretName,
        approvalPlatform,
        sender,
        label: `iOS Test #49 ${role} cycle ${cycle.number}`,
      });
      void test.result.catch(() => {});
      return test;
    }
    return startAndroidStepTest(androidSerial, {
      testClass: scenario.android.stepTestClass,
      testMethod: scenario.android.stepTestMethod,
      role,
      cycle: cycle.number,
      step: 1,
      approvalPlatform,
      sender,
      verifyReceiverAlert: role === 'receiver' && verifyBothReceiverAlerts,
      secretName,
      secretValue: secretValueForName(secretName),
      label: `Android Test #49 ${role} cycle ${cycle.number}`,
    });
  };
  const runRecoveryPhase = async (phase) => {
    const phaseCycles = recoveryCycles.filter((cycle) => cycle.block.startsWith(`phase${phase}-`));
    for (const cycle of phaseCycles) {
      setCurrentCycleForDiagnostics(cycle);
      const sender = cycle.senders[0];
      const approver = cycle.approvals[0].platform;
      const observer = cycle.observer;
      const secretName = cycle.senderSecrets[sender];
      let senderTest;
      let approverTest;
      let observerTest;

      if (approver !== 'web') {
        approverTest = await startNativeRecoveryStep(
          approver,
          'receiver',
          cycle,
          secretName,
          approver,
          sender,
        );
      }
      if (observer !== 'web') {
        observerTest = await startNativeRecoveryStep(
          observer,
          'receiver',
          cycle,
          secretName,
          approver,
          sender,
        );
      }
      if (sender !== 'web') {
        senderTest = await startNativeRecoveryStep(
          sender,
          'sender',
          cycle,
          secretName,
          approver,
          sender,
        );
      }

      await waitForRecoveryParticipantsReady(page, {
        cycleNumber: cycle.number,
        secretName,
        participants: [
          { platform: approver, role: 'receiver', test: approverTest },
          { platform: observer, role: 'receiver', test: observerTest },
          { platform: sender, role: 'sender', test: senderTest },
        ],
        label: `TEST49:${cycle.block}`,
      });
      emitRecoveryMarker(cycle, 'PARTICIPANTS_READY');

      if (sender === 'web') {
        approvalCoordinator.allow('web-sender', cycle.number);
        await startWebRecovery(page, secretName);
      } else {
        approvalCoordinator.allow(`${sender}-sender-1`, cycle.number);
        await senderTest.waitForMarker(
          `E2E: ${sender.toUpperCase()}_RECOVERY_REQUEST_SENT_${cycle.number}_1`,
          180_000,
        );
      }
      emitRecoveryMarker(cycle, `${sender.toUpperCase()}_REQUEST_PERSISTED`);

      const waitForReceiverVisible = async (platform, test) => {
        if (platform === 'web') {
          await openStrictWebIncomingAlert(page, 1, secretName, {
            dismissWithoutDecision: platform === observer,
          });
          return;
        }
        await test.waitForMarker(
          `E2E: ${platform.toUpperCase()}_INCOMING_ALERT_VISIBLE_${cycle.number}_1`,
          180_000,
        );
        if (platform === observer) {
          await test.waitForMarker(
            `E2E: ${platform.toUpperCase()}_INCOMING_ALERT_DISMISSED_WITHOUT_DECISION_${cycle.number}_1`,
            60_000,
          );
        }
      };
      await Promise.all([
        waitForReceiverVisible(approver, approverTest),
        waitForReceiverVisible(observer, observerTest),
      ]);
      emitRecoveryMarker(cycle, 'BOTH_RECEIVERS_VISIBLE');

      if (approver === 'web') {
        await approveOpenIncomingRecoveryOnWeb(page, secretName);
      } else {
        approvalCoordinator.allow(`${approver}-approve-1`, cycle.number);
        await approverTest.waitForMarker(
          `E2E: ${approver.toUpperCase()}_APPROVED_INCOMING_${cycle.number}_1`,
          180_000,
        );
      }
      emitRecoveryMarker(cycle, `${approver.toUpperCase()}_APPROVAL_COMPLETED`);

      if (observer === 'web') {
        await waitForWebIncomingRecoveryGone(page, secretName);
      } else {
        approvalCoordinator.allow(`${observer}-dismiss-1`, cycle.number);
        await observerTest.waitForMarker(
          `E2E: ${observer.toUpperCase()}_DISMISSED_INCOMING_${cycle.number}_1`,
          180_000,
        );
      }
      emitRecoveryMarker(cycle, `${observer.toUpperCase()}_OBSERVER_CLEARED`);

      if (sender === 'web') {
        await revealAndCloseWebSecret(page, secretName, false);
      } else {
        approvalCoordinator.allow(`${sender}-show-1`, cycle.number);
        await senderTest.waitForMarker(
          `E2E: ${sender.toUpperCase()}_RECOVERY_SECRET_VISIBLE_${cycle.number}_1`,
          recoveryShowTimeoutMs,
        );
        await senderTest.waitForMarker(
          `E2E: ${sender.toUpperCase()}_RECOVERY_CLOSED_${cycle.number}_1`,
          60_000,
        );
      }
      emitRecoveryMarker(cycle, `${sender.toUpperCase()}_VALUE_VERIFIED`);
      emitRecoveryMarker(cycle, `${sender.toUpperCase()}_DIALOG_CLOSED`);

      if (senderTest) await senderTest.result;
      if (approverTest) await approverTest.result;
      if (observerTest) await observerTest.result;
      emitRecoveryMarker(cycle, 'CYCLE_COMPLETED');
      writeDiagnostic(
        `[TEST49][phase=${phase}][${cycle.block}][cycle=${cycle.number}] sender=${sender} `
          + `approver=${approver} observer=${observer} secret=${secretName} `
          + 'bothReceiversVisible=true observerGone=true exactValue=verified closed=true',
      );
      page = await refreshWebForKnownNames(page, cycle.number, allSecretNames);
    }
  };

  setJoinExpectedSecretNames([initialName]);
  const iosSetup = startIosAction({
    testMethod: scenario.ios.initiatorSetupTestMethod,
    role: 'creator',
    secretName: initialName,
    label: 'iOS Test #49 creates Vault and Secret1',
  });
  void iosSetup.result.catch(() => {});
  await iosSetup.waitForMarker('E2E: IOS_TEST49_SECRET1_READY', 300_000);
  await iosSetup.waitForMarker('E2E: IOS_TEST49_INITIATOR_SETUP_READY', 60_000);
  await iosSetup.result;

  await submitWebJoinRequest(page);
  console.log('E2E: WEB_TEST49_JOIN_REQUEST_SENT');
  const approveWebJoin = startIosAction({
    testMethod: scenario.ios.approveJoinTestMethod,
    role: 'join-approver',
    secretName: initialName,
    sender: 'web',
    label: 'iOS Test #49 approves Web Join',
  });
  void approveWebJoin.result.catch(() => {});
  await approveWebJoin.waitForMarker('E2E: IOS_TEST49_JOIN_APPROVED_WEB', 180_000);
  await approveWebJoin.result;
  await page.reload({ waitUntil: 'domcontentloaded' });
  await unlockWithPasskeyIfNeeded(page);
  await waitForWebSecretNames(page, [initialName]);
  console.log('E2E: WEB_TEST49_JOIN_READY');
  console.log('E2E: WEB_TEST49_SECRETS_READY');

  await createWebSecret(page, secondName);
  await waitForWebSecretNames(page, [initialName, secondName]);
  console.log('E2E: WEB_TEST49_SECRET_CREATED_2');
  setJoinExpectedSecretNames([initialName, secondName]);

  const androidJoin = await startAndroidJoinTest(androidSerial, {
    testClass: scenario.android.testClass,
    testMethod: scenario.android.joinTestMethod,
    expectedSecretNames: [initialName, secondName],
  });
  await androidJoin.waitForMarker('E2E: ANDROID_TEST49_JOIN_REQUEST_SENT', 180_000);
  const approveAndroidJoin = startIosAction({
    testMethod: scenario.ios.approveJoinTestMethod,
    role: 'join-approver',
    secretName: initialName,
    sender: 'android',
    label: 'iOS Test #49 approves Android Join',
  });
  void approveAndroidJoin.result.catch(() => {});
  await approveAndroidJoin.waitForMarker('E2E: IOS_TEST49_JOIN_APPROVED_ANDROID', 180_000);
  await androidJoin.waitForMarker('E2E: ANDROID_TEST49_JOIN_READY', 180_000);
  await androidJoin.waitForMarker('E2E: ANDROID_TEST49_SECRETS_READY', 60_000);
  await Promise.all([approveAndroidJoin.result, androidJoin.result]);

  const createThird = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.creatorTestClass,
    testMethod: scenario.android.createSecretTestMethod,
    role: 'creator',
    cycle: 3,
    step: 1,
    secretName: thirdName,
    secretValue: secretValueForName(thirdName),
    expectedSecretNames: allSecretNames,
    label: 'Android Test #49 creates Secret3',
  });
  await createThird.waitForMarker('E2E: ANDROID_TEST49_SECRET_CREATED_3', 180_000);
  await createThird.result;
  setJoinExpectedSecretNames(allSecretNames);
  await waitForWebSecretNames(page, allSecretNames);

  const expectedDeviceNames = await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #49 initial three-device boundary',
  );
  console.log('E2E: WEB_TEST49_INITIAL_THREE_DEVICE_READY');
  writeDiagnostic(
    '[TEST49] canonical registered device identities '
      + `web=${JSON.stringify(expectedDeviceNames.web)} `
      + `android=${JSON.stringify(expectedDeviceNames.android)} `
      + `ios=${JSON.stringify(expectedDeviceNames.ios)}`,
  );

  // Phase 1: Android requests iOS removal; Web declines.
  const iosPhase1Target = startIosAction({
    testMethod: scenario.ios.observeIosDeclineTestMethod,
    role: 'removal-target',
    cycle: 1,
    secretName: initialName,
    sender: 'android',
    expectedDeviceNames,
    label: 'iOS Test #49 remains a member after Web Decline',
  });
  void iosPhase1Target.result.catch(() => {});
  const androidPhase1Requester = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.lifecycleTestClass,
    testMethod: scenario.android.requestIosRemovalTestMethod,
    role: 'removal-requester',
    cycle: 1,
    step: 1,
    joinTarget: expectedDeviceNames.ios,
    expectedSecretNames: allSecretNames,
    expectedDeviceNames,
    label: 'Android Test #49 requests iOS removal',
  });
  await Promise.all([
    iosPhase1Target.waitForMarker('E2E: IOS_TEST49_PHASE1_TARGET_READY', 240_000),
    androidPhase1Requester.waitForMarker('E2E: ANDROID_TEST49_PHASE1_REQUESTER_READY', 180_000),
  ]);
  await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #49 phase-1 Web decliner boundary',
    expectedDeviceNames,
  );
  console.log('E2E: WEB_TEST49_PHASE1_DECLINER_READY');
  approvalCoordinator.allow('android-remove-ios-start-test49', 1);
  await androidPhase1Requester.waitForMarker(
    'E2E: ANDROID_TEST49_PHASE1_REMOVAL_REQUESTED',
    180_000,
  );
  await declinePendingWebDeviceRemoval(page, {
    targetCategory: 'iPhone',
    expectedTargetDeviceName: expectedDeviceNames.ios,
    label: 'Test #49 phase-1 Web Decline',
    requestVisibleMarker: 'WEB_TEST49_PHASE1_REMOVAL_REQUEST_VISIBLE',
    completionMarker: 'WEB_TEST49_PHASE1_REMOVAL_DECLINED',
  });
  approvalCoordinator.allow('android-phase1-decline-observed-test49', 1);
  approvalCoordinator.allow('ios-phase1-decline-observed-test49', 1);
  await Promise.all([
    androidPhase1Requester.waitForMarker('E2E: ANDROID_TEST49_PHASE1_DECLINE_OBSERVED', 180_000),
    iosPhase1Target.waitForMarker('E2E: IOS_TEST49_PHASE1_DECLINE_OBSERVED', 180_000),
  ]);
  await Promise.all([androidPhase1Requester.result, iosPhase1Target.result]);
  await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #49 phase-1 terminal Decline boundary',
    expectedDeviceNames,
  );
  await runRecoveryPhase(1);

  // Phase 2: Web requests Android removal; iOS declines.
  const androidPhase2Target = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.lifecycleTestClass,
    testMethod: scenario.android.observeAndroidDeclineTestMethod,
    role: 'removal-target',
    cycle: 2,
    step: 1,
    expectedSecretNames: allSecretNames,
    expectedDeviceNames,
    label: 'Android Test #49 remains a member after iOS Decline',
  });
  const iosPhase2Decliner = startIosAction({
    testMethod: scenario.ios.declineAndroidRemovalTestMethod,
    role: 'removal-decliner',
    cycle: 2,
    secretName: initialName,
    sender: 'web',
    expectedDeviceNames,
    label: 'iOS Test #49 declines Web request to remove Android',
  });
  void iosPhase2Decliner.result.catch(() => {});
  await Promise.all([
    androidPhase2Target.waitForMarker('E2E: ANDROID_TEST49_PHASE2_TARGET_READY', 180_000),
    iosPhase2Decliner.waitForMarker('E2E: IOS_TEST49_PHASE2_DECLINER_READY', 240_000),
  ]);
  await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #49 phase-2 Web requester boundary',
    expectedDeviceNames,
  );
  console.log('E2E: WEB_TEST49_PHASE2_REQUESTER_READY');
  await initiateWebDeviceRemoval(page, {
    targetCategory: 'Android',
    expectedTargetDeviceName: expectedDeviceNames.android,
    waitForTargetGone: false,
    label: 'Test #49 phase-2 Web removal request',
    requestVisibleMarker: 'WEB_TEST49_PHASE2_REMOVAL_DIALOG_VISIBLE',
    completionMarker: 'WEB_TEST49_PHASE2_REMOVAL_REQUESTED',
  });
  await iosPhase2Decliner.waitForMarker(
    'E2E: IOS_TEST49_PHASE2_REMOVAL_REQUEST_VISIBLE',
    180_000,
  );
  approvalCoordinator.allow('ios-decline-android-test49', 2);
  await iosPhase2Decliner.waitForMarker(
    'E2E: IOS_TEST49_PHASE2_REMOVAL_DECLINED',
    180_000,
  );
  approvalCoordinator.allow('android-phase2-decline-observed-test49', 2);
  await androidPhase2Target.waitForMarker(
    'E2E: ANDROID_TEST49_PHASE2_DECLINE_OBSERVED',
    180_000,
  );
  await Promise.all([androidPhase2Target.result, iosPhase2Decliner.result]);
  await page.reload({ waitUntil: 'domcontentloaded' });
  await unlockWithPasskeyIfNeeded(page);
  await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #49 phase-2 terminal Decline boundary',
    expectedDeviceNames,
  );
  console.log('E2E: WEB_TEST49_PHASE2_DECLINE_OBSERVED');
  await runRecoveryPhase(2);

  // Phase 3: iOS requests Web removal; Android declines.
  const androidPhase3Decliner = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.lifecycleTestClass,
    testMethod: scenario.android.declineWebRemovalTestMethod,
    role: 'removal-decliner',
    cycle: 3,
    step: 1,
    joinTarget: expectedDeviceNames.web,
    expectedSecretNames: allSecretNames,
    expectedDeviceNames,
    label: 'Android Test #49 declines iOS request to remove Web',
  });
  const iosPhase3Requester = startIosAction({
    testMethod: scenario.ios.requestWebRemovalTestMethod,
    role: 'removal-requester',
    cycle: 3,
    secretName: initialName,
    sender: 'ios',
    expectedDeviceNames,
    label: 'iOS Test #49 requests Web removal',
  });
  void iosPhase3Requester.result.catch(() => {});
  await Promise.all([
    androidPhase3Decliner.waitForMarker('E2E: ANDROID_TEST49_PHASE3_DECLINER_READY', 180_000),
    iosPhase3Requester.waitForMarker('E2E: IOS_TEST49_PHASE3_REQUESTER_READY', 240_000),
  ]);
  await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #49 phase-3 passive target boundary',
    expectedDeviceNames,
  );
  console.log('E2E: WEB_TEST49_PHASE3_TARGET_READY');
  approvalCoordinator.allow('ios-remove-web-start-test49', 3);
  await iosPhase3Requester.waitForMarker('E2E: IOS_TEST49_PHASE3_REMOVAL_REQUESTED', 180_000);
  await androidPhase3Decliner.waitForMarker(
    'E2E: ANDROID_TEST49_PHASE3_REMOVAL_REQUEST_VISIBLE',
    180_000,
  );
  approvalCoordinator.allow('android-decline-web-test49', 3);
  await androidPhase3Decliner.waitForMarker(
    'E2E: ANDROID_TEST49_PHASE3_REMOVAL_DECLINED',
    180_000,
  );
  approvalCoordinator.allow('ios-phase3-decline-observed-test49', 3);
  await iosPhase3Requester.waitForMarker('E2E: IOS_TEST49_PHASE3_DECLINE_OBSERVED', 180_000);
  await Promise.all([androidPhase3Decliner.result, iosPhase3Requester.result]);
  await page.reload({ waitUntil: 'domcontentloaded' });
  await unlockWithPasskeyIfNeeded(page);
  await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #49 phase-3 terminal Decline boundary',
    expectedDeviceNames,
  );
  console.log('E2E: WEB_TEST49_PHASE3_DECLINE_OBSERVED');
  await runRecoveryPhase(3);

  return { completedCycles: recoveryCycles.length };
}

async function assertWebResetAfterOwnRemoval(page, secretNames) {
  await page.getByPlaceholder('vault name').waitFor({ state: 'visible', timeout: 180_000 });
  await page.getByRole('button', { name: 'Set Vault Name', exact: true })
    .waitFor({ state: 'visible', timeout: 30_000 });
  await Promise.all([
    page.getByRole('link', { name: 'Secrets', exact: true }).waitFor({ state: 'hidden', timeout: 30_000 }),
    page.getByRole('link', { name: 'Devices', exact: true }).waitFor({ state: 'hidden', timeout: 30_000 }),
    page.getByTestId('pending-device-row').waitFor({ state: 'hidden', timeout: 30_000 }),
    page.getByTestId('pending-removal-device-row').waitFor({ state: 'hidden', timeout: 30_000 }),
    page.getByTestId('remove-device-dialog').waitFor({ state: 'hidden', timeout: 30_000 }),
    page.locator('[data-slot="alert-dialog-content"]').waitFor({ state: 'hidden', timeout: 30_000 }),
  ]);
  for (const secretName of secretNames) {
    await Promise.all([
      page.getByText(secretName, { exact: true }).waitFor({ state: 'hidden', timeout: 30_000 }),
      page.getByTestId(`secret-primary-action-${secretName}`)
        .waitFor({ state: 'hidden', timeout: 30_000 }),
      page.getByTestId(`open-recovery-request-${secretName}`)
        .waitFor({ state: 'hidden', timeout: 30_000 }),
    ]);
  }
  console.log(`E2E: WEB_TEST${testNumber}_REMOVED_RESET`);
  writeDiagnostic(
    `[TEST${testNumber}] Web reset to registration with no stale Vault/removal/recovery UI`,
  );
}

async function runAndroidMemberRemovalLifecycle(page, simulatorUdid, androidSerial) {
  const allSecretNames = ['secret1', 'secret2', 'secret3'].map(secretNameFor);
  const [initialName, secondName, thirdName] = allSecretNames;
  const firstRemoval = scenario.membership?.firstRemoval;
  const secondRemoval = scenario.membership?.secondRemoval;
  if ((scenario.membership?.initialPlatforms ?? []).join(',') !== 'android,ios,web') {
    throw new Error('Test #38 requires the Android, iOS, Web three-member boundary');
  }
  if ((scenario.setup?.joinOrder ?? []).join(',') !== 'ios,web') {
    throw new Error('Test #38 requires iOS then Web Join order');
  }
  if (
    firstRemoval?.requester !== 'android'
    || firstRemoval?.approver !== 'ios'
    || firstRemoval?.target !== 'web'
    || firstRemoval?.expectedMemberCount !== 2
  ) {
    throw new Error('Test #38 first removal must be Android -> iOS approval -> Web target');
  }
  if (
    secondRemoval?.requester !== 'android'
    || secondRemoval?.approver !== null
    || secondRemoval?.target !== 'ios'
    || secondRemoval?.expectedMemberCount !== 1
  ) {
    throw new Error('Test #38 second removal must be Android local two-to-one removal of iOS');
  }
  if (recoveryCycles.length !== 9 || recoveryCycles.some((cycle) => (
    cycle.kind !== 'direct-show' || cycle.approvals.length !== 0 || cycle.senders.length !== 1
  ))) {
    throw new Error('Test #38 requires exactly nine single-sender direct-Show cycles and zero recovery');
  }
  const firstBlock = recoveryCycles.slice(0, 6);
  const finalBlock = recoveryCycles.slice(6);
  const actualShowMatrix = recoveryCycles.map((cycle) => (
    `${cycle.block}:${cycle.senders[0]}:${cycle.senderSecrets[cycle.senders[0]]}`
  ));
  const expectedShowMatrix = [
    ...allSecretNames.map((name) => `after-web-removal:android:${name}`),
    ...allSecretNames.map((name) => `after-web-removal:ios:${name}`),
    ...allSecretNames.map((name) => `after-ios-removal:android:${name}`),
  ];
  if (actualShowMatrix.join('|') !== expectedShowMatrix.join('|')) {
    throw new Error('Test #38 direct-Show matrix does not match the two removal boundaries');
  }

  setJoinExpectedSecretNames([initialName]);
  const androidSetup = await startAndroidJoinTest(androidSerial, {
    testClass: scenario.android.initiatorTestClass,
    testMethod: scenario.android.initiatorSetupTestMethod,
    expectedSecretNames: [initialName],
  });
  await androidSetup.waitForMarker('E2E: ANDROID_TEST38_SECRET1_READY', 180_000);
  await androidSetup.waitForMarker('E2E: ANDROID_TEST38_INITIATOR_SETUP_READY', 60_000);
  await androidSetup.result;
  writeDiagnostic('[TEST38] Android Vault and Secret1 persisted');

  const iosJoin = startIosJoinTest(simulatorUdid, {
    testClass: scenario.ios.testClass,
    testMethod: scenario.ios.joinTestMethod,
    label: 'iOS Test #38 joins Android Vault',
  });
  void iosJoin.result.catch(() => {});
  await iosJoin.waitForMarker('E2E: IOS_TEST38_JOIN_REQUEST_SENT', 300_000);
  const approveIosJoin = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.initiatorTestClass,
    testMethod: scenario.android.approveJoinTestMethod,
    role: 'join-approver',
    cycle: 0,
    step: 1,
    joinTarget: 'ios',
    label: 'Android Test #38 approves iOS Join',
  });
  await approveIosJoin.waitForMarker('E2E: ANDROID_TEST38_JOIN_APPROVED_IOS', 180_000);
  await iosJoin.waitForMarker('E2E: IOS_TEST38_JOIN_READY', 180_000);
  await iosJoin.waitForMarker('E2E: IOS_TEST38_SECRETS_READY', 60_000);
  await Promise.all([approveIosJoin.result, iosJoin.result]);
  writeDiagnostic('[TEST38] iOS joined Android Vault and observed Secret1');

  const createSecond = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.initiatorTestClass,
    testMethod: scenario.android.createSecretTestMethod,
    role: 'creator',
    cycle: 2,
    step: 1,
    secretName: secondName,
    secretValue: secretValueForName(secondName),
    expectedSecretNames: [initialName, secondName],
    label: 'Android Test #38 creates Secret2',
  });
  await createSecond.waitForMarker('E2E: ANDROID_TEST38_SECRET_CREATED_2', 180_000);
  await createSecond.result;
  setJoinExpectedSecretNames([initialName, secondName]);
  writeDiagnostic('[TEST38] Android Secret2 persisted before Web Join');

  await submitWebJoinRequest(page);
  const approveWebJoin = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.initiatorTestClass,
    testMethod: scenario.android.approveJoinTestMethod,
    role: 'join-approver',
    cycle: 0,
    step: 1,
    joinTarget: 'web',
    label: 'Android Test #38 approves Web Join',
  });
  await approveWebJoin.waitForMarker('E2E: ANDROID_TEST38_JOIN_APPROVED_WEB', 180_000);
  await approveWebJoin.result;
  await page.reload({ waitUntil: 'domcontentloaded' });
  await unlockWithPasskeyIfNeeded(page);
  await waitForWebSecretNames(page, [initialName, secondName]);
  writeDiagnostic('[TEST38] Web joined Android Vault and observed Secret1/Secret2');

  const createThird = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.initiatorTestClass,
    testMethod: scenario.android.createSecretTestMethod,
    role: 'creator',
    cycle: 3,
    step: 1,
    secretName: thirdName,
    secretValue: secretValueForName(thirdName),
    expectedSecretNames: allSecretNames,
    label: 'Android Test #38 creates Secret3',
  });
  await createThird.waitForMarker('E2E: ANDROID_TEST38_SECRET_CREATED_3', 180_000);
  await createThird.result;
  setJoinExpectedSecretNames(allSecretNames);
  await waitForWebSecretNames(page, allSecretNames);

  const expectedDeviceNames = await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #38 three-device boundary',
  );
  writeDiagnostic(
    '[TEST38] canonical registered device identities discovered from exact Web rows '
      + `web=${JSON.stringify(expectedDeviceNames.web)} `
      + `android=${JSON.stringify(expectedDeviceNames.android)} `
      + `ios=${JSON.stringify(expectedDeviceNames.ios)}`,
  );

  const iosRemovalApproval = startIosJoinTest(simulatorUdid, {
    testClass: scenario.ios.testClass,
    testMethod: scenario.ios.approveRemovalTestMethod,
    label: 'iOS Test #38 approves Web removal',
    environment: stepEnvironment({ role: 'removal-approver', cycle: 1, step: 1, sender: 'android' }),
    secretName: thirdName,
    secretValue: secretValueForName(thirdName),
    expectedDeviceNames,
  });
  void iosRemovalApproval.result.catch(() => {});
  const androidRemovalRequester = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.lifecycleTestClass,
    testMethod: scenario.android.requestRemovalTestMethod,
    role: 'removal-requester',
    cycle: 1,
    step: 1,
    joinTarget: expectedDeviceNames.web,
    expectedSecretNames: allSecretNames,
    expectedDeviceNames,
    label: 'Android Test #38 requests Web removal',
  });
  await Promise.all([
    iosRemovalApproval.waitForMarker('E2E: IOS_TEST38_REMOVAL_RECEIVER_READY', 240_000),
    androidRemovalRequester.waitForMarker(
      'E2E: ANDROID_TEST38_WEB_REMOVAL_REQUESTER_READY',
      180_000,
    ),
  ]);
  await assertNoStaleWebMembershipUi(page, allSecretNames);
  approvalCoordinator.allow('android-remove-web-start', 1);
  await iosRemovalApproval.waitForMarker('E2E: IOS_TEST38_REMOVAL_REQUEST_VISIBLE', 180_000);
  approvalCoordinator.allow('android-web-request-observed', 1);
  await androidRemovalRequester.waitForMarker('E2E: ANDROID_TEST38_WEB_REMOVAL_REQUESTED', 180_000);
  approvalCoordinator.allow('ios-remove-web-approve', 1);
  await Promise.all([
    iosRemovalApproval.waitForMarker('E2E: IOS_TEST38_WEB_REMOVAL_APPROVED', 180_000),
    androidRemovalRequester.waitForMarker('E2E: ANDROID_TEST38_WEB_REMOVAL_COMPLETED', 180_000),
    assertWebResetAfterOwnRemoval(page, allSecretNames),
  ]);
  await Promise.all([iosRemovalApproval.result, androidRemovalRequester.result]);

  for (const cycle of firstBlock) {
    setCurrentCycleForDiagnostics(cycle);
    const sender = cycle.senders[0];
    const secretName = cycle.senderSecrets[sender];
    if (sender === 'android') {
      const show = await startAndroidStepTest(androidSerial, {
        testClass: scenario.android.lifecycleTestClass,
        testMethod: scenario.android.showSecretTestMethod,
        role: 'direct-show',
        cycle: cycle.number,
        step: 1,
        secretName,
        secretValue: secretValueForName(secretName),
        expectedSecretNames: allSecretNames,
        label: `Android Test #38 direct Show cycle ${cycle.number}`,
      });
      await show.waitForMarker(
        `E2E: ANDROID_TEST38_LOCAL_SECRET_VALUE_VERIFIED_${cycle.number}`,
        180_000,
      );
      await show.waitForMarker(`E2E: ANDROID_TEST38_LOCAL_SECRET_CLOSED_${cycle.number}`, 60_000);
      await show.result;
    } else if (sender === 'ios') {
      const show = startIosJoinTest(simulatorUdid, {
        testClass: scenario.ios.testClass,
        testMethod: scenario.ios.showSecretTestMethod,
        label: `iOS Test #38 direct Show cycle ${cycle.number}`,
        environment: stepEnvironment({ role: 'direct-show', cycle: cycle.number, step: 1 }),
        secretName,
        secretValue: secretValueForName(secretName),
      });
      void show.result.catch(() => {});
      await show.waitForMarker(
        `E2E: IOS_TEST38_LOCAL_SECRET_VALUE_VERIFIED_${cycle.number}`,
        180_000,
      );
      await show.waitForMarker(`E2E: IOS_TEST38_LOCAL_SECRET_VISIBLE_${cycle.number}`, 60_000);
      await show.waitForMarker(`E2E: IOS_TEST38_LOCAL_SECRET_CLOSED_${cycle.number}`, 60_000);
      await show.result;
    } else {
      throw new Error(`Test #38 first direct-Show block has unsupported sender=${sender}`);
    }
    writeDiagnostic(
      `[TEST38][after-web-removal][cycle=${cycle.number}] sender=${sender} `
        + `secret=${secretName} directShow=true exactValue=verified closed=true`,
    );
  }

  const iosRemovalTarget = startIosJoinTest(simulatorUdid, {
    testClass: scenario.ios.testClass,
    testMethod: scenario.ios.removalObserverTestMethod,
    label: 'iOS Test #38 observes own removal',
    environment: stepEnvironment({ role: 'removal-target', cycle: 2, step: 1, sender: 'android' }),
    secretName: thirdName,
    secretValue: secretValueForName(thirdName),
    expectedDeviceNames,
  });
  void iosRemovalTarget.result.catch(() => {});
  await iosRemovalTarget.waitForMarker('E2E: IOS_TEST38_REMOVAL_TARGET_READY', 240_000);
  const androidLocalRemoval = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.lifecycleTestClass,
    testMethod: scenario.android.localRemovalTestMethod,
    role: 'removal-requester',
    cycle: 2,
    step: 1,
    joinTarget: expectedDeviceNames.ios,
    expectedSecretNames: allSecretNames,
    expectedDeviceNames,
    label: 'Android Test #38 removes iOS locally',
  });
  await androidLocalRemoval.waitForMarker('E2E: ANDROID_TEST38_IOS_LOCAL_REMOVAL_READY', 180_000);
  approvalCoordinator.allow('android-remove-ios-local', 2);
  await androidLocalRemoval.waitForMarker(
    'E2E: ANDROID_TEST38_IOS_LOCAL_REMOVAL_REQUESTED',
    180_000,
  );
  await Promise.all([
    iosRemovalTarget.waitForMarker('E2E: IOS_TEST38_REMOVED_RESET', 180_000),
    androidLocalRemoval.waitForMarker(
      'E2E: ANDROID_TEST38_IOS_LOCAL_REMOVAL_COMPLETED',
      180_000,
    ),
  ]);
  await Promise.all([iosRemovalTarget.result, androidLocalRemoval.result]);

  for (const cycle of finalBlock) {
    setCurrentCycleForDiagnostics(cycle);
    const secretName = cycle.senderSecrets.android;
    const show = await startAndroidStepTest(androidSerial, {
      testClass: scenario.android.lifecycleTestClass,
      testMethod: scenario.android.showSecretTestMethod,
      role: 'direct-show',
      cycle: cycle.number,
      step: 1,
      secretName,
      secretValue: secretValueForName(secretName),
      expectedSecretNames: allSecretNames,
      label: `Android Test #38 single-member Show cycle ${cycle.number}`,
    });
    await show.waitForMarker(
      `E2E: ANDROID_TEST38_LOCAL_SECRET_VALUE_VERIFIED_${cycle.number}`,
      180_000,
    );
    await show.waitForMarker(`E2E: ANDROID_TEST38_LOCAL_SECRET_CLOSED_${cycle.number}`, 60_000);
    await show.result;
    writeDiagnostic(
      `[TEST38][after-ios-removal][cycle=${cycle.number}] sender=android `
        + `secret=${secretName} directShow=true exactValue=verified closed=true`,
    );
  }
  return { completedCycles: recoveryCycles.length };
}

async function runAndroidSecondaryCreatorMemberRemovalLifecycle(page, simulatorUdid, androidSerial) {
  const allSecretNames = ['secret1', 'secret2', 'secret3'].map(secretNameFor);
  const [initialName, secondName, thirdName] = allSecretNames;
  const firstRemoval = scenario.membership?.firstRemoval;
  const secondRemoval = scenario.membership?.secondRemoval;
  if ((scenario.membership?.initialPlatforms ?? []).join(',') !== 'android,ios,web') {
    throw new Error('Test #41 requires the Android, iOS, Web three-member boundary');
  }
  if ((scenario.setup?.joinOrder ?? []).join(',') !== 'ios,web') {
    throw new Error('Test #41 requires iOS then Web Join order');
  }
  if (scenario.setup?.secondJoinApprover !== 'android') {
    throw new Error('Test #41 requires Android as the sole approver of both Joins');
  }
  if (
    (secretCreationPlan?.initial?.android ?? []).join(',') !== initialName
    || (secretCreationPlan?.afterIosJoin?.ios ?? []).join(',') !== secondName
    || (secretCreationPlan?.afterWebJoin?.ios ?? []).join(',') !== thirdName
  ) {
    throw new Error('Test #41 requires Android Secret1, then iOS Secret2 and Secret3');
  }
  if (
    firstRemoval?.requester !== 'android'
    || firstRemoval?.approver !== 'web'
    || firstRemoval?.target !== 'ios'
    || firstRemoval?.expectedMemberCount !== 2
  ) {
    throw new Error('Test #41 first removal must be Android -> Web approval -> iOS target');
  }
  if (
    secondRemoval?.requester !== 'web'
    || secondRemoval?.approver !== null
    || secondRemoval?.target !== 'android'
    || secondRemoval?.expectedMemberCount !== 1
  ) {
    throw new Error('Test #41 second removal must be Web local two-to-one removal of Android');
  }
  if (recoveryCycles.length !== 9 || recoveryCycles.some((cycle) => (
    cycle.kind !== 'direct-show' || cycle.approvals.length !== 0 || cycle.senders.length !== 1
  ))) {
    throw new Error('Test #41 requires exactly nine single-sender direct-Show cycles and zero recovery');
  }
  const firstBlock = recoveryCycles.slice(0, 6);
  const finalBlock = recoveryCycles.slice(6);
  const actualShowMatrix = recoveryCycles.map((cycle) => (
    `${cycle.block}:${cycle.senders[0]}:${cycle.senderSecrets[cycle.senders[0]]}`
  ));
  const expectedShowMatrix = [
    ...allSecretNames.map((name) => `after-ios-removal:android:${name}`),
    ...allSecretNames.map((name) => `after-ios-removal:web:${name}`),
    ...allSecretNames.map((name) => `after-android-removal:web:${name}`),
  ];
  if (actualShowMatrix.join('|') !== expectedShowMatrix.join('|')) {
    throw new Error('Test #41 direct-Show matrix does not match the two removal boundaries');
  }

  const startIosAction = ({
    testMethod,
    role,
    cycle = 0,
    secretName,
    sender = '',
    expectedDeviceNames,
    label,
  }) => startIosJoinTest(simulatorUdid, {
    testClass: scenario.ios.testClass,
    testMethod,
    label,
    environment: stepEnvironment({ role, cycle, step: 1, sender, secretName }),
    secretName,
    secretValue: secretValueForName(secretName),
    expectedDeviceNames,
  });

  setJoinExpectedSecretNames([initialName]);
  const androidSetup = await startAndroidJoinTest(androidSerial, {
    testClass: scenario.android.initiatorTestClass,
    testMethod: scenario.android.initiatorSetupTestMethod,
    expectedSecretNames: [initialName],
  });
  await androidSetup.waitForMarker('E2E: ANDROID_TEST41_SECRET1_READY', 180_000);
  await androidSetup.waitForMarker('E2E: ANDROID_TEST41_INITIATOR_SETUP_READY', 60_000);
  await androidSetup.result;
  writeDiagnostic('[TEST41] Android Vault and Secret1 persisted');

  const iosJoin = startIosJoinTest(simulatorUdid, {
    testClass: scenario.ios.testClass,
    testMethod: scenario.ios.joinTestMethod,
    label: 'iOS Test #41 joins Android-owned Vault',
  });
  void iosJoin.result.catch(() => {});
  await iosJoin.waitForMarker('E2E: IOS_TEST41_JOIN_REQUEST_SENT', 300_000);
  const approveIosJoin = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.initiatorTestClass,
    testMethod: scenario.android.approveJoinTestMethod,
    role: 'join-approver',
    cycle: 0,
    step: 1,
    joinTarget: 'ios',
    label: 'Android Test #41 approves iOS Join',
  });
  await approveIosJoin.waitForMarker('E2E: ANDROID_TEST41_JOIN_APPROVED_IOS', 180_000);
  await iosJoin.waitForMarker('E2E: IOS_TEST41_JOIN_READY', 180_000);
  await iosJoin.waitForMarker('E2E: IOS_TEST41_SECRETS_READY', 60_000);
  await Promise.all([approveIosJoin.result, iosJoin.result]);

  const createSecond = startIosAction({
    testMethod: scenario.ios.createSecretTestMethod,
    role: 'creator',
    cycle: 2,
    secretName: secondName,
    label: 'iOS Test #41 creates Secret2',
  });
  await createSecond.waitForMarker('E2E: IOS_TEST41_SECRET_CREATED_2', 180_000);
  await createSecond.result;
  setJoinExpectedSecretNames([initialName, secondName]);

  await submitWebJoinRequest(page);
  const approveWebJoin = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.initiatorTestClass,
    testMethod: scenario.android.approveJoinTestMethod,
    role: 'join-approver',
    cycle: 0,
    step: 1,
    joinTarget: 'web',
    label: 'Android Test #41 approves Web Join',
  });
  await approveWebJoin.waitForMarker('E2E: ANDROID_TEST41_JOIN_APPROVED_WEB', 180_000);
  await approveWebJoin.result;
  await page.reload({ waitUntil: 'domcontentloaded' });
  await unlockWithPasskeyIfNeeded(page);
  await waitForWebSecretNames(page, [initialName, secondName]);

  const createThird = startIosAction({
    testMethod: scenario.ios.createSecretTestMethod,
    role: 'creator',
    cycle: 3,
    secretName: thirdName,
    label: 'iOS Test #41 creates Secret3',
  });
  await createThird.waitForMarker('E2E: IOS_TEST41_SECRET_CREATED_3', 180_000);
  await createThird.result;
  setJoinExpectedSecretNames(allSecretNames);
  await waitForWebSecretNames(page, allSecretNames);

  const expectedDeviceNames = await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #41 three-device boundary',
  );
  writeDiagnostic(
    '[TEST41] canonical registered device identities discovered from exact Web rows '
      + `web=${JSON.stringify(expectedDeviceNames.web)} `
      + `android=${JSON.stringify(expectedDeviceNames.android)} `
      + `ios=${JSON.stringify(expectedDeviceNames.ios)}`,
  );

  const iosRemovalTarget = startIosAction({
    testMethod: scenario.ios.removalObserverTestMethod,
    role: 'removal-target',
    cycle: 1,
    secretName: thirdName,
    sender: 'android',
    expectedDeviceNames,
    label: 'iOS Test #41 observes own removal',
  });
  void iosRemovalTarget.result.catch(() => {});
  const androidRemovalRequester = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.lifecycleTestClass,
    testMethod: scenario.android.requestRemovalTestMethod,
    role: 'removal-requester',
    cycle: 1,
    step: 1,
    joinTarget: expectedDeviceNames.ios,
    expectedSecretNames: allSecretNames,
    expectedDeviceNames,
    label: 'Android Test #41 requests iOS removal',
  });
  await Promise.all([
    iosRemovalTarget.waitForMarker('E2E: IOS_TEST41_REMOVAL_TARGET_READY', 240_000),
    androidRemovalRequester.waitForMarker(
      'E2E: ANDROID_TEST41_IOS_REMOVAL_REQUESTER_READY',
      180_000,
    ),
  ]);
  await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #41 Web removal receiver boundary',
    expectedDeviceNames,
  );
  console.log('E2E: WEB_TEST41_REMOVAL_RECEIVER_READY');

  approvalCoordinator.allow('android-remove-ios-start-test41', 1);
  await androidRemovalRequester.waitForMarker(
    'E2E: ANDROID_TEST41_IOS_REMOVAL_REQUESTED',
    180_000,
  );
  await page.reload({ waitUntil: 'domcontentloaded' });
  await unlockWithPasskeyIfNeeded(page);
  await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #41 persisted iOS removal request visible to Web',
    expectedDeviceNames,
  );
  const webApproval = initiateWebDeviceRemoval(page, {
    targetCategory: 'iPhone',
    expectedTargetDeviceName: expectedDeviceNames.ios,
    waitForTargetGone: true,
    label: 'Test #41 Web approval of iOS removal',
    requestVisibleMarker: 'WEB_TEST41_REMOVAL_REQUEST_VISIBLE',
    completionMarker: 'WEB_TEST41_IOS_REMOVAL_APPROVED',
  });
  await Promise.all([
    webApproval,
    androidRemovalRequester.waitForMarker(
      'E2E: ANDROID_TEST41_IOS_REMOVAL_COMPLETED',
      180_000,
    ),
    iosRemovalTarget.waitForMarker('E2E: IOS_TEST41_REMOVED_RESET', 180_000),
  ]);
  await Promise.all([androidRemovalRequester.result, iosRemovalTarget.result]);
  await assertWebMembership(
    page,
    ['Web', 'Android'],
    allSecretNames,
    'Test #41 two-device reshared boundary',
    expectedDeviceNames,
  );

  for (const cycle of firstBlock) {
    setCurrentCycleForDiagnostics(cycle);
    const sender = cycle.senders[0];
    const secretName = cycle.senderSecrets[sender];
    if (sender === 'android') {
      const show = await startAndroidStepTest(androidSerial, {
        testClass: scenario.android.lifecycleTestClass,
        testMethod: scenario.android.showSecretTestMethod,
        role: 'direct-show',
        cycle: cycle.number,
        step: 1,
        secretName,
        secretValue: secretValueForName(secretName),
        expectedSecretNames: allSecretNames,
        label: `Android Test #41 direct Show cycle ${cycle.number}`,
      });
      await show.waitForMarker(
        `E2E: ANDROID_TEST41_LOCAL_SECRET_VALUE_VERIFIED_${cycle.number}`,
        180_000,
      );
      await show.waitForMarker(`E2E: ANDROID_TEST41_LOCAL_SECRET_CLOSED_${cycle.number}`, 60_000);
      await show.result;
    } else if (sender === 'web') {
      await showAndCloseWebSecret(page, secretName);
      console.log(`E2E: WEB_TEST41_LOCAL_SECRET_VALUE_VERIFIED_${cycle.number}`);
      console.log(`E2E: WEB_TEST41_LOCAL_SECRET_CLOSED_${cycle.number}`);
    } else {
      throw new Error(`Test #41 first direct-Show block has unsupported sender=${sender}`);
    }
    await assertNoStaleWebMembershipUi(page, allSecretNames);
    writeDiagnostic(
      `[TEST41][after-ios-removal][cycle=${cycle.number}] sender=${sender} `
        + `secret=${secretName} directShow=true exactValue=verified closed=true`,
    );
  }

  const androidRemovalTarget = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.lifecycleTestClass,
    testMethod: scenario.android.removalObserverTestMethod,
    role: 'removal-target',
    cycle: 2,
    step: 1,
    expectedSecretNames: allSecretNames,
    expectedDeviceNames,
    label: 'Android Test #41 observes own local removal',
  });
  await androidRemovalTarget.waitForMarker('E2E: ANDROID_TEST41_REMOVAL_TARGET_READY', 180_000);
  await assertWebMembership(
    page,
    ['Web', 'Android'],
    allSecretNames,
    'Test #41 pre-two-to-one boundary',
    expectedDeviceNames,
  );
  console.log('E2E: WEB_TEST41_ANDROID_LOCAL_REMOVAL_READY');
  await Promise.all([
    initiateWebDeviceRemoval(page, {
      targetCategory: 'Android',
      expectedTargetDeviceName: expectedDeviceNames.android,
      waitForTargetGone: false,
      label: 'Test #41 Web local removal of Android',
      requestVisibleMarker: 'WEB_TEST41_ANDROID_LOCAL_REMOVAL_REQUEST_VISIBLE',
      completionMarker: 'WEB_TEST41_ANDROID_LOCAL_REMOVAL_COMPLETED',
    }),
    androidRemovalTarget.waitForMarker('E2E: ANDROID_TEST41_REMOVED_RESET', 180_000),
  ]);
  await androidRemovalTarget.result;
  await assertWebMembership(
    page,
    ['Web'],
    allSecretNames,
    'Test #41 single-device boundary',
    expectedDeviceNames,
  );

  for (const cycle of finalBlock) {
    setCurrentCycleForDiagnostics(cycle);
    const secretName = cycle.senderSecrets.web;
    await showAndCloseWebSecret(page, secretName);
    console.log(`E2E: WEB_TEST41_LOCAL_SECRET_VALUE_VERIFIED_${cycle.number}`);
    console.log(`E2E: WEB_TEST41_LOCAL_SECRET_CLOSED_${cycle.number}`);
    await assertNoStaleWebMembershipUi(page, allSecretNames);
    writeDiagnostic(
      `[TEST41][after-android-removal][cycle=${cycle.number}] sender=web `
        + `secret=${secretName} directShow=true exactValue=verified closed=true`,
    );
  }
  return { completedCycles: recoveryCycles.length };
}

async function runAndroidDistributedCreatorMemberRemovalLifecycle(page, simulatorUdid, androidSerial) {
  const allSecretNames = ['secret1', 'secret2', 'secret3'].map(secretNameFor);
  const [initialName, secondName, thirdName] = allSecretNames;
  const firstRemoval = scenario.membership?.firstRemoval;
  const secondRemoval = scenario.membership?.secondRemoval;
  if (testNumber !== 44) {
    throw new Error(`Android distributed-creator removal lifecycle is not defined for Test #${testNumber}`);
  }
  if ((scenario.membership?.initialPlatforms ?? []).join(',') !== 'android,ios,web') {
    throw new Error('Test #44 requires the Android, iOS, Web three-member boundary');
  }
  if ((scenario.setup?.joinOrder ?? []).join(',') !== 'ios,web') {
    throw new Error('Test #44 requires iOS then Web Join order');
  }
  if (scenario.setup?.secondJoinApprover !== 'android') {
    throw new Error('Test #44 requires Android as the sole approver of both Joins');
  }
  if (
    (secretCreationPlan?.initial?.android ?? []).join(',') !== initialName
    || (secretCreationPlan?.afterIosJoin?.ios ?? []).join(',') !== secondName
    || (secretCreationPlan?.afterWebJoin?.web ?? []).join(',') !== thirdName
  ) {
    throw new Error('Test #44 requires Android Secret1, iOS Secret2, then Web Secret3');
  }
  if (
    firstRemoval?.requester !== 'web'
    || firstRemoval?.approver !== 'ios'
    || firstRemoval?.target !== 'android'
    || firstRemoval?.expectedMemberCount !== 2
  ) {
    throw new Error('Test #44 first removal must be Web -> iOS approval -> Android target');
  }
  if (
    secondRemoval?.requester !== 'ios'
    || secondRemoval?.approver !== null
    || secondRemoval?.target !== 'web'
    || secondRemoval?.expectedMemberCount !== 1
  ) {
    throw new Error('Test #44 second removal must be iOS local two-to-one removal of Web');
  }
  if (recoveryCycles.length !== 9 || recoveryCycles.some((cycle) => (
    cycle.kind !== 'direct-show' || cycle.approvals.length !== 0 || cycle.senders.length !== 1
  ))) {
    throw new Error('Test #44 requires exactly nine single-sender direct-Show cycles and zero recovery');
  }
  const firstBlock = recoveryCycles.slice(0, 6);
  const finalBlock = recoveryCycles.slice(6);
  const actualShowMatrix = recoveryCycles.map((cycle) => (
    `${cycle.block}:${cycle.senders[0]}:${cycle.senderSecrets[cycle.senders[0]]}`
  ));
  const expectedShowMatrix = [
    ...allSecretNames.map((name) => `after-android-removal:ios:${name}`),
    ...allSecretNames.map((name) => `after-android-removal:web:${name}`),
    ...allSecretNames.map((name) => `after-web-removal:ios:${name}`),
  ];
  if (actualShowMatrix.join('|') !== expectedShowMatrix.join('|')) {
    throw new Error('Test #44 direct-Show matrix does not match the two removal boundaries');
  }

  const startIosAction = ({
    testMethod,
    role,
    cycle = 0,
    secretName,
    sender = '',
    expectedDeviceNames,
    label,
  }) => startIosJoinTest(simulatorUdid, {
    testClass: scenario.ios.testClass,
    testMethod,
    label,
    environment: stepEnvironment({ role, cycle, step: 1, sender, secretName }),
    secretName,
    secretValue: secretValueForName(secretName),
    expectedDeviceNames,
  });

  setJoinExpectedSecretNames([initialName]);
  const androidSetup = await startAndroidJoinTest(androidSerial, {
    testClass: scenario.android.initiatorTestClass,
    testMethod: scenario.android.initiatorSetupTestMethod,
    expectedSecretNames: [initialName],
  });
  await androidSetup.waitForMarker('E2E: ANDROID_TEST44_SECRET1_READY', 180_000);
  await androidSetup.waitForMarker('E2E: ANDROID_TEST44_INITIATOR_SETUP_READY', 60_000);
  await androidSetup.result;
  writeDiagnostic('[TEST44] Android Vault and Secret1 persisted');

  const iosJoin = startIosJoinTest(simulatorUdid, {
    testClass: scenario.ios.testClass,
    testMethod: scenario.ios.joinTestMethod,
    label: 'iOS Test #44 joins Android-owned Vault',
  });
  void iosJoin.result.catch(() => {});
  await iosJoin.waitForMarker('E2E: IOS_TEST44_JOIN_REQUEST_SENT', 300_000);
  const approveIosJoin = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.initiatorTestClass,
    testMethod: scenario.android.approveJoinTestMethod,
    role: 'join-approver',
    cycle: 0,
    step: 1,
    joinTarget: 'ios',
    label: 'Android Test #44 approves iOS Join',
  });
  await approveIosJoin.waitForMarker('E2E: ANDROID_TEST44_JOIN_APPROVED_IOS', 180_000);
  await iosJoin.waitForMarker('E2E: IOS_TEST44_JOIN_READY', 180_000);
  await iosJoin.waitForMarker('E2E: IOS_TEST44_SECRETS_READY', 60_000);
  await Promise.all([approveIosJoin.result, iosJoin.result]);

  const createSecond = startIosAction({
    testMethod: scenario.ios.createSecretTestMethod,
    role: 'creator',
    cycle: 2,
    secretName: secondName,
    label: 'iOS Test #44 creates Secret2',
  });
  void createSecond.result.catch(() => {});
  await createSecond.waitForMarker('E2E: IOS_TEST44_SECRET_CREATED_2', 180_000);
  await createSecond.result;
  setJoinExpectedSecretNames([initialName, secondName]);

  await submitWebJoinRequest(page);
  const approveWebJoin = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.initiatorTestClass,
    testMethod: scenario.android.approveJoinTestMethod,
    role: 'join-approver',
    cycle: 0,
    step: 1,
    joinTarget: 'web',
    label: 'Android Test #44 approves Web Join',
  });
  await approveWebJoin.waitForMarker('E2E: ANDROID_TEST44_JOIN_APPROVED_WEB', 180_000);
  await approveWebJoin.result;
  await page.reload({ waitUntil: 'domcontentloaded' });
  await unlockWithPasskeyIfNeeded(page);
  await waitForWebSecretNames(page, [initialName, secondName]);

  await createWebSecret(page, thirdName);
  await waitForWebSecretNames(page, allSecretNames);
  console.log('E2E: WEB_TEST44_SECRET_CREATED_3');
  setJoinExpectedSecretNames(allSecretNames);

  const expectedDeviceNames = await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #44 three-device boundary',
  );
  writeDiagnostic(
    '[TEST44] canonical registered device identities discovered from exact Web rows '
      + `web=${JSON.stringify(expectedDeviceNames.web)} `
      + `android=${JSON.stringify(expectedDeviceNames.android)} `
      + `ios=${JSON.stringify(expectedDeviceNames.ios)}`,
  );

  const androidRemovalTarget = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.lifecycleTestClass,
    testMethod: scenario.android.removalObserverTestMethod,
    role: 'removal-target',
    cycle: 1,
    step: 1,
    expectedSecretNames: allSecretNames,
    expectedDeviceNames,
    label: 'Android Test #44 observes own removal',
  });
  const iosRemovalApproval = startIosAction({
    testMethod: scenario.ios.approveRemovalTestMethod,
    role: 'removal-approver',
    cycle: 1,
    secretName: thirdName,
    sender: 'web',
    expectedDeviceNames,
    label: 'iOS Test #44 approves Android removal',
  });
  void iosRemovalApproval.result.catch(() => {});
  await Promise.all([
    androidRemovalTarget.waitForMarker('E2E: ANDROID_TEST44_REMOVAL_TARGET_READY', 180_000),
    iosRemovalApproval.waitForMarker('E2E: IOS_TEST44_REMOVAL_RECEIVER_READY', 240_000),
  ]);
  await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #44 Web removal requester boundary',
    expectedDeviceNames,
  );
  console.log('E2E: WEB_TEST44_ANDROID_REMOVAL_REQUESTER_READY');

  await initiateWebDeviceRemoval(page, {
    targetCategory: 'Android',
    expectedTargetDeviceName: expectedDeviceNames.android,
    waitForTargetGone: false,
    label: 'Test #44 Web request to remove Android',
  });
  await iosRemovalApproval.waitForMarker('E2E: IOS_TEST44_REMOVAL_REQUEST_VISIBLE', 180_000);
  console.log('E2E: WEB_TEST44_ANDROID_REMOVAL_REQUESTED');
  approvalCoordinator.allow('ios-remove-android-approve-test44', 1);
  await Promise.all([
    iosRemovalApproval.waitForMarker('E2E: IOS_TEST44_ANDROID_REMOVAL_APPROVED', 180_000),
    androidRemovalTarget.waitForMarker('E2E: ANDROID_TEST44_REMOVED_RESET', 180_000),
  ]);
  await Promise.all([iosRemovalApproval.result, androidRemovalTarget.result]);
  await assertWebMembership(
    page,
    ['Web', 'iPhone'],
    allSecretNames,
    'Test #44 two-device reshared boundary',
    expectedDeviceNames,
  );
  console.log('E2E: WEB_TEST44_ANDROID_REMOVAL_COMPLETED');

  for (const cycle of firstBlock) {
    setCurrentCycleForDiagnostics(cycle);
    const sender = cycle.senders[0];
    const secretName = cycle.senderSecrets[sender];
    if (sender === 'ios') {
      const show = startIosAction({
        testMethod: scenario.ios.showSecretTestMethod,
        role: 'direct-show',
        cycle: cycle.number,
        secretName,
        label: `iOS Test #44 direct Show cycle ${cycle.number}`,
      });
      void show.result.catch(() => {});
      await show.waitForMarker(
        `E2E: IOS_TEST44_LOCAL_SECRET_VALUE_VERIFIED_${cycle.number}`,
        180_000,
      );
      await show.waitForMarker(`E2E: IOS_TEST44_LOCAL_SECRET_VISIBLE_${cycle.number}`, 60_000);
      await show.waitForMarker(`E2E: IOS_TEST44_LOCAL_SECRET_CLOSED_${cycle.number}`, 60_000);
      await show.result;
    } else if (sender === 'web') {
      await showAndCloseWebSecret(page, secretName);
      console.log(`E2E: WEB_TEST44_LOCAL_SECRET_VALUE_VERIFIED_${cycle.number}`);
      console.log(`E2E: WEB_TEST44_LOCAL_SECRET_CLOSED_${cycle.number}`);
    } else {
      throw new Error(`Test #44 first direct-Show block has unsupported sender=${sender}`);
    }
    await assertNoStaleWebMembershipUi(page, allSecretNames);
    writeDiagnostic(
      `[TEST44][after-android-removal][cycle=${cycle.number}] sender=${sender} `
        + `secret=${secretName} directShow=true exactValue=verified closed=true`,
    );
  }

  await assertWebMembership(
    page,
    ['Web', 'iPhone'],
    allSecretNames,
    'Test #44 pre-two-to-one boundary',
    expectedDeviceNames,
  );
  console.log('E2E: WEB_TEST44_REMOVAL_TARGET_READY');
  const iosLocalRemoval = startIosAction({
    testMethod: scenario.ios.localRemovalTestMethod,
    role: 'removal-requester',
    cycle: 2,
    secretName: thirdName,
    sender: 'ios',
    expectedDeviceNames,
    label: 'iOS Test #44 removes Web locally',
  });
  void iosLocalRemoval.result.catch(() => {});
  await iosLocalRemoval.waitForMarker('E2E: IOS_TEST44_WEB_LOCAL_REMOVAL_READY', 240_000);
  approvalCoordinator.allow('ios-remove-web-local-test44', 2);
  await iosLocalRemoval.waitForMarker(
    'E2E: IOS_TEST44_WEB_LOCAL_REMOVAL_REQUESTED',
    180_000,
  );
  await Promise.all([
    assertWebResetAfterOwnRemoval(page, allSecretNames),
    iosLocalRemoval.waitForMarker(
      'E2E: IOS_TEST44_WEB_LOCAL_REMOVAL_COMPLETED',
      180_000,
    ),
  ]);
  await iosLocalRemoval.result;

  for (const cycle of finalBlock) {
    setCurrentCycleForDiagnostics(cycle);
    const secretName = cycle.senderSecrets.ios;
    const show = startIosAction({
      testMethod: scenario.ios.showSecretTestMethod,
      role: 'direct-show',
      cycle: cycle.number,
      secretName,
      label: `iOS Test #44 single-member Show cycle ${cycle.number}`,
    });
    void show.result.catch(() => {});
    await show.waitForMarker(
      `E2E: IOS_TEST44_LOCAL_SECRET_VALUE_VERIFIED_${cycle.number}`,
      180_000,
    );
    await show.waitForMarker(`E2E: IOS_TEST44_LOCAL_SECRET_VISIBLE_${cycle.number}`, 60_000);
    await show.waitForMarker(`E2E: IOS_TEST44_LOCAL_SECRET_CLOSED_${cycle.number}`, 60_000);
    await show.result;
    writeDiagnostic(
      `[TEST44][after-web-removal][cycle=${cycle.number}] sender=ios `
        + `secret=${secretName} directShow=true exactValue=verified closed=true`,
    );
  }
  return { completedCycles: recoveryCycles.length };
}

async function runIosSecondaryCreatorMemberRemovalLifecycle(page, simulatorUdid, androidSerial) {
  const allSecretNames = ['secret1', 'secret2', 'secret3'].map(secretNameFor);
  const [initialName, secondName, thirdName] = allSecretNames;
  const firstRemoval = scenario.membership?.firstRemoval;
  const secondRemoval = scenario.membership?.secondRemoval;
  if ((scenario.membership?.initialPlatforms ?? []).join(',') !== 'ios,web,android') {
    throw new Error('Test #42 requires the iOS, Web, Android three-member boundary');
  }
  if ((scenario.setup?.joinOrder ?? []).join(',') !== 'web,android') {
    throw new Error('Test #42 requires Web then Android Join order');
  }
  if (scenario.setup?.secondJoinApprover !== 'ios') {
    throw new Error('Test #42 requires iOS as the sole approver of both Joins');
  }
  if (
    (secretCreationPlan?.initial?.ios ?? []).join(',') !== initialName
    || (secretCreationPlan?.afterWebJoin?.web ?? []).join(',') !== secondName
    || (secretCreationPlan?.afterAndroidJoin?.web ?? []).join(',') !== thirdName
  ) {
    throw new Error('Test #42 requires iOS Secret1, then Web Secret2 and Secret3');
  }
  if (
    firstRemoval?.requester !== 'ios'
    || firstRemoval?.approver !== 'android'
    || firstRemoval?.target !== 'web'
    || firstRemoval?.expectedMemberCount !== 2
  ) {
    throw new Error('Test #42 first removal must be iOS -> Android approval -> Web target');
  }
  if (
    secondRemoval?.requester !== 'android'
    || secondRemoval?.approver !== null
    || secondRemoval?.target !== 'ios'
    || secondRemoval?.expectedMemberCount !== 1
  ) {
    throw new Error('Test #42 second removal must be Android local two-to-one removal of iOS');
  }
  if (recoveryCycles.length !== 9 || recoveryCycles.some((cycle) => (
    cycle.kind !== 'direct-show' || cycle.approvals.length !== 0 || cycle.senders.length !== 1
  ))) {
    throw new Error('Test #42 requires exactly nine single-sender direct-Show cycles and zero recovery');
  }
  const firstBlock = recoveryCycles.slice(0, 6);
  const finalBlock = recoveryCycles.slice(6);
  const actualShowMatrix = recoveryCycles.map((cycle) => (
    `${cycle.block}:${cycle.senders[0]}:${cycle.senderSecrets[cycle.senders[0]]}`
  ));
  const expectedShowMatrix = [
    ...allSecretNames.map((name) => `after-web-removal:ios:${name}`),
    ...allSecretNames.map((name) => `after-web-removal:android:${name}`),
    ...allSecretNames.map((name) => `after-ios-removal:android:${name}`),
  ];
  if (actualShowMatrix.join('|') !== expectedShowMatrix.join('|')) {
    throw new Error('Test #42 direct-Show matrix does not match the two removal boundaries');
  }

  const startIosAction = ({
    testMethod,
    role,
    cycle = 0,
    secretName,
    sender = '',
    expectedDeviceNames,
    label,
  }) => startIosJoinTest(simulatorUdid, {
    testClass: scenario.ios.testClass,
    testMethod,
    label,
    environment: stepEnvironment({ role, cycle, step: 1, sender, secretName }),
    secretName,
    secretValue: secretValueForName(secretName),
    expectedDeviceNames,
  });

  setJoinExpectedSecretNames([initialName]);
  const iosSetup = startIosAction({
    testMethod: scenario.ios.initiatorSetupTestMethod,
    role: 'creator',
    secretName: initialName,
    label: 'iOS Test #42 creates Vault and Secret1',
  });
  void iosSetup.result.catch(() => {});
  await iosSetup.waitForMarker('E2E: IOS_TEST42_SECRET1_READY', 300_000);
  await iosSetup.waitForMarker('E2E: IOS_TEST42_INITIATOR_SETUP_READY', 60_000);
  await iosSetup.result;
  writeDiagnostic('[TEST42] iOS Vault and Secret1 persisted');

  await submitWebJoinRequest(page);
  const approveWebJoin = startIosAction({
    testMethod: scenario.ios.approveJoinTestMethod,
    role: 'join-approver',
    secretName: initialName,
    sender: 'web',
    label: 'iOS Test #42 approves Web Join',
  });
  void approveWebJoin.result.catch(() => {});
  await approveWebJoin.waitForMarker('E2E: IOS_TEST42_JOIN_APPROVED_WEB', 180_000);
  await approveWebJoin.result;
  await page.reload({ waitUntil: 'domcontentloaded' });
  await unlockWithPasskeyIfNeeded(page);
  await waitForWebSecretNames(page, [initialName]);
  writeDiagnostic('[TEST42] Web joined iOS Vault and observed Secret1');

  await createWebSecret(page, secondName);
  await waitForWebSecretNames(page, [initialName, secondName]);
  console.log('E2E: WEB_TEST42_SECRET_CREATED_2');
  setJoinExpectedSecretNames([initialName, secondName]);
  writeDiagnostic('[TEST42] Web Secret2 persisted before Android Join');

  const androidJoin = await startAndroidJoinTest(androidSerial, {
    testClass: scenario.android.testClass,
    testMethod: scenario.android.joinTestMethod,
    expectedSecretNames: [initialName, secondName],
  });
  await androidJoin.waitForMarker('E2E: ANDROID_TEST42_JOIN_REQUEST_SENT', 180_000);
  const approveAndroidJoin = startIosAction({
    testMethod: scenario.ios.approveJoinTestMethod,
    role: 'join-approver',
    secretName: initialName,
    sender: 'android',
    label: 'iOS Test #42 approves Android Join',
  });
  void approveAndroidJoin.result.catch(() => {});
  await approveAndroidJoin.waitForMarker('E2E: IOS_TEST42_JOIN_APPROVED_ANDROID', 180_000);
  await androidJoin.waitForMarker('E2E: ANDROID_TEST42_JOIN_READY', 180_000);
  await androidJoin.waitForMarker('E2E: ANDROID_TEST42_SECRETS_READY', 60_000);
  await Promise.all([approveAndroidJoin.result, androidJoin.result]);
  await waitForWebSecretNames(page, [initialName, secondName]);
  writeDiagnostic('[TEST42] Android joined iOS Vault and observed Secret1/Secret2');

  await createWebSecret(page, thirdName);
  await waitForWebSecretNames(page, allSecretNames);
  console.log('E2E: WEB_TEST42_SECRET_CREATED_3');
  setJoinExpectedSecretNames(allSecretNames);

  const expectedDeviceNames = await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #42 three-device boundary',
  );
  writeDiagnostic(
    '[TEST42] canonical registered device identities discovered from exact Web rows '
      + `web=${JSON.stringify(expectedDeviceNames.web)} `
      + `android=${JSON.stringify(expectedDeviceNames.android)} `
      + `ios=${JSON.stringify(expectedDeviceNames.ios)}`,
  );

  const androidRemovalApproval = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.lifecycleTestClass,
    testMethod: scenario.android.approveRemovalTestMethod,
    role: 'removal-approver',
    cycle: 1,
    step: 1,
    joinTarget: expectedDeviceNames.web,
    expectedSecretNames: allSecretNames,
    expectedDeviceNames,
    label: 'Android Test #42 approves Web removal',
  });
  const iosRemovalRequester = startIosAction({
    testMethod: scenario.ios.requestRemovalTestMethod,
    role: 'removal-requester',
    cycle: 1,
    secretName: thirdName,
    sender: 'ios',
    expectedDeviceNames,
    label: 'iOS Test #42 requests Web removal',
  });
  void iosRemovalRequester.result.catch(() => {});
  await Promise.all([
    androidRemovalApproval.waitForMarker('E2E: ANDROID_TEST42_REMOVAL_RECEIVER_READY', 180_000),
    iosRemovalRequester.waitForMarker('E2E: IOS_TEST42_WEB_REMOVAL_REQUESTER_READY', 240_000),
  ]);
  await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #42 Web passive-target boundary',
    expectedDeviceNames,
  );
  console.log('E2E: WEB_TEST42_REMOVAL_TARGET_READY');

  approvalCoordinator.allow('ios-remove-web-start-test42', 1);
  await iosRemovalRequester.waitForMarker('E2E: IOS_TEST42_WEB_REMOVAL_REQUESTED', 180_000);
  await androidRemovalApproval.waitForMarker('E2E: ANDROID_TEST42_REMOVAL_REQUEST_VISIBLE', 180_000);
  const webReset = assertWebResetAfterOwnRemoval(page, allSecretNames);
  approvalCoordinator.allow('android-remove-web-approve-test42', 1);
  await Promise.all([
    androidRemovalApproval.waitForMarker('E2E: ANDROID_TEST42_WEB_REMOVAL_APPROVED', 180_000),
    iosRemovalRequester.waitForMarker('E2E: IOS_TEST42_WEB_REMOVAL_COMPLETED', 180_000),
    webReset,
  ]);
  await Promise.all([androidRemovalApproval.result, iosRemovalRequester.result]);

  for (const cycle of firstBlock) {
    setCurrentCycleForDiagnostics(cycle);
    const sender = cycle.senders[0];
    const secretName = cycle.senderSecrets[sender];
    if (sender === 'ios') {
      const show = startIosAction({
        testMethod: scenario.ios.showSecretTestMethod,
        role: 'direct-show',
        cycle: cycle.number,
        secretName,
        label: `iOS Test #42 direct Show cycle ${cycle.number}`,
      });
      void show.result.catch(() => {});
      await show.waitForMarker(
        `E2E: IOS_TEST42_LOCAL_SECRET_VALUE_VERIFIED_${cycle.number}`,
        180_000,
      );
      await show.waitForMarker(`E2E: IOS_TEST42_LOCAL_SECRET_VISIBLE_${cycle.number}`, 60_000);
      await show.waitForMarker(`E2E: IOS_TEST42_LOCAL_SECRET_CLOSED_${cycle.number}`, 60_000);
      await show.result;
    } else if (sender === 'android') {
      const show = await startAndroidStepTest(androidSerial, {
        testClass: scenario.android.lifecycleTestClass,
        testMethod: scenario.android.showSecretTestMethod,
        role: 'direct-show',
        cycle: cycle.number,
        step: 1,
        secretName,
        secretValue: secretValueForName(secretName),
        expectedSecretNames: allSecretNames,
        label: `Android Test #42 direct Show cycle ${cycle.number}`,
      });
      await show.waitForMarker(
        `E2E: ANDROID_TEST42_LOCAL_SECRET_VALUE_VERIFIED_${cycle.number}`,
        180_000,
      );
      await show.waitForMarker(`E2E: ANDROID_TEST42_LOCAL_SECRET_CLOSED_${cycle.number}`, 60_000);
      await show.result;
    } else {
      throw new Error(`Test #42 first direct-Show block has unsupported sender=${sender}`);
    }
    writeDiagnostic(
      `[TEST42][after-web-removal][cycle=${cycle.number}] sender=${sender} `
        + `secret=${secretName} directShow=true exactValue=verified closed=true`,
    );
  }

  const iosRemovalTarget = startIosAction({
    testMethod: scenario.ios.removalObserverTestMethod,
    role: 'removal-target',
    cycle: 2,
    secretName: thirdName,
    sender: 'android',
    expectedDeviceNames,
    label: 'iOS Test #42 observes own local removal',
  });
  void iosRemovalTarget.result.catch(() => {});
  const androidLocalRemoval = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.lifecycleTestClass,
    testMethod: scenario.android.localRemovalTestMethod,
    role: 'removal-requester',
    cycle: 2,
    step: 1,
    joinTarget: expectedDeviceNames.ios,
    expectedSecretNames: allSecretNames,
    expectedDeviceNames,
    label: 'Android Test #42 removes iOS locally',
  });
  await Promise.all([
    iosRemovalTarget.waitForMarker('E2E: IOS_TEST42_REMOVAL_TARGET_READY', 240_000),
    androidLocalRemoval.waitForMarker('E2E: ANDROID_TEST42_IOS_LOCAL_REMOVAL_READY', 180_000),
  ]);
  approvalCoordinator.allow('android-remove-ios-local-test42', 2);
  await androidLocalRemoval.waitForMarker(
    'E2E: ANDROID_TEST42_IOS_LOCAL_REMOVAL_REQUESTED',
    180_000,
  );
  await Promise.all([
    iosRemovalTarget.waitForMarker('E2E: IOS_TEST42_REMOVED_RESET', 180_000),
    androidLocalRemoval.waitForMarker(
      'E2E: ANDROID_TEST42_IOS_LOCAL_REMOVAL_COMPLETED',
      180_000,
    ),
  ]);
  await Promise.all([iosRemovalTarget.result, androidLocalRemoval.result]);

  for (const cycle of finalBlock) {
    setCurrentCycleForDiagnostics(cycle);
    const secretName = cycle.senderSecrets.android;
    const show = await startAndroidStepTest(androidSerial, {
      testClass: scenario.android.lifecycleTestClass,
      testMethod: scenario.android.showSecretTestMethod,
      role: 'direct-show',
      cycle: cycle.number,
      step: 1,
      secretName,
      secretValue: secretValueForName(secretName),
      expectedSecretNames: allSecretNames,
      label: `Android Test #42 single-member Show cycle ${cycle.number}`,
    });
    await show.waitForMarker(
      `E2E: ANDROID_TEST42_LOCAL_SECRET_VALUE_VERIFIED_${cycle.number}`,
      180_000,
    );
    await show.waitForMarker(`E2E: ANDROID_TEST42_LOCAL_SECRET_CLOSED_${cycle.number}`, 60_000);
    await show.result;
    writeDiagnostic(
      `[TEST42][after-ios-removal][cycle=${cycle.number}] sender=android `
        + `secret=${secretName} directShow=true exactValue=verified closed=true`,
    );
  }
  return { completedCycles: recoveryCycles.length };
}

async function runIosDistributedCreatorMemberRemovalLifecycle(page, simulatorUdid, androidSerial) {
  const allSecretNames = ['secret1', 'secret2', 'secret3'].map(secretNameFor);
  const [initialName, secondName, thirdName] = allSecretNames;
  const firstRemoval = scenario.membership?.firstRemoval;
  const secondRemoval = scenario.membership?.secondRemoval;
  if ((scenario.membership?.initialPlatforms ?? []).join(',') !== 'ios,web,android') {
    throw new Error('Test #45 requires the iOS, Web, Android three-member boundary');
  }
  if ((scenario.setup?.joinOrder ?? []).join(',') !== 'web,android') {
    throw new Error('Test #45 requires Web then Android Join order');
  }
  if (scenario.setup?.secondJoinApprover !== 'ios') {
    throw new Error('Test #45 requires iOS as the sole approver of both Joins');
  }
  if (
    (secretCreationPlan?.initial?.ios ?? []).join(',') !== initialName
    || (secretCreationPlan?.afterWebJoin?.web ?? []).join(',') !== secondName
    || (secretCreationPlan?.afterAndroidJoin?.android ?? []).join(',') !== thirdName
  ) {
    throw new Error('Test #45 requires iOS Secret1, Web Secret2, and Android Secret3');
  }
  if (
    firstRemoval?.requester !== 'android'
    || firstRemoval?.approver !== 'web'
    || firstRemoval?.target !== 'ios'
    || firstRemoval?.expectedMemberCount !== 2
  ) {
    throw new Error('Test #45 first removal must be Android -> Web approval -> iOS target');
  }
  if (
    secondRemoval?.requester !== 'web'
    || secondRemoval?.approver !== null
    || secondRemoval?.target !== 'android'
    || secondRemoval?.expectedMemberCount !== 1
  ) {
    throw new Error('Test #45 second removal must be Web local two-to-one removal of Android');
  }
  if (recoveryCycles.length !== 9 || recoveryCycles.some((cycle) => (
    cycle.kind !== 'direct-show' || cycle.approvals.length !== 0 || cycle.senders.length !== 1
  ))) {
    throw new Error('Test #45 requires exactly nine single-sender direct-Show cycles and zero recovery');
  }
  const firstBlock = recoveryCycles.slice(0, 6);
  const finalBlock = recoveryCycles.slice(6);
  const actualShowMatrix = recoveryCycles.map((cycle) => (
    `${cycle.block}:${cycle.senders[0]}:${cycle.senderSecrets[cycle.senders[0]]}`
  ));
  const expectedShowMatrix = [
    ...allSecretNames.map((name) => `after-ios-removal:android:${name}`),
    ...allSecretNames.map((name) => `after-ios-removal:web:${name}`),
    ...allSecretNames.map((name) => `after-android-removal:web:${name}`),
  ];
  if (actualShowMatrix.join('|') !== expectedShowMatrix.join('|')) {
    throw new Error('Test #45 direct-Show matrix does not match the two removal boundaries');
  }

  const startIosAction = ({
    testMethod,
    role,
    cycle = 0,
    secretName,
    sender = '',
    expectedDeviceNames,
    label,
  }) => startIosJoinTest(simulatorUdid, {
    testClass: scenario.ios.testClass,
    testMethod,
    label,
    environment: stepEnvironment({ role, cycle, step: 1, sender, secretName }),
    secretName,
    secretValue: secretValueForName(secretName),
    expectedDeviceNames,
  });

  setJoinExpectedSecretNames([initialName]);
  const iosSetup = startIosAction({
    testMethod: scenario.ios.initiatorSetupTestMethod,
    role: 'creator',
    secretName: initialName,
    label: 'iOS Test #45 creates Vault and Secret1',
  });
  void iosSetup.result.catch(() => {});
  await iosSetup.waitForMarker('E2E: IOS_TEST45_SECRET1_READY', 300_000);
  await iosSetup.waitForMarker('E2E: IOS_TEST45_INITIATOR_SETUP_READY', 60_000);
  await iosSetup.result;
  writeDiagnostic('[TEST45] iOS Vault and Secret1 persisted');

  await submitWebJoinRequest(page);
  const approveWebJoin = startIosAction({
    testMethod: scenario.ios.approveJoinTestMethod,
    role: 'join-approver',
    secretName: initialName,
    sender: 'web',
    label: 'iOS Test #45 approves Web Join',
  });
  void approveWebJoin.result.catch(() => {});
  await approveWebJoin.waitForMarker('E2E: IOS_TEST45_JOIN_APPROVED_WEB', 180_000);
  await approveWebJoin.result;
  await page.reload({ waitUntil: 'domcontentloaded' });
  await unlockWithPasskeyIfNeeded(page);
  await waitForWebSecretNames(page, [initialName]);
  writeDiagnostic('[TEST45] Web joined iOS Vault and observed Secret1');

  await createWebSecret(page, secondName);
  await waitForWebSecretNames(page, [initialName, secondName]);
  console.log('E2E: WEB_TEST45_SECRET_CREATED_2');
  setJoinExpectedSecretNames([initialName, secondName]);
  writeDiagnostic('[TEST45] Web Secret2 persisted before Android Join');

  const androidJoin = await startAndroidJoinTest(androidSerial, {
    testClass: scenario.android.testClass,
    testMethod: scenario.android.joinTestMethod,
    expectedSecretNames: [initialName, secondName],
  });
  await androidJoin.waitForMarker('E2E: ANDROID_TEST45_JOIN_REQUEST_SENT', 180_000);
  const approveAndroidJoin = startIosAction({
    testMethod: scenario.ios.approveJoinTestMethod,
    role: 'join-approver',
    secretName: initialName,
    sender: 'android',
    label: 'iOS Test #45 approves Android Join',
  });
  void approveAndroidJoin.result.catch(() => {});
  await approveAndroidJoin.waitForMarker('E2E: IOS_TEST45_JOIN_APPROVED_ANDROID', 180_000);
  await androidJoin.waitForMarker('E2E: ANDROID_TEST45_JOIN_READY', 180_000);
  await androidJoin.waitForMarker('E2E: ANDROID_TEST45_SECRETS_READY', 60_000);
  await Promise.all([approveAndroidJoin.result, androidJoin.result]);
  await waitForWebSecretNames(page, [initialName, secondName]);
  writeDiagnostic('[TEST45] Android joined iOS Vault and observed Secret1/Secret2');

  const createThird = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.creatorTestClass,
    testMethod: scenario.android.createSecretTestMethod,
    role: 'creator',
    cycle: 3,
    step: 1,
    secretName: thirdName,
    secretValue: secretValueForName(thirdName),
    expectedSecretNames: allSecretNames,
    label: 'Android Test #45 creates Secret3',
  });
  await createThird.waitForMarker('E2E: ANDROID_TEST45_SECRET_CREATED_3', 180_000);
  await createThird.result;
  setJoinExpectedSecretNames(allSecretNames);
  await waitForWebSecretNames(page, allSecretNames);

  const expectedDeviceNames = await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #45 three-device boundary',
  );
  writeDiagnostic(
    '[TEST45] canonical registered device identities discovered from exact Web rows '
      + `web=${JSON.stringify(expectedDeviceNames.web)} `
      + `android=${JSON.stringify(expectedDeviceNames.android)} `
      + `ios=${JSON.stringify(expectedDeviceNames.ios)}`,
  );

  const iosRemovalTarget = startIosAction({
    testMethod: scenario.ios.removalObserverTestMethod,
    role: 'removal-target',
    cycle: 1,
    secretName: thirdName,
    sender: 'android',
    expectedDeviceNames,
    label: 'iOS Test #45 observes own removal',
  });
  void iosRemovalTarget.result.catch(() => {});
  const androidRemovalRequester = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.lifecycleTestClass,
    testMethod: scenario.android.requestRemovalTestMethod,
    role: 'removal-requester',
    cycle: 1,
    step: 1,
    joinTarget: expectedDeviceNames.ios,
    expectedSecretNames: allSecretNames,
    expectedDeviceNames,
    label: 'Android Test #45 requests iOS removal',
  });
  await Promise.all([
    iosRemovalTarget.waitForMarker('E2E: IOS_TEST45_REMOVAL_TARGET_READY', 240_000),
    androidRemovalRequester.waitForMarker(
      'E2E: ANDROID_TEST45_IOS_REMOVAL_REQUESTER_READY',
      180_000,
    ),
  ]);
  await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #45 Web removal receiver boundary',
    expectedDeviceNames,
  );
  console.log('E2E: WEB_TEST45_REMOVAL_RECEIVER_READY');

  approvalCoordinator.allow('android-remove-ios-start-test45', 1);
  await androidRemovalRequester.waitForMarker(
    'E2E: ANDROID_TEST45_IOS_REMOVAL_REQUESTED',
    180_000,
  );
  await page.reload({ waitUntil: 'domcontentloaded' });
  await unlockWithPasskeyIfNeeded(page);
  await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #45 persisted iOS removal request visible to Web',
    expectedDeviceNames,
  );
  const webApproval = initiateWebDeviceRemoval(page, {
    targetCategory: 'iPhone',
    expectedTargetDeviceName: expectedDeviceNames.ios,
    waitForTargetGone: true,
    label: 'Test #45 Web approval of iOS removal',
    requestVisibleMarker: 'WEB_TEST45_REMOVAL_REQUEST_VISIBLE',
    completionMarker: 'WEB_TEST45_IOS_REMOVAL_APPROVED',
  });
  await Promise.all([
    webApproval,
    androidRemovalRequester.waitForMarker(
      'E2E: ANDROID_TEST45_IOS_REMOVAL_COMPLETED',
      180_000,
    ),
    iosRemovalTarget.waitForMarker('E2E: IOS_TEST45_REMOVED_RESET', 180_000),
  ]);
  await Promise.all([androidRemovalRequester.result, iosRemovalTarget.result]);
  await assertWebMembership(
    page,
    ['Web', 'Android'],
    allSecretNames,
    'Test #45 two-device reshared boundary',
    expectedDeviceNames,
  );

  for (const cycle of firstBlock) {
    setCurrentCycleForDiagnostics(cycle);
    const sender = cycle.senders[0];
    const secretName = cycle.senderSecrets[sender];
    if (sender === 'android') {
      const show = await startAndroidStepTest(androidSerial, {
        testClass: scenario.android.lifecycleTestClass,
        testMethod: scenario.android.showSecretTestMethod,
        role: 'direct-show',
        cycle: cycle.number,
        step: 1,
        secretName,
        secretValue: secretValueForName(secretName),
        expectedSecretNames: allSecretNames,
        label: `Android Test #45 direct Show cycle ${cycle.number}`,
      });
      await show.waitForMarker(
        `E2E: ANDROID_TEST45_LOCAL_SECRET_VALUE_VERIFIED_${cycle.number}`,
        180_000,
      );
      await show.waitForMarker(`E2E: ANDROID_TEST45_LOCAL_SECRET_CLOSED_${cycle.number}`, 60_000);
      await show.result;
    } else if (sender === 'web') {
      await showAndCloseWebSecret(page, secretName);
      console.log(`E2E: WEB_TEST45_LOCAL_SECRET_VALUE_VERIFIED_${cycle.number}`);
      console.log(`E2E: WEB_TEST45_LOCAL_SECRET_CLOSED_${cycle.number}`);
    } else {
      throw new Error(`Test #45 first direct-Show block has unsupported sender=${sender}`);
    }
    await assertNoStaleWebMembershipUi(page, allSecretNames);
    writeDiagnostic(
      `[TEST45][after-ios-removal][cycle=${cycle.number}] sender=${sender} `
        + `secret=${secretName} directShow=true exactValue=verified closed=true`,
    );
  }

  const androidRemovalTarget = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.lifecycleTestClass,
    testMethod: scenario.android.removalObserverTestMethod,
    role: 'removal-target',
    cycle: 2,
    step: 1,
    expectedSecretNames: allSecretNames,
    expectedDeviceNames,
    label: 'Android Test #45 observes own local removal',
  });
  await androidRemovalTarget.waitForMarker('E2E: ANDROID_TEST45_REMOVAL_TARGET_READY', 180_000);
  await assertWebMembership(
    page,
    ['Web', 'Android'],
    allSecretNames,
    'Test #45 pre-two-to-one boundary',
    expectedDeviceNames,
  );
  console.log('E2E: WEB_TEST45_ANDROID_LOCAL_REMOVAL_READY');
  await Promise.all([
    initiateWebDeviceRemoval(page, {
      targetCategory: 'Android',
      expectedTargetDeviceName: expectedDeviceNames.android,
      waitForTargetGone: true,
      label: 'Test #45 Web local removal of Android',
      requestVisibleMarker: 'WEB_TEST45_ANDROID_LOCAL_REMOVAL_REQUEST_VISIBLE',
      completionMarker: 'WEB_TEST45_ANDROID_LOCAL_REMOVAL_COMPLETED',
    }),
    androidRemovalTarget.waitForMarker('E2E: ANDROID_TEST45_REMOVED_RESET', 180_000),
  ]);
  await androidRemovalTarget.result;
  await assertWebMembership(
    page,
    ['Web'],
    allSecretNames,
    'Test #45 single-device boundary',
    expectedDeviceNames,
  );

  for (const cycle of finalBlock) {
    setCurrentCycleForDiagnostics(cycle);
    const secretName = cycle.senderSecrets.web;
    await showAndCloseWebSecret(page, secretName);
    console.log(`E2E: WEB_TEST45_LOCAL_SECRET_VALUE_VERIFIED_${cycle.number}`);
    console.log(`E2E: WEB_TEST45_LOCAL_SECRET_CLOSED_${cycle.number}`);
    await assertNoStaleWebMembershipUi(page, allSecretNames);
    writeDiagnostic(
      `[TEST45][after-android-removal][cycle=${cycle.number}] sender=web `
        + `secret=${secretName} directShow=true exactValue=verified closed=true`,
    );
  }
  return { completedCycles: recoveryCycles.length };
}

async function runIosMemberRemovalLifecycle(page, simulatorUdid, androidSerial) {
  const allSecretNames = ['secret1', 'secret2', 'secret3'].map(secretNameFor);
  const [initialName, secondName, thirdName] = allSecretNames;
  const firstRemoval = scenario.membership?.firstRemoval;
  const secondRemoval = scenario.membership?.secondRemoval;
  if ((scenario.membership?.initialPlatforms ?? []).join(',') !== 'ios,web,android') {
    throw new Error('Test #39 requires the iOS, Web, Android three-member boundary');
  }
  if ((scenario.setup?.joinOrder ?? []).join(',') !== 'web,android') {
    throw new Error('Test #39 requires Web then Android Join order');
  }
  if (scenario.setup?.secondJoinApprover !== 'ios') {
    throw new Error('Test #39 requires iOS as the sole approver of both Joins');
  }
  if (
    firstRemoval?.requester !== 'ios'
    || firstRemoval?.approver !== 'web'
    || firstRemoval?.target !== 'android'
    || firstRemoval?.expectedMemberCount !== 2
  ) {
    throw new Error('Test #39 first removal must be iOS -> Web approval -> Android target');
  }
  if (
    secondRemoval?.requester !== 'ios'
    || secondRemoval?.approver !== null
    || secondRemoval?.target !== 'web'
    || secondRemoval?.expectedMemberCount !== 1
  ) {
    throw new Error('Test #39 second removal must be iOS local two-to-one removal of Web');
  }
  if (recoveryCycles.length !== 9 || recoveryCycles.some((cycle) => (
    cycle.kind !== 'direct-show' || cycle.approvals.length !== 0 || cycle.senders.length !== 1
  ))) {
    throw new Error('Test #39 requires exactly nine single-sender direct-Show cycles and zero recovery');
  }
  const firstBlock = recoveryCycles.slice(0, 6);
  const finalBlock = recoveryCycles.slice(6);
  const actualShowMatrix = recoveryCycles.map((cycle) => (
    `${cycle.block}:${cycle.senders[0]}:${cycle.senderSecrets[cycle.senders[0]]}`
  ));
  const expectedShowMatrix = [
    ...allSecretNames.map((name) => `after-android-removal:ios:${name}`),
    ...allSecretNames.map((name) => `after-android-removal:web:${name}`),
    ...allSecretNames.map((name) => `after-web-removal:ios:${name}`),
  ];
  if (actualShowMatrix.join('|') !== expectedShowMatrix.join('|')) {
    throw new Error('Test #39 direct-Show matrix does not match the two removal boundaries');
  }

  const startIosAction = ({
    testMethod,
    role,
    cycle = 0,
    secretName,
    sender = '',
    expectedDeviceNames,
    label,
  }) => startIosJoinTest(simulatorUdid, {
    testClass: scenario.ios.testClass,
    testMethod,
    label,
    environment: stepEnvironment({ role, cycle, step: 1, sender }),
    secretName,
    secretValue: secretValueForName(secretName),
    expectedDeviceNames,
  });

  setJoinExpectedSecretNames([initialName]);
  const iosSetup = startIosAction({
    testMethod: scenario.ios.initiatorSetupTestMethod,
    role: 'creator',
    secretName: initialName,
    label: 'iOS Test #39 creates Vault and Secret1',
  });
  await iosSetup.waitForMarker('E2E: IOS_TEST39_SECRET1_READY', 300_000);
  await iosSetup.waitForMarker('E2E: IOS_TEST39_INITIATOR_SETUP_READY', 60_000);
  await iosSetup.result;
  writeDiagnostic('[TEST39] iOS Vault and Secret1 persisted');

  await submitWebJoinRequest(page);
  const approveWebJoin = startIosAction({
    testMethod: scenario.ios.approveJoinTestMethod,
    role: 'join-approver',
    secretName: initialName,
    sender: 'web',
    label: 'iOS Test #39 approves Web Join',
  });
  await approveWebJoin.waitForMarker('E2E: IOS_TEST39_JOIN_APPROVED_WEB', 180_000);
  await approveWebJoin.result;
  await page.reload({ waitUntil: 'domcontentloaded' });
  await unlockWithPasskeyIfNeeded(page);
  await waitForWebSecretNames(page, [initialName]);
  writeDiagnostic('[TEST39] Web joined iOS Vault and observed Secret1');

  const createSecond = startIosAction({
    testMethod: scenario.ios.createSecretTestMethod,
    role: 'creator',
    cycle: 2,
    secretName: secondName,
    label: 'iOS Test #39 creates Secret2',
  });
  await createSecond.waitForMarker('E2E: IOS_TEST39_SECRET_CREATED_2', 180_000);
  await createSecond.result;
  setJoinExpectedSecretNames([initialName, secondName]);
  await waitForWebSecretNames(page, [initialName, secondName]);
  writeDiagnostic('[TEST39] iOS Secret2 persisted before Android Join');

  const androidJoin = await startAndroidJoinTest(androidSerial, {
    testClass: scenario.android.testClass,
    testMethod: scenario.android.joinTestMethod,
    expectedSecretNames: [initialName, secondName],
  });
  await androidJoin.waitForMarker('E2E: ANDROID_TEST39_JOIN_REQUEST_SENT', 180_000);
  const approveAndroidJoin = startIosAction({
    testMethod: scenario.ios.approveJoinTestMethod,
    role: 'join-approver',
    secretName: initialName,
    sender: 'android',
    label: 'iOS Test #39 approves Android Join',
  });
  await approveAndroidJoin.waitForMarker('E2E: IOS_TEST39_JOIN_APPROVED_ANDROID', 180_000);
  await androidJoin.waitForMarker('E2E: ANDROID_TEST39_JOIN_READY', 180_000);
  await androidJoin.waitForMarker('E2E: ANDROID_TEST39_SECRETS_READY', 60_000);
  await Promise.all([approveAndroidJoin.result, androidJoin.result]);
  await waitForWebSecretNames(page, [initialName, secondName]);
  writeDiagnostic('[TEST39] Android joined iOS Vault and observed Secret1/Secret2');

  const createThird = startIosAction({
    testMethod: scenario.ios.createSecretTestMethod,
    role: 'creator',
    cycle: 3,
    secretName: thirdName,
    label: 'iOS Test #39 creates Secret3',
  });
  await createThird.waitForMarker('E2E: IOS_TEST39_SECRET_CREATED_3', 180_000);
  await createThird.result;
  setJoinExpectedSecretNames(allSecretNames);
  await waitForWebSecretNames(page, allSecretNames);

  const expectedDeviceNames = await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #39 three-device boundary',
  );
  writeDiagnostic(
    '[TEST39] canonical registered device identities discovered from exact Web rows '
      + `web=${JSON.stringify(expectedDeviceNames.web)} `
      + `android=${JSON.stringify(expectedDeviceNames.android)} `
      + `ios=${JSON.stringify(expectedDeviceNames.ios)}`,
  );

  const androidRemovalTarget = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.lifecycleTestClass,
    testMethod: scenario.android.removalObserverTestMethod,
    role: 'removal-target',
    cycle: 1,
    step: 1,
    expectedSecretNames: allSecretNames,
    expectedDeviceNames,
    label: 'Android Test #39 observes own removal',
  });
  const iosRemovalRequester = startIosAction({
    testMethod: scenario.ios.requestRemovalTestMethod,
    role: 'removal-requester',
    cycle: 1,
    secretName: thirdName,
    sender: 'ios',
    expectedDeviceNames,
    label: 'iOS Test #39 requests Android removal',
  });
  void iosRemovalRequester.result.catch(() => {});
  await Promise.all([
    androidRemovalTarget.waitForMarker('E2E: ANDROID_TEST39_REMOVAL_TARGET_READY', 180_000),
    iosRemovalRequester.waitForMarker('E2E: IOS_TEST39_ANDROID_REMOVAL_REQUESTER_READY', 240_000),
  ]);
  await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #39 Web removal receiver boundary',
    expectedDeviceNames,
  );
  console.log('E2E: WEB_TEST39_REMOVAL_RECEIVER_READY');

  approvalCoordinator.allow('ios-remove-android-start', 1);
  await iosRemovalRequester.waitForMarker(
    'E2E: IOS_TEST39_ANDROID_REMOVAL_REQUESTED',
    180_000,
  );
  // The iOS completion marker proves the request was persisted. Reload Web
  // once at that completed boundary so its next visible action is derived
  // from the current pending-removal Core state, never a stale ordinary
  // remove row that could accidentally create a competing request.
  await page.reload({ waitUntil: 'domcontentloaded' });
  await unlockWithPasskeyIfNeeded(page);
  await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    allSecretNames,
    'Test #39 persisted removal request visible to Web',
    expectedDeviceNames,
  );
  const webApproval = initiateWebDeviceRemoval(page, {
    targetCategory: 'Android',
    expectedTargetDeviceName: expectedDeviceNames.android,
    waitForTargetGone: true,
    label: 'Test #39 Web approval of Android removal',
    requestVisibleMarker: 'WEB_TEST39_REMOVAL_REQUEST_VISIBLE',
    completionMarker: 'WEB_TEST39_ANDROID_REMOVAL_APPROVED',
  });
  await Promise.all([
    webApproval,
    iosRemovalRequester.waitForMarker(
      'E2E: IOS_TEST39_ANDROID_REMOVAL_COMPLETED',
      180_000,
    ),
    androidRemovalTarget.waitForMarker('E2E: ANDROID_TEST39_REMOVED_RESET', 180_000),
  ]);
  await Promise.all([iosRemovalRequester.result, androidRemovalTarget.result]);
  await assertWebMembership(
    page,
    ['Web', 'iPhone'],
    allSecretNames,
    'Test #39 two-device reshared boundary',
    expectedDeviceNames,
  );

  for (const cycle of firstBlock) {
    setCurrentCycleForDiagnostics(cycle);
    const sender = cycle.senders[0];
    const secretName = cycle.senderSecrets[sender];
    if (sender === 'ios') {
      const show = startIosAction({
        testMethod: scenario.ios.showSecretTestMethod,
        role: 'direct-show',
        cycle: cycle.number,
        secretName,
        label: `iOS Test #39 direct Show cycle ${cycle.number}`,
      });
      void show.result.catch(() => {});
      await show.waitForMarker(
        `E2E: IOS_TEST39_LOCAL_SECRET_VALUE_VERIFIED_${cycle.number}`,
        180_000,
      );
      await show.waitForMarker(`E2E: IOS_TEST39_LOCAL_SECRET_VISIBLE_${cycle.number}`, 60_000);
      await show.waitForMarker(`E2E: IOS_TEST39_LOCAL_SECRET_CLOSED_${cycle.number}`, 60_000);
      await show.result;
    } else if (sender === 'web') {
      await showAndCloseWebSecret(page, secretName);
      console.log(`E2E: WEB_TEST39_LOCAL_SECRET_VALUE_VERIFIED_${cycle.number}`);
      console.log(`E2E: WEB_TEST39_LOCAL_SECRET_CLOSED_${cycle.number}`);
    } else {
      throw new Error(`Test #39 first direct-Show block has unsupported sender=${sender}`);
    }
    await assertNoStaleWebMembershipUi(page, allSecretNames);
    writeDiagnostic(
      `[TEST39][after-android-removal][cycle=${cycle.number}] sender=${sender} `
        + `secret=${secretName} directShow=true exactValue=verified closed=true`,
    );
  }

  await assertWebMembership(
    page,
    ['Web', 'iPhone'],
    allSecretNames,
    'Test #39 pre-two-to-one boundary',
    expectedDeviceNames,
  );
  console.log('E2E: WEB_TEST39_REMOVAL_TARGET_READY');
  const iosLocalRemoval = startIosAction({
    testMethod: scenario.ios.localRemovalTestMethod,
    role: 'removal-requester',
    cycle: 2,
    secretName: thirdName,
    sender: 'ios',
    expectedDeviceNames,
    label: 'iOS Test #39 removes Web locally',
  });
  void iosLocalRemoval.result.catch(() => {});
  await iosLocalRemoval.waitForMarker('E2E: IOS_TEST39_WEB_LOCAL_REMOVAL_READY', 240_000);
  approvalCoordinator.allow('ios-remove-web-local', 2);
  await iosLocalRemoval.waitForMarker(
    'E2E: IOS_TEST39_WEB_LOCAL_REMOVAL_REQUESTED',
    180_000,
  );
  await Promise.all([
    assertWebResetAfterOwnRemoval(page, allSecretNames),
    iosLocalRemoval.waitForMarker(
      'E2E: IOS_TEST39_WEB_LOCAL_REMOVAL_COMPLETED',
      180_000,
    ),
  ]);
  await iosLocalRemoval.result;

  for (const cycle of finalBlock) {
    setCurrentCycleForDiagnostics(cycle);
    const secretName = cycle.senderSecrets.ios;
    const show = startIosAction({
      testMethod: scenario.ios.showSecretTestMethod,
      role: 'direct-show',
      cycle: cycle.number,
      secretName,
      label: `iOS Test #39 single-member Show cycle ${cycle.number}`,
    });
    void show.result.catch(() => {});
    await show.waitForMarker(
      `E2E: IOS_TEST39_LOCAL_SECRET_VALUE_VERIFIED_${cycle.number}`,
      180_000,
    );
    await show.waitForMarker(`E2E: IOS_TEST39_LOCAL_SECRET_VISIBLE_${cycle.number}`, 60_000);
    await show.waitForMarker(`E2E: IOS_TEST39_LOCAL_SECRET_CLOSED_${cycle.number}`, 60_000);
    await show.result;
    writeDiagnostic(
      `[TEST39][after-web-removal][cycle=${cycle.number}] sender=ios `
        + `secret=${secretName} directShow=true exactValue=verified closed=true`,
    );
  }
  return { completedCycles: recoveryCycles.length };
}

async function runWebMultiSecretLifecycle(page, simulatorUdid, androidSerial) {
  const setup = await runWebMultiSecretLifecycleSetup(page, simulatorUdid, androidSerial);
  let androidDirectShowTest = setup.androidTest;
  const expectedCycles = scenario.recovery.expectedCycles ?? recoveryCycles.length;
  if (recoveryCycles.length !== expectedCycles) {
    throw new Error(`Test #22 expected ${expectedCycles} recovery cycles, got ${recoveryCycles.length}`);
  }
  const expectedRecoveryCycles = scenario.recovery.expectedRecoveryCycles
    ?? recoveryCycles.filter((cycle) => cycle.kind !== 'direct-show').length;
  const actualRecoveryCycles = recoveryCycles.filter((cycle) => cycle.kind !== 'direct-show').length;
  if (actualRecoveryCycles !== expectedRecoveryCycles) {
    throw new Error(`Test #22 expected ${expectedRecoveryCycles} recovery cycles, got ${actualRecoveryCycles}`);
  }
  const stepTestMethod = 'handleRecoveryStep';
  const startNativeStep = async (platform, role, cycle, secretName, approvalPlatform, sender) => {
    const environment = stepEnvironment({
      role,
      cycle: cycle.number,
      step: 1,
      approvalPlatform,
      sender,
      secretName,
    });
    // Test22 switches secrets mid-run, so its per-cycle native selector must
    // follow the cycle's sender/approval secret instead of the initial vault secret.
    environment.E2E_SECRET_NAME = secretName;
    if (platform === 'ios') {
      const test = startIosJoinTest(simulatorUdid, {
        testClass: scenario.ios.stepTestClass
          ?? scenario.ios.testClass
          ?? 'CaseEightIosBothOfflineUITest',
        testMethod: scenario.ios.stepTestMethod ?? stepTestMethod,
        label: `iOS Test #22 ${role} cycle ${cycle.number}`,
        environment,
        secretName,
        secretValue: secretValueForName(secretName),
      });
      void test.result.catch(() => {});
      return test;
    }
    return startAndroidStepTest(androidSerial, {
      role,
      cycle: cycle.number,
      step: 1,
      approvalPlatform,
      sender,
      secretName,
      testMethod: scenario.android.stepTestMethod ?? stepTestMethod,
      label: `Android Test #22 ${role} cycle ${cycle.number}`,
    });
  };

  for (const cycle of recoveryCycles) {
    setCurrentCycleForDiagnostics(cycle);
    if (cycle.senders.length !== 1) {
      throw new Error(`Test #22 cycle ${cycle.number} must have one sender`);
    }
    const sender = cycle.senders[0];
    const approval = cycle.approvals[0];
    const approver = approval?.platform;
    const secretName = cycle.senderSecrets[sender] ?? approval?.secret ?? defaultSecret?.name;
    if (!['web', 'android', 'ios'].includes(sender)) {
      throw new Error(`Test #22 cycle ${cycle.number} has unsupported sender/approver`);
    }
    writeDiagnostic(
      `[TEST22] cycle=${cycle.number} block=${cycle.block ?? ''} sender=${sender} `
        + `approver=${approver ?? 'none'} kind=${cycle.kind} secret=${secretName}`,
    );

    const runAfterCycleHooks = async () => {
      if (cycle.number === scenario.recovery.createSecret3AfterCycle) {
        const deferredSecrets = secretCreationPlan?.afterRecovery?.web ?? [];
        for (const deferredSecret of deferredSecrets) await createWebSecret(page, deferredSecret);
        await waitForWebSecrets(page);
        writeDiagnostic(`[TEST22] deferred Web secret creation complete names=${deferredSecrets.join(',')}`);
      }
      if (cycle.number === scenario.recovery.createSecret2AfterCycle) {
        const deferredSecrets = secretCreationPlan?.afterAndroidJoin?.web ?? [];
        for (const deferredSecret of deferredSecrets) await createWebSecret(page, deferredSecret);
        await waitForWebSecretNames(page, [
          ...(secretCreationPlan?.initial?.web ?? []),
          ...deferredSecrets,
        ]);
        writeDiagnostic(`[TEST22] Secret2 created; iOS join deferred until cycle ${scenario.recovery.joinIosAfterCycle ?? cycle.number}`);
      }
      if (cycle.number === scenario.recovery.joinIosAfterCycle) {
        const namesBeforeIos = Object.values(secretCreationPlan?.initial ?? {})
          .flat()
          .concat(Object.values(secretCreationPlan?.afterAndroidJoin ?? {}).flat())
          .map(secretNameFor);
        await runWebMultiSecretLifecycleIosJoin(
          page,
          simulatorUdid,
          [...new Set(namesBeforeIos)],
          androidSerial,
        );
        writeDiagnostic(`[TEST22] iOS joined after direct Show cycles=${cycle.number}`);
      }
      if (scenario.mode === 'web-multi-secret-lifecycle') {
        page = await refreshWebForLongRun(page, cycle.number);
      }
    };

    if (cycle.kind === 'show' || cycle.kind === 'direct-show') {
      if (cycle.approvals.length !== 0) {
        throw new Error(`Test #22 show cycle ${cycle.number} must not have an approval`);
      }
      if (sender === 'web') {
        await showAndCloseWebSecret(page, secretName);
      } else if (sender === 'android') {
        if (androidDirectShowTest) {
          if (cycle.number === 4 || cycle.number === 10) {
            approvalCoordinator.allow('android-direct-show-start', cycle.number);
          }
          await androidDirectShowTest.waitForMarker(
            `E2E: ANDROID_LOCAL_SECRET_VISIBLE_${cycle.number}`,
            180_000,
          );
          await androidDirectShowTest.waitForMarker(
            `E2E: ANDROID_LOCAL_SECRET_CLOSED_${cycle.number}`,
            60_000,
          );
          if (cycle.number === 12) {
            await androidDirectShowTest.waitForMarker('E2E: ANDROID_DIRECT_SHOW_DONE', 60_000);
            await androidDirectShowTest.result;
            androidDirectShowTest = null;
          }
        } else {
          const showTest = await startAndroidStepTest(androidSerial, {
            role: 'show',
            cycle: cycle.number,
            step: 1,
            secretName,
            testMethod: 'showLocalSecret',
            label: `Android Test #22 local Show cycle ${cycle.number}`,
          });
          await showTest.waitForMarker(`E2E: ANDROID_LOCAL_SECRET_VISIBLE_${cycle.number}`, 180_000);
          await showTest.waitForMarker(`E2E: ANDROID_LOCAL_SECRET_CLOSED_${cycle.number}`, 60_000);
          await showTest.result;
          await stopAndroidApplication(androidSerial);
        }
      } else {
        throw new Error(`Test #22 local Show does not support sender ${sender}`);
      }
      await runAfterCycleHooks();
      console.log(`✅ Test #22 cycle ${cycle.number}/${expectedCycles} passed: ${sender} local Show`);
      continue;
    }

    if (!['web', 'android', 'ios'].includes(approver)) {
      throw new Error(`Test #22 recovery cycle ${cycle.number} has unsupported approver`);
    }
    if (sender === approver) throw new Error(`Test #22 cycle ${cycle.number} sender and approver must differ`);

    let senderTest;
    let approverTest;
    if (approver !== 'web') {
      approverTest = await startNativeStep(approver, 'receiver', cycle, secretName, approver, sender);
    }
    if (sender !== 'web') {
      senderTest = await startNativeStep(sender, 'sender', cycle, secretName, approver, sender);
    }

    await waitForRecoveryParticipantsReady(page, {
      cycleNumber: cycle.number,
      secretName,
      participants: [
        { platform: approver, role: 'receiver', test: approverTest },
        { platform: sender, role: 'sender', test: senderTest },
      ],
      label: 'TEST22',
    });

    if (sender === 'web') {
      approvalCoordinator.allow('web-sender', cycle.number);
      await startWebRecovery(page, secretName);
    } else {
      approvalCoordinator.allow(`${sender}-sender-1`, cycle.number);
      await senderTest.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_REQUEST_SENT_${cycle.number}_1`,
        180_000,
      );
    }

    if (approver === 'web') {
      await approveIncomingRecoveryOnWeb(page, secretName, 1);
    } else {
      await approverTest.waitForMarker(
        `E2E: ${approver.toUpperCase()}_INCOMING_VISIBLE_${cycle.number}_1`,
        180_000,
      );
      approvalCoordinator.allow(`${approver}-approve-1`, cycle.number);
      await approverTest.waitForMarker(
        `E2E: ${approver.toUpperCase()}_APPROVED_INCOMING_${cycle.number}_1`,
        180_000,
      );
    }

    if (sender === 'web') {
      await revealAndCloseWebSecret(page, secretName, false);
    } else {
      approvalCoordinator.allow(`${sender}-show-1`, cycle.number);
      await senderTest.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_SECRET_VISIBLE_${cycle.number}_1`,
        recoveryShowTimeoutMs,
      );
      await senderTest.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_CLOSED_${cycle.number}_1`,
        60_000,
      );
    }

    if (senderTest) await senderTest.result;
    if (approverTest) await approverTest.result;
    if (sender === 'android' || approver === 'android') await stopAndroidApplication(androidSerial);
    if (sender === 'ios' || approver === 'ios') await stopIosApplication(simulatorUdid);

    await runAfterCycleHooks();
    console.log(`✅ Test #22 cycle ${cycle.number}/${expectedCycles} passed: ${sender} recovery/show`);
  }
}

async function submitWebJoinRequest(page) {
  await page.goto(scenario.web.url, { waitUntil: 'domcontentloaded' });
  await unlockWithPasskeyIfNeeded(page);
  await page.getByPlaceholder('vault name').fill(scenario.vault.name);
  await page.getByRole('button', { name: 'Set Vault Name' }).click();
  await page.getByRole('button', { name: 'Join', exact: true }).waitFor({ timeout: 120_000 });
  await page.getByRole('button', { name: 'Join', exact: true }).click();
  writeDiagnostic(`[TEST${testNumber}] Web join request submitted`);
}

async function refreshWebForKnownNames(page, cycleNumber, expectedNames, refreshCycles = webPageRefreshCycles(scenario.recovery)) {
  if (!refreshCycles.includes(cycleNumber)) return page;
  writeDiagnostic(`[TEST${testNumber}] recycling Web page after completed cycle ${cycleNumber}`);

  // A reload keeps the same Chromium renderer process and therefore does not
  // release the heap accumulated by a long UI run. Open a fresh page in the
  // same context so IndexedDB/localStorage still represent the same Web
  // device, clone its target-scoped passkey, and only then close the old page.
  const replacement = await recycleWebPage({
    page,
    configurePage: configurePrimaryTestPage,
    navigatePage: (target) => target.goto(scenario.web.url, { waitUntil: 'domcontentloaded' }),
    cloneAuthenticator: cloneVirtualAuthenticator,
    unlockPage: unlockWithPasskeyIfNeeded,
    waitUntilReady: (target) => waitForWebSecretNames(target, expectedNames),
  });
  primaryVirtualAuthenticator = replacement.authenticator;

  writeDiagnostic(`[TEST${testNumber}] Web page recycled and secrets ready after cycle ${cycleNumber}`);
  return replacement.page;
}

async function runAndroidMultiSecretLifecycle(page, simulatorUdid, androidSerial) {
  const initialName = secretCreationPlan?.initial?.android?.[0] ?? secretNameFor('secret1');
  const secondName = secretCreationPlan?.afterIosJoin?.android?.[0] ?? secretNameFor('secret2');
  const thirdName = secretCreationPlan?.afterRecovery?.android?.[0] ?? secretNameFor('secret3');
  const namesBeforeWeb = [initialName, secondName];
  const expectedCycles = scenario.recovery.expectedCycles ?? recoveryCycles.length;
  if (recoveryCycles.length !== expectedCycles) {
    throw new Error(`Test #23 expected ${expectedCycles} cycles, got ${recoveryCycles.length}`);
  }

  // Android creates Secret1 and remains the vault owner. Direct
  // instrumentation is used so later invocations retain the persisted vault.
  setJoinExpectedSecretNames([initialName]);
  const androidSetup = await startAndroidJoinTest(androidSerial, {
    testClass: scenario.android.initiatorSetupTestClass,
    testMethod: scenario.android.initiatorSetupTestMethod,
    expectedSecretNames: [initialName],
  });
  await androidSetup.waitForMarker('E2E: ANDROID_TEST23_INITIATOR_SETUP_READY', 180_000);
  await androidSetup.result;
  writeDiagnostic('[TEST23] Android vault and Secret1 ready');

  // iOS joins first. The owner approval is explicitly gated so the join test
  // cannot race the Android Devices screen.
  const iosJoin = startIosJoinTest(simulatorUdid, {
    testClass: scenario.ios.joinTestClass ?? scenario.ios.testClass ?? 'CaseEightIosBothOfflineUITest',
    testMethod: scenario.ios.joinTestMethod ?? 'joinWebInitiatedVault',
    label: 'iOS Android-initiated join',
  });
  void iosJoin.result.catch(() => {});
  await iosJoin.waitForMarker('E2E: IOS_JOIN_REQUEST_SENT', 300_000);
  approvalCoordinator.allow('android-approve-ios-join', '0');
  const iosApproval = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.initiatorSetupTestClass,
    testMethod: 'approvePendingJoinForTest23',
    role: 'owner',
    cycle: 0,
    step: 1,
    joinTarget: 'ios',
    label: 'Android approves iOS join',
  });
  await iosApproval.waitForMarker('E2E: ANDROID_JOIN_APPROVED_IOS', 180_000);
  await Promise.all([iosJoin.result, iosApproval.result]);
  writeDiagnostic('[TEST23] iOS joined Android vault');

  const afterCompletedCycle = createPostCycleActions([
    {
      name: 'create Secret2', afterCycle: scenario.recovery.createSecret2AfterCycle,
      async run(cycleNumber) {
        const secret2 = await startAndroidStepTest(androidSerial, {
          testClass: scenario.android.initiatorSetupTestClass,
          testMethod: 'createSecretForTest23',
          role: 'creator', cycle: cycleNumber, step: 1,
          secretName: secondName, secretValue: secretValueForName(secondName),
          initializeShow: true, label: 'Android creates Secret2',
        });
        await secret2.waitForMarker(`E2E: ANDROID_TEST23_SECRET_CREATED_${cycleNumber}`, 180_000);
        await secret2.result;
        setJoinExpectedSecretNames(namesBeforeWeb);
        writeDiagnostic(`[TEST23] Secret2 created after completed cycle ${cycleNumber}`);
      },
    },
    {
      name: 'create Secret3', afterCycle: scenario.recovery.createSecret3AfterCycle,
      async run(cycleNumber) {
        const secret3 = await startAndroidStepTest(androidSerial, {
          testClass: scenario.android.initiatorSetupTestClass,
          testMethod: 'createSecretForTest23',
          role: 'creator', cycle: cycleNumber, step: 1,
          secretName: thirdName, secretValue: secretValueForName(thirdName),
          initializeShow: false, label: 'Android creates Secret3',
        });
        await secret3.waitForMarker(`E2E: ANDROID_TEST23_SECRET_CREATED_${cycleNumber}`, 180_000);
        await secret3.result;
        writeDiagnostic(`[TEST23] Secret3 created after completed cycle ${cycleNumber}`);
      },
    },
  ]);

  // Direct Show on the two-device vault: Android then iOS, three cycles each.
  for (const cycle of recoveryCycles.filter((entry) => entry.kind === 'direct-show')) {
    setCurrentCycleForDiagnostics(cycle);
    const sender = cycle.senders[0];
    const secretName = cycle.senderSecrets[sender];
    if (sender === 'android') {
      const test = await startAndroidStepTest(androidSerial, {
        testClass: scenario.android.stepTestClass ?? scenario.android.testClass,
        testMethod: 'showLocalSecret',
        role: 'show',
        cycle: cycle.number,
        step: 1,
        secretName,
        label: `Android Test #23 local Show cycle ${cycle.number}`,
      });
      await test.waitForMarker(`E2E: ANDROID_LOCAL_SECRET_VISIBLE_${cycle.number}`, 180_000);
      await test.waitForMarker(`E2E: ANDROID_LOCAL_SECRET_CLOSED_${cycle.number}`, 60_000);
      await test.result;
    } else if (sender === 'ios') {
      const environment = stepEnvironment({ role: 'show', cycle: cycle.number, step: 1, secretName });
      environment.E2E_SECRET_NAME = secretName;
      environment.E2E_SECRET_VALUE = secretValueForName(secretName);
      const test = startIosJoinTest(simulatorUdid, {
        testClass: scenario.ios.stepTestClass ?? scenario.ios.testClass ?? 'CaseEightIosBothOfflineUITest',
        testMethod: 'showLocalSecret',
        label: `iOS Test #23 local Show cycle ${cycle.number}`,
        environment,
        secretName,
        secretValue: secretValueForName(secretName),
      });
      await test.waitForMarker(`E2E: IOS_LOCAL_SECRET_VISIBLE_${cycle.number}`, 180_000);
      await test.waitForMarker(`E2E: IOS_LOCAL_SECRET_CLOSED_${cycle.number}`, 60_000);
      await test.result;
    } else {
      throw new Error(`Test #23 direct Show does not support sender ${sender}`);
    }
    await afterCompletedCycle(cycle.number);
    console.log(`✅ Test #23 cycle ${cycle.number}/${expectedCycles} passed: ${sender} local Show`);
  }

  // Web is the deferred third participant. It submits a normal join request;
  // Android approves it through the same UI path as the first join.
  await submitWebJoinRequest(page);
  approvalCoordinator.allow('android-approve-web-join', '0');
  const webApproval = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.initiatorSetupTestClass,
    testMethod: 'approvePendingJoinForTest23',
    role: 'owner',
    cycle: 0,
    step: 1,
    joinTarget: 'web',
    label: 'Android approves Web join',
  });
  await webApproval.waitForMarker('E2E: ANDROID_JOIN_APPROVED_WEB', 180_000);
  await webApproval.result;
  // The Web page submitted the JOIN request while the Android owner was on
  // its approval screen.  After approval the page can still be on the JOIN
  // route with the pre-join state cached in the renderer.  Reloading here is
  // a test synchronization boundary: it makes the Web client fetch the
  // persisted membership and secret list before recovery starts.
  await page.reload({ waitUntil: 'domcontentloaded' });
  await unlockWithPasskeyIfNeeded(page);
  await waitForWebSecretNames(page, namesBeforeWeb);
  writeDiagnostic('[TEST23] Web joined Android vault after direct-show block');

  const startNativeStep = async (platform, role, cycle, secretName, approvalPlatform, sender) => {
    const environment = stepEnvironment({
      role,
      cycle: cycle.number,
      step: 1,
      approvalPlatform,
      sender,
      secretName,
    });
    if (platform === 'ios') {
      const test = startIosJoinTest(simulatorUdid, {
        testClass: scenario.ios.stepTestClass ?? scenario.ios.testClass ?? 'CaseEightIosBothOfflineUITest',
        testMethod: scenario.ios.stepTestMethod ?? 'handleRecoveryStep',
        label: `iOS Test #23 ${role} cycle ${cycle.number}`,
        environment,
        secretName,
        secretValue: secretValueForName(secretName),
      });
      void test.result.catch(() => {});
      return test;
    }
    return startAndroidStepTest(androidSerial, {
      testClass: scenario.android.stepTestClass ?? scenario.android.testClass,
      role,
      cycle: cycle.number,
      step: 1,
      approvalPlatform,
      sender,
      secretName,
      testMethod: scenario.android.stepTestMethod ?? 'handleRecoveryStep',
      label: `Android Test #23 ${role} cycle ${cycle.number}`,
    });
  };

  for (const cycle of recoveryCycles.filter((entry) => entry.kind !== 'direct-show')) {
    setCurrentCycleForDiagnostics(cycle);
    const sender = cycle.senders[0];
    const approval = cycle.approvals[0];
    const approver = approval?.platform;
    const secretName = cycle.senderSecrets[sender] ?? approval?.secret ?? defaultSecret?.name;
    if (!approver || sender === approver) throw new Error(`Test #23 invalid recovery cycle ${cycle.number}`);
    writeDiagnostic(`[TEST23] cycle=${cycle.number} sender=${sender} approver=${approver} secret=${secretName}`);

    let senderTest;
    let approverTest;
    if (approver !== 'web') {
      approverTest = await startNativeStep(approver, 'receiver', cycle, secretName, approver, sender);
    }
    if (sender !== 'web') {
      senderTest = await startNativeStep(sender, 'sender', cycle, secretName, approver, sender);
    }

    await waitForRecoveryParticipantsReady(page, {
      cycleNumber: cycle.number,
      secretName,
      participants: [
        { platform: approver, role: 'receiver', test: approverTest },
        { platform: sender, role: 'sender', test: senderTest },
      ],
      label: 'TEST23',
    });

    if (sender === 'web') {
      approvalCoordinator.allow('web-sender', cycle.number);
      await startWebRecovery(page, secretName);
    } else {
      approvalCoordinator.allow(`${sender}-sender-1`, cycle.number);
      await senderTest.waitForMarker(`E2E: ${sender.toUpperCase()}_RECOVERY_REQUEST_SENT_${cycle.number}_1`, 180_000);
    }

    if (approver === 'web') {
      await approveIncomingRecoveryOnWeb(page, secretName, 1);
    } else {
      await approverTest.waitForMarker(`E2E: ${approver.toUpperCase()}_INCOMING_VISIBLE_${cycle.number}_1`, 180_000);
      approvalCoordinator.allow(`${approver}-approve-1`, cycle.number);
      await approverTest.waitForMarker(`E2E: ${approver.toUpperCase()}_APPROVED_INCOMING_${cycle.number}_1`, 180_000);
    }

    if (sender === 'web') {
      await revealAndCloseWebSecret(page, secretName, false);
    } else {
      approvalCoordinator.allow(`${sender}-show-1`, cycle.number);
      await senderTest.waitForMarker(`E2E: ${sender.toUpperCase()}_RECOVERY_SECRET_VISIBLE_${cycle.number}_1`, recoveryShowTimeoutMs);
      await senderTest.waitForMarker(`E2E: ${sender.toUpperCase()}_RECOVERY_CLOSED_${cycle.number}_1`, 60_000);
    }
    if (senderTest) await senderTest.result;
    if (approverTest) await approverTest.result;
    await afterCompletedCycle(cycle.number);
    page = await refreshWebForKnownNames(page, cycle.number, [initialName, secondName, ...(cycle.number >= scenario.recovery.createSecret3AfterCycle ? [thirdName] : [])]);
    console.log(`✅ Test #23 cycle ${cycle.number}/${expectedCycles} passed: ${sender} recovery/show`);
  }
}

async function runIosMultiSecretLifecycle(page, simulatorUdid, androidSerial) {
  const isTest33 = scenario.mode === 'ios-two-secret-owner-lifecycle';
  const initialName = secretCreationPlan?.initial?.ios?.[0] ?? secretNameFor('secret1');
  const secondName = (isTest33
    ? secretCreationPlan?.afterWebJoin?.ios?.[0]
    : secretCreationPlan?.afterSecret1Show?.ios?.[0]) ?? secretNameFor('secret2');
  const thirdName = secretCreationPlan?.afterRecovery?.ios?.[0] ?? secretNameFor('secret3');
  const expectedCycles = scenario.recovery.expectedCycles ?? recoveryCycles.length;
  const expectedRecoveryCycles = scenario.recovery.expectedRecoveryCycles
    ?? recoveryCycles.filter((cycle) => cycle.kind !== 'direct-show').length;
  const actualRecoveryCycles = recoveryCycles.filter((cycle) => cycle.kind !== 'direct-show').length;
  const directShowCycles = recoveryCycles.filter((cycle) => cycle.kind === 'direct-show');
  if (recoveryCycles.length !== expectedCycles) {
    throw new Error(`Test #${testNumber} expected ${expectedCycles} cycles, got ${recoveryCycles.length}`);
  }
  if (actualRecoveryCycles !== expectedRecoveryCycles) {
    throw new Error(`Test #${testNumber} expected ${expectedRecoveryCycles} recovery cycles, got ${actualRecoveryCycles}`);
  }
  if (isTest33 && directShowCycles.length !== 0) {
    throw new Error(`Test #33 must not contain direct-Show cycles, got ${directShowCycles.length}`);
  }
  if ((scenario.setup?.joinOrder ?? []).join(',') !== 'web,android') {
    throw new Error(`Test #${testNumber} requires setup.joinOrder=[web,android]`);
  }
  if (isTest33 && scenario.setup?.secondJoinApprover !== 'web') {
    throw new Error(`Test #${testNumber} requires Web as the sole Android Join approver`);
  }

  const startIosAction = ({
    testMethod,
    role,
    cycle = 0,
    secretName,
    secretValue = secretValueForName(secretName),
    approvalPlatform = '',
    sender = '',
    label,
  }) => startIosJoinTest(simulatorUdid, {
    testClass: scenario.ios.testClass ?? 'CaseEightIosBothOfflineUITest',
    testMethod,
    label,
    environment: stepEnvironment({
      role,
      cycle,
      step: 1,
      approvalPlatform,
      sender,
      secretName,
    }),
    secretName,
    secretValue,
  });

  // A=iOS creates the Vault and Secret1 through the visible application UI.
  const iosSetup = startIosAction({
    testMethod: scenario.ios.initiatorSetupTestMethod ?? 'createVaultForTest24',
    role: 'creator',
    secretName: initialName,
    label: `iOS Test #${testNumber} creates Vault and Secret1`,
  });
  await iosSetup.waitForMarker(
    isTest33 ? `E2E: IOS_TEST${testNumber}_INITIATOR_SETUP_READY` : 'E2E: IOS_TEST24_INITIATOR_SETUP_READY',
    300_000,
  );
  await iosSetup.result;
  setJoinExpectedSecretNames([initialName]);
  writeDiagnostic(`[TEST${testNumber}] iOS vault and Secret1 ready`);

  // B=Web joins first and iOS approves the pending request in the Devices UI.
  await submitWebJoinRequest(page);
  const approveWeb = startIosAction({
    testMethod: scenario.ios.approveJoinTestMethod ?? 'approvePendingJoinForTest24',
    role: 'owner',
    secretName: initialName,
    sender: 'web',
    label: `iOS Test #${testNumber} approves Web join`,
  });
  await approveWeb.waitForMarker(
    isTest33 ? `E2E: IOS_TEST${testNumber}_JOIN_APPROVED_WEB` : 'E2E: IOS_JOIN_APPROVED_WEB',
    180_000,
  );
  await approveWeb.result;
  await page.reload({ waitUntil: 'domcontentloaded' });
  await unlockWithPasskeyIfNeeded(page);
  await waitForWebSecretNames(page, [initialName]);
  writeDiagnostic(`[TEST${testNumber}] Web joined iOS vault`);

  if (isTest33) {
    const secret2 = startIosAction({
      testMethod: scenario.ios.createSecretTestMethod ?? `createSecretForTest${testNumber}`,
      role: 'creator',
      cycle: 0,
      secretName: secondName,
      label: `iOS Test #${testNumber} creates Secret2`,
    });
    await secret2.waitForMarker(`E2E: IOS_TEST${testNumber}_SECRET_CREATED_0`, 180_000);
    await secret2.result;
    await waitForWebSecretNames(page, [initialName, secondName]);
    setJoinExpectedSecretNames([initialName, secondName]);
    writeDiagnostic(`[TEST${testNumber}] iOS persisted Secret2 before Android Join`);
  }

  // Two-device Show stage. Secret2 is created only after both devices finish
  // the three Secret1 repetitions, matching the user-visible scenario order.
  for (const cycle of recoveryCycles.filter((entry) => entry.kind === 'direct-show')) {
    setCurrentCycleForDiagnostics(cycle);
    const sender = cycle.senders[0];
    const secretName = cycle.senderSecrets[sender];
    if (sender === 'ios') {
      const test = startIosAction({
        testMethod: 'showLocalSecret',
        role: 'show',
        cycle: cycle.number,
        secretName,
        label: `iOS Test #${testNumber} local Show cycle ${cycle.number}`,
      });
      await test.waitForMarker(`E2E: IOS_LOCAL_SECRET_VISIBLE_${cycle.number}`, 180_000);
      await test.waitForMarker(`E2E: IOS_LOCAL_SECRET_CLOSED_${cycle.number}`, 60_000);
      await test.result;
    } else if (sender === 'web') {
      await showAndCloseWebSecret(page, secretName);
    } else {
      throw new Error(`Test #${testNumber} direct Show does not support sender ${sender}`);
    }
    console.log(`✅ Test #${testNumber} cycle ${cycle.number}/${expectedCycles} passed: ${sender} local Show`);

    if (cycle.number === scenario.recovery.createSecret2AfterCycle) {
      const secret2 = startIosAction({
        testMethod: scenario.ios.createSecretTestMethod ?? 'createSecretForTest24',
        role: 'creator',
        cycle: cycle.number,
        secretName: secondName,
        label: `iOS Test #${testNumber} creates Secret2`,
      });
      await secret2.waitForMarker(
        `E2E: IOS_TEST${testNumber}_SECRET_CREATED_${cycle.number}`,
        180_000,
      );
      await secret2.result;
      await waitForWebSecretNames(page, [initialName, secondName]);
      writeDiagnostic(`[TEST${testNumber}] Secret2 created after both Secret1 Show blocks`);
    }
  }

  // C=Android joins after the pre-join setup stage. For Test #33 Web approves
  // this Join; legacy Test #24 keeps the iOS approval path below.
  setJoinExpectedSecretNames([initialName, secondName]);
  const androidJoin = await startAndroidJoinTest(androidSerial, {
    testClass: scenario.android.testClass,
    testMethod: scenario.android.joinTestMethod ?? 'joinWebInitiatedVault',
    expectedSecretNames: [initialName, secondName],
  });
  await androidJoin.waitForMarker('E2E: ANDROID_JOIN_REQUEST_SENT', 180_000);
  if (isTest33) {
    // Test #33 deliberately makes Web the sole approver of Android's Join.
    // iOS remains an observer and must not receive an approval action.
    await approveJoinRequestOnWeb(page, 'Android', { strictCompletion: true });
    await androidJoin.waitForMarker('E2E: ANDROID_JOIN_READY', 180_000);
    await androidJoin.result;
    writeDiagnostic(
      `[TEST${testNumber}][setup] Web exclusively approved Android Join; `
        + 'iOS second-join approval not started',
    );
  } else {
    const approveAndroid = startIosAction({
      testMethod: scenario.ios.approveJoinTestMethod ?? 'approvePendingJoinForTest24',
      role: 'owner',
      secretName: initialName,
      sender: 'android',
      label: `iOS Test #${testNumber} approves Android join`,
    });
    await approveAndroid.waitForMarker('E2E: IOS_JOIN_APPROVED_ANDROID', 180_000);
    await androidJoin.waitForMarker('E2E: ANDROID_JOIN_READY', 180_000);
    await Promise.all([approveAndroid.result, androidJoin.result]);
  }
  await waitForWebSecretNames(page, [initialName, secondName]);
  writeDiagnostic(`[TEST${testNumber}] Android joined after the direct-Show block`);

  const startNativeStep = async (platform, role, cycle, secretName, approvalPlatform, sender) => {
    if (platform === 'ios') {
      const test = startIosAction({
        testMethod: scenario.ios.stepTestMethod ?? 'handleRecoveryStep',
        role,
        cycle: cycle.number,
        secretName,
        approvalPlatform,
        sender,
        label: `iOS Test #${testNumber} ${role} cycle ${cycle.number}`,
      });
      void test.result.catch(() => {});
      return test;
    }
    return startAndroidStepTest(androidSerial, {
      testClass: scenario.android.testClass,
      testMethod: scenario.android.stepTestMethod ?? 'handleRecoveryStep',
      role,
      cycle: cycle.number,
      step: 1,
      approvalPlatform,
      sender,
      secretName,
      secretValue: secretValueForName(secretName),
      label: `Android Test #${testNumber} ${role} cycle ${cycle.number}`,
    });
  };

  for (const cycle of recoveryCycles.filter((entry) => entry.kind !== 'direct-show')) {
    setCurrentCycleForDiagnostics(cycle);
    const sender = cycle.senders[0];
    if (cycle.senders.length !== 1 || cycle.approvals.length !== 1) {
      throw new Error(
        `Test #${testNumber} recovery cycle ${cycle.number} must have exactly one sender and approver`,
      );
    }
    const approval = cycle.approvals[0];
    const approver = approval?.platform;
    const secretName = cycle.senderSecrets[sender] ?? approval?.secret ?? defaultSecret?.name;
    if (!approver || sender === approver) throw new Error(`Test #${testNumber} invalid recovery cycle ${cycle.number}`);
    const observer = ['web', 'ios', 'android']
      .find((platform) => platform !== sender && platform !== approver);
    if (!observer) throw new Error(`Test #${testNumber} cycle ${cycle.number} has no non-approving receiver`);
    if (cycle.observer && cycle.observer !== observer) {
      throw new Error(
        `Test #${testNumber} cycle ${cycle.number} observer mismatch: `
          + `scenario=${cycle.observer} derived=${observer}`,
      );
    }

    // Secret3 belongs after the scenario-declared completed-cycle boundary,
    // not during the final preceding Secret2 cycle.
    if (cycle.number === scenario.recovery.createSecret3AfterCycle + 1) {
      const secret3 = startIosAction({
        testMethod: scenario.ios.createSecretTestMethod ?? 'createSecretForTest24',
        role: 'creator',
        cycle: scenario.recovery.createSecret3AfterCycle,
        secretName: thirdName,
        label: `iOS Test #${testNumber} creates Secret3`,
      });
      await secret3.waitForMarker(
        `E2E: IOS_TEST${testNumber}_SECRET_CREATED_${scenario.recovery.createSecret3AfterCycle}`,
        180_000,
      );
      await secret3.result;
      await waitForWebSecretNames(page, [initialName, secondName, thirdName]);
      if (isTest33 && scenario.android.redistributionCheckTestMethod) {
        const androidSecret3 = await startAndroidStepTest(androidSerial, {
          testClass: scenario.android.testClass,
          testMethod: scenario.android.redistributionCheckTestMethod,
          role: 'redistribution-observer',
          cycle: scenario.recovery.createSecret3AfterCycle,
          step: 1,
          secretName: thirdName,
          secretValue: secretValueForName(thirdName),
          label: `Android Test #${testNumber} observes Secret3 before cycle ${cycle.number}`,
        });
        await androidSecret3.waitForMarker(
          `E2E: ANDROID_TEST${testNumber}_SECRET_VISIBLE_${scenario.recovery.createSecret3AfterCycle}`,
          180_000,
        );
        await androidSecret3.result;
        writeDiagnostic(
          `[TEST${testNumber}][redistribution] Android observed ${thirdName} before cycle ${cycle.number}`,
        );
      }
      writeDiagnostic(`[TEST${testNumber}] Secret3 created after Secret1/Secret2 recovery blocks`);
    }

    writeDiagnostic(
      `[TEST${testNumber}] cycle=${cycle.number} sender=${sender} approver=${approver} `
        + `observer=${observer} secret=${secretName}`,
    );
    let senderTest;
    let approverTest;
    let observerTest;
    if (approver !== 'web') {
      approverTest = await startNativeStep(approver, 'receiver', cycle, secretName, approver, sender);
    }
    if (observer !== 'web') {
      observerTest = await startNativeStep(observer, 'receiver', cycle, secretName, approver, sender);
    }
    if (sender !== 'web') {
      senderTest = await startNativeStep(sender, 'sender', cycle, secretName, approver, sender);
    }

    await waitForRecoveryParticipantsReady(page, {
      cycleNumber: cycle.number,
      secretName,
      participants: [
        { platform: approver, role: 'receiver', test: approverTest },
        { platform: observer, role: 'receiver', test: observerTest },
        { platform: sender, role: 'sender', test: senderTest },
      ],
      label: `TEST${testNumber}:${cycle.block}`,
    });

    if (sender === 'web') {
      approvalCoordinator.allow('web-sender', cycle.number);
      await startWebRecovery(page, secretName);
    } else {
      approvalCoordinator.allow(`${sender}-sender-1`, cycle.number);
      await senderTest.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_REQUEST_SENT_${cycle.number}_1`,
        180_000,
      );
    }

    // Both receivers must observe the same pending Claim before the selected
    // approver is allowed to make it terminal. This prevents a fast Approve
    // from hiding a missing notification on the non-approving device.
    const waitForReceiverVisible = async (platform, test) => {
      if (platform === 'web') {
        await waitForWebIncomingRecoveryCount(page, 1, secretName);
        await page.getByTestId(`open-recovery-request-${secretName}`).waitFor({
          state: 'visible',
          timeout: 120_000,
        });
        writeDiagnostic(`[TEST${testNumber}] cycle=${cycle.number} Web receiver badge/open action visible`);
        return;
      }
      await test.waitForMarker(
        `E2E: ${platform.toUpperCase()}_INCOMING_VISIBLE_${cycle.number}_1`,
        180_000,
      );
    };
    await Promise.all([
      waitForReceiverVisible(approver, approverTest),
      waitForReceiverVisible(observer, observerTest),
    ]);

    if (approver === 'web') {
      await approveIncomingRecoveryOnWeb(page, secretName, 1);
    } else {
      approvalCoordinator.allow(`${approver}-approve-1`, cycle.number);
      await approverTest.waitForMarker(
        `E2E: ${approver.toUpperCase()}_APPROVED_INCOMING_${cycle.number}_1`,
        180_000,
      );
    }

    // The receiver that did not Approve must remove both its badge and open
    // action as a direct consequence of the terminal response.
    if (observer === 'web') {
      await waitForWebIncomingRecoveryGone(page, secretName);
    } else {
      approvalCoordinator.allow(`${observer}-dismiss-1`, cycle.number);
      await observerTest.waitForMarker(
        `E2E: ${observer.toUpperCase()}_DISMISSED_INCOMING_${cycle.number}_1`,
        180_000,
      );
    }
    writeDiagnostic(`[TEST${testNumber}] cycle=${cycle.number} observer=${observer} request disappeared after Approve`);

    if (sender === 'web') {
      await revealAndCloseWebSecret(page, secretName, false);
    } else {
      approvalCoordinator.allow(`${sender}-show-1`, cycle.number);
      await senderTest.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_SECRET_VISIBLE_${cycle.number}_1`,
        recoveryShowTimeoutMs,
      );
      await senderTest.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_CLOSED_${cycle.number}_1`,
        60_000,
      );
    }
    if (senderTest) await senderTest.result;
    if (approverTest) await approverTest.result;
    if (observerTest) await observerTest.result;

    page = await refreshWebForKnownNames(page, cycle.number, [
      initialName,
      secondName,
      ...(cycle.number > scenario.recovery.createSecret3AfterCycle ? [thirdName] : []),
    ]);
    console.log(`✅ Test #24 cycle ${cycle.number}/${expectedCycles} passed: ${sender} recovery/show`);
  }
}

async function runIosSecondaryCreatorLifecycle(page, simulatorUdid, androidSerial) {
  const initialName = secretCreationPlan?.initial?.ios?.[0] ?? secretNameFor('secret1');
  const secondName = secretCreationPlan?.afterWebJoin?.web?.[0] ?? secretNameFor('secret2');
  const thirdSecretCreator = scenario.setup?.thirdSecretCreator ?? 'web';
  const secondJoinApprover = scenario.setup?.secondJoinApprover ?? 'ios';
  const verifyBothReceiverAlerts = scenario.recovery.verifyBothReceiverAlerts === true;
  const thirdName = secretCreationPlan?.afterRecovery?.[thirdSecretCreator]?.[0]
    ?? secretNameFor('secret3');
  const twoDeviceNames = [initialName, secondName];
  const expectedCycles = scenario.recovery.expectedCycles ?? recoveryCycles.length;
  const directShowCycles = recoveryCycles.filter((cycle) => cycle.kind === 'direct-show');
  const recoveryOnlyCycles = recoveryCycles.filter((cycle) => cycle.kind !== 'direct-show');
  const expectedDirectShowCycles = scenario.recovery.expectedDirectShowCycles ?? 12;
  const expectedRecoveryCycles = scenario.recovery.expectedRecoveryCycles ?? 27;

  if (recoveryCycles.length !== expectedCycles) {
    throw new Error(`Test #${testNumber} expected ${expectedCycles} cycles, got ${recoveryCycles.length}`);
  }
  if (directShowCycles.length !== expectedDirectShowCycles) {
    throw new Error(
      `Test #${testNumber} expected ${expectedDirectShowCycles} direct Show cycles, got ${directShowCycles.length}`,
    );
  }
  if (recoveryOnlyCycles.length !== expectedRecoveryCycles) {
    throw new Error(
      `Test #${testNumber} expected ${expectedRecoveryCycles} recovery cycles, got ${recoveryOnlyCycles.length}`,
    );
  }
  if ((scenario.setup?.joinOrder ?? []).join(',') !== 'web,android') {
    throw new Error(`Test #${testNumber} requires setup.joinOrder=[web,android]`);
  }
  if (scenario.setup?.secondarySecretCreator !== 'web') {
    throw new Error(`Test #${testNumber} requires Web as the secondary Secret creator`);
  }
  if (!['web', 'android'].includes(thirdSecretCreator)) {
    throw new Error(
      `Test #${testNumber} requires Web or Android as the third Secret creator, got ${thirdSecretCreator}`,
    );
  }
  if (!['ios', 'web'].includes(secondJoinApprover)) {
    throw new Error(
      `Test #${testNumber} requires iOS or Web as the Android Join approver, got ${secondJoinApprover}`,
    );
  }

  const expectedRoleCount = scenario.recovery.expectedRoleCountPerPlatform;
  if (Number.isInteger(expectedRoleCount)) {
    const roleCounts = Object.fromEntries(
      ['web', 'android', 'ios'].map((platform) => [platform, { sender: 0, approver: 0, observer: 0 }]),
    );
    for (const cycle of recoveryOnlyCycles) {
      const sender = cycle.senders[0];
      const approver = cycle.approvals[0]?.platform;
      const observer = ['web', 'android', 'ios']
        .find((platform) => platform !== sender && platform !== approver);
      if (roleCounts[sender]) roleCounts[sender].sender += 1;
      if (roleCounts[approver]) roleCounts[approver].approver += 1;
      if (roleCounts[observer]) roleCounts[observer].observer += 1;
      if (cycle.observer && cycle.observer !== observer) {
        throw new Error(
          `Test #${testNumber} cycle ${cycle.number} observer mismatch: `
            + `scenario=${cycle.observer} derived=${observer}`,
        );
      }
    }
    for (const [platform, counts] of Object.entries(roleCounts)) {
      for (const [role, count] of Object.entries(counts)) {
        if (count !== expectedRoleCount) {
          throw new Error(
            `Test #${testNumber} expected ${platform} ${role}=${expectedRoleCount}, got ${count}`,
          );
        }
      }
    }
  }

  const startIosAction = ({
    testMethod,
    role,
    cycle = 0,
    secretName,
    secretValue = secretValueForName(secretName),
    approvalPlatform = '',
    sender = '',
    label,
  }) => startIosJoinTest(simulatorUdid, {
    testClass: scenario.ios.testClass ?? 'CaseEightIosBothOfflineUITest',
    testMethod,
    label,
    environment: {
      ...stepEnvironment({
        role,
        cycle,
        step: 1,
        approvalPlatform,
        sender,
        secretName,
      }),
      E2E_SECRET_NAME: secretName,
      E2E_SECRET_VALUE: secretValue,
    },
    secretName,
    secretValue,
  });

  // A=iOS creates the Vault and Secret1 through the visible application UI.
  const iosSetup = startIosAction({
    testMethod: scenario.ios.initiatorSetupTestMethod ?? 'createVaultForTest27',
    role: 'creator',
    secretName: initialName,
    label: `iOS Test #${testNumber} creates Vault and Secret1`,
  });
  await iosSetup.waitForMarker(`E2E: IOS_TEST${testNumber}_INITIATOR_SETUP_READY`, 300_000);
  await iosSetup.result;
  setJoinExpectedSecretNames([initialName]);
  writeDiagnostic(`[TEST${testNumber}][setup] iOS Vault and Secret1 persisted`);

  // B=Web joins first. iOS acts only after the Web request is persisted, and
  // Web refreshes canonical Core state before creating Secret2.
  await submitWebJoinRequest(page);
  const approveWeb = startIosAction({
    testMethod: scenario.ios.approveJoinTestMethod ?? 'approvePendingJoinForTest27',
    role: 'owner',
    secretName: initialName,
    sender: 'web',
    label: `iOS Test #${testNumber} approves Web join`,
  });
  await approveWeb.waitForMarker(`E2E: IOS_TEST${testNumber}_JOIN_APPROVED_WEB`, 180_000);
  await approveWeb.result;
  await page.reload({ waitUntil: 'domcontentloaded' });
  await unlockWithPasskeyIfNeeded(page);
  await waitForWebSecretNames(page, [initialName]);
  writeDiagnostic(`[TEST${testNumber}][setup] Web joined iOS Vault and received Secret1`);

  // B=Web creates Secret2 before either two-device member starts its direct
  // Show repetitions. The UI row is the persisted completion boundary.
  await createWebSecret(page, secondName);
  await waitForWebSecretNames(page, twoDeviceNames);
  setJoinExpectedSecretNames(twoDeviceNames);
  writeDiagnostic(`[TEST${testNumber}][create] Web persisted Secret2`);

  const assertIosSecretVisible = async (
    secretName,
    boundaryCycle,
    testMethod = scenario.ios.redistributionCheckTestMethod,
  ) => {
    if (!testMethod) return;
    const assertion = startIosAction({
      testMethod,
      role: 'redistribution-observer',
      cycle: boundaryCycle,
      secretName,
      label: `iOS Test #${testNumber} observes redistributed ${secretName}`,
    });
    void assertion.result.catch(() => {});
    await assertion.waitForMarker(
      `E2E: IOS_TEST${testNumber}_SECRET_VISIBLE_${boundaryCycle}`,
      180_000,
    );
    await assertion.result;
    writeDiagnostic(
      `[TEST${testNumber}][redistribution] iOS observed ${secretName} boundary=${boundaryCycle}`,
    );
  };

  await assertIosSecretVisible(secondName, 2);

  for (const cycle of directShowCycles) {
    setCurrentCycleForDiagnostics(cycle);
    const sender = cycle.senders[0];
    const secretName = cycle.senderSecrets[sender];
    if (cycle.approvals.length !== 0) {
      throw new Error(`Test #${testNumber} direct Show cycle ${cycle.number} must not have an approver`);
    }

    if (sender === 'ios') {
      const show = startIosAction({
        testMethod: scenario.ios.directShowTestMethod ?? 'showLocalSecretForTest27',
        role: 'show',
        cycle: cycle.number,
        secretName,
        label: `iOS Test #${testNumber} local Show cycle ${cycle.number}`,
      });
      void show.result.catch(() => {});
      await show.waitForMarker(
        `E2E: IOS_TEST${testNumber}_LOCAL_SECRET_VALUE_VERIFIED_${cycle.number}`,
        180_000,
      );
      await show.waitForMarker(
        `E2E: IOS_TEST${testNumber}_LOCAL_SECRET_VISIBLE_${cycle.number}`,
        180_000,
      );
      await show.waitForMarker(
        `E2E: IOS_TEST${testNumber}_LOCAL_SECRET_CLOSED_${cycle.number}`,
        60_000,
      );
      await show.result;
    } else if (sender === 'web') {
      await showAndCloseWebSecret(page, secretName);
    } else {
      throw new Error(`Test #${testNumber} direct Show does not support sender ${sender}`);
    }

    writeDiagnostic(
      `[TEST${testNumber}][${cycle.block}][cycle=${cycle.number}] sender=${sender} `
        + `secret=${secretName} exactValue=verified action=closed`,
    );
    console.log(`✅ Test #${testNumber} cycle ${cycle.number}/${expectedCycles} passed: ${sender} local Show`);
  }

  const actualJoinBoundary = directShowCycles.at(-1)?.number ?? 0;
  if (actualJoinBoundary !== scenario.recovery.joinAndroidAfterCycle) {
    throw new Error(
      `Test #${testNumber} Android JOIN boundary must follow cycle ${scenario.recovery.joinAndroidAfterCycle}`,
    );
  }

  // C=Android joins at the declared completed-cycle boundary. It must render
  // both existing Secrets before the three-device recovery matrix begins.
  // The scenario explicitly selects the sole approver; no approval action is
  // launched on the other existing member.
  const androidJoin = await startAndroidJoinTest(androidSerial, {
    testClass: scenario.android.testClass,
    testMethod: scenario.android.joinTestMethod ?? 'joinWebInitiatedVault',
    expectedSecretNames: twoDeviceNames,
  });
  await androidJoin.waitForMarker('E2E: ANDROID_JOIN_REQUEST_SENT', 180_000);
  if (secondJoinApprover === 'web') {
    await approveJoinRequestOnWeb(page, 'Android', { strictCompletion: true });
    await androidJoin.waitForMarker('E2E: ANDROID_JOIN_READY', 180_000);
    await androidJoin.result;
    writeDiagnostic(
      `[TEST${testNumber}][setup] Web exclusively approved Android Join; `
        + 'iOS second-join approval not started',
    );
  } else {
    const approveAndroid = startIosAction({
      testMethod: scenario.ios.approveJoinTestMethod ?? 'approvePendingJoinForTest27',
      role: 'owner',
      secretName: initialName,
      sender: 'android',
      label: `iOS Test #${testNumber} approves Android join`,
    });
    await approveAndroid.waitForMarker(
      `E2E: IOS_TEST${testNumber}_JOIN_APPROVED_ANDROID`,
      180_000,
    );
    await androidJoin.waitForMarker('E2E: ANDROID_JOIN_READY', 180_000);
    await Promise.all([approveAndroid.result, androidJoin.result]);
  }
  await waitForWebSecretNames(page, twoDeviceNames);
  writeDiagnostic(
    `[TEST${testNumber}][setup] Android joined after cycle ${actualJoinBoundary}; `
      + `approver=${secondJoinApprover}; received Secret1/Secret2`,
  );

  const createThirdSecret = async (boundaryCycle) => {
    if (thirdSecretCreator === 'web') {
      await createWebSecret(page, thirdName);
    } else {
      const creation = await startAndroidStepTest(androidSerial, {
        testClass: scenario.android.creatorTestClass,
        testMethod: scenario.android.createSecretTestMethod ?? `createSecretForTest${testNumber}`,
        role: 'creator',
        cycle: boundaryCycle,
        step: 1,
        secretName: thirdName,
        secretValue: secretValueForName(thirdName),
        label: `Android Test #${testNumber} creates Secret3`,
      });
      await creation.waitForMarker(
        `E2E: ANDROID_TEST${testNumber}_SECRET_CREATED_${boundaryCycle}`,
        180_000,
      );
      await creation.result;
    }
    await waitForWebSecretNames(page, [...twoDeviceNames, thirdName]);
    await assertIosSecretVisible(thirdName, boundaryCycle);
    writeDiagnostic(
      `[TEST${testNumber}][create] ${thirdSecretCreator} created Secret3 `
        + `after completed cycle ${boundaryCycle}; iOS/Web visibility verified`,
    );
  };

  const startNativeStep = async (platform, role, cycle, secretName, approvalPlatform, sender) => {
    const environment = stepEnvironment({
      role,
      cycle: cycle.number,
      step: 1,
      approvalPlatform,
      sender,
      verifyReceiverAlert: verifyBothReceiverAlerts,
      secretName,
    });
    environment.E2E_SECRET_NAME = secretName;
    environment.E2E_SECRET_VALUE = secretValueForName(secretName);
    if (platform === 'ios') {
      const test = startIosJoinTest(simulatorUdid, {
        testClass: scenario.ios.stepTestClass
          ?? scenario.ios.testClass
          ?? 'CaseEightIosBothOfflineUITest',
        testMethod: scenario.ios.stepTestMethod ?? 'handleRecoveryStep',
        label: `iOS Test #${testNumber} ${role} cycle ${cycle.number}`,
        environment,
        secretName,
        secretValue: secretValueForName(secretName),
      });
      void test.result.catch(() => {});
      return test;
    }
    return startAndroidStepTest(androidSerial, {
      testClass: scenario.android.stepTestClass ?? scenario.android.testClass,
      testMethod: scenario.android.stepTestMethod ?? 'handleRecoveryStep',
      role,
      cycle: cycle.number,
      step: 1,
      approvalPlatform,
      sender,
      verifyReceiverAlert: verifyBothReceiverAlerts,
      secretName,
      secretValue: secretValueForName(secretName),
      label: `Android Test #${testNumber} ${role} cycle ${cycle.number}`,
    });
  };

  for (const cycle of recoveryOnlyCycles) {
    setCurrentCycleForDiagnostics(cycle);
    if (cycle.senders.length !== 1 || cycle.approvals.length !== 1) {
      throw new Error(
        `Test #${testNumber} recovery cycle ${cycle.number} must have exactly one sender and approver`,
      );
    }
    const sender = cycle.senders[0];
    const approval = cycle.approvals[0];
    const approver = approval?.platform;
    const secretName = cycle.senderSecrets[sender] ?? approval?.secret ?? defaultSecret?.name;
    if (!['web', 'android', 'ios'].includes(sender)) {
      throw new Error(`Test #${testNumber} cycle ${cycle.number} has unsupported sender ${sender}`);
    }
    if (!['web', 'android', 'ios'].includes(approver) || sender === approver) {
      throw new Error(`Test #${testNumber} cycle ${cycle.number} has invalid approver ${approver}`);
    }
    const observer = ['web', 'android', 'ios']
      .find((platform) => platform !== sender && platform !== approver);
    if (!observer) throw new Error(`Test #${testNumber} cycle ${cycle.number} has no observer`);
    if (cycle.observer !== observer) {
      throw new Error(
        `Test #${testNumber} cycle ${cycle.number} observer mismatch: scenario=${cycle.observer} derived=${observer}`,
      );
    }

    // Create Secret3 at the scenario's declared completed-cycle boundary.
    // Test #27 places it after Secret1/Secret2 recovery; Test #30 places it
    // immediately after the second JOIN and before the first recovery cycle.
    if (cycle.number === scenario.recovery.createSecret3AfterCycle + 1) {
      await createThirdSecret(scenario.recovery.createSecret3AfterCycle);
    }

    writeDiagnostic(
      `[TEST${testNumber}][${cycle.block}][cycle=${cycle.number}] sender=${sender} `
        + `approver=${approver} observer=${observer} secret=${secretName}`,
    );

    let senderTest;
    let approverTest;
    let observerTest;
    if (approver !== 'web') {
      approverTest = await startNativeStep(approver, 'receiver', cycle, secretName, approver, sender);
    }
    if (observer !== 'web') {
      observerTest = await startNativeStep(observer, 'receiver', cycle, secretName, approver, sender);
    }
    if (sender !== 'web') {
      senderTest = await startNativeStep(sender, 'sender', cycle, secretName, approver, sender);
    }

    await waitForRecoveryParticipantsReady(page, {
      cycleNumber: cycle.number,
      secretName,
      participants: [
        { platform: approver, role: 'receiver', test: approverTest },
        { platform: observer, role: 'receiver', test: observerTest },
        { platform: sender, role: 'sender', test: senderTest },
      ],
      label: `TEST${testNumber}:${cycle.block}`,
    });

    if (sender === 'web') {
      approvalCoordinator.allow('web-sender', cycle.number);
      await startWebRecovery(page, secretName);
    } else {
      approvalCoordinator.allow(`${sender}-sender-1`, cycle.number);
      await senderTest.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_REQUEST_SENT_${cycle.number}_1`,
        180_000,
      );
    }

    const waitForReceiverVisible = async (platform, test) => {
      if (platform === 'web') {
        if (verifyBothReceiverAlerts) {
          await openStrictWebIncomingAlert(page, 1, secretName, {
            dismissWithoutDecision: platform === observer,
          });
        } else {
          await waitForStrictWebIncomingVisible(page, 1, secretName);
        }
        return;
      }
      await test.waitForMarker(
        verifyBothReceiverAlerts
          ? `E2E: ${platform.toUpperCase()}_INCOMING_ALERT_VISIBLE_${cycle.number}_1`
          : `E2E: ${platform.toUpperCase()}_INCOMING_VISIBLE_${cycle.number}_1`,
        180_000,
      );
      if (verifyBothReceiverAlerts && platform === observer) {
        await test.waitForMarker(
          `E2E: ${platform.toUpperCase()}_INCOMING_ALERT_DISMISSED_WITHOUT_DECISION_${cycle.number}_1`,
          60_000,
        );
      }
    };
    await Promise.all([
      waitForReceiverVisible(approver, approverTest),
      waitForReceiverVisible(observer, observerTest),
    ]);
    writeDiagnostic(`[TEST${testNumber}][${cycle.block}][cycle=${cycle.number}] bothReceiversVisible=true`);

    if (approver === 'web') {
      if (verifyBothReceiverAlerts) {
        await approveOpenIncomingRecoveryOnWeb(page, secretName);
      } else {
        await approveIncomingRecoveryOnWeb(page, secretName, 1);
      }
    } else {
      approvalCoordinator.allow(`${approver}-approve-1`, cycle.number);
      await approverTest.waitForMarker(
        `E2E: ${approver.toUpperCase()}_APPROVED_INCOMING_${cycle.number}_1`,
        180_000,
      );
    }

    if (observer === 'web') {
      await waitForWebIncomingRecoveryGone(page, secretName);
    } else {
      approvalCoordinator.allow(`${observer}-dismiss-1`, cycle.number);
      await observerTest.waitForMarker(
        `E2E: ${observer.toUpperCase()}_DISMISSED_INCOMING_${cycle.number}_1`,
        180_000,
      );
    }
    writeDiagnostic(
      `[TEST${testNumber}][${cycle.block}][cycle=${cycle.number}] observer=${observer} requestGone=true`,
    );

    if (sender === 'web') {
      await revealAndCloseWebSecret(page, secretName, false);
    } else {
      approvalCoordinator.allow(`${sender}-show-1`, cycle.number);
      await senderTest.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_SECRET_VISIBLE_${cycle.number}_1`,
        recoveryShowTimeoutMs,
      );
      await senderTest.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_CLOSED_${cycle.number}_1`,
        60_000,
      );
    }

    if (senderTest) await senderTest.result;
    if (approverTest) await approverTest.result;
    if (observerTest) await observerTest.result;

    writeDiagnostic(
      `[TEST${testNumber}][${cycle.block}][cycle=${cycle.number}] sender=${sender} `
        + 'exactValue=verified action=closed',
    );
    page = await refreshWebForKnownNames(page, cycle.number, [
      ...twoDeviceNames,
      ...(cycle.number > scenario.recovery.createSecret3AfterCycle ? [thirdName] : []),
    ]);
    console.log(`✅ Test #${testNumber} cycle ${cycle.number}/${expectedCycles} passed: ${sender} recovery/show`);
  }
}

async function runWebSecondaryCreatorLifecycle(page, simulatorUdid, androidSerial) {
  const initialName = secretCreationPlan?.initial?.web?.[0] ?? secretNameFor('secret1');
  const secondarySecretCreator = scenario.setup?.secondarySecretCreator ?? 'android';
  const secondName = secretCreationPlan?.afterAndroidJoin?.[secondarySecretCreator]?.[0]
    ?? secretNameFor('secret2');
  const thirdSecretCreator = scenario.setup?.thirdSecretCreator ?? 'android';
  const thirdName = secretCreationPlan?.afterRecovery?.[thirdSecretCreator]?.[0]
    ?? secretNameFor('secret3');
  const twoDeviceNames = [initialName, secondName];
  const expectedCycles = scenario.recovery.expectedCycles ?? recoveryCycles.length;
  const directShowCycles = recoveryCycles.filter((cycle) => cycle.kind === 'direct-show');
  const recoveryOnlyCycles = recoveryCycles.filter((cycle) => cycle.kind !== 'direct-show');
  const expectedDirectShowCycles = scenario.recovery.expectedDirectShowCycles ?? 12;
  const expectedRecoveryCycles = scenario.recovery.expectedRecoveryCycles ?? 27;
  const verifyBothReceiverAlerts = scenario.recovery.verifyBothReceiverAlerts === true;

  if (recoveryCycles.length !== expectedCycles) {
    throw new Error(`Test #${testNumber} expected ${expectedCycles} cycles, got ${recoveryCycles.length}`);
  }
  if (directShowCycles.length !== expectedDirectShowCycles) {
    throw new Error(
      `Test #${testNumber} expected ${expectedDirectShowCycles} direct Show cycles, got ${directShowCycles.length}`,
    );
  }
  if (recoveryOnlyCycles.length !== expectedRecoveryCycles) {
    throw new Error(
      `Test #${testNumber} expected ${expectedRecoveryCycles} recovery cycles, got ${recoveryOnlyCycles.length}`,
    );
  }
  if ((scenario.setup?.joinOrder ?? []).join(',') !== 'android,ios') {
    throw new Error(`Test #${testNumber} requires setup.joinOrder=[android,ios]`);
  }
  if (!['android', 'web'].includes(secondarySecretCreator)) {
    throw new Error(
      `Test #${testNumber} requires Android or Web as the secondary Secret creator, `
        + `got ${secondarySecretCreator}`,
    );
  }
  if (!['android', 'ios', 'web'].includes(thirdSecretCreator)) {
    throw new Error(
      `Test #${testNumber} requires Android, iOS, or Web as the third Secret creator, `
        + `got ${thirdSecretCreator}`,
    );
  }
  if (testNumber === 34) {
    if (secondarySecretCreator !== 'android') {
      throw new Error(`Test #34 requires Android as the secondary Secret creator`);
    }
    if (thirdSecretCreator !== 'ios') {
      throw new Error(`Test #34 requires iOS as the third Secret creator`);
    }
    if (scenario.setup?.secondJoinApprover !== 'android') {
      throw new Error(`Test #34 requires Android as the sole second Join approver`);
    }
  }

  const expectedRoleCount = scenario.recovery.expectedRoleCountPerPlatform;
  if (Number.isInteger(expectedRoleCount)) {
    const roleCounts = Object.fromEntries(
      ['web', 'android', 'ios'].map((platform) => [platform, { sender: 0, approver: 0, observer: 0 }]),
    );
    for (const cycle of recoveryOnlyCycles) {
      if (cycle.senders.length !== 1 || cycle.approvals.length !== 1) {
        throw new Error(
          `Test #${testNumber} recovery cycle ${cycle.number} must have exactly one sender and approver`,
        );
      }
      const sender = cycle.senders[0];
      const approver = cycle.approvals[0]?.platform;
      const observer = ['web', 'android', 'ios']
        .find((platform) => platform !== sender && platform !== approver);
      if (roleCounts[sender]) roleCounts[sender].sender += 1;
      if (roleCounts[approver]) roleCounts[approver].approver += 1;
      if (roleCounts[observer]) roleCounts[observer].observer += 1;
      if (cycle.observer && cycle.observer !== observer) {
        throw new Error(
          `Test #${testNumber} cycle ${cycle.number} observer mismatch: `
            + `scenario=${cycle.observer} derived=${observer}`,
        );
      }
    }
    for (const [platform, counts] of Object.entries(roleCounts)) {
      for (const [role, count] of Object.entries(counts)) {
        if (count !== expectedRoleCount) {
          throw new Error(
            `Test #${testNumber} expected ${platform} ${role}=${expectedRoleCount}, got ${count}`,
          );
        }
      }
    }
  }

  // A=Web creates the Vault and Secret1, then B=Android joins through the
  // regular visible JOIN flow. Only Secret1 exists at this boundary.
  const setup = await runWebMultiSecretLifecycleSetup(page, simulatorUdid, androidSerial);
  await setup.androidTest.result;

  // The scenario-selected two-device member creates Secret2 after Android's
  // JOIN has been persisted. Native creation stays in a separate visible UI
  // invocation; Web creation is followed by an explicit Android row check.
  const createAndroidSecret = async (secretName, cycle, label) => {
    const creation = await startAndroidStepTest(androidSerial, {
      testClass: scenario.android.creatorTestClass,
      testMethod: scenario.android.createSecretTestMethod ?? 'createSecretForTest25',
      role: 'creator',
      cycle,
      step: 1,
      secretName,
      secretValue: secretValueForName(secretName),
      label,
    });
    await creation.waitForMarker(
      `E2E: ANDROID_TEST${testNumber}_SECRET_CREATED_${cycle}`,
      180_000,
    );
    await creation.result;
  };

  const createIosSecret = async (secretName, cycle, label) => {
    const creation = startIosJoinTest(simulatorUdid, {
      testClass: scenario.ios.testClass ?? 'CaseEightIosBothOfflineUITest',
      testMethod: scenario.ios.createSecretTestMethod ?? `createSecretForTest${testNumber}`,
      label,
      environment: stepEnvironment({ role: 'creator', cycle, step: 1, secretName }),
      secretName,
      secretValue: secretValueForName(secretName),
    });
    void creation.result.catch(() => {});
    await creation.waitForMarker(
      `E2E: IOS_TEST${testNumber}_SECRET_CREATED_${cycle}`,
      180_000,
    );
    await creation.result;
  };

  if (secondarySecretCreator === 'android') {
    await createAndroidSecret(secondName, 0, `Android Test #${testNumber} creates Secret2`);
  } else {
    await createWebSecret(page, secondName);
    await waitForWebSecretNames(page, twoDeviceNames);
    const androidVisibility = await startAndroidStepTest(androidSerial, {
      testClass: scenario.android.lifecycleTestClass
        ?? scenario.android.creatorTestClass
        ?? scenario.android.testClass,
      testMethod: scenario.android.redistributionCheckTestMethod,
      role: 'redistribution-check',
      cycle: 0,
      step: 1,
      secretName: secondName,
      label: `Android Test #${testNumber} verifies Web-created Secret2 before iOS Join`,
    });
    await androidVisibility.waitForMarker(
      `E2E: ANDROID_TEST${testNumber}_SECRET_VISIBLE_0`,
      180_000,
    );
    await androidVisibility.result;
    writeDiagnostic(
      `[TEST${testNumber}] Android observed Web-created Secret2 before approving iOS Join`,
    );
  }
  await waitForWebSecretNames(page, twoDeviceNames);
  setJoinExpectedSecretNames(twoDeviceNames);
  writeDiagnostic(`[TEST${testNumber}] Web and Android ready with Secret1/Secret2`);

  // Scenarios with a two-device Show phase exercise both members three times;
  // distributed-creator scenarios can declare zero direct-Show cycles.
  for (const cycle of directShowCycles) {
    setCurrentCycleForDiagnostics(cycle);
    const sender = cycle.senders[0];
    const secretName = cycle.senderSecrets[sender];
    if (cycle.approvals.length !== 0) {
      throw new Error(`Test #${testNumber} direct Show cycle ${cycle.number} must not have an approver`);
    }
    if (sender === 'web') {
      await showAndCloseWebSecret(page, secretName);
    } else if (sender === 'android') {
      const show = await startAndroidStepTest(androidSerial, {
        testClass: scenario.android.creatorTestClass,
        testMethod: 'showLocalSecretForTest25',
        role: 'show',
        cycle: cycle.number,
        step: 1,
        secretName,
        secretValue: secretValueForName(secretName),
        label: `Android Test #${testNumber} local Show cycle ${cycle.number}`,
      });
      await show.waitForMarker(
        `E2E: ANDROID_TEST${testNumber}_LOCAL_SECRET_VALUE_VERIFIED_${cycle.number}`,
        180_000,
      );
      await show.waitForMarker(`E2E: ANDROID_LOCAL_SECRET_VISIBLE_${cycle.number}`, 180_000);
      await show.waitForMarker(`E2E: ANDROID_LOCAL_SECRET_CLOSED_${cycle.number}`, 60_000);
      await show.result;
    } else {
      throw new Error(`Test #${testNumber} direct Show does not support sender ${sender}`);
    }
    writeDiagnostic(
      `[TEST${testNumber}] cycle=${cycle.number} kind=direct-show sender=${sender} `
        + `secret=${secretName} exactValue=verified`,
    );
    console.log(`✅ Test #${testNumber} cycle ${cycle.number}/${expectedCycles} passed: ${sender} local Show`);
  }

  const actualJoinBoundary = directShowCycles.at(-1)?.number ?? 0;
  if (actualJoinBoundary !== scenario.recovery.joinIosAfterCycle) {
    throw new Error(
      `Test #${testNumber} iOS JOIN boundary must follow cycle ${scenario.recovery.joinIosAfterCycle}`,
    );
  }

  // C=iOS joins at the declared completed-cycle boundary and must receive both
  // existing Secrets before the three-device recovery matrix starts.
  await runWebMultiSecretLifecycleIosJoin(page, simulatorUdid, twoDeviceNames, androidSerial);
  writeDiagnostic(
    `[TEST${testNumber}] iOS joined after cycle ${actualJoinBoundary} and received Secret1/Secret2`,
  );

  const startNativeStep = async (platform, role, cycle, secretName, approvalPlatform, sender) => {
    const environment = stepEnvironment({
      role,
      cycle: cycle.number,
      step: 1,
      approvalPlatform,
      sender,
      verifyReceiverAlert: verifyBothReceiverAlerts,
      secretName,
    });
    environment.E2E_SECRET_NAME = secretName;
    environment.E2E_SECRET_VALUE = secretValueForName(secretName);
    if (platform === 'ios') {
      const test = startIosJoinTest(simulatorUdid, {
        testClass: scenario.ios.stepTestClass
          ?? scenario.ios.testClass
          ?? 'CaseEightIosBothOfflineUITest',
        testMethod: scenario.ios.stepTestMethod ?? 'handleRecoveryStep',
        label: `iOS Test #${testNumber} ${role} cycle ${cycle.number}`,
        environment,
        secretName,
        secretValue: secretValueForName(secretName),
      });
      void test.result.catch(() => {});
      return test;
    }
    return startAndroidStepTest(androidSerial, {
      testClass: scenario.android.stepTestClass ?? scenario.android.testClass,
      testMethod: scenario.android.stepTestMethod ?? 'handleRecoveryStep',
      role,
      cycle: cycle.number,
      step: 1,
      approvalPlatform,
      sender,
      verifyReceiverAlert: verifyBothReceiverAlerts,
      secretName,
      secretValue: secretValueForName(secretName),
      label: `Android Test #${testNumber} ${role} cycle ${cycle.number}`,
    });
  };

  for (const cycle of recoveryOnlyCycles) {
    setCurrentCycleForDiagnostics(cycle);
    const sender = cycle.senders[0];
    const approval = cycle.approvals[0];
    const approver = approval?.platform;
    const secretName = cycle.senderSecrets[sender] ?? approval?.secret ?? defaultSecret?.name;
    if (!['web', 'android', 'ios'].includes(sender)) {
      throw new Error(`Test #${testNumber} cycle ${cycle.number} has unsupported sender ${sender}`);
    }
    if (!['web', 'android', 'ios'].includes(approver) || sender === approver) {
      throw new Error(`Test #${testNumber} cycle ${cycle.number} has invalid approver ${approver}`);
    }
    const observer = ['web', 'android', 'ios']
      .find((platform) => platform !== sender && platform !== approver);
    if (!observer) throw new Error(`Test #${testNumber} cycle ${cycle.number} has no observer`);
    if (cycle.observer && cycle.observer !== observer) {
      throw new Error(
        `Test #${testNumber} cycle ${cycle.number} observer mismatch: `
          + `scenario=${cycle.observer} derived=${observer}`,
      );
    }

    // Create Secret3 at the scenario boundary and wait until Web observes the
    // persisted row. Native sender/receiver steps independently wait for it.
    if (cycle.number === scenario.recovery.createSecret3AfterCycle + 1) {
      if (thirdSecretCreator === 'android') {
        await createAndroidSecret(
          thirdName,
          scenario.recovery.createSecret3AfterCycle,
          `Android Test #${testNumber} creates Secret3`,
        );
      } else if (thirdSecretCreator === 'ios') {
        await createIosSecret(
          thirdName,
          scenario.recovery.createSecret3AfterCycle,
          `iOS Test #${testNumber} creates Secret3`,
        );
        if (testNumber === 34 && scenario.android.redistributionCheckTestMethod) {
          const androidVisibility = await startAndroidStepTest(androidSerial, {
            testClass: scenario.android.lifecycleTestClass
              ?? scenario.android.creatorTestClass
              ?? scenario.android.testClass,
            testMethod: scenario.android.redistributionCheckTestMethod,
            role: 'redistribution-check',
            cycle: scenario.recovery.createSecret3AfterCycle,
            step: 1,
            secretName: thirdName,
            secretValue: secretValueForName(thirdName),
            label: `Android Test #${testNumber} verifies iOS-created Secret3`,
          });
          await androidVisibility.waitForMarker(
            `E2E: ANDROID_TEST${testNumber}_SECRET_VISIBLE_${scenario.recovery.createSecret3AfterCycle}`,
            180_000,
          );
          await androidVisibility.result;
          writeDiagnostic(
            `[TEST${testNumber}] Android observed iOS-created Secret3 before cycle ${cycle.number}`,
          );
        }
      } else {
        await createWebSecret(page, thirdName);
        await waitForWebSecretNames(page, [...twoDeviceNames, thirdName]);

        const androidVisibility = await startAndroidStepTest(androidSerial, {
          testClass: scenario.android.lifecycleTestClass
            ?? scenario.android.creatorTestClass
            ?? scenario.android.testClass,
          testMethod: scenario.android.redistributionCheckTestMethod,
          role: 'redistribution-check',
          cycle: scenario.recovery.createSecret3AfterCycle,
          step: 1,
          secretName: thirdName,
          label: `Android Test #${testNumber} verifies Web-created Secret3`,
        });
        await androidVisibility.waitForMarker(
          `E2E: ANDROID_TEST${testNumber}_SECRET_VISIBLE_${scenario.recovery.createSecret3AfterCycle}`,
          180_000,
        );
        await androidVisibility.result;

        const iosVisibility = startIosJoinTest(simulatorUdid, {
          testClass: scenario.ios.testClass ?? 'CaseEightIosBothOfflineUITest',
          testMethod: scenario.ios.redistributionCheckTestMethod,
          label: `iOS Test #${testNumber} verifies Web-created Secret3`,
          environment: stepEnvironment({
            role: 'redistribution-check',
            cycle: scenario.recovery.createSecret3AfterCycle,
            step: 1,
            secretName: thirdName,
          }),
          secretName: thirdName,
          secretValue: secretValueForName(thirdName),
        });
        void iosVisibility.result.catch(() => {});
        await iosVisibility.waitForMarker(
          `E2E: IOS_TEST${testNumber}_SECRET_VISIBLE_${scenario.recovery.createSecret3AfterCycle}`,
          180_000,
        );
        await iosVisibility.result;
        writeDiagnostic(
          `[TEST${testNumber}] Android and iOS observed Web-created Secret3 before `
            + `cycle ${cycle.number}`,
        );
      }
      await waitForWebSecretNames(page, [...twoDeviceNames, thirdName]);
      writeDiagnostic(
        `[TEST${testNumber}] ${thirdSecretCreator} created Secret3 after completed `
          + `cycle ${scenario.recovery.createSecret3AfterCycle}`,
      );
    }

    writeDiagnostic(
      `[TEST${testNumber}][${cycle.block}][cycle=${cycle.number}] `
        + `kind=recovery sender=${sender} approver=${approver} `
        + `observer=${observer} secret=${secretName}`,
    );

    let senderTest;
    let approverTest;
    let observerTest;
    if (approver !== 'web') {
      approverTest = await startNativeStep(approver, 'receiver', cycle, secretName, approver, sender);
    }
    if (observer !== 'web') {
      observerTest = await startNativeStep(observer, 'receiver', cycle, secretName, approver, sender);
    }
    if (sender !== 'web') {
      senderTest = await startNativeStep(sender, 'sender', cycle, secretName, approver, sender);
    }

    await waitForRecoveryParticipantsReady(page, {
      cycleNumber: cycle.number,
      secretName,
      participants: [
        { platform: approver, role: 'receiver', test: approverTest },
        { platform: observer, role: 'receiver', test: observerTest },
        { platform: sender, role: 'sender', test: senderTest },
      ],
      label: `TEST${testNumber}:${cycle.block}`,
    });

    if (sender === 'web') {
      approvalCoordinator.allow('web-sender', cycle.number);
      await startWebRecovery(page, secretName);
    } else {
      approvalCoordinator.allow(`${sender}-sender-1`, cycle.number);
      await senderTest.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_REQUEST_SENT_${cycle.number}_1`,
        180_000,
      );
    }

    // Both receivers must expose the same pending Claim before the selected
    // approver can act. This detects lost delivery on the observer rather than
    // allowing a fast approval to hide it.
    const waitForReceiverVisible = async (platform, test) => {
      if (platform === 'web') {
        if (verifyBothReceiverAlerts) {
          await openStrictWebIncomingAlert(page, 1, secretName, {
            dismissWithoutDecision: platform === observer,
          });
        } else {
          await waitForStrictWebIncomingVisible(page, 1, secretName);
        }
        writeDiagnostic(
          `[TEST${testNumber}][${cycle.block}][cycle=${cycle.number}] `
            + 'Web receiver badge/open action visible',
        );
        return;
      }
      await test.waitForMarker(
        verifyBothReceiverAlerts
          ? `E2E: ${platform.toUpperCase()}_INCOMING_ALERT_VISIBLE_${cycle.number}_1`
          : `E2E: ${platform.toUpperCase()}_INCOMING_VISIBLE_${cycle.number}_1`,
        180_000,
      );
      if (verifyBothReceiverAlerts && platform === observer) {
        await test.waitForMarker(
          `E2E: ${platform.toUpperCase()}_INCOMING_ALERT_DISMISSED_WITHOUT_DECISION_${cycle.number}_1`,
          60_000,
        );
      }
    };
    await Promise.all([
      waitForReceiverVisible(approver, approverTest),
      waitForReceiverVisible(observer, observerTest),
    ]);
    writeDiagnostic(
      `[TEST${testNumber}][${cycle.block}][cycle=${cycle.number}] bothReceiversVisible=true`,
    );

    if (approver === 'web') {
      if (verifyBothReceiverAlerts) {
        await approveOpenIncomingRecoveryOnWeb(page, secretName);
      } else {
        await approveIncomingRecoveryOnWeb(page, secretName, 1);
      }
    } else {
      approvalCoordinator.allow(`${approver}-approve-1`, cycle.number);
      await approverTest.waitForMarker(
        `E2E: ${approver.toUpperCase()}_APPROVED_INCOMING_${cycle.number}_1`,
        180_000,
      );
    }

    if (observer === 'web') {
      await waitForWebIncomingRecoveryGone(page, secretName);
    } else {
      approvalCoordinator.allow(`${observer}-dismiss-1`, cycle.number);
      await observerTest.waitForMarker(
        `E2E: ${observer.toUpperCase()}_DISMISSED_INCOMING_${cycle.number}_1`,
        180_000,
      );
    }
    writeDiagnostic(
      `[TEST${testNumber}][${cycle.block}][cycle=${cycle.number}] `
        + `observer=${observer} requestGone=true`,
    );

    if (sender === 'web') {
      await revealAndCloseWebSecret(page, secretName, false);
    } else {
      approvalCoordinator.allow(`${sender}-show-1`, cycle.number);
      await senderTest.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_SECRET_VISIBLE_${cycle.number}_1`,
        recoveryShowTimeoutMs,
      );
      await senderTest.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_CLOSED_${cycle.number}_1`,
        60_000,
      );
    }

    if (senderTest) await senderTest.result;
    if (approverTest) await approverTest.result;
    if (observerTest) await observerTest.result;

    writeDiagnostic(
      `[TEST${testNumber}][${cycle.block}][cycle=${cycle.number}] sender=${sender} `
        + 'exactValue=verified action=closed',
    );
    page = await refreshWebForKnownNames(page, cycle.number, [
      ...twoDeviceNames,
      ...(cycle.number > scenario.recovery.createSecret3AfterCycle ? [thirdName] : []),
    ]);
    console.log(`✅ Test #${testNumber} cycle ${cycle.number}/${expectedCycles} passed: ${sender} recovery/show`);
    if (Number.isInteger(diagnosticStopAfterCycle) && cycle.number >= diagnosticStopAfterCycle) {
      writeDiagnostic(
        `[TEST${testNumber}][DIAGNOSTIC] requested stop after completed cycle=${cycle.number}`,
      );
      return { diagnosticStop: true, completedCycles: cycle.number };
    }
  }

  return { diagnosticStop: false, completedCycles: expectedCycles };
}

async function runAndroidSecondaryCreatorLifecycle(page, simulatorUdid, androidSerial) {
  const initialName = secretCreationPlan?.initial?.android?.[0] ?? secretNameFor('secret1');
  const secondarySecretCreator = scenario.setup?.secondarySecretCreator ?? 'ios';
  const secondName = secretCreationPlan?.afterIosJoin?.[secondarySecretCreator]?.[0]
    ?? secretNameFor('secret2');
  const thirdSecretCreator = scenario.setup?.thirdSecretCreator ?? 'ios';
  const secondJoinApprover = scenario.setup?.secondJoinApprover ?? 'android';
  const thirdName = secretCreationPlan?.afterRecovery?.[thirdSecretCreator]?.[0]
    ?? secretNameFor('secret3');
  const twoDeviceNames = [initialName, secondName];
  const expectedCycles = scenario.recovery.expectedCycles ?? recoveryCycles.length;
  const directShowCycles = recoveryCycles.filter((cycle) => cycle.kind === 'direct-show');
  const recoveryOnlyCycles = recoveryCycles.filter((cycle) => cycle.kind !== 'direct-show');
  const expectedDirectShowCycles = scenario.recovery.expectedDirectShowCycles ?? 12;
  const expectedRecoveryCycles = scenario.recovery.expectedRecoveryCycles ?? 27;
  const verifyBothReceiverAlerts = scenario.recovery.verifyBothReceiverAlerts === true;

  if (recoveryCycles.length !== expectedCycles) {
    throw new Error(`Test #${testNumber} expected ${expectedCycles} cycles, got ${recoveryCycles.length}`);
  }
  if (directShowCycles.length !== expectedDirectShowCycles) {
    throw new Error(
      `Test #${testNumber} expected ${expectedDirectShowCycles} direct Show cycles, got ${directShowCycles.length}`,
    );
  }
  if (recoveryOnlyCycles.length !== expectedRecoveryCycles) {
    throw new Error(
      `Test #${testNumber} expected ${expectedRecoveryCycles} recovery cycles, got ${recoveryOnlyCycles.length}`,
    );
  }
  if ((scenario.setup?.joinOrder ?? []).join(',') !== 'ios,web') {
    throw new Error(`Test #${testNumber} requires setup.joinOrder=[ios,web]`);
  }
  if (!['android', 'ios'].includes(secondarySecretCreator)) {
    throw new Error(
      `Test #${testNumber} requires Android or iOS as the secondary Secret creator, `
        + `got ${secondarySecretCreator}`,
    );
  }
  if (!['android', 'ios', 'web'].includes(thirdSecretCreator)) {
    throw new Error(
      `Test #${testNumber} requires Android, iOS, or Web as the third Secret creator, `
        + `got ${thirdSecretCreator}`,
    );
  }
  if (!['android', 'ios'].includes(secondJoinApprover)) {
    throw new Error(
      `Test #${testNumber} requires Android or iOS as the second Join approver, `
        + `got ${secondJoinApprover}`,
    );
  }
  if (testNumber === 35) {
    if (secondarySecretCreator !== 'ios') {
      throw new Error(`Test #35 requires iOS as the secondary Secret creator`);
    }
    if (thirdSecretCreator !== 'web') {
      throw new Error(`Test #35 requires Web as the third Secret creator`);
    }
    if (secondJoinApprover !== 'ios') {
      throw new Error(`Test #35 requires iOS as the sole second Join approver`);
    }
  }

  const expectedRoleCount = scenario.recovery.expectedRoleCountPerPlatform;
  if (Number.isInteger(expectedRoleCount)) {
    const roleCounts = Object.fromEntries(
      ['web', 'android', 'ios'].map((platform) => [platform, { sender: 0, approver: 0, observer: 0 }]),
    );
    for (const cycle of recoveryOnlyCycles) {
      if (testNumber === 35 && (cycle.senders.length !== 1 || cycle.approvals.length !== 1)) {
        throw new Error(
          `Test #35 recovery cycle ${cycle.number} must have exactly one sender and approver`,
        );
      }
      const sender = cycle.senders[0];
      const approver = cycle.approvals[0]?.platform;
      const observer = ['web', 'android', 'ios']
        .find((platform) => platform !== sender && platform !== approver);
      if (roleCounts[sender]) roleCounts[sender].sender += 1;
      if (roleCounts[approver]) roleCounts[approver].approver += 1;
      if (roleCounts[observer]) roleCounts[observer].observer += 1;
      if (cycle.observer && cycle.observer !== observer) {
        throw new Error(
          `Test #${testNumber} cycle ${cycle.number} observer mismatch: `
            + `scenario=${cycle.observer} derived=${observer}`,
        );
      }
    }
    for (const [platform, counts] of Object.entries(roleCounts)) {
      for (const [role, count] of Object.entries(counts)) {
        if (count !== expectedRoleCount) {
          throw new Error(
            `Test #${testNumber} expected ${platform} ${role}=${expectedRoleCount}, got ${count}`,
          );
        }
      }
    }
  }

  // A=Android creates the Vault and Secret1 through the visible Compose UI.
  setJoinExpectedSecretNames([initialName]);
  const androidSetup = await startAndroidJoinTest(androidSerial, {
    testClass: scenario.android.initiatorSetupTestClass,
    testMethod: scenario.android.initiatorSetupTestMethod,
    expectedSecretNames: [initialName],
  });
  await androidSetup.waitForMarker(
    `E2E: ANDROID_TEST${testNumber}_INITIATOR_SETUP_READY`,
    180_000,
  );
  await androidSetup.result;
  writeDiagnostic(`[TEST${testNumber}][setup] Android Vault and Secret1 persisted`);

  // B=iOS joins first. The Android owner action starts only after iOS emits
  // the completed request marker, so no time-based join synchronization is
  // needed.
  const iosJoin = startIosJoinTest(simulatorUdid, {
    testClass: scenario.ios.joinTestClass
      ?? scenario.ios.testClass
      ?? 'CaseEightIosBothOfflineUITest',
    testMethod: scenario.ios.joinTestMethod ?? 'joinWebInitiatedVault',
    label: `iOS Test #${testNumber} joins Android Vault`,
  });
  void iosJoin.result.catch(() => {});
  await iosJoin.waitForMarker('E2E: IOS_JOIN_REQUEST_SENT', 300_000);
  const iosApproval = await startAndroidStepTest(androidSerial, {
    testClass: scenario.android.initiatorSetupTestClass,
    testMethod: scenario.android.approveJoinTestMethod ?? 'approvePendingJoinForTest26',
    role: 'owner',
    cycle: 0,
    step: 1,
    joinTarget: 'ios',
    label: `Android Test #${testNumber} approves iOS join`,
  });
  await iosApproval.waitForMarker(
    `E2E: ANDROID_TEST${testNumber}_JOIN_APPROVED_IOS`,
    180_000,
  );
  await iosJoin.waitForMarker('E2E: IOS_JOIN_READY', 180_000);
  await Promise.all([iosJoin.result, iosApproval.result]);
  writeDiagnostic(`[TEST${testNumber}][setup] iOS joined Android Vault and received Secret1`);

  const createIosSecret = async (secretName, cycle, label) => {
    const creation = startIosJoinTest(simulatorUdid, {
      testClass: scenario.ios.testClass ?? 'CaseEightIosBothOfflineUITest',
      testMethod: scenario.ios.createSecretTestMethod ?? `createSecretForTest${testNumber}`,
      label,
      environment: stepEnvironment({ role: 'creator', cycle, step: 1, secretName }),
      secretName,
      secretValue: secretValueForName(secretName),
    });
    void creation.result.catch(() => {});
    await creation.waitForMarker(`E2E: IOS_TEST${testNumber}_SECRET_CREATED_${cycle}`, 180_000);
    await creation.result;
    writeDiagnostic(`[TEST${testNumber}][create] iOS persisted ${secretName} boundaryCycle=${cycle}`);
  };

  const createAndroidSecret = async (secretName, cycle, label) => {
    const creation = await startAndroidStepTest(androidSerial, {
      testClass: scenario.android.creatorTestClass
        ?? scenario.android.initiatorSetupTestClass,
      testMethod: scenario.android.createSecretTestMethod
        ?? `createSecretForTest${testNumber}`,
      role: 'creator',
      cycle,
      step: 1,
      secretName,
      secretValue: secretValueForName(secretName),
      label,
    });
    await creation.waitForMarker(
      `E2E: ANDROID_TEST${testNumber}_SECRET_CREATED_${cycle}`,
      180_000,
    );
    await creation.result;
    writeDiagnostic(
      `[TEST${testNumber}][create] Android persisted ${secretName} boundaryCycle=${cycle}`,
    );
  };

  const assertAndroidSecretVisible = async (secretName, boundaryCycle) => {
    const testMethod = scenario.android.redistributionCheckTestMethod;
    if (!testMethod) return;
    const assertion = await startAndroidStepTest(androidSerial, {
      testClass: scenario.android.initiatorSetupTestClass,
      testMethod,
      role: 'redistribution-observer',
      cycle: boundaryCycle,
      step: 1,
      secretName,
      label: `Android Test #${testNumber} observes redistributed ${secretName}`,
    });
    await assertion.waitForMarker(
      `E2E: ANDROID_TEST${testNumber}_SECRET_VISIBLE_${boundaryCycle}`,
      180_000,
    );
    await assertion.result;
    writeDiagnostic(
      `[TEST${testNumber}][redistribution] Android observed ${secretName} boundary=${boundaryCycle}`,
    );
  };

  const assertIosSecretVisible = async (secretName, boundaryCycle) => {
    const testMethod = scenario.ios.redistributionCheckTestMethod;
    if (!testMethod) return;
    const assertion = startIosJoinTest(simulatorUdid, {
      testClass: scenario.ios.testClass ?? 'CaseEightIosBothOfflineUITest',
      testMethod,
      label: `iOS Test #${testNumber} observes redistributed ${secretName}`,
      environment: stepEnvironment({
        role: 'redistribution-observer',
        cycle: boundaryCycle,
        step: 1,
        secretName,
      }),
      secretName,
      secretValue: secretValueForName(secretName),
    });
    void assertion.result.catch(() => {});
    await assertion.waitForMarker(
      `E2E: IOS_TEST${testNumber}_SECRET_VISIBLE_${boundaryCycle}`,
      180_000,
    );
    await assertion.result;
    writeDiagnostic(
      `[TEST${testNumber}][redistribution] iOS observed ${secretName} boundary=${boundaryCycle}`,
    );
  };

  // Optional run-specific boundary assertion proves that iOS completed its
  // first Join with Secret1 already visible before Android creates Secret2.
  if (scenario.ios.firstJoinCheckTestMethod) {
    await assertIosSecretVisible(
      initialName,
      1,
      scenario.ios.firstJoinCheckTestMethod,
    );
  }

  // Secret2 is created before either the optional two-device Show phase or
  // the immediate Web JOIN used by the distributed-creator rotations.
  if (secondarySecretCreator === 'ios') {
    await createIosSecret(secondName, 0, `iOS Test #${testNumber} creates Secret2`);
  } else {
    await createAndroidSecret(secondName, 0, `Android Test #${testNumber} creates Secret2`);
  }
  setJoinExpectedSecretNames(twoDeviceNames);
  if (secondarySecretCreator === 'ios') {
    await assertAndroidSecretVisible(secondName, 2);
  } else {
    // This completes before Web submits its Join, proving iOS has Android's
    // Secret2 before iOS is allowed to approve the new member.
    await assertIosSecretVisible(secondName, 2);
  }

  for (const cycle of directShowCycles) {
    setCurrentCycleForDiagnostics(cycle);
    const sender = cycle.senders[0];
    const secretName = cycle.senderSecrets[sender];
    if (cycle.approvals.length !== 0) {
      throw new Error(`Test #${testNumber} direct Show cycle ${cycle.number} must not have an approver`);
    }

    if (sender === 'android') {
      const show = await startAndroidStepTest(androidSerial, {
        testClass: scenario.android.initiatorSetupTestClass,
        testMethod: scenario.android.directShowTestMethod ?? `showLocalSecretForTest${testNumber}`,
        role: 'show',
        cycle: cycle.number,
        step: 1,
        secretName,
        secretValue: secretValueForName(secretName),
        label: `Android Test #${testNumber} local Show cycle ${cycle.number}`,
      });
      await show.waitForMarker(
        `E2E: ANDROID_TEST${testNumber}_LOCAL_SECRET_VALUE_VERIFIED_${cycle.number}`,
        180_000,
      );
      await show.waitForMarker(
        `E2E: ANDROID_TEST${testNumber}_LOCAL_SECRET_VISIBLE_${cycle.number}`,
        180_000,
      );
      await show.waitForMarker(
        `E2E: ANDROID_TEST${testNumber}_LOCAL_SECRET_CLOSED_${cycle.number}`,
        60_000,
      );
      await show.result;
    } else if (sender === 'ios') {
      const show = startIosJoinTest(simulatorUdid, {
        testClass: scenario.ios.testClass ?? 'CaseEightIosBothOfflineUITest',
        testMethod: scenario.ios.directShowTestMethod ?? `showLocalSecretForTest${testNumber}`,
        label: `iOS Test #${testNumber} local Show cycle ${cycle.number}`,
        environment: {
          ...stepEnvironment({ role: 'show', cycle: cycle.number, step: 1, secretName }),
          E2E_SECRET_NAME: secretName,
          E2E_SECRET_VALUE: secretValueForName(secretName),
        },
        secretName,
        secretValue: secretValueForName(secretName),
      });
      void show.result.catch(() => {});
      await show.waitForMarker(
        `E2E: IOS_TEST${testNumber}_LOCAL_SECRET_VALUE_VERIFIED_${cycle.number}`,
        180_000,
      );
      await show.waitForMarker(
        `E2E: IOS_TEST${testNumber}_LOCAL_SECRET_VISIBLE_${cycle.number}`,
        180_000,
      );
      await show.waitForMarker(
        `E2E: IOS_TEST${testNumber}_LOCAL_SECRET_CLOSED_${cycle.number}`,
        60_000,
      );
      await show.result;
    } else {
      throw new Error(`Test #${testNumber} direct Show does not support sender ${sender}`);
    }

    writeDiagnostic(
      `[TEST${testNumber}][${cycle.block}][cycle=${cycle.number}] sender=${sender} `
        + `secret=${secretName} exactValue=verified action=closed`,
    );
    console.log(`✅ Test #${testNumber} cycle ${cycle.number}/${expectedCycles} passed: ${sender} local Show`);
  }

  const actualJoinBoundary = directShowCycles.at(-1)?.number ?? 0;
  if (actualJoinBoundary !== scenario.recovery.joinWebAfterCycle) {
    throw new Error(
      `Test #${testNumber} Web JOIN boundary must follow cycle ${scenario.recovery.joinWebAfterCycle}`,
    );
  }

  // C=Web joins at the declared completed-cycle boundary. The scenario names
  // the already-joined device that must approve; the other device never gets
  // an approval action from this harness. Web must then render both existing
  // Secrets before recovery starts.
  await submitWebJoinRequest(page);
  if (secondJoinApprover === 'android') {
    const webApproval = await startAndroidStepTest(androidSerial, {
      testClass: scenario.android.initiatorSetupTestClass,
      testMethod: scenario.android.approveJoinTestMethod ?? 'approvePendingJoinForTest26',
      role: 'owner',
      cycle: 0,
      step: 1,
      joinTarget: 'web',
      label: `Android Test #${testNumber} approves Web join`,
    });
    await webApproval.waitForMarker(
      `E2E: ANDROID_TEST${testNumber}_JOIN_APPROVED_WEB`,
      180_000,
    );
    await webApproval.result;
  } else {
    const webApproval = startIosJoinTest(simulatorUdid, {
      testClass: scenario.ios.testClass ?? 'CaseEightIosBothOfflineUITest',
      testMethod: scenario.ios.approveJoinTestMethod
        ?? `approvePendingJoinForTest${testNumber}`,
      label: `iOS Test #${testNumber} approves Web join`,
      environment: stepEnvironment({
        role: 'join-approver',
        cycle: 0,
        step: 1,
        sender: 'web',
        secretName: initialName,
      }),
    });
    void webApproval.result.catch(() => {});
    await webApproval.waitForMarker(
      `E2E: IOS_TEST${testNumber}_JOIN_APPROVED_WEB`,
      180_000,
    );
    await webApproval.result;
  }
  await page.reload({ waitUntil: 'domcontentloaded' });
  await unlockWithPasskeyIfNeeded(page);
  await waitForWebSecretNames(page, twoDeviceNames);
  writeDiagnostic(
    `[TEST${testNumber}][setup] Web joined after cycle ${actualJoinBoundary} `
      + 'and received Secret1/Secret2',
  );

  const startNativeStep = async (platform, role, cycle, secretName, approvalPlatform, sender) => {
    const environment = stepEnvironment({
      role,
      cycle: cycle.number,
      step: 1,
      approvalPlatform,
      sender,
      verifyReceiverAlert: verifyBothReceiverAlerts,
      secretName,
    });
    environment.E2E_SECRET_NAME = secretName;
    environment.E2E_SECRET_VALUE = secretValueForName(secretName);
    if (platform === 'ios') {
      const test = startIosJoinTest(simulatorUdid, {
        testClass: scenario.ios.stepTestClass
          ?? scenario.ios.testClass
          ?? 'CaseEightIosBothOfflineUITest',
        testMethod: scenario.ios.stepTestMethod ?? 'handleRecoveryStep',
        label: `iOS Test #${testNumber} ${role} cycle ${cycle.number}`,
        environment,
        secretName,
        secretValue: secretValueForName(secretName),
      });
      void test.result.catch(() => {});
      return test;
    }
    return startAndroidStepTest(androidSerial, {
      testClass: scenario.android.stepTestClass ?? scenario.android.testClass,
      testMethod: scenario.android.stepTestMethod ?? 'handleRecoveryStep',
      role,
      cycle: cycle.number,
      step: 1,
      approvalPlatform,
      sender,
      verifyReceiverAlert: verifyBothReceiverAlerts,
      secretName,
      secretValue: secretValueForName(secretName),
      label: `Android Test #${testNumber} ${role} cycle ${cycle.number}`,
    });
  };

  for (const cycle of recoveryOnlyCycles) {
    setCurrentCycleForDiagnostics(cycle);
    const sender = cycle.senders[0];
    const approval = cycle.approvals[0];
    const approver = approval?.platform;
    const secretName = cycle.senderSecrets[sender] ?? approval?.secret ?? defaultSecret?.name;
    if (!['web', 'android', 'ios'].includes(sender)) {
      throw new Error(`Test #${testNumber} cycle ${cycle.number} has unsupported sender ${sender}`);
    }
    if (!['web', 'android', 'ios'].includes(approver) || sender === approver) {
      throw new Error(`Test #${testNumber} cycle ${cycle.number} has invalid approver ${approver}`);
    }
    const observer = ['web', 'android', 'ios']
      .find((platform) => platform !== sender && platform !== approver);
    if (!observer) throw new Error(`Test #${testNumber} cycle ${cycle.number} has no observer`);
    if (cycle.observer !== observer) {
      throw new Error(
        `Test #${testNumber} cycle ${cycle.number} observer mismatch: `
          + `scenario=${cycle.observer} derived=${observer}`,
      );
    }

    if (cycle.number === scenario.recovery.createSecret3AfterCycle + 1) {
      if (thirdSecretCreator === 'ios') {
        await createIosSecret(
          thirdName,
          scenario.recovery.createSecret3AfterCycle,
          `iOS Test #${testNumber} creates Secret3`,
        );
      } else if (thirdSecretCreator === 'android') {
        await createAndroidSecret(
          thirdName,
          scenario.recovery.createSecret3AfterCycle,
          `Android Test #${testNumber} creates Secret3`,
        );
      } else {
        await createWebSecret(page, thirdName);
        writeDiagnostic(
          `[TEST${testNumber}][create] Web persisted ${thirdName} `
            + `boundaryCycle=${scenario.recovery.createSecret3AfterCycle}`,
        );
      }
      await waitForWebSecretNames(page, [...twoDeviceNames, thirdName]);
      if (thirdSecretCreator === 'web') {
        await assertAndroidSecretVisible(thirdName, 3);
        await assertIosSecretVisible(thirdName, 3);
      } else if (thirdSecretCreator === 'android') {
        await assertIosSecretVisible(
          thirdName,
          scenario.recovery.createSecret3AfterCycle,
        );
      }
      writeDiagnostic(
        `[TEST${testNumber}][create] ${thirdSecretCreator} created Secret3 after completed `
          + `cycle ${scenario.recovery.createSecret3AfterCycle}`,
      );
    }

    writeDiagnostic(
      `[TEST${testNumber}][${cycle.block}][cycle=${cycle.number}] sender=${sender} `
        + `approver=${approver} observer=${observer} secret=${secretName}`,
    );

    let senderTest;
    let approverTest;
    let observerTest;
    if (approver !== 'web') {
      approverTest = await startNativeStep(approver, 'receiver', cycle, secretName, approver, sender);
    }
    if (observer !== 'web') {
      observerTest = await startNativeStep(observer, 'receiver', cycle, secretName, approver, sender);
    }
    if (sender !== 'web') {
      senderTest = await startNativeStep(sender, 'sender', cycle, secretName, approver, sender);
    }

    await waitForRecoveryParticipantsReady(page, {
      cycleNumber: cycle.number,
      secretName,
      participants: [
        { platform: approver, role: 'receiver', test: approverTest },
        { platform: observer, role: 'receiver', test: observerTest },
        { platform: sender, role: 'sender', test: senderTest },
      ],
      label: `TEST${testNumber}:${cycle.block}`,
    });

    if (sender === 'web') {
      approvalCoordinator.allow('web-sender', cycle.number);
      await startWebRecovery(page, secretName);
    } else {
      approvalCoordinator.allow(`${sender}-sender-1`, cycle.number);
      await senderTest.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_REQUEST_SENT_${cycle.number}_1`,
        180_000,
      );
    }

    const waitForReceiverVisible = async (platform, test) => {
      if (platform === 'web') {
        if (verifyBothReceiverAlerts) {
          await openStrictWebIncomingAlert(page, 1, secretName, {
            dismissWithoutDecision: platform === observer,
          });
        } else {
          await waitForStrictWebIncomingVisible(page, 1, secretName);
        }
        return;
      }
      await test.waitForMarker(
        verifyBothReceiverAlerts
          ? `E2E: ${platform.toUpperCase()}_INCOMING_ALERT_VISIBLE_${cycle.number}_1`
          : `E2E: ${platform.toUpperCase()}_INCOMING_VISIBLE_${cycle.number}_1`,
        180_000,
      );
      if (verifyBothReceiverAlerts && platform === observer) {
        await test.waitForMarker(
          `E2E: ${platform.toUpperCase()}_INCOMING_ALERT_DISMISSED_WITHOUT_DECISION_${cycle.number}_1`,
          60_000,
        );
      }
    };
    await Promise.all([
      waitForReceiverVisible(approver, approverTest),
      waitForReceiverVisible(observer, observerTest),
    ]);
    writeDiagnostic(
      `[TEST${testNumber}][${cycle.block}][cycle=${cycle.number}] bothReceiversVisible=true`,
    );

    if (approver === 'web') {
      if (verifyBothReceiverAlerts) {
        await approveOpenIncomingRecoveryOnWeb(page, secretName);
      } else {
        await approveIncomingRecoveryOnWeb(page, secretName, 1);
      }
    } else {
      approvalCoordinator.allow(`${approver}-approve-1`, cycle.number);
      await approverTest.waitForMarker(
        `E2E: ${approver.toUpperCase()}_APPROVED_INCOMING_${cycle.number}_1`,
        180_000,
      );
    }

    if (observer === 'web') {
      await waitForWebIncomingRecoveryGone(page, secretName);
    } else {
      approvalCoordinator.allow(`${observer}-dismiss-1`, cycle.number);
      await observerTest.waitForMarker(
        `E2E: ${observer.toUpperCase()}_DISMISSED_INCOMING_${cycle.number}_1`,
        180_000,
      );
    }
    writeDiagnostic(
      `[TEST${testNumber}][${cycle.block}][cycle=${cycle.number}] observer=${observer} requestGone=true`,
    );

    if (sender === 'web') {
      await revealAndCloseWebSecret(page, secretName, false);
    } else {
      approvalCoordinator.allow(`${sender}-show-1`, cycle.number);
      await senderTest.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_SECRET_VISIBLE_${cycle.number}_1`,
        recoveryShowTimeoutMs,
      );
      await senderTest.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_CLOSED_${cycle.number}_1`,
        60_000,
      );
    }

    if (senderTest) await senderTest.result;
    if (approverTest) await approverTest.result;
    if (observerTest) await observerTest.result;

    writeDiagnostic(
      `[TEST${testNumber}][${cycle.block}][cycle=${cycle.number}] sender=${sender} `
        + `exactValue=verified action=closed`,
    );
    page = await refreshWebForKnownNames(page, cycle.number, [
      ...twoDeviceNames,
      ...(cycle.number > scenario.recovery.createSecret3AfterCycle ? [thirdName] : []),
    ]);
    console.log(`✅ Test #${testNumber} cycle ${cycle.number}/${expectedCycles} passed: ${sender} recovery/show`);
  }
}

async function joinWebToCliVault(page, cliDevice, platform) {
  await page.goto(scenario.web.url, { waitUntil: 'domcontentloaded' });
  await unlockWithPasskeyIfNeeded(page);
  await page.getByPlaceholder('vault name').fill(scenario.vault.name);
  await page.getByRole('button', { name: 'Set Vault Name' }).click();
  await page.getByRole('button', { name: 'Join', exact: true }).waitFor({ timeout: 120_000 });
  await page.getByRole('button', { name: 'Join', exact: true }).click();
  await acceptNextCliJoin(cliDevice, platform);
  // The Web confirmation screen can remain visible while its accepted member
  // state is persisted locally. The CLI acceptance is the authoritative
  // membership boundary; no route transition is needed for this setup step.
  writeDiagnostic(`[TEST21] ${platform} joined CLI-created vault`);
}

async function rejectFourthDeviceOnWeb(page, blockName) {
  await page.goto(scenario.web.url, { waitUntil: 'domcontentloaded' });
  await unlockWithPasskeyIfNeeded(page);
  await page.getByPlaceholder('vault name').fill(scenario.vault.name);
  await page.getByRole('button', { name: 'Set Vault Name' }).click();
  await page.getByRole('button', { name: 'Join', exact: true }).waitFor({ timeout: 120_000 });
  await page.getByRole('button', { name: 'Join', exact: true }).click();
  const error = page.getByTestId('signup-error');
  await error.waitFor({ state: 'visible', timeout: 120_000 });
  await page.screenshot({
    path: resolve(artifactsDirectory, `test-21-${blockName}-web-rejection.png`),
    fullPage: true,
  });
  writeDiagnostic(`[SCREENSHOT] Web saved test-21-${blockName}-web-rejection.png`);
  if (!(await error.innerText()).toLowerCase().includes('maximum of 3 devices')) {
    throw new Error(`Web fourth-device error did not mention the device limit: ${await error.innerText()}`);
  }
  writeDiagnostic('[TEST21] Web fourth device rejected');

  await page.getByRole('button', { name: 'Reset & Create New', exact: true }).click();
  // The reset action clears IndexedDB and reinitializes the WASM manager before
  // routing back to registration. Reload only after the reset control has
  // disappeared so the next interaction cannot race the cleanup/re-init.
  await error.waitFor({ state: 'hidden', timeout: 60_000 });
  await page.reload({ waitUntil: 'domcontentloaded' });
  await unlockWithPasskeyIfNeeded(page);
  await page.getByPlaceholder('vault name').waitFor({ state: 'visible', timeout: 60_000 });
  const retryVaultName = scenario.vault.retryName;
  const retryVaultInput = page.getByPlaceholder('vault name');
  const retrySetVaultButton = page.getByRole('button', { name: 'Set Vault Name' });
  await retryVaultInput.fill(retryVaultName);
  await retrySetVaultButton.waitFor({ state: 'visible', timeout: 30_000 });
  await retrySetVaultButton.click();
  await page.getByText('Vault name is free!', { exact: true }).waitFor({ timeout: 120_000 });
  await page.getByRole('button', { name: 'Create', exact: true }).click();
  await page.getByRole('link', { name: 'Secrets', exact: true }).waitFor({ timeout: 180_000 });
  writeDiagnostic('[TEST21] Web retry registration completed');
}

async function runFourthDeviceRejection(page, simulatorUdid, androidSerial) {
  if (scenario.mode !== 'fourth-device-rejection') return;
  const cliDevice = await createMetaCliRootDevice();
  const members = scenario.memberPlatforms ?? [];
  if (members.length !== 2) throw new Error(`Test #21 requires exactly two approved UI members, got ${members.length}`);

  for (const platform of members) {
    if (platform === 'web') {
      await joinWebToCliVault(page, cliDevice, 'web');
      continue;
    }
    if (platform === 'ios') {
      const iosTest = startIosJoinTest(simulatorUdid, {
        testClass: scenario.ios.testClass,
        testMethod: scenario.ios.joinTestMethod ?? 'joinWebInitiatedVault',
        label: 'iOS Test #21 member join',
      });
      void iosTest.result.catch(() => {});
      await iosTest.waitForMarker('E2E: IOS_JOIN_REQUEST_SENT', 180_000);
      await acceptNextCliJoin(cliDevice, 'ios');
      await iosTest.waitForMarker('E2E: IOS_JOIN_READY', 180_000);
      await iosTest.result;
      continue;
    }
    if (platform === 'android') {
      const androidTest = await startAndroidJoinTest(androidSerial, {
        testClass: scenario.android.testClass,
        testMethod: scenario.android.joinTestMethod ?? 'joinWebInitiatedVault',
      });
      await androidTest.waitForMarker('E2E: ANDROID_JOIN_REQUEST_SENT', 180_000);
      await acceptNextCliJoin(cliDevice, 'android');
      await androidTest.waitForMarker('E2E: ANDROID_JOIN_READY', 180_000);
      await androidTest.result;
      continue;
    }
    throw new Error(`Unsupported Test #21 member platform: ${platform}`);
  }

  const rejected = scenario.rejectionPlatform;
  const blockName = scenario.runId
    ? `${scenario.runId}-attempt-${process.env.E2E_TEST_ATTEMPT ?? '1'}`
    : `block-${rejected}`;
  if (rejected === 'web') {
    await rejectFourthDeviceOnWeb(page, blockName);
  } else if (rejected === 'ios') {
    const iosTest = startIosJoinTest(simulatorUdid, {
      testClass: scenario.ios.testClass,
      testMethod: 'rejectFourthDeviceAndRegisterNewVault',
      label: 'iOS Test #21 fourth-device rejection',
    });
    void iosTest.result.catch(() => {});
    await iosTest.waitForMarker('E2E: IOS_FOURTH_DEVICE_REJECTED', 180_000);
    await captureIosScreenshot(
      simulatorUdid,
      resolve(artifactsDirectory, `test-21-${blockName}-ios-rejection.png`),
    );
    approvalCoordinator.allow('ios-rejection-screenshot', 1);
    await iosTest.waitForMarker('E2E: IOS_FOURTH_DEVICE_RETRY_REGISTERED', 180_000);
    await iosTest.result;
  } else if (rejected === 'android') {
    const androidTest = await startAndroidJoinTest(androidSerial, {
      testClass: scenario.android.testClass,
      testMethod: 'rejectFourthDeviceAndRegisterNewVault',
    });
    await androidTest.waitForMarker('E2E: ANDROID_FOURTH_DEVICE_REJECTED', 180_000);
    await captureAndroidScreenshot(
      androidSerial,
      resolve(artifactsDirectory, `test-21-${blockName}-android-rejection.png`),
    );
    approvalCoordinator.allow('android-rejection-screenshot', 1);
    await androidTest.waitForMarker('E2E: ANDROID_FOURTH_DEVICE_RETRY_REGISTERED', 180_000);
    await androidTest.result;
  } else {
    throw new Error(`Unsupported Test #21 rejected platform: ${rejected}`);
  }
  console.log(`✅ Test #21 ${blockName}: ${rejected} fourth device rejected and retry registered`);
}

async function runReceiverOfflineAfterAlertRecovery(page, simulatorUdid, androidSerial) {
  const expectedCycles = scenario.recovery.expectedCycles ?? recoveryCycles.length;
  if (recoveryCycles.length !== expectedCycles) {
    throw new Error(
      `Test #10 expected ${expectedCycles} recovery requests, got ${recoveryCycles.length}`,
    );
  }

  for (const cycle of recoveryCycles) {
    setCurrentCycleForDiagnostics(cycle);
    if (cycle.senders.length !== 1 || cycle.approvals.length !== 1) {
      throw new Error(
        `Test #10 cycle ${cycle.number} must have exactly one sender and one approver`,
      );
    }

    const sender = cycle.senders[0];
    const approver = cycle.approvals[0].platform;
    const offlineReceiver = cycle.offlineReceiver;
    const secretName = cycle.senderSecrets[sender]
      ?? cycle.approvals[0].secret
      ?? defaultSecret?.name;
    const receivers = ['web', 'ios', 'android'].filter((platform) => platform !== sender);
    if (!offlineReceiver || !receivers.includes(offlineReceiver) || offlineReceiver === approver) {
      throw new Error(
        `Test #10 cycle ${cycle.number} must mark the non-approving receiver as offline `
        + `(sender=${sender}, approver=${approver}, offline=${offlineReceiver})`,
      );
    }

    const step = 1;
    const nativeProcesses = new Map();
    let webSenderRequestPending = false;

    writeDiagnostic(
      `TEST10 cycle=${cycle.number} sender=${sender} secret=${secretName} `
      + `offlineReceiver=${offlineReceiver} approver=${approver}`,
    );
    console.log(
      `Test #10 cycle ${cycle.number}/${expectedCycles}: sender=${sender} `
      + `offlineReceiver=${offlineReceiver} approver=${approver} secret=${secretName}`,
    );

    // Always put the Web app at a known semantic state before using it as a
    // sender, receiver, or approver. Navigation is a state reset, not a
    // timing delay; waitForWebSecrets below confirms that the vault is usable.
    if (receivers.includes('web') || sender === 'web') {
      await reopenWebAfterOffline(page);
    }

    const startNativeStep = async (platform, role) => {
      const environment = stepEnvironment({
        role,
        cycle: cycle.number,
        step,
        approvalPlatform: approver,
        sender,
      });
      if (platform === 'ios') {
        const test = startIosJoinTest(simulatorUdid, {
          testClass: scenario.ios.testClass,
          testMethod: scenario.ios.stepTestMethod ?? 'handleRecoveryStep',
          label: `iOS Test #10 ${role} cycle ${cycle.number}`,
          secretName,
          environment,
        });
        void test.result.catch(() => {});
        nativeProcesses.set(platform, { platform, test, role });
        return;
      }
      if (platform === 'android') {
        const test = await startAndroidStepTest(androidSerial, {
          role,
          cycle: cycle.number,
          step,
          approvalPlatform: approver,
          sender,
          secretName,
          testMethod: scenario.android.stepTestMethod ?? 'handleRecoveryStep',
          label: `Android Test #10 ${role} cycle ${cycle.number}`,
        });
        nativeProcesses.set(platform, { platform, test, role });
        return;
      }
      throw new Error(`Unsupported native Test #10 platform: ${platform}`);
    };

    // Start native participants before releasing the sender gate. This keeps
    // the receiver UI alive before the request is emitted and avoids the
    // missed-first-event race that affected the earlier offline scenarios.
    for (const receiver of receivers) {
      if (receiver !== 'web') await startNativeStep(receiver, 'receiver');
    }
    if (sender !== 'web') {
      await startNativeStep(sender, 'sender');
    }

    await waitForRecoveryParticipantsReady(page, {
      cycleNumber: cycle.number,
      secretName,
      participants: [
        ...receivers.map((platform) => ({
          platform,
          role: 'receiver',
          test: nativeProcesses.get(platform)?.test,
        })),
        { platform: sender, role: 'sender', test: nativeProcesses.get(sender)?.test },
      ],
      label: 'TEST10',
    });

    if (sender !== 'web') {
      approvalCoordinator.allow(`${sender}-sender-${step}`, cycle.number);
    } else {
      approvalCoordinator.allow('web-sender', cycle.number);
      await startWebRecovery(page, secretName);
      webSenderRequestPending = true;
    }

    // The sender marker only means the native recover() action was dispatched;
    // receiver-visible markers are the authoritative boundary before taking a
    // receiver offline. For Web, the incoming badge is the same boundary.
    if (sender !== 'web') {
      await nativeProcesses.get(sender).test.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_REQUEST_SENT_${cycle.number}_${step}`,
        180_000,
      );
    }

    for (const receiver of receivers) {
      if (receiver === 'web') {
        await waitForWebIncomingRecoveryCount(page, 1, secretName);
      } else {
        await nativeProcesses.get(receiver).test.waitForMarker(
          `E2E: ${receiver.toUpperCase()}_INCOMING_VISIBLE_${cycle.number}_${step}`,
          180_000,
        );
      }
    }
    if (webSenderRequestPending) {
      // Keep a Web sender alive until both receivers observed the request. The
      // sender socket may still be carrying the claim persistence event.
      writeDiagnostic(`TEST10 cycle=${cycle.number} web sender request observed by all receivers`);
    }
    console.log(`✅ Test #10 cycle ${cycle.number}: both receiver alerts are visible`);

    // This is the defining Test #10 boundary: the receiver is killed only
    // after its alert has been observed, never before the request arrives.
    if (offlineReceiver === 'android') await stopAndroidApplication(androidSerial);
    if (offlineReceiver === 'ios') await stopIosApplication(simulatorUdid);
    if (offlineReceiver === 'web') await stopWebApplication(page);
    if (offlineReceiver !== 'web') {
      // Force-stopping the app is the scenario boundary. Terminate the
      // corresponding instrumentation wrapper as well so a blocked receiver
      // test cannot retain Gradle/XCTest resources into the next repetition.
      const stoppedReceiver = nativeProcesses.get(offlineReceiver)?.test;
      if (stoppedReceiver?.child && !stoppedReceiver.child.killed) {
        stoppedReceiver.child.kill('SIGTERM');
      }
    }
    writeDiagnostic(`TEST10 cycle=${cycle.number} offline receiver stopped after alert: ${offlineReceiver}`);

    if (approver === 'web') {
      await approveIncomingRecoveryOnWeb(page, secretName, 1);
    } else {
      approvalCoordinator.allow(`${approver}-approve-${step}`, cycle.number);
      await nativeProcesses.get(approver).test.waitForMarker(
        `E2E: ${approver.toUpperCase()}_APPROVED_INCOMING_${cycle.number}_${step}`,
        180_000,
      );
    }
    console.log(`✅ Test #10 cycle ${cycle.number}: ${approver} approved while ${offlineReceiver} was offline`);

    if (sender === 'web') {
      // Web keeps the sender dialog open; after the approver completes it
      // resolves to the recovered value without creating a second claim.
      await revealAndCloseWebSecret(page, secretName, false);
    } else {
      approvalCoordinator.allow(`${sender}-show-${step}`, cycle.number);
      await nativeProcesses.get(sender).test.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_SECRET_VISIBLE_${cycle.number}_${step}`,
        recoveryShowTimeoutMs,
      );
      await nativeProcesses.get(sender).test.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_CLOSED_${cycle.number}_${step}`,
        60_000,
      );
    }

    // Relaunch the receiver that had the visible alert and assert against the
    // persisted state. The assertion checks both native claim state and UI
    // alert/badge state, so a stale local alert cannot hide behind a clean UI.
    if (offlineReceiver === 'android') {
      await runAndroidStaleAlertAssertion(androidSerial);
    } else if (offlineReceiver === 'ios') {
      await runIosStaleAlertAssertion(simulatorUdid);
    } else {
      await reopenWebAfterOffline(page);
      await assertNoStaleWebRecoveryAlert(page, secretName);
    }
    console.log(`✅ Test #10 cycle ${cycle.number}: no stale alert after ${offlineReceiver} restart`);

    // Await only participants that were expected to finish. The intentionally
    // stopped receiver's instrumentation is not awaited because force-stop is
    // its test boundary and produces a non-zero process exit by design.
    for (const [platform, entry] of nativeProcesses.entries()) {
      if (platform === offlineReceiver) continue;
      await entry.test.result;
    }

    // Stop native participants before the next repetition. This prevents a
    // prior Compose/XCTest process from consuming the next cycle's event.
    for (const platform of ['android', 'ios']) {
      if (platform === offlineReceiver || receivers.includes(platform) || sender === platform) {
        if (platform === 'android') await stopAndroidApplication(androidSerial);
        if (platform === 'ios') await stopIosApplication(simulatorUdid);
      }
    }
    if (offlineReceiver === 'web') await stopWebApplication(page);
    console.log(`✅ Test #10 cycle ${cycle.number}/${expectedCycles} passed`);
  }
}

async function runApproveDeclineRaceRecovery(page, simulatorUdid, androidSerial) {
  const expectedCycles = scenario.recovery.expectedCycles ?? recoveryCycles.length;
  if (recoveryCycles.length !== expectedCycles) {
    throw new Error(`Test #${testNumber} expected ${expectedCycles} recovery requests, got ${recoveryCycles.length}`);
  }

  for (const cycle of recoveryCycles) {
    setCurrentCycleForDiagnostics(cycle);
    if (cycle.senders.length !== 1 || cycle.approvals.length !== 2) {
      throw new Error(`Test #${testNumber} cycle ${cycle.number} must have one sender and two ordered approvals`);
    }
    const sender = cycle.senders[0];
    const secretName = cycle.senderSecrets[sender] ?? cycle.approvals[0].secret ?? defaultSecret?.name;
    const receivers = ['web', 'ios', 'android'].filter((platform) => platform !== sender);
    const expectedOutcome = cycle.expectedOutcome ?? (cycle.approvals[0].action === 'approve' ? 'approved' : 'declined');
    const step = 1;
    const nativeProcesses = new Map();
    const first = cycle.approvals[0];
    const second = cycle.approvals[1];

    if (!['approve', 'decline'].includes(first.action) || !['approve', 'decline'].includes(second.action)) {
      throw new Error(`Test #${testNumber} cycle ${cycle.number} actions must be approve or decline`);
    }
    if (new Set(cycle.approvals.map((approval) => approval.platform)).size !== 2) {
      throw new Error(`Test #${testNumber} cycle ${cycle.number} approvals must come from two different receivers`);
    }

    writeDiagnostic(
      `TEST${testNumber} cycle=${cycle.number} sender=${sender} secret=${secretName} `
      + `first=${first.platform}:${first.action} second=${second.platform}:${second.action} `
      + `expected=${expectedOutcome}`,
    );
    console.log(
      `Test #${testNumber} cycle ${cycle.number}/${expectedCycles}: sender=${sender} `
      + `first=${first.platform}/${first.action} second=${second.platform}/${second.action} `
      + `expected=${expectedOutcome}`,
    );

    if (receivers.includes('web') || sender === 'web') await reopenWebAfterOffline(page);

    const startNativeStep = async (platform, role, approval) => {
      const environment = stepEnvironment({
        role,
        cycle: cycle.number,
        step,
        approvalPlatform: approval?.platform ?? '',
        sender,
        action: approval?.action ?? '',
        expectedOutcome,
      });
      if (platform === 'ios') {
        const test = startIosJoinTest(simulatorUdid, {
          testClass: scenario.ios.testClass,
          testMethod: scenario.ios.raceStepTestMethod ?? 'handleApproveDeclineStep',
          label: `iOS Test #${testNumber} ${role} cycle ${cycle.number}`,
          secretName,
          environment,
        });
        void test.result.catch(() => {});
        nativeProcesses.set(platform, { platform, role, test, approval });
      } else if (platform === 'android') {
        const test = await startAndroidStepTest(androidSerial, {
          role,
          cycle: cycle.number,
          step,
          approvalPlatform: approval?.platform ?? '',
          sender,
          decision: approval?.action ?? '',
          expectedOutcome,
          secretName,
          testMethod: scenario.android.raceStepTestMethod ?? 'handleApproveDeclineStep',
          label: `Android Test #${testNumber} ${role} cycle ${cycle.number}`,
        });
        nativeProcesses.set(platform, { platform, role, test, approval });
      } else {
        throw new Error(`Unsupported native Test #${testNumber} platform: ${platform}`);
      }
    };

    for (const receiver of receivers) {
      if (receiver !== 'web') {
        const approval = cycle.approvals.find((entry) => entry.platform === receiver);
        await startNativeStep(receiver, 'receiver', approval);
      }
    }

    if (sender !== 'web') {
      await startNativeStep(sender, 'sender', { platform: first.platform, action: first.action });
      approvalCoordinator.allow(`${sender}-sender-${step}`, cycle.number);
      await nativeProcesses.get(sender).test.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_REQUEST_SENT_${cycle.number}_${step}`,
        180_000,
      );
    } else {
      await startWebRecovery(page, secretName);
    }

    for (const receiver of receivers) {
      if (receiver === 'web') {
        await waitForWebIncomingRecoveryCount(page, 1, secretName);
      } else {
        await nativeProcesses.get(receiver).test.waitForMarker(
          `E2E: ${receiver.toUpperCase()}_INCOMING_VISIBLE_${cycle.number}_${step}`,
          180_000,
        );
      }
    }
    console.log(`✅ Test #${testNumber} cycle ${cycle.number}: both receiver alerts visible`);

    const runWebDecision = async (approval, allowTerminalSkip = false) => {
      if (!allowTerminalSkip) {
        await decideIncomingRecoveryOnWeb(page, secretName, approval.action, 1);
        return;
      }
      const openRequest = page.getByTestId(`open-recovery-request-${secretName}`);
      const badge = page.getByTestId(`recovery-request-badge-${secretName}`);
      const state = await Promise.race([
        openRequest.waitFor({ state: 'visible', timeout: 120_000 }).then(() => 'pending', () => null),
        badge.waitFor({ state: 'hidden', timeout: 120_000 }).then(() => 'terminal', () => null),
      ]);
      if (state === 'terminal') {
        console.log(
          `[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: `
          + `late ${approval.action} skipped after terminal recovery`,
        );
        return;
      }
      if (state !== 'pending') {
        throw new Error(`Web recovery request did not become actionable or terminal for cycle ${cycle.number}`);
      }
      await decideIncomingRecoveryOnWeb(page, secretName, approval.action, 1);
    };
    const releaseNativeDecision = async (approval) => {
      approvalCoordinator.allow(`${approval.platform}-${approval.action}-${step}`, cycle.number);
      const process = nativeProcesses.get(approval.platform)?.test;
      if (!process) throw new Error(`Missing native process for ${approval.platform}`);
      await process.waitForMarker(
        `E2E: ${approval.platform.toUpperCase()}_ACTION_STARTED_${cycle.number}_${step}`,
        120_000,
      );
      // Release the second responder only after the first responder has
      // completed its actual decision. The native action waits for its local
      // sync/processing path before emitting this marker, so the test observes
      // the same ordering that the server receives instead of racing two UI
      // clicks behind an artificial delay.
      await Promise.race([
        process.waitForMarker(
          `E2E: ${approval.platform.toUpperCase()}_${approval.action === 'approve' ? 'APPROVED' : 'DECLINED'}_INCOMING_${cycle.number}_${step}`,
          180_000,
        ),
        process.waitForMarker(
          `E2E: ${approval.platform.toUpperCase()}_ACTION_SKIPPED_AFTER_TERMINAL_${cycle.number}_${step}`,
          180_000,
        ),
      ]);
    };

    if (first.platform === 'web') await runWebDecision(first);
    else await releaseNativeDecision(first);
    // Once the first response is terminal, release the second runner as well.
    // It should observe the terminal state and record
    // ACTION_SKIPPED_AFTER_TERMINAL instead of changing the result.
    if (second.platform === 'web') {
      await runWebDecision(second, expectedOutcome === 'declined');
    }
    else approvalCoordinator.allow(`${second.platform}-${second.action}-${step}`, cycle.number);

    const waitNativeDecision = async (approval) => {
      if (approval.platform === 'web') return;
      const process = nativeProcesses.get(approval.platform)?.test;
      const prefix = approval.platform.toUpperCase();
      await Promise.race([
        process.waitForMarker(`E2E: ${prefix}_${approval.action === 'approve' ? 'APPROVED' : 'DECLINED'}_INCOMING_${cycle.number}_${step}`, 180_000),
        process.waitForMarker(`E2E: ${prefix}_ACTION_SKIPPED_AFTER_TERMINAL_${cycle.number}_${step}`, 180_000),
      ]);
    };
    await Promise.all([waitNativeDecision(first), waitNativeDecision(second)]);

    if (expectedOutcome === 'approved') {
      if (sender === 'web') {
        await revealAndCloseWebSecret(page, secretName, false);
      } else {
        approvalCoordinator.allow(`${sender}-show-${step}`, cycle.number);
        const senderTest = nativeProcesses.get(sender).test;
        await senderTest.waitForMarker(
          `E2E: ${sender.toUpperCase()}_RECOVERY_SECRET_VISIBLE_${cycle.number}_${step}`,
          recoveryShowTimeoutMs,
        );
        await senderTest.waitForMarker(`E2E: ${sender.toUpperCase()}_RECOVERY_CLOSED_${cycle.number}_${step}`, 60_000);
      }
    } else if (sender === 'web') {
      await assertWebRecoveryDeclined(page, secretName);
    } else {
      approvalCoordinator.allow(`${sender}-declined-${step}`, cycle.number);
      const senderTest = nativeProcesses.get(sender).test;
      await senderTest.waitForMarker(`E2E: ${sender.toUpperCase()}_RECOVERY_NOT_VISIBLE_${cycle.number}_${step}`, 180_000);
      await senderTest.waitForMarker(`E2E: ${sender.toUpperCase()}_RECOVERY_CLOSED_${cycle.number}_${step}`, 60_000);
    }

    for (const entry of nativeProcesses.values()) await entry.test.result;
    for (const platform of ['android', 'ios']) {
      if (sender === platform || receivers.includes(platform)) {
        if (platform === 'android') await stopAndroidApplication(androidSerial);
        if (platform === 'ios') await stopIosApplication(simulatorUdid);
      }
    }
    console.log(`✅ Test #${testNumber} cycle ${cycle.number}/${expectedCycles} passed`);
  }
}

async function runRepeatApproveRecovery(page, simulatorUdid, androidSerial) {
  const expectedCycles = scenario.recovery.expectedCycles ?? recoveryCycles.length;
  if (recoveryCycles.length !== expectedCycles) {
    throw new Error(`Test #${testNumber} expected ${expectedCycles} recovery requests, got ${recoveryCycles.length}`);
  }

  for (const cycle of recoveryCycles) {
    setCurrentCycleForDiagnostics(cycle);
    if (cycle.senders.length !== 1 || cycle.approvals.length !== 1) {
      throw new Error(`Test #${testNumber} cycle ${cycle.number} must have one sender and one receiver approval`);
    }
    const sender = cycle.senders[0];
    const approval = cycle.approvals[0];
    const target = approval.platform;
    const secretName = cycle.senderSecrets[sender] ?? approval.secret ?? defaultSecret?.name;
    const step = 1;
    if (sender === target || !['web', 'ios', 'android'].includes(sender) || !['web', 'ios', 'android'].includes(target)) {
      throw new Error(`Test #${testNumber} cycle ${cycle.number} sender and receiver must be different supported platforms`);
    }
    if (approval.action !== 'approve') {
      throw new Error(`Test #${testNumber} cycle ${cycle.number} must use an approve receiver action`);
    }

    writeDiagnostic(
      `TEST${testNumber} cycle=${cycle.number} sender=${sender} receiver=${target} `
      + `secret=${secretName} action=approve repeat=true`,
    );
    console.log(
      `Test #${testNumber} cycle ${cycle.number}/${expectedCycles}: sender=${sender} `
      + `receiver=${target} double-approve guard`,
    );

    if (sender === 'web' || target === 'web') await reopenWebAfterOffline(page);

    const nativeProcesses = new Map();
    const startNativeStep = async (platform, role) => {
      const environment = {
        ...stepEnvironment({
          role,
          cycle: cycle.number,
          step,
          approvalPlatform: target,
          sender,
          action: 'approve',
          expectedOutcome: 'approved',
        }),
        E2E_REPEAT_APPROVE: '1',
      };
      if (platform === 'ios') {
        const test = startIosJoinTest(simulatorUdid, {
          testClass: scenario.ios.testClass,
          testMethod: scenario.ios.repeatApproveTestMethod
            ?? scenario.ios.raceStepTestMethod
            ?? 'handleApproveDeclineStep',
          label: `iOS Test #${testNumber} ${role} cycle ${cycle.number}`,
          secretName,
          environment,
        });
        void test.result.catch(() => {});
        nativeProcesses.set(platform, { platform, role, test });
        return;
      }
      if (platform === 'android') {
        const test = await startAndroidStepTest(androidSerial, {
          role,
          cycle: cycle.number,
          step,
          approvalPlatform: target,
          sender,
          decision: 'approve',
          expectedOutcome: 'approved',
          secretName,
          repeatApprove: true,
          testMethod: scenario.android.repeatApproveTestMethod
            ?? scenario.android.raceStepTestMethod
            ?? 'handleApproveDeclineStep',
          label: `Android Test #${testNumber} ${role} cycle ${cycle.number}`,
        });
        nativeProcesses.set(platform, { platform, role, test });
        return;
      }
      throw new Error(`Unsupported native Test #${testNumber} platform: ${platform}`);
    };

    if (target !== 'web') await startNativeStep(target, 'receiver');
    if (sender !== 'web') await startNativeStep(sender, 'sender');

    if (sender === 'web') {
      await startWebRecovery(page, secretName);
    } else {
      approvalCoordinator.allow(`${sender}-sender-${step}`, cycle.number);
      await nativeProcesses.get(sender).test.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_REQUEST_SENT_${cycle.number}_${step}`,
        180_000,
      );
    }

    if (target === 'web') {
      await decideIncomingRecoveryTwiceOnWeb(page, secretName);
    } else {
      const targetProcess = nativeProcesses.get(target)?.test;
      await targetProcess.waitForMarker(
        `E2E: ${target.toUpperCase()}_INCOMING_VISIBLE_${cycle.number}_${step}`,
        180_000,
      );
      approvalCoordinator.allow(`${target}-approve-${step}`, cycle.number);
      await targetProcess.waitForMarker(
        `E2E: ${target.toUpperCase()}_ACTION_STARTED_${cycle.number}_${step}`,
        120_000,
      );
      await targetProcess.waitForMarker(
        `E2E: ${target.toUpperCase()}_APPROVED_INCOMING_${cycle.number}_${step}`,
        180_000,
      );
      await Promise.race([
        targetProcess.waitForMarker(
          `E2E: ${target.toUpperCase()}_REPEAT_APPROVE_SECOND_TAP_SENT_${cycle.number}_${step}`,
          30_000,
        ),
        targetProcess.waitForMarker(
          `E2E: ${target.toUpperCase()}_REPEAT_APPROVE_SECOND_TAP_SKIPPED_AFTER_DISMISS_${cycle.number}_${step}`,
          30_000,
        ),
      ]);
    }

    if (sender === 'web') {
      await revealAndCloseWebSecret(page, secretName, false);
    } else {
      approvalCoordinator.allow(`${sender}-show-${step}`, cycle.number);
      const senderProcess = nativeProcesses.get(sender).test;
      await senderProcess.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_SECRET_VISIBLE_${cycle.number}_${step}`,
        recoveryShowTimeoutMs,
      );
      await senderProcess.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_CLOSED_${cycle.number}_${step}`,
        60_000,
      );
    }

    for (const entry of nativeProcesses.values()) await entry.test.result;
    if (nativeProcesses.has('android')) await stopAndroidApplication(androidSerial);
    if (nativeProcesses.has('ios')) await stopIosApplication(simulatorUdid);
    console.log(`✅ Test #${testNumber} cycle ${cycle.number}/${expectedCycles} passed: one approval despite repeat tap`);
  }
}

async function runDuplicateRecovery(page, simulatorUdid, androidSerial) {
  const expectedCycles = scenario.recovery.expectedCycles ?? recoveryCycles.length;
  if (recoveryCycles.length !== expectedCycles) {
    throw new Error(`Test #${testNumber} expected ${expectedCycles} duplicate-recovery cycles, got ${recoveryCycles.length}`);
  }

  for (const cycle of recoveryCycles) {
    setCurrentCycleForDiagnostics(cycle);
    if (cycle.senders.length !== 1 || cycle.approvals.length !== 1) {
      throw new Error(`Test #${testNumber} cycle ${cycle.number} must have one sender and one approval target`);
    }
    const sender = cycle.senders[0];
    const approval = cycle.approvals[0];
    const receiver = approval.platform;
    const secretName = cycle.senderSecrets[sender] ?? approval.secret ?? defaultSecret?.name;
    const step = 1;
    if (sender === receiver || !['web', 'ios', 'android'].includes(sender) || !['web', 'ios', 'android'].includes(receiver)) {
      throw new Error(`Test #${testNumber} cycle ${cycle.number} sender and receiver must be different supported platforms`);
    }
    if (approval.action !== 'approve') {
      throw new Error(`Test #${testNumber} cycle ${cycle.number} must approve the single surviving claim`);
    }

    writeDiagnostic(
      `TEST${testNumber} cycle=${cycle.number} sender=${sender} receiver=${receiver} `
      + `secret=${secretName} duplicateRecovery=true expectedActiveClaims=1`,
    );
    console.log(
      `Test #${testNumber} cycle ${cycle.number}/${expectedCycles}: sender=${sender} `
      + `submits recovery twice before ${receiver} approves`,
    );

    if (sender === 'web' || receiver === 'web') await reopenWebAfterOffline(page);

    const nativeProcesses = new Map();
    const startNativeParticipant = async (platform, role) => {
      const environment = {
        ...stepEnvironment({
          role,
          cycle: cycle.number,
          step,
          approvalPlatform: receiver,
          sender,
          action: 'approve',
          expectedOutcome: 'approved',
        }),
        E2E_DUPLICATE_RECOVERY: '1',
      };
      if (platform === 'ios') {
        const test = startIosJoinTest(simulatorUdid, {
          testClass: scenario.ios.testClass,
          testMethod: scenario.ios.duplicateRecoveryTestMethod
            ?? scenario.ios.raceStepTestMethod
            ?? 'handleApproveDeclineStep',
          label: `iOS Test #${testNumber} ${role} cycle ${cycle.number}`,
          secretName,
          environment,
        });
        void test.result.catch(() => {});
        nativeProcesses.set(platform, { platform, role, test });
        return;
      }
      if (platform === 'android') {
        const test = await startAndroidStepTest(androidSerial, {
          role,
          cycle: cycle.number,
          step,
          approvalPlatform: receiver,
          sender,
          decision: 'approve',
          expectedOutcome: 'approved',
          secretName,
          duplicateRecovery: true,
          testMethod: scenario.android.duplicateRecoveryTestMethod
            ?? scenario.android.raceStepTestMethod
            ?? 'handleApproveDeclineStep',
          label: `Android Test #${testNumber} ${role} cycle ${cycle.number}`,
        });
        nativeProcesses.set(platform, { platform, role, test });
        return;
      }
      throw new Error(`Unsupported native Test #${testNumber} platform: ${platform}`);
    };

    // The receiver is listening before either sender submit. This makes the
    // single-claim assertion observe the persisted state, not a transient UI.
    if (receiver !== 'web') await startNativeParticipant(receiver, 'receiver');
    if (sender !== 'web') await startNativeParticipant(sender, 'sender');

    if (sender === 'web') {
      await startWebDuplicateRecovery(page, secretName);
    } else {
      approvalCoordinator.allow(`${sender}-sender-${step}`, cycle.number);
      const senderProcess = nativeProcesses.get(sender).test;
      await senderProcess.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_REQUEST_SENT_${cycle.number}_${step}`,
        180_000,
      );
      await Promise.race([
        senderProcess.waitForMarker(
          `E2E: ${sender.toUpperCase()}_DUPLICATE_RECOVERY_SENT_${cycle.number}_${step}`,
          30_000,
        ),
        senderProcess.waitForMarker(
          `E2E: ${sender.toUpperCase()}_DUPLICATE_RECOVERY_SKIPPED_AFTER_GUARD_${cycle.number}_${step}`,
          30_000,
        ),
      ]);
    }

    if (receiver === 'web') {
      await waitForWebRecoveryBadgeCount(page, 1, secretName);
      writeDiagnostic(`[UI][Web] cycle ${cycle.number}: exactly one active recovery badge observed`);
      console.log(`[UI][Web] cycle ${cycle.number}: exactly one active recovery claim observed`);
    } else {
      const receiverProcess = nativeProcesses.get(receiver).test;
      await receiverProcess.waitForMarker(
        `E2E: ${receiver.toUpperCase()}_INCOMING_VISIBLE_${cycle.number}_${step}`,
        180_000,
      );
      await receiverProcess.waitForMarker(
        `E2E: ${receiver.toUpperCase()}_SINGLE_ACTIVE_CLAIM_${cycle.number}_${step}`,
        180_000,
      );
    }

    if (receiver === 'web') {
      await decideIncomingRecoveryOnWeb(page, secretName, 'approve', 1);
    } else {
      approvalCoordinator.allow(`${receiver}-approve-${step}`, cycle.number);
      await nativeProcesses.get(receiver).test.waitForMarker(
        `E2E: ${receiver.toUpperCase()}_APPROVED_INCOMING_${cycle.number}_${step}`,
        180_000,
      );
    }

    if (sender === 'web') {
      await revealAndCloseWebSecret(page, secretName, true);
    } else {
      // iOS names coordinator gates through waitForApproval(platform, step),
      // so the key includes the step suffix just like the receiver gate.
      approvalCoordinator.allow(`${sender}-duplicate-finish-${step}`, cycle.number);
      const senderProcess = nativeProcesses.get(sender).test;
      await senderProcess.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_SECRET_VISIBLE_${cycle.number}_${step}`,
        recoveryShowTimeoutMs,
      );
      await senderProcess.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_CLOSED_${cycle.number}_${step}`,
        60_000,
      );
    }

    for (const entry of nativeProcesses.values()) await entry.test.result;
    if (nativeProcesses.has('android')) await stopAndroidApplication(androidSerial);
    if (nativeProcesses.has('ios')) await stopIosApplication(simulatorUdid);
    console.log(`✅ Test #${testNumber} cycle ${cycle.number}/${expectedCycles} passed: one active claim after duplicate submit`);
  }
}

async function runNewDeviceDuringRecoveryWithCli(page, simulatorUdid, androidSerial) {
  const expectedCycles = scenario.recovery.expectedCycles ?? recoveryCycles.length;
  if (recoveryCycles.length !== expectedCycles) {
    throw new Error(`Test #${testNumber} expected ${expectedCycles} recovery requests, got ${recoveryCycles.length}`);
  }
  if (scenario.setup?.initiator !== 'web') {
    throw new Error('Test #15 CLI block currently requires a Web-created vault');
  }

  for (const cycle of recoveryCycles) {
    setCurrentCycleForDiagnostics(cycle);
    const sender = cycle.senders?.[0] ?? 'web';
    const secretName = cycle.senderSecrets?.[sender] ?? defaultSecret?.name;
    if (sender !== 'web') {
      throw new Error(
        `Test #15 CLI smoke block currently supports Web as sender; got ${sender} in cycle ${cycle.number}`,
      );
    }

    writeDiagnostic(
      `TEST15 cycle=${cycle.number} sender=${sender} secret=${secretName} `
      + 'newDevice=cli during active recovery',
    );
    console.log(
      `Test #15 cycle ${cycle.number}/${expectedCycles}: Web requests recovery, `
      + 'CLI joins before the old claim is answered',
    );

    await reopenWebAfterOffline(page);
    const nativeReceiverPlatforms = scenario.newDevice?.oldReceivers ?? ['ios', 'android'];
    const nativeReceivers = new Map();
    for (const platform of nativeReceiverPlatforms) {
      if (platform === 'ios') {
        const test = startIosJoinTest(simulatorUdid, {
          testClass: scenario.ios.testClass,
          testMethod: scenario.ios.stepTestMethod ?? 'handleRecoveryStep',
          label: `iOS Test #15 old receiver cycle ${cycle.number}`,
          secretName,
          environment: stepEnvironment({
            role: 'receiver', cycle: cycle.number, step: 1,
            approvalPlatform: 'cli', sender,
          }),
        });
        void test.result.catch(() => {});
        nativeReceivers.set(platform, test);
      } else {
        const test = await startAndroidStepTest(androidSerial, {
          role: 'receiver', cycle: cycle.number, step: 1,
          approvalPlatform: 'cli', sender, secretName,
          decision: 'dismiss',
          expectedOutcome: 'approved',
          testMethod: scenario.android.stepTestMethod ?? 'handleRecoveryStep',
          label: `Android Test #15 old receiver cycle ${cycle.number}`,
        });
        nativeReceivers.set(platform, test);
      }
    }

    await waitForRecoveryParticipantsReady(page, {
      cycleNumber: cycle.number,
      secretName,
      participants: [
        ...nativeReceiverPlatforms.map((platform) => ({
          platform,
          role: 'receiver',
          test: nativeReceivers.get(platform),
        })),
        { platform: 'web', role: 'sender' },
      ],
      label: 'TEST15-LEGACY',
    });

    await startWebRecovery(page, secretName);
    for (const [platform, test] of nativeReceivers.entries()) {
      await test.waitForMarker(
        `E2E: ${platform.toUpperCase()}_INCOMING_VISIBLE_${cycle.number}_1`,
        180_000,
      );
    }
    writeDiagnostic(
      `TEST15 cycle=${cycle.number} old recovery visible on native receivers `
      + nativeReceiverPlatforms.join(','),
    );

    const cliDevice = await createMetaCliDevice(cycle);
    // The Web sender's waiting dialog is only a view of the active claim;
    // close it before navigating to Devices to approve the new CLI member.
    // The claim remains active until a receiver answers it.
    await dismissWebRecoveryWaitingUi(page, secretName);
    await approveJoinRequestOnWeb(page, 'CLI');
    const cliClaimsInfo = await assertMetaCliMember(cliDevice, 4);
    const activeRecoveryClaims = (cliClaimsInfo.claims ?? [])
      .filter((claim) => claim.type === 'Recover' && claim.password === secretName)
      .map((claim) => ({
        id: claim.id,
        clientStatus: claim.clientStatus,
        receivers: (claim.receivers ?? []).map((receiver) => ({
          id: receiver.id,
          status: receiver.status,
        })),
      }));
    writeDiagnostic(
      `[CLI] post-join recovery claims secret=${secretName} `
      + JSON.stringify(activeRecoveryClaims),
    );
    writeDiagnostic(`TEST15 cycle=${cycle.number} CLI join accepted and redistributed`);

    // The join is the state boundary. Release the old native receivers only
    // after the new membership is canonical; their own UI assertions must
    // prove that the pre-join recovery was invalidated, not merely hidden by
    // a test-side dismissal.
    for (const platform of nativeReceiverPlatforms) {
      approvalCoordinator.allow(`${platform}-dismiss-1`, cycle.number);
    }
    for (const [platform, test] of nativeReceivers.entries()) {
      await test.waitForMarker(
        `E2E: ${platform.toUpperCase()}_DISMISSED_INCOMING_${cycle.number}_1`,
        180_000,
      );
    }
    for (const test of nativeReceivers.values()) await test.result;

    await stopAndroidApplication(androidSerial);
    await stopIosApplication(simulatorUdid);
    await reopenWebAfterOffline(page);
    await assertNoStaleWebRecoveryAlert(page, secretName);
    const primaryAction = page.getByTestId(`secret-primary-action-${secretName}`);
    await primaryAction.filter({ hasText: /^\s*Recover\s*$/ }).waitFor({
      state: 'visible',
      timeout: 120_000,
    });
    writeDiagnostic(`TEST15 cycle=${cycle.number} old sender claim is no longer actionable`);

    // Membership redistribution gives the new member a Split claim. The
    // recovery that was already open before the join must stay terminal and
    // must not be recreated as a recovery request for the new device. The
    // sender already has a complete Split after redistribution, so its action
    // is Show rather than another recovery request in this cycle.
    const staleRecoveryClaims = (cliClaimsInfo.claims ?? [])
      .filter((claim) => claim.type === 'Recover' && claim.password === secretName);
    const cliInStaleRecovery = staleRecoveryClaims.some((claim) => (
      claim.receivers?.some((receiver) => receiver.id === cliDevice.deviceId)
        && (claim.receivers ?? []).some((receiver) => (
          ['Pending', 'Sent', 'Delivered', 'NeedApprove'].includes(receiver.status)
        ))
    ));
    if (cliInStaleRecovery) {
      throw new Error(
        `CLI unexpectedly received an actionable old recovery claim for ${secretName}; `
        + `device=${cliDevice.deviceId}`,
      );
    }
    const terminalOldRecovery = staleRecoveryClaims.every((claim) => (
      (claim.receivers ?? []).length > 0
        && (claim.receivers ?? []).every((receiver) => receiver.status === 'Declined')
    ));
    if (!terminalOldRecovery) {
      throw new Error(
        `Old recovery claim is not terminal after CLI join for ${secretName}: `
        + `${JSON.stringify(staleRecoveryClaims).slice(0, 2_000)}`,
      );
    }
    writeDiagnostic(
      `TEST15 cycle=${cycle.number} new CLI has Split only; `
      + 'old Recover is Declined and no stale recovery was recreated',
    );
    console.log(`✅ Test #15 cycle ${cycle.number}/${expectedCycles} passed`);
  }
}

async function runTest15PostJoinRecoveryPlan(page, simulatorUdid, androidSerial, cliDevice, persistentNativeCategory) {
  const plan = scenario.postJoinRecoveryPlan;
  const persistentNativePlatform = persistentNativeCategory === 'iPhone' ? 'ios' : 'android';
  const survivingPlatforms = ['web', persistentNativePlatform, 'cli'];
  if (!Array.isArray(plan) || plan.length !== 3) {
    throw new Error('Test #15 requires exactly three post-join recovery validations');
  }
  const senders = plan.map(({ sender }) => sender);
  const approvers = plan.map(({ approver }) => approver);
  if (new Set(senders).size !== 3 || new Set(approvers).size !== 3
      || survivingPlatforms.some((platform) => !senders.includes(platform) || !approvers.includes(platform))
      || plan.some(({ sender, approver, expectedOutcome }) => (
        !survivingPlatforms.includes(sender)
          || !survivingPlatforms.includes(approver)
          || sender === approver
          || expectedOutcome !== 'approved'
      ))) {
    throw new Error(
      `Test #15 post-join plan must use each surviving member exactly once as sender and approver; `
      + `survivors=${survivingPlatforms.join(',')} plan=${JSON.stringify(plan)}`,
    );
  }

  for (const [index, entry] of plan.entries()) {
    const cycleNumber = recoveryCycles.length + index + 1;
    const { sender, approver } = entry;
    const secretName = defaultSecret.name;
    const cycle = { number: cycleNumber };
    setCurrentCycleForDiagnostics(cycle);
    writeDiagnostic(
      `TEST15 post-join validation=${index + 1}/3 sender=${sender} approver=${approver} `
      + `secret=${secretName} survivors=${survivingPlatforms.join(',')}`,
    );
    console.log(
      `Test #15 post-join recovery ${index + 1}/3: ${sender} requests; `
      + `${approver} approves; verify the secret can be shown.`,
    );

    const nativeProcesses = new Map();
    const nativePlatforms = survivingPlatforms.filter((platform) => platform !== 'web' && platform !== 'cli');
    const startNative = async (platform, role) => {
      const environment = stepEnvironment({
        role,
        cycle: cycleNumber,
        step: 1,
        // The native sender helper has a special CLI-approval mode reserved
        // for Test15's membership-removal branch. Use the regular gated Show
        // path here; the CLI decision itself is still made through the CLI
        // command and confirmed by the server invalidation below.
        approvalPlatform: role === 'sender' && approver === 'cli' ? 'web' : approver,
        sender,
        expectedOutcome: 'approved',
      });
      if (platform === 'ios') {
        const test = startIosJoinTest(simulatorUdid, {
          testClass: scenario.ios.testClass,
          testMethod: scenario.ios.stepTestMethod ?? 'handleRecoveryStep',
          label: `iOS Test #15 post-join ${role} cycle ${cycleNumber}`,
          secretName,
          secretValue: defaultSecret.value,
          environment,
        });
        void test.result.catch(() => {});
        nativeProcesses.set(platform, { role, test });
        return test;
      }
      const test = await startAndroidStepTest(androidSerial, {
        role,
        cycle: cycleNumber,
        step: 1,
        approvalPlatform: role === 'sender' && approver === 'cli' ? 'web' : approver,
        sender,
        expectedOutcome: 'approved',
        secretName,
        secretValue: defaultSecret.value,
        testMethod: scenario.android.stepTestMethod ?? 'handleRecoveryStep',
        label: `Android Test #15 post-join ${role} cycle ${cycleNumber}`,
      });
      nativeProcesses.set(platform, { role, test });
      return test;
    };

    for (const platform of nativePlatforms) {
      await startNative(platform, platform === sender ? 'sender' : 'receiver');
    }
    const uiParticipants = survivingPlatforms
      .filter((platform) => platform !== 'cli')
      .map((platform) => ({
        platform,
        role: platform === sender ? 'sender' : 'receiver',
        test: nativeProcesses.get(platform)?.test,
      }));
    await waitForRecoveryParticipantsReady(page, {
      cycleNumber,
      secretName,
      participants: uiParticipants,
      label: 'TEST15 post-join',
    });

    // The CLI may retain pending Recover claims from earlier cycles when it
    // was a non-approver. Snapshot its view so this cycle cannot approve one
    // of those older claims by matching only the secret and receiver.
    const claimsBeforeRequest = await readMetaCliInfo(
      cliDevice,
      ['info', 'recovery-claims'],
      { suppressOutput: true },
    );
    const baselineRecoveryClaimIds = recoveryClaimIds(claimsBeforeRequest, secretName);
    writeDiagnostic(
      `TEST15 recovery claim baseline cycle=${cycleNumber} `
      + `claimIds=${[...baselineRecoveryClaimIds].join(',') || 'none'}`,
    );

    let senderClaimId = null;
    let requestCommitted;
    if (sender === 'web') {
      requestCommitted = activeServerProcess.waitForNextMarker(ssClaimsInvalidationMarker, 180_000);
      await startWebRecovery(page, secretName);
      await requestCommitted;
    } else if (sender === 'cli') {
      requestCommitted = activeServerProcess.waitForNextMarker(ssClaimsInvalidationMarker, 180_000);
      await runMetaCli(cliDevice.cliDirectory, ['secret', 'recovery-request', '--pass-name', secretName], {
        label: `Test15 CLI recovery request cycle ${cycleNumber}`,
        suppressOutput: true,
      });
      await requestCommitted;
      const senderClaims = await readMetaCliInfo(
        cliDevice,
        ['info', 'recovery-claims'],
        { suppressOutput: true },
      );
      const request = selectNewPendingRecoveryClaim(senderClaims, {
        password: secretName,
        baselineClaimIds: baselineRecoveryClaimIds,
        senderId: cliDevice.deviceId,
      });
      if (!request) throw new Error(`Test #15 CLI sender has no pending recovery claim at cycle ${cycleNumber}`);
      senderClaimId = request.id;
    } else {
      requestCommitted = activeServerProcess.waitForNextMarker(ssClaimsInvalidationMarker, 180_000);
      approvalCoordinator.allow(`${sender}-sender-1`, cycleNumber);
      await nativeProcesses.get(sender).test.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_REQUEST_SENT_${cycleNumber}_1`,
        180_000,
      );
      await requestCommitted;
    }

    if (approver === 'web') {
      const approvedClaimId = await approveIncomingRecoveryOnWeb(page, secretName, 1, {
        senderSecretName: sender === 'web' ? secretName : null,
        expectedRemainingCount: 0,
      });
      if (sender === 'cli' && senderClaimId !== approvedClaimId) {
        throw new Error(
          `Test #15 Web approved a different CLI recovery claim: `
          + `cycle=${cycleNumber} expected=${senderClaimId ?? 'none'} actual=${approvedClaimId ?? 'none'}`,
        );
      }
    } else if (approver === 'cli') {
      const receiverClaims = await readMetaCliInfo(
        cliDevice,
        ['info', 'recovery-claims'],
        { suppressOutput: true },
      );
      const pending = selectNewPendingRecoveryClaim(receiverClaims, {
        password: secretName,
        baselineClaimIds: baselineRecoveryClaimIds,
        receiverId: cliDevice.deviceId,
      });
      if (!pending) throw new Error(`Test #15 CLI approver has no pending recovery claim at cycle ${cycleNumber}`);
      writeDiagnostic(
        `TEST15 CLI approval target cycle=${cycleNumber} claimId=${pending.id} `
        + `sender=${pending.sender} cliReceiver=${cliDevice.deviceId} status=Pending`,
      );
      const approvalCommitted = activeServerProcess.waitForNextMarker(ssClaimsInvalidationMarker, 180_000);
      const approvalCommand = runMetaCli(cliDevice.cliDirectory, [
        'secret', 'accept-recovery-request', '--claim-id', pending.id,
      ], {
        label: `Test15 CLI approves recovery cycle ${cycleNumber}`,
      });
      const [approvalResult] = await Promise.all([approvalCommand, approvalCommitted]);
      writeDiagnostic(
        `TEST15 CLI approval command completed cycle=${cycleNumber} `
        + `stdoutBytes=${Buffer.byteLength(approvalResult.stdout)} `
        + `stderrBytes=${Buffer.byteLength(approvalResult.stderr)}`,
      );
      senderClaimId ??= pending.id;
    } else {
      const approverTest = nativeProcesses.get(approver)?.test;
      if (!approverTest) throw new Error(`Test #15 has no ${approver} approval actor`);
      approvalCoordinator.allow(`${approver}-approve-1`, cycleNumber);
      await approverTest.waitForMarker(
        `E2E: ${approver.toUpperCase()}_APPROVED_INCOMING_${cycleNumber}_1`,
        180_000,
      );
    }

    if (sender === 'web') {
      await revealAndCloseWebSecret(page, secretName, false);
    } else if (sender === 'cli') {
      const shown = await runMetaCli(cliDevice.cliDirectory, [
        'secret', 'show', '--claim-id', senderClaimId,
      ], {
        label: `Test15 CLI verifies recovered secret cycle ${cycleNumber}`,
        suppressOutput: true,
      });
      const shownJson = parseCliJson(shown, 'Test15 recovery show');
      if (shownJson.status !== 'success' || shownJson.secret !== defaultSecret.value) {
        throw new Error(`Test #15 CLI secret verification failed at cycle ${cycleNumber}`);
      }
      writeDiagnostic(`TEST15 cycle=${cycleNumber} CLI secret read verified exact value`);
    } else {
      approvalCoordinator.allow(`${sender}-show-1`, cycleNumber);
      const senderTest = nativeProcesses.get(sender)?.test;
      await senderTest.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_SECRET_VISIBLE_${cycleNumber}_1`,
        recoveryShowTimeoutMs,
      );
      await senderTest.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_CLOSED_${cycleNumber}_1`,
        60_000,
      );
    }

    for (const platform of nativePlatforms) {
      if (platform !== sender && platform !== approver) {
        approvalCoordinator.allow(`${platform}-dismiss-1`, cycleNumber);
        const observerTest = nativeProcesses.get(platform)?.test;
        if (!observerTest) throw new Error(`Test #15 has no ${platform} observer for cycle ${cycleNumber}`);
        const dismissedMarker = `E2E: ${platform.toUpperCase()}_DISMISSED_INCOMING_${cycleNumber}_1`;
        const terminalMarker = `E2E: ${platform.toUpperCase()}_ACTION_SKIPPED_AFTER_TERMINAL_${cycleNumber}_1`;
        await observerTest.waitForMarker(
          new RegExp(`(?:${dismissedMarker}|${terminalMarker})`),
          180_000,
        );
        if (observerTest.outputTail().includes(terminalMarker)) {
          if (sender !== 'cli' || !senderClaimId) {
            throw new Error(
              `Test #15 ${platform} observed terminal recovery without an exact CLI sender claim `
              + `to verify: cycle=${cycleNumber} sender=${sender} claimId=${senderClaimId ?? 'none'}`,
            );
          }
          const senderClaims = await readMetaCliInfo(
            cliDevice,
            ['info', 'recovery-claims'],
            { suppressOutput: true },
          );
          const completedClaim = (senderClaims.claims ?? []).find((claim) => claim.id === senderClaimId);
          if (completedClaim?.type !== 'Recover' || completedClaim.clientStatus !== 'Done') {
            throw new Error(
              `Test #15 observer saw terminal recovery but exact sender claim is not Done: `
              + `cycle=${cycleNumber} claimId=${senderClaimId} `
              + `claim=${JSON.stringify(completedClaim ?? null)}`,
            );
          }
          writeDiagnostic(
            `TEST15 cycle=${cycleNumber} ${platform} observed terminal recovery; `
            + `exact sender claim=${senderClaimId} clientStatus=Done`,
          );
        }
      }
    }
    if (sender !== 'web' && approver !== 'web') {
      await waitForWebIncomingRecoveryGone(page, secretName);
    }
    for (const { test } of nativeProcesses.values()) await test.result;
    if (nativeProcesses.has('android')) await stopAndroidApplication(androidSerial);
    if (nativeProcesses.has('ios')) await stopIosApplication(simulatorUdid);
    writeDiagnostic(`TEST15 post-join validation cycle=${cycleNumber} approved and secret read verified`);
    console.log(`✅ Test #15 post-join recovery ${index + 1}/3 passed`);
  }
}

async function runNewDeviceDuringRecoveryWithCliFull(page, simulatorUdid, androidSerial) {
  const expectedCycles = scenario.recovery.expectedCycles ?? recoveryCycles.length;
  if (expectedCycles !== 1 || recoveryCycles.length !== 1) {
    throw new Error('Test #15 requires exactly one initial Recover/removal/CLI-rejoin sequence');
  }
  if (recoveryCycles[0].expectedOutcome !== 'declined') {
    throw new Error('Test #15 initial Recover must be expected to become Declined after membership removal');
  }
  if (recoveryCycles.length !== expectedCycles) {
    throw new Error(`Test #${testNumber} expected ${expectedCycles} recovery requests, got ${recoveryCycles.length}`);
  }

  const senderForCycle = (cycle) => {
    if (cycle.senders.length !== 1) {
      throw new Error(`Test #15 cycle ${cycle.number} must have exactly one sender`);
    }
    return cycle.senders[0];
  };

  const initialDeviceNames = await assertWebMembership(
    page,
    ['Web', 'Android', 'iPhone'],
    [defaultSecret.name],
    'Test #15 initial three-member boundary',
  );
  const persistentNativeCategory = scenario.newDevice?.removalApprover === 'ios'
    ? 'iPhone'
    : 'Android';
  let previousCliDevice = null;
  for (const cycle of recoveryCycles) {
    setCurrentCycleForDiagnostics(cycle);
    const sender = senderForCycle(cycle);
    const secretName = cycle.senderSecrets[sender] ?? defaultSecret?.name;
    const oldReceivers = scenario.newDevice?.oldReceivers
      ?? ['web', 'ios', 'android'].filter((platform) => platform !== sender);
    if (!['web', 'ios', 'android'].includes(sender)) {
      throw new Error(`Test #15 unsupported sender platform: ${sender}`);
    }
    const removalApprover = scenario.newDevice?.removalApprover;
    const removalTargetPlatform = cycle.number === 1 ? scenario.newDevice?.removalTarget : 'cli';
    if (!removalApprover || !removalTargetPlatform || removalApprover === removalTargetPlatform) {
      throw new Error(`Test #15 cycle ${cycle.number} requires an exact non-target removal approver`);
    }

    writeDiagnostic(
      `TEST15 cycle=${cycle.number} sender=${sender} secret=${secretName} `
      + `oldReceivers=${oldReceivers.join(',')} remove=${removalTargetPlatform} newDevice=cli`,
    );
    console.log(
      `Test #15 cycle ${cycle.number}/${expectedCycles}: sender=${sender}; `
      + `membership change invalidates the pending recovery, then CLI joins; approver=${removalApprover}`,
    );

    // Keep a Web sender's live owner session intact. Reloading it here can
    // surface the setup-time Show action and leave no Recover action to start
    // the request. When Web is only a receiver, reload it to restore its
    // receiver state before the native sender starts.
    if (sender !== 'web') await reopenWebAfterOffline(page);
    const nativeProcesses = new Map();
    const startNative = async (platform, role) => {
      const environment = stepEnvironment({
        role,
        cycle: cycle.number,
        step: 1,
        approvalPlatform: 'cli',
        sender,
        expectedOutcome: 'membership-removal',
      });
      if (platform === 'ios') {
        const test = startIosJoinTest(simulatorUdid, {
          testClass: scenario.ios.testClass,
          testMethod: scenario.ios.stepTestMethod ?? 'handleRecoveryStep',
          label: `iOS Test #15 ${role} cycle ${cycle.number}`,
          secretName,
          environment,
        });
        void test.result.catch(() => {});
        nativeProcesses.set(platform, { role, test });
        return;
      }
      if (platform === 'android') {
        const test = await startAndroidStepTest(androidSerial, {
          role,
          cycle: cycle.number,
          step: 1,
          approvalPlatform: 'cli',
          sender,
          secretName,
          expectedOutcome: 'membership-removal',
          testMethod: scenario.android.stepTestMethod ?? 'handleRecoveryStep',
          label: `Android Test #15 ${role} cycle ${cycle.number}`,
        });
        nativeProcesses.set(platform, { role, test });
        await test.waitForMarker(
          `E2E: ANDROID_${role.toUpperCase()}_UI_READY_${cycle.number}_1`,
          180_000,
        );
        return;
      }
      throw new Error(`Test #15 cannot start Web as a native process: ${platform}`);
    };

    const nativeObserverPlatforms = sender === 'web' ? [removalApprover] : [];
    for (const platform of nativeObserverPlatforms) await startNative(platform, 'receiver');
    if (sender !== 'web') await startNative(sender, 'sender');

    const readinessParticipants = [
      ...nativeObserverPlatforms.map((platform) => ({
        platform,
        role: 'receiver',
        test: nativeProcesses.get(platform)?.test,
      })),
      ...(sender !== 'web' ? [{ platform: 'web', role: 'receiver' }] : []),
      { platform: sender, role: 'sender', test: nativeProcesses.get(sender)?.test },
    ];
    if (sender === 'web') await unlockWithPasskeyIfNeeded(page);
    await waitForRecoveryParticipantsReady(page, {
      cycleNumber: cycle.number,
      secretName,
      participants: readinessParticipants,
      label: 'TEST15',
    });

    let serverRecoveryCommitted;
    if (sender === 'web') {
      await startWebRecovery(page, secretName);
    } else {
      serverRecoveryCommitted = activeServerProcess.waitForNextMarker(
        // All other claim-producing actors are held at their ready gates: the
        // native sender waits on its sender gate, the other native receiver is
        // only observing, and Web is not allowed to approve/show anything
        // before this barrier. Thus the next SsClaims invalidation is caused
        // by this sender's request. Match only the stable scope because the
        // runtime VaultName formatting varies across server log versions and
        // does not reliably match the scenario's UI alias.
        ssClaimsInvalidationMarker,
        180_000,
      );
      approvalCoordinator.allow(`${sender}-sender-1`, cycle.number);
      const senderProcess = nativeProcesses.get(sender)?.test;
      await senderProcess.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_REQUEST_SENT_${cycle.number}_1`,
        180_000,
      );
      await serverRecoveryCommitted;
      approvalCoordinator.allow(`${sender}-request-committed-1`, cycle.number);
    }

    if (sender !== 'web') {
      await waitForWebRecoveryBadgeCount(page, 1, secretName);
    } else {
      await nativeProcesses.get(removalApprover).test.waitForMarker(
        `E2E: ${removalApprover.toUpperCase()}_RECOVERY_READY_FOR_REMOVAL_${cycle.number}_1`,
        180_000,
      );
    }
    if (sender !== 'web') await nativeProcesses.get(sender).test.result;
    writeDiagnostic(`TEST15 cycle=${cycle.number} pending recovery observed before removal`);

    await dismissWebRecoveryWaitingUi(page, secretName);
    const removalTargetName = removalTargetPlatform === 'cli'
      ? previousCliDevice?.deviceName
      : initialDeviceNames[removalTargetPlatform];
    const removalTargetCategory = removalTargetPlatform === 'cli'
      ? 'CLI'
      : removalTargetPlatform === 'ios' ? 'iPhone' : 'Android';
    if (!removalTargetName) throw new Error(`Test #15 could not resolve removal target ${removalTargetPlatform}`);

    const removalEnvironment = {
      ...stepEnvironment({
        role: 'removal-approver', cycle: cycle.number, step: 1,
        sender, secretName, expectedOutcome: 'membership-removal',
      }),
      E2E_JOIN_TARGET: removalTargetName,
    };
    let removalApproval;
    if (removalApprover === 'android') {
      removalApproval = await startAndroidStepTest(androidSerial, {
        role: 'removal-approver', cycle: cycle.number, step: 1,
        joinTarget: removalTargetName,
        expectedSecretNames: [secretName], expectedDeviceNames: initialDeviceNames,
        testClass: 'metasecret.project.com.Case37AndroidMemberRemovalTest',
        testMethod: 'approveRemovalDuringRecoveryForTest15',
        label: `Android Test #15 approves removal during recovery cycle ${cycle.number}`,
      });
    } else {
      removalApproval = startIosJoinTest(simulatorUdid, {
        testClass: scenario.ios.testClass,
        testMethod: 'approveRemovalDuringRecoveryForTest15',
        label: `iOS Test #15 approves removal during recovery cycle ${cycle.number}`,
        environment: removalEnvironment,
        secretName,
        expectedDeviceNames: initialDeviceNames,
      });
      void removalApproval.result.catch(() => {});
    }
    await removalApproval.waitForMarker(
      `E2E: ${removalApprover.toUpperCase()}_TEST15_REMOVAL_APPROVER_READY`,
      180_000,
    );
    const removedTarget = await initiateWebDeviceRemoval(page, {
      targetCategory: removalTargetCategory,
      expectedTargetDeviceName: removalTargetName,
      waitForTargetGone: false,
      label: `Test #15 cycle ${cycle.number} removal during recovery`,
    });
    await removalApproval.waitForMarker(
      `E2E: ${removalApprover.toUpperCase()}_TEST15_REMOVAL_REQUEST_VISIBLE`,
      180_000,
    );
    approvalCoordinator.allow('test15-remove-approve', cycle.number);
    await removalApproval.waitForMarker(
      `E2E: ${removalApprover.toUpperCase()}_TEST15_REMOVAL_APPROVED_RECOVERY_CLEARED`,
      180_000,
    );
    await removedTarget.row.waitFor({
      state: 'hidden',
      timeout: 180_000,
    });
    writeDiagnostic(`TEST15 cycle=${cycle.number} membership change declined active Recover before CLI join`);

    const cliDevice = await createMetaCliDevice(cycle);
    await approveJoinRequestOnWeb(page, 'CLI');
    // Keep the owner Web session and its state-event subscription alive across
    // membership acceptance. The passing flow emitted the new Split while the
    // owner processed that invalidation; reloading or navigating another actor
    // first can discard that event-driven handoff.
    await activeServerProcess.waitForMarker(
      new RegExp(
        `distribution_type: Split, receivers: \\[([^\\]]*Base64Text\\("${cliDevice.deviceId}"\\)[^\\]]*)\\]`,
      ),
      180_000,
    );
    // Only after the owner's exact Split is committed, let the retained native
    // actor perform its UI refresh and read the new member's fresh Core state.
    approvalCoordinator.allow('test15-cli-member-accepted', cycle.number);
    const nativeRefreshPlatform = removalApprover.toUpperCase();
    await removalApproval.waitForMarker(
      `E2E: ${nativeRefreshPlatform}_TEST15_CLI_STATE_REFRESHED`,
      180_000,
    );
    await readMetaCliInfo(cliDevice, ['info', 'recovery-claims']);
    const cliClaimsInfo = await assertMetaCliMember(cliDevice, 3);
    approvalCoordinator.allow('test15-cli-redistribution-observed', cycle.number);
    await removalApproval.result;
    previousCliDevice = cliDevice;
    await assertWebMembership(
      page,
      ['Web', persistentNativeCategory, 'CLI'],
      [secretName],
      `Test #15 cycle ${cycle.number} post-join three-member boundary`,
    );
    const activeRecoveryClaims = (cliClaimsInfo.claims ?? [])
      .filter((claim) => claim.type === 'Recover' && claim.password === secretName)
      .map((claim) => ({
        id: claim.id,
        clientStatus: claim.clientStatus,
        receivers: (claim.receivers ?? []).map((receiver) => ({
          id: receiver.id,
          status: receiver.status,
        })),
      }));
    writeDiagnostic(
      `[CLI] post-join recovery claims secret=${secretName} `
      + JSON.stringify(activeRecoveryClaims),
    );

    await waitForWebIncomingRecoveryGone(page, secretName);
    await assertNoStaleWebRecoveryAlert(page, secretName);
    for (const { test } of nativeProcesses.values()) await test.result;

    const staleRecoveryClaims = (cliClaimsInfo.claims ?? [])
      .filter((claim) => claim.type === 'Recover' && claim.password === secretName);
    const cliInStaleRecovery = staleRecoveryClaims.some((claim) => (
      claim.receivers?.some((receiver) => receiver.id === cliDevice.deviceId)
        && (claim.receivers ?? []).some((receiver) => (
          ['Pending', 'Sent', 'Delivered', 'NeedApprove'].includes(receiver.status)
        ))
    ));
    if (cliInStaleRecovery) {
      throw new Error(
        `CLI unexpectedly received an actionable old recovery claim for ${secretName}; `
        + `device=${cliDevice.deviceId}`,
      );
    }
    const terminalOldRecovery = staleRecoveryClaims.length > 0
      && staleRecoveryClaims.every((claim) => (
        (claim.receivers ?? []).length > 0
          && (claim.receivers ?? []).every((receiver) => receiver.status === 'Declined')
      ));
    if (!terminalOldRecovery) {
      throw new Error(
        `Old recovery claim is not terminal after CLI join for ${secretName}: `
        + `${JSON.stringify(staleRecoveryClaims).slice(0, 2_000)}`,
      );
    }
    writeDiagnostic(
      `TEST15 cycle=${cycle.number} new CLI has Split only; `
      + 'all old Recover claims are Declined and no stale recovery was recreated',
    );
    await runTest15PostJoinRecoveryPlan(
      page,
      simulatorUdid,
      androidSerial,
      cliDevice,
      persistentNativeCategory,
    );
    console.log(
      `✅ Test #15 cycle ${cycle.number}/${expectedCycles} passed: one removal, one CLI rejoin, `
      + 'three approved post-join recovery checks',
    );
  }
}

function stepEnvironment({
  role,
  cycle,
  step,
  approvalPlatform,
  sender,
  secretName,
  action = '',
  expectedOutcome = '',
  verifyReceiverAlert = false,
  receiverRequestAlreadySent = false,
}) {
  return createStepEnvironment({ role, cycle, step, approvalPlatform, sender, secretName,
    action, expectedOutcome, verifyReceiverAlert, receiverRequestAlreadySent });
}

async function runBothReceiversOfflineRecovery(page, simulatorUdid, androidSerial) {
  const blocks = scenario.recovery.offlineBlocks ?? [];
  const expectedBlockCount = scenario.recovery.expectedBlockCount ?? 3;
  const expectedCycles = scenario.recovery.expectedCycles ?? expectedBlockCount * 6;
  if (blocks.length !== expectedBlockCount) {
    throw new Error(`Test #8 requires ${expectedBlockCount} sender block(s)`);
  }
  let cycle = 0;

  for (const block of blocks) {
    if (block.count !== 3) throw new Error(`Test #8 block ${block.name} must repeat exactly three times`);
    if (block.receivers.length !== 2 || block.approvalOrder.length !== 2) {
      throw new Error(`Test #8 block ${block.name} must have two receivers and two approval steps`);
    }
    const secretName = defaultSecret?.name;
    for (let repeat = 1; repeat <= block.count; repeat += 1) {
      for (let step = 1; step <= 2; step += 1) {
        cycle += 1;
        setCurrentCycleForDiagnostics(cycle);
        const approver = block.approvalOrder[step - 1];
        const nonApprover = block.receivers.find((receiver) => receiver !== approver);
        console.log(
          `Test #8 cycle ${cycle}/${expectedCycles}: sender=${block.sender} `
          + `repeat=${repeat}/3 approval=${approver} then dismiss=${nonApprover}`,
        );

        // Receivers are deliberately stopped before the sender creates its
        // claim. They are restarted only after the request marker exists.
        for (const receiver of block.receivers) {
          if (receiver === 'android') await stopAndroidApplication(androidSerial);
          if (receiver === 'ios') await stopIosApplication(simulatorUdid);
          if (receiver === 'web') await stopWebApplication(page);
        }
        if (block.sender === 'web') await reopenWebAfterOffline(page);

        let senderProcess;
        if (block.sender === 'ios') {
          senderProcess = startIosJoinTest(simulatorUdid, {
            testClass: scenario.ios.testClass,
            testMethod: scenario.ios.stepTestMethod,
            label: `iOS sender cycle ${cycle}`,
            environment: stepEnvironment({
              role: 'sender', cycle, step, approvalPlatform: approver, sender: block.sender,
            }),
          });
          void senderProcess.result.catch(() => {});
          approvalCoordinator.allow(`ios-sender-${step}`, cycle);
          await senderProcess.waitForMarker(`E2E: IOS_RECOVERY_REQUEST_SENT_${cycle}_${step}`, 180_000);
        } else if (block.sender === 'android') {
          senderProcess = await startAndroidStepTest(androidSerial, {
            role: 'sender', cycle, step, approvalPlatform: approver, sender: block.sender,
            label: `Android sender cycle ${cycle}`,
          });
          approvalCoordinator.allow(`android-sender-${step}`, cycle);
          await senderProcess.waitForMarker(`E2E: ANDROID_RECOVERY_REQUEST_SENT_${cycle}_${step}`, 180_000);
        } else {
          await startWebRecovery(page, secretName);
        }

        const receiverProcesses = [];
        for (const receiver of block.receivers) {
          if (receiver === 'android') {
            receiverProcesses.push({
              platform: receiver,
              test: await startAndroidStepTest(androidSerial, {
                role: 'receiver', cycle, step, approvalPlatform: approver, sender: block.sender,
                receiverRequestAlreadySent: true,
                label: `Android receiver cycle ${cycle}`,
              }),
            });
          } else if (receiver === 'ios') {
            const test = startIosJoinTest(simulatorUdid, {
              testClass: scenario.ios.testClass,
              testMethod: scenario.ios.stepTestMethod,
              label: `iOS receiver cycle ${cycle}`,
              environment: stepEnvironment({
                role: 'receiver', cycle, step, approvalPlatform: approver, sender: block.sender,
                receiverRequestAlreadySent: true,
              }),
            });
            void test.result.catch(() => {});
            receiverProcesses.push({ platform: receiver, test });
          } else {
            await reopenWebAfterOffline(page);
            receiverProcesses.push({ platform: receiver, test: null });
          }
        }

        for (const receiverProcess of receiverProcesses) {
          if (receiverProcess.platform === 'web') {
            await waitForWebIncomingRecoveryCount(page, 1, secretName);
          } else {
            const marker = receiverProcess.platform === 'ios'
              ? `E2E: IOS_INCOMING_VISIBLE_${cycle}_${step}`
              : `E2E: ANDROID_INCOMING_VISIBLE_${cycle}_${step}`;
            await receiverProcess.test.waitForMarker(marker, 180_000);
          }
        }
        console.log(`✅ Test #8 cycle ${cycle}: both receiver alerts visible`);

        if (approver === 'web') {
          await approveIncomingRecoveryOnWeb(page, secretName, 1);
        } else {
          approvalCoordinator.allow(`${approver}-approve-${step}`, cycle);
          const approving = receiverProcesses.find((entry) => entry.platform === approver);
          const marker = approver === 'ios'
            ? `E2E: IOS_APPROVED_INCOMING_${cycle}_${step}`
            : `E2E: ANDROID_APPROVED_INCOMING_${cycle}_${step}`;
          await approving.test.waitForMarker(marker, 180_000);
        }

        if (nonApprover === 'web') {
          await waitForWebIncomingRecoveryGone(page, secretName);
        } else {
          approvalCoordinator.allow(`${nonApprover}-dismiss-${step}`, cycle);
          const dismissed = receiverProcesses.find((entry) => entry.platform === nonApprover);
          const marker = nonApprover === 'ios'
            ? `E2E: IOS_DISMISSED_INCOMING_${cycle}_${step}`
            : `E2E: ANDROID_DISMISSED_INCOMING_${cycle}_${step}`;
          await dismissed.test.waitForMarker(marker, 180_000);
        }
        console.log(`✅ Test #8 cycle ${cycle}: ${approver} approved and ${nonApprover} alert disappeared`);

        if (block.sender === 'web') {
          await revealAndCloseWebSecret(page, secretName, false);
        } else {
          approvalCoordinator.allow(`${block.sender}-show-${step}`, cycle);
          const marker = block.sender === 'ios'
            ? `E2E: IOS_RECOVERY_CLOSED_${cycle}_${step}`
            : `E2E: ANDROID_RECOVERY_CLOSED_${cycle}_${step}`;
          await senderProcess.waitForMarker(marker, 180_000);
          await senderProcess.result;
        }

        for (const receiverProcess of receiverProcesses) {
          if (receiverProcess.test) await receiverProcess.test.result;
        }
        // Let each instrumentation/UI process publish its final result before
        // stopping its application. Killing the target first can leave
        // `am instrument` waiting forever after its E2E marker was emitted.
        for (const receiver of block.receivers) {
          if (receiver === 'android') await stopAndroidApplication(androidSerial);
          if (receiver === 'ios') await stopIosApplication(simulatorUdid);
          if (receiver === 'web') await stopWebApplication(page);
        }
      }
    }
  }
  if (cycle !== expectedCycles) throw new Error(`Test #8 expected ${expectedCycles} recovery requests, ran ${cycle}`);
}

async function runSenderOfflineSetup(page, simulatorUdid, androidSerial) {
  const initiator = scenario.setup?.initiator;
  if (!initiator) throw new Error('Test #9 requires setup.initiator');

  if (initiator === 'web') {
    const joinPlatforms = scenario.mode === 'new-device-during-recovery'
      ? ['ios', 'android'].filter((platform) => scenario.newDevice?.oldReceivers?.includes(platform))
      : ['ios', 'android'];
    return runWebInitiatedSetup(page, simulatorUdid, androidSerial, { joinPlatforms });
  }

  if (initiator === 'android') {
    const joinPlatforms = scenario.mode === 'new-device-during-recovery'
      ? scenario.newDevice?.oldReceivers ?? []
      : ['web', 'ios'];
    const androidTest = await startAndroidJoinTest(androidSerial, {
      testClass: scenario.android.setupTestClass ?? scenario.android.testClass,
      joinPlatforms,
    });
    await androidTest.waitForMarker('E2E: ANDROID_INITIATOR_READY', 180_000);

    await page.goto(scenario.web.url, { waitUntil: 'domcontentloaded' });
    await unlockWithPasskeyIfNeeded(page);
    await page.getByPlaceholder('vault name').fill(scenario.vault.name);
    await page.getByRole('button', { name: 'Set Vault Name' }).click();
    await page.getByRole('button', { name: 'Join', exact: true }).click();
    await androidTest.waitForMarker('E2E: ANDROID_WEB_JOIN_APPROVED', 180_000);
    await page.getByRole('button', { name: '+ Add Secret' }).waitFor({ timeout: 120_000 });

    let iosTest = null;
    if (joinPlatforms.includes('ios')) {
      iosTest = startIosJoinTest(simulatorUdid, {
        testClass: scenario.ios.setupTestClass ?? scenario.ios.testClass,
        testMethod: scenario.ios.joinTestMethod,
        label: 'iOS Android-initiated join',
      });
      void iosTest.result.catch(() => {});
      await iosTest.waitForMarker('E2E: IOS_JOIN_REQUEST_SENT', 180_000);
      await androidTest.waitForMarker('E2E: ANDROID_IOS_JOIN_APPROVED', 180_000);
      await iosTest.waitForMarker('E2E: IOS_SECRETS_READY', 210_000);
    }
    await androidTest.waitForMarker('E2E: ANDROID_SECRETS_READY', 210_000);
    await Promise.all([iosTest?.result, androidTest.result]);
    // The setup instrumentation leaves MainActivity alive. Start the
    // sender phase from a clean Android process so its socket/native request
    // loop is foregrounded and can flush the first recovery event.
    await stopAndroidApplication(androidSerial);
    await waitForWebSecrets(page);
    return { iosTest: null, androidTest: null };
  }

  if (initiator === 'ios') {
    const joinPlatforms = scenario.mode === 'new-device-during-recovery'
      ? scenario.newDevice?.oldReceivers ?? []
      : ['web', 'android'];
    const iosTest = startIosJoinTest(simulatorUdid, {
      testClass: scenario.ios.setupTestClass ?? scenario.ios.testClass,
      testMethod: scenario.ios.joinTestMethod,
      label: 'iOS initiator setup',
      environment: { E2E_JOIN_PLATFORMS: joinPlatforms.join(',') },
    });
    void iosTest.result.catch(() => {});
    await iosTest.waitForMarker('E2E: IOS_INITIATOR_READY', 180_000);

    await page.goto(scenario.web.url, { waitUntil: 'domcontentloaded' });
    await unlockWithPasskeyIfNeeded(page);
    await page.getByPlaceholder('vault name').fill(scenario.vault.name);
    await page.getByRole('button', { name: 'Set Vault Name' }).click();
    await page.getByRole('button', { name: 'Join', exact: true }).click();
    await iosTest.waitForMarker('E2E: IOS_WEB_JOIN_APPROVED', 180_000);
    await page.getByRole('button', { name: '+ Add Secret' }).waitFor({ timeout: 120_000 });

    let androidTest = null;
    if (joinPlatforms.includes('android')) {
      androidTest = await startAndroidJoinTest(androidSerial, {
        testClass: scenario.android.setupTestClass ?? scenario.android.testClass,
      });
      await androidTest.waitForMarker('E2E: ANDROID_JOIN_REQUEST_SENT', 180_000);
      await iosTest.waitForMarker('E2E: IOS_ANDROID_JOIN_APPROVED', 180_000);
      await androidTest.waitForMarker('E2E: ANDROID_JOIN_READY', 180_000);
      await androidTest.waitForMarker('E2E: ANDROID_SECRETS_READY', 210_000);
    }
    await iosTest.waitForMarker('E2E: IOS_SECRETS_READY', 210_000);
    await Promise.all([iosTest.result, androidTest?.result]);
    await waitForWebSecrets(page);
    return { iosTest: null, androidTest: null };
  }

  throw new Error(`Unsupported Test #9 setup initiator: ${initiator}`);
}

async function runSenderOfflineRecovery(page, simulatorUdid, androidSerial) {
  const expectedCycles = scenario.recovery.expectedCycles ?? recoveryCycles.length;
  if (recoveryCycles.length !== expectedCycles) {
    throw new Error(`Test #9 expected ${expectedCycles} recovery requests, got ${recoveryCycles.length}`);
  }

  for (const cycle of recoveryCycles) {
    setCurrentCycleForDiagnostics(cycle);
    if (cycle.senders.length !== 1 || cycle.approvals.length !== 1) {
      throw new Error(`Test #9 cycle ${cycle.number} must have one sender and one approver`);
    }
    const sender = cycle.senders[0];
    const approver = cycle.approvals[0].platform;
    const secretName = cycle.senderSecrets[sender] ?? cycle.approvals[0].secret ?? defaultSecret?.name;
    const receivers = ['web', 'ios', 'android'].filter((platform) => platform !== sender);
    const nonApprover = receivers.find((platform) => platform !== approver);
    const step = 1;
    // Web's recover_js call starts asynchronously from the UI click. Keep the
    // sender page alive until at least one receiver observes the request; this
    // is the synchronization point proving the request reached the server.
    let webSenderRequestPending = false;
    let androidSenderRequestPending = false;
    console.log(
      `Test #9 cycle ${cycle.number}/${expectedCycles}: sender=${sender} secret=${secretName} `
        + `approver=${approver} nonApprover=${nonApprover}`,
    );
    writeDiagnostic(
      `TEST9 cycle=${cycle.number} sender=${sender} secret=${secretName} `
        + `approver=${approver} nonApprover=${nonApprover}`,
    );

    let senderRequest;
    if (sender === 'web') {
      await reopenWebAfterOffline(page);
      approvalCoordinator.allow('web-sender', cycle.number);
      await startWebRecovery(page, secretName);
      writeDiagnostic(`TEST9 cycle=${cycle.number} web request dialog opened; taking sender offline`);
      webSenderRequestPending = true;
    } else if (sender === 'ios') {
      senderRequest = startIosJoinTest(simulatorUdid, {
        testClass: scenario.ios.testClass,
        testMethod: scenario.ios.requestTestMethod ?? 'sendRecoveryRequestAndExit',
        label: `iOS sender request cycle ${cycle.number}`,
        secretName,
        environment: stepEnvironment({
          role: 'sender-offline', cycle: cycle.number, step,
          approvalPlatform: approver, sender, secretName,
        }),
      });
      void senderRequest.result.catch(() => {});
      approvalCoordinator.allow('ios-sender-1', cycle.number);
      await senderRequest.waitForMarker(`E2E: IOS_RECOVERY_REQUEST_SENT_${cycle.number}_1`, 180_000);
      await senderRequest.result;
      await stopIosApplication(simulatorUdid);
    } else {
      // The Android native service emits a cycle-independent recover-result
      // marker. Clear the emulator buffer before this sender step so the
      // waiter cannot consume a result from an earlier cycle.
      await runAdbWithRetry(['-s', androidSerial, 'logcat', '-c']);
      senderRequest = await startAndroidStepTest(androidSerial, {
        role: 'sender-offline', cycle: cycle.number, step,
        approvalPlatform: approver, sender, secretName,
        testMethod: scenario.android.requestTestMethod ?? 'sendRecoveryRequestAndExit',
        label: `Android sender request cycle ${cycle.number}`,
      });
      approvalCoordinator.allow('android-sender-1', cycle.number);
      await senderRequest.waitForMarker(`E2E: ANDROID_RECOVERY_REQUEST_SENT_${cycle.number}_1`, 180_000);
      await senderRequest.waitForMarker(`E2E: ANDROID_NATIVE_RECOVER_RESULT secret=${secretName}`, 180_000);
      approvalCoordinator.allow('android-request-persisted', cycle.number);
      await senderRequest.result;
      // The marker is emitted immediately after the Compose click, while the
      // socket event may still be in flight. Keep the sender app alive until
      // a receiver observes the request; stopping it here can lose the event
      // before the server persists it.
      androidSenderRequestPending = true;
    }

    const receiverProcesses = [];
    for (const receiver of receivers) {
      if (receiver === 'web') {
        await reopenWebAfterOffline(page);
        receiverProcesses.push({ platform: receiver, test: null });
      } else if (receiver === 'ios') {
        const test = startIosJoinTest(simulatorUdid, {
          testClass: scenario.ios.testClass,
          testMethod: scenario.ios.receiverTestMethod ?? scenario.ios.stepTestMethod,
          label: `iOS receiver cycle ${cycle.number}`,
          secretName,
          environment: stepEnvironment({
          role: 'receiver', cycle: cycle.number, step,
            approvalPlatform: approver, sender, secretName, receiverRequestAlreadySent: true,
          }),
        });
        void test.result.catch(() => {});
        receiverProcesses.push({ platform: receiver, test });
      } else {
        receiverProcesses.push({
          platform: receiver,
          test: await startAndroidStepTest(androidSerial, {
            role: 'receiver', cycle: cycle.number, step,
            approvalPlatform: approver, sender, secretName, receiverRequestAlreadySent: true,
            testMethod: scenario.android.receiverTestMethod ?? scenario.android.stepTestMethod,
            label: `Android receiver cycle ${cycle.number}`,
          }),
        });
      }
    }

    for (const receiverProcess of receiverProcesses) {
      if (receiverProcess.platform === 'web') {
        await waitForWebIncomingRecoveryCount(page, 1, secretName);
      } else {
        const marker = receiverProcess.platform === 'ios'
          ? `E2E: IOS_INCOMING_VISIBLE_${cycle.number}_1`
          : `E2E: ANDROID_INCOMING_VISIBLE_${cycle.number}_1`;
        await receiverProcess.test.waitForMarker(marker, 180_000);
      }
      // Once a receiver has observed the request, the sender can safely go
      // offline. The remaining receiver must still process the persisted claim.
      if (webSenderRequestPending) {
        await stopWebApplication(page);
        webSenderRequestPending = false;
        writeDiagnostic(`TEST9 cycle=${cycle.number} web request observed by ${receiverProcess.platform}; sender offline`);
      }
      if (androidSenderRequestPending) {
        await stopAndroidApplication(androidSerial);
        androidSenderRequestPending = false;
        writeDiagnostic(`TEST9 cycle=${cycle.number} android request observed by ${receiverProcess.platform}; sender offline`);
      }
    }
    console.log(`✅ Test #9 cycle ${cycle.number}: both receivers saw the request`);

    if (approver === 'web') {
      await approveIncomingRecoveryOnWeb(page, secretName, 1);
    } else {
      approvalCoordinator.allow(`${approver}-approve-1`, cycle.number);
      const approving = receiverProcesses.find((entry) => entry.platform === approver);
      const marker = approver === 'ios'
        ? `E2E: IOS_APPROVED_INCOMING_${cycle.number}_1`
        : `E2E: ANDROID_APPROVED_INCOMING_${cycle.number}_1`;
      await approving.test.waitForMarker(marker, 180_000);
    }

    if (sender === 'web') {
      await reopenWebAfterOffline(page);
      await revealAndCloseWebSecret(page, secretName, true);
    } else if (sender === 'ios') {
      const show = startIosJoinTest(simulatorUdid, {
        testClass: scenario.ios.testClass,
        testMethod: scenario.ios.showTestMethod ?? 'showAcceptedRecoveryAfterOffline',
        label: `iOS sender return cycle ${cycle.number}`,
        secretName,
        environment: stepEnvironment({
          role: 'sender-returned', cycle: cycle.number, step,
          approvalPlatform: approver, sender, secretName,
        }),
      });
      void show.result.catch(() => {});
      approvalCoordinator.allow('ios-show-1', cycle.number);
      await show.waitForMarker(`E2E: IOS_RECOVERY_SECRET_VISIBLE_${cycle.number}_1`, recoveryShowTimeoutMs);
      await show.waitForMarker(`E2E: IOS_RECOVERY_CLOSED_${cycle.number}_1`, 60_000);
      await show.result;
    } else {
      const show = await startAndroidStepTest(androidSerial, {
        role: 'sender-returned', cycle: cycle.number, step,
        approvalPlatform: approver, sender, secretName,
        testMethod: scenario.android.showTestMethod ?? 'showAcceptedRecoveryAfterOffline',
        label: `Android sender return cycle ${cycle.number}`,
      });
      approvalCoordinator.allow('android-show-1', cycle.number);
      await show.waitForMarker(`E2E: ANDROID_RECOVERY_SECRET_VISIBLE_${cycle.number}_1`, recoveryShowTimeoutMs);
      await show.waitForMarker(`E2E: ANDROID_RECOVERY_CLOSED_${cycle.number}_1`, 60_000);
      await show.result;
    }

    // A non-approving receiver remains in NEED_APPROVE until the sender has
    // completed Show. Only then should its alert disappear; release the
    // receiver's assertion gate after sender completion.
    if (nonApprover === 'web') {
      await waitForWebIncomingRecoveryGone(page, secretName);
    } else {
      approvalCoordinator.allow(`${nonApprover}-dismiss-1`, cycle.number);
      const dismissed = receiverProcesses.find((entry) => entry.platform === nonApprover);
      const marker = nonApprover === 'ios'
        ? `E2E: IOS_DISMISSED_INCOMING_${cycle.number}_1`
        : `E2E: ANDROID_DISMISSED_INCOMING_${cycle.number}_1`;
      await dismissed.test.waitForMarker(marker, 180_000);
    }
    console.log(`✅ Test #9 cycle ${cycle.number}: ${approver} approved, sender showed, and ${nonApprover} alert disappeared`);

    for (const receiverProcess of receiverProcesses) {
      if (receiverProcess.test) await receiverProcess.test.result;
    }
    for (const receiver of receivers) {
      if (receiver === 'android') await stopAndroidApplication(androidSerial);
      if (receiver === 'ios') await stopIosApplication(simulatorUdid);
      if (receiver === 'web') await stopWebApplication(page);
    }
    console.log(`✅ Test #9 cycle ${cycle.number}/${expectedCycles} passed`);
  }
}

async function runAndroidStaleAlertAssertion(serial) {
  console.log('15b. Starting Android assertion after recovery');
  await waitForOnlineAndroidDevice(serial, 120_000);
  const appApkPath = resolve(composeRoot, 'composeApp/build/outputs/apk/debug/composeApp-debug.apk');
  const testApkPath = resolve(composeRoot, 'composeApp/build/outputs/apk/androidTest/debug/composeApp-debug-androidTest.apk');
  await runAdbWithRetry(['-s', serial, 'install', '-r', appApkPath]);
  await runAdbWithRetry(['-s', serial, 'install', '-r', testApkPath]);
  const assertionClass = scenario.android.assertionClass
    ?? scenario.android.testClass
    ?? 'metasecret.project.com.CaseSevenAndroidOfflineReceiverTest';
  const assertionMethod = scenario.android.assertionMethod
    ?? 'assertNoStaleRecoveryAlertAfterOfflineRestart';
  const assertion = watchAndroidInstrumentation(
    serial,
    ['-e', 'class', `${assertionClass}#${assertionMethod}`],
    `${androidTestBundleId}/androidx.test.runner.AndroidJUnitRunner`,
  );
  await assertion.result;
}

async function stopWebApplication(page) {
  console.log('15d. Navigating Web receiver offline');
  await page.goto('about:blank', { waitUntil: 'load' });
}

async function reopenWebAfterOffline(page) {
  console.log('15e. Reopening Web receiver');
  await page.goto(scenario.web.url, { waitUntil: 'domcontentloaded' });
  await unlockWithPasskeyIfNeeded(page);
  await waitForWebSecrets(page);
}

async function assertNoStaleWebRecoveryAlert(page, secretName) {
  const openRequest = page.getByTestId(`open-recovery-request-${secretName}`);
  const badge = page.locator('li')
    .filter({ has: page.getByTestId(`secret-primary-action-${secretName}`) })
    .getByTestId('recovery-request-badge');
  await openRequest.waitFor({ state: 'hidden', timeout: 30_000 }).catch(() => {});
  await badge.waitFor({ state: 'hidden', timeout: 30_000 }).catch(() => {});
  const openVisible = await openRequest.isVisible().catch(() => false);
  const badgeVisible = await badge.isVisible().catch(() => false);
  if (openVisible || badgeVisible) {
    throw new Error(`Web receiver retained a stale recovery request for ${secretName}`);
  }
}

async function waitForWebIncomingRecoveryGone(page, secretName) {
  const openRequest = page.getByTestId(`open-recovery-request-${secretName}`);
  await openRequest.waitFor({ state: 'hidden', timeout: 120_000 });
  const row = page.locator('li').filter({ has: page.getByTestId(`secret-primary-action-${secretName}`) });
  await row.getByTestId('recovery-request-badge').waitFor({ state: 'hidden', timeout: 120_000 });
  console.log(`[UI][Web] incoming recovery closed secret=${secretName}`);
}

async function setupVirtualAuthenticator(page) {
  const cdp = await page.context().newCDPSession(page);
  await cdp.send('WebAuthn.enable');
  const { authenticatorId } = await cdp.send('WebAuthn.addVirtualAuthenticator', {
    options: {
      protocol: 'ctap2',
      transport: 'internal',
      hasResidentKey: true,
      hasUserVerification: true,
      isUserVerified: true,
      automaticPresenceSimulation: true,
    },
  });
  return { cdp, authenticatorId };
}

async function configurePrimaryTestPage(page) {
  // The WASM tracing layer writes every DEBUG state snapshot to
  // console.debug. Suppress that high-volume stream before application code
  // starts, including when Test #23 replaces a long-lived renderer.
  await page.addInitScript(() => {
    console.debug = () => {};
  });
  page.on('console', (message) => {
    const line = `[${message.type()}] ${message.text()}`;
    recordBrowserDiagnostic(line, {
      important: !['debug', 'info'].includes(message.type()),
    });
  });
  page.on('pageerror', (error) => {
    const line = `[pageerror] ${error.message}`;
    recordBrowserDiagnostic(line);
  });
  page.on('crash', () => {
    writeDiagnostic(`[BROWSER] Web page crashed during cycle ${currentCycleForDiagnostics ?? '?'} `
      + '(Playwright renderer crash)');
  });
}

async function cloneVirtualAuthenticator(targetPage) {
  if (!primaryVirtualAuthenticator) {
    throw new Error('Primary virtual authenticator is not initialized');
  }
  const credentials = await primaryVirtualAuthenticator.cdp
    .send('WebAuthn.getCredentials', {
      authenticatorId: primaryVirtualAuthenticator.authenticatorId,
    });
  const targetCdp = await targetPage.context().newCDPSession(targetPage);
  await targetCdp.send('WebAuthn.enable');
  const { authenticatorId } = await targetCdp.send('WebAuthn.addVirtualAuthenticator', {
    options: {
      protocol: 'ctap2',
      transport: 'internal',
      hasResidentKey: true,
      hasUserVerification: true,
      isUserVerified: true,
      automaticPresenceSimulation: true,
    },
  });
  for (const credential of credentials.credentials ?? []) {
    await targetCdp.send('WebAuthn.addCredential', {
      authenticatorId,
      credential: {
        credentialId: credential.credentialId,
        isResidentCredential: credential.isResidentCredential,
        rpId: credential.rpId,
        privateKey: credential.privateKey,
        userHandle: credential.userHandle,
        signCount: credential.signCount,
      },
    });
  }
  return { cdp: targetCdp, authenticatorId };
}

async function unlockWithPasskeyIfNeeded(page) {
  const createPasskeyButton = page.getByRole('button', { name: 'Create Passkey' });
  const authenticateButton = page.getByRole('button', { name: 'Authenticate with Passkey' });
  const vaultNameInput = page.getByPlaceholder('vault name');
  const secretsLink = page.getByRole('link', { name: 'Secrets', exact: true });
  const authError = page.locator('[data-slot="dialog-content"] .text-destructive').first();

  const writeUnlockDiagnostic = async (prefix) => {
    const bodyText = await page.locator('body').innerText().catch(() => '');
    const credentialCount = primaryVirtualAuthenticator
      ? await primaryVirtualAuthenticator.cdp
        .send('WebAuthn.getCredentials', {
          authenticatorId: primaryVirtualAuthenticator.authenticatorId,
        })
        .then(({ credentials }) => credentials?.length ?? 0)
        .catch(() => -1)
      : -1;
    writeDiagnostic(
      `[UI][Web] ${prefix} url=${page.url()} `
      + `title=${JSON.stringify(await page.title().catch(() => ''))} `
      + `credentialCount=${credentialCount} `
      + `body=${JSON.stringify(bodyText.slice(0, 2_000))}`,
    );
  };

  try {
    // A freshly loaded document always starts with an unauthenticated Pinia
    // store, even when its static navigation briefly renders before the auth
    // dialog mounts. Do not treat that transient navigation as readiness.
    // The in-document marker is set only after this helper has observed the
    // authenticated UI; it naturally disappears on reload/navigation.
    const authenticatedInThisDocument = await page.evaluate(
      () => window.__METASECRET_E2E_AUTHENTICATED === true,
    );
    const lockedAgain = await authenticateButton.isVisible().catch(() => false)
      || await createPasskeyButton.isVisible().catch(() => false);

    if (!authenticatedInThisDocument || lockedAgain) {
      const hasStoredCredential = await page.evaluate(
        () => Boolean(window.localStorage.getItem('credential_id')),
      );
      const authControl = hasStoredCredential ? authenticateButton : createPasskeyButton;
      const actionLabel = hasStoredCredential ? 'Authenticating with' : 'Creating';

      await authControl.waitFor({ state: 'visible', timeout: 30_000 });
      console.log(`5a. ${actionLabel} test passkey`);
      await authControl.click();

      const outcome = await Promise.race([
        vaultNameInput.waitFor({ state: 'visible', timeout: 30_000 }).then(() => 'ready'),
        secretsLink.waitFor({ state: 'visible', timeout: 30_000 }).then(() => 'ready'),
        authError.waitFor({ state: 'visible', timeout: 30_000 }).then(() => 'error'),
      ]);
      if (outcome === 'error') {
        const message = await authError.innerText().catch(() => 'unknown WebAuthn error');
        throw new Error(`Web test passkey action failed: ${message}`);
      }
      await authControl.waitFor({ state: 'hidden', timeout: 30_000 });
      await page.evaluate(() => {
        window.__METASECRET_E2E_AUTHENTICATED = true;
      });
      return;
    }

    await Promise.race([
      vaultNameInput.waitFor({ state: 'visible', timeout: 30_000 }),
      secretsLink.waitFor({ state: 'visible', timeout: 30_000 }),
    ]);
  } catch (error) {
    await writeUnlockDiagnostic('unlock readiness failed');
    throw error;
  }
}

async function approveJoinRequestOnWeb(page, deviceName, { strictCompletion = false } = {}) {
  console.log(`Web approving ${deviceName} join request`);
  await page.getByRole('link', { name: 'Devices', exact: true }).click();
  const pendingDeviceRow = page.getByTestId('pending-device-row');
  await pendingDeviceRow.waitFor({ state: 'visible', timeout: 120_000 });
  await pendingDeviceRow.click();
  await page.getByTestId('accept-join-request').click();
  const completion = pendingDeviceRow.waitFor({ state: 'detached', timeout: 120_000 });
  if (strictCompletion) {
    await completion;
    writeDiagnostic(`[TEST${testNumber}] Web Join approval completed for ${deviceName}; pending row detached`);
  } else {
    await completion.catch(() => {});
  }
}

async function closeWebSecret(page, secretValue = defaultSecret?.value) {
  if (!secretValue) throw new Error('Web reveal close requires the exact expected secret value');
  const revealedValue = page.getByText(secretValue, { exact: true });
  await revealedValue.waitFor({ state: 'visible', timeout: recoveryShowTimeoutMs });
  const dialog = page.locator('[data-slot="dialog-content"]').filter({ has: revealedValue });
  await dialog.waitFor({ state: 'visible', timeout: 30_000 });
  const closeButton = dialog.locator('[data-slot="dialog-close"]');
  const closeCount = await closeButton.count();
  if (closeCount !== 1 || !await closeButton.isVisible()) {
    throw new Error(`Web reveal close control is missing or ambiguous: count=${closeCount}`);
  }
  await closeButton.click();
  await Promise.all([
    revealedValue.waitFor({ state: 'hidden', timeout: 30_000 }),
    dialog.waitFor({ state: 'hidden', timeout: 30_000 }),
  ]);
  if (await revealedValue.isVisible().catch(() => false)) {
    throw new Error('Web reveal exact secret value remained visible after Close');
  }
}

async function waitForWebPreRequestUiReady(page, secretName, role, timeoutMs) {
  const row = page
    .locator('li')
    .filter({ has: page.getByTestId(`secret-primary-action-${secretName}`) });
  const action = row.getByTestId(`secret-primary-action-${secretName}`);
  const recover = action.filter({ hasText: /^\s*Recover\s*$/ });
  const show = action.filter({ hasText: /^\s*Show\s*$/ });
  const exactValue = page.getByText(secretValueForName(secretName), { exact: true });
  const dialog = page.locator('[data-slot="dialog-content"]');
  const alert = page.locator('[data-slot="alert-dialog-content"]');
  const badge = row.getByTestId('recovery-request-badge');
  const openRequest = row.getByTestId(`open-recovery-request-${secretName}`);

  await Promise.all([
    row.waitFor({ state: 'visible', timeout: timeoutMs }),
    action.waitFor({ state: 'visible', timeout: timeoutMs }),
  ]);

  if (role === 'sender') {
    const initialAction = await Promise.race([
      recover.waitFor({ state: 'visible', timeout: timeoutMs }).then(() => 'recover'),
      show.waitFor({ state: 'visible', timeout: timeoutMs }).then(() => 'show'),
    ]);
    if (initialAction === 'show') {
      writeDiagnostic(
        `[TEST${testNumber}] cycle=${currentCycleForDiagnostics ?? '?'} Web sender `
          + `consuming stale Show before recovery secret=${secretName}`,
      );
      await show.click();
      await exactValue.waitFor({ state: 'visible', timeout: recoveryShowTimeoutMs });
      await closeWebSecret(page, secretValueForName(secretName));
    }
    await recover.waitFor({ state: 'visible', timeout: timeoutMs });
  }

  await Promise.all([
    dialog.waitFor({ state: 'hidden', timeout: timeoutMs }),
    exactValue.waitFor({ state: 'hidden', timeout: timeoutMs }),
    badge.waitFor({ state: 'hidden', timeout: timeoutMs }),
    openRequest.waitFor({ state: 'hidden', timeout: timeoutMs }),
    alert.waitFor({ state: 'hidden', timeout: timeoutMs }),
  ]);
  if (role === 'sender') {
    const [actionCount, actionText] = await Promise.all([action.count(), action.textContent()]);
    if (actionCount !== 1 || actionText?.trim() !== 'Recover') {
      throw new Error(
        `Web sender did not reach exact Recover for ${secretName}: `
          + `count=${actionCount} text=${JSON.stringify(actionText?.trim() ?? null)}`,
      );
    }
  }
}

async function waitForRecoveryParticipantsReady(
  page,
  { cycleNumber, secretName, participants, timeoutMs = 240_000, label = `TEST${testNumber}` },
) {
  await Promise.all(participants.map(async ({ platform, role, test }) => {
    if (!['sender', 'receiver'].includes(role)) {
      throw new Error(`${label} cycle ${cycleNumber} has invalid readiness role=${role}`);
    }
    if (platform === 'web') {
      await waitForWebPreRequestUiReady(page, secretName, role, timeoutMs);
      return;
    }
    if (!test) {
      throw new Error(`${label} cycle ${cycleNumber} missing ${platform} ${role} process`);
    }
    await test.waitForMarker(
      `E2E: ${platform.toUpperCase()}_${role.toUpperCase()}_UI_READY_${cycleNumber}_1`,
      timeoutMs,
    );
  }));
  writeDiagnostic(
    `[${label}] cycle=${cycleNumber} exact pre-request readiness complete `
      + `participants=${participants.map(({ platform, role }) => `${platform}:${role}`).join(',')}`,
  );
}

async function waitForWebSecretValue(page, secretValue = defaultSecret?.value) {
  await page.getByText(secretValue, { exact: true })
    .waitFor({ state: 'visible', timeout: recoveryShowTimeoutMs });
}

async function waitForWebRecoveryBadgeCount(page, count, secretName) {
  const badge = page.getByTestId('recovery-request-badge');
  // The Web UI keeps a shared badge test id, scoped to the <li> for each
  // secret. Select that row through its secret-specific action instead of
  // assuming a per-secret badge id (the Compose UI has the latter).
  const scopedBadge = secretName
    ? page
      .locator('li')
      .filter({ has: page.getByTestId(`secret-primary-action-${secretName}`) })
      .getByTestId('recovery-request-badge')
    : badge;
  await scopedBadge.filter({ hasText: String(count) }).waitFor({ state: 'visible', timeout: 120_000 });
  console.log(`[UI][Web] recovery badge secret=${secretName ?? '<default>'} count=${count}`);
}

async function waitForWebRecoveryRequestCount(page, count, secretName) {
  const row = page
    .locator('li')
    .filter({ has: page.getByTestId(`secret-primary-action-${secretName}`) });
  const openRequest = row.getByTestId(`open-recovery-request-${secretName}`);
  const badge = row.getByTestId('recovery-request-badge');
  if (count === 0) {
    await Promise.all([
      openRequest.waitFor({ state: 'hidden', timeout: 120_000 }),
      badge.waitFor({ state: 'hidden', timeout: 120_000 }),
    ]);
  } else {
    await Promise.all([
      openRequest.waitFor({ state: 'visible', timeout: 120_000 }),
      badge.filter({ hasText: new RegExp(`^\\s*${count}\\s*$`) })
        .waitFor({ state: 'visible', timeout: 120_000 }),
    ]);
  }
  writeDiagnostic(
    `[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: recovery request state settled `
      + `secret=${secretName} remaining=${count}`,
  );
}

async function waitForWebIncomingRecoveryCount(page, count, secretName) {
  const badge = secretName
    ? page
      .locator('li')
      .filter({ has: page.getByTestId(`secret-primary-action-${secretName}`) })
      .getByTestId('recovery-request-badge')
    : page.getByTestId('recovery-request-badge');
  try {
    await badge.filter({ hasText: String(count) }).waitFor({ state: 'attached', timeout: 120_000 });
  } catch (error) {
    const badgeTexts = await badge.allTextContents().catch(() => []);
    const requestText = await page
      .getByTestId(`open-recovery-request-${secretName ?? defaultSecret?.name}`)
      .textContent()
      .catch(() => null);
    writeDiagnostic(
      `[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: expected incoming badge ${count} `
      + `for secret=${secretName ?? '<default>'}; `
      + `visibleBadges=${JSON.stringify(badgeTexts).slice(0, 1_000)} `
      + `openRequestText=${JSON.stringify(requestText).slice(0, 500)}`,
    );
    throw error;
  }
  console.log(`[UI][Web] incoming recovery secret=${secretName ?? '<default>'} count=${count}`);
}

async function waitForStrictWebIncomingVisible(page, count, secretName) {
  const row = page
    .locator('li')
    .filter({ has: page.getByTestId(`secret-primary-action-${secretName}`) });
  const badge = row
    .getByTestId('recovery-request-badge')
    .filter({ hasText: new RegExp(`^\\s*${count}\\s*$`) });
  const openRequest = row.getByTestId(`open-recovery-request-${secretName}`);

  try {
    await Promise.all([
      row.waitFor({ state: 'visible', timeout: 120_000 }),
      badge.waitFor({ state: 'visible', timeout: 120_000 }),
      openRequest.waitFor({ state: 'visible', timeout: 120_000 }),
    ]);

    const [rowCount, badgeCount, openCount, badgeText, badgeVisible, openVisible] = await Promise.all([
      row.count(),
      badge.count(),
      openRequest.count(),
      badge.textContent(),
      badge.isVisible(),
      openRequest.isVisible(),
    ]);
    if (
      rowCount !== 1
      || badgeCount !== 1
      || openCount !== 1
      || badgeText?.trim() !== String(count)
      || !badgeVisible
      || !openVisible
    ) {
      throw new Error(
        `Test #${testNumber} Web incoming request is not uniquely visible for ${secretName}: `
          + `rows=${rowCount} badges=${badgeCount} opens=${openCount} `
          + `badgeText=${JSON.stringify(badgeText?.trim() ?? null)} `
          + `badgeVisible=${badgeVisible} openVisible=${openVisible}`,
      );
    }
  } catch (error) {
    const [rowCount, badgeTexts, openCount] = await Promise.all([
      row.count().catch(() => -1),
      row.getByTestId('recovery-request-badge').allTextContents().catch(() => []),
      openRequest.count().catch(() => -1),
    ]);
    writeDiagnostic(
      `[TEST${testNumber}] cycle=${currentCycleForDiagnostics ?? '?'} Web receiver visibility failed `
        + `secret=${secretName} expectedCount=${count} rows=${rowCount} `
        + `badgeTexts=${JSON.stringify(badgeTexts).slice(0, 500)} opens=${openCount}`,
    );
    throw error;
  }
}

async function openStrictWebIncomingAlert(page, count, secretName, { dismissWithoutDecision = false } = {}) {
  await waitForStrictWebIncomingVisible(page, count, secretName);
  const row = page
    .locator('li')
    .filter({ has: page.getByTestId(`secret-primary-action-${secretName}`) });
  const openRequest = row.getByTestId(`open-recovery-request-${secretName}`);
  await openRequest.click();
  const alert = page.locator('[data-slot="alert-dialog-content"]');
  await alert.waitFor({ state: 'visible', timeout: 30_000 });
  const [alertCount, secretCount, approveVisible, declineVisible] = await Promise.all([
    alert.count(),
    alert.getByText(secretName, { exact: true }).count(),
    alert.getByRole('button', { name: 'Approve', exact: true }).isVisible(),
    alert.getByRole('button', { name: 'Decline', exact: true }).isVisible(),
  ]);
  if (alertCount !== 1 || secretCount !== 1 || !approveVisible || !declineVisible) {
    throw new Error(
      `Web incoming alert is not exact for ${secretName}: alerts=${alertCount} `
        + `secretLabels=${secretCount} approve=${approveVisible} decline=${declineVisible}`,
    );
  }
  writeDiagnostic(
    `[TEST${testNumber}] cycle=${currentCycleForDiagnostics ?? '?'} Web incoming alert visible secret=${secretName}`,
  );

  if (!dismissWithoutDecision) return;
  await page.keyboard.press('Escape');
  await alert.waitFor({ state: 'hidden', timeout: 30_000 });
  // Escape closes local alert state only. The unchanged badge/open action is
  // strict evidence that the observer made no recovery business decision.
  await waitForStrictWebIncomingVisible(page, count, secretName);
  writeDiagnostic(
    `[TEST${testNumber}] cycle=${currentCycleForDiagnostics ?? '?'} `
      + `Web observer alert dismissed without decision; request remains pending`,
  );
}

async function approveOpenIncomingRecoveryOnWeb(page, secretName) {
  const alert = page.locator('[data-slot="alert-dialog-content"]');
  await alert.waitFor({ state: 'visible', timeout: 30_000 });
  const approve = alert.getByRole('button', { name: 'Approve', exact: true });
  await approve.waitFor({ state: 'visible', timeout: 30_000 });
  await approve.click();
  await alert.waitFor({ state: 'hidden', timeout: 120_000 });
  console.log(`[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: incoming recovery approved`);
}

async function startWebRecovery(page, secretName = defaultSecret?.name) {
  console.log(`[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: starting recovery request for ${secretName}`);
  await page.getByRole('link', { name: 'Secrets', exact: true }).click();
  await page.getByTestId(`secret-primary-action-${secretName}`).click();
  await page.locator('[data-slot="dialog-content"]').waitFor({ state: 'visible', timeout: 30_000 });
  console.log(`[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: recovery waiting dialog opened for ${secretName}`);
}

async function startWebDuplicateRecovery(page, secretName = defaultSecret?.name) {
  console.log(`[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: starting duplicate recovery submit for ${secretName}`);
  await page.getByRole('link', { name: 'Secrets', exact: true }).click();
  const action = page.getByTestId(`secret-primary-action-${secretName}`);
  await action.click();
  await page.locator('[data-slot="dialog-content"]').waitFor({ state: 'visible', timeout: 30_000 });
  writeDiagnostic(`[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: first recovery submit dispatched`);

  // Issue the second submit immediately through the same UI action. If the
  // modal has already removed/blocked the action, that is the expected guard;
  // either outcome is recorded and the receiver-side count remains the
  // authoritative duplicate-claim assertion.
  let duplicateSent = false;
  try {
    await action.click({ timeout: 1_000 });
    duplicateSent = true;
  } catch (_) {
    // The first click normally changes the action state before a second click
    // can be delivered. This is a state-based guard, not a timing sleep.
  }
  writeDiagnostic(
    `[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: `
    + (duplicateSent ? 'duplicate recovery submit dispatched' : 'duplicate recovery submit blocked by UI guard'),
  );
  await dismissWebRecoveryWaitingUi(page, secretName);
}

async function createWebSecret(page, secretName) {
  const config = secretConfigFor(secretName);
  console.log(`[UI][Web] creating secret after join: ${config.name}`);
  await page.getByRole('link', { name: 'Secrets', exact: true }).click();
  await page.getByRole('button', { name: '+ Add Secret', exact: true }).click();
  const dialog = page.locator('[data-slot="dialog-content"]');
  await dialog.waitFor({ state: 'visible', timeout: 30_000 });
  await page.getByPlaceholder('Secret name').fill(config.name);
  await page.getByPlaceholder('Enter your secret').fill(config.value);
  await page.getByRole('button', { name: 'Add Secret', exact: true }).click();
  await page.getByText(config.name, { exact: true }).waitFor({ state: 'visible', timeout: 180_000 });
  await dialog.waitFor({ state: 'hidden', timeout: 30_000 }).catch(() => {});
  console.log(`[UI][Web] secret added after join: ${config.name}`);
}

async function createWebSecretsAfterJoin(page) {
  for (const secretName of secretCreationPlan?.afterWebJoin?.web ?? []) {
    await createWebSecret(page, secretName);
  }
}

async function dismissWebRecoveryWaitingUi(
  page,
  secretName = defaultSecret?.name,
) {
  console.log(`[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: closing sender waiting dialog`);
  const close = page.locator('[data-slot="dialog-content"] [data-slot="dialog-close"]');
  if (!await close.isVisible().catch(() => false)) {
    console.log(`[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: sender waiting dialog already closed`);
    return;
  }
  await close.click();
  await page.locator('[data-slot="dialog-content"]').waitFor({ state: 'hidden', timeout: 30_000 });
  console.log(`[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: sender waiting dialog closed`);
}

async function approveIncomingRecoveryOnWeb(
  page,
  incomingSecretName = defaultSecret?.name,
  expectedCount = null,
  { senderSecretName = null, expectedRemainingCount = null } = {},
) {
  if (senderSecretName) {
    await dismissWebRecoveryWaitingUi(page, senderSecretName);
  }
  if (expectedCount != null) {
    await waitForWebRecoveryBadgeCount(page, expectedCount, incomingSecretName);
  }
  console.log(
    `[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: waiting for incoming recovery marker `
    + `secret=${incomingSecretName}`,
  );
  const openRequest = page.getByTestId(`open-recovery-request-${incomingSecretName}`);
  await openRequest.waitFor({ state: 'visible', timeout: 120_000 });
  await openRequest.click();
  const approveButton = page.getByTestId('approve-recovery-request');
  let claimId;
  let persistedDecision;
  if (expectedRemainingCount != null) {
    claimId = await approveButton.getAttribute('data-claim-id');
    if (!claimId) {
      throw new Error(`Web approval dialog did not expose its exact claimId for ${incomingSecretName}`);
    }
    persistedDecision = waitForBrowserConsole(
      page,
      (message) => message.type() === 'log'
        && message.text().includes(`E2E_SSE_RECOVERY_DECISION claimId=${claimId} outcome=accepted`),
    );
  }
  await approveButton.click();
  if (persistedDecision) {
    await persistedDecision;
    writeDiagnostic(
      `[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: server SSE confirmed `
        + `recovery approval claimId=${claimId}`,
    );
  }
  if (expectedRemainingCount != null) {
    await waitForWebRecoveryRequestCount(page, expectedRemainingCount, incomingSecretName);
  }
  console.log(`[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: incoming recovery approved`);
  return claimId;
}

async function decideIncomingRecoveryOnWeb(
  page,
  incomingSecretName = defaultSecret?.name,
  action = 'approve',
  expectedCount = 1,
) {
  if (expectedCount != null) {
    await waitForWebRecoveryBadgeCount(page, expectedCount, incomingSecretName);
  }
  const openRequest = page.getByTestId(`open-recovery-request-${incomingSecretName}`);
  await openRequest.waitFor({ state: 'visible', timeout: 120_000 });
  await openRequest.click();
  const buttonName = action === 'decline' ? 'Decline' : 'Approve';
  await page.getByRole('button', { name: buttonName, exact: true }).click();
  await openRequest.waitFor({ state: 'hidden', timeout: 120_000 }).catch(() => {});
  await page.getByTestId(`recovery-request-badge-${incomingSecretName}`)
    .waitFor({ state: 'hidden', timeout: 120_000 }).catch(() => {});
  console.log(
    `[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: incoming recovery ${action}d `
    + `secret=${incomingSecretName}`,
  );
}

async function attemptWebApproveWhileOffline(page, secretName) {
  const openRequest = page.getByTestId(`open-recovery-request-${secretName}`);
  await openRequest.waitFor({ state: 'visible', timeout: 120_000 });
  await openRequest.click();
  const approve = page.getByRole('button', { name: 'Approve', exact: true });
  await approve.waitFor({ state: 'visible', timeout: 30_000 });
  // The click starts WebAuthn before the native recovery workflow is written.
  // Keep the browser offline until the local decision is actually persisted;
  // reconnecting immediately after click would test only the online path.
  const actionSubmitted = waitForBrowserConsole(
    page,
    (message) => message.type() === 'log' && message.text().includes('[Recovery] accept_recover done'),
  );
  await approve.click();
  await actionSubmitted;
  writeDiagnostic(
    `[NETWORK] Web offline approve dispatched secret=${secretName}; `
      + 'the page remains offline until the reconnect boundary',
  );
}

async function approveWebAfterReconnect(page, secretName) {
  const approve = page.getByRole('button', { name: 'Approve', exact: true });
  const outcome = await page.waitForFunction(
    ({ name }) => {
      const isVisible = (element) => Boolean(
        element
        && (element.offsetWidth || element.offsetHeight || element.getClientRects().length),
      );
      const approveButton = [...document.querySelectorAll('button')]
        .find((button) => isVisible(button) && button.textContent?.trim() === 'Approve');
      if (approveButton) return 'approve';
      const openRequest = document.querySelector(`[data-testid="open-recovery-request-${name}"]`);
      const badge = document.querySelector('[data-testid="recovery-request-badge"]');
      return !isVisible(openRequest) && !isVisible(badge) ? 'resolved' : false;
    },
    { name: secretName },
    { timeout: 120_000 },
  ).then((handle) => handle.jsonValue());
  if (outcome === 'approve') {
    try {
      // Reconnect refresh can remove this transient button between the
      // visibility check and the click. The offline response is already
      // persisted, so a detached locator means the refresh won the race;
      // wait for the resolved state instead of issuing a duplicate action.
      await approve.click({ timeout: 5_000 });
    } catch (error) {
      console.log(
        `[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: `
          + `Approve detached during reconnect refresh; waiting for resolution (${error.message})`,
      );
    }
  } else {
    console.log(
      `[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: `
      + 'recovery approval was retried automatically after reconnect',
    );
  }
  await waitForWebIncomingRecoveryGone(page, secretName);
}

async function runNetworkLossRecovery(page, simulatorUdid, androidSerial) {
  const expectedCycles = scenario.recovery.expectedCycles ?? recoveryCycles.length;
  if (recoveryCycles.length !== expectedCycles) {
    throw new Error(`Test #16 expected ${expectedCycles} recovery requests, got ${recoveryCycles.length}`);
  }

  const blocks = [...new Set(recoveryCycles.map((cycle) => cycle.block))];
  if (blocks.length !== 3) throw new Error(`Test #16 expected 3 blocks, got ${blocks.length}`);

  for (const block of blocks) {
    const blockCycles = recoveryCycles.filter((cycle) => cycle.block === block);
    const first = blockCycles[0];
    const sender = first.senders[0];
    const offlineReceiver = first.offlineReceiver;
    const participants = ['web', 'ios', 'android'];
    const observer = participants.find((platform) => platform !== sender && platform !== offlineReceiver);
    const secretName = first.senderSecrets[sender] ?? defaultSecret?.name;
    if (!sender || !offlineReceiver || !observer || sender === offlineReceiver) {
      throw new Error(
        `Test #16 block ${block} must define distinct sender/offline receiver/observer: `
          + `sender=${sender} offline=${offlineReceiver} observer=${observer}`,
      );
    }
    if (blockCycles.some((cycle) => (
      cycle.senders[0] !== sender
        || cycle.offlineReceiver !== offlineReceiver
        || (cycle.senderSecrets[sender] ?? defaultSecret?.name) !== secretName
    ))) {
      throw new Error(`Test #16 block ${block} changes roles or secret between repetitions`);
    }

    setCurrentCycleForDiagnostics(first);
    writeDiagnostic(
      `TEST16 block=${block} sender=${sender} offlineReceiver=${offlineReceiver} `
        + `observer=${observer} cycles=${blockCycles.map((cycle) => cycle.number).join(',')}`,
    );
    console.log(
      `Test #16 ${block}: sender=${sender}, offline receiver=${offlineReceiver}, `
        + `observer=${observer}, cycles=${blockCycles.map((cycle) => cycle.number).join(',')}`,
    );

    if (participants.includes('web')) await reopenWebAfterOffline(page);
    const nativeProcesses = new Map();
    const cycleNumbers = blockCycles.map((cycle) => cycle.number).join(',');
    const startNative = async (platform, role) => {
      const environment = {
        E2E_NETWORK_ROLE: role,
        E2E_NETWORK_BLOCK: block,
        E2E_NETWORK_CYCLES: cycleNumbers,
        E2E_CORE_SERVER_URL: networkProxyUrl(platform),
      };
      if (platform === 'ios') {
        const test = startIosJoinTest(simulatorUdid, {
          testClass: scenario.ios.testClass,
          testMethod: scenario.ios.networkStepTestMethod ?? 'runNetworkLossCycles',
          label: `iOS Test #16 ${role} block ${block}`,
          secretName,
          environment,
        });
        void test.result.catch(() => {});
        nativeProcesses.set(platform, { test, role });
      } else if (platform === 'android') {
        const test = await startAndroidStepTest(androidSerial, {
          role,
          networkRole: role,
          networkBlock: block,
          networkCycles: cycleNumbers,
          cycle: first.number,
          step: 1,
          sender,
          secretName,
          testMethod: scenario.android.networkStepTestMethod ?? 'runNetworkLossCycles',
          label: `Android Test #16 ${role} block ${block}`,
        });
        nativeProcesses.set(platform, { test, role });
      }
    };

    for (const platform of [sender, offlineReceiver, observer]) {
      if (platform !== 'web') await startNative(platform, platform === sender ? 'sender' : platform === offlineReceiver ? 'offline-receiver' : 'observer');
    }

    // Do not release the first recovery request until each native runner has
    // rendered its secret row. This is a state-driven startup barrier: without
    // it a fast Web request can arrive while Android is still launching, and
    // the test would wait forever for an alert that was missed by the UI.
    for (const [platform, process] of nativeProcesses) {
      await process.test.waitForMarker(
        `E2E: ${platform.toUpperCase()}_NETWORK_READY`,
        180_000,
      );
    }

    for (const cycle of blockCycles) {
      setCurrentCycleForDiagnostics(cycle);
      if (sender === 'web') {
        approvalCoordinator.allow('web-sender', cycle.number);
        await startWebRecovery(page, secretName);
      } else {
        approvalCoordinator.allow(`${sender}-sender-1`, cycle.number);
        await nativeProcesses.get(sender).test.waitForMarker(
          `E2E: ${sender.toUpperCase()}_RECOVERY_REQUEST_SENT_${cycle.number}_1`,
          180_000,
        );
      }

      if (offlineReceiver === 'web') {
        await waitForWebIncomingRecoveryCount(page, 1, secretName);
      } else {
        await nativeProcesses.get(offlineReceiver).test.waitForMarker(
          `E2E: ${offlineReceiver.toUpperCase()}_INCOMING_VISIBLE_${cycle.number}_1`,
          180_000,
        );
      }
      if (observer === 'web') {
        await waitForWebIncomingRecoveryCount(page, 1, secretName);
      } else {
        await nativeProcesses.get(observer).test.waitForMarker(
          `E2E: ${observer.toUpperCase()}_INCOMING_VISIBLE_${cycle.number}_1`,
          180_000,
        );
      }
      writeDiagnostic(`TEST16 cycle=${cycle.number} both receiver alerts visible`);

      if (offlineReceiver === 'web') {
        await page.context().setOffline(true);
        writeDiagnostic(`TEST16 cycle=${cycle.number} Web -> OFFLINE`);
        await attemptWebApproveWhileOffline(page, secretName);
        await page.context().setOffline(false);
        writeDiagnostic(`TEST16 cycle=${cycle.number} Web -> ONLINE`);
        await approveWebAfterReconnect(page, secretName);
      } else {
        await setNetworkGate(offlineReceiver, false);
        approvalCoordinator.allow(`${offlineReceiver}-offline-attempt-1`, cycle.number);
        await nativeProcesses.get(offlineReceiver).test.waitForMarker(
          `E2E: ${offlineReceiver.toUpperCase()}_OFFLINE_APPROVE_CLICKED_${cycle.number}`,
          120_000,
        );
        await setNetworkGate(offlineReceiver, true);
        approvalCoordinator.allow(`${offlineReceiver}-offline-online-1`, cycle.number);
        await nativeProcesses.get(offlineReceiver).test.waitForMarker(
          `E2E: ${offlineReceiver.toUpperCase()}_APPROVED_AFTER_RECONNECT_${cycle.number}_1`,
          180_000,
        );
      }

      if (sender === 'web') {
        await revealAndCloseWebSecret(page, secretName, false);
      } else {
        approvalCoordinator.allow(`${sender}-show-1`, cycle.number);
        await nativeProcesses.get(sender).test.waitForMarker(
          `E2E: ${sender.toUpperCase()}_RECOVERY_SECRET_VISIBLE_${cycle.number}_1`,
          recoveryShowTimeoutMs,
        );
        await nativeProcesses.get(sender).test.waitForMarker(
          `E2E: ${sender.toUpperCase()}_RECOVERY_CLOSED_${cycle.number}_1`,
          60_000,
        );
      }

      if (observer === 'web') {
        await waitForWebIncomingRecoveryGone(page, secretName);
      } else {
        approvalCoordinator.allow(`${observer}-observer-finish-1`, cycle.number);
        await nativeProcesses.get(observer).test.waitForMarker(
          `E2E: ${observer.toUpperCase()}_OBSERVER_CLOSED_${cycle.number}_1`,
          180_000,
        );
      }
      console.log(`✅ Test #16 cycle ${cycle.number}/${expectedCycles} passed`);
    }

    for (const entry of nativeProcesses.values()) await entry.test.result;
    if (nativeProcesses.has('android')) await stopAndroidApplication(androidSerial);
    if (nativeProcesses.has('ios')) await stopIosApplication(simulatorUdid);
  }
}

async function restartRecoveryServer(cycleNumber, testLabel) {
  writeDiagnostic(`${testLabel} cycle=${cycleNumber}: restarting server container=${serverContainer}`);
  console.log(`${testLabel} cycle ${cycleNumber}: restarting server`);
  await runAndWait('docker', ['restart', serverContainer]);
  await waitForHttp(scenario.server.url);
  writeDiagnostic(`${testLabel} cycle=${cycleNumber}: server HTTP ready after restart`);
}

async function revealWebAfterServerRestart(page, secretName) {
  const secretValue = secretValueForName(secretName);
  const primaryAction = page.getByTestId(`secret-primary-action-${secretName}`);
  const visibleSecret = page.getByText(secretValue, { exact: true });
  const state = await Promise.race([
    visibleSecret.waitFor({ state: 'visible', timeout: 120_000 }).then(() => 'visible'),
    primaryAction.filter({ hasText: /^\s*Show\s*$/ }).waitFor({ state: 'visible', timeout: 120_000 }).then(() => 'show'),
  ]);
  if (state === 'show') {
    await primaryAction.click();
    await visibleSecret.waitFor({ state: 'visible', timeout: recoveryShowTimeoutMs });
  }
  await closeWebSecret(page, secretValue);
}

async function runServerRestartRecovery(page, simulatorUdid, androidSerial) {
  const expectedCycles = scenario.recovery.expectedCycles ?? recoveryCycles.length;
  if (recoveryCycles.length !== expectedCycles) {
    throw new Error(`Test #17 expected ${expectedCycles} recovery requests, got ${recoveryCycles.length}`);
  }

  for (const cycle of recoveryCycles) {
    setCurrentCycleForDiagnostics(cycle);
    if (cycle.senders.length !== 1 || cycle.approvals.length !== 1) {
      throw new Error(`Test #17 cycle ${cycle.number} must define one sender and one approval`);
    }
    const sender = cycle.senders[0];
    const approver = cycle.approvals[0].platform;
    const secretName = cycle.senderSecrets[sender] ?? cycle.approvals[0].secret ?? defaultSecret?.name;
    const receivers = ['web', 'ios', 'android'].filter((platform) => platform !== sender);
    const nonApprover = receivers.find((platform) => platform !== approver);
    const step = 1;
    if (!receivers.includes(approver) || !nonApprover) {
      throw new Error(`Test #17 cycle ${cycle.number} has invalid receiver roles`);
    }

    writeDiagnostic(
      `TEST17 cycle=${cycle.number} sender=${sender} approver=${approver} `
      + `nonApprover=${nonApprover} secret=${secretName}`,
    );
    console.log(
      `Test #17 cycle ${cycle.number}/${expectedCycles}: sender=${sender} `
      + `approver=${approver} nonApprover=${nonApprover}`,
    );

    if (receivers.includes('web') || sender === 'web') await reopenWebAfterOffline(page);
    const nativeProcesses = new Map();
    const startNativeStep = async (platform, role) => {
      const environment = stepEnvironment({
        role,
        cycle: cycle.number,
        step,
        approvalPlatform: approver,
        sender,
      });
      if (platform === 'ios') {
        const test = startIosJoinTest(simulatorUdid, {
          testClass: scenario.ios.testClass,
          testMethod: scenario.ios.serverRestartStepTestMethod ?? scenario.ios.stepTestMethod ?? 'handleRecoveryStep',
          label: `iOS Test #17 ${role} cycle ${cycle.number}`,
          secretName,
          environment,
        });
        void test.result.catch(() => {});
        nativeProcesses.set(platform, { test, role });
      } else if (platform === 'android') {
        const test = await startAndroidStepTest(androidSerial, {
          role,
          cycle: cycle.number,
          step,
          approvalPlatform: approver,
          sender,
          secretName,
          testMethod: scenario.android.serverRestartStepTestMethod ?? scenario.android.stepTestMethod ?? 'handleRecoveryStep',
          label: `Android Test #17 ${role} cycle ${cycle.number}`,
        });
        nativeProcesses.set(platform, { test, role });
      }
    };

    for (const receiver of receivers) {
      if (receiver !== 'web') await startNativeStep(receiver, 'receiver');
    }
    if (sender !== 'web') await startNativeStep(sender, 'sender');

    await waitForRecoveryParticipantsReady(page, {
      cycleNumber: cycle.number,
      secretName,
      participants: [
        ...receivers.map((platform) => ({
          platform,
          role: 'receiver',
          test: nativeProcesses.get(platform)?.test,
        })),
        { platform: sender, role: 'sender', test: nativeProcesses.get(sender)?.test },
      ],
      label: 'TEST17',
    });

    if (sender === 'web') {
      approvalCoordinator.allow('web-sender', cycle.number);
      await startWebRecovery(page, secretName);
    } else {
      approvalCoordinator.allow(`${sender}-sender-${step}`, cycle.number);
      await nativeProcesses.get(sender).test.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_REQUEST_SENT_${cycle.number}_${step}`,
        180_000,
      );
    }

    for (const receiver of receivers) {
      if (receiver === 'web') {
        await waitForWebIncomingRecoveryCount(page, 1, secretName);
      } else {
        await nativeProcesses.get(receiver).test.waitForMarker(
          `E2E: ${receiver.toUpperCase()}_INCOMING_VISIBLE_${cycle.number}_${step}`,
          180_000,
        );
      }
    }
    writeDiagnostic(`TEST17 cycle=${cycle.number}: all receiver alerts visible before restart`);

    await restartRecoveryServer(cycle.number, 'TEST17');

    if (approver === 'web') {
      await decideIncomingRecoveryOnWeb(page, secretName, 'approve', 1);
    } else {
      approvalCoordinator.allow(`${approver}-approve-${step}`, cycle.number);
      await nativeProcesses.get(approver).test.waitForMarker(
        `E2E: ${approver.toUpperCase()}_APPROVED_INCOMING_${cycle.number}_${step}`,
        180_000,
      );
    }

    if (sender === 'web') {
      await revealWebAfterServerRestart(page, secretName);
    } else {
      approvalCoordinator.allow(`${sender}-show-${step}`, cycle.number);
      await nativeProcesses.get(sender).test.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_SECRET_VISIBLE_${cycle.number}_${step}`,
        recoveryShowTimeoutMs,
      );
      await nativeProcesses.get(sender).test.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_CLOSED_${cycle.number}_${step}`,
        60_000,
      );
    }

    if (nonApprover === 'web') {
      await waitForWebIncomingRecoveryGone(page, secretName);
    } else {
      approvalCoordinator.allow(`${nonApprover}-dismiss-${step}`, cycle.number);
      await nativeProcesses.get(nonApprover).test.waitForMarker(
        `E2E: ${nonApprover.toUpperCase()}_DISMISSED_INCOMING_${cycle.number}_${step}`,
        180_000,
      );
    }

    for (const entry of nativeProcesses.values()) await entry.test.result;
    if (nativeProcesses.has('android')) await stopAndroidApplication(androidSerial);
    if (nativeProcesses.has('ios')) await stopIosApplication(simulatorUdid);
    if (receivers.includes('web') || sender === 'web') await stopWebApplication(page);
    console.log(`✅ Test #17 cycle ${cycle.number}/${expectedCycles} passed`);
  }
}

async function runMultipleWebTabsRecovery(page, simulatorUdid, androidSerial) {
  const expectedCycles = scenario.recovery.expectedCycles ?? recoveryCycles.length;
  if (recoveryCycles.length !== expectedCycles) {
    throw new Error(`Test #19 expected ${expectedCycles} recovery requests, got ${recoveryCycles.length}`);
  }
  const secondPage = await page.context().newPage();
  await secondPage.setViewportSize({ width: 1100, height: 800 });
  secondPage.on('console', (message) => recordBrowserDiagnostic(`[web-tab-2][${message.type()}] ${message.text()}`, { important: !['debug', 'info'].includes(message.type()) }));
  secondPage.on('pageerror', (error) => recordBrowserDiagnostic(`[web-tab-2][pageerror] ${error.message}`));
  // Open the same authenticated route as the first tab. Auth state is kept
  // in the Pinia store per page, so the second tab still performs its own
  // passkey assertion, while localStorage supplies the credential id.
  await secondPage.goto(page.url(), { waitUntil: 'domcontentloaded' });
  // CDP virtual authenticators are target-scoped. Clone the credential from
  // the first tab into the second tab's authenticator so both tabs represent
  // the same Web device without creating a second vault identity.
  await cloneVirtualAuthenticator(secondPage);
  await unlockWithPasskeyIfNeeded(secondPage);
  await waitForWebSecrets(secondPage);

  try {
    for (const cycle of recoveryCycles) {
      setCurrentCycleForDiagnostics(cycle);
      if (cycle.senders.length !== 1 || cycle.senders[0] === 'web') {
        throw new Error(`Test #19 cycle ${cycle.number} must use one native sender`);
      }
      const sender = cycle.senders[0];
      const secretName = cycle.senderSecrets[sender] ?? defaultSecret?.name;
      const observer = ['ios', 'android'].find((platform) => platform !== sender);
      const nativeProcesses = new Map();
      const step = 1;

      const startNativeStep = async (platform, role) => {
        const environment = stepEnvironment({
          role,
          cycle: cycle.number,
          step,
          approvalPlatform: 'web',
          sender,
        });
        if (platform === 'ios') {
          const test = startIosJoinTest(simulatorUdid, {
            testClass: scenario.ios.testClass,
            testMethod: scenario.ios.multiTabStepTestMethod ?? scenario.ios.stepTestMethod ?? 'handleRecoveryStep',
            label: `iOS Test #19 ${role} cycle ${cycle.number}`,
            secretName,
            environment,
          });
          void test.result.catch(() => {});
          nativeProcesses.set(platform, { test, role });
        } else if (platform === 'android') {
          const test = await startAndroidStepTest(androidSerial, {
            role,
            cycle: cycle.number,
            step,
            approvalPlatform: 'web',
            sender,
            secretName,
            testMethod: scenario.android.multiTabStepTestMethod ?? scenario.android.stepTestMethod ?? 'handleRecoveryStep',
            label: `Android Test #19 ${role} cycle ${cycle.number}`,
          });
          nativeProcesses.set(platform, { test, role });
        }
      };

      await startNativeStep(sender, 'sender');
      await startNativeStep(observer, 'receiver');
      await waitForRecoveryParticipantsReady(page, {
        cycleNumber: cycle.number,
        secretName,
        participants: [
          { platform: sender, role: 'sender', test: nativeProcesses.get(sender)?.test },
          { platform: observer, role: 'receiver', test: nativeProcesses.get(observer)?.test },
          { platform: 'web', role: 'receiver' },
        ],
        label: 'TEST19',
      });
      await waitForWebPreRequestUiReady(secondPage, secretName, 'receiver', 240_000);
      approvalCoordinator.allow(`${sender}-sender-${step}`, cycle.number);
      await nativeProcesses.get(sender).test.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_REQUEST_SENT_${cycle.number}_${step}`,
        180_000,
      );
      await Promise.all([
        waitForWebIncomingRecoveryCount(page, 1, secretName),
        waitForWebIncomingRecoveryCount(secondPage, 1, secretName),
        nativeProcesses.get(observer).test.waitForMarker(
          `E2E: ${observer.toUpperCase()}_INCOMING_VISIBLE_${cycle.number}_${step}`,
          180_000,
        ),
      ]);
      writeDiagnostic(`TEST19 cycle=${cycle.number}: both Web tabs and native observer saw incoming request`);

      await decideIncomingRecoveryOnWeb(secondPage, secretName, 'approve', 1);
      await waitForWebIncomingRecoveryGone(page, secretName);
      approvalCoordinator.allow(`${sender}-show-${step}`, cycle.number);
      await nativeProcesses.get(sender).test.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_SECRET_VISIBLE_${cycle.number}_${step}`,
        recoveryShowTimeoutMs,
      );
      await nativeProcesses.get(sender).test.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_CLOSED_${cycle.number}_${step}`,
        60_000,
      );
      approvalCoordinator.allow(`${observer}-dismiss-${step}`, cycle.number);
      await nativeProcesses.get(observer).test.waitForMarker(
        `E2E: ${observer.toUpperCase()}_DISMISSED_INCOMING_${cycle.number}_${step}`,
        180_000,
      );
      for (const entry of nativeProcesses.values()) await entry.test.result;
      if (nativeProcesses.has('android')) await stopAndroidApplication(androidSerial);
      if (nativeProcesses.has('ios')) await stopIosApplication(simulatorUdid);
      console.log(`✅ Test #19 cycle ${cycle.number}/${expectedCycles} passed: tab 2 approved, tab 1 closed`);
    }
  } finally {
    await secondPage.close().catch(() => {});
  }
}

async function decideIncomingRecoveryTwiceOnWeb(page, incomingSecretName = defaultSecret?.name) {
  await waitForWebRecoveryBadgeCount(page, 1, incomingSecretName);
  const openRequest = page.getByTestId(`open-recovery-request-${incomingSecretName}`);
  await openRequest.waitFor({ state: 'visible', timeout: 120_000 });
  await openRequest.click();
  const approve = page.getByRole('button', { name: 'Approve', exact: true });
  await approve.waitFor({ state: 'visible', timeout: 30_000 });
  await approve.click();

  // A second click is attempted only while the UI still exposes an enabled
  // Approve control. Usually the first click closes the alert immediately;
  // that disappearance is itself the expected duplicate-click guard.
  const secondTapAvailable = await approve.isVisible().catch(() => false)
    && await approve.isEnabled().catch(() => false);
  if (secondTapAvailable) {
    writeDiagnostic(
      `[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: repeat approve second click sent`,
    );
    await approve.click();
    writeDiagnostic(
      `[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: repeat approve second click completed`,
    );
  } else {
    writeDiagnostic(
      `[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: repeat approve second click skipped after dismiss`,
    );
  }
  await openRequest.waitFor({ state: 'hidden', timeout: 120_000 }).catch(() => {});
  await page.getByTestId(`recovery-request-badge-${incomingSecretName}`)
    .waitFor({ state: 'hidden', timeout: 120_000 }).catch(() => {});
  console.log(
    `[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: incoming recovery approved; `
    + 'repeat-click guard checked',
  );
}

async function assertWebRecoveryDeclined(page, secretName = defaultSecret?.name) {
  const secretValue = secretValueForName(secretName);
  await page.locator('[data-slot="dialog-content"]').waitFor({ state: 'hidden', timeout: 120_000 }).catch(() => {});
  await page.getByText(secretValue, { exact: true }).waitFor({ state: 'hidden', timeout: 10_000 }).catch(() => {});
  const primaryAction = page.getByTestId(`secret-primary-action-${secretName}`);
  await primaryAction.filter({ hasText: /^\s*Recover\s*$/ }).waitFor({ state: 'visible', timeout: 120_000 });
  const valueVisible = await page.getByText(secretValue, { exact: true }).isVisible().catch(() => false);
  if (valueVisible) throw new Error(`Web unexpectedly revealed ${secretName} after declined recovery`);
  console.log(`[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: declined recovery did not reveal ${secretName}`);
}

async function showAndCloseWebSecret(page, secretName = defaultSecret?.name) {
  const secretValue = secretValueForName(secretName);
  const primaryAction = page.getByTestId(`secret-primary-action-${secretName}`);
  await primaryAction.filter({ hasText: /^\s*Show\s*$/ }).waitFor({
    state: 'visible',
    timeout: 120_000,
  });
  await primaryAction.click();
  await waitForWebSecretValue(page, secretValue);
  await closeWebSecret(page, secretValue);
  await primaryAction.filter({ hasText: /^\s*Show\s*$/ }).waitFor({
    state: 'visible',
    timeout: 120_000,
  });
}

async function revealAndCloseWebSecret(page, secretName = defaultSecret?.name, reopenClaim = false) {
  const secretValue = secretValueForName(secretName);
  const primaryAction = page.getByTestId(`secret-primary-action-${secretName}`);
  if (webSenderRevealCompletedCycles.delete(currentCycleForDiagnostics)) {
    console.log(
      `[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: sender secret was already `
      + 'revealed after the first approval',
    );
    return;
  }
  if (reopenClaim) {
    console.log(`[UI][Web] reopening secret for reveal: ${secretName}`);
    const actionBeforeWait = await primaryAction.textContent().catch(() => null);
    console.log(
      `[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: waiting for primary action Show `
      + `(current=${JSON.stringify(actionBeforeWait?.trim() ?? null)})`,
    );
    // State invalidation is asynchronous after another device accepts the
    // sender's claim. Do not click while the button still says Recover: that
    // would create a second, unapproved recovery claim for the same cycle.
    await primaryAction.filter({ hasText: /^\s*Show\s*$/ }).waitFor({
      state: 'visible',
      timeout: 120_000,
    });
    const actionAfterWait = await primaryAction.textContent().catch(() => null);
    console.log(
      `[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: primary action ready `
      + `(current=${JSON.stringify(actionAfterWait?.trim() ?? null)})`,
    );
    await primaryAction.click();
  } else {
    console.log(`[UI][Web] waiting for the already-open sender dialog to reveal ${secretName}`);
  }
  try {
    await waitForWebSecretValue(page, secretValue);
  } catch (error) {
    const actionText = await primaryAction.textContent().catch(() => null);
    const waitingDialogVisible = await page
      .locator('[data-slot="dialog-content"]')
      .isVisible()
      .catch(() => false);
    const secretRowText = await page
      .locator('li')
      .filter({ has: primaryAction })
      .textContent()
      .catch(() => null);
    writeDiagnostic(
      `[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: reveal timeout `
      + `secret=${secretName}; primaryAction=${JSON.stringify(actionText?.trim() ?? null)}; `
      + `waitingDialogVisible=${waitingDialogVisible}; `
      + `secretRow=${JSON.stringify(secretRowText?.trim() ?? null).slice(0, 1_000)}`,
    );
    throw error;
  }
  console.log('[UI][Web] secret revealed; closing reveal dialog');
  await closeWebSecret(page, secretValue);
  console.log('[UI][Web] reveal dialog closed');
}

async function waitForWebSecretNames(page, secretNames) {
  await page.getByRole('link', { name: 'Secrets', exact: true }).click();
  for (const secretName of secretNames) {
    const config = secretConfigFor(secretName);
    if (!config?.name) continue;
    await page.getByText(config.name, { exact: true }).waitFor({ state: 'visible', timeout: 180_000 });
    await page.getByTestId(`secret-primary-action-${config.name}`).waitFor({ state: 'visible', timeout: 180_000 });
    console.log(`[UI][Web] secret ready: ${config.name}`);
  }
}

async function waitForWebSecrets(page) {
  return waitForWebSecretNames(page, Object.values(secretConfigs).map((config) => config.name));
}

async function refreshWebForLongRun(page, cycleNumber) {
  // Reuse the proven fresh-target lifecycle at completed boundaries. Reload
  // does not release the renderer heap. Context storage/device identity and
  // target-scoped passkeys survive, and the caller adopts the returned page.
  const expectedNames = [
    ...(secretCreationPlan?.initial?.web ?? []),
    ...(cycleNumber >= (scenario.recovery.createSecret2AfterCycle ?? Number.POSITIVE_INFINITY)
      ? (secretCreationPlan?.afterAndroidJoin?.web ?? [])
      : []),
    ...(cycleNumber >= (scenario.recovery.createSecret3AfterCycle ?? Number.POSITIVE_INFINITY)
      ? (secretCreationPlan?.afterRecovery?.web ?? [])
      : []),
  ];
  return refreshWebForKnownNames(page, cycleNumber, [...new Set(expectedNames)], [12, 21, 30]);
}

async function runConcurrentRecoveryCycles(page, iosTest, androidTest) {
  await page.getByRole('link', { name: 'Secrets', exact: true }).click();

  for (const cycle of recoveryCycles) {
    setCurrentCycleForDiagnostics(cycle);
    const senderDescription = cycle.senders
      .map((sender) => `${sender}:${cycle.senderSecrets[sender] ?? defaultSecret?.name}`)
      .join(' + ');
    console.log(
      `15.${cycle.number} ${senderDescription} request recovery concurrently; `
      + `${cycle.approvals.map((approval) => `${approval.platform}:${approval.secret}`).join(' then ')}`,
    );

    // These two actions intentionally overlap. Each creates a different
    // sender-owned claim. Claims are bound to the sender and secret, so a
    // request for Secret A can never consume the response for Secret B.
    const senderWaits = [];
    if (cycle.senders.includes('web')) {
      approvalCoordinator.allow('web-sender', cycle.number);
      senderWaits.push(startWebRecovery(page, cycle.senderSecrets.web));
    }
    if (cycle.senders.includes('android')) {
      approvalCoordinator.allow('android-sender', cycle.number);
      senderWaits.push(androidTest.waitForMarker(`E2E: ANDROID_RECOVERY_REQUEST_SENT_${cycle.number}`));
    }
    if (cycle.senders.includes('ios')) {
      approvalCoordinator.allow('ios-sender', cycle.number);
      senderWaits.push(iosTest.waitForMarker(`E2E: IOS_RECOVERY_REQUEST_SENT_${cycle.number}`, 180_000));
    }
    await Promise.all(senderWaits);

    // Do not grant any approver permission until every receiver has rendered
    // its incoming request. Native tests publish readiness markers before
    // they wait on the approval coordinator; Web is checked through its badge.
    const webReadySecrets = new Set();
    const nativeReadyPlatforms = new Set();
    if (cycle.senderSecrets.web && cycle.approvals.some(({ platform }) => platform === 'web')) {
      await dismissWebRecoveryWaitingUi(page, cycle.senderSecrets.web);
    }
    for (const [index, approval] of cycle.approvals.entries()) {
      const step = index + 1;
      if (approval.platform === 'web') {
        if (webReadySecrets.has(approval.secret)) continue;
        webReadySecrets.add(approval.secret);
        const expectedCount = cycle.approvals.filter(
          (pending) => pending.platform === 'web' && pending.secret === approval.secret,
        ).length;
        await waitForWebRecoveryBadgeCount(page, expectedCount, approval.secret);
        writeDiagnostic(
          `[TEST4] cycle=${cycle.number} Web receiver ready `
            + `secret=${approval.secret} badge=${expectedCount}`,
        );
      } else if (approval.platform === 'ios') {
        if (!nativeReadyPlatforms.has(approval.platform)) {
          await iosTest.waitForMarker(`E2E: IOS_INCOMING_VISIBLE_${cycle.number}_${step}`, 180_000);
          nativeReadyPlatforms.add(approval.platform);
        }
      } else if (approval.platform === 'android') {
        if (!nativeReadyPlatforms.has(approval.platform)) {
          await androidTest.waitForMarker(`E2E: ANDROID_INCOMING_VISIBLE_${cycle.number}_${step}`, 180_000);
          nativeReadyPlatforms.add(approval.platform);
        }
      }
    }

    for (const [index, approval] of cycle.approvals.entries()) {
      const step = index + 1;
      const approver = approval.platform;
      if (approver === 'web') {
        // A single Web secret row can have multiple concurrent incoming
        // recovery requests. Its badge counts all pending requests for that
        // secret, so wait for the number still assigned to Web at this step.
        const expectedWebBadgeCount = cycle.approvals
          .slice(index)
          .filter((pending) => pending.platform === 'web' && pending.secret === approval.secret)
          .length;
        const expectedRemainingCount = cycle.approvals
          .slice(index + 1)
          .filter((pending) => pending.secret === approval.secret)
          .length;
        const webSenderSecret = cycle.senderSecrets.web;
        await approveIncomingRecoveryOnWeb(page, approval.secret, expectedWebBadgeCount, {
          senderSecretName: webSenderSecret,
          expectedRemainingCount,
        });
        console.log(`[UI][Web] cycle ${cycle.number}: approval ${step} for ${approval.secret} complete`);
      } else {
        await (approver === 'ios' ? iosTest : androidTest).waitForMarker(
          `E2E: ${approver.toUpperCase()}_INCOMING_VISIBLE_${cycle.number}_${step}`,
          180_000,
        );
        approvalCoordinator.allow(`${approver}-approve-${step}`, cycle.number);
        const test = approver === 'ios' ? iosTest : androidTest;
        await test.waitForMarker(`E2E: ${approver.toUpperCase()}_APPROVED_INCOMING_${cycle.number}_${step}`, 180_000);
      }
    }

    for (const sender of cycle.senders) {
      const secretName = cycle.senderSecrets[sender] ?? defaultSecret?.name;
      if (sender === 'web') {
        await revealAndCloseWebSecret(
          page,
          secretName,
          cycle.approvals.some((approval) => approval.platform === 'web'),
        );
      } else {
        approvalCoordinator.allow(`${sender}-show`, cycle.number);
        const test = sender === 'ios' ? iosTest : androidTest;
        await test.waitForMarker(`E2E: ${sender.toUpperCase()}_RECOVERY_SECRET_VISIBLE_${cycle.number}`, recoveryShowTimeoutMs);
        await test.waitForMarker(`E2E: ${sender.toUpperCase()}_RECOVERY_CLOSED_${cycle.number}`, 60_000);
      }
    }
    // Test #6 processes three Secrets through 18 recovery cycles in one
    // visible Chromium renderer. Recycle at the completed parallel-block
    // boundary, as the long-running lifecycle tests do, to release renderer
    // memory without resetting the browser context, passkey, or IndexedDB.
    if (testNumber === 6) {
      page = await refreshWebForKnownNames(
        page,
        cycle.number,
        Object.values(secretConfigs).map((config) => config.name),
        [12],
      );
    }
    console.log(`✅ Concurrent recovery ${cycle.number}/${recoveryCycles.length} passed`);

  }
}

async function main() {
  mkdirSync(artifactsDirectory, { recursive: true });
  writeFileSync(diagnosticLogPath, `=== Test #${testNumber} started ${new Date().toISOString()} ===\n`);
  writeDiagnostic(`scenario=${scenario.name}; cycles=${recoveryCycles.length}`);
  if (!existsSync(webDirectory)) throw new Error(`Web directory not found: ${webDirectory}`);
  if (!existsSync(composeRoot)) throw new Error(`Compose directory not found: ${composeRoot}`);
  const serverUrl = new URL(scenario.server.url);
  const serverUsesLoopback = ['localhost', '127.0.0.1', '::1'].includes(serverUrl.hostname);
  const environmentMetadataIsLocal = scenario.server.environment === 'local'
    || (scenario.server.environment == null && serverUsesLoopback);
  if (!environmentMetadataIsLocal) {
    throw new Error(`Visible UI E2E requires server.environment=local, got ${scenario.server.environment}`);
  }

  await prepareLocalEnvironmentGuard();
  await ensureIosFfi({ coreRoot, composeRoot, runAndWait, evidenceDirectory: artifactsDirectory, onEvent: writeDiagnostic });
  if (reuseProvenArtifacts) verifyIosXcodeBuild({ derivedDataPath: iosDerivedDataPath });

  console.log(`\n=== ${scenario.name} ===`);
  approvalCoordinator = await startApprovalCoordinator();
  if (reuseProvenArtifacts) {
    if (!buildProvenanceVerified) throw new Error('Artifact reuse requested before provenance verification');
    console.log('1-2. Reusing exact fingerprint-verified WASM/native/server artifacts');
  } else {
    console.log('1. Rebuilding the Web WASM package');
    await runAndWait('/opt/homebrew/bin/task', ['wasm-local'], { cwd: coreRoot });

    console.log('2. Building local server image');
    await runAndWait('/opt/homebrew/bin/task', ['meta-server'], { cwd: coreRoot });
    await reclaimBuildCacheForVisibleRun();
  }

  console.log('3. Cleaning server state');
  // Preflight proved this scenario-specific name was not owned by another
  // process. Cleanup below only removes the container after this ownership
  // flag is set.
  console.log('4. Starting local server');
  const serverLogLevel = process.env.E2E_SERVER_LOG_LEVEL ?? 'info';
  const server = watchProcessOutput(
    'docker',
    [
      'run', '--rm', '--name', serverContainer,
      '-e', `RUST_LOG=${serverLogLevel}`,
      '-e', 'META_SERVER_E2E_SIGNALS=1',
      '-p', `${scenario.server.port}:3000`, serverImage,
    ],
    { diagnosticRole: 'e2e-server' },
  );
  activeServerProcess = server;
  serverContainerOwned = true;
  void server.result.catch(() => {});
  await waitForHttp(scenario.server.url);
  await startNetworkLossGates();

  console.log('5. Starting Web in a visible browser');
  run('npm', ['run', 'dev', '--', '--host', 'localhost', '--port', '5173'], { cwd: webDirectory });
  await waitForHttp(scenario.web.url);

  browserInstance = await chromium.launch({ headless: false });
  const browser = browserInstance;
  // Use an explicit context because Test #19 opens a second page in the
  // same browser context. The Browser.newPage() convenience API creates an
  // owned context that intentionally rejects context.newPage().
  const browserContext = await browser.newContext({ viewport: { width: 1280, height: 900 } });
  const page = await browserContext.newPage();
  await configurePrimaryTestPage(page);
  primaryVirtualAuthenticator = await setupVirtualAuthenticator(page);
  await preparePrimaryWebClient(page);

  // Clean the iOS simulator before Android starts. The Android helper
  // approves the first pending join request, so a left-over iOS UI-test app
  // from an interrupted run could otherwise be mistaken for the Web request.
  // Test #3 is safe without this because iOS is launched only after Web has
  // already been approved.
  const simulatorUdid = await prepareIosSimulator();
  if (reuseIosBuildForTesting) {
    if (reuseProvenArtifacts) {
      verifyIosXcodeBuild({ derivedDataPath: iosDerivedDataPath });
      console.log('8a. Reusing exact fingerprint-verified iOS build-for-testing bundle');
    } else {
      await prepareIosTestBundle(simulatorUdid);
    }
    await installBuiltIosTestApps({
      simulatorUdid,
      derivedDataPath: iosDerivedDataPath,
      appBundleId: scenario.ios.bundleId,
      runAndWait,
      runAndCapture,
    });
  }

  const androidSerial = await prepareAndroidEmulator();
  await installAndVerifyAndroidE2eApps({
    serial: androidSerial,
    appBundleId: scenario.android.bundleId,
    composeRoot,
    runAndWait,
    runAndCapture,
  });
  if (
    [
      'android-secondary-creator-lifecycle',
      'android-distributed-creator-lifecycle',
      'android-owner-creator-lifecycle',
    ]
      .includes(scenario.mode)
    && scenario.setup?.initiator === 'android'
  ) {
    await runAndroidSecondaryCreatorLifecycle(page, simulatorUdid, androidSerial);
    console.log(
      `✅ Test #${testNumber} Android-initiated multi-creator lifecycle passed: `
        + `${recoveryCycles.length} cycles`,
    );
    if (exitOnSuccess) {
      await stopProcesses();
      process.exit(0);
    }
    console.log('Browser remains open. Press Ctrl+C when ready to stop.');
    await new Promise(() => {});
  }
  if (scenario.mode === 'android-member-removal-lifecycle' && scenario.setup?.initiator === 'android') {
    await runAndroidMemberRemovalLifecycle(page, simulatorUdid, androidSerial);
    console.log(
      `✅ Test #${testNumber} Android membership removal lifecycle passed: `
        + `${recoveryCycles.length} direct Show cycles`,
    );
    if (exitOnSuccess) {
      await stopProcesses();
      process.exit(0);
    }
    console.log('Browser remains open. Press Ctrl+C when ready to stop.');
    await new Promise(() => {});
  }
  if (
    scenario.mode === 'android-secondary-creator-member-removal-lifecycle'
    && scenario.setup?.initiator === 'android'
  ) {
    await runAndroidSecondaryCreatorMemberRemovalLifecycle(page, simulatorUdid, androidSerial);
    console.log(
      `✅ Test #${testNumber} Android/iOS/Web member-removal lifecycle passed: `
        + `${recoveryCycles.length} direct Show cycles`,
    );
    if (exitOnSuccess) {
      await stopProcesses();
      process.exit(0);
    }
    console.log('Browser remains open. Press Ctrl+C when ready to stop.');
    await new Promise(() => {});
  }
  if (
    scenario.mode === 'android-distributed-creator-member-removal-lifecycle'
    && scenario.setup?.initiator === 'android'
  ) {
    await runAndroidDistributedCreatorMemberRemovalLifecycle(page, simulatorUdid, androidSerial);
    console.log(
      `✅ Test #${testNumber} distributed-creator member-removal lifecycle passed: `
        + `${recoveryCycles.length} direct Show cycles`,
    );
    if (exitOnSuccess) {
      await stopProcesses();
      process.exit(0);
    }
    console.log('Browser remains open. Press Ctrl+C when ready to stop.');
    await new Promise(() => {});
  }
  if (scenario.mode === 'android-multi-secret-lifecycle' && scenario.setup?.initiator === 'android') {
    await runAndroidMultiSecretLifecycle(page, simulatorUdid, androidSerial);
    console.log(`✅ Test #${testNumber} Android multi-secret lifecycle passed: ${recoveryCycles.length} cycles`);
    if (exitOnSuccess) {
      await stopProcesses();
      process.exit(0);
    }
    console.log('Browser remains open. Press Ctrl+C when ready to stop.');
    await new Promise(() => {});
  }
  if (
    ['ios-secondary-creator-lifecycle', 'ios-distributed-creator-lifecycle']
      .includes(scenario.mode)
    && scenario.setup?.initiator === 'ios'
  ) {
    await runIosSecondaryCreatorLifecycle(page, simulatorUdid, androidSerial);
    console.log(
      `✅ Test #${testNumber} iOS-initiated multi-creator lifecycle passed: ${recoveryCycles.length} cycles`,
    );
    if (exitOnSuccess) {
      await stopProcesses();
      process.exit(0);
    }
    console.log('Browser remains open. Press Ctrl+C when ready to stop.');
    await new Promise(() => {});
  }
  if (scenario.mode === 'ios-member-removal-lifecycle' && scenario.setup?.initiator === 'ios') {
    await runIosMemberRemovalLifecycle(page, simulatorUdid, androidSerial);
    console.log(
      `✅ Test #${testNumber} iOS membership removal lifecycle passed: `
        + `${recoveryCycles.length} direct Show cycles`,
    );
    if (exitOnSuccess) {
      await stopProcesses();
      process.exit(0);
    }
    console.log('Browser remains open. Press Ctrl+C when ready to stop.');
    await new Promise(() => {});
  }
  if (
    scenario.mode === 'ios-secondary-creator-member-removal-lifecycle'
    && scenario.setup?.initiator === 'ios'
  ) {
    await runIosSecondaryCreatorMemberRemovalLifecycle(page, simulatorUdid, androidSerial);
    console.log(
      `✅ Test #${testNumber} iOS/Web/Android member-removal lifecycle passed: `
        + `${recoveryCycles.length} direct Show cycles`,
    );
    if (exitOnSuccess) {
      await stopProcesses();
      process.exit(0);
    }
    console.log('Browser remains open. Press Ctrl+C when ready to stop.');
    await new Promise(() => {});
  }
  if (
    scenario.mode === 'ios-distributed-creator-member-removal-lifecycle'
    && scenario.setup?.initiator === 'ios'
  ) {
    await runIosDistributedCreatorMemberRemovalLifecycle(page, simulatorUdid, androidSerial);
    console.log(
      `✅ Test #${testNumber} distributed-creator member-removal lifecycle passed: `
        + `${recoveryCycles.length} direct Show cycles`,
    );
    if (exitOnSuccess) {
      await stopProcesses();
      process.exit(0);
    }
    console.log('Browser remains open. Press Ctrl+C when ready to stop.');
    await new Promise(() => {});
  }
  if (
    ['ios-multi-secret-lifecycle', 'ios-two-secret-owner-lifecycle'].includes(scenario.mode)
    && scenario.setup?.initiator === 'ios'
  ) {
    await runIosMultiSecretLifecycle(page, simulatorUdid, androidSerial);
    console.log(`✅ Test #${testNumber} iOS multi-secret lifecycle passed: ${recoveryCycles.length} cycles`);
    if (exitOnSuccess) {
      await stopProcesses();
      process.exit(0);
    }
    console.log('Browser remains open. Press Ctrl+C when ready to stop.');
    await new Promise(() => {});
  }
  if (scenario.mode === 'web-member-removal-lifecycle' && scenario.setup?.initiator === 'web') {
    await runWebMemberRemovalLifecycle(page, simulatorUdid, androidSerial);
    console.log(
      `✅ Test #${testNumber} Web membership removal lifecycle passed: `
        + `${recoveryCycles.length} direct Show cycles`,
    );
    if (exitOnSuccess) {
      await stopProcesses();
      process.exit(0);
    }
    console.log('Browser remains open. Press Ctrl+C when ready to stop.');
    await new Promise(() => {});
  }
  if (
    scenario.mode === 'web-secondary-creator-member-removal-lifecycle'
    && scenario.setup?.initiator === 'web'
  ) {
    await runWebSecondaryCreatorMemberRemovalLifecycle(page, simulatorUdid, androidSerial);
    console.log(
      `✅ Test #${testNumber} Web/Android/iOS member-removal lifecycle passed: `
        + `${recoveryCycles.length} direct Show cycles`,
    );
    if (exitOnSuccess) {
      await stopProcesses();
      process.exit(0);
    }
    console.log('Browser remains open. Press Ctrl+C when ready to stop.');
    await new Promise(() => {});
  }
  if (
    scenario.mode === 'web-distributed-creator-member-removal-lifecycle'
    && scenario.setup?.initiator === 'web'
  ) {
    await runWebDistributedCreatorMemberRemovalLifecycle(page, simulatorUdid, androidSerial);
    console.log(
      `✅ Test #${testNumber} distributed-creator member-removal lifecycle passed: `
        + `${recoveryCycles.length} direct Show cycles`,
    );
    if (exitOnSuccess) {
      await stopProcesses();
      process.exit(0);
    }
    console.log('Browser remains open. Press Ctrl+C when ready to stop.');
    await new Promise(() => {});
  }
  if (
    scenario.mode === 'web-distributed-creator-removal-rejoin-recovery-lifecycle'
    && scenario.setup?.initiator === 'web'
  ) {
    await runWebDistributedCreatorRemovalRejoinRecoveryLifecycle(
      page,
      simulatorUdid,
      androidSerial,
    );
    console.log(
      `✅ Test #${testNumber} removal/re-Join/recovery lifecycle passed: `
        + '2 removals, 2 re-Joins, 6 direct Shows, 27 recovery cycles',
    );
    if (exitOnSuccess) {
      await stopProcesses();
      process.exit(0);
    }
    console.log('Browser remains open. Press Ctrl+C when ready to stop.');
    await new Promise(() => {});
  }
  if (
    scenario.mode === 'web-distributed-creator-declined-removal-recovery-lifecycle'
    && scenario.setup?.initiator === 'web'
  ) {
    await runWebDistributedCreatorDeclinedRemovalRecoveryLifecycle(
      page,
      simulatorUdid,
      androidSerial,
    );
    console.log(
      `✅ Test #${testNumber} declined-removal/recovery lifecycle passed: `
        + '3 declined removals, 27 recovery cycles',
    );
    if (exitOnSuccess) {
      await stopProcesses();
      process.exit(0);
    }
    console.log('Browser remains open. Press Ctrl+C when ready to stop.');
    await new Promise(() => {});
  }
  if (
    scenario.mode === 'android-distributed-creator-declined-removal-recovery-lifecycle'
    && scenario.setup?.initiator === 'android'
  ) {
    await runAndroidDistributedCreatorDeclinedRemovalRecoveryLifecycle(
      page,
      simulatorUdid,
      androidSerial,
    );
    console.log(
      `✅ Test #${testNumber} rotated declined-removal/recovery lifecycle passed: `
        + '3 declined removals, 27 recovery cycles',
    );
    if (exitOnSuccess) {
      await stopProcesses();
      process.exit(0);
    }
    console.log('Browser remains open. Press Ctrl+C when ready to stop.');
    await new Promise(() => {});
  }
  if (
    scenario.mode === 'ios-distributed-creator-declined-removal-recovery-lifecycle'
    && scenario.setup?.initiator === 'ios'
  ) {
    await runIosDistributedCreatorDeclinedRemovalRecoveryLifecycle(
      page,
      simulatorUdid,
      androidSerial,
    );
    console.log(
      `✅ Test #${testNumber} final declined-removal/recovery rotation passed: `
        + '3 declined removals, 27 recovery cycles',
    );
    if (exitOnSuccess) {
      await stopProcesses();
      process.exit(0);
    }
    console.log('Browser remains open. Press Ctrl+C when ready to stop.');
    await new Promise(() => {});
  }
  if (scenario.mode === 'web-multi-secret-lifecycle' && scenario.setup?.initiator === 'web') {
    await runWebMultiSecretLifecycle(page, simulatorUdid, androidSerial);
    console.log(`✅ Test #${testNumber} Web multi-secret lifecycle passed: ${recoveryCycles.length} cycles`);
    if (exitOnSuccess) {
      await stopProcesses();
      process.exit(0);
    }
    console.log('Browser remains open. Press Ctrl+C when ready to stop.');
    await new Promise(() => {});
  }
  if (scenario.mode === 'web-secondary-creator-lifecycle' && scenario.setup?.initiator === 'web') {
    const lifecycleResult = await runWebSecondaryCreatorLifecycle(page, simulatorUdid, androidSerial);
    if (lifecycleResult?.diagnosticStop) {
      console.log(
        `✅ Test #${testNumber} diagnostic boundary reached after ${lifecycleResult.completedCycles} cycles`,
      );
    } else {
      console.log(
        `✅ Test #${testNumber} Web-initiated multi-creator lifecycle passed: `
          + `${recoveryCycles.length} cycles`,
      );
    }
    if (exitOnSuccess) {
      await stopProcesses();
      process.exit(0);
    }
    console.log('Browser remains open. Press Ctrl+C when ready to stop.');
    await new Promise(() => {});
  }
  if (scenario.mode === 'fourth-device-rejection') {
    if (process.env.E2E_META_CLI_PREBUILT !== '1') {
      console.log('6. Building meta-cli from current Core sources for Test #21');
      await runAndWait('cargo', ['build', '-p', 'meta-cli'], {
        cwd: resolve(projectRoot, 'meta-secret'),
      });
    } else {
      console.log('6. Using meta-cli built from current Core sources by the range preflight');
    }
    if (!existsSync(metaCliBinaryPath)) {
      throw new Error(`Test #21 CLI readiness failed: cargo build did not produce ${metaCliBinaryPath}`);
    }
    updateReadinessReport({
      platform: 'CLI',
      checks: { binary: metaCliBinaryPath, builtFromCurrentCargoSources: true },
      requiredNonPlatformParticipants: ['CLI'],
    });
    console.log('✅ Test #21 CLI binary built and verified before vault setup');
    await runFourthDeviceRejection(page, simulatorUdid, androidSerial);
    if (exitOnSuccess) {
      await stopProcesses();
      process.exit(0);
    }
    console.log('Browser remains open. Press Ctrl+C when ready to stop.');
    await new Promise(() => {});
  }
  if (scenario.mode === 'network-loss' && scenario.setup?.initiator === 'web') {
    await runWebInitiatedSetup(page, simulatorUdid, androidSerial);
    await runNetworkLossRecovery(page, simulatorUdid, androidSerial);
    console.log(`✅ Test #16 network-loss recovery passed: ${recoveryCycles.length} requests`);
    if (exitOnSuccess) {
      await stopProcesses();
      process.exit(0);
    }
    console.log('Browser remains open. Press Ctrl+C when ready to stop.');
    await new Promise(() => {});
  }
  if (scenario.mode === 'server-restart-recovery' && scenario.setup?.initiator === 'web') {
    await runWebInitiatedSetup(page, simulatorUdid, androidSerial);
    await runServerRestartRecovery(page, simulatorUdid, androidSerial);
    console.log(`✅ Test #17 server-restart recovery passed: ${recoveryCycles.length} requests`);
    if (exitOnSuccess) {
      await stopProcesses();
      process.exit(0);
    }
    console.log('Browser remains open. Press Ctrl+C when ready to stop.');
    await new Promise(() => {});
  }
  if (scenario.mode === 'multiple-web-tabs' && scenario.setup?.initiator === 'web') {
    await runWebInitiatedSetup(page, simulatorUdid, androidSerial);
    await runMultipleWebTabsRecovery(page, simulatorUdid, androidSerial);
    console.log(`✅ Test #19 multiple-Web-tabs recovery passed: ${recoveryCycles.length} requests`);
    if (exitOnSuccess) {
      await stopProcesses();
      process.exit(0);
    }
    console.log('Browser remains open. Press Ctrl+C when ready to stop.');
    await new Promise(() => {});
  }
  if (scenario.mode === 'new-device-during-recovery') {
    if (scenario.setup?.initiator === 'web') {
      await runWebInitiatedSetup(page, simulatorUdid, androidSerial);
    } else {
      await runSenderOfflineSetup(page, simulatorUdid, androidSerial);
    }
    await runNewDeviceDuringRecoveryWithCliFull(page, simulatorUdid, androidSerial);
    console.log(`✅ Test #15 new-device-during-recovery passed: ${recoveryCycles.length} cycles`);
    if (exitOnSuccess) {
      await stopProcesses();
      process.exit(0);
    }
    console.log('Browser remains open. Press Ctrl+C when ready to stop.');
    await new Promise(() => {});
  }
  if (scenario.mode === 'receiver-offline-after-alert' && scenario.setup?.initiator === 'web') {
    await runWebInitiatedSetup(page, simulatorUdid, androidSerial);
    await runReceiverOfflineAfterAlertRecovery(page, simulatorUdid, androidSerial);
    console.log(`✅ Test #10 receiver-offline-after-alert recovery passed: ${recoveryCycles.length} requests`);
    if (exitOnSuccess) {
      await stopProcesses();
      process.exit(0);
    }
    console.log('Browser remains open. Press Ctrl+C when ready to stop.');
    await new Promise(() => {});
  }
  if (scenario.mode === 'both-receivers-offline' && scenario.setup?.initiator === 'web') {
    await runWebInitiatedSetup(page, simulatorUdid, androidSerial);
    await runBothReceiversOfflineRecovery(page, simulatorUdid, androidSerial);
    const expectedCycles = scenario.recovery.expectedCycles ?? 18;
    console.log(`✅ Test #8 both-receivers-offline recovery passed: ${expectedCycles} requests`);
    if (exitOnSuccess) {
      await stopProcesses();
      process.exit(0);
    }
    console.log('Browser remains open. Press Ctrl+C when ready to stop.');
    await new Promise(() => {});
  }
  if (scenario.mode === 'sender-offline') {
    await runSenderOfflineSetup(page, simulatorUdid, androidSerial);
    await runSenderOfflineRecovery(page, simulatorUdid, androidSerial);
    console.log(`✅ Test #9 sender-offline recovery passed: ${recoveryCycles.length} requests`);
    if (exitOnSuccess) {
      await stopProcesses();
      process.exit(0);
    }
    console.log('Browser remains open. Press Ctrl+C when ready to stop.');
    await new Promise(() => {});
  }
  if (scenario.mode === 'approve-decline-race' && scenario.setup?.initiator === 'web') {
    await runWebInitiatedSetup(page, simulatorUdid, androidSerial);
    await runApproveDeclineRaceRecovery(page, simulatorUdid, androidSerial);
    console.log(`✅ Test #${testNumber} approve/decline recovery passed: ${recoveryCycles.length} requests`);
    if (exitOnSuccess) {
      await stopProcesses();
      process.exit(0);
    }
    console.log('Browser remains open. Press Ctrl+C when ready to stop.');
    await new Promise(() => {});
  }
  if (scenario.mode === 'repeat-approve' && scenario.setup?.initiator === 'web') {
    await runWebInitiatedSetup(page, simulatorUdid, androidSerial);
    await runRepeatApproveRecovery(page, simulatorUdid, androidSerial);
    console.log(`✅ Test #${testNumber} repeat-approve recovery passed: ${recoveryCycles.length} requests`);
    if (exitOnSuccess) {
      await stopProcesses();
      process.exit(0);
    }
    console.log('Browser remains open. Press Ctrl+C when ready to stop.');
    await new Promise(() => {});
  }
  if (scenario.mode === 'duplicate-recovery' && scenario.setup?.initiator === 'web') {
    await runWebInitiatedSetup(page, simulatorUdid, androidSerial);
    await runDuplicateRecovery(page, simulatorUdid, androidSerial);
    console.log(`✅ Test #${testNumber} duplicate-recovery passed: ${recoveryCycles.length} requests`);
    if (exitOnSuccess) {
      await stopProcesses();
      process.exit(0);
    }
    console.log('Browser remains open. Press Ctrl+C when ready to stop.');
    await new Promise(() => {});
  }
  const androidTest = await startAndroidJoinTest(androidSerial);
  await androidTest.waitForMarker('E2E: ANDROID_INITIATOR_READY');

  console.log('Web joining Android-created vault');
  // The Web app keeps a live connection for vault state, so `networkidle`
  // is not a valid readiness signal and can time out after the UI is usable.
  // Subsequent semantic waits and E2E markers provide the actual readiness
  // boundaries for this scenario.
  await page.goto(scenario.web.url, { waitUntil: 'domcontentloaded' });
  await unlockWithPasskeyIfNeeded(page);
  await page.getByPlaceholder('vault name').fill(scenario.vault.name);
  await page.getByRole('button', { name: 'Set Vault Name' }).click();
  await page.getByRole('button', { name: 'Join', exact: true }).click();
  // The Android UI helper allows 180s for the pending join row to appear.
  // Keep the outer marker deadline longer so teardown does not kill it first.
  await androidTest.waitForMarker('E2E: ANDROID_WEB_JOIN_APPROVED', 210_000);
  await page.getByRole('button', { name: '+ Add Secret' }).waitFor({ timeout: 120_000 });

  await createWebSecretsAfterJoin(page);

  const iosTest = startIosJoinTest(simulatorUdid);
  // Same as Android above: this test runs in parallel with the orchestrator.
  // Keep its eventual rejection observable through `iosTest.result`, while
  // preventing Node from reporting it as an unhandled rejection during
  // cleanup.
  void iosTest.result.catch(() => {});
  await iosTest.waitForMarker('E2E: IOS_JOIN_REQUEST_SENT');
  approvalCoordinator.allow('android-ios-join', 0);
  await androidTest.waitForMarker('E2E: ANDROID_IOS_JOIN_APPROVED', 120_000);
  // Let iOS finish its own join timeout (and emit a useful failure) before
  // stopping the parallel Android instrumentation.
  await iosTest.waitForMarker('E2E: IOS_SECRETS_READY', 210_000);
  await waitForWebSecrets(page);

  if (scenario.offlineReceiver?.platform === 'android') {
    await runOfflineReceiverRecovery(page, iosTest, androidTest, androidSerial, simulatorUdid);
  } else {
    await runConcurrentRecoveryCycles(page, iosTest, androidTest);
  }
  await Promise.all([iosTest.result, androidTest.result]);

  console.log(`✅ Test #${testNumber} concurrent Web + iOS + Android recovery passed`);
  if (exitOnSuccess) {
    await stopProcesses();
    process.exit(0);
  }
  console.log('Browser remains open. Press Ctrl+C when ready to stop.');
  await new Promise(() => {});
}

async function finishAfterSignal(signal, exitCode) {
  requestedSignalExitCode ??= exitCode;
  const error = new Error(`Scenario cancelled by ${signal}`);
  writeDiagnostic(`CANCELLED: ${error.message}`);
  try {
    await harnessShutdown.finish({ error, diagnostics: true });
    process.exitCode = requestedSignalExitCode;
  } catch (cleanupError) {
    const safeCleanupError = formatHarnessShutdownError(cleanupError, redactSensitiveText);
    console.error(`Environment cleanup failed: ${safeCleanupError}`);
    writeDiagnostic(`CLEANUP BLOCKER: ${safeCleanupError}`);
    process.exitCode = 1;
  }
}

process.once('SIGINT', () => { void finishAfterSignal('SIGINT', 130); });
process.once('SIGTERM', () => { void finishAfterSignal('SIGTERM', 143); });

main().catch(async (error) => {
  const safeError = redactSensitiveText(error.stack ?? error.message);
  console.error(`\n❌ Test #${testNumber} failed: ${redactSensitiveText(error.message)}`);
  writeDiagnostic(`FAILED: ${safeError}`);
  try {
    await harnessShutdown.finish({ error, diagnostics: true });
    process.exitCode = requestedSignalExitCode ?? 1;
  } catch (cleanupError) {
    const safeCleanupError = formatHarnessShutdownError(cleanupError, redactSensitiveText);
    console.error(`Environment cleanup failed: ${safeCleanupError}`);
    writeDiagnostic(`CLEANUP BLOCKER: ${safeCleanupError}`);
    process.exitCode = 1;
  }
});
