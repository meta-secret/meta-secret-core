import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { ensureIosFfi, verifyIosFfi, prepareIosXcodeBuild, recordIosXcodeBuild, verifyIosXcodeBuild } from './ios-ffi-provenance.mjs';

function fixture(t) {
  const root = mkdtempSync(resolve(tmpdir(), 'ios-ffi-gate-test-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const coreRoot = resolve(root, 'meta-secret-core');
  const composeRoot = resolve(root, 'meta-secret-compose');
  const put = (path, content = 'fixture') => { mkdirSync(resolve(path, '..'), { recursive: true }); writeFileSync(path, content); };
  mkdirSync(coreRoot);
  execFileSync('git', ['init', '-q', coreRoot]);
  put(resolve(coreRoot, '.gitignore'), 'meta-secret/target/\n');
  put(resolve(coreRoot, 'meta-secret/core/src/node/app/sync/environment.rs'), 'const SELECTED_SERVER_ENVIRONMENT: ServerEnvironment = ServerEnvironment::Local;');
  put(resolve(composeRoot, 'iosApp/iosApp.xcodeproj/project.pbxproj'));
  let builds = 0;
  const build = async (command, args) => {
    assert.equal(command, 'bash');
    assert.equal(args[0], resolve(coreRoot, 'meta-secret/mobile/scripts/build-ios.sh'));
    builds++;
    for (const [target, archive] of [['aarch64-apple-ios', 'ios-arm64'], ['aarch64-apple-ios-sim', 'ios-simulator-arm64']]) {
      put(resolve(coreRoot, `meta-secret/target/${target}/release/libmetasecret_mobile.a`), `library-${builds}-${target}`);
      put(resolve(composeRoot, `iosApp/Libs/libmetasecret_mobile-${archive}.a`), `library-${builds}-${target}`);
    }
    for (const dir of ['iosApp/iosApp/UniffiGenerated', 'iosApp/iosApp/MetaSecretCoreService/UniffiGenerated']) put(resolve(composeRoot, dir, 'mobile_uniffi.swift'), 'current binding');
  };
  const options = { coreRoot, composeRoot, runAndWait: build, onEvent: () => {} };
  return { ...options, options, put, builds: () => builds };
}

test('missing provenance triggers the official build; only unchanged source and outputs can be reused', async (t) => {
  const f = fixture(t);
  assert.throws(() => verifyIosFfi(f), /missing/);
  await ensureIosFfi(f.options);
  await ensureIosFfi(f.options);
  assert.equal(f.builds(), 1);
  f.put(resolve(f.composeRoot, 'iosApp/Libs/libmetasecret_mobile-ios-simulator-arm64.a'), 'old archive');
  assert.throws(() => verifyIosFfi(f), /stale/);
  await ensureIosFfi(f.options);
  assert.equal(f.builds(), 2);
  f.put(resolve(f.coreRoot, 'meta-secret/core/changed.rs'), 'new protocol field');
  assert.throws(() => verifyIosFfi(f), /stale/);
  await ensureIosFfi(f.options);
  assert.equal(f.builds(), 3);
});

test('generated binding drift fails the same gate and remote configuration never compiles', async (t) => {
  const f = fixture(t);
  await ensureIosFfi(f.options);
  f.put(resolve(f.composeRoot, 'iosApp/iosApp/UniffiGenerated/mobile_uniffi.swift'), 'old binding');
  assert.throws(() => verifyIosFfi(f), /stale/);
  f.put(resolve(f.coreRoot, 'meta-secret/core/src/node/app/sync/environment.rs'), 'const SELECTED_SERVER_ENVIRONMENT: ServerEnvironment = ServerEnvironment::Remote;');
  await assert.rejects(ensureIosFfi(f.options), /local environment/);
  assert.equal(f.builds(), 1);
});

test('Xcode must select simulator archive; prior bundle is invalidated and product drift prevents installation', async (t) => {
  const f = fixture(t);
  await ensureIosFfi(f.options);
  const derivedDataPath = resolve(f.coreRoot, 'e2eTest/.derivedData/iosApp');
  f.put(resolve(derivedDataPath, 'stale-marker'));
  const archive = resolve(f.composeRoot, 'iosApp/Libs/libmetasecret_mobile-ios-simulator-arm64.a');
  const runAndCapture = async () => ({ stdout: JSON.stringify([{ target: 'iosApp', buildSettings: { OTHER_LDFLAGS: archive } }]) });
  const inputs = await prepareIosXcodeBuild({ ...f, derivedDataPath, runAndCapture });
  assert.equal(existsSync(resolve(derivedDataPath, 'stale-marker')), false);
  f.put(resolve(derivedDataPath, 'Build/Products/Debug-iphonesimulator/iosApp.app/iosApp'), 'app executable');
  f.put(resolve(derivedDataPath, 'Build/Products/Debug-iphonesimulator/iosAppUITests-Runner.app/runner'), 'test executable');
  recordIosXcodeBuild({ ...f, derivedDataPath, inputs });
  assert.equal(verifyIosXcodeBuild({ derivedDataPath }).linkerSettings.simulatorArchive, archive);
  const manifest = JSON.parse(readFileSync(resolve(derivedDataPath, 'ios-core-link-provenance.json')));
  assert.notEqual(manifest.apps['iosApp.app'], manifest.inputs.native.simulatorArchiveSha256);
  f.put(resolve(derivedDataPath, 'Build/Products/Debug-iphonesimulator/iosApp.app/iosApp'), 'unrelated old binary');
  assert.throws(() => verifyIosXcodeBuild({ derivedDataPath }), /no longer matches/);
  await assert.rejects(prepareIosXcodeBuild({ ...f, derivedDataPath, runAndCapture: async () => ({ stdout: JSON.stringify([{ target: 'iosApp', buildSettings: { OTHER_LDFLAGS: 'wrong.a' } }]) }) }), /linker settings/);
});
