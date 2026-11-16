import assert from 'node:assert/strict';
import test from 'node:test';
import * as fs from 'node:fs';
import { createHash, randomUUID } from 'node:crypto';
import * as path from 'node:path';
import { tmpdir } from 'node:os';
import { runInNewContext } from 'node:vm';
import { hashArtifactTree } from './artifact-tree.mjs';
import { createBatchBuildHandoff, verifyPreparedProduct, enumerateAndroidConfigurations, androidConfigurationForScenario } from './batch-build-handoff.mjs';
import { verifyArtifactProvenance } from './build-provenance-manifest.mjs';
import { batchEnvironmentPaths, createBatchEnvironmentGuard, environmentTreeHash, assertActiveBatchEnvironmentGuard, assertSnapshotDiskBudget } from './batch-environment.mjs';
import { E2E_BUILD_PROFILE, e2eBuildEnvironment, e2eGradleArguments } from './e2e-resource-budget.mjs';

import {createEnvironmentTestRun} from './environment-test-fixture.mjs';

const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const leafName = 'composeApp/build/intermediates/stripped_native_libs/debug/stripDebugDebugSymbols/out/lib/arm64-v8a/libmetasecret_mobile.so';
const id = 'android-stripped-arm64';
function actualModule(name, bindings = {}) {
  const source = fs.readFileSync(new URL(name, import.meta.url), 'utf8');
  const exports = [...source.matchAll(/^export (?:async )?(?:function|const) (\w+)/gm)].map(match => match[1]);
  const body = source.replace(/^export \{[^\n]*\};?\n/gm, '').replace(/^import[\s\S]*?;\n/gm, '').replace(/^export /gm, '');
  const extra = name === './batch-product-proof.mjs' ? ', raw: typeof strippedFileSha256 === "function" ? strippedFileSha256 : null, parity: typeof assertAndroidStrippedSupport === "function" ? assertAndroidStrippedSupport : null' : '';
  return runInNewContext(body + '\n({' + exports.join(',') + extra + '})', {
    ...fs, ...path, createHash, randomUUID, Buffer, structuredClone, process, ...bindings,
  }, { timeout: 1000 });
}
function actualFunction(source, name) {
  const begin = source.indexOf('async function ' + name + '(');
  assert(begin >= 0); const end = source.indexOf('\n}', begin); assert(end > begin);
  return source.slice(begin, end + 2);
}
async function fixture(t, options = {}) {
  const persistent = createEnvironmentTestRun();
  const root = fs.realpathSync(fs.mkdtempSync(path.join(tmpdir(), 'stripped-support-')));
  const coreRoot = path.join(root, 'core'), composeRoot = path.join(root, 'compose'), runDirectory = persistent.runDirectory;
  const paths = { coreRoot, composeRoot, runDirectory }, runId = 'closed-stripped-support';
  const put = (name, bytes = 'closed owning bytes') => { fs.mkdirSync(path.dirname(name), { recursive: true }); fs.writeFileSync(name, bytes); return name; };
  const trace = [], budgets = [], guards = [], mutations = { copy: null };
  const processPort = { pid: process.pid, ppid: process.ppid, execPath: process.execPath, version: process.version, stdout: { write() {} }, env: { E2E_RUN_ID: runId } };
  const coreNames = ['Taskfile.yml', 'meta-secret/core/src/node/app/sync/environment.rs', 'meta-secret/mobile/scripts/build-android.sh'];
  const composeNames = ['settings.gradle.kts', 'gradle.properties', 'iosApp/Configuration/Config.xcconfig'];
  const inventory = (args, cwd) => {
    const scopes = args.slice(args.indexOf('--') + 1);
    const names = cwd === coreRoot ? [...coreNames, ...runnerNames] : composeNames;
    return names.filter(name => scopes.some(scope => name === scope || name.startsWith(scope + '/'))).sort().join('\0') + '\0';
  };
  const versions = { cargo: 'closed cargo', rustc: 'closed rustc', java: 'closed java' };
  const sync = (file, args, settings = {}) => {
    if (file === 'git') return inventory(Array.from(args), settings.cwd);
    if (versions[file]) return versions[file];
    if (file === 'unzip') {
      const apk = JSON.parse(fs.readFileSync(args[1], 'utf8'));
      return args[0] === '-Z1' ? Object.keys(apk.abis).map(abi => 'lib/' + abi + '/libmetasecret_mobile.so').join('\n') : Buffer.from(apk.abis[args[2].split('/')[1]]);
    }
    assert.fail('No external synchronous tool permitted: ' + file);
  };
  const identity = actualModule('./batch-replay-identity.mjs', { process: processPort, execFileSync: sync });
  const runnerNames = [...new Set([...identity.RUNNER_RECIPE_PATHS, ...identity.RUNNER_NONCOMPILED_PATHS,
    'shared/batch-stripped-support.test.mjs'])].map(name => 'e2eTest/' + name);
  for (const name of [...coreNames, ...runnerNames]) put(path.join(coreRoot, name), name);
  for (const name of composeNames) put(path.join(composeRoot, name), name);
  const targets = batchEnvironmentPaths(coreRoot, composeRoot);
  targets.configs.forEach((name, index) => put(name, index === 0
    ? 'const SELECTED_SERVER_ENVIRONMENT: ServerEnvironment = ServerEnvironment::Local;\n'
    : (index === 1 ? 'VITE_META_SECRET_ENV=' : 'META_SECRET_ENV=') + 'local\n'));
  const leaf = put(path.join(composeRoot, leafName), 'original stale stripped'); fs.chmodSync(leaf, 0o640);
  const original = [...targets.configs, ...targets.artifacts].map(environmentTreeHash);
  const guard = () => {
    const value = createBatchEnvironmentGuard({ ...targets, runDirectory, preflight: async () => {}, setLocal: async () => assert.fail('Already local'),
      snapshotBudget: (names, options) => assertSnapshotDiskBudget(names, { ...options, freeBytes: 64 * 1024 ** 3 }) });
    guards.push(value); return value;
  };
  t.after(async () => { try { for (const value of guards) await value.restore(); } finally { fs.rmSync(root, { recursive: true, force: true }); persistent.cleanup(); } });
  const auditModule = actualModule('./runner-validation-audit.mjs', { ...identity, process: processPort });
  fs.mkdirSync(runDirectory, { recursive: true });
  fs.mkdirSync(path.join(coreRoot, 'e2eTest', '.artifacts'), { recursive: true });
  // Actual audit creation/validation with CLOSED successful command ports; never real F1 evidence.
  const runnerAudit = await auditModule.createRunnerAudit({ paths, runId, runAndCapture: async () => ({ exitCode: 0, signal: null, stdout: 'closed F1', stderr: '' }) });
  Object.assign(processPort.env, { E2E_BATCH_PRODUCTS_PATH: path.join(runDirectory, 'batch-products.json'),
    E2E_RUNNER_AUDIT_PATH: runnerAudit.path, E2E_RUNNER_AUDIT_SHA256: runnerAudit.sha256, E2E_PARENT_RUNNER_PID: String(process.pid) });
  const proof = actualModule('./batch-product-proof.mjs', { ...identity, process: processPort, execFileSync: sync,
    hashArtifactTree, verifyArtifactProvenance, batchEnvironmentPaths, assertActiveBatchEnvironmentGuard, assertSnapshotDiskBudget,
    requireIosIdentityBuild() { assert.fail('No iOS target'); }, hashIosTree() { assert.fail('No iOS target'); },
    verifyIosFfi() { assert.fail('No iOS target'); }, verifyIosXcodeBuild() { assert.fail('No iOS target'); },
    cpSync(...args) { mutations.copy?.(...args); return fs.cpSync(...args); } });
  const capture = async (file, args, settings) => {
    if (file === 'git' || versions[file]) return { stdout: sync(file, args, settings) };
    assert.equal(file, 'docker'); return { stdout: 'sha256:closed-image' };
  };
  const ports = proof.createProductProofPorts({ coreRoot, composeRoot, runAndCapture: capture });
  const abis = ['arm64-v8a', 'armeabi-v7a', 'x86_64'];
  const execute = async (file, args) => {
    trace.push([file, ...args]);
    if (file === 'bash') {
      assert(args[0].endsWith('build-android.sh'));
      for (const abi of abis) put(path.join(composeRoot, 'composeApp/src/androidMain/jniLibs', abi, 'libmetasecret_mobile.so'), '[CORE_DIAG] sync_vault response events= closed/' + abi);
      put(path.join(composeRoot, 'composeApp/src/androidMain/kotlin/com/metasecret/core/uniffi/bindings.kt'));
    } else if (file === './gradlew') {
      const libs = Object.fromEntries(abis.map(abi => [abi, fs.readFileSync(path.join(composeRoot, 'composeApp/src/androidMain/jniLibs', abi, 'libmetasecret_mobile.so'), 'utf8')]));
      put(path.join(composeRoot, 'composeApp/build/outputs/apk/debug/composeApp-debug.apk'), JSON.stringify({ abis: libs, args }));
      put(path.join(composeRoot, 'composeApp/build/outputs/apk/androidTest/debug/composeApp-debug-androidTest.apk'));
      if (options.missing) fs.rmSync(leaf, { force: true });
      else { put(leaf, options.wrong ? 'wrong ABI bytes' : libs['arm64-v8a']); fs.chmodSync(leaf, 0o644); }
    } else {
      assert.equal(file, '/opt/homebrew/bin/task');
      if (args[0] === 'wasm-local') put(path.join(ports.wasmPath, 'module.wasm'));
      else assert.equal(args[0], 'meta-server');
    }
    return { exitCode: 0 };
  };
  const range = fs.readFileSync(new URL('../run-ui-e2e-range.mjs', import.meta.url), 'utf8');
  const constants = range.slice(range.indexOf('const androidFfiBuildScript ='), range.indexOf('const diskThresholdGiB ='));
  const producer = range.slice(range.indexOf('function fingerprintAndroidCoreSource('), range.indexOf('\nfunction parseOptions('));
  const native = runInNewContext(constants + '\n' + producer + '\n({prepareAndroidFfiBeforePlatforms})', {
    ...fs, ...path, createHash, Buffer, process: processPort, coreRoot, composeRoot, e2eRoot: path.join(coreRoot, 'e2eTest'),
    metaCliRoot: path.join(coreRoot, 'meta-secret'), metaCliBinaryPath: path.join(coreRoot, 'meta-secret/target/debug/meta-cli'),
    androidFfiSourcePrepared: undefined, metaCliSourcePrepared: undefined, runPreparationCommand: execute,
    spawnSync(file, args, settings) { assert.equal(file, 'git'); return { status: 0, stdout: inventory(args, settings.cwd) }; } });
  const factory = actualModule('./batch-product-builders.mjs', { ...proof, ...identity, process: processPort,
    createBatchBuildHandoff, verifyPreparedProduct, enumerateAndroidConfigurations, hashArtifactTree,
    batchEnvironmentPaths, assertActiveBatchEnvironmentGuard, assertSnapshotDiskBudget, E2E_BUILD_PROFILE, e2eBuildEnvironment, e2eGradleArguments,
    IOS_E2E_CONFIGURATION: {}, prepareIosXcodeBuild() { assert.fail('No iOS build'); }, recordIosXcodeBuild() { assert.fail('No iOS build'); } });
  const first = guard(); await first.prepare();
  const scenarios = (options.noAndroid ? [1] : [1, 16, 50]).map(testNumber => ({ testNumber,
    scenario: { web: {}, ...(!options.noAndroid ? { android: {} } : {}), server: { image: 'closed-image' },
      ...(testNumber === 1 ? {} : { networkLoss: { androidProxyPort: testNumber === 16 ? 5181 : 5201 } }) } }));
  const config = { paths, runId, newProofVersion: options.version ?? 5, runnerAudit, environmentGuard: first, scenarios,
    prepareNative: async () => ({ android: await native.prepareAndroidFfiBeforePlatforms({ scenarios: scenarios.map(row => ({ value: row.scenario })) }, { writeEvent() {}, writeOutput() {} }), ios: false, cli: false }),
    commands: { proofPorts: ports, runAndWait: execute, runAndCapture: capture,
      async checkBudget(names) { budgets.push([...names]); if (options.expireCapture && names.includes(leaf)) await first.restore(); return { requiredBytes: 0 }; } } };
  if (options.beforeCopy) mutations.copy = (from, to) => { if (String(to).endsWith('/' + id)) options.beforeCopy(from, to); };
  await factory.createBatchProductPreparation(config).prepare();
  const indexPath = processPort.env.E2E_BATCH_PRODUCTS_PATH, indexBytes = fs.readFileSync(indexPath), index = JSON.parse(indexBytes);
  const consumer = actualModule('./batch-product-consumer.mjs', { ...proof, ...identity, process: processPort, execFileSync: sync,
    hashArtifactTree, verifyPreparedProduct, verifyArtifactProvenance, androidConfigurationForScenario, IOS_E2E_CONFIGURATION: {},
    iosAppProductHashes() { assert.fail('No iOS target'); } });
  const select = () => consumer.loadBatchProducts({ coreRoot, composeRoot, scenario: scenarios[0].scenario, testNumber: 1, runAndCapture: capture });
  const tripleSource = fs.readFileSync(new URL('../Test4/orchestrator.mjs', import.meta.url), 'utf8');
  const triple = async () => {
    const selected = index.receipts.find(row => row.tuple.product === 'android');
    const guardFn = runInNewContext(actualFunction(tripleSource, 'verifyAndroidApkFfiProvenance') + '\nverifyAndroidApkFfiProvenance', {
      existsSync: fs.existsSync, readFileSync: fs.readFileSync, Buffer, androidAppApkPath: selected.products.app,
      androidArm64PackagedFfiPath: leaf, androidFfiBuildProvenancePath: path.join(coreRoot, 'e2eTest/.artifacts/android-ffi-build-provenance.json'),
      currentAndroidFfiSourceFingerprint: async () => selected.inputs.inputHashes.ffiSource, androidFfiSha256: digest,
      extractAndroidApkFfi: apk => sync('unzip', ['-p', apk, 'lib/arm64-v8a/libmetasecret_mobile.so']), writeDiagnostic() {} });
    return guardFn();
  };
  return { root, paths, leaf, first, guard, original, proof, index, indexPath, indexBytes, config, factory, ports,
    triple, select, trace, budgets, put, mutations, targets, snapshot: () => [...targets.configs, ...targets.artifacts].map(environmentTreeHash) };
}

test('causal RED: actual V5 replay restores generated arm64 bytes before the unchanged Test4 triple guard', async t => {
  const f = await fixture(t); assert.equal(f.index.version, 5); assert.equal(f.index.sealed, true);
  assert.equal(f.index.receipts.filter(row => row.tuple.product === 'android').length, 3); await f.triple();
  const prepared = f.snapshot(), builds = f.trace.length;
  await f.first.restore(); assert.deepEqual(f.snapshot(), f.original);
  const next = f.guard(); await next.prepare();
  try {
    await f.factory.createBatchProductPreparation({ ...f.config, environmentGuard: next }).prepare();
    const selected = await f.select();
    assert.equal(f.trace.length, builds, 'Replay runs no owning build or strip command');
    assert.deepEqual(fs.readFileSync(f.indexPath), f.indexBytes, 'No reseal/index rewrite');
    assert(selected.android); assert.equal(f.index.receipts.length, 5);
    await assert.doesNotReject(f.triple, 'Old helper leaves stale stripped bytes; original triple guard must now pass');
    assert.deepEqual(f.snapshot(), prepared); assert.equal(fs.statSync(f.leaf).mode & 0o777, 0o644);
  } finally { await next.restore(); assert.deepEqual(f.snapshot(), f.original); }
});

test('GREEN: capture budgets and stores one required leaf, all Android tuple parity is nonvacuous', async t => {
  const f = await fixture(t), bundle = f.proof.verifyStoredProductBundle(f.paths, f.index).bundle;
  const rows = bundle.entries.filter(row => row.id === id); assert.equal(rows.length, 1); assert.equal(rows[0].present, true);
  assert(f.budgets.some(names => names.includes(f.leaf)), 'Existing budget receives the actual extra leaf');
  assert.equal(typeof f.proof.parity, 'function');
  const empty = structuredClone(f.index); empty.receipts = empty.receipts.filter(row => row.tuple.product !== 'android');
  assert.throws(() => f.proof.parity(empty, f.leaf), /Nonempty/);
  const duplicate = structuredClone(f.index); duplicate.receipts.push(structuredClone(duplicate.receipts[0]));
  assert.throws(() => f.proof.parity(duplicate, f.leaf), /unique Android receipts/);
  for (const selected of [1, 2]) {
    const wrong = structuredClone(f.index), android = wrong.receipts.filter(row => row.tuple.product === 'android');
    android[selected].current.artifacts.products.packagedAbis['arm64-v8a'] = 'f'.repeat(64);
    android[selected].inputs.inputHashes.ffiAbis['arm64-v8a'] = 'f'.repeat(64);
    assert.throws(() => f.proof.parity(wrong, f.leaf), /owning Android tuple/);
  }
});

test('GREEN: missing wrong-ABI and source-copy drift refuse a fresh seal', async t => {
  for (const settings of [{ missing: true }, { wrong: true }, { expireCapture: true }, { beforeCopy(from) { fs.writeFileSync(from, 'raced source'); } }]) {
    await assert.rejects(fixture(t, settings), /stripped|support|Required|drift|active environment guard/i);
  }
});

test('GREEN: retained content mode presence and metadata corruption fail before replay writes', async t => {
  for (const kind of ['bytes', 'mode', 'missing', 'present', 'old-v5']) {
    const f = await fixture(t), directory = path.join(f.paths.runDirectory, 'supporting-products');
    const manifest = path.join(directory, 'manifest.json'), leaf = path.join(directory, id);
    const index = structuredClone(f.index), bundle = JSON.parse(fs.readFileSync(manifest));
    if (kind === 'bytes') fs.writeFileSync(leaf, 'wrong retained ABI');
    if (kind === 'mode') fs.chmodSync(leaf, 0o700);
    if (kind === 'missing') fs.rmSync(leaf);
    if (kind === 'present' || kind === 'old-v5') {
      if (kind === 'present') bundle.entries.find(row => row.id === id).present = false;
      else bundle.entries = bundle.entries.filter(row => row.id !== id);
      // Deliberately forged private metadata is refusal input, never a valid re-seal.
      fs.writeFileSync(manifest, JSON.stringify(bundle)); index.supportingSha256 = digest(fs.readFileSync(manifest));
    }
    await f.first.restore(); const next = f.guard(); await next.prepare(); const before = f.snapshot();
    await assert.rejects(f.proof.materializeStoredProductBundle({ paths: f.paths, index, environmentGuard: next,
      captureReplayInputs: f.ports.captureProductReplayInputs, inspectProducts: f.ports.inspectProducts,
      runnerAudit: f.config.runnerAudit, checkBudget: () => assert.fail('Immutable refusal precedes copying') }), /support|missing|V5|ENOENT/i);
    assert.deepEqual(f.snapshot(), before); await next.restore();
  }
});

test('GREEN: expired and cloned guards cannot copy; restore during an awaited precheck leaves target exact', async t => {
  const f = await fixture(t); await f.first.restore();
  const invoke = (guard, extra = {}) => f.proof.materializeStoredProductBundle({ paths: f.paths, index: f.index,
    environmentGuard: guard, captureReplayInputs: f.ports.captureProductReplayInputs, inspectProducts: f.ports.inspectProducts,
    runnerAudit: f.config.runnerAudit, checkBudget: () => {}, ...extra });
  await assert.rejects(invoke(f.first), /active environment guard/);
  const next = f.guard(); await next.prepare(); const before = f.snapshot();
  await assert.rejects(invoke({ ...next }), /active environment guard/);
  await assert.rejects(invoke(next, { async captureReplayInputs(tuple) { const result = await f.ports.captureProductReplayInputs(tuple); await next.restore(); return result; } }), /active environment guard/);
  assert.deepEqual(f.snapshot(), before);
  const budgetGuard = f.guard(); await budgetGuard.prepare();
  await assert.rejects(invoke(budgetGuard, { async checkBudget() { await budgetGuard.restore(); } }), /active environment guard/);
  assert.deepEqual(f.snapshot(), before);
});

test('GREEN: no-Android capture has no stripped entry and V4 historical layout remains unchanged', async t => {
  const noAndroid = await fixture(t, { noAndroid: true });
  assert(!noAndroid.proof.verifyStoredProductBundle(noAndroid.paths, noAndroid.index).layout.some(row => row.id === id));
  const v4 = await fixture(t, { version: 4 });
  assert(!v4.proof.verifyStoredProductBundle(v4.paths, v4.index).layout.some(row => row.id === id));
});

test('GREEN: private raw digest matches an independent oracle for zero short and multichunk reads', t => {
  const root = fs.mkdtempSync(path.join(tmpdir(), 'stripped-raw-')); t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  for (const length of [0, 1, 1024 * 1024, 1024 * 1024 + 17, 3 * 1024 * 1024 + 7]) {
    const name = path.join(root, String(length)), bytes = Buffer.alloc(length, 0x6b); fs.writeFileSync(name, bytes);
    let closes = 0, requested = 0;
    const proof = actualModule('./batch-product-proof.mjs', { readSync(fd, buffer, offset, size, position) {
      requested = Math.max(requested, size); return fs.readSync(fd, buffer, offset, Math.min(size, length < 1024 ? 17 : 4093), position);
    }, closeSync(fd) { closes++; return fs.closeSync(fd); } });
    assert.equal(proof.raw(name), digest(bytes)); assert.equal(closes, 1); assert(requested <= 1024 * 1024);
  }
});

test('GREEN: private raw digest closes on read drift and preserves primary error over close failure', t => {
  const root = fs.mkdtempSync(path.join(tmpdir(), 'stripped-fault-')); t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const name = path.join(root, 'leaf'); fs.writeFileSync(name, 'closed bytes'); const primary = Error('FIXED_READ'), cleanup = Error('FIXED_CLOSE');
  let closes = 0;
  const proof = actualModule('./batch-product-proof.mjs', { readSync() { throw primary; }, closeSync(fd) { fs.closeSync(fd); closes++; throw cleanup; } });
  assert.throws(() => proof.raw(name), error => error === primary); assert.equal(closes, 1);
  const early = actualModule('./batch-product-proof.mjs', { readSync() { return 0; } });
  assert.throws(() => early.raw(name), /initial size/);
  let stats = 0;
  const drift = actualModule('./batch-product-proof.mjs', { fstatSync(fd, settings) {
    const current = fs.fstatSync(fd, settings); if (++stats === 2) return { ...current, size: current.size + 1n }; return current;
  } });
  assert.throws(() => drift.raw(name), /changed during/);
});


test('GREEN: original packaged validators still reject each wrong ABI before using copied support', async t => {
  for (const abi of ['arm64-v8a', 'armeabi-v7a', 'x86_64']) {
    const f = await fixture(t), receipt = f.index.receipts.find(row => row.tuple.product === 'android');
    const app = path.join(f.paths.composeRoot, 'composeApp/build/outputs/apk/debug/composeApp-debug.apk');
    const apk = JSON.parse(fs.readFileSync(app)); apk.abis[abi] = 'wrong packaged ABI'; fs.writeFileSync(app, JSON.stringify(apk));
    assert.throws(() => f.ports.inspectProducts({ tuple: receipt.tuple, paths: { app,
      test: path.join(f.paths.composeRoot, 'composeApp/build/outputs/apk/androidTest/debug/composeApp-debug-androidTest.apk') } }), /Packaged Android FFI ABI mismatch/);
  }
});

test('GREEN: retained symlink and changed owning receipt cannot authorize a working write', async t => {
  for (const kind of ['symlink', 'receipt']) {
    const f = await fixture(t), retained = path.join(f.paths.runDirectory, 'supporting-products', id);
    const outside = f.put(path.join(f.root, 'unrelated'), 'unrelated sentinel');
    if (kind === 'symlink') { fs.rmSync(retained); fs.symlinkSync(outside, retained); }
    else {
      const receiptPath = f.index.receipts[0].builderReceiptPath;
      const receipt = JSON.parse(fs.readFileSync(receiptPath, 'utf8'));
      fs.writeFileSync(receiptPath, JSON.stringify({ ...receipt, closedFixtureTamper: true }));
    }
    await f.first.restore(); const next = f.guard(); await next.prepare(); const before = f.snapshot();
    await assert.rejects(f.proof.materializeStoredProductBundle({ paths: f.paths, index: f.index, environmentGuard: next,
      captureReplayInputs: f.ports.captureProductReplayInputs, inspectProducts: f.ports.inspectProducts,
      runnerAudit: f.config.runnerAudit }), /symlink|builder receipt|immutable bytes/i);
    assert.deepEqual(f.snapshot(), before); assert.equal(fs.readFileSync(outside, 'utf8'), 'unrelated sentinel'); await next.restore();
  }
});

test('GREEN: raw digest open close and path replacement faults have exact fd ownership', t => {
  const root = fs.mkdtempSync(path.join(tmpdir(), 'stripped-owner-')); t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const name = path.join(root, 'leaf'); fs.writeFileSync(name, 'closed original');
  const primary = Error('FIXED_OPEN'), cleanup = Error('FIXED_CLOSE'); let closes = 0;
  const open = actualModule('./batch-product-proof.mjs', { openSync() { throw primary; }, closeSync() { closes++; } });
  assert.throws(() => open.raw(name), error => error === primary); assert.equal(closes, 0);
  const close = actualModule('./batch-product-proof.mjs', { closeSync(fd) { fs.closeSync(fd); closes++; throw cleanup; } });
  assert.throws(() => close.raw(name), error => error === cleanup); assert.equal(closes, 1);
  let swapped = false;
  const replace = actualModule('./batch-product-proof.mjs', { readSync(...args) {
    const count = fs.readSync(...args);
    if (!swapped) { swapped = true; fs.renameSync(name, name + '.old'); fs.writeFileSync(name, 'replacement'); }
    return count;
  } });
  assert.throws(() => replace.raw(name), /changed during/);
});
