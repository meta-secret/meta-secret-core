import assert from 'node:assert/strict';
import test from 'node:test';
import { EventEmitter } from 'node:events';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import vm from 'node:vm';
import { createDiagnosticRedactor } from '../shared/diagnostic-redaction.mjs';
import { createHarnessShutdown } from '../shared/harness-shutdown.mjs';

const source = readFileSync(new URL('./orchestrator.mjs', import.meta.url), 'utf8');
const part = (start, end) => {
  const first = source.indexOf(start), last = source.indexOf(end, first + start.length);
  assert(first >= 0 && last > first, 'Actual Test3 source boundary required');
  return source.slice(first, last);
};
const prefix = 'BROWSER_LIFECYCLE ';
const canary = 'PRIVATE-CANARY-DO-NOT-RETAIN';
async function fixture({ deferSetup = false, onGoto } = {}) {
  const browser = new EventEmitter(), context = new EventEmitter(), page = new EventEmitter();
  const order = [], diagnostics = []; let time = 1000, connected = true, closed = false;
  browser.isConnected = () => connected; page.isClosed = () => closed; page.context = () => context;
  browser.newPage = async () => page;
  browser.close = async () => {
    order.push('browser.close'); if (closed) return;
    connected = false; closed = true;
    page.emit('close', canary); context.emit('close', canary); browser.emit('disconnected', canary);
  };
  page.goto = async (url, options) => {
    assert.equal(url, 'controlled://not-loaded'); assert.equal(options.waitUntil, 'networkidle');
    assert.deepEqual(Object.keys(options), ['waitUntil']);
    order.push('goto'); if (onGoto) await onGoto(c);
  };
  const c = vm.createContext({ Promise, Set, Map, setTimeout, clearTimeout, resolve,
    console: { log() {}, error() {}, warn() {} }, Date: { now: () => ++time },
    browserInstance: undefined, browserCloseRequested: false, browserLifecyclePhase: 'NOT_CREATED',
    browserDiagnostics: diagnostics, errorRedactor: createDiagnosticRedactor([canary]),
    chromium: { launch: async options => { assert.equal(options.headless, false); return browser; } },
    setupVirtualAuthenticator: async () => { order.push('auth'); },
    processes: [], ownedChildClosures: new Map(), iosSuiteActors: [], watchedProcessOutputs: [],
    nativeBusinessPromise: undefined, nativeBusinessFailure: undefined, createHarnessShutdown,
    approvalCoordinator: { close: async () => { order.push('coordinator.close'); } },
    wait: async () => {}, serverContainer: 'controlled-server', runAndWait: async () => { order.push('cleanup'); },
    printFailureDiagnostics() {}, scenario: { web: { url: 'controlled://not-loaded' }, android: { bundleId: 'controlled.android' }, ios: { bundleId: 'controlled.ios' }, vault: { name: 'controlled-vault' } },
    batchProducts: null, composeRoot: '/controlled/compose', iosProjectPath: '/controlled/project',
    iosDerivedDataPath: '/controlled/derived', artifactsDirectory: '/controlled/artifacts',
    runAndCapture() { throw Error('SDK forbidden'); }, mkdirSync() {},
    prepareAndroidEmulator: async () => 'controlled-existing-android', installAndVerifyAndroidE2eApps: async () => {},
    prepareIosSimulator: async () => 'controlled-existing-ios', prepareIosTestApps: async () => ({ appBundleId: 'controlled.ios' }),
    unlockWithPasskeyIfNeeded: async () => {}, requestWebVaultJoin: async () => {}, openWebSecrets: async () => {},
    startAndroidJoinTest: async () => ({ waitForMarker: async () => {} }),
  });
  const initialize = async () => Object.assign(c, await vm.runInContext(`(async () => {${part('  const browser = browserInstance = await chromium.launch(', '  const androidSerial = await prepareAndroidEmulator();')}return { browser, page };})()`, c));
  if (!deferSetup) await initialize();
  return { c, browser, context, page, order, diagnostics, initialize,
    rows: () => diagnostics.filter(line => line.startsWith(prefix)).map(line => JSON.parse(line.slice(prefix.length))),
    phase: name => { c.browserLifecyclePhase = name; },
    installShutdown() { vm.runInContext(part('async function performCleanup(', 'async function findIosSimulator('), c); },
    async nativeAndGoto() { await vm.runInContext(`(async () => {${part('  const androidSerial = await prepareAndroidEmulator();', '  const iosTest = startIosJoinTest(simulatorUdid);')}})()`, c); },
  };
}

test('actual Test3 retains unexpected browser disconnect during native preparation', async () => {
  const f = await fixture(); f.browser.emit('disconnected', { reason: canary });
  assert.equal(f.rows().length, 1, 'Unexpected browser disconnect must be retained during native preparation');
  const row = f.rows()[0];
  assert.equal(row.EVENT, 'BROWSER_DISCONNECTED'); assert.equal(row.REQUESTED_CLOSE, 0);
  assert.equal(row.PHASE, 'NATIVE_PREP'); assert.equal(row.SEQ, 1); assert.equal(row.TIME_MS, 1001);
  assert.deepEqual(f.order, ['auth', 'goto']);
});

test('actual Test3 records immutable event-time phase and bounded nonthrowing fixed metadata', async () => {
  const f = await fixture();
  f.context.emit('close', canary); f.page.emit('crash', new Error(canary));
  f.c.browserCloseRequested = true; f.phase('OWNER_CLOSE'); f.page.emit('close', canary);
  const rows = f.rows(); assert.equal(rows.length, 3);
  assert.equal(rows[0].REQUESTED_CLOSE, 0); assert.equal(rows[0].PHASE, 'NATIVE_PREP');
  assert.equal(rows[2].REQUESTED_CLOSE, 1); assert.equal(rows[2].PHASE, 'OWNER_CLOSE');
  assert.deepEqual(rows.map(row => row.SEQ), [1, 2, 3]);
  assert.deepEqual(rows.map(row => row.TIME_MS), [1001, 1002, 1003]);
  f.phase(canary); f.page.emit('crash', canary); assert.equal(f.rows()[3].PHASE, 'UNKNOWN');
  for (let n = 0; n < 30; n++) f.page.emit('close', { url: canary, title: canary });
  assert.equal(f.rows().length, 8); assert(!f.diagnostics.join('\n').includes(canary));
  for (const row of f.rows()) {
    assert.deepEqual(Object.keys(row), ['SEQ', 'TIME_MS', 'PHASE', 'EVENT', 'REQUESTED_CLOSE', 'CONNECTED', 'PAGE_CLOSED']);
    assert(Buffer.byteLength(prefix + JSON.stringify(row)) <= 512);
  }
  f.diagnostics.push = () => { throw Error('controlled log failure'); };
  assert.doesNotThrow(() => f.browser.emit('disconnected', canary));
});

test('actual Test3 existing await phases and both owner close sites capture requested close', async () => {
  const phases = [], markers = [];
  const f = await fixture({ onGoto: c => { phases.push(c.browserLifecyclePhase); } });
  f.c.prepareAndroidEmulator = async () => { phases.push(f.c.browserLifecyclePhase); return 'controlled-android'; };
  f.c.startAndroidJoinTest = async () => ({ waitForMarker: async marker => {
    markers.push(marker);
    if (marker === 'E2E: ANDROID_INITIATOR_READY') phases.push(f.c.browserLifecyclePhase);
    else assert.equal(marker, 'E2E: ANDROID_WEB_JOIN_APPROVED');
  } });
  f.c.unlockWithPasskeyIfNeeded = async () => { phases.push(f.c.browserLifecyclePhase); };
  await f.nativeAndGoto();
  assert.deepEqual(markers, ['E2E: ANDROID_INITIATOR_READY', 'E2E: ANDROID_WEB_JOIN_APPROVED']);
  assert.deepEqual(phases, ['BEFORE_GOTO', 'NATIVE_PREP', 'AWAIT_ANDROID_INITIATOR', 'AWAIT_ANDROID_INITIATOR']);
  f.browser.emit('disconnected', canary); const original = f.rows()[0];
  f.installShutdown(); await vm.runInContext('harnessShutdown.finish({ error: new Error("controlled primary") })', f.c);
  assert.equal(original.REQUESTED_CLOSE, 0); assert.equal(f.rows()[1].REQUESTED_CLOSE, 1);
  assert.equal(f.rows()[1].PHASE, 'OWNER_CLOSE'); assert.equal(f.order.filter(x => x === 'cleanup').length, 1);
  const success = await fixture(); success.c.exitOnSuccess = true;
  success.c.stopProcesses = async () => { success.order.push('success.cleanup'); };
  const start = source.indexOf('  if (exitOnSuccess) {', source.indexOf('async function main('));
  const end = source.indexOf("  console.log('Browser remains open.", start);
  await vm.runInContext(`(async () => {${source.slice(start, end)}})()`, success.c);
  assert.equal(success.rows()[0].REQUESTED_CLOSE, 1); assert.equal(success.rows()[0].PHASE, 'OWNER_CLOSE');
  assert.deepEqual(success.order, ['auth', 'goto', 'browser.close', 'success.cleanup']);
});

test('actual Test3 observer failure preserves exact navigation primary and single cleanup', async () => {
  for (const message of ['controlled navigation failure', 'Target page, context or browser has been closed']) {
    const f = await fixture({ deferSetup: true }), primary = new Error(message), rendered = [], exits = [];
    f.diagnostics.push = () => { throw Error('controlled observer failure'); };
    f.c.setupVirtualAuthenticator = async () => {
      assert.doesNotThrow(() => f.page.emit('crash', { detail: canary }));
    };
    f.page.goto = async () => { throw primary; }; f.installShutdown();
    f.c.prepareAndroidEmulator = async () => { throw Error('Native preparation must not follow failed goto'); };
    f.c.main = async () => { await f.initialize(); await f.nativeAndGoto(); };
    f.c.formatHarnessShutdownError = error => { rendered.push(error); return 'controlled safe primary'; };
    f.c.process = { exit: code => exits.push(code) };
    await vm.runInContext(source.slice(source.indexOf('main().catch(async (error) => {')), f.c);
    assert.equal(rendered[0], primary); assert.deepEqual(exits, [1]);
    assert.equal(f.order.filter(x => x === 'cleanup').length, 1);
    assert.equal(f.order.filter(x => x === 'browser.close').length, 1);
  }
});

test('actual Test3 opens Web before blocked native preparation and gates joins on Android READY', async () => {
  const f = await fixture(), calls = [];
  let prepEntered, releasePrep, readyEntered, releaseReady;
  const entered = new Promise(done => { prepEntered = done; });
  const prep = new Promise(done => { releasePrep = done; });
  const readySeen = new Promise(done => { readyEntered = done; });
  const ready = new Promise(done => { releaseReady = done; });
  f.c.prepareAndroidEmulator = async () => {
    calls.push('native-prepare'); prepEntered(); await prep; return 'controlled-android';
  };
  f.c.startAndroidJoinTest = async () => ({ waitForMarker: async marker => {
    calls.push(marker);
    if (marker === 'E2E: ANDROID_INITIATOR_READY') { readyEntered(); await ready; }
    else assert.equal(marker, 'E2E: ANDROID_WEB_JOIN_APPROVED');
  } });
  f.c.unlockWithPasskeyIfNeeded = async () => { calls.push('web-auth'); };
  f.c.requestWebVaultJoin = async () => { calls.push('web-join'); };
  f.c.startIosJoinTest = () => { calls.push('ios-actor'); return {}; };
  const body = vm.runInContext(`(async () => {${part('  const androidSerial = await prepareAndroidEmulator();', '  nativeBusinessPromise = runIosJoinAndRecovery(')}})()`, f.c);
  void body.catch(() => {});
  try {
    await entered;
    assert.equal(f.order.filter(item => item === 'goto').length, 1,
      'The same single Web goto must finish before native preparation starts');
    assert.deepEqual(calls, ['native-prepare']);
    releasePrep(); await readySeen;
    assert.deepEqual(calls, ['native-prepare', 'E2E: ANDROID_INITIATOR_READY']);
    releaseReady(); await body;
    assert.deepEqual(calls, ['native-prepare', 'E2E: ANDROID_INITIATOR_READY', 'web-auth', 'web-join',
      'E2E: ANDROID_WEB_JOIN_APPROVED', 'ios-actor']);
    assert.equal(f.order.filter(item => item === 'goto').length, 1);
  } finally { releasePrep(); releaseReady(); await body.catch(() => {}); }
});
