import { IOS_E2E_CONFIGURATION } from './ios-test-identity.mjs';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { verifyArtifactProvenance } from './build-provenance-manifest.mjs';
import { hashArtifactTree } from './artifact-tree.mjs';
import { iosAppProductHashes } from './ios-ffi-provenance.mjs';
import { androidConfigurationForScenario, verifyPreparedProduct } from './batch-build-handoff.mjs';
import { assertStoredWorkingProducts, createProductProofPorts, androidTarget, iosTarget, productToolchainCommands, storedProductProofVersion } from './batch-product-proof.mjs';
import { validateRunnerAudit } from './batch-replay-identity.mjs';
const equal = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const fileHash = (path) => createHash('sha256').update(readFileSync(path)).digest('hex');
function currentRunnerAudit(paths, runId) {
  const audit = { path: process.env.E2E_RUNNER_AUDIT_PATH, sha256: process.env.E2E_RUNNER_AUDIT_SHA256 };
  const runnerPid = Number(process.env.E2E_PARENT_RUNNER_PID);
  validateRunnerAudit({ paths, runId, audit, expectedRunnerPid: runnerPid });
  return { audit, runnerPid };
}
export function verifyPlatformProductReceipt({ verifiedBuild, paths, platform }) {
  if (!verifiedBuild?.path || !verifiedBuild.current) throw new Error('Verified product provenance receipt required before installation');
  const { path, current } = verifiedBuild; verifyArtifactProvenance({ path, current });
  const artifacts = current.artifacts;
  if (platform === 'android') {
    for (const key of ['app', 'test']) {
      const expected = artifacts.products?.hashes?.[key] ?? artifacts[key === 'app' ? 'androidAppApk' : 'androidTestApk'];
      if (!paths[key] || !existsSync(paths[key]) || ![hashArtifactTree(paths[key]), fileHash(paths[key])].includes(expected)) throw new Error('Android APK app/test artifact provenance mismatch');
    }
  } else {
    const actual = iosAppProductHashes(paths.derivedDataPath);
    if (artifacts.iosProducts && !equal(artifacts.iosProducts, actual)) throw new Error('iOS product provenance mismatch');
    if (artifacts.products?.iosWorkingProducts && !equal(artifacts.products.iosWorkingProducts, actual)) throw new Error('Working iOS app/XCTest product provenance mismatch');
    if (!artifacts.iosProducts && !artifacts.products?.iosProducts && !artifacts.iosBuild) throw new Error('Full iOS product proof missing');
  }
  return current;
}
/** A bare flag is never authority. Every selection re-captures current full inputs,
 * successful owning receipts and immutable product hashes before child reuse. */
export async function loadBatchProducts({ coreRoot, composeRoot, scenario, testNumber, runAndCapture }) {
  const path = process.env.E2E_BATCH_PRODUCTS_PATH;
  if (!path) {
    if (process.env.E2E_REUSE_PROVEN_ARTIFACTS === '1' && process.env.E2E_RANGE_RUN === '1') throw new Error('Parent reuse requested without product proof index');
    return null;
  }
  const index = JSON.parse(readFileSync(path, 'utf8'));
  if (!index.sealed || index.runId !== process.env.E2E_RUN_ID) throw new Error('Parent product handoff run/seal mismatch');
  if (index.version === undefined && (index.proofKind !== undefined || index.supportingSha256 !== undefined || existsSync(resolve(dirname(path), 'supporting-products')))) throw Error('Legacy product index cannot acquire supporting proof');
  if (index.version !== undefined && ![4, storedProductProofVersion].includes(index.version)) throw Error('Unknown prepared product proof format');
  const proofPaths = { coreRoot, composeRoot, runDirectory: dirname(path) };
  if ([4, storedProductProofVersion].includes(index.version)) assertStoredWorkingProducts(proofPaths, index);
  const runner = index.version === storedProductProofVersion ? currentRunnerAudit(proofPaths, index.runId) : null;
  const ports = createProductProofPorts({ coreRoot, composeRoot, runAndCapture }); const selected = {};
  for (const product of ['android', 'ios', 'wasm', 'server']) {
    if (product === 'android' && !scenario.android || product === 'ios' && !scenario.ios) continue;
    const configuration = product === 'android' ? androidConfigurationForScenario({ testNumber, scenario })
      : product === 'ios' ? IOS_E2E_CONFIGURATION : { environment: 'local' };
    const target = product === 'android' ? androidTarget : product === 'ios' ? iosTarget
      : product === 'wasm' ? { platform: 'web', target: 'wasm32-unknown-unknown' } : { platform: 'docker', image: scenario.server.image };
    const matches = index.receipts.filter((entry) => equal(entry.tuple, { product, configuration, target }));
    if (matches.length !== 1) throw new Error(`Exact prepared ${product} configuration proof missing or ambiguous`);
    const receipt = matches[0];
    await verifyPreparedProduct({ receipt, captureInputs: ports.captureInputs, inspectProducts: ports.inspectProducts });
    selected[product] = { ...receipt, path: receipt.manifestPath };
  }
  if (runner) currentRunnerAudit(proofPaths, index.runId);
  return { ...selected, derivedDataPath: ports.derivedDataPath, ...([4, storedProductProofVersion].includes(index.version)
    ? { storedProof: { path, sha256: fileHash(path), ...(runner ? { runnerAudit: runner.audit, runnerPid: runner.runnerPid } : {}) } } : {}) };
}

// Existing iOS actor factories are synchronous. Preserve that interface while
// rejecting full source/product drift immediately before their actual spawn.
export function assertBatchProductsCurrent({ products, coreRoot, composeRoot }) {
  if (!products) return;
  if (products.storedProof) {
    const { path, sha256 } = products.storedProof;
    if (fileHash(path) !== sha256) throw Error('Stored product index changed before child spawn');
    const index = JSON.parse(readFileSync(path, 'utf8'));
    if (index.runId !== process.env.E2E_RUN_ID) throw Error('Stored product run mismatch before child spawn');
    assertStoredWorkingProducts({ coreRoot, composeRoot, runDirectory: dirname(path) }, index);
    if (index.version === storedProductProofVersion) {
      const current = currentRunnerAudit({ coreRoot, composeRoot, runDirectory: dirname(path) }, index.runId);
      if (!equal(current.audit, products.storedProof.runnerAudit) || current.runnerPid !== products.storedProof.runnerPid) throw Error('Runner audit handoff changed before child spawn');
    }
  }
  const source = (root, scopes) => {
    const names = execFileSync('git', ['ls-files', '-co', '--exclude-standard', '-z', '--', ...scopes], { cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, timeout: 30000 });
    const digest = createHash('sha256');
    for (const name of names.split('\0').filter(Boolean).sort()) { const path = resolve(root, name); if (!existsSync(path)) continue; digest.update(`${name}\0`); digest.update(readFileSync(path)); }
    return digest.digest('hex');
  };
  const ports = createProductProofPorts({ coreRoot, composeRoot, runAndCapture: () => { throw new Error('Synchronous native check cannot start an asynchronous process'); } });
  for (const product of ['android', 'ios']) {
    const receipt = products[product]; if (!receipt) continue;
    const inputs = receipt.inputs.inputHashes;
    const versions = Object.fromEntries(productToolchainCommands(product).map(([file, args]) => [file, execFileSync(file, args, { encoding: 'utf8', timeout: 30000 }).trim()]));
    if (!equal(inputs.toolchain, { node: process.version, developerDir: process.env.DEVELOPER_DIR ?? null,
      javaHome: process.env.JAVA_HOME ?? null, androidSdkRoot: process.env.ANDROID_SDK_ROOT ?? process.env.ANDROID_HOME ?? null, versions })) throw new Error('Prepared native toolchain configuration drift');
    if (product === 'android') {
      if (hashArtifactTree(resolve(composeRoot, 'composeApp/src/androidMain/kotlin/com/metasecret/core/uniffi')) !== inputs.bindings) throw new Error('Android generated binding proof drift');
      for (const [abi, expected] of Object.entries(inputs.ffiAbis)) if (fileHash(resolve(composeRoot, `composeApp/src/androidMain/jniLibs/${abi}/libmetasecret_mobile.so`)) !== expected) throw new Error('Android FFI ABI product drift');
    }
    if (source(coreRoot, ['Taskfile.yml', 'docker-bake.hcl', 'meta-secret']) !== inputs.core
      || source(composeRoot, ['settings.gradle.kts', 'build.gradle.kts', 'gradle.properties', 'gradle', 'composeApp', 'iosApp']) !== inputs.compose) throw new Error('Prepared native full-source provenance drift');
    if (fileHash(receipt.builderReceiptPath) !== receipt.current.artifacts.builderReceipt
      || receipt.ffiReceiptPath && fileHash(receipt.ffiReceiptPath) !== receipt.current.artifacts.ffiReceipt) throw new Error('Owning builder proof drift');
    verifyArtifactProvenance({ path: receipt.manifestPath, current: { ...receipt.inputs,
      artifacts: { ...receipt.current.artifacts, products: ports.inspectProducts({ tuple: receipt.tuple, paths: receipt.products }) } } });
  }
}
