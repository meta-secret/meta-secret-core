import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync, chmodSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { verifyArtifactProvenance } from './build-provenance-manifest.mjs';
import { createBatchBuildHandoff, enumerateAndroidConfigurations, verifyPreparedProduct } from './batch-build-handoff.mjs';
import { createBatchProductPreparation } from './batch-product-builders.mjs';
import { hashArtifactTree } from './artifact-tree.mjs';
import { hashWorkingSources, createProductProofPorts } from './batch-product-proof.mjs';
import { batchEnvironmentPaths, createBatchEnvironmentGuard, environmentTreeHash, assertSnapshotDiskBudget } from './batch-environment.mjs';
import { GiB, planPreparationBudget, createPreparationResourceBudget, assertPreparationResources } from './e2e-resource-budget.mjs';

// Deferred module contract. This file is excluded from the first baseline;
// missing imports are not meaningful RED. Builders below are injected recording
// ports: these tests prove orchestration, never an actual compiler or APK ABI.
import {createEnvironmentTestRun} from './environment-test-fixture.mjs';

const configurations = [
  { name: 'defaultLocal', environment: 'local', socketUrl: '', e2eServerUrl: '' },
  { name: 'network16', environment: 'local', socketUrl: 'http://10.0.2.2:5181', e2eServerUrl: 'http://10.0.2.2:5181' },
  { name: 'deletion50', environment: 'local', socketUrl: 'http://10.0.2.2:5201', e2eServerUrl: 'http://10.0.2.2:5201' },
];
const target = { platform: 'android', buildType: 'debug', package: 'metasecret.project.com', abis: ['arm64-v8a', 'armeabi-v7a', 'x86_64'] };
const tuple = (configuration = configurations[0]) => ({ product: 'android', configuration, target });
const hash = (path) => createHash('sha256').update(readFileSync(path)).digest('hex');

function handoffFixture({ holdBuild = false, failBuild = false, badAbi = false } = {}) {
  const directory = mkdtempSync(resolve(tmpdir(), 'e2e-batch-build-handoff-'));
  const trace = []; const state = { source: 'core+compose-current', bindings: 'bindings-current', toolchain: 'toolchain-current', iosLink: 'expanded-link-current' };
  const working = { app: resolve(directory, 'working-app.apk'), test: resolve(directory, 'working-test.apk') };
  let releaseBuild; let enteredBuild;
  const entered = new Promise((resolvePromise) => { enteredBuild = resolvePromise; });
  const held = new Promise((resolvePromise) => { releaseBuild = resolvePromise; });
  const makeReceipt = (name, request, products) => {
    const receiptPath = resolve(directory, `${name}-${trace.length}-builder.json`);
    writeFileSync(receiptPath, JSON.stringify({ completed: true, exitCode: 0, request, products }));
    return { exitCode: 0, receiptPath, products };
  };
  const builders = {
    androidFfi: async (request) => {
      trace.push(['bash', 'meta-secret/mobile/scripts/build-android.sh']);
      return makeReceipt('ffi', request, {});
    },
    android: async (request) => {
      trace.push(['./gradlew', ':composeApp:assembleDebug', ':composeApp:assembleDebugAndroidTest',
        '-PMETA_SECRET_ENV=local', `-PMETA_SECRET_SOCKET_URL=${request.configuration.socketUrl}`,
        `-PMETA_SECRET_E2E_SERVER_URL=${request.configuration.e2eServerUrl}`]);
      enteredBuild(); if (holdBuild) await held;
      if (failBuild) throw new Error('owning Gradle builder failed');
      const abis = Object.fromEntries(request.target.abis.map((abi) => [abi, 'current-ffi']));
      if (badAbi === 'missing-x86_64') delete abis.x86_64;
      else if (badAbi) abis[badAbi === 'foreign-v7' ? 'armeabi-v7a' : 'arm64-v8a'] = 'foreign-ffi';
      writeFileSync(working.app, JSON.stringify({ configuration: request.configuration, target: request.target,
        abis, source: state.source }));
      writeFileSync(working.test, JSON.stringify({ configuration: request.configuration, source: state.source, package: `${target.package}.test` }));
      return makeReceipt('android', request, working);
    },
  };
  for (const product of ['ios', 'wasm', 'server']) builders[product] = async (request) => {
    trace.push(product === 'ios' ? ['xcodebuild', 'build-for-testing', '-scheme', 'iosApp', '-configuration', 'Debug']
      : ['/opt/homebrew/bin/task', product === 'wasm' ? 'wasm-local' : 'meta-server']);
    const products = Object.fromEntries((product === 'ios' ? ['app', 'test'] : [product]).map((kind) => {
      const path = resolve(directory, `working-${product}-${kind}`); writeFileSync(path, `${product}/${kind}/${state.source}`); return [kind, path];
    }));
    return makeReceipt(product, request, products);
  };
  const options = { runDirectory: directory,
    captureInputs: async (request) => ({ version: 3, scope: ['native'], sourceFingerprint: state.source,
      configuration: structuredClone(request.configuration), target: structuredClone(request.target),
      inputHashes: { core: 'core-current', compose: state.source, bindings: state.bindings, toolchain: state.toolchain,
        ffiSource: 'core-current', ffiAbis: { 'arm64-v8a': 'current-ffi', 'armeabi-v7a': 'current-ffi', x86_64: 'current-ffi' },
        iosLink: state.iosLink, iosAppAndTests: state.source, wasmSources: state.source, serverSources: state.source } }),
    builders,
    inspectProducts: async ({ tuple: request, paths }) => {
      if (request.product !== 'android') return { hashes: Object.fromEntries(Object.entries(paths).map(([kind, path]) => [kind, hash(path)])) };
      const app = JSON.parse(readFileSync(paths.app, 'utf8')); const instrumentedTest = JSON.parse(readFileSync(paths.test, 'utf8'));
      if (JSON.stringify(Object.keys(app.abis).sort()) !== JSON.stringify([...request.target.abis].sort())
        || request.target.abis.some((abi) => app.abis[abi] !== 'current-ffi')) throw new Error('packaged native ABI provenance mismatch');
      if (JSON.stringify(app.configuration) !== JSON.stringify(instrumentedTest.configuration) || app.source !== instrumentedTest.source) throw new Error('app/test pair provenance mismatch');
      return { hashes: { app: hash(paths.app), test: hash(paths.test) }, packagedAbis: app.abis,
        configuration: app.configuration, target: app.target };
    },
  };
  return { directory, options, trace, state, working, entered, releaseBuild, cleanup: () => rmSync(directory, { recursive: true, force: true }) };
}

test('F2 handoff: all74 original JSON select the existing three Android route configurations', () => {
  const e2eRoot = fileURLToPath(new URL('..', import.meta.url)); const scenarios = [];
  for (const name of readdirSync(e2eRoot).filter((value) => /^Test\d+$/.test(value) && Number(value.slice(4)) <= 50)) {
    for (const file of readdirSync(resolve(e2eRoot, name)).filter((value) => /^scenario.*\.json$/.test(value))) {
      scenarios.push({ testNumber: Number(name.slice(4)), scenario: JSON.parse(readFileSync(resolve(e2eRoot, name, file), 'utf8')) });
    }
  }
  const before = JSON.stringify(scenarios); assert.equal(scenarios.length, 74);
  const actual = enumerateAndroidConfigurations(scenarios);
  assert.deepEqual(actual.map(({ configuration }) => configuration).sort((a, b) => a.name.localeCompare(b.name)),
    [...configurations].sort((a, b) => a.name.localeCompare(b.name)));
  assert.deepEqual(Object.fromEntries(actual.map(({ configuration, scenarios: entries }) => [configuration.name, entries.length])),
    { defaultLocal: 71, network16: 1, deletion50: 1 });
  assert.equal(JSON.stringify(scenarios), before);
});

test('F2 handoff: three app+test assemblies share one FFI build and repeated selections use immutable route-bound stashes', async () => {
  const f = handoffFixture();
  try {
    const handoff = createBatchBuildHandoff(f.options); const prepared = [];
    for (const configuration of configurations) prepared.push(await handoff.prepare(tuple(configuration)));
    await handoff.prepare(tuple()); await handoff.seal();
    const buildTrace = structuredClone(f.trace);
    assert.equal(f.trace.filter(([file]) => file === 'bash').length, 1);
    assert.equal(f.trace.filter(([file]) => file === './gradlew').length, 3);
    assert.equal(new Set(prepared.map(({ products }) => products.app)).size, 3);
    for (const configuration of configurations) for (let cycle = 0; cycle < 2; cycle++) {
      const receipt = await handoff.consume(tuple(configuration));
      assert.notEqual(receipt.products.app, f.working.app); assert.notEqual(receipt.products.test, f.working.test);
      assert.equal(JSON.parse(readFileSync(receipt.products.app, 'utf8')).configuration.socketUrl, configuration.socketUrl);
      assert.equal(JSON.parse(readFileSync(receipt.products.test, 'utf8')).configuration.e2eServerUrl, configuration.e2eServerUrl);
      assert.deepEqual(receipt.current.configuration, configuration); assert.deepEqual(receipt.current.target, target);
      assert.doesNotThrow(() => verifyArtifactProvenance({ path: receipt.manifestPath, current: receipt.current }));
    }
    assert.deepEqual(f.trace, buildTrace, 'Reuse must not invoke any builder, including FFI');
  } finally { f.cleanup(); }
});

test('F2 handoff: awaited owning build completion is required before recording proof or handing off products', { timeout: 5000 }, async (t) => {
  const f = handoffFixture({ holdBuild: true });
  let preparation;
  const aborted = new Promise((_, reject) => t.signal.addEventListener('abort', () => reject(t.signal.reason), { once: true }));
  const bounded = (promise) => Promise.race([promise, aborted]);
  try {
    const handoff = createBatchBuildHandoff(f.options); let settled = false;
    preparation = handoff.prepare(tuple()).finally(() => { settled = true; });
    await bounded(Promise.race([f.entered, preparation])); await Promise.resolve(); assert.equal(settled, false);
    await bounded(assert.rejects(handoff.consume(tuple()), /prepared|pending|build|sealed/i));
    f.releaseBuild(); const receipt = await bounded(preparation);
    assert(existsSync(receipt.builderReceiptPath)); assert(existsSync(receipt.manifestPath));
    await handoff.seal(); await handoff.consume(tuple());
  } finally { f.releaseBuild(); if (preparation) await bounded(preparation).catch(() => {}); f.cleanup(); }
});

test('F2 handoff: failed owning builder and invalid packaged FFI cannot mint reusable proof', async () => {
  for (const options of [{ failBuild: true }, { badAbi: true }, { badAbi: 'missing-x86_64' }, { badAbi: 'foreign-v7' }]) {
    const f = handoffFixture(options);
    try {
      const handoff = createBatchBuildHandoff(f.options);
      await assert.rejects(handoff.prepare(tuple()), /builder failed|ABI provenance mismatch/);
      await assert.rejects(handoff.consume(tuple()), /prepared|proof|failed|sealed/i);
    } finally { f.cleanup(); }
  }
});

test('F2 handoff: source, binding and toolchain drift after preparation fails closed without recompiling a prepared tuple', async () => {
  for (const key of ['source', 'bindings', 'toolchain']) {
    const f = handoffFixture();
    try {
      const handoff = createBatchBuildHandoff(f.options); await handoff.prepare(tuple()); const before = structuredClone(f.trace);
      f.state[key] = 'changed-after-preparation';
      await assert.rejects(handoff.prepare(tuple()), /changed|drift|mismatch/i);
      await assert.rejects(handoff.consume(tuple()), /changed|drift|mismatch|sealed/i);
      assert.deepEqual(f.trace, before);
    } finally { f.cleanup(); }
  }
});

test('F2 handoff: mutated stash, missing manifest and lost or forged successful builder receipt fail before business work', async () => {
  for (const mutation of ['app', 'test', 'manifest', 'missingBuilder', 'forgedBuilder']) {
    const f = handoffFixture();
    try {
      const handoff = createBatchBuildHandoff(f.options); const receipt = await handoff.prepare(tuple()); await handoff.seal();
      const before = structuredClone(f.trace);
      if (mutation === 'app' || mutation === 'test') writeFileSync(receipt.products[mutation], 'mutated product');
      if (mutation === 'manifest') rmSync(receipt.manifestPath);
      if (mutation === 'missingBuilder') rmSync(receipt.builderReceiptPath);
      if (mutation === 'forgedBuilder') writeFileSync(receipt.builderReceiptPath, JSON.stringify({ exitCode: 0, completed: true, evidence: 'invented success' }));
      await assert.rejects(handoff.consume(tuple()), /proof|provenance|manifest|receipt|product|JSON/i, mutation);
      assert.deepEqual(f.trace, before, 'Missing proof during an active batch cannot bootstrap or rebuild');
    } finally { f.cleanup(); }
  }
});

test('F2 handoff: unprepared configuration, package and ABI target cannot borrow a prepared receipt after sealing', async () => {
  const f = handoffFixture();
  try {
    const handoff = createBatchBuildHandoff(f.options); await handoff.prepare(tuple()); await handoff.seal();
    const before = structuredClone(f.trace);
    for (const request of [tuple(configurations[1]), { ...tuple(), target: { ...target, package: 'foreign.app' } },
      { ...tuple(), target: { ...target, abis: ['x86_64'] } }]) {
      await assert.rejects(handoff.consume(request), /prepared|configuration|target|tuple|proof/i);
      await assert.rejects(handoff.prepare(request), /sealed|prepared|configuration|target|tuple/i);
    }
    assert.deepEqual(f.trace, before);
  } finally { f.cleanup(); }
});

test('F2 handoff: different source+configuration+target tuples can build only in their owning preparation stage', async () => {
  const f = handoffFixture();
  try {
    const handoff = createBatchBuildHandoff(f.options);
    await handoff.prepare(tuple());
    const x64 = { ...tuple(), target: { ...target, abis: ['x86_64'] } };
    await handoff.prepare(x64); await handoff.seal();
    assert.equal(f.trace.filter(([file]) => file === './gradlew').length, 2);
    assert.equal(f.trace.filter(([file]) => file === 'bash').length, 1);
    assert(!f.trace.some((command) => command.includes('set-env') || command.includes('install') || command.includes('boot')));
  } finally { f.cleanup(); }
});

test('F2 handoff: iOS full app/XCTest scope plus WASM and server each build once before runtime and reuse without compilers', async () => {
  const f = handoffFixture();
  const requests = [
    { product: 'ios', configuration: { environment: 'local', configuration: 'Debug' },
      target: { platform: 'ios', sdk: 'iphonesimulator', arch: 'arm64', scheme: 'iosApp',
        testSelectors: ['iosAppUITests/CaseOneJoinFromIosUITest/testJoinApprovedVaultAndShowSecret',
          'iosAppUITests/CaseTwoIosInitiatorUITest/testCreateVaultThenApproveAndroidAndWeb',
          'iosAppUITests/CaseThreeIosJoinUITest/testJoinAndroidInitiatedVaultAndHandleRecovery'] } },
    { product: 'wasm', configuration: { environment: 'local' }, target: { platform: 'web', target: 'wasm32-unknown-unknown' } },
    { product: 'server', configuration: { environment: 'local' }, target: { platform: 'docker', image: 'local-e2e-server' } },
  ];
  try {
    const handoff = createBatchBuildHandoff(f.options);
    for (const request of requests) { await handoff.prepare(request); await handoff.prepare(request); }
    await handoff.seal(); const built = structuredClone(f.trace);
    assert.equal(built.filter(([file]) => file === 'xcodebuild').length, 1);
    assert.equal(built.filter((call) => call.includes('wasm-local')).length, 1);
    assert.equal(built.filter((call) => call.includes('meta-server')).length, 1);
    assert(!built.some((call) => call.some((argument) => argument.startsWith('-only-testing:'))), 'Shared bundle cannot bake in only Test1');
    for (const request of requests) for (let actor = 0; actor < 2; actor++) {
      const receipt = await handoff.consume(request);
      assert.doesNotThrow(() => verifyArtifactProvenance({ path: receipt.manifestPath, current: receipt.current }));
      assert.deepEqual(receipt.current.target, request.target);
    }
    assert.deepEqual(f.trace, built);
    f.state.iosLink = 'foreign-expanded-link-inputs';
    await assert.rejects(handoff.consume(requests[0]), /changed|drift|mismatch/i);
    assert.deepEqual(f.trace, built, 'Full iOS/link drift must fail instead of compiling inside business runtime');
  } finally { f.cleanup(); }
});

test('F2 real builder ports: actual preparation graph awaits owning commands, stashes each route once and never installs clients', async () => {
  const f = handoffFixture(); const trace = []; let nativeCalls = 0; let iosRecorded = 0;
  const coreRoot = f.directory; const composeRoot = resolve(f.directory, 'compose');
  const derivedDataPath = resolve(f.directory, 'derived'); const wasmPath = resolve(f.directory, 'pkg');
  const put = (path, value) => { mkdirSync(resolve(path, '..'), { recursive: true }); writeFileSync(path, value); };
  const outputs = { app: resolve(composeRoot, 'composeApp/build/outputs/apk/debug/composeApp-debug.apk'),
    test: resolve(composeRoot, 'composeApp/build/outputs/apk/androidTest/debug/composeApp-debug-androidTest.apk') };
  const scenarios = [1, 16, 50].map((testNumber) => ({ testNumber, scenario: { android: {}, ios: {}, web: {},
    server: { image: 'fixture-image' }, ...(testNumber === 1 ? {} : { networkLoss: { androidProxyPort: testNumber === 16 ? 5181 : 5201 } }) } }));
  try {
    const options = { paths: { coreRoot, composeRoot, runDirectory: f.directory }, runId: 'fixture-run', scenarios,
      prepareNative: async () => { nativeCalls++; put(resolve(coreRoot, 'e2eTest/.artifacts/android-ffi-build-provenance.json'), 'fixture official-native receipt'); return { android: true, ios: true, cli: false }; },
      commands: {
        checkBudget: () => ({ requiredBytes: 0 }),
        proofPorts: { derivedDataPath, wasmPath, captureInputs: f.options.captureInputs,
          inspectProducts: ({ paths }) => ({ hashes: Object.fromEntries(Object.entries(paths).map(([key, path]) => [key, hashArtifactTree(path)])) }) },
        iosBuild: { prepare: async options => { assert.equal(options.identityRequired, true); return { checkedLink: true }; }, record: () => { iosRecorded++; assert(trace.some((call) => call.includes('build-for-testing'))); } },
        runAndCapture: async (file, args) => { trace.push([file, ...args]); assert.equal(file, 'docker'); return { stdout: 'sha256:fixture-image\n' }; },
        runAndWait: async (file, args, options) => {
          trace.push([file, ...args]);
          if (args[0] === 'wasm-local') assert.equal(options.env.PUSH_CACHE, '', 'Local WASM preparation must suppress inherited Docker publishing');
          if (file === './gradlew') for (const [key, path] of Object.entries(outputs)) put(path, `${key}/${args.find((arg) => arg.startsWith('-PMETA_SECRET_SOCKET_URL='))}`);
          else if (file === 'xcodebuild') for (const name of ['iosApp.app', 'iosAppUITests-Runner.app']) put(resolve(derivedDataPath, 'Build/Products/Debug-iphonesimulator', name, 'Info.plist'), 'fixture app tree');
          else if (args[0] === 'wasm-local') put(resolve(wasmPath, 'module.wasm'), 'fixture wasm');
          await Promise.resolve(); return { exitCode: 0 };
        },
      },
    };
    const graph = createBatchProductPreparation(options);
    const indexPath = await graph.prepare(); const before = structuredClone(trace);
    assert.equal(await graph.prepare(), indexPath); assert.deepEqual(trace, before);
    assert.equal(nativeCalls, 1); assert.equal(iosRecorded, 1);
    assert.equal(trace.filter(([file]) => file === './gradlew').length, 3);
    assert.equal(trace.filter(([file]) => file === 'xcodebuild').length, 1);
    assert(trace.find(([file]) => file === 'xcodebuild').includes('METASECRET_E2E_IDENTITY_REQUIRED=YES'));
    assert(!trace.flat().some((arg) => /installDebug|connectedDebug|simctl|set-env|only-testing/.test(arg)));
    const resumed = createBatchProductPreparation(options);
    assert.equal(resumed.hasPersistedProof, true); assert.equal(await resumed.prepare(), indexPath);
    assert.deepEqual(trace, before, 'Persisted proven tuples cannot compile again');
    const index = JSON.parse(readFileSync(indexPath, 'utf8')); assert.equal(index.sealed, true); assert.equal(index.receipts.length, 6);
    assert.equal(index.receipts.find(value => value.tuple.product === 'ios').tuple.configuration.identityRequired, 'YES');
    for (const receipt of index.receipts) assert.doesNotThrow(() => verifyArtifactProvenance({ path: receipt.manifestPath, current: receipt.current }));
    writeFileSync(index.receipts[0].products.app, 'mutated persisted pair');
    const unsafeResume = createBatchProductPreparation(options);
    await assert.rejects(unsafeResume.prepare(), /proof|provenance|mismatch/i);
    assert.deepEqual(trace, before, 'Unprovable prepared tuples fail closed instead of rebuilding');
  } finally { f.cleanup(); }
});

test('F2 source authority: Android source hashing retains the existing skip-deleted algorithm', async () => {
  const f = handoffFixture();
  try {
    const present = 'meta-secret/present.rs'; mkdirSync(resolve(f.directory, 'meta-secret'));
    writeFileSync(resolve(f.directory, present), 'current source');
    const actual = await hashWorkingSources({ root: f.directory, scopes: ['meta-secret'],
      runAndCapture: async (file, args) => { assert.equal(file, 'git'); assert.equal(args[0], 'ls-files'); return { stdout: `meta-secret/deleted.rs\0${present}\0` }; },
    });
    assert.equal(actual, createHash('sha256').update(`${present}\0`).update('current source').digest('hex'));
  } finally { f.cleanup(); }
});


for (const loss of ['deleted', 'mutated']) test(`F2 cached CLI admission rejects ${loss} exact product without rebuilding or resealing`, async () => {
  const f = handoffFixture(); const trace = [];
  const coreRoot = f.directory; const composeRoot = resolve(f.directory, 'compose');
  const cliPath = resolve(coreRoot, 'meta-secret/target/debug/meta-cli'); const wasmPath = resolve(f.directory, 'pkg');
  const put = (path, value) => { mkdirSync(resolve(path, '..'), { recursive: true }); writeFileSync(path, value); };
  try {
    const options = { paths: { coreRoot, composeRoot, runDirectory: f.directory }, runId: 'fixture-cli-proof',
      scenarios: [{ testNumber: 15, scenario: { web: {}, cli: { required: true }, server: { image: 'fixture-image' } } }],
      prepareNative: async () => { trace.push('native'); put(cliPath, 'fixture exact CLI'); return { android: false, ios: false, cli: true }; },
      commands: { checkBudget: () => ({ requiredBytes: 0 }),
        proofPorts: { wasmPath, captureInputs: f.options.captureInputs,
          inspectProducts: ({ paths }) => ({ hashes: Object.fromEntries(Object.entries(paths).map(([key, path]) => [key, hashArtifactTree(path)])) }) },
        runAndWait: async (file, args) => { trace.push([file, ...args]); assert.equal(file, '/opt/homebrew/bin/task');
          if (args[0] === 'wasm-local') put(resolve(wasmPath, 'module.wasm'), 'fixture wasm');
          else assert.equal(args[0], 'meta-server');
          return { exitCode: 0 }; },
        runAndCapture: async (file) => { trace.push(file); assert.equal(file, 'docker'); return { stdout: 'sha256:fixture-image\n' }; },
      },
    };
    const graph = createBatchProductPreparation(options); const indexPath = await graph.prepare();
    const indexBytes = readFileSync(indexPath); const built = structuredClone(trace);
    assert.equal(await graph.prepare(), indexPath, 'Exact unchanged CLI remains reusable');
    assert.deepEqual(trace, built);
    if (loss === 'deleted') rmSync(cliPath); else put(cliPath, 'foreign CLI bytes');
    await assert.rejects(graph.prepare(), /CLI.*proof.*(?:missing|changed|mismatch)/i,
      'Cached preparation must not turn a lost CLI into a PREBUILT admission');
    assert.deepEqual(trace, built, 'No hidden native build or CLI recovery is allowed after sealing');
    assert.deepEqual(readFileSync(indexPath), indexBytes, 'The original seal is immutable after proof loss');
    put(cliPath, 'fixture exact CLI');
    assert.equal(await graph.prepare(), indexPath, 'Only restoring the identical product restores the original proof');
    const resumed = createBatchProductPreparation(options);
    assert.equal(resumed.hasPersistedProof, true); assert.equal(await resumed.prepare(), indexPath);
    assert.deepEqual(trace, built); assert.deepEqual(readFileSync(indexPath), indexBytes);
  } finally { f.cleanup(); }
});

// Proposal fixture: real current 22 paths and real environment restore; external
// owning commands/inspection remain the existing closed-port fixture boundary.
async function restoredBatchFixture(t, { resourceBudget, onCommand } = {}) {
  const f = handoffFixture(), persistent = createEnvironmentTestRun(), runDirectory = persistent.runDirectory;
  const guards = [];
  t.after(async () => { for (const value of guards) await value.restore(); f.cleanup(); persistent.cleanup(); });
  const coreRoot = f.directory, composeRoot = resolve(f.directory, 'compose');
  const paths = batchEnvironmentPaths(coreRoot, composeRoot);
  assert.equal(paths.configs.length + paths.artifacts.length, 22);
  const put = (path, value) => { mkdirSync(resolve(path, '..'), { recursive: true }); writeFileSync(path, value); };
  const cli = resolve(coreRoot, 'meta-secret/target/debug/meta-cli');
  const derivedDataPath = resolve(coreRoot, 'e2eTest/.derivedData/iosApp');
  const wasmPath = resolve(coreRoot, 'meta-secret/web-cli/ui/pkg');
  const select = mode => paths.configs.forEach((path, index) => put(path, index === 0
    ? 'pub const SELECTED_SERVER_ENVIRONMENT: ServerEnvironment = ServerEnvironment::' + (mode === 'local' ? 'Local' : 'Remote') + ';\n'
    : (index === 1 ? 'VITE_META_SECRET_ENV=' : 'META_SECRET_ENV=') + mode + '\n'));
  select('remote'); put(cli, 'original user CLI'); chmodSync(cli, 0o700);
  const baseline = () => [...paths.configs, ...paths.artifacts].map(environmentTreeHash);
  const original = baseline(); const builders = []; let nativeCalls = 0;
  const supportPaths = [
    resolve(coreRoot, 'e2eTest/.artifacts/android-ffi-build-provenance.json'),
    resolve(coreRoot, 'e2eTest/.artifacts/ios-ffi-build-provenance.json'),
    ...target.abis.map(abi => resolve(composeRoot, 'composeApp/src/androidMain/jniLibs', abi, 'libmetasecret_mobile.so')),
    resolve(composeRoot, 'composeApp/src/androidMain/kotlin/com/metasecret/core/uniffi/bindings.kt'),
    resolve(composeRoot, 'iosApp/Libs/libmetasecret_mobile-ios-simulator-arm64.a'),
    resolve(composeRoot, 'iosApp/Libs/libmetasecret_mobile-ios-arm64.a'),
    resolve(composeRoot, 'iosApp/iosApp/UniffiGenerated/bindings.swift'),
    resolve(composeRoot, 'iosApp/iosApp/MetaSecretCoreService/UniffiGenerated/bindings.swift'),
  ];
  const guard = () => {
    const value = createBatchEnvironmentGuard({ ...paths, runDirectory,
      preflight: async () => {}, setLocal: async () => select('local'),
      snapshotBudget: (list, options) => assertSnapshotDiskBudget(list, { ...options, freeBytes: 64 * 1024 ** 3 }),
    });
    guards.push(value); return value;
  };
  const scenarios = [1, 16, 50].map(testNumber => ({ testNumber, scenario: {
    android: {}, ios: {}, web: {}, cli: { required: true }, server: { image: 'fixture-image' },
    ...(testNumber === 1 ? {} : { networkLoss: { androidProxyPort: testNumber === 16 ? 5181 : 5201 } }),
  } }));
  const proofPorts = { derivedDataPath, wasmPath, captureInputs: f.options.captureInputs,
    inspectProducts: ({ tuple, paths: products }) => {
      if (tuple.product === 'wasm') return createProductProofPorts({ coreRoot, composeRoot,
        runAndCapture: async () => assert.fail('WASM inspection needs no host command'),
      }).inspectProducts({ tuple, paths: products });
      if (tuple.product === 'android') return f.options.inspectProducts({ tuple, paths: products });
      return { hashes: Object.fromEntries(Object.entries(products).map(([key, path]) => [key, hashArtifactTree(path)])) };
    } };
  const options = { paths: { coreRoot, composeRoot, runDirectory }, runId: 'same-run-full-restore', scenarios, resourceBudget,
    prepareNative: async () => {
      nativeCalls++; put(cli, 'owning F2 CLI'); chmodSync(cli, 0o755);
      for (const path of supportPaths) put(path, 'closed owning native output/' + path);
      for (const [target, label] of [['aarch64-apple-ios', 'ios-arm64'], ['aarch64-apple-ios-sim', 'ios-simulator-arm64']])
        put(resolve(coreRoot, 'meta-secret/target', target, 'release/libmetasecret_mobile.a'),
          readFileSync(resolve(composeRoot, 'iosApp/Libs/libmetasecret_mobile-' + label + '.a')));
      return { android: true, ios: true, cli: true };
    },
    commands: { checkBudget: () => ({ requiredBytes: 0 }), proofPorts,
      iosBuild: { prepare: async options => { assert.equal(options.identityRequired, true); return { checkedLink: true }; },
        record: () => {
          put(resolve(derivedDataPath, 'Build/Products/owning.xctestrun'), 'closed owning generated template');
          put(resolve(derivedDataPath, 'ios-core-link-provenance.json'), 'closed owning link manifest');
        } },
      runAndCapture: async (file, args) => { assert.equal(file, 'docker'); return { stdout: 'sha256:fixture-image\n' }; },
      runAndWait: async (file, args) => {
        builders.push([file, ...args]);
        if (file === './gradlew') {
          const configuration = configurations.find(value => args.includes('-PMETA_SECRET_SOCKET_URL=' + value.socketUrl));
          assert(configuration);
          const app = { configuration, target, abis: Object.fromEntries(target.abis.map(abi => [abi, 'current-ffi'])), source: f.state.source };
          put(resolve(composeRoot, 'composeApp/build/outputs/apk/debug/composeApp-debug.apk'), JSON.stringify(app));
          put(resolve(composeRoot, 'composeApp/build/outputs/apk/androidTest/debug/composeApp-debug-androidTest.apk'),
            JSON.stringify({ configuration, source: f.state.source, package: target.package + '.test' }));
        } else if (file === 'xcodebuild') {
          for (const name of ['iosApp.app', 'iosAppUITests-Runner.app'])
            put(resolve(derivedDataPath, 'Build/Products/Debug-iphonesimulator', name, 'Info.plist'), 'closed owning app tree');
        } else {
          assert.equal(file, '/opt/homebrew/bin/task');
          if (args[0] === 'wasm-local') put(resolve(wasmPath, 'module.wasm'), 'owning wasm bytes');
          else assert.equal(args[0], 'meta-server');
        }
        onCommand?.({ file, args });
        return { exitCode: 0 };
      },
    },
  };
  const firstGuard = guard(); await firstGuard.prepare();
  let indexPath, indexBytes, owned, nativeBuilt, built;
  try {
    const first = createBatchProductPreparation({ ...options, environmentGuard: firstGuard }); indexPath = await first.prepare();
    indexBytes = readFileSync(indexPath); owned = baseline(); nativeBuilt = nativeCalls; built = structuredClone(builders);
    assert.equal(JSON.parse(indexBytes).receipts.length, 6);
  } finally { await firstGuard.restore(); }
  assert.deepEqual(baseline(), original);
  assert.equal(readFileSync(cli, 'utf8'), 'original user CLI'); assert.equal(statSync(cli).mode & 0o777, 0o700);
  return { f, runDirectory, paths, options, guard, cli, indexPath, indexBytes, baseline, original, owned, built, builders,
    nativeCalls: () => nativeCalls, nativeBuilt, proofPorts };
}

test('actual preparation waits for server postbuild RAM then stashes supports and seals once', async t => {
  const budget = planPreparationBudget({ snapshotBytes: 0, androidConfigurationCount: 3,
    androidConfigurationKeys: ['defaultLocal', 'network16', 'deletion50'], iosRequired: true, wasmRequired: true, cliRequired: true });
  let time = 0, low = false, waited = 0, serverCalls = 0;
  const resourceBudget = createPreparationResourceBudget({ budget,
    observe: async () => ({ freeBytes: 1024 * GiB, physicalBytes: 16 * GiB,
      availableBytes: low ? 0 : 3 * GiB, swapUsedBytes: 0, swapTotalBytes: 0 }),
    recovery: { now: () => time, wait: async ms => { time += ms; waited++; low = false; } },
  });
  resourceBudget.complete('snapshot');
  const f = await restoredBatchFixture(t, { resourceBudget,
    onCommand: ({ args }) => { if (args[0] === 'meta-server') { serverCalls++; low = true; } } });
  assert.equal(serverCalls, 1); assert.equal(waited, 1);
  const index = JSON.parse(f.indexBytes);
  assert.equal(index.sealed, true); assert.equal(index.receipts.length, 6);
  assert.equal(index.receipts.filter(row => row.tuple.product === 'server').length, 1);
  assert(existsSync(resolve(f.runDirectory, 'supporting-products/manifest.json')));
  assert.equal(resourceBudget.snapshot().sealed, true);
});

test('same-run sealed supporting bytes survive exact restore before zero-build replay', async t => {
  const f = await restoredBatchFixture(t), next = f.guard();
  await next.prepare();
  try {
    const resumed = createBatchProductPreparation({ ...f.options, environmentGuard: next });
    assert.equal(await resumed.prepare(), f.indexPath);
    assert.deepEqual(f.baseline(), f.owned, 'Replay must restore the exact owning outputs, including CLI mode');
    const index = JSON.parse(readFileSync(f.indexPath, 'utf8'));
    for (const receipt of index.receipts) await verifyPreparedProduct({ receipt, captureInputs: f.proofPorts.captureInputs,
      inspectProducts: f.proofPorts.inspectProducts });
    assert.deepEqual(readFileSync(f.indexPath), f.indexBytes, 'Original seal is not rewritten');
    assert.deepEqual(f.builders, f.built); assert.equal(f.nativeCalls(), f.nativeBuilt);
  } finally { await next.restore(); assert.deepEqual(f.baseline(), f.original); }
});



// R32: whole actual preparation graph + actual resource ledger; no compiler/Docker invoked.
function postWasmAdmissionFixture(t, mode = 'recover') {
  const f = handoffFixture(); t.after(f.cleanup);
  const calls = [], samples = [], waits = []; let phase = '', now = 0, serverSamples = 0;
  const stop = new AbortController(); const failure = new Error('Controlled admission probe failure');
  let active = true;
  const budget = planPreparationBudget({ snapshotBytes: 0, androidConfigurationCount: 0,
    androidConfigurationKeys: [], iosRequired: false, wasmRequired: true, cliRequired: false });
  const good = { freeBytes: budget.minimumFreeBytes + 32 * GiB, physicalBytes: 16 * GiB,
    availableBytes: 4 * GiB, swapUsedBytes: 0, swapTotalBytes: GiB };
  const ledger = createPreparationResourceBudget({ budget, observe: async () => {
    if (phase !== 'server-build') {
      const sample = { ...good, availableBytes: mode === 'fresh' && phase === 'wasm-build' ? 2 * GiB : good.availableBytes };
      samples.push({ phase, ...sample }); return sample;
    }
    serverSamples++;
    if (mode === 'probe') throw failure;
    const sample = { ...good, availableBytes: mode === 'recover' && serverSamples > 2 ? 3 * GiB : 2 * GiB };
    if (mode === 'disk') sample.freeBytes = 0;
    if (mode === 'physical') sample.physicalBytes = 15 * GiB;
    if (mode === 'counter') sample.swapUsedBytes = -1;
    samples.push({ phase, ...sample }); return sample;
  }, recovery: { now: () => now, wait: async ms => {
    assert.equal(calls.filter(value => value === 'meta-server').length, 0, 'No producer while recovering');
    assert(ms > 0 && ms <= 5000); waits.push(ms); now += ms;
    if (mode === 'cancel') stop.abort(new Error('Controlled admission canceled'));
    if (mode === 'guard') active = false;
  } } });
  ledger.complete('snapshot');
  const resourceBudget = { complete: (...args) => ledger.complete(...args), check: options => ledger.check({
    ...options, signal: stop.signal, assertActive: () => { options.assertActive(); assert(active, 'Controlled guard revoked'); },
  }) };
  const put = (path, data) => { mkdirSync(resolve(path, '..'), { recursive: true }); writeFileSync(path, data); };
  const wasmPath = resolve(f.directory, 'wasm');
  const graph = createBatchProductPreparation({ paths: { coreRoot: f.directory, composeRoot: resolve(f.directory, 'compose'), runDirectory: f.directory },
    runId: 'post-wasm-admission', resourceBudget,
    preparationMetrics: { measure: async (name, operation) => { phase = name; return operation(); } },
    scenarios: [{ testNumber: 16, scenario: { web: {}, server: { image: 'fixture-image' } } }],
    prepareNative: async () => ({ android: false, ios: false, cli: false }),
    commands: { checkBudget: () => ({ requiredBytes: 0 }), proofPorts: { wasmPath,
      captureInputs: f.options.captureInputs,
      inspectProducts: ({ paths }) => ({ hashes: Object.fromEntries(Object.entries(paths).map(([key, path]) => [key, hashArtifactTree(path)])) }),
    }, runAndCapture: async () => ({ stdout: 'sha256:fixture-image\n' }),
    runAndWait: async (_file, args) => {
      if (args[0] === 'meta-server') assert(samples.at(-1).availableBytes >= 3 * GiB);
      calls.push(args[0]); if (args[0] === 'wasm-local') put(resolve(wasmPath, 'module.wasm'), 'fixture wasm');
      return { exitCode: 0 };
    } },
  });
  return { graph, calls, samples, waits, failure, elapsed: () => now };
}

test('R32 actual completed WASM server admission recovers a late RAM dip before exactly one producer', async t => {
  const f = postWasmAdmissionFixture(t);
  await f.graph.prepare();
  assert.deepEqual(f.calls, ['wasm-local', 'meta-server']);
  assert(f.samples.some(sample => sample.phase !== 'server-build' && sample.availableBytes >= 3 * GiB));
  assert.deepEqual(f.samples.filter(sample => sample.phase === 'server-build').slice(0, 3).map(sample => sample.availableBytes), [2 * GiB, 2 * GiB, 3 * GiB]);
  assert.equal(f.waits.length, 2); assert.equal(f.elapsed(), 10000);
});

for (const mode of ['disk', 'physical', 'counter', 'probe', 'guard', 'cancel', 'deadline']) {
  test(`R32 actual server admission refuses ${mode} without invoking producer`, async t => {
    const f = postWasmAdmissionFixture(t, mode);
    await assert.rejects(f.graph.prepare(), mode === 'probe' ? error => error === f.failure : undefined);
    assert.deepEqual(f.calls, ['wasm-local']);
    if (['disk', 'physical', 'counter', 'probe'].includes(mode)) assert.equal(f.waits.length, 0);
    else if (mode === 'deadline') assert.equal(f.elapsed(), 180000);
    else assert.equal(f.waits.length, 1);
  });
}


test('R32 fresh precompiler low RAM remains failfast with no recovery or producer', async t => {
  const f = postWasmAdmissionFixture(t, 'fresh');
  await assert.rejects(f.graph.prepare(), /RAM headroom/);
  assert.deepEqual(f.calls, []); assert.equal(f.waits.length, 0);
});

// G38: actual initial preparation caller + unchanged actual resource recovery loop.
function initialF2AdmissionFixture(t, mode = 'recover') {
  const f = handoffFixture(); t.after(f.cleanup);
  const budget = planPreparationBudget({ snapshotBytes: 0, androidConfigurationCount: 0,
    androidConfigurationKeys: [], iosRequired: false, wasmRequired: true, cliRequired: false });
  let now = 0, observed = 0, nativeCalls = 0, active = true;
  const samples = [], waits = [];
  const entered = new Error('Controlled native boundary reached');
  const ledger = createPreparationResourceBudget({ budget, observe: async () => {
    observed++;
    const sample = { freeBytes: budget.minimumFreeBytes + GiB, physicalBytes: 16 * GiB,
      availableBytes: mode === 'healthy' || mode === 'recover' && observed >= 3 ? 3 * GiB : 2 * GiB,
      swapUsedBytes: 0, swapTotalBytes: GiB };
    if (mode === 'disk') sample.freeBytes = budget.minimumFreeBytes - 1;
    samples.push(sample); return sample;
  }, recovery: { now: () => now, wait: async ms => {
    assert.equal(nativeCalls, 0); assert(ms > 0 && ms <= 5000);
    waits.push(ms); now += ms;
    if (mode === 'guard') active = false;
  } } });
  ledger.complete('snapshot');
  const graph = createBatchProductPreparation({
    paths: { coreRoot: f.directory, composeRoot: resolve(f.directory, 'compose'), runDirectory: f.directory },
    runId: 'initial-f2-admission', scenarios: [{ testNumber: 16, scenario: { web: {}, server: { image: 'fixture' } } }],
    resourceBudget: { complete: (...args) => ledger.complete(...args), check: options => ledger.check({ ...options,
      assertActive: () => { options.assertActive(); assert(active, 'Controlled guard revoked'); } }) },
    prepareNative: async () => { nativeCalls++; assert(samples.at(-1).availableBytes >= 3 * GiB); throw entered; },
    commands: { proofPorts: { captureInputs: f.options.captureInputs },
      runAndWait: async () => { assert.fail('No compiler belongs in this initial admission bank'); },
      runAndCapture: async () => { assert.fail('No product inspection before native boundary'); } },
  });
  return { graph, samples, waits, entered, nativeCalls: () => nativeCalls, elapsed: () => now,
    observations: () => observed };
}

test('G38 actual initial F2 healthy control then RAM recovery admits exactly one native boundary', async t => {
  const healthy = initialF2AdmissionFixture(t, 'healthy');
  await assert.rejects(healthy.graph.prepare(), error => error === healthy.entered);
  assert.equal(healthy.nativeCalls(), 1); assert.equal(healthy.waits.length, 0);
  const f = initialF2AdmissionFixture(t);
  await assert.rejects(f.graph.prepare(), error => error === f.entered);
  assert.deepEqual(f.samples.map(s => s.availableBytes), [2 * GiB, 2 * GiB, 3 * GiB]);
  assert.equal(f.nativeCalls(), 1); assert.equal(f.waits.length, 2); assert.equal(f.elapsed(), 10000);
  await assert.rejects(f.graph.prepare(), error => error === f.entered);
  assert.equal(f.nativeCalls(), 1, 'Failed preparation cannot restart the native producer');
});
test('G38 actual initial F2 disk failure with low RAM never waits or invokes native', async t => {
  const f = initialF2AdmissionFixture(t, 'disk');
  await assert.rejects(f.graph.prepare(), /disk|reservation|floor/i);
  assert.equal(f.nativeCalls(), 0); assert.equal(f.waits.length, 0);
});
test('G38 actual initial F2 guard loss during RAM wait cannot admit native or restart', async t => {
  const f = initialF2AdmissionFixture(t, 'guard');
  await assert.rejects(f.graph.prepare(), /Controlled guard revoked/);
  assert.equal(f.nativeCalls(), 0); assert.equal(f.waits.length, 1);
  await assert.rejects(f.graph.prepare()); assert.equal(f.nativeCalls(), 0);
});

// G42: actual whole graph, handoff validation, ledger and real private filesystem.
async function serverWorkingFixture(t, mode = 'recover') {
  const f = handoffFixture(), persistent = createEnvironmentTestRun(), runDirectory = persistent.runDirectory;
  const coreRoot = f.directory, composeRoot = resolve(f.directory, 'compose');
  const paths = batchEnvironmentPaths(coreRoot, composeRoot), events = [], calls = [];
  const put = (path, bytes) => { mkdirSync(resolve(path, '..'), { recursive: true }); writeFileSync(path, bytes); };
  paths.configs.forEach((path, i) => put(path, i === 0
    ? 'pub const SELECTED_SERVER_ENVIRONMENT: ServerEnvironment = ServerEnvironment::Local;\n'
    : (i === 1 ? 'VITE_META_SECRET_ENV=' : 'META_SECRET_ENV=') + 'local\n'));
  const guard = createBatchEnvironmentGuard({ ...paths, runDirectory, preflight: async () => {}, setLocal: async () => assert.fail('Already local'),
    snapshotBudget: (list, options) => assertSnapshotDiskBudget(list, { ...options, freeBytes: 64 * GiB }) });
  await guard.prepare(); t.after(async () => { await guard.restore(); f.cleanup(); persistent.cleanup(); });
  const budget = planPreparationBudget({ snapshotBytes: 0, androidConfigurationCount: 0, androidConfigurationKeys: [],
    iosRequired: false, wasmRequired: true, cliRequired: false });
  let freeBytes = budget.minimumFreeBytes + 32 * GiB, serverDone = false, inspections = 0, nativeCalls = 0;
  const phase = [];
  const ledger = createPreparationResourceBudget({ budget, observe: async () => ({ freeBytes,
    physicalBytes: 16 * GiB, availableBytes: 4 * GiB, swapUsedBytes: 0, swapTotalBytes: GiB }) });
  ledger.complete('snapshot');
  const resourceBudget = { check: options => ledger.check(options), complete: (...args) => {
    events.push('complete:' + args[0]); phase.push(args[0]); ledger.complete(...args);
  } };
  const wasmPath = resolve(coreRoot, 'meta-secret/web-cli/ui/pkg');
  const serverTuple = { product: 'server', configuration: { environment: 'local' }, target: { platform: 'docker', image: 'fixture-image' } };
  const productDir = resolve(runDirectory, 'products', createHash('sha256').update(JSON.stringify(serverTuple)).digest('hex'));
  const indexPath = resolve(runDirectory, 'batch-products.json');
  const graph = createBatchProductPreparation({ paths: { coreRoot, composeRoot, runDirectory },
    runId: 'server-working-boundary', environmentGuard: guard, resourceBudget,
    scenarios: [{ testNumber: 9, scenario: { web: {}, server: { image: 'fixture-image' } } }],
    prepareNative: async () => { nativeCalls++; return { android: false, ios: false, cli: false }; },
    commands: {
      proofPorts: { wasmPath,
        captureInputs: async tuple => {
          const result = await f.options.captureInputs(tuple);
          return mode === 'input' && tuple.product === 'server' && serverDone ? { ...result, sourceFingerprint: 'changed' } : result;
        },
        captureReplayInputs: f.options.captureInputs,
        inspectProducts: async ({ tuple, paths: products }) => {
          if (tuple.product === 'server') {
            inspections++; events.push('server-inspect:' + inspections);
            if (mode === 'inspect') throw Error('Controlled product inspection failed');
            if (inspections === 1) {
              assert.equal(phase.includes('serverWorkingComplete'), false);
              const receipts = readdirSync(resolve(runDirectory, 'owning-builds')).filter(name => /^build-\d+\.json$/.test(name));
              assert(receipts.some(name => JSON.parse(readFileSync(resolve(runDirectory, 'owning-builds', name))).request.product === 'server'));
              const required = assertPreparationResources({ budget, completed: ledger.snapshot(),
                observation: { freeBytes: Number.MAX_SAFE_INTEGER, physicalBytes: 16 * GiB, availableBytes: 4 * GiB, swapUsedBytes: 0, swapTotalBytes: GiB } }).requiredBytes;
              if (mode !== 'healthy') freeBytes = required + budget.minimumRemainingBytes - (mode === 'floor' ? budget.components.serverWorking + 1 : GiB);
              if (mode === 'guard') await guard.restore();
            }
          }
          const hashes = Object.fromEntries(Object.entries(products).map(([key, path]) => [key, hashArtifactTree(path)]));
          if (mode === 'stash-mismatch' && tuple.product === 'server' && inspections === 2) hashes.server = 'mismatch';
          return { hashes };
        } },
      checkBudget: list => { events.push('copy-budget'); if (mode === 'copy-budget' && serverDone) throw Error('Controlled copy budget failed');
        return assertSnapshotDiskBudget(list, { freeBytes: 64 * GiB }); },
      runAndCapture: async (file, args) => { assert.equal(file, 'docker'); events.push('image-inspect');
        if (mode === 'image-throw') throw Error('Controlled image inspect failed');
        return { stdout: mode === 'image-empty' ? '' : 'sha256:fixture-image\n' }; },
      runAndWait: async (_file, args) => {
        calls.push(args[0]); events.push('command:' + args[0]);
        if (args[0] === 'wasm-local') put(resolve(wasmPath, 'module.wasm'), 'actual controlled wasm');
        if (args[0] === 'meta-server') { serverDone = true; if (mode === 'producer') return { exitCode: 1 }; }
        return { exitCode: 0 };
      },
    },
  });
  return { graph, ledger, events, calls, phase, indexPath, productDir, nativeCalls: () => nativeCalls };
}

test('G42 actual server before-stash releases completed compiler working only after validation', async t => {
  const f = await serverWorkingFixture(t);
  const index = JSON.parse(readFileSync(await f.graph.prepare()));
  assert.equal(index.sealed, true); assert.equal(index.receipts.length, 2);
  assert.equal(f.nativeCalls(), 1); assert.deepEqual(f.calls, ['wasm-local', 'meta-server']);
  assert(f.events.indexOf('server-inspect:1') < f.events.indexOf('complete:serverWorkingComplete'));
  assert(f.events.indexOf('complete:serverWorkingComplete') < f.events.indexOf('server-inspect:2'));
  assert(f.events.indexOf('server-inspect:2') < f.events.indexOf('complete:server'));
  await f.graph.prepare(); assert.equal(f.nativeCalls(), 1); assert.equal(f.calls.length, 2);
});
test('G42 actual server healthy control keeps original builders and prepared stage', async t => {
  const f = await serverWorkingFixture(t, 'healthy'); await f.graph.prepare();
  assert.equal(f.nativeCalls(), 1); assert.deepEqual(f.calls, ['wasm-local', 'meta-server']);
  assert.equal(f.ledger.snapshot().server, true); assert.equal(f.ledger.snapshot().sealed, true);
});
for (const mode of ['producer', 'image-empty', 'image-throw', 'input', 'inspect', 'guard']) {
  test(`G42 actual server ${mode} cannot mark compiler working completion or copy`, async t => {
    const f = await serverWorkingFixture(t, mode);
    await assert.rejects(f.graph.prepare());
    assert.equal(f.phase.includes('serverWorkingComplete'), false);
    assert.equal(existsSync(f.productDir), false); assert.equal(existsSync(f.indexPath), false);
    assert.equal(f.ledger.snapshot().server, undefined);
  });
}
for (const mode of ['floor', 'copy-budget', 'stash-mismatch']) {
  test(`G42 actual server ${mode} after working phase cannot prepare or seal`, async t => {
    const f = await serverWorkingFixture(t, mode); await assert.rejects(f.graph.prepare());
    assert.equal(f.ledger.snapshot().serverWorkingComplete, true);
    assert.equal(f.ledger.snapshot().server, undefined); assert.equal(f.ledger.snapshot().sealed, undefined);
    assert.equal(existsSync(f.indexPath), false);
    if (mode !== 'stash-mismatch') assert.equal(existsSync(f.productDir), false);
    const commands = [...f.calls]; await assert.rejects(f.graph.prepare()); assert.deepEqual(f.calls, commands);
  });
}
test('G42 actual handoff rejects an invalid builder receipt before the before-stash boundary', async t => {
  const f = handoffFixture(); t.after(f.cleanup); let boundaryCalls = 0;
  const bad = resolve(f.directory, 'invalid-builder.json');
  writeFileSync(bad, JSON.stringify({ completed: false, exitCode: 0 }));
  const handoff = createBatchBuildHandoff({ ...f.options,
    builders: { ...f.options.builders, wasm: async () => ({ exitCode: 0, receiptPath: bad, products: {} }) },
    beforeStash: async () => { boundaryCalls++; } });
  await assert.rejects(handoff.prepare({ product: 'wasm', configuration: {}, target: {} }), /receipt|incomplete|failed/i);
  assert.equal(boundaryCalls, 0); assert.deepEqual(handoff.entries(), []);
});
