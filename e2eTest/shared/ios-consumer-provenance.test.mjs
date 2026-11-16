import assert from 'node:assert/strict';
import test from 'node:test';
import { chmodSync, mkdirSync, mkdtempSync, renameSync, rmSync, writeFileSync, readFileSync, readdirSync, realpathSync, statSync } from 'node:fs';
import * as filePorts from 'node:fs';
import { createHash } from 'node:crypto';
import { runInNewContext } from 'node:vm';
import { fileURLToPath } from 'node:url';
import { tmpdir } from 'node:os';
import { dirname, join, resolve, basename, relative, sep } from 'node:path';
import { iosAppProductHashes, hashIosTree } from './ios-ffi-provenance.mjs';
import { hashArtifactTree } from './artifact-tree.mjs';
import { iosBuildProvenanceIdentity } from './ios-consumer-provenance.mjs';
import { recordArtifactProvenance, verifyArtifactProvenance } from './build-provenance-manifest.mjs';
import { createBatchProductPreparation } from './batch-product-builders.mjs';
import { verifyPreparedProduct, androidConfigurationForScenario } from './batch-build-handoff.mjs';
import { IOS_E2E_CONFIGURATION, requireIosIdentityBuild, iosGeneratedTemplateProof } from './ios-test-identity.mjs';
import { batchEnvironmentPaths, createBatchEnvironmentGuard, environmentTreeHash, assertSnapshotDiskBudget, assertActiveBatchEnvironmentGuard } from './batch-environment.mjs';

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'ios-consumer-proof-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const products = join(root, 'Build/Products/Debug-iphonesimulator');
  const put = (path, value = path) => { const file = join(root, path); mkdirSync(dirname(file), { recursive: true }); writeFileSync(file, value); chmodSync(file, 0o644); return file; };
  const prefix = 'Build/Products/Debug-iphonesimulator/';
  const files = {
    app: put(`${prefix}MetaSecret.app/MetaSecret`),
    resource: put(`${prefix}MetaSecret.app/Resources/design.json`),
    framework: put(`${prefix}MetaSecret.app/Frameworks/ComposeApp.framework/ComposeApp`),
    runner: put(`${prefix}iosAppUITests-Runner.app/iosAppUITests-Runner`),
    plugin: put(`${prefix}iosAppUITests-Runner.app/PlugIns/iosAppUITests.xctest/iosAppUITests`),
  };
  const current = () => {
    const identity = iosBuildProvenanceIdentity({ derivedDataPath: root, registrationOnly: true });
    return { version: identity.version, sourceFingerprint: 'same-core-compose-source',
      artifacts: { iosBuild: identity.iosBuild, serverImageId: 'same-server' } };
  };
  const manifest = join(root, 'native-proof.json');
  recordArtifactProvenance(manifest, current(), 'approved build receipt');
  return { root, products, files, put, current, manifest };
}

test('actual consumed app packages retain identity when Xcode writes runtime cache outside them', (t) => {
  const f = fixture(t);
  const before = f.current();
  f.put('Build/Intermediates.noindex/XCBuildData/test-session.db', 'new runtime cache');
  f.put('Build/Products/test-session-status.json', 'finished');
  assert.deepEqual(f.current(), before);
  verifyArtifactProvenance({ path: f.manifest, current: f.current() });
  assert.deepEqual(iosAppProductHashes(f.root), Object.fromEntries(Object.keys(before.artifacts.iosBuild)
    .map((name) => [name, hashIosTree(join(f.products, name))])), 'Existing FFI/link verifier default hashes remain unchanged');
});

for (const target of ['app', 'runner', 'plugin', 'framework', 'resource']) {
  test(`a changed consumed ${target} rejects the stored build proof`, (t) => {
    const f = fixture(t);
    writeFileSync(f.files[target], 'tampered bytes');
    assert.throws(() => verifyArtifactProvenance({ path: f.manifest, current: f.current() }), /artifact keys=iosBuild/);
  });
}

test('package file modes and resource paths remain part of strict consumer identity', (t) => {
  const f = fixture(t);
  chmodSync(f.files.framework, 0o755);
  assert.throws(() => verifyArtifactProvenance({ path: f.manifest, current: f.current() }), /provenance mismatch/);
  chmodSync(f.files.framework, 0o644);
  renameSync(f.files.resource, `${f.files.resource}.renamed`);
  assert.throws(() => verifyArtifactProvenance({ path: f.manifest, current: f.current() }), /provenance mismatch/);
});

test('all app bundles are consumed and renamed/missing packages cannot reuse old proof', (t) => {
  const f = fixture(t);
  f.put('Build/Products/Debug-iphonesimulator/Extra.app/Extra');
  assert.equal(Object.keys(f.current().artifacts.iosBuild).length, 3);
  assert.throws(() => verifyArtifactProvenance({ path: f.manifest, current: f.current() }), /provenance mismatch/);
  rmSync(join(f.products, 'Extra.app'), { recursive: true });
  renameSync(join(f.products, 'MetaSecret.app'), join(f.products, 'Renamed.app'));
  assert.throws(() => verifyArtifactProvenance({ path: f.manifest, current: f.current() }), /provenance mismatch/);
  rmSync(join(f.products, 'iosAppUITests-Runner.app'), { recursive: true });
  assert.throws(() => f.current(), /app and XCTest runner products missing/);
});

test('old whole-Build manifest version cannot certify the new consumed-package format', (t) => {
  const f = fixture(t);
  const old = f.current(); old.version = 1; old.artifacts.iosBuild = 'old-whole-tree-hash';
  recordArtifactProvenance(f.manifest, old, 'old build format');
  assert.throws(() => verifyArtifactProvenance({ path: f.manifest, current: f.current() }), /version=1->3/);
});

test('legacy whole-Build identity keeps version 1 and the original absent behavior without app prerequisites', (t) => {
  const f = fixture(t);
  rmSync(join(f.root, 'Build'), { recursive: true });
  assert.deepEqual(iosBuildProvenanceIdentity({ derivedDataPath: f.root, registrationOnly: false }),
    { version: 1, iosBuild: hashArtifactTree(join(f.root, 'Build')) });
  assert.throws(() => iosBuildProvenanceIdentity({ derivedDataPath: f.root, registrationOnly: true }), /build products missing/);
  f.put('Build/Intermediates.noindex/runtime.db', 'before');
  const before = iosBuildProvenanceIdentity({ derivedDataPath: f.root, registrationOnly: false });
  assert.deepEqual(before, { version: 1, iosBuild: hashArtifactTree(join(f.root, 'Build')) });
  f.put('Build/Intermediates.noindex/runtime.db', 'after');
  assert.notDeepEqual(iosBuildProvenanceIdentity({ derivedDataPath: f.root, registrationOnly: false }), before);
});

import {createEnvironmentTestRun} from './environment-test-fixture.mjs';

function actualModule(name, bindings) {
  const url = new URL(name, import.meta.url), source = readFileSync(url, 'utf8');
  const names = [...source.matchAll(/^export (?:async )?(?:function|const)\s+(\w+)/gm)].map(match => match[1]);
  const body = source.replace(/^import\s[\s\S]*?;\s*$/gm, '').replace(/^export\s*\{[^}]*\}[^;]*;\s*$/gm, '')
    .replace(/^export /gm, '').replace(/import\.meta\.url/g, JSON.stringify(url.href));
  return runInNewContext(body + '\n({' + names.join(',') + '});', { ...filePorts,
    createHash, resolve, dirname, basename, relative, sep, fileURLToPath, Buffer, URL, ...bindings });
}

async function nativeComposition(t, { guarded = true, missingArchivesAtSeal = false } = {}) {
  const persistent = createEnvironmentTestRun();
  const root = realpathSync(mkdtempSync(join(tmpdir(), 'actual-native-composition-')));
  const coreRoot = join(root, 'core'), composeRoot = join(root, 'compose'), runDirectory = persistent.runDirectory;
  const derivedDataPath = join(coreRoot, 'e2eTest/.derivedData/iosApp'), wasmPath = join(coreRoot, 'meta-secret/web-cli/ui/pkg');
  const put = (path, value = 'controlled owning bytes') => { mkdirSync(dirname(path), { recursive: true }); writeFileSync(path, value); return path; };
  const coreNames = ['Taskfile.yml', 'meta-secret/core/src/node/app/sync/environment.rs', 'meta-secret/mobile/scripts/build-android.sh', 'e2eTest/shared/fixture-harness.mjs'];
  const composeNames = ['settings.gradle.kts', 'iosApp/iosApp.xcodeproj/project.pbxproj'];
  for (const name of coreNames) put(join(coreRoot, name));
  for (const name of composeNames) put(join(composeRoot, name));
  const paths = batchEnvironmentPaths(coreRoot, composeRoot);
  paths.configs.forEach((path, index) => put(path, index === 0
    ? 'const SELECTED_SERVER_ENVIRONMENT: ServerEnvironment = ServerEnvironment::Local;\n'
    : (index === 1 ? 'VITE_META_SECRET_ENV=' : 'META_SECRET_ENV=') + 'local\n'));
  const cli = put(join(coreRoot, 'meta-secret/target/debug/meta-cli'), 'original user CLI'); chmodSync(cli, 0o700);
  for (const path of ['iosApp/iosApp/UniffiGenerated', 'iosApp/iosApp/MetaSecretCoreService/UniffiGenerated'])
    put(join(composeRoot, path, 'mobile_uniffi_compat.swift'), 'original user compatibility');
  const original = [...paths.configs, ...paths.artifacts].map(environmentTreeHash), guards = [];
  const guard = () => {
    const value = createBatchEnvironmentGuard({ ...paths, runDirectory, preflight: async () => {}, setLocal: async () => assert.fail('Already Local'),
      snapshotBudget: (list, options) => assertSnapshotDiskBudget(list, { ...options, freeBytes: 64 * 1024 ** 3 }) });
    guards.push(value); return value;
  };
  t.after(async () => { try { for (const value of guards) await value.restore(); }
    finally { rmSync(root, { recursive: true, force: true }); persistent.cleanup(); } });
  const trace = [], versions = { cargo: 'controlled cargo', rustc: 'controlled rustc', java: 'controlled java', xcodebuild: 'controlled xcode' };
  const state = { image: 'sha256:controlled-image' };
  const inventory = (args, cwd) => {
    const scopes = args.slice(args.indexOf('--') + 1), names = cwd === coreRoot ? coreNames : composeNames;
    return names.filter(name => scopes.some(scope => name === scope || name.startsWith(scope + '/'))).join('\0') + '\0';
  };
  const sync = (file, args, options = {}) => {
    trace.push(['read', file, ...Array.from(args)]);
    if (file === 'git') return inventory(Array.from(args), options.cwd);
    if (versions[file]) return versions[file];
    if (file === 'unzip') {
      assert(['-Z1', '-p'].includes(args[0]));
      const apk = JSON.parse(readFileSync(args[1], 'utf8'));
      if (args[0] === '-Z1') return Object.keys(apk.abis).map(abi => 'lib/' + abi + '/libmetasecret_mobile.so').join('\n');
      assert.equal(args[0], '-p'); return Buffer.from(apk.abis[args[2].split('/')[1]]);
    }
    assert.fail('No external synchronous tool: ' + file);
  };
  const spawnSync = (file, args, options) => { assert.equal(file, 'git'); return { status: 0, stdout: inventory(Array.from(args), options.cwd) }; };
  const environment = { E2E_RUN_ID: 'native-composition', E2E_BATCH_PRODUCTS_PATH: join(runDirectory, 'batch-products.json') };
  const processPort = { env: environment, version: process.version, stdout: { write() {} }, stderr: { write() {} } };
  const command = async (file, args) => {
    if (file === 'xcodebuild') return { stdout: JSON.stringify([{ target: 'iosApp', buildSettings: {
      OTHER_LDFLAGS: join(composeRoot, 'iosApp/Libs/libmetasecret_mobile-ios-simulator-arm64.a'), METASECRET_E2E_IDENTITY_REQUIRED: 'YES',
    } }]) };
    if (file === 'docker') return { stdout: state.image };
    assert.fail('No external asynchronous tool: ' + file);
  };
  const capture = async (file, args, options = {}) => {
    if (file === 'git' || versions[file] && ['--version', '-version'].includes(args[0])) return { stdout: sync(file, Array.from(args), options) };
    return command(file, args);
  };
  const ios = actualModule('./ios-ffi-provenance.mjs', { spawnSync, process: processPort,
    spawn: () => assert.fail('No compiler process'), });
  const rangeSource = readFileSync(new URL('../run-ui-e2e-range.mjs', import.meta.url), 'utf8');
  const constants = rangeSource.slice(rangeSource.indexOf('const androidFfiBuildScript ='), rangeSource.indexOf('const diskThresholdGiB ='));
  const producer = rangeSource.slice(rangeSource.indexOf('function fingerprintAndroidCoreSource('), rangeSource.indexOf('\nfunction parseOptions('));
  const abis = ['arm64-v8a', 'armeabi-v7a', 'x86_64'];
  const execute = async (file, args) => {
    trace.push(['build', file, ...Array.from(args)]);
    if (file === 'bash' && args[0].endsWith('build-android.sh')) {
      for (const abi of abis) put(join(composeRoot, 'composeApp/src/androidMain/jniLibs', abi, 'libmetasecret_mobile.so'), '[CORE_DIAG] sync_vault response events= controlled/' + abi);
      put(join(composeRoot, 'composeApp/src/androidMain/kotlin/com/metasecret/core/uniffi/bindings.kt'));
    } else if (file === 'bash') {
      assert(args[0].endsWith('build-ios.sh'));
      for (const [target, label] of [['aarch64-apple-ios', 'ios-arm64'], ['aarch64-apple-ios-sim', 'ios-simulator-arm64']]) {
        put(join(coreRoot, 'meta-secret/target', target, 'release/libmetasecret_mobile.a'), target);
        put(join(composeRoot, 'iosApp/Libs/libmetasecret_mobile-' + label + '.a'), target);
      }
      for (const path of ['iosApp/iosApp/UniffiGenerated', 'iosApp/iosApp/MetaSecretCoreService/UniffiGenerated']) put(join(composeRoot, path, 'bindings.swift'));
    } else if (file === 'cargo') { put(cli, 'owning F2 CLI'); chmodSync(cli, 0o755);
    } else if (file === './gradlew') {
      const libs = Object.fromEntries(abis.map(abi => [abi, readFileSync(join(composeRoot, 'composeApp/src/androidMain/jniLibs', abi, 'libmetasecret_mobile.so'), 'utf8')]));
      put(join(composeRoot, 'composeApp/build/outputs/apk/debug/composeApp-debug.apk'), JSON.stringify({ abis: libs }));
      put(join(composeRoot, 'composeApp/build/outputs/apk/androidTest/debug/composeApp-debug-androidTest.apk'));
    } else if (file === 'xcodebuild') {
      assert(args.includes('METASECRET_E2E_IDENTITY_REQUIRED=YES'));
      for (const name of ['iosApp.app', 'iosAppUITests-Runner.app']) {
        put(join(derivedDataPath, 'Build/Products/Debug-iphonesimulator', name, 'Info.plist'), JSON.stringify({ MetaSecretE2EIdentityRequired: 'YES' }));
        put(join(derivedDataPath, 'Build/Products/Debug-iphonesimulator', name, 'executable'));
      }
      put(join(derivedDataPath, 'Build/Products/owning.xctestrun'), 'original generated owning template');
    } else { assert.equal(file, '/opt/homebrew/bin/task');
      if (args[0] === 'wasm-local') put(join(wasmPath, 'module.wasm'));
      else { assert.equal(args[0], 'meta-server');
        if (missingArchivesAtSeal) for (const target of ['aarch64-apple-ios', 'aarch64-apple-ios-sim'])
          rmSync(join(coreRoot, 'meta-secret/target', target, 'release/libmetasecret_mobile.a'));
      }
    }
    return { exitCode: 0 };
  };
  mkdirSync(join(coreRoot, 'e2eTest/.artifacts'), { recursive: true });
  const native = runInNewContext(constants + '\n' + producer + '\n({ prepareAndroidFfiBeforePlatforms, prepareMetaCliBeforePlatforms });', {
    ...filePorts, resolve, createHash, Buffer, spawnSync, process: processPort, coreRoot, composeRoot,
    e2eRoot: join(coreRoot, 'e2eTest'), metaCliRoot: join(coreRoot, 'meta-secret'), metaCliBinaryPath: cli,
    androidFfiSourcePrepared: undefined, metaCliSourcePrepared: undefined, runPreparationCommand: execute,
  });
  const identity = app => requireIosIdentityBuild(app, {
    readMarker: path => JSON.parse(readFileSync(join(path, 'Info.plist'), 'utf8')).MetaSecretE2EIdentityRequired,
    verifySignature: () => {}, // Closed external codesign port; no signature security claim.
  });
  const mutations = { observe() {}, cpSync: filePorts.cpSync };
  const writePorts = Object.fromEntries(['cpSync', 'rmSync', 'mkdirSync', 'chmodSync', 'writeFileSync'].map(name => [name,
    (...args) => { mutations.observe(name, args); return (name === 'cpSync' ? mutations.cpSync : filePorts[name])(...args); }]));
  const proof = actualModule('./batch-product-proof.mjs', { ...writePorts, process: processPort, execFileSync: sync,
    requireIosIdentityBuild: identity, hashArtifactTree, hashIosTree: ios.hashIosTree,
    verifyIosFfi: ios.verifyIosFfi, verifyIosXcodeBuild: ios.verifyIosXcodeBuild,
    batchEnvironmentPaths, assertActiveBatchEnvironmentGuard, assertSnapshotDiskBudget, verifyArtifactProvenance });
  const ports = proof.createProductProofPorts({ coreRoot, composeRoot, runAndCapture: capture });
  const initialGuard = guard(); await initialGuard.prepare();
  const options = { paths: { coreRoot, composeRoot, runDirectory }, runId: environment.E2E_RUN_ID,
    ...(guarded ? { environmentGuard: initialGuard } : {}),
    scenarios: [1, 16, 50].map(testNumber => ({ testNumber, scenario: { android: {}, ios: {}, web: {}, cli: { required: true },
      server: { image: 'controlled-image' }, ...(testNumber === 1 ? {} : { networkLoss: { androidProxyPort: testNumber === 16 ? 5181 : 5201 } }) } })),
    prepareNative: async () => {
      const entry = { testNumber: 1, scenarios: [{ value: { android: {}, cli: { required: true } } }] };
      const collector = { writeEvent() {}, writeOutput() {} };
      const android = await native.prepareAndroidFfiBeforePlatforms(entry, collector);
      await ios.ensureIosFfi({ coreRoot, composeRoot, runAndWait: execute, onEvent() {} });
      const cli = await native.prepareMetaCliBeforePlatforms(entry, collector); return { android, ios: true, cli };
    }, commands: { proofPorts: ports, checkBudget: () => ({ requiredBytes: 0 }),
      runAndWait: execute, runAndCapture: capture, iosBuild: { prepare: ios.prepareIosXcodeBuild, record: ios.recordIosXcodeBuild } },
  };
  const graph = createBatchProductPreparation(options); await graph.prepare();
  const indexPath = environment.E2E_BATCH_PRODUCTS_PATH, indexBytes = readFileSync(indexPath);
  const consumer = actualModule('./batch-product-consumer.mjs', { ...proof, process: processPort, execFileSync: sync,
    IOS_E2E_CONFIGURATION, androidConfigurationForScenario, verifyPreparedProduct, verifyArtifactProvenance, hashArtifactTree,
    iosAppProductHashes: ios.iosAppProductHashes });
  const select = () => consumer.loadBatchProducts({ coreRoot, composeRoot, scenario: options.scenarios[0].scenario,
    testNumber: 1, runAndCapture: capture });
  return { root, coreRoot, composeRoot, runDirectory, derivedDataPath, wasmPath, cli, paths, original, guard, initialGuard,
    graph, options, indexPath, indexBytes, put, trace, state, versions, select, consumer, ios, ports, mutations, proof,
    snapshot: () => [...paths.configs, ...paths.artifacts].map(environmentTreeHash),
    builds: () => trace.filter(call => call[0] === 'build').length };
}

test('actual native validators compose with full guarded restore replay and consumer pre-spawn', async t => {
  const f = await nativeComposition(t), owned = f.snapshot(), builds = f.builds();
  const selected = await f.select(); f.consumer.assertBatchProductsCurrent({ products: selected, coreRoot: f.coreRoot, composeRoot: f.composeRoot });
  assert.equal(iosGeneratedTemplateProof(f.derivedDataPath).original, join(f.derivedDataPath, 'Build/Products/owning.xctestrun'));
  await f.initialGuard.restore(); assert.deepEqual(f.snapshot(), f.original);
  assert.equal(readFileSync(f.cli, 'utf8'), 'original user CLI'); assert.equal(statSync(f.cli).mode & 0o777, 0o700);
  const next = f.guard(); await next.prepare();
  try {
    await createBatchProductPreparation({ ...f.options, environmentGuard: next }).prepare();
    assert.deepEqual(f.snapshot(), owned); assert.equal(f.builds(), builds);
    const consumed = await f.select(); f.consumer.assertBatchProductsCurrent({ products: consumed, coreRoot: f.coreRoot, composeRoot: f.composeRoot });
    assert.deepEqual(readFileSync(f.indexPath), f.indexBytes);
  } finally { await next.restore(); assert.deepEqual(f.snapshot(), f.original); }
});

test('actual V4 capture refuses missing required uncovered archives after native production', async t => {
  await assert.rejects(nativeComposition(t, { missingArchivesAtSeal: true }), /archive|required|missing/i,
    'Valid copied native proof cannot authorize a seal when uncovered Cargo archives are absent');
});

function corruptStored(path) {
  while (statSync(path).isDirectory()) path = join(path, readdirSync(path).sort()[0]);
  writeFileSync(path, 'controlled corrupt stored bytes');
}
const storedIds = ['android-jni', 'android-bindings', 'android-provenance', 'android-apks', 'ios-libs',
  'ios-bindings-app', 'ios-bindings-service', 'ios-provenance', 'ios-link', 'ios-app', 'ios-test', 'ios-template', 'cli', 'wasm'];
const corruptionCases = [
  ...storedIds.map(id => ['stored-' + id, f => corruptStored(join(f.runDirectory, 'supporting-products', id))]),
  ['missing-bundle', f => rmSync(join(f.runDirectory, 'supporting-products/manifest.json'))],
  ['cli-mode', f => chmodSync(join(f.runDirectory, 'supporting-products/cli'), 0o600)],
  ['source-drift', f => f.put(join(f.coreRoot, 'meta-secret/mobile/scripts/build-android.sh'), 'changed source')],
  ['harness-drift', f => f.put(join(f.coreRoot, 'e2eTest/shared/fixture-harness.mjs'), 'changed harness')],
  ['toolchain-drift', f => { f.versions.cargo = 'different current cargo'; }],
  ['server-image', f => { f.state.image = 'sha256:different-image'; }],
  ['missing-cross-archive', f => rmSync(join(f.coreRoot, 'meta-secret/target/aarch64-apple-ios/release/libmetasecret_mobile.a'))],
  ['compatibility-drift', f => f.put(join(f.composeRoot, 'iosApp/iosApp/UniffiGenerated/mobile_uniffi_compat.swift'), 'new user compatibility')],
  ['owning-receipt-failed', f => { const path = JSON.parse(f.indexBytes).receipts[0].builderReceiptPath;
    const value = JSON.parse(readFileSync(path)); value.exitCode = 1; writeFileSync(path, JSON.stringify(value)); }],
  ...['run', 'format', 'unsealed', 'duplicate', 'omitted', 'root'].map(kind => ['index-' + kind, f => {
    const value = JSON.parse(f.indexBytes);
    if (kind === 'run') value.runId = 'foreign-run'; if (kind === 'format') value.version = 99;
    if (kind === 'unsealed') value.sealed = false; if (kind === 'duplicate') value.receipts.push(value.receipts[0]);
    if (kind === 'omitted') value.receipts.pop(); if (kind === 'root') value.receipts[0].products.app = join(f.root, 'foreign.apk');
    writeFileSync(f.indexPath, JSON.stringify(value));
  }]),
  ['stored-symlink', f => { const path = join(f.runDirectory, 'supporting-products/cli'); rmSync(path);
    filePorts.symlinkSync(f.put(join(f.root, 'outside-cli'), 'untouched outside'), path); }],
  ['destination-symlink', f => { const path = join(f.composeRoot, 'composeApp/src/androidMain/jniLibs');
    mkdirSync(join(f.root, 'outside'), { recursive: true }); filePorts.symlinkSync(join(f.root, 'outside'), path); }],
];
for (const [name, mutate] of corruptionCases) test('V4 actual composition rejects ' + name + ' before replay writes or builders', async t => {
  const f = await nativeComposition(t); await f.initialGuard.restore(); mutate(f);
  const before = f.snapshot(), builds = f.builds(), index = readFileSync(f.indexPath), next = f.guard(); await next.prepare();
  try {
    await assert.rejects(createBatchProductPreparation({ ...f.options, environmentGuard: next }).prepare());
    assert.deepEqual(f.snapshot(), before); assert.equal(f.builds(), builds);
    assert.deepEqual(readFileSync(f.indexPath), index, 'A rejected controlled fixture is never resealed');
  } finally { await next.restore(); assert.deepEqual(f.snapshot(), before); }
});

test('cached factory await cannot return prepared authority after restore begins', async t => {
  const f = await nativeComposition(t), builds = f.builds();
  const pending = f.graph.prepare(), restoring = f.initialGuard.restore();
  await assert.rejects(pending, /active environment guard/); await restoring;
  assert.deepEqual(f.snapshot(), f.original); assert.equal(f.builds(), builds);
});

test('concurrent actual materialization is refused while first current guard lease waits', async t => {
  const f = await nativeComposition(t); await f.initialGuard.restore(); const next = f.guard(); await next.prepare();
  let entered, release, held = false; const waiting = new Promise(resolve => { entered = resolve; });
  const barrier = new Promise(resolve => { release = resolve; }), capture = f.ports.captureReplayInputs;
  f.ports.captureReplayInputs = async tuple => { const value = await capture(tuple);
    if (!held) { held = true; entered(); await barrier; } return value; };
  const options = { ...f.options, environmentGuard: next }, before = f.snapshot(), builds = f.builds();
  const pending = createBatchProductPreparation(options).prepare(); void pending.catch(() => {}); await waiting;
  try {
    await assert.rejects(createBatchProductPreparation(options).prepare(), /Concurrent/);
    assert.deepEqual(f.snapshot(), before); release(); await pending; assert.equal(f.builds(), builds);
  } finally { release(); await pending.catch(() => {}); await next.restore(); assert.deepEqual(f.snapshot(), f.original); }
});

test('mutation after asynchronous replay prechecks is refused before first write', async t => {
  const f = await nativeComposition(t); await f.initialGuard.restore(); const next = f.guard(); await next.prepare();
  const before = f.snapshot(), builds = f.builds();
  f.options.commands.checkBudget = async () => { corruptStored(join(f.runDirectory, 'supporting-products/cli')); return { requiredBytes: 0 }; };
  try { await assert.rejects(createBatchProductPreparation({ ...f.options, environmentGuard: next }).prepare());
    assert.deepEqual(f.snapshot(), before); assert.equal(f.builds(), builds);
  } finally { await next.restore(); assert.deepEqual(f.snapshot(), f.original); }
});


const nativeDrifts = [
  ['android-jni', 'android', f => f.put(join(f.composeRoot, 'composeApp/src/androidMain/jniLibs/arm64-v8a/libmetasecret_mobile.so'), 'wrong JNI')],
  ['android-binding', 'android', f => f.put(join(f.composeRoot, 'composeApp/src/androidMain/kotlin/com/metasecret/core/uniffi/bindings.kt'), 'wrong binding')],
  ['ios-archive', 'ios', f => f.put(join(f.composeRoot, 'iosApp/Libs/libmetasecret_mobile-ios-simulator-arm64.a'), 'wrong archive')],
  ['ios-binding', 'ios', f => f.put(join(f.composeRoot, 'iosApp/iosApp/UniffiGenerated/bindings.swift'), 'wrong binding')],
  ['ios-template', 'ios', f => f.put(join(f.derivedDataPath, 'Build/Products/owning.xctestrun'), 'wrong original template')],
  ['ios-app', 'ios', f => f.put(join(f.derivedDataPath, 'Build/Products/Debug-iphonesimulator/iosApp.app/executable'), 'wrong app')],
  ['packaged-ABI', 'android', (f, receipt) => { const path = receipt.products.app, apk = JSON.parse(readFileSync(path)); apk.abis['arm64-v8a'] = 'foreign ABI'; writeFileSync(path, JSON.stringify(apk)); }],
  ['link-identity', 'ios', f => { const path = join(f.derivedDataPath, 'ios-core-link-provenance.json'), link = JSON.parse(readFileSync(path)); link.inputs.identityRequired = 'NO'; writeFileSync(path, JSON.stringify(link)); }],
];
for (const [name, product, mutate] of nativeDrifts) test('unchanged native validator and pre-spawn reject current ' + name, async t => {
  const f = await nativeComposition(t), selected = await f.select(), receipt = selected[product]; mutate(f, receipt);
  await assert.rejects(verifyPreparedProduct({ receipt, captureInputs: f.ports.captureInputs, inspectProducts: f.ports.inspectProducts }));
  assert.throws(() => f.consumer.assertBatchProductsCurrent({ products: selected, coreRoot: f.coreRoot, composeRoot: f.composeRoot }));
  await assert.rejects(f.select());
});

test('genuine legacy V3 intact proof is accepted but full restore cannot hydrate rebuild or reseal it', async t => {
  const f = await nativeComposition(t, { guarded: false });
  assert.notEqual(JSON.parse(f.indexBytes).version, 4); assert.equal(filePorts.existsSync(join(f.runDirectory, 'supporting-products')), false);
  await f.select(); await f.initialGuard.restore(); assert.deepEqual(f.snapshot(), f.original);
  const next = f.guard(); await next.prepare(); const before = f.snapshot(), builds = f.builds();
  try { await assert.rejects(createBatchProductPreparation({ ...f.options, environmentGuard: next }).prepare());
    assert.deepEqual(f.snapshot(), before); assert.equal(f.builds(), builds);
    assert.deepEqual(readFileSync(f.indexPath), f.indexBytes); assert.equal(filePorts.existsSync(join(f.runDirectory, 'supporting-products')), false);
  } finally { await next.restore(); assert.deepEqual(f.snapshot(), f.original); }
});

test('restore during actual replay input await rejects without materializer writes', async t => {
  const f = await nativeComposition(t); await f.initialGuard.restore(); const next = f.guard(); await next.prepare();
  let entered, release, writes = 0; const waiting = new Promise(resolve => { entered = resolve; });
  const barrier = new Promise(resolve => { release = resolve; }), capture = f.ports.captureReplayInputs;
  f.ports.captureReplayInputs = async tuple => { const value = await capture(tuple); entered(); await barrier; return value; };
  const immutable = hashArtifactTree(join(f.runDirectory, 'supporting-products')), builds = f.builds();
  const pending = f.proof.materializeStoredProductBundle({ paths: f.options.paths, index: JSON.parse(f.indexBytes),
    environmentGuard: next, captureReplayInputs: f.ports.captureReplayInputs, inspectProducts: f.ports.inspectProducts,
    checkBudget: f.options.commands.checkBudget }); void pending.catch(() => {});
  try {
    await Promise.race([waiting, pending.then(() => assert.fail('Replay input await must be entered'))]);
    const restoring = next.restore(); f.mutations.observe = () => { writes++; }; await restoring; release();
    await assert.rejects(pending, /active environment guard/); assert.equal(writes, 0); assert.equal(f.builds(), builds);
    assert.deepEqual(f.snapshot(), f.original); assert.deepEqual(readFileSync(f.indexPath), f.indexBytes);
    assert.equal(hashArtifactTree(join(f.runDirectory, 'supporting-products')), immutable);
  } finally { release(); await pending.catch(() => {}); await next.restore(); assert.deepEqual(f.snapshot(), f.original); }
});

test('mid-copy failure preserves immutable proof and finally restores actual22 originals', async t => {
  const f = await nativeComposition(t); await f.initialGuard.restore(); const next = f.guard(); await next.prepare();
  const before = f.snapshot(), immutable = hashArtifactTree(join(f.runDirectory, 'supporting-products')), builds = f.builds();
  const failure = new Error('Controlled copy failure after actual mutation'); let copies = 0;
  f.mutations.cpSync = (...args) => { filePorts.cpSync(...args); copies++; assert.notDeepEqual(f.snapshot(), before); throw failure; };
  try {
    await assert.rejects(f.proof.materializeStoredProductBundle({ paths: f.options.paths, index: JSON.parse(f.indexBytes),
      environmentGuard: next, captureReplayInputs: f.ports.captureReplayInputs, inspectProducts: f.ports.inspectProducts,
      checkBudget: f.options.commands.checkBudget }), error => error === failure);
    assert.equal(copies, 1); assert.equal(f.builds(), builds); assert.notDeepEqual(f.snapshot(), before);
    assert.deepEqual(readFileSync(f.indexPath), f.indexBytes); assert.equal(hashArtifactTree(join(f.runDirectory, 'supporting-products')), immutable);
  } finally { await next.restore(); assert.deepEqual(f.snapshot(), f.original); }
});
