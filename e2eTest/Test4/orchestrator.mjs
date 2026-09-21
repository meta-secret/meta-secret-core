import { appendFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { spawn } from 'node:child_process';
import { createServer, request as httpRequest } from 'node:http';
import process from 'node:process';
import { setTimeout as wait } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const root = dirname(fileURLToPath(import.meta.url));
const e2eRoot = resolve(root, '..');
const artifactsDirectory = resolve(e2eRoot, '.artifacts');
const scenarioArgument = process.argv.slice(2).find((argument) => !argument.startsWith('--'));
const exitOnSuccess = process.argv.includes('--exit-on-success') || process.env.E2E_EXIT_ON_SUCCESS === '1';
const scenarioPath = resolve(root, scenarioArgument ?? 'scenario.json');
const scenario = JSON.parse(readFileSync(scenarioPath, 'utf8'));
const testNumber = scenario.testNumber ?? 4;
const secretConfigs = scenario.secrets ?? { default: scenario.secret };
const defaultSecretKey = Object.keys(secretConfigs)[0];
const defaultSecret = secretConfigs[defaultSecretKey];
const diagnosticFileName = scenario.runId
  ? `test-${testNumber}-${scenario.runId}.log`
  : `test-${testNumber}.log`;
const diagnosticLogPath = resolve(artifactsDirectory, diagnosticFileName);
const iosStepConfigPath = '/tmp/metasecret-e2e-ios-step.json';

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
      const approvals = group.approvals
        ? group.approvals.map((approval) => normalizeApproval(approval, defaultApprovalSecret))
        : [
            normalizeApproval(resolveValue(group.firstApprover), defaultApprovalSecret),
            normalizeApproval(resolveValue(group.secondApprover), defaultApprovalSecret),
          ];
      return {
        block: group.block,
        offlineReceiver: group.offlineReceiver ?? scenario.offlineReceiver?.platform,
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
    number: index + 1,
    senders: cycle.senders ?? Object.keys(senderSecrets),
    senderSecrets,
    approvals,
    expectedOutcome: cycle.expectedOutcome,
    firstApprover: approvals[0]?.platform,
    secondApprover: approvals[1]?.platform,
  };
});
const recoveryShowTimeoutMs = scenario.recovery.showTimeoutMs ?? 45_000;
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
const androidEmulatorPath = '/Users/dmitrykuklin/Library/Android/sdk/emulator/emulator';
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
let currentCycleForDiagnostics = null;
let browserInstance;
let primaryVirtualAuthenticator;
let simulatorUdidForCleanup;
let androidSerialForCleanup;
let cleanupStarted = false;
const webSenderRevealCompletedCycles = new Set();
const networkGateProcesses = new Map();

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
  const boundedLine = line.length > maxBrowserDiagnosticLineChars
    ? `${line.slice(0, maxBrowserDiagnosticLineChars)}… [truncated]`
    : line;
  browserDiagnostics.push(boundedLine);
  if (browserDiagnostics.length > maxBrowserDiagnosticEntries) {
    browserDiagnostics.splice(0, browserDiagnostics.length - maxBrowserDiagnosticEntries);
  }
  appendFileSync(diagnosticLogPath, `[browser] ${boundedLine}\n`);
}

function writeDiagnostic(message) {
  const line = `[${new Date().toISOString()}] ${message}\n`;
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
  const child = spawn(command, args, {
    cwd: options.cwd ?? projectRoot,
    env: {
      ...process.env,
      PATH: `/opt/homebrew/bin:/usr/local/bin:${process.env.PATH ?? ''}`,
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

async function runMetaCli(cliDirectory, args, { label = args.join(' ') } = {}) {
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
  if (result.stderr.trim()) {
    writeDiagnostic(`[CLI][stderr] ${result.stderr.trim().slice(-4_000)}`);
  }
  if (result.stdout.trim()) {
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
    return JSON.parse(result.stdout);
  } catch (error) {
    throw new Error(
      `CLI ${label} returned invalid JSON: ${error.message}; `
      + `stdout=${result.stdout.slice(-4_000)}`,
    );
  }
}

async function readMetaCliInfo(cliDevice, command = ['info', 'default']) {
  const result = await runMetaCli(cliDevice.cliDirectory, command, {
    label: command.join(' '),
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
  let stdoutTail = '';
  let stderrTail = '';
  let markerScanTail = '';
  const markerWaiters = new Set();

  const child = run(command, args, { ...options, stdio: ['ignore', 'pipe', 'pipe'] });
  const processOutput = (chunk, sink) => {
    const text = chunk.toString();
    if (sink === 'stdout') stdoutTail = appendBounded(stdoutTail, text);
    else stderrTail = appendBounded(stderrTail, text);
    markerScanTail = appendBounded(markerScanTail, text);
    const diagnosticText = filterVerboseWatchedOutput(command, sink, text);
    if (diagnosticText) {
      process[sink].write(diagnosticText);
      appendFileSync(diagnosticLogPath, `[${command} ${sink}] ${diagnosticText}`);
    }
    for (const waiter of markerWaiters) {
      if (markerScanTail.includes(waiter.marker)) {
        clearTimeout(waiter.timeout);
        markerWaiters.delete(waiter);
        waiter.resolve();
      }
    }
  };

  child.stdout.on('data', (chunk) => processOutput(chunk, 'stdout'));
  child.stderr.on('data', (chunk) => processOutput(chunk, 'stderr'));

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

  const waitForMarker = (marker, timeoutMs = 300_000) => new Promise((resolvePromise, reject) => {
    if (markerScanTail.includes(marker)) return resolvePromise();
    if (processError) return reject(processError);
    const waiter = {
      marker,
      resolve: resolvePromise,
      reject: reject,
      timeout: setTimeout(() => {
        markerWaiters.delete(waiter);
        reject(new Error(`Timed out waiting for ${marker}`));
      }, timeoutMs),
    };
    markerWaiters.add(waiter);
  });

  const watched = {
    command,
    args,
    child,
    waitForMarker,
    result,
    outputTail: () => `${stdoutTail}\n${stderrTail}`.slice(-12_000),
  };
  watchedProcessOutputs.push(watched);
  return watched;
}

function printFailureDiagnostics() {
  for (const watched of watchedProcessOutputs) {
    const tail = watched.outputTail();
    if (tail.trim()) {
      console.error(`\n=== E2E diagnostic: ${watched.command} ${watched.args.join(' ')} ===\n${tail}`);
      appendFileSync(diagnosticLogPath, `\n=== E2E diagnostic: ${watched.command} ${watched.args.join(' ')} ===\n${tail}\n`);
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

async function stopProcesses() {
  if (cleanupStarted) return;
  cleanupStarted = true;
  for (const gate of networkGateProcesses.values()) await gate.close().catch(() => {});
  networkGateProcesses.clear();
  await approvalCoordinator?.close().catch(() => {});
  approvalCoordinator = undefined;
  await browserInstance?.close().catch(() => {});
  browserInstance = undefined;
  for (const child of processes.reverse()) {
    if (!child.killed) child.kill('SIGTERM');
  }
  await wait(500);
  if (exitOnSuccess && androidSerialForCleanup) {
    await runAndWait(
      'adb',
      ['-s', androidSerialForCleanup, 'emu', 'kill'],
      { stdio: 'ignore' },
    ).catch(() => {});
  }
  if (exitOnSuccess && simulatorUdidForCleanup) {
    await runAndWait(
      'xcrun',
      ['simctl', 'shutdown', simulatorUdidForCleanup],
      { stdio: 'ignore' },
    ).catch(() => {});
  }
  if (exitOnSuccess) {
    await runAndWait('./gradlew', ['--stop'], {
      cwd: composeRoot,
      stdio: 'ignore',
    }).catch(() => {});
  }
  await runAndWait('docker', ['rm', '-f', serverContainer], { stdio: 'ignore' }).catch(() => {});
}

async function captureIosScreenshot(simulatorUdid, filePath) {
  await runAndWait('xcrun', ['simctl', 'io', simulatorUdid, 'screenshot', filePath]);
  writeDiagnostic(`[SCREENSHOT] iOS saved ${filePath}`);
}

function captureAndroidScreenshot(serial, filePath) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn('adb', ['-s', serial, 'exec-out', 'screencap', '-p'], {
      cwd: projectRoot,
      env: { ...process.env, PATH: `/opt/homebrew/bin:/usr/local/bin:${process.env.PATH ?? ''}` },
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

async function findIosSimulator() {
  const { stdout } = await runAndCapture('xcrun', ['simctl', 'list', 'devices', 'available', '-j']);
  const devicesByRuntime = JSON.parse(stdout).devices;
  const devices = Object.values(devicesByRuntime).flat();
  const device = devices.find((candidate) => candidate.name === scenario.ios.simulatorName);
  if (!device) throw new Error(`iOS simulator not found: ${scenario.ios.simulatorName}`);
  return device;
}

async function prepareIosSimulator() {
  const simulator = await findIosSimulator();
  simulatorUdidForCleanup = simulator.udid;
  console.log(`8. Preparing iOS simulator: ${simulator.name} (${simulator.udid})`);
  if (simulator.state !== 'Booted') {
    await runAndWait('xcrun', ['simctl', 'boot', simulator.udid]);
  }
  await runAndWait('xcrun', ['simctl', 'bootstatus', simulator.udid, '-b']);
  await runAndWait('xcrun', ['simctl', 'uninstall', simulator.udid, scenario.ios.bundleId], { stdio: 'ignore' }).catch(() => {});
  await runAndWait('open', ['-a', 'Simulator']).catch(() => {});
  return simulator.udid;
}

function startIosJoinTest(simulatorUdid, {
  testClass = scenario.ios.testClass ?? 'CaseFourIosConcurrentRecoveryUITest',
  testMethod = iosTestMethod,
  label = 'iOS UI test',
  environment = {},
  secretName = '',
} = {}) {
  console.log(`9. Starting ${label}`);
  // xcodebuild does not reliably propagate per-invocation E2E_* variables to
  // the XCTest process. Persist the compact step context on the host so the
  // iOS UI test can read the exact cycle/step without timing-based defaults.
  if (environment.E2E_ROLE || environment.E2E_NETWORK_ROLE) {
    writeFileSync(iosStepConfigPath, JSON.stringify({
      role: environment.E2E_ROLE ?? environment.E2E_NETWORK_ROLE,
      cycle: environment.E2E_CYCLE,
      step: environment.E2E_STEP,
      approvalPlatform: environment.E2E_APPROVAL_PLATFORM,
      sender: environment.E2E_SENDER,
      action: environment.E2E_ACTION,
      expectedOutcome: environment.E2E_EXPECTED_OUTCOME,
      secretName: environment.E2E_SECRET_NAME ?? secretName,
      repeatApprove: environment.E2E_REPEAT_APPROVE === '1',
      duplicateRecovery: environment.E2E_DUPLICATE_RECOVERY === '1',
      networkRole: environment.E2E_NETWORK_ROLE,
      networkCycles: environment.E2E_NETWORK_CYCLES,
    }));
  }
  const xcodebuildArgs = [
    'test',
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
        E2E_VAULT_NAME: scenario.vault.name,
        E2E_SECRET_NAME: secretName || defaultSecret?.name || '',
        E2E_SECRET_VALUE: defaultSecret?.value ?? '',
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
  const selectedSuitePassed = watched.waitForMarker("Test Suite 'Selected tests' passed", 300_000)
    .then(() => {
      if (!watched.child.killed) watched.child.kill('SIGTERM');
      return { stdout: watched.outputTail(), stderr: '' };
    });
  const result = Promise.race([watched.result, selectedSuitePassed]);
  void result.catch(() => {});
  return { ...watched, result };
}

async function prepareAndroidEmulator() {
  const { stdout } = await runAndCapture('adb', ['devices']);
  let serial = stdout.split('\n').map((line) => line.trim().split(/\s+/)).find(([id, state]) => id.startsWith('emulator-') && state === 'device')?.[0];

  if (!serial) {
    console.log(`12. Starting Android emulator: ${scenario.android.avdName}`);
    // A stale named snapshot can report boot completed while leaving the
    // emulated shared-storage volume unmounted. Start from the current AVD
    // userdata without loading that snapshot; readiness is checked below.
    run(androidEmulatorPath, ['-avd', scenario.android.avdName, '-no-snapshot-load']);
    // `adb wait-for-device` also returns for an offline emulator. Wait for
    // the transport to become usable before starting Gradle.
    serial = await waitForOnlineAndroidDevice();
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
  await runAndWait('adb', ['-s', serial, 'uninstall', scenario.android.bundleId], { stdio: 'ignore' }).catch(() => {});
  await runAndWait('adb', ['-s', serial, 'uninstall', androidTestBundleId], { stdio: 'ignore' }).catch(() => {});
  return serial;
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

async function waitForAndroidBoot(serial, timeoutMs = 120_000) {
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

async function startAndroidJoinTest(
  serial,
  { testClass = scenario.android.testClass, testMethod = scenario.android.testMethod ?? scenario.android.joinTestMethod } = {},
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
  const appApkPath = resolve(composeRoot, 'composeApp/build/outputs/apk/debug/composeApp-debug.apk');
  const testApkPath = resolve(
    composeRoot,
    'composeApp/build/outputs/apk/androidTest/debug/composeApp-debug-androidTest.apk',
  );
  const logcat = watchProcessOutput('adb', [
    '-s', serial, 'logcat',
    'MetaSecretE2E:I',
    'AndroidRuntime:E',
    'ActivityManager:E',
    'libc:F',
    '*:S',
  ]);
  void logcat.result.catch(() => {}); // logcat is stopped deliberately when the instrumentation test ends
  const androidTestClass = testClass
    ?? 'metasecret.project.com.CaseFourAndroidConcurrentRecoveryTest';
  const androidTestMethod = testMethod;
  const androidTestSelector = androidTestMethod
    ? `${androidTestClass}#${androidTestMethod}`
    : androidTestClass;
  const runnerArguments = [
    '-e', 'class', androidTestSelector,
    '-e', 'vaultName', scenario.vault.name,
    '-e', 'secretName', defaultSecret?.name ?? '',
    '-e', 'secretConfig', secretConfigJson,
    '-e', 'recoveryCycles', String(recoveryCycles.length),
    '-e', 'cyclePlan', cyclePlanJson,
    '-e', 'secretCreationPlan', JSON.stringify(secretCreationPlan),
    '-e', 'approvalCoordinatorUrl', 'http://10.0.2.2:5180',
    '-e', 'setupOnly', scenario.android.setupOnly ? 'true' : 'false',
  ];
  // Android's `am` parser treats JSON punctuation as shell syntax when a
  // long value is passed through `adb shell`. The Test #7 setup only needs
  // these scalar values; keeping the JSON plan for the Gradle path avoids a
  // fragile command-line encoding and does not reduce scenario coverage.
  const directRunnerArguments = [
    '-e', 'class', androidTestSelector,
    '-e', 'vaultName', scenario.vault.name,
    '-e', 'secretName', defaultSecret?.name ?? '',
    '-e', 'secretNames', Object.values(secretConfigs).map((config) => config.name).join(','),
    '-e', 'secretValues', Object.values(secretConfigs).map((config) => config.value).join(','),
    '-e', 'approvalCoordinatorUrl', 'http://10.0.2.2:5180',
    '-e', 'setupOnly', scenario.android.setupOnly ? 'true' : 'false',
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
    await runAndWait('./gradlew', [
      ':composeApp:assembleDebug',
      ':composeApp:assembleDebugAndroidTest',
      ...androidNetworkGradleArgs,
    ], { cwd: composeRoot, env: { ANDROID_SERIAL: serial } });
    await runAdbWithRetry(['-s', serial, 'install', '-r', appApkPath]);
    await runAdbWithRetry(['-s', serial, 'install', '-r', testApkPath]);
    await dismissAndroidCompatibilityDialog(serial);
    test = watchProcessOutput('adb', [
      '-s', serial,
      'shell',
      '--',
      'am',
      'instrument',
      '-w',
      ...directRunnerArguments,
      `${androidTestBundleId}/androidx.test.runner.AndroidJUnitRunner`,
    ]);
    directInstrumentation = true;
  } else {
    // The orchestrator uninstalls the app before each run, while Gradle may
    // consider installDebug up-to-date and skip reinstalling it. Install the
    // freshly assembled target APK explicitly so ActivityScenario can resolve
    // MainActivity even when the Gradle install task is cached. Build both
    // artifacts here as well: a clean workspace may not have APKs yet.
    await runAndWait('./gradlew', [
      ':composeApp:assembleDebug',
      ':composeApp:assembleDebugAndroidTest',
      ...androidNetworkGradleArgs,
    ], { cwd: composeRoot, env: { ANDROID_SERIAL: serial } });
    await runAdbWithRetry(['-s', serial, 'install', '-r', appApkPath]);
    // Android 16 may show the 16 KB compatibility warning only when the
    // freshly installed target is first launched by instrumentation. Dismiss
    // it before the UI test reaches a screenshot marker so the captured frame
    // contains the application error state rather than the system dialog.
    await dismissAndroidCompatibilityDialog(serial);
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
        '-Pandroid.testInstrumentationRunnerArguments.approvalCoordinatorUrl=http://10.0.2.2:5180',
        `-Pandroid.testInstrumentationRunnerArguments.setupOnly=${scenario.android.setupOnly ? 'true' : 'false'}`,
        ...androidNetworkGradleArgs,
      ],
      { cwd: composeRoot, env: { ANDROID_SERIAL: serial } },
    );
  }

  const result = (directInstrumentation
    ? test.result.then((output) => {
        assertAndroidInstrumentationPassed(output, androidTestSelector);
        return output;
      })
    : test
  ).finally(() => logcat.child.kill('SIGTERM'));
  // The orchestrator waits for E2E markers before awaiting the full Android
  // result. Attach a rejection handler now so a parallel failure cannot turn
  // into an unhandled rejection and hide the original orchestration error.
  void result.catch(() => {});
  return { ...logcat, result };
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
  secretName = defaultSecret?.name ?? '',
  testMethod = scenario.android.stepTestMethod ?? 'handleRecoveryStep',
  label = 'Android recovery step',
} = {}) {
  console.log(`Starting ${label}: role=${role} cycle=${cycle} step=${step}`);
  const appApkPath = resolve(composeRoot, 'composeApp/build/outputs/apk/debug/composeApp-debug.apk');
  const testApkPath = resolve(composeRoot, 'composeApp/build/outputs/apk/androidTest/debug/composeApp-debug-androidTest.apk');
  await runAdbWithRetry(['-s', serial, 'install', '-r', appApkPath]);
  await runAdbWithRetry(['-s', serial, 'install', '-r', testApkPath]);
  await dismissAndroidCompatibilityDialog(serial);
  const logcat = watchProcessOutput('adb', [
    '-s', serial, 'logcat',
    'MetaSecretE2E:I',
    'AndroidRuntime:E',
    'ActivityManager:E',
    'libc:F',
    '*:S',
  ]);
  void logcat.result.catch(() => {});
  const androidTestClass = scenario.android.testClass
    ?? 'metasecret.project.com.CaseEightAndroidBothOfflineTest';
  const androidTestMethod = testMethod;
  const selector = `${androidTestClass}#${androidTestMethod}`;
  const runnerArguments = [
    '-s', serial,
    'shell', '--', 'am', 'instrument', '-w',
    '-e', 'class', selector,
    '-e', 'vaultName', scenario.vault.name,
    '-e', 'secretName', secretName,
    '-e', 'role', role,
    '-e', 'cycle', String(cycle),
    '-e', 'step', String(step),
    '-e', 'repeatApprove', repeatApprove ? 'true' : 'false',
    '-e', 'duplicateRecovery', duplicateRecovery ? 'true' : 'false',
    '-e', 'approvalCoordinatorUrl', 'http://10.0.2.2:5180',
    `${androidTestBundleId}/androidx.test.runner.AndroidJUnitRunner`,
  ];
  const optionalRunnerArguments = [
    ['approvalPlatform', approvalPlatform],
    ['sender', sender],
    ['expectedOutcome', expectedOutcome],
    ['networkRole', networkRole],
    ['networkBlock', networkBlock],
    ['networkCycles', networkCycles],
  ];
  const runnerInsertAt = runnerArguments.length - 1;
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
  const test = watchProcessOutput('adb', runnerArguments);
  const result = test.result
    .then((output) => {
      assertAndroidInstrumentationPassed(output, selector);
      return output;
    })
    .finally(() => logcat.child.kill('SIGTERM'));
  void result.catch(() => {});
  // Android E2E markers are emitted with Log.i and therefore arrive on the
  // dedicated logcat watcher, not on `am instrument` stdout. Keep the
  // instrumentation result for pass/fail validation, but expose the logcat
  // marker waiter to the orchestrator.
  return { ...test, waitForMarker: logcat.waitForMarker, result };
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
  const test = watchProcessOutput('adb', [
    '-s', serial,
    'shell', '--', 'am', 'instrument', '-w',
    '-e', 'class', selector,
    '-e', 'vaultName', scenario.vault.name,
    '-e', 'secretName', defaultSecret?.name ?? '',
    '-e', 'approvalCoordinatorUrl', 'http://10.0.2.2:5180',
    `${androidTestBundleId}/androidx.test.runner.AndroidJUnitRunner`,
  ]);
  const result = test.result.then((output) => {
    assertAndroidInstrumentationPassed(output, selector);
    return output;
  }).finally(() => logcat.child.kill('SIGTERM'));
  void result.catch(() => {});
  return { ...logcat, result };
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
  currentCycleForDiagnostics = androidCycle.number;
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
  currentCycleForDiagnostics = webCycle.number;
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
  currentCycleForDiagnostics = iosCycle.number;
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
  await page.goto(scenario.web.url, { waitUntil: 'domcontentloaded' });
  await unlockWithPasskeyIfNeeded(page);
  await page.getByPlaceholder('vault name').fill(scenario.vault.name);
  await page.getByRole('button', { name: 'Set Vault Name' }).click();
  await page.getByText('Vault name is free!').waitFor({ timeout: 120_000 });
  await page.getByRole('button', { name: 'Create', exact: true }).click();
  await page.getByRole('button', { name: '+ Add Secret', exact: true }).waitFor({ timeout: 180_000 });
  for (const secretName of secretCreationPlan?.initial?.web ?? [defaultSecret?.name]) {
    await createWebSecret(page, secretName);
  }
  await waitForWebSecrets(page);
  console.log('✅ Web vault and initial secret ready');
}

async function runWebInitiatedSetup(page, simulatorUdid, androidSerial) {
  await createWebInitiatedVault(page);

  const iosTest = startIosJoinTest(simulatorUdid, {
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

  const androidTest = await startAndroidJoinTest(androidSerial);
  await androidTest.waitForMarker('E2E: ANDROID_JOIN_REQUEST_SENT', 180_000);
  await approveJoinRequestOnWeb(page, 'Android');
  await androidTest.waitForMarker('E2E: ANDROID_JOIN_READY', 180_000);
  await androidTest.result;
  await waitForWebSecrets(page);
  return { iosTest, androidTest };
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
  const blockName = scenario.runId ?? `block-${rejected}`;
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
    currentCycleForDiagnostics = cycle.number;
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
    currentCycleForDiagnostics = cycle.number;
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
    currentCycleForDiagnostics = cycle.number;
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
    currentCycleForDiagnostics = cycle.number;
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
    currentCycleForDiagnostics = cycle.number;
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

async function runNewDeviceDuringRecoveryWithCliFull(page, simulatorUdid, androidSerial) {
  const expectedCycles = scenario.recovery.expectedCycles ?? recoveryCycles.length;
  if (recoveryCycles.length !== expectedCycles) {
    throw new Error(`Test #${testNumber} expected ${expectedCycles} recovery requests, got ${recoveryCycles.length}`);
  }

  const senderForCycle = (cycle) => {
    if (cycle.senders.length !== 1) {
      throw new Error(`Test #15 cycle ${cycle.number} must have exactly one sender`);
    }
    return cycle.senders[0];
  };

  for (const cycle of recoveryCycles) {
    currentCycleForDiagnostics = cycle.number;
    const sender = senderForCycle(cycle);
    const secretName = cycle.senderSecrets[sender] ?? defaultSecret?.name;
    const oldReceivers = scenario.newDevice?.oldReceivers
      ?? ['web', 'ios', 'android'].filter((platform) => platform !== sender);
    if (!['web', 'ios', 'android'].includes(sender)) {
      throw new Error(`Test #15 unsupported sender platform: ${sender}`);
    }
    if (oldReceivers.length !== 2 || oldReceivers.includes(sender)) {
      throw new Error(
        `Test #15 cycle ${cycle.number} must observe the two receivers other than sender=${sender}; `
        + `got=${JSON.stringify(oldReceivers)}`,
      );
    }

    writeDiagnostic(
      `TEST15 cycle=${cycle.number} sender=${sender} secret=${secretName} `
      + `oldReceivers=${oldReceivers.join(',')} newDevice=cli`,
    );
    console.log(
      `Test #15 cycle ${cycle.number}/${expectedCycles}: sender=${sender}; `
      + `CLI joins during recovery; old receivers=${oldReceivers.join(',')}`,
    );

    await reopenWebAfterOffline(page);
    const nativeProcesses = new Map();
    const startNative = async (platform, role) => {
      const environment = stepEnvironment({
        role,
        cycle: cycle.number,
        step: 1,
        approvalPlatform: 'cli',
        sender,
        expectedOutcome: 'invalidated',
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
          expectedOutcome: 'invalidated',
          testMethod: scenario.android.stepTestMethod ?? 'handleRecoveryStep',
          label: `Android Test #15 ${role} cycle ${cycle.number}`,
        });
        nativeProcesses.set(platform, { role, test });
        await test.waitForMarker(
          `E2E: ANDROID_STEP_CONFIG role=${role} cycle=${cycle.number} step=1 approval=cli`,
          180_000,
        );
        return;
      }
      throw new Error(`Test #15 cannot start Web as a native process: ${platform}`);
    };

    for (const platform of oldReceivers) {
      if (platform !== 'web') await startNative(platform, 'receiver');
    }
    if (sender !== 'web') await startNative(sender, 'sender');

    if (sender === 'web') {
      await startWebRecovery(page, secretName);
    } else {
      approvalCoordinator.allow(`${sender}-sender-1`, cycle.number);
      const senderProcess = nativeProcesses.get(sender)?.test;
      await senderProcess.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_REQUEST_SENT_${cycle.number}_1`,
        180_000,
      );
    }

    for (const platform of oldReceivers) {
      if (platform === 'web') {
        await waitForWebRecoveryBadgeCount(page, 1, secretName);
      } else {
        await nativeProcesses.get(platform).test.waitForMarker(
          `E2E: ${platform.toUpperCase()}_INCOMING_VISIBLE_${cycle.number}_1`,
          180_000,
        );
      }
    }
    writeDiagnostic(`TEST15 cycle=${cycle.number} old recovery visible on all receivers`);

    const cliDevice = await createMetaCliDevice(cycle);
    // Web is always an existing member in all three blocks, so it is the
    // deterministic join approver even when it is also observing the old
    // recovery request as a receiver.
    if (sender === 'web') await dismissWebRecoveryWaitingUi(page, secretName);
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

    for (const platform of oldReceivers) {
      if (platform === 'web') {
        await waitForWebIncomingRecoveryGone(page, secretName);
      } else {
        approvalCoordinator.allow(`${platform}-dismiss-1`, cycle.number);
        await nativeProcesses.get(platform).test.waitForMarker(
          `E2E: ${platform.toUpperCase()}_DISMISSED_INCOMING_${cycle.number}_1`,
          180_000,
        );
      }
    }
    if (sender !== 'web') {
      approvalCoordinator.allow(`${sender}-invalidated-1`, cycle.number);
      await nativeProcesses.get(sender).test.waitForMarker(
        `E2E: ${sender.toUpperCase()}_RECOVERY_INVALIDATED_${cycle.number}_1`,
        180_000,
      );
    }

    for (const { test } of nativeProcesses.values()) await test.result;
    if (nativeProcesses.has('android')) await stopAndroidApplication(androidSerial);
    if (nativeProcesses.has('ios')) await stopIosApplication(simulatorUdid);

    await reopenWebAfterOffline(page);
    if (oldReceivers.includes('web')) await assertNoStaleWebRecoveryAlert(page, secretName);
    if (sender === 'web') {
      const primaryAction = page.getByTestId(`secret-primary-action-${secretName}`);
      await primaryAction.filter({ hasText: /^\s*Recover\s*$/ }).waitFor({
        state: 'visible',
        timeout: 120_000,
      });
    }

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
    console.log(`✅ Test #15 cycle ${cycle.number}/${expectedCycles} passed`);
  }
}

function stepEnvironment({ role, cycle, step, approvalPlatform, sender, action = '', expectedOutcome = '' }) {
  return {
    E2E_ROLE: role,
    E2E_CYCLE: String(cycle),
    E2E_STEP: String(step),
    E2E_APPROVAL_PLATFORM: approvalPlatform,
    E2E_SENDER: sender,
    E2E_ACTION: action,
    E2E_EXPECTED_OUTCOME: expectedOutcome,
  };
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
        currentCycleForDiagnostics = cycle;
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
    return runWebInitiatedSetup(page, simulatorUdid, androidSerial);
  }

  if (initiator === 'android') {
    const androidTest = await startAndroidJoinTest(androidSerial, {
      testClass: scenario.android.setupTestClass ?? scenario.android.testClass,
    });
    await androidTest.waitForMarker('E2E: ANDROID_INITIATOR_READY', 180_000);

    await page.goto(scenario.web.url, { waitUntil: 'domcontentloaded' });
    await unlockWithPasskeyIfNeeded(page);
    await page.getByPlaceholder('vault name').fill(scenario.vault.name);
    await page.getByRole('button', { name: 'Set Vault Name' }).click();
    await page.getByRole('button', { name: 'Join', exact: true }).click();
    await androidTest.waitForMarker('E2E: ANDROID_WEB_JOIN_APPROVED', 180_000);
    await page.getByRole('button', { name: '+ Add Secret' }).waitFor({ timeout: 120_000 });

    const iosTest = startIosJoinTest(simulatorUdid, {
      testClass: scenario.ios.setupTestClass ?? scenario.ios.testClass,
      testMethod: scenario.ios.joinTestMethod,
      label: 'iOS Android-initiated join',
    });
    void iosTest.result.catch(() => {});
    await iosTest.waitForMarker('E2E: IOS_JOIN_REQUEST_SENT', 180_000);
    await androidTest.waitForMarker('E2E: ANDROID_IOS_JOIN_APPROVED', 180_000);
    await iosTest.waitForMarker('E2E: IOS_SECRETS_READY', 210_000);
    await androidTest.waitForMarker('E2E: ANDROID_SECRETS_READY', 210_000);
    await Promise.all([iosTest.result, androidTest.result]);
    // The setup instrumentation leaves MainActivity alive. Start the
    // sender phase from a clean Android process so its socket/native request
    // loop is foregrounded and can flush the first recovery event.
    await stopAndroidApplication(androidSerial);
    await waitForWebSecrets(page);
    return { iosTest: null, androidTest: null };
  }

  if (initiator === 'ios') {
    const iosTest = startIosJoinTest(simulatorUdid, {
      testClass: scenario.ios.setupTestClass ?? scenario.ios.testClass,
      testMethod: scenario.ios.joinTestMethod,
      label: 'iOS initiator setup',
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

    const androidTest = await startAndroidJoinTest(androidSerial, {
      testClass: scenario.android.setupTestClass ?? scenario.android.testClass,
    });
    await androidTest.waitForMarker('E2E: ANDROID_JOIN_REQUEST_SENT', 180_000);
    await iosTest.waitForMarker('E2E: IOS_ANDROID_JOIN_APPROVED', 180_000);
    await androidTest.waitForMarker('E2E: ANDROID_JOIN_READY', 180_000);
    await androidTest.waitForMarker('E2E: ANDROID_SECRETS_READY', 210_000);
    await iosTest.waitForMarker('E2E: IOS_SECRETS_READY', 210_000);
    await Promise.all([iosTest.result, androidTest.result]);
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
    currentCycleForDiagnostics = cycle.number;
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
          approvalPlatform: approver, sender,
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
            approvalPlatform: approver, sender,
          }),
        });
        void test.result.catch(() => {});
        receiverProcesses.push({ platform: receiver, test });
      } else {
        receiverProcesses.push({
          platform: receiver,
          test: await startAndroidStepTest(androidSerial, {
            role: 'receiver', cycle: cycle.number, step,
            approvalPlatform: approver, sender, secretName,
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
          approvalPlatform: approver, sender,
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
  const assertion = watchProcessOutput('adb', [
    '-s', serial, 'shell', '--', 'am', 'instrument', '-w', '-e', 'class',
    `${assertionClass}#${assertionMethod}`,
    `${androidTestBundleId}/androidx.test.runner.AndroidJUnitRunner`,
  ]);
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
  return targetCdp;
}

async function unlockWithPasskeyIfNeeded(page) {
  const createPasskeyButton = page.getByRole('button', { name: 'Create Passkey' });
  const authenticateButton = page.getByRole('button', { name: 'Authenticate with Passkey' });
  const vaultNameInput = page.getByPlaceholder('vault name');
  const secretsLink = page.getByRole('link', { name: 'Secrets', exact: true });

  try {
    await Promise.race([
      vaultNameInput.waitFor({ state: 'visible' }),
      createPasskeyButton.waitFor({ state: 'visible' }),
      authenticateButton.waitFor({ state: 'visible' }),
      // A page reopened in the same browser context may already be unlocked.
      // In that case the navigation link is the readiness boundary; there is
      // no vault-name input to wait for.
      secretsLink.waitFor({ state: 'visible' }),
    ]);
  } catch (error) {
    const bodyText = await page.locator('body').innerText().catch(() => '');
    const html = await page.locator('body').innerHTML().catch(() => '');
    writeDiagnostic(
      `[UI][Web] unlock readiness timeout url=${page.url()} `
      + `title=${JSON.stringify(await page.title().catch(() => ''))} `
      + `body=${JSON.stringify(bodyText.slice(0, 2_000))} `
      + `html=${JSON.stringify(html.slice(0, 4_000))}`,
    );
    throw error;
  }

  if (await createPasskeyButton.isVisible()) {
    console.log('5a. Creating test passkey');
    await createPasskeyButton.click();
  } else if (await authenticateButton.isVisible()) {
    console.log('5a. Authenticating with test passkey');
    await authenticateButton.click();
  }

  if (await vaultNameInput.isVisible().catch(() => false)) return;
  await Promise.race([
    vaultNameInput.waitFor({ state: 'visible' }),
    secretsLink.waitFor({ state: 'visible' }),
  ]);
}

async function approveJoinRequestOnWeb(page, deviceName) {
  console.log(`Web approving ${deviceName} join request`);
  await page.getByRole('link', { name: 'Devices', exact: true }).click();
  await page.getByTestId('pending-device-row').waitFor({ state: 'visible', timeout: 120_000 });
  await page.getByTestId('pending-device-row').click();
  await page.getByTestId('accept-join-request').click();
  await page.getByTestId('pending-device-row').waitFor({ state: 'detached', timeout: 120_000 }).catch(() => {});
}

async function closeWebSecret(page, secretValue = defaultSecret?.value) {
  const closeButton = page.getByRole('button', { name: /close/i }).first();
  if (await closeButton.isVisible().catch(() => false)) {
    await closeButton.click();
    if (secretValue) {
      await page.getByText(secretValue, { exact: true }).waitFor({ state: 'hidden' }).catch(() => {});
    }
  }
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
  { senderClaimMayAlreadyBeAccepted = false } = {},
) {
  const secretValue = secretValueForName(secretName);
  if (senderClaimMayAlreadyBeAccepted) {
    console.log(
      `[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: waiting for sender recovery `
      + 'to reveal after the first approval',
    );
    await page.getByText(secretValue, { exact: true }).waitFor({
      state: 'visible',
      timeout: recoveryShowTimeoutMs,
    });
    console.log(
      `[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: sender recovery auto-revealed; `
      + 'closing it before incoming approval',
    );
    await closeWebSecret(page, secretValue);
    webSenderRevealCompletedCycles.add(currentCycleForDiagnostics);
    return;
  }

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
  { senderSecretName = null, senderClaimMayAlreadyBeAccepted = false } = {},
) {
  if (senderSecretName) {
    await dismissWebRecoveryWaitingUi(page, senderSecretName, { senderClaimMayAlreadyBeAccepted });
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
  await page.getByRole('button', { name: 'Approve', exact: true }).click();
  console.log(`[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: incoming recovery approved`);
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

    currentCycleForDiagnostics = first.number;
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
      currentCycleForDiagnostics = cycle.number;
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
    currentCycleForDiagnostics = cycle.number;
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
      currentCycleForDiagnostics = cycle.number;
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

async function waitForWebSecrets(page) {
  await page.getByRole('link', { name: 'Secrets', exact: true }).click();
  for (const config of Object.values(secretConfigs)) {
    if (!config?.name) continue;
    await page.getByText(config.name, { exact: true }).waitFor({ state: 'visible', timeout: 180_000 });
    await page.getByTestId(`secret-primary-action-${config.name}`).waitFor({ state: 'visible', timeout: 180_000 });
    console.log(`[UI][Web] secret ready: ${config.name}`);
  }
}

async function runConcurrentRecoveryCycles(page, iosTest, androidTest) {
  await page.getByRole('link', { name: 'Secrets', exact: true }).click();

  for (const cycle of recoveryCycles) {
    currentCycleForDiagnostics = cycle.number;
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
    let webSenderDialogHandled = false;
    for (const [index, approval] of cycle.approvals.entries()) {
      const step = index + 1;
      const approver = approval.platform;
      if (approver === 'web') {
        const webSenderSecret = cycle.senderSecrets.web;
        const webOwnClaimAccepted = cycle.approvals
          .slice(0, index)
          .some((previous) => previous.platform !== 'web' && previous.secret === webSenderSecret);
        await approveIncomingRecoveryOnWeb(page, approval.secret, 1, {
          senderSecretName: webSenderSecret && !webSenderDialogHandled ? webSenderSecret : null,
          senderClaimMayAlreadyBeAccepted: webOwnClaimAccepted,
        });
        webSenderDialogHandled = true;
        console.log(`[UI][Web] cycle ${cycle.number}: approval ${step} for ${approval.secret} complete`);
      } else {
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
    console.log(`✅ Concurrent recovery ${cycle.number}/${recoveryCycles.length} passed`);

  }
}

async function main() {
  mkdirSync(artifactsDirectory, { recursive: true });
  writeFileSync(diagnosticLogPath, `=== Test #${testNumber} started ${new Date().toISOString()} ===\n`);
  writeDiagnostic(`scenario=${scenario.name}; cycles=${recoveryCycles.length}`);
  if (!existsSync(webDirectory)) throw new Error(`Web directory not found: ${webDirectory}`);
  if (!existsSync(composeRoot)) throw new Error(`Compose directory not found: ${composeRoot}`);

  console.log(`\n=== ${scenario.name} ===`);
  approvalCoordinator = await startApprovalCoordinator();
  console.log('1. Rebuilding the Web WASM package');
  await runAndWait('/opt/homebrew/bin/task', ['wasm-local'], { cwd: coreRoot });

  console.log('2. Building local server image');
  await runAndWait('/opt/homebrew/bin/task', ['meta-server'], { cwd: coreRoot });

  console.log('3. Cleaning server state');
  await runAndWait('docker', ['rm', '-f', serverContainer], { stdio: 'ignore' }).catch(() => {});
  console.log('4. Starting local server');
  const serverLogLevel = process.env.E2E_SERVER_LOG_LEVEL ?? 'info';
  const server = watchProcessOutput(
    'docker',
    [
      'run', '--rm', '--name', serverContainer,
      '-e', `RUST_LOG=${serverLogLevel}`,
      '-p', `${scenario.server.port}:3000`, serverImage,
    ],
  );
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
  // The WASM tracing layer writes every DEBUG state snapshot to
  // console.debug. With repeated recovery cycles those snapshots become very
  // large and can
  // make the browser target retain hundreds of megabytes of inspector data.
  // E2E markers and failure diagnostics are emitted by the native runners and
  // orchestrator, so suppress only browser DEBUG output for this test page.
  await page.addInitScript(() => {
    console.debug = () => {};
  });
  page.on('console', (message) => {
    const line = `[${message.type()}] ${message.text()}`;
    // Core tracing emits a high-volume `info` stream during recovery. Keep
    // actionable browser logs while avoiding synchronous diagnostic I/O for
    // every state snapshot; native E2E markers remain the synchronization
    // source of truth.
    recordBrowserDiagnostic(line, {
      important: !['debug', 'info'].includes(message.type()),
    });
  });
  page.on('pageerror', (error) => {
    const line = `[pageerror] ${error.message}`;
    recordBrowserDiagnostic(line);
  });
  primaryVirtualAuthenticator = await setupVirtualAuthenticator(page);

  // Clean the iOS simulator before Android starts. The Android helper
  // approves the first pending join request, so a left-over iOS UI-test app
  // from an interrupted run could otherwise be mistaken for the Web request.
  // Test #3 is safe without this because iOS is launched only after Web has
  // already been approved.
  const simulatorUdid = await prepareIosSimulator();

  const androidSerial = await prepareAndroidEmulator();
  if (scenario.mode === 'fourth-device-rejection') {
    if (!existsSync(metaCliBinaryPath)) {
      console.log('6. Building meta-cli for Test #21');
      await runAndWait('cargo', ['build', '-p', 'meta-cli'], {
        cwd: resolve(projectRoot, 'meta-secret'),
      });
    }
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
  await androidTest.waitForMarker('E2E: ANDROID_WEB_JOIN_APPROVED', 120_000);
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

process.once('SIGINT', async () => {
  await stopProcesses();
  process.exit(130);
  });
process.once('SIGTERM', async () => {
  await stopProcesses();
  process.exit(143);
});

main().catch(async (error) => {
  console.error(`\n❌ Test #${testNumber} failed: ${error.message}`);
  writeDiagnostic(`FAILED: ${error.stack ?? error.message}`);
  printFailureDiagnostics();
  await stopProcesses();
  process.exit(1);
});
