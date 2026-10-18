import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync, mkdirSync, writeFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fingerprintIosCoreSource, iosArtifactRoots, hashIosTree, verifyIosFfi } from '../shared/ios-ffi-provenance.mjs';

const receipts = new WeakSet();
const expectations = new WeakSet();
const environmentFile = 'meta-secret/core/src/node/app/sync/environment.rs';
const selector = 'pub const SELECTED_SERVER_ENVIRONMENT: ServerEnvironment = ServerEnvironment::';
const androidMarker = '[CORE_DIAG] sync_vault response events=';
const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');
const hashFile = (path) => {
  const bytes = readFileSync(path);
  if (!bytes.length) throw new Error(`Empty official-build output: ${path}`);
  return sha256(bytes);
};
const manifest = (coreRoot, name) => resolve(coreRoot, 'e2eTest/.artifacts', name);
const writeJson = (path, value) => { mkdirSync(dirname(path), { recursive: true }); writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`); };

function bindingHash(path, fileName) {
  let found = false;
  const visit = (directory) => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const child = resolve(directory, entry.name);
      if (entry.isDirectory()) visit(child);
      else if (entry.isFile() && entry.name === fileName) { hashFile(child); found = true; }
    }
  };
  visit(path);
  if (!found) throw new Error(`Generated binding missing: ${fileName}`);
  return hashIosTree(path);
}

function sourcePaths(coreRoot) {
  const result = spawnSync('git', ['ls-files', '-co', '--exclude-standard', '-z', '--', 'meta-secret'], { cwd: coreRoot, encoding: 'utf8' });
  if (result.status !== 0) throw new Error('Cannot enumerate frozen official-build sources');
  return result.stdout.split('\0').filter(Boolean).sort();
}

/** Capture before the command; allow only its exact Remote -> Local selector edit. */
export function captureExpectedLocalSource({ coreRoot }) {
  coreRoot = resolve(coreRoot);
  const files = {};
  const digest = createHash('sha256');
  for (const name of sourcePaths(coreRoot)) {
    const path = resolve(coreRoot, name);
    let bytes = existsSync(path) ? readFileSync(path) : Buffer.from('<deleted>');
    if (name === environmentFile) {
      const text = bytes.toString('utf8');
      const matches = text.match(/pub const SELECTED_SERVER_ENVIRONMENT: ServerEnvironment = ServerEnvironment::(?:Local|Remote);/g);
      if (matches?.length !== 1) throw new Error('Expected one known Core environment selector');
      bytes = Buffer.from(text.replace(`${selector}Remote;`, `${selector}Local;`));
    }
    files[name] = sha256(bytes);
    digest.update(`${name}\0`); digest.update(bytes);
  }
  if (!files[environmentFile]) throw new Error('Core environment selector source missing');
  const expected = Object.freeze({ coreRoot, files: Object.freeze(files), fingerprint: digest.digest('hex') });
  expectations.add(expected);
  return expected;
}

/** Only this awaited invocation can issue a receipt accepted by the recorder. */
export async function invokeOfficialLocalBuild({ coreRoot, runId, runCommand }) {
  if (!/^[a-zA-Z0-9_-]+$/.test(runId ?? '') || typeof runCommand !== 'function') throw new Error('Owned official build requires run ID and command runner');
  const command = '/opt/homebrew/bin/task';
  const args = ['set-env', '--', 'local'];
  const cwd = resolve(coreRoot);
  const startedAt = new Date().toISOString();
  const result = await runCommand(command, args, { cwd });
  if (result?.exitCode !== 0) throw new Error('Official local build did not return a successful exit receipt');
  const receipt = Object.freeze({ runId, command, args: Object.freeze(args), cwd, exitCode: 0, startedAt, finishedAt: new Date().toISOString() });
  receipts.add(receipt);
  return receipt;
}

function inputs(coreRoot, composeRoot) {
  const environment = readFileSync(resolve(coreRoot, environmentFile), 'utf8');
  if (!environment.includes(`${selector}Local;`)) throw new Error('Official checkpoint requires local Core configuration');
  const ios = {
    version: 1, coreRoot, composeRoot, sourceFingerprint: fingerprintIosCoreSource(coreRoot),
    simulatorArchiveSha256: hashFile(resolve(composeRoot, 'iosApp/Libs/libmetasecret_mobile-ios-simulator-arm64.a')),
    deviceArchiveSha256: hashFile(resolve(composeRoot, 'iosApp/Libs/libmetasecret_mobile-ios-arm64.a')),
    generatedBindings: Object.fromEntries(iosArtifactRoots(composeRoot).slice(1).map((path) => [path, bindingHash(path, 'mobile_uniffi.swift')])),
  };
  for (const [target, hash] of [['aarch64-apple-ios-sim', ios.simulatorArchiveSha256], ['aarch64-apple-ios', ios.deviceArchiveSha256]]) {
    if (hashFile(resolve(coreRoot, `meta-secret/target/${target}/release/libmetasecret_mobile.a`)) !== hash) throw new Error(`Copied iOS archive differs from official Cargo output: ${target}`);
  }
  const abiSha256 = {}, staticArchiveSha256 = {};
  for (const [abi, target] of [['arm64-v8a', 'aarch64-linux-android'], ['armeabi-v7a', 'armv7-linux-androideabi'], ['x86_64', 'x86_64-linux-android']]) {
    const bytes = readFileSync(resolve(composeRoot, `composeApp/src/androidMain/jniLibs/${abi}/libmetasecret_mobile.so`));
    if (!bytes.includes(Buffer.from(androidMarker))) throw new Error(`Official Android output lacks required diagnostic marker: ${abi}`);
    abiSha256[abi] = sha256(bytes);
    staticArchiveSha256[target] = hashFile(resolve(coreRoot, `meta-secret/target/${target}/release/libmetasecret_mobile.a`));
  }
  const androidBindings = resolve(composeRoot, 'composeApp/src/androidMain/kotlin/com/metasecret/core/uniffi');
  return { ios, android: { version: 1, sourceFingerprint: ios.sourceFingerprint, diagnosticMarker: androidMarker, abiSha256 },
    androidBindingSha256: bindingHash(androidBindings, 'mobile_uniffi.kt'), androidStaticArchiveSha256: staticArchiveSha256 };
}

export function recordOfficialNativeBuild({ coreRoot, composeRoot, expectedSource, receipt, checkpointPath }) {
  coreRoot = resolve(coreRoot); composeRoot = resolve(composeRoot);
  if (!receipts.has(receipt) || receipt.cwd !== coreRoot || receipt.exitCode !== 0) throw new Error('Missing owned successful official-build receipt');
  if (!expectations.has(expectedSource) || expectedSource.coreRoot !== coreRoot) throw new Error('Missing original frozen source expectation');
  if (fingerprintIosCoreSource(coreRoot) !== expectedSource.fingerprint) throw new Error('Unrelated source drift during official local build');
  const current = inputs(coreRoot, composeRoot);
  const checkpoint = { version: 1, receipt, inputs: current };
  writeJson(manifest(coreRoot, 'ios-ffi-build-provenance.json'), { inputs: current.ios, builtAt: receipt.finishedAt });
  writeJson(manifest(coreRoot, 'android-ffi-build-provenance.json'), { ...current.android, builtAt: receipt.finishedAt });
  verifyIosFfi({ coreRoot, composeRoot });
  writeJson(checkpointPath, checkpoint);
  receipts.delete(receipt);
  return checkpoint;
}

export function verifyOfficialNativeBuild({ coreRoot, composeRoot, checkpointPath }) {
  coreRoot = resolve(coreRoot); composeRoot = resolve(composeRoot);
  const checkpoint = JSON.parse(readFileSync(checkpointPath, 'utf8'));
  if (checkpoint.version !== 1 || checkpoint.receipt?.cwd !== coreRoot || checkpoint.receipt?.command !== '/opt/homebrew/bin/task'
    || JSON.stringify(checkpoint.receipt?.args) !== JSON.stringify(['set-env', '--', 'local']) || checkpoint.receipt?.exitCode !== 0
    || !/^[a-zA-Z0-9_-]+$/.test(checkpoint.receipt?.runId ?? '')) throw new Error('Invalid official-build checkpoint receipt');
  const current = inputs(coreRoot, composeRoot);
  if (JSON.stringify(checkpoint.inputs) !== JSON.stringify(current)) throw new Error('Official native build checkpoint is stale or outputs/bindings changed');
  verifyIosFfi({ coreRoot, composeRoot });
  const android = JSON.parse(readFileSync(manifest(coreRoot, 'android-ffi-build-provenance.json'), 'utf8'));
  const { builtAt, ...androidInputs } = android;
  if (JSON.stringify(androidInputs) !== JSON.stringify(current.android) || builtAt !== checkpoint.receipt.finishedAt) throw new Error('Android official build manifest changed');
  return checkpoint;
}
