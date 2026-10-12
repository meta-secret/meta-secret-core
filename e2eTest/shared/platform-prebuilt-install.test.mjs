import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, readFileSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { installAndVerifyAndroidE2eApps } from './platform-readiness.mjs';

test('prebuilt Android installation never reruns Gradle or changes proven APK bytes', async () => {
  const root = mkdtempSync(resolve(tmpdir(), 'prebuilt-install-')); const app = resolve(root, 'app.apk'); const instrumentation = resolve(root, 'test.apk');
  writeFileSync(app, 'compiled-proxy-app'); writeFileSync(instrumentation, 'compiled-tests');
  const calls = [];
  try {
    await installAndVerifyAndroidE2eApps({ serial: 'owned-serial', appBundleId: 'test.app', composeRoot: root,
      prebuiltApks: { app, test: instrumentation },
      runAndWait: async (command, args) => calls.push([command, ...args]),
      runAndCapture: async (_, args) => ({ stdout: args.includes('instrumentation') ? 'instrumentation:test.app.test/runner (target=test.app)' : 'package:/owned/base.apk' }),
    });
    assert.deepEqual(calls, [['adb', '-s', 'owned-serial', 'install', '-r', app], ['adb', '-s', 'owned-serial', 'install', '-r', instrumentation]]);
    assert.equal(readFileSync(app, 'utf8'), 'compiled-proxy-app'); assert.equal(readFileSync(instrumentation, 'utf8'), 'compiled-tests');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('missing prebuilt product fails before any install/rebuild; legacy install still uses its existing tasks', async () => {
  const calls = []; const options = { serial: 'serial', appBundleId: 'test.app', composeRoot: '/fixture', runAndWait: async (command, args) => calls.push([command, ...args]), runAndCapture: async (_, args) => ({ stdout: args.includes('instrumentation') ? 'test.app.test' : 'package:/base.apk' }) };
  await assert.rejects(installAndVerifyAndroidE2eApps({ ...options, prebuiltApks: { app: '/missing.apk', test: '/missing-test.apk' } }), /missing/); assert.equal(calls.length, 0);
  await installAndVerifyAndroidE2eApps(options); assert.deepEqual(calls[0], ['./gradlew', ':composeApp:installDebug', ':composeApp:installDebugAndroidTest']);
});

test('Test50 native startup enforces compiled and actual Core override before any business method', () => {
  const source = readFileSync(new URL('../../../meta-secret-compose/composeApp/src/androidTest/kotlin/metasecret/project/com/CaseEightAndroidBothOfflineTest.kt', import.meta.url), 'utf8');
  assert(source.includes('@Before\n    fun requireTest50CoreRoutingBeforeBusiness()'));
  assert(source.includes('BuildConfig.META_SECRET_E2E_SERVER_URL == required'));
  assert(source.includes('Os.getenv("METASECRET_E2E_SERVER_URL") == required'));
  const harness = readFileSync(new URL('../Test4/orchestrator.mjs', import.meta.url), 'utf8');
  assert(harness.includes("prebuiltApks: scenario.mode === 'vault-secret-deletion'"));
  assert.equal((harness.match(/'-e', 'requiredE2eServerUrl'/g) ?? []).length, 2);
  assert(harness.includes("['requiredE2eServerUrl', scenario.mode === 'vault-secret-deletion'"));
});
