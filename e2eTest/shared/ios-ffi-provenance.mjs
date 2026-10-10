import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, appendFileSync, readFileSync, readdirSync, readlinkSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';

export function hashIosTree(path, { ignoreXcodeUserData = false } = {}) {
  if (!existsSync(path)) throw new Error(`iOS provenance input missing: ${path}`);
  const digest = createHash('sha256');
  function visit(current, relative) {
    const entries = readdirSync(current, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name));
    for (const entry of entries) {
      if (ignoreXcodeUserData && entry.name === 'xcuserdata') continue;
      const child = resolve(current, entry.name);
      const name = `${relative}/${entry.name}`;
      if (entry.isDirectory()) visit(child, name);
      else if (entry.isFile()) { digest.update(`${name}\0`); digest.update(readFileSync(child)); }
      else if (entry.isSymbolicLink()) { digest.update(`${name}\0link:${readlinkSync(child)}`); }
      else throw new Error(`Unsupported iOS provenance input: ${child}`);
    }
  }
  visit(path, '');
  return digest.digest('hex');
}
const hashFile = (path) => {
  if (!existsSync(path)) throw new Error(`iOS provenance input missing: ${path}`);
  return createHash('sha256').update(readFileSync(path)).digest('hex');
};
export function iosArtifactRoots(composeRoot) {
  return ['iosApp/Libs', 'iosApp/iosApp/UniffiGenerated',
    'iosApp/iosApp/MetaSecretCoreService/UniffiGenerated'].map((path) => resolve(composeRoot, path));
}
export function fingerprintIosCoreSource(coreRoot) {
  const listed = spawnSync('git', ['ls-files', '-co', '--exclude-standard', '-z', '--', 'meta-secret'], { cwd: coreRoot, encoding: 'utf8' });
  if (listed.status !== 0) throw new Error(`Cannot enumerate iOS Core sources: ${listed.stderr}`);
  const digest = createHash('sha256');
  for (const name of listed.stdout.split('\0').filter(Boolean).sort()) {
    const path = resolve(coreRoot, name);
    digest.update(`${name}\0`);
    digest.update(existsSync(path) ? readFileSync(path) : '<deleted>');
  }
  return digest.digest('hex');
}
function nativeInputs(coreRoot, composeRoot) {
  return {
    version: 1, coreRoot, composeRoot,
    sourceFingerprint: fingerprintIosCoreSource(coreRoot),
    simulatorArchiveSha256: hashFile(resolve(composeRoot, 'iosApp/Libs/libmetasecret_mobile-ios-simulator-arm64.a')),
    deviceArchiveSha256: hashFile(resolve(composeRoot, 'iosApp/Libs/libmetasecret_mobile-ios-arm64.a')),
    generatedBindings: Object.fromEntries(iosArtifactRoots(composeRoot).slice(1).map((path) => [path, hashIosTree(path)])),
  };
}
const nativeManifest = (coreRoot) => resolve(coreRoot, 'e2eTest/.artifacts/ios-ffi-build-provenance.json');
export function assertIosProvenanceMatches(expected, actual) {
  if (!expected || expected.version !== 1 || JSON.stringify(expected) !== JSON.stringify(actual)) {
    throw new Error('iOS Core source/archive/generated binding provenance is missing or stale; rebuild before platform startup');
  }
}
export function verifyIosFfi({ coreRoot, composeRoot }) {
  const path = nativeManifest(coreRoot);
  if (!existsSync(path)) throw new Error(`iOS Core provenance missing: ${path}`);
  const expected = JSON.parse(readFileSync(path, 'utf8')).inputs;
  const actual = nativeInputs(coreRoot, composeRoot);
  assertIosProvenanceMatches(expected, actual);
  return actual;
}
export async function ensureIosFfi({ coreRoot, composeRoot, runAndWait, evidenceDirectory, onEvent = console.log }) {
  const environment = readFileSync(resolve(coreRoot, 'meta-secret/core/src/node/app/sync/environment.rs'), 'utf8');
  if (!/SELECTED_SERVER_ENVIRONMENT[^\n]*ServerEnvironment::Local/.test(environment)) {
    throw new Error('iOS Core preflight requires local environment before compiling; run task set-env -- local first');
  }
  const path = nativeManifest(coreRoot);
  let inputs;
  try { inputs = verifyIosFfi({ coreRoot, composeRoot }); } catch { /* A full native build is the only valid bootstrap. */ }
  if (!inputs) {
    const sourceBefore = fingerprintIosCoreSource(coreRoot);
    onEvent('🧭 Building iOS Core from current sources before simulator startup');
    const script = resolve(coreRoot, 'meta-secret/mobile/scripts/build-ios.sh');
    if (runAndWait) await runAndWait('bash', [script], { cwd: coreRoot });
    else await new Promise((done, fail) => {
      const logPath = evidenceDirectory && resolve(evidenceDirectory, 'ios-core-build.log');
      if (logPath) { mkdirSync(evidenceDirectory, { recursive: true }); writeFileSync(logPath, ''); }
      const child = spawn('bash', [script], { cwd: coreRoot, stdio: ['ignore', 'pipe', 'pipe'] });
      for (const [stream, output] of [[child.stdout, process.stdout], [child.stderr, process.stderr]]) {
        stream.on('data', (chunk) => { output.write(chunk); if (logPath) appendFileSync(logPath, chunk); });
      }
      child.once('error', fail);
      child.once('close', (code, signal) => code === 0 ? done() : fail(new Error(`iOS Core build failed: ${code ?? signal}`)));
    });
    inputs = nativeInputs(coreRoot, composeRoot);
    for (const [target, hash] of [['aarch64-apple-ios-sim', inputs.simulatorArchiveSha256], ['aarch64-apple-ios', inputs.deviceArchiveSha256]]) {
      if (hashFile(resolve(coreRoot, `meta-secret/target/${target}/release/libmetasecret_mobile.a`)) !== hash) {
        throw new Error(`iOS copied archive differs from freshly built Cargo output: ${target}`);
      }
    }
    if (inputs.sourceFingerprint !== sourceBefore) throw new Error('Core sources changed while building iOS FFI');
    // A fresh native build invalidates every cached Xcode bundle, including when the archive bytes coincide.
    mkdirSync(resolve(path, '..'), { recursive: true });
    writeFileSync(path, `${JSON.stringify({ inputs, builtAt: new Date().toISOString() }, null, 2)}\n`);
  }
  if (evidenceDirectory) {
    mkdirSync(evidenceDirectory, { recursive: true });
    writeFileSync(resolve(evidenceDirectory, 'ios-ffi-provenance.json'), readFileSync(path));
  }
  onEvent(`✅ iOS Core provenance verified source=${inputs.sourceFingerprint} simulatorArchive=${inputs.simulatorArchiveSha256}`);
  return inputs;
}
function xcodeInputs(coreRoot, composeRoot) {
  const project = resolve(composeRoot, 'iosApp/iosApp.xcodeproj');
  const schemes = resolve(project, 'xcshareddata/xcschemes');
  return {
    native: verifyIosFfi({ coreRoot, composeRoot }),
    project: {
      pbxproj: hashFile(resolve(project, 'project.pbxproj')),
      sharedSchemes: existsSync(schemes) ? hashIosTree(schemes) : null,
    },
  };
}
const appManifest = (derivedDataPath) => resolve(derivedDataPath, 'ios-core-link-provenance.json');
export async function prepareIosXcodeBuild({ coreRoot, composeRoot, derivedDataPath, runAndCapture }) {
  const inputs = xcodeInputs(coreRoot, composeRoot);
  const { stdout } = await runAndCapture('xcodebuild', ['-showBuildSettings', '-json', '-project', resolve(composeRoot, 'iosApp/iosApp.xcodeproj'), '-scheme', 'iosApp', '-configuration', 'Debug', '-sdk', 'iphonesimulator', '-destination', 'generic/platform=iOS Simulator'], { cwd: resolve(composeRoot, 'iosApp') });
  const settings = JSON.parse(stdout).find((entry) => entry.target === 'iosApp')?.buildSettings;
  const archive = resolve(composeRoot, 'iosApp/Libs/libmetasecret_mobile-ios-simulator-arm64.a');
  if (!settings?.OTHER_LDFLAGS?.includes(archive)) throw new Error('iOS Xcode simulator linker settings do not select the verified Core archive');
  const path = appManifest(derivedDataPath);
  let matches = false;
  if (existsSync(path)) {
    try { matches = JSON.stringify(JSON.parse(readFileSync(path, 'utf8')).inputs) === JSON.stringify(inputs); } catch { /* invalid cache */ }
  }
  if (!matches) rmSync(derivedDataPath, { recursive: true, force: true });
  return { ...inputs, linkerSettings: { simulatorArchive: archive, otherLdFlags: settings.OTHER_LDFLAGS } };
}
function iosAppProductHashes(derivedDataPath) {
  const products = resolve(derivedDataPath, 'Build/Products/Debug-iphonesimulator');
  if (!existsSync(products)) throw new Error('iOS build products missing');
  const apps = readdirSync(products).filter((name) => name.endsWith('.app')).sort();
  if (apps.length < 2) throw new Error('iOS app and XCTest runner products missing');
  return Object.fromEntries(apps.map((name) => [name, hashIosTree(resolve(products, name))]));
}
export function recordIosXcodeBuild({ coreRoot, composeRoot, derivedDataPath, inputs }) {
  const actual = xcodeInputs(coreRoot, composeRoot);
  const { linkerSettings, ...buildInputs } = inputs;
  if (JSON.stringify(buildInputs) !== JSON.stringify(actual)) throw new Error('iOS Core inputs changed during Xcode build');
  writeFileSync(appManifest(derivedDataPath), `${JSON.stringify({ inputs: actual, linkerSettings, apps: iosAppProductHashes(derivedDataPath), builtAt: new Date().toISOString() }, null, 2)}\n`);
}
export function verifyIosXcodeBuild({ derivedDataPath }) {
  const path = appManifest(derivedDataPath);
  if (!existsSync(path)) throw new Error('iOS Xcode Core link provenance missing; rebuild before installing');
  const expected = JSON.parse(readFileSync(path, 'utf8'));
  const { coreRoot, composeRoot } = expected.inputs.native;
  if (expected.linkerSettings?.simulatorArchive !== resolve(composeRoot, 'iosApp/Libs/libmetasecret_mobile-ios-simulator-arm64.a')
    || JSON.stringify(expected.inputs) !== JSON.stringify(xcodeInputs(coreRoot, composeRoot))
    || JSON.stringify(expected.apps) !== JSON.stringify(iosAppProductHashes(derivedDataPath))) {
    throw new Error('iOS Xcode bundle no longer matches its verified Core link inputs; rebuild before installing');
  }
  const runId = process.env.E2E_RUN_ID;
  if (runId && /^[a-zA-Z0-9_-]+$/.test(runId)) {
    const evidence = resolve(coreRoot, 'e2eTest/.artifacts/runs', runId);
    mkdirSync(evidence, { recursive: true });
    writeFileSync(resolve(evidence, `ios-xcode-test-${process.env.E2E_TEST_NUMBER ?? 'direct'}-attempt-${process.env.E2E_TEST_ATTEMPT ?? '1'}-${Date.now()}.json`), `${JSON.stringify(expected, null, 2)}\n`);
  }
  return expected;
}
