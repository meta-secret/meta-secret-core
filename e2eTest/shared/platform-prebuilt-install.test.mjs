import { createHash } from 'node:crypto';
import { recordArtifactProvenance } from './build-provenance-manifest.mjs';
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, readFileSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { runInNewContext } from 'node:vm';
import { installAndVerifyAndroidE2eApps } from './platform-readiness.mjs';

test('prebuilt Android installation never reruns Gradle or changes proven APK bytes', async () => {
  const root = mkdtempSync(resolve(tmpdir(), 'prebuilt-install-')); const app = resolve(root, 'app.apk'); const instrumentation = resolve(root, 'test.apk');
  writeFileSync(app, 'compiled-proxy-app'); writeFileSync(instrumentation, 'compiled-tests');
  const calls = [];
  const current = { version: 3, sourceFingerprint: 'fixture-current-source', configuration: { environment: 'local' },
    target: { platform: 'android', package: 'test.app' }, artifacts: { androidAppApk: createHash('sha256').update(readFileSync(app)).digest('hex'),
      androidTestApk: createHash('sha256').update(readFileSync(instrumentation)).digest('hex') } };
  const path = resolve(root, 'owning-build-proof.json'); recordArtifactProvenance(path, current, 'fixture completed owning builder receipt');
  try {
    await installAndVerifyAndroidE2eApps({ serial: 'owned-serial', appBundleId: 'test.app', composeRoot: root,
      prebuiltApks: { app, test: instrumentation }, verifiedBuild: { path, current },
      runAndWait: async (command, args) => calls.push([command, ...args]),
      runAndCapture: async (_, args) => ({ stdout: args.includes('instrumentation') ? 'instrumentation:test.app.test/androidx.test.runner.AndroidJUnitRunner (target=test.app)' : 'package:/owned/base.apk' }),
    });
    assert.deepEqual(calls, [['adb', '-s', 'owned-serial', 'install', '-r', app], ['adb', '-s', 'owned-serial', 'install', '-r', instrumentation]]);
    assert.equal(readFileSync(app, 'utf8'), 'compiled-proxy-app'); assert.equal(readFileSync(instrumentation, 'utf8'), 'compiled-tests');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('missing prebuilt product fails before any install/rebuild; legacy install still uses its existing tasks', async () => {
  const calls = []; const options = { serial: 'serial', appBundleId: 'test.app', composeRoot: '/fixture', runAndWait: async (command, args) => calls.push([command, ...args]), runAndCapture: async (_, args) => ({ stdout: args.includes('instrumentation') ? 'instrumentation:test.app.test/androidx.test.runner.AndroidJUnitRunner (target=test.app)' : 'package:/base.apk' }) };
  await assert.rejects(installAndVerifyAndroidE2eApps({ ...options, prebuiltApks: { app: '/missing.apk', test: '/missing-test.apk' } }), /missing/); assert.equal(calls.length, 0);
  await installAndVerifyAndroidE2eApps(options); assert.deepEqual(calls[0], ['./gradlew', ':composeApp:installDebug', ':composeApp:installDebugAndroidTest']);
});

test('Test50 native startup enforces compiled and actual Core override before any business method', () => {
  const source = readFileSync(new URL('../../../meta-secret-compose/composeApp/src/androidTest/kotlin/metasecret/project/com/CaseEightAndroidBothOfflineTest.kt', import.meta.url), 'utf8');
  assert(source.includes('@Before\n    fun requireTest50CoreRoutingBeforeBusiness()'));
  assert(source.includes('BuildConfig.META_SECRET_E2E_SERVER_URL == required'));
  assert(source.includes('Os.getenv("METASECRET_E2E_SERVER_URL") == required'));
  const harness = readFileSync(new URL('../Test4/orchestrator.mjs', import.meta.url), 'utf8');
  assert(harness.includes("prebuiltApks: reuseProvenArtifacts || scenario.mode === 'vault-secret-deletion' ? { app: androidAppApkPath, test: androidTestApkPath } : null"));
  assert(harness.includes("verifiedBuild: batchProducts?.android"));
  assert(harness.includes("androidAppApkPath = batchProducts.android.products.app; androidTestApkPath = batchProducts.android.products.test;"));
  for (const entry of ['startAndroidJoinTest', 'startAndroidStepTest']) {
    const body = harness.slice(harness.indexOf('async function ' + entry + '('));
    const first = body.indexOf('  const runnerArguments = [');
    const last = body.indexOf(entry === 'startAndroidJoinTest'
      ? '  const androidNetworkGradleArgs =' : '  if (decision) {', first);
    assert(first >= 0 && last > first, 'Actual Android argument builder must exist: ' + entry);
    for (const mode of ['vault-secret-deletion', 'ordinary']) {
      const args = runInNewContext(body.slice(first, last) + '\n' +
        (entry === 'startAndroidJoinTest' ? 'directRunnerArguments' : 'runnerArguments'), {
        scenario: { mode, vault: { name: 'controlled-vault' }, android: { setupOnly: false } },
        androidTestSelector: 'ControlledClass#controlledMethod', selector: 'ControlledClass#controlledMethod',
        declaredDevicePasscodes: {}, defaultSecret: { name: 'controlled-secret', value: 'controlled-value' },
        secretConfigs: { default: { name: 'controlled-secret', value: 'controlled-value' } },
        secretConfigJson: '{}', recoveryCycles: [], cyclePlanJson: '[]', secretCreationPlan: null,
        expectedSecretNames: [], directShowSecretNames: [], deletionAuthProfile: '', joinPlatforms: null,
        role: 'controlled-role', secretName: 'controlled-secret', cycle: 1, step: 1, repeatApprove: false, duplicateRecovery: false,
        approvalPlatform: '', sender: '', expectedOutcome: '', expectRequest: true,
        networkRole: '', networkBlock: '', networkCycles: '', joinTarget: '', secretValue: '',
        initializeShow: false, verifyReceiverAlert: false, receiverRequestAlreadySent: false,
        expectedDeviceNames: null, registrationCase: null, test15FifoContext: null, recoveryPhase: undefined,
        recoveryPhaseExpected: undefined,
        networkProxyUrl: platform => { assert.equal(platform, 'android'); return 'http://controlled-route'; },
      });
      const values = [];
      for (let at = 0; at < args.length; at += 3) {
        assert.equal(args[at], '-e', 'Canonical instrumentation triplet required');
        if (args[at + 1] === 'requiredE2eServerUrl') values.push(args[at + 2]);
      }
      assert.deepEqual(values, mode === 'vault-secret-deletion' ? ['http://controlled-route'] : [], entry);
    }
  }
  assert(harness.includes("['requiredE2eServerUrl', scenario.mode === 'vault-secret-deletion'"));
});
