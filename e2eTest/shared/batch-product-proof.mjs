import { verifyArtifactProvenance } from './build-provenance-manifest.mjs';
import { requireIosIdentityBuild } from './ios-test-identity.mjs';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { constants, chmodSync, closeSync, cpSync, existsSync, fstatSync, lstatSync, mkdirSync, openSync, readFileSync, readSync, readdirSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { basename, dirname, relative, resolve, sep } from 'node:path';
import { assertActiveBatchEnvironmentGuard, assertSnapshotDiskBudget, batchEnvironmentPaths } from './batch-environment.mjs';
import { hashArtifactTree } from './artifact-tree.mjs';
import { hashIosTree, verifyIosFfi, verifyIosXcodeBuild } from './ios-ffi-provenance.mjs';
import { captureRunnerIdentity, productRunnerIdentity, validateRunnerAudit, verifyRunnerAuditLineage } from './batch-replay-identity.mjs';
export const sha256 = (value) => createHash('sha256').update(value).digest('hex');
export const androidTarget = { platform: 'android', buildType: 'debug', package: 'metasecret.project.com', abis: ['arm64-v8a', 'armeabi-v7a', 'x86_64'] };
export const iosTarget = { platform: 'ios', sdk: 'iphonesimulator', arch: 'arm64', scheme: 'iosApp' };
export const productToolchainCommands = (product) => [
  ...(['android', 'ios', 'wasm'].includes(product) ? [['cargo', ['--version']], ['rustc', ['--version']]] : []),
  ...(product === 'ios' ? [['xcodebuild', ['-version']]] : []),
  ...(product === 'android' ? [['java', ['--version']]] : []),
];
export async function hashWorkingSources({ root, scopes, runAndCapture }) {
  const { stdout } = await runAndCapture('git', ['ls-files', '-co', '--exclude-standard', '-z', '--', ...scopes], { cwd: root });
  const digest = createHash('sha256');
  for (const name of stdout.split('\0').filter(Boolean).sort()) {
    const path = resolve(root, name); if (!existsSync(path)) continue;
    digest.update(`${name}\0`); digest.update(readFileSync(path));
  }
  return digest.digest('hex');
}
export function createProductProofPorts({ coreRoot, composeRoot, runAndCapture }) {
  const derivedDataPath = resolve(coreRoot, 'e2eTest/.derivedData/iosApp');
  const wasmPath = resolve(coreRoot, 'meta-secret/web-cli/ui/pkg');
  const ffiPath = resolve(coreRoot, 'e2eTest/.artifacts/android-ffi-build-provenance.json');
  async function captureInputs(tuple) {
    const core = await hashWorkingSources({ root: coreRoot, scopes: ['Taskfile.yml', 'docker-bake.hcl', 'meta-secret'], runAndCapture });
    const native = ['android', 'ios'].includes(tuple.product);
    const compose = native ? await hashWorkingSources({ root: composeRoot, scopes: ['settings.gradle.kts', 'build.gradle.kts', 'gradle.properties', 'gradle', 'composeApp', 'iosApp'], runAndCapture }) : null;
    const versions = {};
    for (const [file, args] of productToolchainCommands(tuple.product)) {
      const result = await runAndCapture(file, args); const version = result.stdout?.trim();
      if (!version) throw new Error(`Build toolchain version proof missing: ${file}`); versions[file] = version;
    }
    const inputHashes = { core, ...(native ? { compose } : {}), toolchain: { node: process.version, developerDir: process.env.DEVELOPER_DIR ?? null,
      javaHome: process.env.JAVA_HOME ?? null, androidSdkRoot: process.env.ANDROID_SDK_ROOT ?? process.env.ANDROID_HOME ?? null, versions } };
    if (tuple.product === 'android') {
      const proof = JSON.parse(readFileSync(ffiPath, 'utf8'));
      const source = await hashWorkingSources({ root: coreRoot, scopes: ['meta-secret'], runAndCapture });
      if (source !== proof.sourceFingerprint) throw new Error('Android FFI source provenance mismatch');
      for (const abi of androidTarget.abis) {
        const bytes = readFileSync(resolve(composeRoot, `composeApp/src/androidMain/jniLibs/${abi}/libmetasecret_mobile.so`));
        if (sha256(bytes) !== proof.abiSha256[abi] || !bytes.includes(Buffer.from(proof.diagnosticMarker))) throw new Error(`Android FFI ABI provenance mismatch: ${abi}`);
      }
      Object.assign(inputHashes, { ffiSource: source, ffiAbis: proof.abiSha256,
        bindings: hashArtifactTree(resolve(composeRoot, 'composeApp/src/androidMain/kotlin/com/metasecret/core/uniffi')) });
    }
    if (tuple.product === 'ios') inputHashes.iosNative = verifyIosFfi({ coreRoot, composeRoot });
    return { version: 3, scope: [native ? 'native' : 'web'], sourceFingerprint: sha256(JSON.stringify([core, compose])),
      configuration: tuple.configuration, target: tuple.target, inputHashes };
  }
  function inspectProducts({ tuple, paths }) {
    const hashes = Object.fromEntries(Object.entries(paths).map(([key, path]) => [key, hashArtifactTree(path)]));
    if (tuple.product === 'android') {
      const expected = JSON.parse(readFileSync(ffiPath, 'utf8')); const packagedAbis = {};
      const names = execFileSync('unzip', ['-Z1', paths.app], { encoding: 'utf8', maxBuffer: 16 * 1024 * 1024, timeout: 30000 }).split(/\r?\n/);
      const abis = names.map((name) => name.match(/^lib\/([^/]+)\/libmetasecret_mobile\.so$/)?.[1]).filter(Boolean).sort();
      if (JSON.stringify(abis) !== JSON.stringify([...tuple.target.abis].sort())) throw new Error('APK native ABI target set mismatch');
      for (const abi of tuple.target.abis) {
        const bytes = execFileSync('unzip', ['-p', paths.app, `lib/${abi}/libmetasecret_mobile.so`], { maxBuffer: 128 * 1024 * 1024, timeout: 30000 });
        packagedAbis[abi] = sha256(bytes);
        if (packagedAbis[abi] !== expected.abiSha256[abi] || !bytes.includes(Buffer.from(expected.diagnosticMarker))) throw new Error(`Packaged Android FFI ABI mismatch: ${abi}`);
      }
      return { hashes, packagedAbis };
    }
    if (tuple.product === 'ios') {
      if (tuple.configuration.identityRequired !== 'YES') throw Error('Isolated iOS product configuration required');
      requireIosIdentityBuild(paths.app);
      const link = verifyIosXcodeBuild({ derivedDataPath });
      if (link.inputs.identityRequired !== 'YES') throw Error('Owning iOS link proof lacks required identity configuration');
      const main = Object.keys(link.apps).filter((name) => !/-Runner\.app$/.test(name));
      const tests = Object.keys(link.apps).filter((name) => /UITests-Runner\.app$/.test(name));
      if (main.length !== 1 || tests.length !== 1 || hashIosTree(paths.app) !== link.apps[main[0]] || hashIosTree(paths.test) !== link.apps[tests[0]]) throw new Error('Working iOS app/XCTest products differ from immutable stash');
      return { hashes, iosProducts: Object.fromEntries(['app', 'test'].map((key) => [key, hashIosTree(paths[key])])), iosWorkingProducts: link.apps,
        iosLink: link.inputs, linkerSettings: link.linkerSettings };
    }
    if (tuple.product === 'wasm' && hashes.wasm !== hashArtifactTree(wasmPath)) throw new Error('Working WASM differs from its proven immutable product');
    if (tuple.product === 'server') {
      const expected = JSON.parse(readFileSync(paths.server, 'utf8'));
      return Promise.resolve(runAndCapture('docker', ['image', 'inspect', expected.image, '--format', '{{.Id}}'])).then(({ stdout }) => {
        if (!stdout.trim() || stdout.trim() !== expected.imageId) throw new Error('Server image proof mismatch');
        return { hashes };
      });
    }
    return { hashes };
  }
  async function captureReplayInputs(tuple) {
    const native = ['android', 'ios'].includes(tuple.product), versions = {};
    for (const [file, args] of productToolchainCommands(tuple.product)) versions[file] = (await runAndCapture(file, args)).stdout?.trim();
    if (Object.values(versions).some(value => !value)) throw Error('Build toolchain version proof missing');
    return { core: await hashWorkingSources({ root: coreRoot, scopes: ['Taskfile.yml', 'docker-bake.hcl', 'meta-secret'], runAndCapture }),
      ...(native ? { compose: await hashWorkingSources({ root: composeRoot, scopes: ['settings.gradle.kts', 'build.gradle.kts', 'gradle.properties', 'gradle', 'composeApp', 'iosApp'], runAndCapture }) } : {}),
      toolchain: { node: process.version, developerDir: process.env.DEVELOPER_DIR ?? null, javaHome: process.env.JAVA_HOME ?? null,
        androidSdkRoot: process.env.ANDROID_SDK_ROOT ?? process.env.ANDROID_HOME ?? null, versions },
      harness: await hashWorkingSources({ root: coreRoot, scopes: ['e2eTest'], runAndCapture }) };
  }
  async function captureProductReplayInputs(tuple) {
    const { harness, ...compiled } = await captureReplayInputs(tuple);
    const { stdout } = await runAndCapture('git', ['ls-files', '-co', '--exclude-standard', '-z', '--', 'e2eTest'], { cwd: coreRoot });
    const runner = captureRunnerIdentity({ coreRoot, composeRoot }, { listFiles: () => stdout.split('\0').filter(Boolean) });
    const environment = Object.fromEntries(Object.entries(process.env).filter(([key]) => /^(?:CARGO|RUST|GRADLE|ORG_GRADLE_PROJECT_|KOTLIN|ANDROID_NDK|NDK|PKG_CONFIG)/.test(key)
      || ['JAVA_TOOL_OPTIONS', '_JAVA_OPTIONS', 'JDK_JAVA_OPTIONS', 'CC', 'CXX', 'AR', 'CFLAGS', 'CXXFLAGS', 'LDFLAGS', 'SDKROOT'].includes(key)).sort(([a], [b]) => a.localeCompare(b)));
    return { ...compiled, compilerEnvironmentSha256: sha256(JSON.stringify(environment)), runner: productRunnerIdentity(runner) };
  }
  return { captureInputs, captureReplayInputs, captureProductReplayInputs, inspectProducts, derivedDataPath, wasmPath };
}


export const storedProductProofKind = 'owning-f2-support-v1';
export const storedProductProofVersion = 5;
export const runnerIndependentProductProofKind = 'owning-f2-support-v2';
const replayingGuards = new WeakSet();
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const bundleDirectory = paths => resolve(paths.runDirectory, 'supporting-products');
function assertPlainTree(path, root = path) {
  const base = resolve(root), child = resolve(path);
  if (child !== base && !child.startsWith(base + sep)) throw Error('Supporting path escapes fixed root');
  const stat = lstatSync(child);
  if (stat.isSymbolicLink() || !stat.isDirectory() && !stat.isFile()) throw Error('Supporting path symlink or unsupported kind');
  if (stat.isDirectory()) for (const name of readdirSync(child)) assertPlainTree(resolve(child, name), base);
}
function assertCanonicalDestination(path, roots, repositories) {
  const root = roots.filter(value => path === value || path.startsWith(value + sep)).sort((a, b) => b.length - a.length)[0];
  if (!root) throw Error('Supporting destination is outside exact environment targets');
  // The caller's trusted repository roots may have OS aliases; no descendant may redirect a write.
  const anchor = repositories.filter(value => path === value || path.startsWith(value + sep)).sort((a, b) => b.length - a.length)[0];
  if (!anchor) throw Error('Supporting destination root mismatch');
  let current = path;
  while (!existsSync(current)) current = dirname(current);
  for (;;) {
    if (lstatSync(current).isSymbolicLink()) throw Error('Supporting destination has a symlink ancestor');
    if (current === anchor || current === dirname(current)) break;
    current = dirname(current);
  }
}
function strippedFileSha256(path) {
  const before = lstatSync(path, { bigint: true });
  const fields = ['dev', 'ino', 'mode', 'size', 'mtimeNs', 'ctimeNs'];
  if (!before.isFile()) throw Error('Required stripped support is not a regular file');
  const fd = openSync(path, 'r');
  let failed = false;
  try {
    const initial = fstatSync(fd, { bigint: true });
    if (!initial.isFile() || fields.some(key => initial[key] !== before[key])) throw Error('Stripped support changed before digest');
    const digest = createHash('sha256'), buffer = Buffer.allocUnsafe(1024 * 1024);
    let remaining = initial.size;
    while (remaining > 0n) {
      const length = Number(remaining < BigInt(buffer.length) ? remaining : BigInt(buffer.length));
      const count = readSync(fd, buffer, 0, length, null);
      if (!Number.isInteger(count) || count <= 0 || count > length) throw Error('Stripped support ended before its initial size');
      digest.update(buffer.subarray(0, count));
      remaining -= BigInt(count);
    }
    const final = fstatSync(fd, { bigint: true }), current = lstatSync(path, { bigint: true });
    if (fields.some(key => initial[key] !== final[key] || initial[key] !== current[key])) throw Error('Stripped support changed during digest');
    return digest.digest('hex');
  } catch (error) { failed = true; throw error; }
  finally { try { closeSync(fd); } catch (error) { if (!failed) throw error; } }
}
function assertAndroidStrippedSupport(index, path) {
  if (index.version !== storedProductProofVersion || !index.nativePreparation.android) return;
  const android = index.receipts.filter(row => row.tuple.product === 'android');
  if (!android.length || new Set(android.map(row => JSON.stringify(row.tuple))).size !== android.length) throw Error('Nonempty unique Android receipts required for stripped support');
  const actual = strippedFileSha256(path);
  for (const receipt of android) {
    const packaged = receipt.current.artifacts.products?.packagedAbis?.['arm64-v8a'];
    const fresh = receipt.inputs.inputHashes?.ffiAbis?.['arm64-v8a'];
    if (typeof packaged !== 'string' || !/^[a-f0-9]{64}$/.test(packaged) || fresh !== packaged || actual !== packaged) throw Error('Stripped arm64 support differs from an owning Android tuple');
  }
}
function supportLayout(paths, index, templateName) {
  const { coreRoot: core, composeRoot: app } = paths;
  const native = index.nativePreparation, entries = [];
  const add = (id, path, required = true, preserveCompat = false) => entries.push({ id, path: resolve(path), required, preserveCompat });
  if (native.android) {
    add('android-jni', resolve(app, 'composeApp/src/androidMain/jniLibs'));
    add('android-bindings', resolve(app, 'composeApp/src/androidMain/kotlin/com/metasecret/core/uniffi'));
    add('android-provenance', resolve(core, 'e2eTest/.artifacts/android-ffi-build-provenance.json'));
    add('android-apks', resolve(app, 'composeApp/build/outputs/apk'));
    if (index.version === storedProductProofVersion) add('android-stripped-arm64', resolve(app,
      'composeApp/build/intermediates/stripped_native_libs/debug/stripDebugDebugSymbols/out/lib/arm64-v8a/libmetasecret_mobile.so'));
  }
  if (native.ios) {
    add('ios-libs', resolve(app, 'iosApp/Libs'));
    add('ios-bindings-app', resolve(app, 'iosApp/iosApp/UniffiGenerated'), true, true);
    add('ios-bindings-service', resolve(app, 'iosApp/iosApp/MetaSecretCoreService/UniffiGenerated'), true, true);
    add('ios-provenance', resolve(core, 'e2eTest/.artifacts/ios-ffi-build-provenance.json'));
    const derived = resolve(core, 'e2eTest/.derivedData/iosApp');
    add('ios-link', resolve(derived, 'ios-core-link-provenance.json'));
    const receipt = index.receipts.find(row => row.tuple.product === 'ios');
    for (const role of ['app', 'test']) {
      const name = basename(receipt?.products?.[role] ?? '').replace(new RegExp(`^${role}-`), '');
      if (!name.endsWith('.app') || basename(name) !== name || /[\\\0]/.test(name)) throw Error('Original owning iOS app/test path required');
      add(`ios-${role}`, resolve(derived, 'Build/Products/Debug-iphonesimulator', name));
    }
    if (!templateName || basename(templateName) !== templateName || !/^[^\\/\0]+\.xctestrun$/.test(templateName)) throw Error('Genuine generated template basename required');
    add('ios-template', resolve(derived, 'Build/Products', templateName));
  }
  if (native.cli) add('cli', resolve(core, 'meta-secret/target/debug/meta-cli'));
  if (index.receipts.some(receipt => receipt.tuple.product === 'wasm')) add('wasm', resolve(core, 'meta-secret/web-cli/ui/pkg'));
  return entries;
}
function readOnlyLayout(paths, index) {
  if (!index.nativePreparation.ios) return [];
  return [
    ...['aarch64-apple-ios', 'aarch64-apple-ios-sim'].map(target => resolve(paths.coreRoot, 'meta-secret/target', target, 'release/libmetasecret_mobile.a')),
    ...['iosApp/iosApp/UniffiGenerated', 'iosApp/iosApp/MetaSecretCoreService/UniffiGenerated'].map(root => resolve(paths.composeRoot, root, 'mobile_uniffi_compat.swift')),
  ];
}
function assertRequiredReadOnlyArchives(paths, index) {
  for (const path of readOnlyLayout(paths, index).slice(0, 2)) {
    if (!existsSync(path) || !lstatSync(path).isFile()) throw Error('Required uncovered Cargo archive missing or not a regular file');
  }
}
function fixedTargets(paths) {
  const { configs, artifacts } = batchEnvironmentPaths(paths.coreRoot, paths.composeRoot);
  return [...configs, ...artifacts].map(path => resolve(path));
}
function proofFiles(index) {
  return [...new Set(index.receipts.flatMap(row => [row.manifestPath, row.builderReceiptPath, row.ffiReceiptPath, ...Object.values(row.products)].filter(Boolean)))];
}
function assertReceiptPaths(paths, index) {
  const builds = resolve(paths.runDirectory, 'owning-builds') + sep;
  for (const receipt of index.receipts) {
    const id = sha256(JSON.stringify(receipt.tuple)), root = resolve(paths.runDirectory, 'products', id) + sep;
    if (!receipt.manifestPath.startsWith(root) || !Object.values(receipt.products).every(path => dirname(path) + sep === root)
      || ![receipt.builderReceiptPath, ...(receipt.ffiReceiptPath ? [receipt.ffiReceiptPath] : [])].every(path => path.startsWith(builds) && /^build-\d+\.json$/.test(basename(path)))) throw Error('Fixed owning receipt or product path mismatch');
    for (const path of [receipt.manifestPath, receipt.builderReceiptPath, receipt.ffiReceiptPath, ...Object.values(receipt.products)].filter(Boolean)) {
      assertCanonicalDestination(path, [paths.runDirectory], [paths.runDirectory]); assertPlainTree(path);
    }
    const expectedKeys = ['android', 'ios'].includes(receipt.tuple.product) ? ['app', 'test'] : [receipt.tuple.product];
    if (!same(Object.keys(receipt.products).sort(), expectedKeys.sort())) throw Error('Owning product role inventory mismatch');
    for (const [path, expected, request] of [[receipt.builderReceiptPath, receipt.current.artifacts.builderReceipt, receipt.tuple],
      ...(receipt.ffiReceiptPath ? [[receipt.ffiReceiptPath, receipt.current.artifacts.ffiReceipt, { product: 'androidFfi' }]] : [])]) {
      const bytes = readFileSync(path), owning = JSON.parse(bytes);
      if (sha256(bytes) !== expected || owning.completed !== true || owning.exitCode !== 0 || !same(owning.request, request)) throw Error('Original owning builder receipt failed or changed');
    }
    verifyArtifactProvenance({ path: receipt.manifestPath, current: receipt.current });

  }
}
export function verifyStoredProductBundle(paths, index) {
  if (!(index.version === 4 && index.proofKind === storedProductProofKind
    || index.version === storedProductProofVersion && index.proofKind === runnerIndependentProductProofKind)
    || index.sealed !== true) throw Error('Unknown or unsealed stored product proof');
  if (!index.nativePreparation || !same(Object.keys(index.nativePreparation).sort(), ['android', 'cli', 'ios'])
    || Object.values(index.nativePreparation).some(value => typeof value !== 'boolean') || !Array.isArray(index.receipts) || index.receipts.length > 6) throw Error('Native preparation proof shape mismatch');
  const path = resolve(bundleDirectory(paths), 'manifest.json');
  assertCanonicalDestination(path, [paths.runDirectory], [paths.runDirectory]); assertPlainTree(path);
  if (sha256(readFileSync(path)) !== index.supportingSha256) throw Error('Stored supporting manifest proof mismatch');
  const bundle = JSON.parse(readFileSync(path, 'utf8'));
  const { supportingSha256, ...identity } = index;
  if (!same(bundle.identity, identity) || !same(bundle.roots, [paths.coreRoot, paths.composeRoot, paths.runDirectory].map(path => realpathSync(path)))) throw Error('Stored proof run/root/index mismatch');
  if (index.version === storedProductProofVersion) verifyRunnerAuditLineage({ paths, runId: index.runId, audit: bundle.buildRunnerAudit });
  if (!Array.isArray(index.receipts) || new Set(index.receipts.map(row => JSON.stringify(row.tuple))).size !== index.receipts.length) throw Error('Duplicate or missing prepared tuple');
  assertReceiptPaths(paths, index);
  if (!same(bundle.proofs?.map(row => row.path), proofFiles(index)) || bundle.inputs?.length !== index.receipts.length) throw Error('Owning proof inventory mismatch');
  for (const row of bundle.proofs) if (hashArtifactTree(row.path) !== row.hash) throw Error('Owning product/receipt immutable bytes changed');
  const layout = supportLayout(paths, index, bundle.templateName);
  if (!same(bundle.entries.map(({ id }) => id), layout.map(({ id }) => id))) throw Error(
    index.version === storedProductProofVersion && index.nativePreparation.android && !bundle.entries.some(row => row.id === 'android-stripped-arm64')
      ? 'Stored V5 lacks required stripped support; new genuine generation required' : 'Stored supporting path inventory mismatch');
  for (const [i, entry] of layout.entries()) {
    const row = bundle.entries[i];
    if (row.present !== true && row.present !== false || entry.required && !row.present) throw Error('Required supporting bytes missing');
    if (row.present) {
      const source = resolve(bundleDirectory(paths), entry.id); assertPlainTree(source);
      if (hashArtifactTree(source) !== row.hash) throw Error('Stored supporting bytes/mode mismatch');
    }
  }
  const stripped = layout.find(entry => entry.id === 'android-stripped-arm64');
  if (stripped) assertAndroidStrippedSupport(index, resolve(bundleDirectory(paths), stripped.id));
  if (!same(bundle.readOnly.map(row => row.path), readOnlyLayout(paths, index))) throw Error('Read-only native path inventory mismatch');
  assertRequiredReadOnlyArchives(paths, index);
  for (const row of bundle.readOnly) {
    if (existsSync(row.path)) assertPlainTree(row.path);
    if (hashArtifactTree(row.path) !== row.hash) throw Error('Uncovered native archive or preserved compatibility mismatch');
  }
  return { bundle, layout };
}
export async function captureStoredProductBundle(options) {
  const { paths, index, environmentGuard, checkBudget = assertSnapshotDiskBudget, captureReplayInputs, runnerAudit } = options;
  const targets = fixedTargets(paths), active = () => assertActiveBatchEnvironmentGuard(environmentGuard, targets);
  active(); assertReceiptPaths(paths, index);
  if (index.version === storedProductProofVersion) validateRunnerAudit({ paths, runId: index.runId, audit: runnerAudit });
  const products = resolve(paths.coreRoot, 'e2eTest/.derivedData/iosApp/Build/Products');
  const templates = index.nativePreparation.ios ? readdirSync(products).filter(name => name.endsWith('.xctestrun') && !name.startsWith('.ms-')) : [];
  if (index.nativePreparation.ios && templates.length !== 1) throw Error('One genuine owning generated template required');
  const layout = supportLayout(paths, index, templates[0]), roots = [paths.coreRoot, paths.composeRoot, paths.runDirectory].map(path => realpathSync(path));
  const inputs = [];
  for (const receipt of index.receipts) { inputs.push(await captureReplayInputs(receipt.tuple)); active(); }
  const entries = layout.map(entry => {
    assertCanonicalDestination(entry.path, targets, [paths.coreRoot, paths.composeRoot]);
    const present = existsSync(entry.path);
    if (entry.required && !present) throw Error('Required original supporting bytes missing');
    if (present) assertPlainTree(entry.path);
    return { id: entry.id, present, hash: hashArtifactTree(entry.path) };
  });
  const stripped = layout.find(entry => entry.id === 'android-stripped-arm64');
  if (stripped) assertAndroidStrippedSupport(index, stripped.path);
  assertRequiredReadOnlyArchives(paths, index);
  const readOnly = readOnlyLayout(paths, index).map(path => { if (existsSync(path)) assertPlainTree(path); return { path, hash: hashArtifactTree(path) }; });
  await checkBudget(layout.filter((entry, i) => entries[i].present).map(entry => entry.path)); active();
  const directory = bundleDirectory(paths); mkdirSync(directory, { mode: 0o700 });
  for (const [i, entry] of layout.entries()) if (entries[i].present) { active(); cpSync(entry.path, resolve(directory, entry.id), { recursive: true, preserveTimestamps: true, verbatimSymlinks: true, mode: constants.COPYFILE_FICLONE }); }
  if (stripped) assertAndroidStrippedSupport(index, resolve(directory, stripped.id));
  const proofs = proofFiles(index).map(path => ({ path, hash: hashArtifactTree(path) }));
  const bundle = { identity: index, roots, proofs, templateName: templates[0] ?? null, entries, readOnly, inputs,
    ...(index.version === storedProductProofVersion ? { buildRunnerAudit: runnerAudit } : {}) };
  for (const [i, entry] of layout.entries()) if (entries[i].present && hashArtifactTree(resolve(directory, entry.id)) !== entries[i].hash) throw Error('Supporting capture source drift');
  for (const [i, receipt] of index.receipts.entries()) {
    if (!same(await captureReplayInputs(receipt.tuple), inputs[i])) throw Error('Supporting capture input drift');
    active();
  }
  if (index.version === storedProductProofVersion) validateRunnerAudit({ paths, runId: index.runId, audit: runnerAudit });
  active(); writeFileSync(resolve(directory, 'manifest.json'), JSON.stringify(bundle), { flag: 'wx', mode: 0o600 });
  return sha256(readFileSync(resolve(directory, 'manifest.json')));
}
function replaceSupportingTree(source, entry) {
  if (!entry.preserveCompat) { rmSync(entry.path, { recursive: true, force: true }); mkdirSync(dirname(entry.path), { recursive: true }); cpSync(source, entry.path, { recursive: true, preserveTimestamps: true }); return; }
  const removeGenerated = path => {
    if (!existsSync(path)) return;
    for (const name of readdirSync(path)) {
      const child = resolve(path, name);
      if (name === 'mobile_uniffi_compat.swift') continue;
      if (lstatSync(child).isDirectory()) { removeGenerated(child); if (!readdirSync(child).length) rmSync(child, { recursive: true }); } else rmSync(child);
    }
  };
  removeGenerated(entry.path); mkdirSync(entry.path, { recursive: true });
  cpSync(source, entry.path, { recursive: true, preserveTimestamps: true, filter: path => basename(path) !== 'mobile_uniffi_compat.swift' });
  chmodSync(entry.path, lstatSync(source).mode & 0o7777);
}
export async function materializeStoredProductBundle(options) {
  const { paths, index, environmentGuard, captureReplayInputs, inspectProducts, checkBudget = assertSnapshotDiskBudget, runnerAudit } = options;
  const targets = fixedTargets(paths), active = () => assertActiveBatchEnvironmentGuard(environmentGuard, targets);
  active();
  if (index.version === storedProductProofVersion) validateRunnerAudit({ paths, runId: index.runId, audit: runnerAudit });
  if (replayingGuards.has(environmentGuard)) throw Error('Concurrent supporting materialization forbidden');
  replayingGuards.add(environmentGuard);
  try {
    const { bundle, layout } = verifyStoredProductBundle(paths, index);
    if (typeof inspectProducts !== 'function') throw Error('Original read-only product inspector required');
    for (const receipt of index.receipts.filter(row => row.tuple.product === 'server')) {
      const actual = await inspectProducts({ tuple: receipt.tuple, paths: receipt.products }); active();
      if (!same(actual, receipt.current.artifacts.products)) throw Error('Current server image proof mismatch');
    }
    for (const [i, receipt] of index.receipts.entries()) {
      if (!same(await captureReplayInputs(receipt.tuple), bundle.inputs[i])) throw Error('Stored source/configuration/target/toolchain input drift');
      active();
    }
    for (const entry of layout) {
      assertCanonicalDestination(entry.path, targets, [paths.coreRoot, paths.composeRoot]);
      if (existsSync(entry.path)) assertPlainTree(entry.path);
    }
    await checkBudget(layout.filter((_entry, i) => bundle.entries[i].present).map(entry => resolve(bundleDirectory(paths), entry.id))); active();
    // Re-read all immutable bytes/receipts after asynchronous prechecks, before the first write.
    verifyStoredProductBundle(paths, index); active();
    if (index.version === storedProductProofVersion) validateRunnerAudit({ paths, runId: index.runId, audit: runnerAudit });
    for (const entry of layout) {
      assertCanonicalDestination(entry.path, targets, [paths.coreRoot, paths.composeRoot]);
      if (existsSync(entry.path)) assertPlainTree(entry.path);
    }
    for (const [i, entry] of layout.entries()) {
      active();
      if (bundle.entries[i].present && hashArtifactTree(entry.path) !== bundle.entries[i].hash) replaceSupportingTree(resolve(bundleDirectory(paths), entry.id), entry);
      if (bundle.entries[i].present && hashArtifactTree(entry.path) !== bundle.entries[i].hash) throw Error('Materialized supporting bytes/mode mismatch');
    }
  } finally { replayingGuards.delete(environmentGuard); }
}

/** Child consumers verify real working bytes; they never materialize or build. */
export function assertStoredWorkingProducts(paths, index) {
  const { bundle, layout } = verifyStoredProductBundle(paths, index);
  for (const [i, entry] of layout.entries()) {
    if (bundle.entries[i].present && hashArtifactTree(entry.path) !== bundle.entries[i].hash) throw Error('Working supporting product proof missing or changed');
  }
}
