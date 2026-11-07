import { requireIosIdentityBuild } from './ios-test-identity.mjs';
import { verifyPlatformProductReceipt } from './batch-product-consumer.mjs';
import { prepareIosXcodeBuild, recordIosXcodeBuild, verifyIosXcodeBuild } from './ios-ffi-provenance.mjs';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { updateReadinessReport } from './platform-supervisor.mjs';

function installationClock() {
  try { const value = globalThis.performance.now(); return Number.isFinite(value) ? value : null; }
  catch { return null; }
}
function installationElapsed(start) {
  const end = installationClock();
  return start !== null && end !== null && end >= start ? end - start : -1;
}

function verifySelectedRuntime({ platform, identity }) {
  if (!process.env.E2E_READINESS_PATH) return;
  const report = JSON.parse(readFileSync(process.env.E2E_READINESS_PATH, 'utf8'));
  const matches = report.platforms.filter((entry) => entry.platform === platform);
  if (matches.length !== 1 || (platform === 'Android' ? matches[0].serial : matches[0].udid) !== identity) throw new Error(`Selected ${platform} runtime identity does not match parent readiness`);
}

export async function installAndVerifyAndroidE2eApps({
  serial,
  appBundleId,
  instrumentationBundleId = `${appBundleId}.test`,
  composeRoot,
  runAndWait,
  runAndCapture,
  prebuiltApks = null,
  verifiedBuild,
}) {
  const installStarted = installationClock();
  if (!serial || !appBundleId || !composeRoot) {
    throw new Error('Android readiness requires a device serial, app package, and Compose root');
  }
  verifySelectedRuntime({ platform: 'Android', identity: serial });
  if (prebuiltApks) {
    if (!existsSync(prebuiltApks.app) || !existsSync(prebuiltApks.test)) throw new Error('Verified prebuilt Android app/test APKs are missing');
    verifyPlatformProductReceipt({ verifiedBuild, paths: prebuiltApks, platform: 'android' });
    await runAndWait('adb', ['-s', serial, 'install', '-r', prebuiltApks.app]);
    await runAndWait('adb', ['-s', serial, 'install', '-r', prebuiltApks.test]);
  } else {
    await runAndWait('./gradlew', [':composeApp:installDebug', ':composeApp:installDebugAndroidTest'], {
      cwd: composeRoot,
      env: { ANDROID_SERIAL: serial },
    });
  }
  for (const bundle of [appBundleId, instrumentationBundleId]) {
    const { stdout } = await runAndCapture('adb', ['-s', serial, 'shell', 'pm', 'path', bundle]);
    if (!stdout.split(/\r?\n/).some((line) => /^package:\//.test(line))) throw new Error(`Android app ${bundle} is not installed on ${serial}`);
  }
  const { stdout: instrumentation } = await runAndCapture('adb', ['-s', serial, 'shell', 'pm', 'list', 'instrumentation']);
  if (!instrumentation.split(/\r?\n/).some((line) => line.trim() === `instrumentation:${instrumentationBundleId}/androidx.test.runner.AndroidJUnitRunner (target=${appBundleId})`)) {
    throw new Error(`Android instrumentation ${instrumentationBundleId} is not registered on ${serial} for ${appBundleId}`);
  }
  updateReadinessReport({
    platform: 'Android',
    checks: { serial, appBundleId, appInstalled: true, instrumentationBundleId, instrumentationInstalled: true,
      installDurationMs: installationElapsed(installStarted) },
  });
  return { serial, appBundleId, instrumentationBundleId, appInstalled: true, instrumentationInstalled: true };
}

async function readIosBundleIdentifier(appPath, runAndCapture) {
  const infoPlistPath = resolve(appPath, 'Info.plist');
  if (!existsSync(infoPlistPath)) throw new Error(`iOS app is missing Info.plist: ${appPath}`);
  const { stdout } = await runAndCapture('/usr/libexec/PlistBuddy', [
    '-c', 'Print :CFBundleIdentifier', infoPlistPath,
  ]);
  const bundleIdentifier = stdout.trim();
  if (!bundleIdentifier) throw new Error(`iOS app bundle identifier is empty: ${appPath}`);
  return bundleIdentifier;
}

async function listIosAppProducts(productsPath, runAndCapture) {
  if (!existsSync(productsPath)) throw new Error(`iOS build products directory does not exist: ${productsPath}`);
  const productNames = readdirSync(productsPath, { withFileTypes: true })
    .filter((entry) => entry.isDirectory() && entry.name.endsWith('.app'))
    .map((entry) => entry.name);
  const products = [];
  for (const name of productNames) {
    const appPath = resolve(productsPath, name);
    products.push({ path: appPath, bundleIdentifier: await readIosBundleIdentifier(appPath, runAndCapture) });
  }
  return products;
}

export async function installBuiltIosTestApps({
  simulatorUdid,
  derivedDataPath,
  appBundleId,
  runAndWait,
  runAndCapture,
  verifiedBuild,
  nativeBuild = { verify: verifyIosXcodeBuild },
}) {
  const installStarted = installationClock();
  if (!simulatorUdid || !derivedDataPath || !appBundleId) {
    throw new Error('iOS readiness requires a simulator UDID, DerivedData path, and app bundle identifier');
  }
  verifySelectedRuntime({ platform: 'iOS', identity: simulatorUdid });
  if (verifiedBuild) verifyPlatformProductReceipt({ verifiedBuild, paths: { derivedDataPath }, platform: 'ios' });
  nativeBuild.verify({ derivedDataPath });
  const productsPath = resolve(derivedDataPath, 'Build/Products/Debug-iphonesimulator');
  const products = await listIosAppProducts(productsPath, runAndCapture);
  const app = products.find((product) => product.bundleIdentifier === appBundleId);
  const testRunner = products.find((product) => /UITests-Runner\.app$/i.test(product.path));
  if (!app) throw new Error(`Built iOS app ${appBundleId} was not found in ${productsPath}`);
  if (!testRunner) throw new Error(`Built iOS UI test runner was not found in ${productsPath}`);
  const marker = (await runAndCapture('/usr/libexec/PlistBuddy', ['-c', 'Print :MetaSecretE2EIdentityRequired', resolve(app.path, 'Info.plist')])).stdout.trim();
  requireIosIdentityBuild(app.path, { readMarker: () => marker, verifySignature: () => {} });
  await runAndCapture('/usr/bin/codesign', ['--verify', '--strict', '--deep', app.path]);

  for (const product of [app, testRunner]) {
    await runAndWait('xcrun', ['simctl', 'install', simulatorUdid, product.path]);
    await runAndWait('xcrun', ['simctl', 'get_app_container', simulatorUdid, product.bundleIdentifier]);
  }
  updateReadinessReport({
    platform: 'iOS',
    checks: {
      simulatorUdid,
      appBundleId: app.bundleIdentifier,
      appInstalled: true,
      testRunnerBundleId: testRunner.bundleIdentifier,
      testRunnerInstalled: true,
      installDurationMs: installationElapsed(installStarted),
    },
  });
  return {
    simulatorUdid,
    appBundleId: app.bundleIdentifier,
    appInstalled: true,
    testRunnerBundleId: testRunner.bundleIdentifier,
    testRunnerInstalled: true,
    productsPath,
  };
}

export async function prepareIosTestApps({
  simulatorUdid,
  projectPath,
  scheme = 'iosApp',
  configuration = 'Debug',
  derivedDataPath,
  appBundleId,
  testSelector,
  buildSettings = [],
  cwd,
  env,
  runAndWait,
  runAndCapture,
  verifiedBuild,
  nativeBuild = { verify: verifyIosXcodeBuild },
}) {
  if (verifiedBuild) return installBuiltIosTestApps({ simulatorUdid, derivedDataPath, appBundleId, runAndWait, runAndCapture, verifiedBuild, nativeBuild });
  const composeRoot = resolve(projectPath, '../..');
  const coreRoot = resolve(composeRoot, '../meta-secret-core');
  const nativeInputs = await prepareIosXcodeBuild({ coreRoot, composeRoot, derivedDataPath, runAndCapture, identityRequired: true });
  const args = [
    'build-for-testing',
    '-sdk', 'iphonesimulator', 'ARCHS=arm64', 'ONLY_ACTIVE_ARCH=YES',
    '-project', projectPath,
    '-scheme', scheme,
    '-configuration', configuration,
    '-destination', `id=${simulatorUdid}`,
    '-derivedDataPath', derivedDataPath,
  ];
  if (testSelector) args.push(`-only-testing:${testSelector}`);
  if (buildSettings.some(value => value.startsWith('METASECRET_E2E_IDENTITY_REQUIRED='))) throw Error('Identity marker is owned by the E2E builder');
  args.push(...buildSettings, 'METASECRET_E2E_IDENTITY_REQUIRED=YES');
  await runAndWait('xcodebuild', args, { cwd, env });
  recordIosXcodeBuild({ coreRoot, composeRoot, derivedDataPath, inputs: nativeInputs });
  return installBuiltIosTestApps({
    simulatorUdid,
    derivedDataPath,
    appBundleId,
    runAndWait,
    runAndCapture,
  });
}
