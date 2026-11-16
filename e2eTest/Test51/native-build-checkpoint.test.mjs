import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve, dirname } from 'node:path';
import { captureExpectedLocalSource, invokeOfficialLocalBuild, recordOfficialNativeBuild, verifyOfficialNativeBuild } from './native-build-checkpoint.mjs';
import { ensureIosFfi } from '../shared/ios-ffi-provenance.mjs';

function fixture(t) {
  const root = mkdtempSync(resolve(tmpdir(), 'registration-native-checkpoint-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const coreRoot = resolve(root, 'core'), composeRoot = resolve(root, 'compose');
  const put = (path, value = 'fixture') => { mkdirSync(dirname(path), { recursive: true }); writeFileSync(path, value); };
  mkdirSync(coreRoot); execFileSync('git', ['init', '-q', coreRoot]);
  put(resolve(coreRoot, '.gitignore'), 'meta-secret/target/\nmeta-secret/Cargo.lock\ne2eTest/.artifacts/\n');
  const environment = resolve(coreRoot, 'meta-secret/core/src/node/app/sync/environment.rs');
  const remote = 'pub const SELECTED_SERVER_ENVIRONMENT: ServerEnvironment = ServerEnvironment::Remote;\n';
  put(environment, remote);
  put(resolve(coreRoot, 'meta-secret/core/src/lib.rs'), 'source before official command');
  const checkpointPath = resolve(coreRoot, 'e2eTest/.artifacts/runs/test51/native-build.json');
  let commands = 0;
  const runCommand = async (command, args, options) => {
    commands++;
    assert.equal(command, '/opt/homebrew/bin/task'); assert.deepEqual(args, ['set-env', '--', 'local']); assert.equal(options.cwd, coreRoot);
    put(environment, remote.replace('::Remote;', '::Local;'));
    for (const [target, archive] of [['aarch64-apple-ios', 'ios-arm64'], ['aarch64-apple-ios-sim', 'ios-simulator-arm64']]) {
      put(resolve(coreRoot, `meta-secret/target/${target}/release/libmetasecret_mobile.a`), target);
      put(resolve(composeRoot, `iosApp/Libs/libmetasecret_mobile-${archive}.a`), target);
    }
    for (const dir of ['iosApp/iosApp/UniffiGenerated', 'iosApp/iosApp/MetaSecretCoreService/UniffiGenerated']) put(resolve(composeRoot, dir, 'mobile_uniffi.swift'), 'official Swift binding');
    put(resolve(composeRoot, 'composeApp/src/androidMain/kotlin/com/metasecret/core/uniffi/mobile_uniffi.kt'), 'official Kotlin binding');
    for (const [abi, target] of [['arm64-v8a', 'aarch64-linux-android'], ['armeabi-v7a', 'armv7-linux-androideabi'], ['x86_64', 'x86_64-linux-android']]) {
      put(resolve(composeRoot, `composeApp/src/androidMain/jniLibs/${abi}/libmetasecret_mobile.so`), `${target} [CORE_DIAG] sync_vault response events=`);
      put(resolve(coreRoot, `meta-secret/target/${target}/release/libmetasecret_mobile.a`), target);
    }
    put(resolve(coreRoot, 'meta-secret/Cargo.lock'), 'ignored regenerated lock');
    return { exitCode: 0 };
  };
  const options = { coreRoot, composeRoot, checkpointPath };
  const prepare = async () => {
    const expectedSource = captureExpectedLocalSource(options);
    const receipt = await invokeOfficialLocalBuild({ coreRoot, runId: 'test51', runCommand });
    return { ...options, expectedSource, receipt };
  };
  return { ...options, put, prepare, runCommand, commands: () => commands };
}

test('one owned official build records strict outputs and existing iOS verifier reuses without another build', async (t) => {
  const f = fixture(t), build = await f.prepare();
  const checkpoint = recordOfficialNativeBuild(build);
  assert.deepEqual(verifyOfficialNativeBuild(f), checkpoint);
  let extraBuilds = 0;
  await ensureIosFfi({ ...f, runAndWait: async () => { extraBuilds++; throw new Error('unexpected rebuild'); }, onEvent: () => {} });
  assert.equal(extraBuilds, 0); assert.equal(f.commands(), 1);
  assert.equal(checkpoint.inputs.ios.sourceFingerprint, checkpoint.inputs.android.sourceFingerprint);
  assert.throws(() => recordOfficialNativeBuild(build), /receipt/);
});

test('missing, forged, failed or nonzero official command receipts cannot record provenance', async (t) => {
  const f = fixture(t), expectedSource = captureExpectedLocalSource(f);
  assert.throws(() => recordOfficialNativeBuild({ ...f, expectedSource }), /receipt/);
  assert.throws(() => recordOfficialNativeBuild({ ...f, expectedSource, receipt: { exitCode: 0, cwd: f.coreRoot } }), /receipt/);
  for (const result of [undefined, {}, { exitCode: 1 }]) await assert.rejects(invokeOfficialLocalBuild({ ...f, runId: 'test51', runCommand: async () => result }), /successful exit receipt/);
  await assert.rejects(invokeOfficialLocalBuild({ ...f, runId: 'test51', runCommand: async () => { throw new Error('build failed'); } }), /build failed/);
  assert.equal(existsSync(resolve(f.coreRoot, 'e2eTest/.artifacts/ios-ffi-build-provenance.json')), false);
});

test('unrelated source drift during official build fails before writing manifests', async (t) => {
  const f = fixture(t), build = await f.prepare();
  f.put(resolve(f.coreRoot, 'meta-secret/core/src/lib.rs'), 'changed source');
  assert.throws(() => recordOfficialNativeBuild(build), /Unrelated source drift/);
  assert.equal(existsSync(f.checkpointPath), false);
});

test('extra changes to environment source fail, even when selector becomes local', async (t) => {
  const f = fixture(t), build = await f.prepare();
  const path = resolve(f.coreRoot, 'meta-secret/core/src/node/app/sync/environment.rs');
  f.put(path, readFileSync(path, 'utf8') + '// unrelated change\n');
  assert.throws(() => recordOfficialNativeBuild(build), /Unrelated source drift/);
});

test('copied iOS archives must equal the official Cargo target output', async (t) => {
  const f = fixture(t), build = await f.prepare();
  f.put(resolve(f.composeRoot, 'iosApp/Libs/libmetasecret_mobile-ios-simulator-arm64.a'), 'stale archive');
  assert.throws(() => recordOfficialNativeBuild(build), /differs from official Cargo output/);
});

for (const [name, location] of [
  ['Core source', ['coreRoot', 'meta-secret/core/src/lib.rs']],
  ['Swift binding', ['composeRoot', 'iosApp/iosApp/UniffiGenerated/mobile_uniffi.swift']],
  ['second Swift binding tree', ['composeRoot', 'iosApp/iosApp/MetaSecretCoreService/UniffiGenerated/mobile_uniffi.swift']],
  ['Kotlin binding', ['composeRoot', 'composeApp/src/androidMain/kotlin/com/metasecret/core/uniffi/mobile_uniffi.kt']],
  ['Android output', ['composeRoot', 'composeApp/src/androidMain/jniLibs/arm64-v8a/libmetasecret_mobile.so']],
  ['Android static archive', ['coreRoot', 'meta-secret/target/aarch64-linux-android/release/libmetasecret_mobile.a']],
  ['iOS Cargo archive', ['coreRoot', 'meta-secret/target/aarch64-apple-ios-sim/release/libmetasecret_mobile.a']],
]) test(`${name} tamper invalidates reuse checkpoint`, async (t) => {
  const f = fixture(t); recordOfficialNativeBuild(await f.prepare());
  f.put(resolve(f[location[0]], location[1]), 'tampered [CORE_DIAG] sync_vault response events=');
  assert.throws(() => verifyOfficialNativeBuild(f), /stale|changed|differs/);
});

test('missing archive or Android diagnostic marker rejects build checkpoint', async (t) => {
  const f = fixture(t), build = await f.prepare();
  const output = resolve(f.composeRoot, 'composeApp/src/androidMain/jniLibs/x86_64/libmetasecret_mobile.so');
  f.put(output, 'missing diagnostic');
  assert.throws(() => recordOfficialNativeBuild(build), /diagnostic marker/);
  rmSync(output);
  assert.throws(() => recordOfficialNativeBuild(build), /ENOENT/);
});

test('empty generated binding trees cannot masquerade as successful build outputs', async (t) => {
  const f = fixture(t), build = await f.prepare();
  rmSync(resolve(f.composeRoot, 'iosApp/iosApp/UniffiGenerated/mobile_uniffi.swift'));
  assert.throws(() => recordOfficialNativeBuild(build), /Generated binding missing/);
});

test('remote configuration and modified persisted command receipt reject reuse', async (t) => {
  const f = fixture(t); recordOfficialNativeBuild(await f.prepare());
  const checkpoint = JSON.parse(readFileSync(f.checkpointPath));
  checkpoint.receipt.args = ['set-env', '--', 'remote'];
  f.put(f.checkpointPath, JSON.stringify(checkpoint));
  assert.throws(() => verifyOfficialNativeBuild(f), /Invalid official-build checkpoint receipt/);
});
