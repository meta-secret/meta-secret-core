import { IOS_E2E_CONFIGURATION } from './ios-test-identity.mjs';
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { randomUUID } from 'node:crypto';
import { createBatchBuildHandoff, enumerateAndroidConfigurations, verifyPreparedProduct } from './batch-build-handoff.mjs';
import { hashArtifactTree } from './artifact-tree.mjs';
import { assertActiveBatchEnvironmentGuard, assertSnapshotDiskBudget, batchEnvironmentPaths } from './batch-environment.mjs';
import { captureStoredProductBundle, materializeStoredProductBundle, verifyStoredProductBundle, storedProductProofKind,
  storedProductProofVersion, runnerIndependentProductProofKind, createProductProofPorts, androidTarget, iosTarget, sha256 } from './batch-product-proof.mjs';
import { validateRunnerAudit } from './batch-replay-identity.mjs';
import { prepareIosXcodeBuild, recordIosXcodeBuild } from './ios-ffi-provenance.mjs';
import { E2E_BUILD_PROFILE, e2eBuildEnvironment, e2eGradleArguments } from './e2e-resource-budget.mjs';
const preparingProductGuards = new WeakSet();
/** No install, boot, business action or environment transition belongs here. */
export function createBatchProductPreparation(options) {
  const { paths, commands, prepareNative, scenarios, runId, environmentGuard, resourceBudget, preparationMetrics,
    newProofVersion = 4, runnerAudit } = options;
  const guarded = Object.hasOwn(options, 'environmentGuard');
  const { coreRoot, composeRoot, runDirectory } = paths;
  const { runAndWait, runAndCapture } = commands;
  const checkResources = (recoverHeadroom = false) => resourceBudget?.check({ recoverHeadroom,
    assertActive: () => { if (guarded) active(); } });
  const complete = (stage, configuration) => resourceBudget?.complete(stage, configuration);
  const measure = (phase, operation) => preparationMetrics ? preparationMetrics.measure(phase, operation) : operation();
  const ports = commands.proofPorts ?? createProductProofPorts({ coreRoot, composeRoot, runAndCapture });
  const iosBuild = commands.iosBuild ?? { prepare: prepareIosXcodeBuild, record: recordIosXcodeBuild };
  const directory = resolve(runDirectory, 'owning-builds'); const indexPath = resolve(runDirectory, 'batch-products.json');
  const hasPersistedProof = existsSync(indexPath);
  const cliPath = resolve(coreRoot, 'meta-secret/target/debug/meta-cli');
  const active = () => {
    const targets = batchEnvironmentPaths(coreRoot, composeRoot);
    return assertActiveBatchEnvironmentGuard(environmentGuard, [...targets.configs, ...targets.artifacts]);
  };
  const replayInputs = ports.captureReplayInputs ?? ports.captureInputs;
  const inputsForVersion = version => {
    if (version !== storedProductProofVersion) return replayInputs;
    if (typeof ports.captureProductReplayInputs !== 'function') throw Error('V5 compiled product replay input port required');
    return ports.captureProductReplayInputs;
  };
  const validateAudit = async version => {
    if (version !== storedProductProofVersion) return;
    if (!guarded) throw Error('V5 preparation requires an exact active environment guard');
    await validateRunnerAudit({ paths, runId, audit: runnerAudit }); active();
  };
  const verify = receipt => verifyPreparedProduct({ receipt, captureInputs: ports.captureInputs, inspectProducts: ports.inspectProducts });
  const expectedTuples = () => {
    const ui = scenarios.filter(({ scenario }) => scenario.web), tuples = [];
    for (const { configuration } of enumerateAndroidConfigurations(ui)) tuples.push({ product: 'android', configuration, target: androidTarget });
    if (ui.some(({ scenario }) => scenario.ios)) tuples.push({ product: 'ios', configuration: IOS_E2E_CONFIGURATION, target: iosTarget });
    if (ui.length) {
      tuples.push({ product: 'wasm', configuration: { environment: 'local' }, target: { platform: 'web', target: 'wasm32-unknown-unknown' } });
      const images = [...new Set(ui.map(({ scenario }) => scenario.server.image))];
      if (images.length !== 1) throw Error('Batch server image target is ambiguous');
      tuples.push({ product: 'server', configuration: { environment: 'local' }, target: { platform: 'docker', image: images[0] } });
    }
    return tuples.map(tuple => JSON.stringify(tuple)).sort();
  };
  let prepared; let androidFfiReceipt; let sequence = 0; let serverRecoveryPending = false; let nativePreparation; let cliProductHash;
  const receipt = (request, products, commandEvidence) => {
    mkdirSync(directory, { recursive: true });
    const receiptPath = resolve(directory, `build-${++sequence}.json`);
    writeFileSync(receiptPath, JSON.stringify({ completed: true, exitCode: 0, request, products, commandEvidence }, null, 2), { flag: 'wx', mode: 0o600 });
    return { exitCode: 0, receiptPath, products };
  };
  const execute = async (file, args, options, recoverHeadroom = false) => {
    await checkResources(recoverHeadroom);
    const result = await runAndWait(file, args, options);
    if (result?.exitCode != null && result.exitCode !== 0) throw new Error('Owning builder returned nonzero exit');
    return { file, args, exitCode: 0, resourceProfile: E2E_BUILD_PROFILE };
  };
  const builders = {
    androidFfi: async () => androidFfiReceipt,
    android: async (tuple) => {
      const evidence = await measure('android-build', () => execute('./gradlew', [':composeApp:assembleDebug', ':composeApp:assembleDebugAndroidTest',
        '-PMETA_SECRET_ENV=local', `-PMETA_SECRET_SOCKET_URL=${tuple.configuration.socketUrl}`, `-PMETA_SECRET_E2E_SERVER_URL=${tuple.configuration.e2eServerUrl}`,
        ...e2eGradleArguments()], { cwd: composeRoot, env: e2eBuildEnvironment() }));
      return receipt(tuple, { app: resolve(composeRoot, 'composeApp/build/outputs/apk/debug/composeApp-debug.apk'),
        test: resolve(composeRoot, 'composeApp/build/outputs/apk/androidTest/debug/composeApp-debug-androidTest.apk') }, evidence);
    },
    ios: async (tuple) => {
      const inputs = await iosBuild.prepare({ coreRoot, composeRoot, derivedDataPath: ports.derivedDataPath, runAndCapture, identityRequired: true });
      const evidence = await measure('ios-build', () => execute('xcodebuild', ['build-for-testing', '-jobs', '1', '-sdk', 'iphonesimulator', 'ARCHS=arm64', 'ONLY_ACTIVE_ARCH=YES', '-project', resolve(composeRoot, 'iosApp/iosApp.xcodeproj'), '-scheme', 'iosApp',
        '-configuration', 'Debug', '-destination', 'generic/platform=iOS Simulator', '-derivedDataPath', ports.derivedDataPath, 'METASECRET_E2E_IDENTITY_REQUIRED=YES'], { cwd: resolve(composeRoot, 'iosApp'), env: e2eBuildEnvironment() }));
      iosBuild.record({ coreRoot, composeRoot, derivedDataPath: ports.derivedDataPath, inputs });
      const products = resolve(ports.derivedDataPath, 'Build/Products/Debug-iphonesimulator');
      const names = readdirSync(products); const test = names.filter((name) => /UITests-Runner\.app$/.test(name));
      const app = names.filter((name) => name.endsWith('.app') && !/-Runner\.app$/.test(name));
      if (test.length !== 1 || app.length !== 1) throw new Error('Exact app/XCTest runner product missing or ambiguous');
      return receipt(tuple, { app: resolve(products, app[0]), test: resolve(products, test[0]) }, evidence);
    },
    wasm: async (tuple) => receipt(tuple, { wasm: ports.wasmPath }, await measure('wasm-build', () => execute('/opt/homebrew/bin/task', ['wasm-local'], { cwd: coreRoot, env: { PUSH_CACHE: '' } }))),
    server: async (tuple) => {
      // One completed-WASM recovery window moves to the final immediate server admission.
      const recoverHeadroom = serverRecoveryPending; serverRecoveryPending = false;
      const evidence = await measure('server-build', () => execute('/opt/homebrew/bin/task', ['meta-server'],
        { cwd: coreRoot }, recoverHeadroom));
      const { stdout } = await runAndCapture('docker', ['image', 'inspect', tuple.target.image, '--format', '{{.Id}}']);
      if (!stdout.trim()) throw new Error('Successful server builder has no image identity proof');
      mkdirSync(directory, { recursive: true }); const product = resolve(directory, 'server-image.json');
      writeFileSync(product, JSON.stringify({ image: tuple.target.image, imageId: stdout.trim() }));
      return receipt(tuple, { server: product }, evidence);
    },
  };
  const handoff = createBatchBuildHandoff({ runDirectory, captureInputs: ports.captureInputs, inspectProducts: ports.inspectProducts, builders,
    beforeStash: async ({ tuple, paths }) => {
      if (guarded) active();
      if (tuple.product === 'server') complete('serverWorkingComplete');
      await checkResources(true);
      return (commands.checkBudget ?? assertSnapshotDiskBudget)(Object.values(paths));
    } });
  return {
    hasPersistedProof,
    nativePreparation: () => nativePreparation,
    async prepare() {
      if (guarded) active();
      if (!prepared) prepared = (async () => {
        if (guarded) {
          if (preparingProductGuards.has(environmentGuard)) throw Error('Concurrent guarded product preparation forbidden');
          preparingProductGuards.add(environmentGuard);
        }
        try {
        await checkResources(true);
        if (resourceBudget?.mode === 'replay' && !hasPersistedProof) throw Error('Replay resource reservation cannot admit a fresh owning build');
        if (hasPersistedProof) {
          const saved = JSON.parse(readFileSync(indexPath, 'utf8'));
          if (!saved.sealed || saved.runId !== runId || !saved.nativePreparation) throw new Error('Persisted prepared product proof missing or incompatible; full restart required');
          if (saved.version === undefined && (saved.proofKind !== undefined || saved.supportingSha256 !== undefined || existsSync(resolve(runDirectory, 'supporting-products')))) throw Error('Legacy product index cannot acquire supporting proof');
          if (saved.version !== undefined && ![4, storedProductProofVersion].includes(saved.version)) throw Error('Unknown prepared product proof format');
          await validateAudit(saved.version);
          if ([4, storedProductProofVersion].includes(saved.version)) {
            if (!guarded) throw Error('Stored product replay requires an exact active environment guard');
            if (JSON.stringify(saved.receipts.map(receipt => JSON.stringify(receipt.tuple)).sort()) !== JSON.stringify(expectedTuples())) throw Error('Prepared tuple inventory mismatch');
            await measure('product-replay', () => materializeStoredProductBundle({ paths, index: saved, environmentGuard,
              captureReplayInputs: inputsForVersion(saved.version), inspectProducts: ports.inspectProducts, checkBudget: commands.checkBudget,
              ...(saved.version === storedProductProofVersion ? { runnerAudit, runId } : {}) })); active();
          }
          for (const receipt of saved.receipts) { await verify(receipt); if (guarded) active(); }
          if (saved.nativePreparation.cli && saved.cliProductHash !== hashArtifactTree(cliPath)) throw new Error('Persisted CLI product proof mismatch; no automatic rebuild');
          cliProductHash = saved.cliProductHash;
          nativePreparation = saved.nativePreparation;
          if ([4, storedProductProofVersion].includes(saved.version)) {
            active(); const records = resolve(runDirectory, 'materializations'); mkdirSync(records, { recursive: true, mode: 0o700 });
            writeFileSync(resolve(records, `${randomUUID()}.json`), JSON.stringify({ operation: 'verified-materialization', at: new Date().toISOString(),
              runId, supportingSha256: saved.supportingSha256, originalBuilderReceipts: saved.receipts.map(receipt => receipt.current.artifacts.builderReceipt),
              unchangedValidatorsPassed: true, builds: 0,
              ...(saved.version === storedProductProofVersion ? { runnerAudit } : {}) }), { flag: 'wx', mode: 0o600 });
          }
          // Release reservation only after the original replay and every product proof pass.
          complete('native'); if (nativePreparation.cli) complete('cli');
          for (const { tuple } of saved.receipts) {
            if (tuple.product === 'android') complete('android', tuple.configuration.name);
            else if (['ios', 'wasm', 'server'].includes(tuple.product)) complete(tuple.product);
          }
          complete('supporting'); complete('sealed'); await checkResources(true);
          return indexPath;
        }
        if (['supporting-products', 'products', 'owning-builds'].some(name => existsSync(resolve(runDirectory, name)))) {
          throw Error('Unsealed owning preparation cannot restart or rebuild; previous evidence preserved');
        }
        if (![4, storedProductProofVersion].includes(newProofVersion)) throw Error('Unknown new owning product proof version');
        await validateAudit(newProofVersion);
        inputsForVersion(newProofVersion); // Refuse missing V5 authority before any native work.
        const ui = scenarios.filter(({ scenario }) => scenario.web);
        if (ui.length) {
          const native = await measure('native', () => prepareNative()); nativePreparation = native;
          if (ui.some(({ scenario }) => scenario.android) && native.android !== true || ui.some(({ scenario }) => scenario.ios) && native.ios !== true) throw new Error('Owning native preparation did not prove all required targets');
          complete('native'); if (native.cli) complete('cli'); await checkResources(true);
          if (ui.some(({ scenario }) => scenario.android)) {
            const manifest = resolve(coreRoot, 'e2eTest/.artifacts/android-ffi-build-provenance.json');
            androidFfiReceipt = receipt({ product: 'androidFfi' }, {}, { owningNativePreparation: native, provenanceSha256: sha256(readFileSync(manifest)) });
          }
          for (const { configuration } of enumerateAndroidConfigurations(ui)) {
            await checkResources(); await handoff.prepare({ product: 'android', configuration, target: androidTarget });
            complete('android', configuration.name); await checkResources(true);
          }
          if (ui.some(({ scenario }) => scenario.ios)) {
            await checkResources(); await handoff.prepare({ product: 'ios', configuration: IOS_E2E_CONFIGURATION, target: iosTarget });
            complete('ios'); await checkResources(true);
          }
          await handoff.prepare({ product: 'wasm', configuration: { environment: 'local' }, target: { platform: 'web', target: 'wasm32-unknown-unknown' } });
          complete('wasm'); serverRecoveryPending = true;
          const images = [...new Set(ui.map(({ scenario }) => scenario.server.image))];
          if (images.length !== 1) throw new Error('Batch server image target is ambiguous');
          await handoff.prepare({ product: 'server', configuration: { environment: 'local' }, target: { platform: 'docker', image: images[0] } });
          complete('server'); await checkResources(true);
        }
        nativePreparation ??= { android: false, ios: false, cli: false };
        cliProductHash = nativePreparation.cli ? hashArtifactTree(cliPath) : undefined;
        const index = { ...(guarded ? { version: newProofVersion,
          proofKind: newProofVersion === storedProductProofVersion ? runnerIndependentProductProofKind : storedProductProofKind } : {}), runId, sealed: true, nativePreparation,
          ...(nativePreparation.cli ? { cliProductHash } : {}), receipts: handoff.entries() };
        if (guarded) {
          for (const receipt of index.receipts) { await verify(receipt); active(); }
          await checkResources(true);
          index.supportingSha256 = await measure('supporting-capture', () => captureStoredProductBundle({ paths, index, environmentGuard,
            captureReplayInputs: inputsForVersion(newProofVersion), checkBudget: commands.checkBudget,
            ...(newProofVersion === storedProductProofVersion ? { runnerAudit, runId } : {}) })); active();
          verifyStoredProductBundle(paths, index);
          for (const receipt of index.receipts) { await verify(receipt); active(); }
        }
        await handoff.seal(); if (guarded) active();
        writeFileSync(indexPath, JSON.stringify(index, null, 2), { flag: 'wx', mode: 0o600 });
        if (!ui.length) complete('native');
        complete('supporting'); complete('sealed'); await checkResources(true);
        return indexPath;
        } finally { if (guarded) preparingProductGuards.delete(environmentGuard); }
      })();
      const index = await prepared;
      if (guarded) active();
      if (nativePreparation.cli && (!existsSync(cliPath) || cliProductHash !== hashArtifactTree(cliPath))) {
        throw new Error('Prepared CLI product proof missing or changed; no automatic rebuild');
      }
      return index;
    },
  };
}
