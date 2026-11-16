export { IOS_E2E_CONFIGURATION } from './ios-test-identity.mjs';
import { constants, cpSync, existsSync, mkdirSync, readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { resolve, basename } from 'node:path';
import { recordArtifactProvenance, verifyArtifactProvenance, provenanceManifestPath } from './build-provenance-manifest.mjs';

const digest = (value) => createHash('sha256').update(value).digest('hex');
const equal = (a, b) => JSON.stringify(a) === JSON.stringify(b);
export const androidConfigurations = Object.freeze([
  { name: 'defaultLocal', environment: 'local', socketUrl: '', e2eServerUrl: '' },
  { name: 'network16', environment: 'local', socketUrl: 'http://10.0.2.2:5181', e2eServerUrl: 'http://10.0.2.2:5181' },
  { name: 'deletion50', environment: 'local', socketUrl: 'http://10.0.2.2:5201', e2eServerUrl: 'http://10.0.2.2:5201' },
]);
export function androidConfigurationForScenario({ testNumber, scenario }) {
  if (!scenario.android) return null;
  const port = scenario.networkLoss?.androidProxyPort;
  const configuration = port ? androidConfigurations.find((entry) => entry.socketUrl === `http://10.0.2.2:${port}`) : androidConfigurations[0];
  if (!configuration || port && !((testNumber === 16 && port === 5181) || (testNumber === 50 && port === 5201))) throw new Error('Unregistered Android configuration/route');
  return structuredClone(configuration);
}
export function enumerateAndroidConfigurations(scenarios) {
  const entries = new Map();
  for (const input of scenarios) {
    const configuration = androidConfigurationForScenario(input); if (!configuration) continue;
    if (!entries.has(configuration.name)) entries.set(configuration.name, { configuration, scenarios: [] });
    entries.get(configuration.name).scenarios.push(input);
  }
  return [...entries.values()];
}
function assertBuilderReceipt(result) {
  if (result?.exitCode !== 0 || !result.receiptPath || !existsSync(result.receiptPath)) throw new Error('Successful owning builder receipt required');
  const receipt = JSON.parse(readFileSync(result.receiptPath, 'utf8'));
  if (receipt.exitCode !== 0 || receipt.completed !== true) throw new Error('Owning builder receipt is incomplete or failed');
  return digest(readFileSync(result.receiptPath));
}
export async function verifyPreparedProduct({ receipt, captureInputs, inspectProducts }) {
  if (!equal(await captureInputs(receipt.tuple), receipt.inputs)) throw new Error('Prepared source/configuration/target input drift mismatch');
  for (const [path, expected] of [[receipt.builderReceiptPath, receipt.current.artifacts.builderReceipt], ...(receipt.ffiReceiptPath ? [[receipt.ffiReceiptPath, receipt.current.artifacts.ffiReceipt]] : [])]) {
    if (!existsSync(path) || digest(readFileSync(path)) !== expected) throw new Error('Owning builder receipt proof changed or missing');
  }
  let products;
  try { products = await inspectProducts({ tuple: receipt.tuple, paths: receipt.products }); }
  catch (cause) { throw new Error(`Product provenance inspection failed: ${cause.message}`, { cause }); }
  verifyArtifactProvenance({ path: receipt.manifestPath, current: { ...receipt.inputs, artifacts: { ...receipt.current.artifacts, products } } });
  return receipt;
}
/** This coordinator owns orchestration, while the injected real ports own actual
 * build commands and product inspection. The existing manifest remains authority. */
export function createBatchBuildHandoff({ runDirectory, captureInputs, builders, inspectProducts, beforeStash = () => {} }) {
  const records = new Map(); const pending = new Map(); const ffi = new Map(); let sealed = false;
  const key = (tuple) => digest(JSON.stringify(tuple));
  const inspect = async (tuple, paths) => {
    try { return await inspectProducts({ tuple, paths }); }
    catch (cause) { throw new Error(`Product provenance inspection failed: ${cause.message}`, { cause }); }
  };
  const verify = (_tuple, receipt) => verifyPreparedProduct({ receipt, captureInputs, inspectProducts });
  return {
    async prepare(tuple) {
      const id = key(tuple);
      if (records.has(id)) return verify(tuple, records.get(id));
      if (sealed) throw new Error('Build preparation is sealed; unprepared tuple cannot compile during runtime');
      if (pending.has(id)) return pending.get(id);
      const preparation = (async () => {
        const inputs = await captureInputs(tuple); let ffiResult;
        if (tuple.product === 'android') {
          const ffiKey = digest(JSON.stringify([inputs.inputHashes?.ffiSource, inputs.inputHashes?.ffiAbis]));
          if (!ffi.has(ffiKey)) ffi.set(ffiKey, Promise.resolve().then(() => builders.androidFfi(inputs)).then((result) => { assertBuilderReceipt(result); return result; }));
          ffiResult = await ffi.get(ffiKey);
        }
        const result = await builders[tuple.product](tuple);
        const builderHash = assertBuilderReceipt(result);
        if (!equal(inputs, await captureInputs(tuple))) throw new Error('Build inputs changed during owning builder execution');
        const before = await inspect(tuple, result.products);
        await beforeStash({ tuple, paths: result.products });
        const directory = resolve(runDirectory, 'products', id); mkdirSync(directory, { recursive: true });
        const products = {};
        for (const [name, path] of Object.entries(result.products)) {
          const destination = resolve(directory, `${name}-${basename(path)}`);
          cpSync(path, destination, { recursive: true, verbatimSymlinks: true, preserveTimestamps: true, mode: constants.COPYFILE_FICLONE }); products[name] = destination;
        }
        const after = await inspect(tuple, products);
        if (!equal(before, after)) throw new Error('Product stash hash/pair mismatch');
        const current = { ...inputs, artifacts: { inputHashes: inputs.inputHashes, products: after, builderReceipt: builderHash,
          ...(ffiResult ? { ffiReceipt: digest(readFileSync(ffiResult.receiptPath)) } : {}) } };
        const manifestPath = provenanceManifestPath(directory, tuple.product === 'wasm' || tuple.product === 'server' ? 'web' : 'native');
        recordArtifactProvenance(manifestPath, current, { completedOwningBuilderReceipt: result.receiptPath });
        const receipt = { tuple, inputs, products, current, manifestPath, builderReceiptPath: result.receiptPath, ...(ffiResult ? { ffiReceiptPath: ffiResult.receiptPath } : {}) };
        records.set(id, receipt); return receipt;
      })();
      pending.set(id, preparation);
      try { return await preparation; } finally { pending.delete(id); }
    },
    async seal() { if (pending.size) throw new Error('Cannot seal pending owning builds'); sealed = true; },
    async consume(tuple) {
      if (!sealed) throw new Error('Products must be prepared and sealed before runtime handoff');
      const receipt = records.get(key(tuple)); if (!receipt) throw new Error('No prepared proof for requested configuration/target tuple');
      return verify(tuple, receipt);
    },
    entries() { return [...records.values()]; },
  };
}
