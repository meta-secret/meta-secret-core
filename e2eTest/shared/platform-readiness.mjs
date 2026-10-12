import { prepareIosXcodeBuild, recordIosXcodeBuild, verifyIosXcodeBuild } from './ios-ffi-provenance.mjs';
import { existsSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { updateReadinessReport } from './platform-supervisor.mjs';

export async function installAndVerifyAndroidE2eApps({
  serial,
  appBundleId,
  instrumentationBundleId = `${appBundleId}.test`,
  composeRoot,
  runAndWait,
  runAndCapture,
  prebuiltApks = null,
}) {
  if (!serial || !appBundleId || !composeRoot) {
    throw new Error('Android readiness requires a device serial, app package, and Compose root');
  }
  if (prebuiltApks) {
    if (!existsSync(prebuiltApks.app) || !existsSync(prebuiltApks.test)) throw new Error('Verified prebuilt Android app/test APKs are missing');
    await runAndWait('adb', ['-s', serial, 'install', '-r', prebuiltApks.app]);
    await runAndWait('adb', ['-s', serial, 'install', '-r', prebuiltApks.test]);
  } else {
    await runAndWait('./gradlew', [':composeApp:installDebug', ':composeApp:installDebugAndroidTest'], {
      cwd: composeRoot,
      env: { ANDROID_SERIAL: serial },
    });
  }
  const { stdout: appPath } = await runAndCapture('adb', ['-s', serial, 'shell', 'pm', 'path', appBundleId]);
  if (!appPath.includes('package:')) {
    throw new Error(`Android app ${appBundleId} is not installed on ${serial}`);
  }
  const { stdout: instrumentation } = await runAndCapture('adb', ['-s', serial, 'shell', 'pm', 'list', 'instrumentation']);
  if (!instrumentation.includes(instrumentationBundleId)) {
    throw new Error(`Android instrumentation ${instrumentationBundleId} is not registered on ${serial}`);
  }
  updateReadinessReport({
    platform: 'Android',
    checks: { serial, appBundleId, appInstalled: true, instrumentationBundleId, instrumentationInstalled: true },
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
}) {
  if (!simulatorUdid || !derivedDataPath || !appBundleId) {
    throw new Error('iOS readiness requires a simulator UDID, DerivedData path, and app bundle identifier');
  }
  verifyIosXcodeBuild({ derivedDataPath });
  const productsPath = resolve(derivedDataPath, 'Build/Products/Debug-iphonesimulator');
  const products = await listIosAppProducts(productsPath, runAndCapture);
  const app = products.find((product) => product.bundleIdentifier === appBundleId);
  const testRunner = products.find((product) => /UITests-Runner\.app$/i.test(product.path));
  if (!app) throw new Error(`Built iOS app ${appBundleId} was not found in ${productsPath}`);
  if (!testRunner) throw new Error(`Built iOS UI test runner was not found in ${productsPath}`);

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
}) {
  const composeRoot = resolve(projectPath, '../..');
  const coreRoot = resolve(composeRoot, '../meta-secret-core');
  const nativeInputs = await prepareIosXcodeBuild({ coreRoot, composeRoot, derivedDataPath, runAndCapture });
  const args = [
    'build-for-testing',
    '-project', projectPath,
    '-scheme', scheme,
    '-configuration', configuration,
    '-destination', `id=${simulatorUdid}`,
    '-derivedDataPath', derivedDataPath,
  ];
  if (testSelector) args.push(`-only-testing:${testSelector}`);
  args.push(...buildSettings);
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
