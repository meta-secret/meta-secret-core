// Ordinary host integration tests execute complete actual owning-source bodies.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {once} from 'node:events';
import {createServer as nodeCreateServer} from 'node:http';
import {readFileSync} from 'node:fs';
import {spawn} from 'node:child_process';
import {abortRecoveryPhaseHarness, createRecoveryPhaseRegistry, handleRecoveryPhaseRequest} from './recovery-phase.mjs';
import {superviseAndroidInstrumentation} from './android-instrumentation.mjs';
import {createHarnessShutdown} from './harness-shutdown.mjs';
import {createFifoPublication} from './recovery-fifo.mjs';

const source = readFileSync(new URL('../Test4/orchestrator.mjs', import.meta.url), 'utf8');
function actual(marker) {
  assert.equal(source.split(marker).length, 2, 'Exact actual-source boundary required');
  const start = source.indexOf(marker);
  const bodyOpen = marker.endsWith('{') ? start + marker.length - 1 : source.indexOf('{', start);
  let cursor = bodyOpen + 1, depth = 1;
  // Same finite balanced-source extraction used by the original host bank.
  while (depth && cursor < source.length) { depth += (source[cursor] === '{') - (source[cursor] === '}'); cursor++; }
  assert.equal(depth, 0); return source.slice(start, cursor);
}
async function actualCoordinatorFixture({testNumber = 24, mode = 'ios-multi-secret-lifecycle'} = {}) {
  let actualServer;
  const createServer = (...args) => (actualServer = nodeCreateServer(...args));
  const ports = {createServer, createFifoPublication, createRecoveryPhaseRegistry, handleRecoveryPhaseRequest,
    scenario: {runId: 'SYNTHETIC_UNIT_RUN', mode, vault: {name: 'SYNTHETIC_HOST_VAULT'}}, testNumber,
    scenarioArtifactLabel: 'SYNTHETIC_BLOCK', secretConfigs: {}, recoveryCycles: [], secretCreationPlan: [], joinExpectedSecretNames: []};
  const start = new Function('ports', 'const {createServer,createFifoPublication,createRecoveryPhaseRegistry,handleRecoveryPhaseRequest,scenario,testNumber,scenarioArtifactLabel,secretConfigs,recoveryCycles,secretCreationPlan,joinExpectedSecretNames}=ports;\n'
    + actual('function startApprovalCoordinator(') + '\nreturn startApprovalCoordinator;')(ports);
  const coordinator = await start(0);
  return {actual: coordinator, port: actualServer.address().port};
}
function actualTrackedHarnessFixture() {
  const prelude = `const {spawn,once,process,createHarnessShutdown}=ports;
const processes=[],watchedProcessOutputs=[],effects=[],projectRoot=process.cwd();
let genericIosActor,genericIosBusinessPromise,browserInstance,cancelledActorPids=[];
const androidSdkTools=null,withAndroidSdkPath=value=>value;
const ownedProcessTreePids=async()=>processes.filter(p=>p.exitCode===null&&p.signalCode===null).map(p=>p.pid);
const terminateOwnedProcessTree=async()=>{};
const printFailureDiagnostics=()=>effects.push('diagnostics'),captureFailureScreenshots=async()=>{};
const performCleanup=async()=>{effects.push('cleanup');await Promise.all(processes.map(p=>p.exitCode===null&&p.signalCode===null?once(p,'close'):Promise.resolve()));};
`;
  return new Function('ports', prelude + actual('const harnessShutdown = createHarnessShutdown({') + ');\n'
    + actual('function run(command, args, options = {}) {') + '\nreturn {run,harnessShutdown,processes,effects};')({spawn, once, process, createHarnessShutdown});
}

async function coordinator(options) {
  const {actual, port} = await actualCoordinatorFixture(options);
  return {actual, base: `http://127.0.0.1:${port}`};
}
const binding = () => ({version: 1, runId: 'SYNTHETIC_UNIT_RUN', testNumber: 24, attempt: 1,
  blockId: 'secret1-three-device', cycle: 13, logicalStep: 1, mode: 'ios-multi-secret-lifecycle',
  contextId: 'a'.repeat(32), sender: 'web', participants: ['web','android','ios'].map((platform,i) => ({
    platform, role: platform === 'web' ? 'sender' : 'receiver', invocationId: String(i+1).repeat(32)}))});
function nativeLine(context, platform, event) {
  const id = context.participants.find(p => p.platform === platform).invocationId;
  const line = `E2E: RECOVERY_PHASE_V1 context=${context.contextId} invocation=${id} platform=${platform} phase=${event}\n`;
  return platform === 'android' ? '11-03 00:15:35.985 18342 18362 I MetaSecretE2E: ' + line : line;
}

test('actual opted-in coordinator denies an already-allowed old approval HTTP read after freeze', async () => {
  const {actual, base} = await coordinator(), context = binding();
  const phase = actual.registerRecoveryPhase(context); const path = '/approval?platform=android-approve-1&cycle=13';
  try {
    const android = phase.observer('android', {}), ios = phase.observer('ios', {});
    phase.web('READY'); android.push('stdout', nativeLine(context, 'android', 'READY')); ios.push('stdout', nativeLine(context, 'ios', 'READY'));
    phase.releaseSender(); phase.web('REQUEST_SENT');
    android.push('stdout', nativeLine(context, 'android', 'INCOMING_VISIBLE')); ios.push('stdout', nativeLine(context, 'ios', 'INCOMING_VISIBLE'));
    phase.requireBothBadges();
    actual.allow('android-approve-1', 13);
    assert.equal(await (await fetch(base + path)).text(), 'allowed');
    actual.freezeGrants();
    assert.equal((await fetch(base + path)).status, 410);
    assert.throws(() => actual.allow('ios-approve-1', 13));
    assert.throws(() => phase.releaseSender());
  } finally { actual.freezeGrants(); await actual.close(); }
});

test('actual non-opted coordinator retains legacy reads, setup/direct-Show keys remain usable', async () => {
  const {actual, base} = await coordinator({testNumber: 37, mode: 'web-member-removal-lifecycle'});
  try {
    actual.allow('android-approve-1', 13); actual.freezeGrants();
    assert.equal(await (await fetch(base + '/approval?platform=android-approve-1&cycle=13')).text(), 'allowed');
    assert.equal((await fetch(base + '/recovery-phase?v=1')).status, 404);
  } finally { await actual.close(); }
  const opted = await coordinator();
  try {
    opted.actual.allow('ios-owner', 2);
    assert.equal(await (await fetch(opted.base + '/approval?platform=ios-owner&cycle=2')).text(), 'allowed');
    assert.throws(() => opted.actual.allow('android-approve-1', 14));
  } finally { opted.actual.freezeGrants(); await opted.actual.close(); }
});

test('actual run tracks real instrumentation and logcat workers; actual non-generic shutdown cancels both', async () => {
  const {actual} = await coordinator(), harness = actualTrackedHarnessFixture(), context = binding();
  const workerArgs = ['-e', 'setInterval(()=>{},1000)'];
  const instrumentation = harness.run(process.execPath, workerArgs, {stdio: 'ignore'});
  const logcat = harness.run(process.execPath, workerArgs, {stdio: 'ignore'});
  const instrumentClosed = once(instrumentation, 'close'), logcatClosed = once(logcat, 'close');
  await Promise.all([once(instrumentation, 'spawn'), once(logcat, 'spawn')]);
  const actor = superviseAndroidInstrumentation({instrumentationResult: instrumentClosed.then(() => ({stdout: 'HOST_FIXTURE', stderr: ''})),
    logcat: {child: logcat, result: logcatClosed, waitForMarker: () => new Promise(() => {})}, selector: 'SYNTHETIC_HOST_WORKER'});
  const phase = actual.registerRecoveryPhase(context, abortRecoveryPhaseHarness({coordinator: actual, shutdown: harness.harnessShutdown}));
  try {
    assert.equal(harness.processes.length, 2); assert.equal(actor.child, logcat);
    actor.child.kill('SIGTERM'); await logcatClosed;
    assert.equal(instrumentation.exitCode, null); assert.equal(instrumentation.signalCode, null);
    const error = Error('SYNTHETIC_PHASE_TIMEOUT'); phase.abort(error);
    await assert.rejects(phase.failure, current => current === error);
    await harness.harnessShutdown.finish({error}); await instrumentClosed; await actor.result;
    assert.ok(instrumentation.exitCode !== null || instrumentation.signalCode !== null);
    assert.throws(() => harness.run(process.execPath, workerArgs, {stdio: 'ignore'}));
    assert.deepEqual(harness.effects, ['diagnostics', 'cleanup']);
  } finally {
    phase.abort(Error('fixture cleanup'));
    for (const child of harness.processes) if (child.exitCode === null && child.signalCode === null) child.kill('SIGKILL');
    await Promise.all([instrumentClosed, logcatClosed]); await actual.close();
  }
});

test('actual coordinator phase evidence is current-scoped; both receivers precede approval continuation', async () => {
  const {actual, base} = await coordinator(), context = binding();
  const phase = actual.registerRecoveryPhase(context), android = phase.observer('android', {}), ios = phase.observer('ios', {});
  try {
    android.push('stdout', nativeLine(context, 'android', 'READY')); ios.push('stdout', nativeLine(context, 'ios', 'READY'));
    assert.throws(() => phase.releaseSender()); phase.web('READY'); phase.releaseSender(); phase.web('REQUEST_SENT');
    android.push('stdout', nativeLine(context, 'android', 'INCOMING_VISIBLE')); assert.throws(() => phase.requireBothBadges());
    ios.push('stdout', nativeLine(context, 'ios', 'INCOMING_VISIBLE')); phase.requireBothBadges();
    actual.allow('android-approve-1', 13);
    assert.equal(await (await fetch(base + '/approval?platform=android-approve-1&cycle=13')).text(), 'allowed');
    phase.complete();
    assert.equal((await fetch(base + '/approval?platform=android-approve-1&cycle=13')).status, 410);
  } finally { actual.freezeGrants(); await actual.close(); }
});
