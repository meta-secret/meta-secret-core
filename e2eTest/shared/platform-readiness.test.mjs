import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import test from 'node:test';
import { provenanceManifestPath, recordArtifactProvenance, verifyArtifactProvenance } from './build-provenance-manifest.mjs';
import { hashIosTree } from './ios-ffi-provenance.mjs';
import { installAndVerifyAndroidE2eApps, prepareIosTestApps } from './platform-readiness.mjs';

function androidFixture() {
  const directory = mkdtempSync(resolve(tmpdir(), 'e2e-prebuilt-readiness-'));
  const apks = { app: resolve(directory, 'app.apk'), test: resolve(directory, 'test.apk') };
  writeFileSync(apks.app, 'fixture-app-with-current-ffi'); writeFileSync(apks.test, 'fixture-test');
  const hash = (path) => createHash('sha256').update(readFileSync(path)).digest('hex');
  const current = { version: 3, sourceFingerprint: 'core+compose-current', scope: ['native'],
    configuration: { name: 'defaultLocal', environment: 'local', socketUrl: '', e2eServerUrl: '' },
    target: { platform: 'android', buildType: 'debug', package: 'metasecret.project.com', abis: ['arm64-v8a', 'armeabi-v7a', 'x86_64'] },
    artifacts: { androidAppApk: hash(apks.app), androidTestApk: hash(apks.test), androidFfiSourceFingerprint: 'core-current',
      androidJniLibs: 'jni-current', androidPackagedAbis: { 'arm64-v8a': 'ffi-arm64', 'armeabi-v7a': 'ffi-v7', x86_64: 'ffi-x64' },
      iosBuild: 'ios-full-app+XCTest+link', wasm: 'wasm-current', serverImageId: 'image-current' },
  };
  const path = provenanceManifestPath(directory, 'native');
  recordArtifactProvenance(path, current, 'fixture completed owning build receipt');
  const previousReadinessPath = process.env.E2E_READINESS_PATH;
  delete process.env.E2E_READINESS_PATH;
  const calls = [];
  const options = { serial: 'emulator-5554', appBundleId: 'metasecret.project.com', composeRoot: directory,
    prebuiltApks: apks, verifiedBuild: { path, current },
    runAndWait: async (file, args) => { calls.push([file, ...args]); },
    runAndCapture: async (file, args) => {
      calls.push([file, ...args]);
      return { stdout: args.includes('path') ? 'package:/data/app/current/base.apk\n'
        : 'instrumentation:metasecret.project.com.test/androidx.test.runner.AndroidJUnitRunner (target=metasecret.project.com)\n' };
    },
  };
  return { directory, apks, current, path, calls, options, cleanup: () => {
    if (previousReadinessPath === undefined) delete process.env.E2E_READINESS_PATH;
    else process.env.E2E_READINESS_PATH = previousReadinessPath;
    rmSync(directory, { recursive: true, force: true });
  } };
}

test('F2 current: existing proof authority rejects explicit configuration and target mismatches even with identical product hashes', () => {
  const f = androidFixture();
  try {
    assert.doesNotThrow(() => verifyArtifactProvenance({ path: f.path, current: f.current }));
    for (const changed of [
      { ...f.current, configuration: { ...f.current.configuration, name: 'network16', socketUrl: 'http://10.0.2.2:5181', e2eServerUrl: 'http://10.0.2.2:5181' } },
      { ...f.current, configuration: { ...f.current.configuration, name: 'deletion50', socketUrl: 'http://10.0.2.2:5201', e2eServerUrl: 'http://10.0.2.2:5201' } },
      { ...f.current, target: { ...f.current.target, buildType: 'release' } },
      { ...f.current, target: { ...f.current.target, package: 'foreign.app' } },
      { ...f.current, target: { ...f.current.target, abis: ['x86_64'] } },
      { ...f.current, configuration: undefined }, { ...f.current, target: undefined },
    ]) assert.throws(() => verifyArtifactProvenance({ path: f.path, current: changed }), /provenance mismatch|configuration|target/i);
  } finally { f.cleanup(); }
});

test('F2 current: verified Android prebuilt readiness installs the selected app and test pair then verifies the selected serial', async () => {
  const f = androidFixture();
  try {
    const result = await installAndVerifyAndroidE2eApps(f.options);
    assert.deepEqual(f.calls, [
      ['adb', '-s', 'emulator-5554', 'install', '-r', f.apks.app],
      ['adb', '-s', 'emulator-5554', 'install', '-r', f.apks.test],
      ['adb', '-s', 'emulator-5554', 'shell', 'pm', 'path', 'metasecret.project.com'],
      ['adb', '-s', 'emulator-5554', 'shell', 'pm', 'path', 'metasecret.project.com.test'],
      ['adb', '-s', 'emulator-5554', 'shell', 'pm', 'list', 'instrumentation'],
    ]);
    assert.equal(result.appInstalled, true); assert.equal(result.instrumentationInstalled, true);
    assert(!f.calls.some(([file]) => /gradle|cargo|xcodebuild|task/.test(file)));
  } finally { f.cleanup(); }
});

test('F2 current: absent, stale, wrong-route, wrong-target and mutated Android proof fails before install or actor work', async () => {
  for (const kind of ['absent', 'source', 'route', 'target', 'abi', 'app', 'test', 'substitutedPair']) {
    const f = androidFixture(); const current = structuredClone(f.current); let prebuiltApks = f.apks;
    if (kind === 'absent') rmSync(f.path);
    if (kind === 'source') current.sourceFingerprint = 'foreign-source';
    if (kind === 'route') current.configuration.socketUrl = 'http://10.0.2.2:5181';
    if (kind === 'target') current.target.package = 'foreign.app';
    if (kind === 'abi') current.artifacts.androidPackagedAbis['arm64-v8a'] = 'foreign-ffi';
    if (kind === 'app' || kind === 'test') writeFileSync(f.apks[kind], `mutated-${kind}`);
    if (kind === 'substitutedPair') {
      const alternate = resolve(f.directory, 'other-test.apk'); writeFileSync(alternate, 'foreign-test');
      prebuiltApks = { ...f.apks, test: alternate };
    }
    try {
      await assert.rejects(installAndVerifyAndroidE2eApps({ ...f.options, prebuiltApks, verifiedBuild: { path: f.path, current } }),
        /proof|provenance|configuration|target|APK|artifact|route|source|manifest/i, kind);
      assert.deepEqual(f.calls, [], `${kind} must fail before installation`);
    } finally { f.cleanup(); }
  }
});

test('F2 current: Android prebuilt readiness still rejects missing app installation and foreign instrumentation', async () => {
  for (const failure of ['app', 'instrumentation', 'instrumentationTarget']) {
    const f = androidFixture();
    try {
      await assert.rejects(installAndVerifyAndroidE2eApps({ ...f.options,
        runAndCapture: async (_file, args) => ({ stdout: args.includes('path')
          ? failure === 'app' ? '' : 'package:/data/app/current/base.apk'
          : failure === 'instrumentationTarget'
            ? 'instrumentation:metasecret.project.com.test/androidx.test.runner.AndroidJUnitRunner (target=foreign.app)'
            : 'instrumentation:foreign.app.test/androidx.test.runner.AndroidJUnitRunner (target=foreign.app)' }),
      }), failure === 'app' ? /not installed/ : /not registered/);
      assert.equal(f.calls.filter((call) => call.includes('install')).length, 2);
    } finally { f.cleanup(); }
  }
});

test('F2 current: standalone Android readiness retains the owning Gradle fallback when prebuilt reuse is not requested', async () => {
  const f = androidFixture();
  try {
    const { prebuiltApks, verifiedBuild, ...standalone } = f.options;
    await installAndVerifyAndroidE2eApps(standalone);
    assert.deepEqual(f.calls[0], ['./gradlew', ':composeApp:installDebug', ':composeApp:installDebugAndroidTest']);
  } finally { f.cleanup(); }
});

test('F2 selected runtime: wrong parent serial rejects before installing an otherwise proven pair', async () => {
  const f = androidFixture();
  try {
    const path = resolve(f.directory, 'readiness.json'); writeFileSync(path, JSON.stringify({ platforms: [{ platform: 'Android', serial: 'foreign-serial' }] }));
    process.env.E2E_READINESS_PATH = path;
    await assert.rejects(installAndVerifyAndroidE2eApps(f.options), /runtime identity.*parent readiness/);
    assert.deepEqual(f.calls, []);
  } finally { f.cleanup(); }
});

// Deferred until Stage4b. Supplying the grouped native seam must bypass only the
// compiler; both real .app products and exact selected UDID are still checked.
test('F2 deferred: proven iOS reuse verifies full source/product/link proof and installs app plus XCTest without compiling', async () => {
  for (const mutation of [null, 'app', 'test', 'source', 'link', 'marker', 'signature']) {
    const directory = mkdtempSync(resolve(tmpdir(), 'e2e-prebuilt-ios-')); const calls = [];
    const derivedDataPath = resolve(directory, 'derived'); const products = resolve(derivedDataPath, 'Build/Products/Debug-iphonesimulator');
    const app = resolve(products, 'iosApp.app'); const runner = resolve(products, 'iosAppUITests-Runner.app');
    for (const path of [app, runner]) { mkdirSync(path, { recursive: true }); writeFileSync(resolve(path, 'Info.plist'), 'fixture plist'); }
    const link = resolve(directory, 'expanded-link-inputs'); writeFileSync(link, 'current local archive+bindings+PBX');
    const fileHash = (path) => createHash('sha256').update(readFileSync(path)).digest('hex');
    const current = { version: 3, sourceFingerprint: 'full-core+compose+ios-app+XCTest-source',
      configuration: { environment: 'local', configuration: 'Debug' }, target: { platform: 'ios', sdk: 'iphonesimulator', arch: 'arm64' },
      artifacts: { iosProducts: { 'iosApp.app': hashIosTree(app), 'iosAppUITests-Runner.app': hashIosTree(runner) }, iosBuild: fileHash(link) } };
    const path = provenanceManifestPath(directory, 'native'); recordArtifactProvenance(path, current, 'fixture completed owning iOS build receipt');
    let linkProofChecks = 0;
    const options = { simulatorUdid: 'exact-ios-udid', projectPath: resolve(directory, 'iosApp/iosApp.xcodeproj'),
      derivedDataPath, appBundleId: 'org.metasecret.vault', testSelector: 'iosAppUITests/CaseOneJoinFromIosUITest/testJoinApprovedVaultAndShowSecret',
      verifiedBuild: { path, current },
      nativeBuild: { verify: () => { linkProofChecks++; if (fileHash(link) !== current.artifacts.iosBuild) throw new Error('iOS link proof mismatch'); },
        prepare: () => { throw new Error('Reuse cannot prepare a build'); }, record: () => { throw new Error('Reuse cannot mint link proof'); } },
      runAndWait: async (file, args) => { calls.push([file, ...args]); },
      runAndCapture: async (file, args) => {
        calls.push([file, ...args]);
        if (file === '/usr/bin/codesign') {
          if (mutation === 'signature') throw Error('Controlled signature mismatch');
          return { stdout: '' };
        }
        assert.equal(file, '/usr/libexec/PlistBuddy');
        if (args.includes('Print :MetaSecretE2EIdentityRequired')) return { stdout: mutation === 'marker' ? 'NO' : 'YES' };
        return { stdout: args.at(-1).includes('UITests-Runner') ? 'org.metasecret.vault.uitests.xctrunner\n' : 'org.metasecret.vault\n' };
      },
    };
    if (mutation === 'app' || mutation === 'test') writeFileSync(resolve(mutation === 'app' ? app : runner, 'Info.plist'), 'mutated product');
    if (mutation === 'source') options.verifiedBuild.current = { ...current, sourceFingerprint: 'foreign source' };
    if (mutation === 'link') writeFileSync(link, 'foreign archive linkage');
    try {
      if (mutation) {
        await assert.rejects(prepareIosTestApps(options), /proof|provenance|product|link|mismatch|marker/i);
        if (['marker', 'signature'].includes(mutation)) assert(!calls.some(call => call.includes('install')));
        else assert.deepEqual(calls, [], `${mutation} must fail before installation or a compiler`);
      } else {
        await prepareIosTestApps(options); assert.equal(linkProofChecks, 1);
        assert(!calls.some(([file]) => /xcodebuild|gradle|cargo|task/.test(file)));
        assert.deepEqual(calls.filter((call) => call[2] === 'install'), [
          ['xcrun', 'simctl', 'install', 'exact-ios-udid', app], ['xcrun', 'simctl', 'install', 'exact-ios-udid', runner],
        ]);
        assert.equal(calls.filter((call) => call[2] === 'get_app_container' && call[3] === 'exact-ios-udid').length, 2);
      }
    } finally { rmSync(directory, { recursive: true, force: true }); }
  }
});
