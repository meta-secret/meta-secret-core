import { existsSync, lstatSync, readFileSync, readdirSync, statfsSync } from 'node:fs';
import { tmpdir, totalmem } from 'node:os';
import { dirname, resolve, sep } from 'node:path';
import { batchEnvironmentPaths } from './batch-environment.mjs';
import { enumerateAndroidConfigurations } from './batch-build-handoff.mjs';

export const GiB = 1024 ** 3;
export const E2E_BUILD_PROFILE = Object.freeze({
  id: 'e2e-16g-v1', gradleHeapBytes: 3 * GiB, gradleMetaspaceBytes: 768 * 1024 ** 2,
  maxWorkers: 1, kotlinInProcess: true, minimumPhysicalBytes: 16 * GiB,
  minimumHeadroomBytes: 3 * GiB, minimumRemainingBytes: 12 * GiB,
  workingAllowanceBytes: 4 * GiB, cacheAllowanceBytes: 2 * GiB, swapAllowanceBytes: 4 * GiB,
});
const gradleJvmArguments = '-Xmx3072m -XX:MaxMetaspaceSize=768m -Dfile.encoding=UTF-8';
export function e2eGradleArguments() {
  return ['--no-daemon', '--no-parallel', '--max-workers=1', `-Dorg.gradle.jvmargs=${gradleJvmArguments}`,
    '-Pkotlin.compiler.execution.strategy=in-process'];
}
/** Also reaches the Gradle invocation embedded in the existing Xcode shell phase. */
export function e2eBuildEnvironment() {
  return { GRADLE_OPTS: `-Dorg.gradle.jvmargs="${gradleJvmArguments}" -Dorg.gradle.daemon=false -Dorg.gradle.parallel=false -Dorg.gradle.workers.max=1`,
    'ORG_GRADLE_PROJECT_kotlin.compiler.execution.strategy': 'in-process' };
}
const bytes = (value) => Number.isSafeInteger(value) && value >= 0;
const positive = (value) => bytes(value) && value > 0;
const requireBytes = (value) => { if (!bytes(value)) throw Error('Invalid preparation resource scalar'); return value; };
const sum = (values) => requireBytes(values.reduce((total, value) => total + requireBytes(value), 0));
// Estimates are deliberately full copies, not APFS clone credits or certified OS caps.
const envelope = (value, unknownFallback) => positive(value) ? requireBytes(value * 2) : unknownFallback;
export function planPreparationBudget(input, { profile = E2E_BUILD_PROFILE } = {}) {
  const mode = input.mode ?? 'fresh';
  if (!['fresh', 'replay'].includes(mode)) throw Error('Unknown preparation resource mode');
  for (const key of ['androidPairBytes', 'iosPairBytes', 'wasmBytes', 'cliBytes', 'nativeBytes', 'supportingBytes', 'serverBytes']) {
    if (input[key] != null && !bytes(input[key])) throw Error('Invalid measured preparation product size');
  }
  const count = input.androidConfigurationCount, keys = input.androidConfigurationKeys;
  if (!Number.isSafeInteger(count) || count < 0 || count > 3 || !Array.isArray(keys) || keys.length !== count
    || new Set(keys).size !== count || keys.some(key => !['defaultLocal', 'network16', 'deletion50'].includes(key))) throw Error('Invalid preparation Android inventory');
  for (const key of ['iosRequired', 'wasmRequired', 'cliRequired']) if (typeof input[key] !== 'boolean') throw Error('Invalid preparation target inventory');
  const android = count ? envelope(input.androidPairBytes, GiB / 2) : 0;
  const ios = input.iosRequired ? envelope(input.iosPairBytes, GiB / 2) : 0;
  const wasm = input.wasmRequired ? envelope(input.wasmBytes, GiB / 2) : 0;
  const cli = input.cliRequired ? envelope(input.cliBytes, GiB / 2) : 0;
  const native = count || input.iosRequired ? envelope(input.nativeBytes, GiB) : 0;
  const components = { snapshot: requireBytes(input.snapshotBytes), androidWorking: android,
    androidStashes: requireBytes(android * count), iosWorking: ios, iosStash: ios, wasmWorking: wasm, wasmStash: wasm,
    cliWorking: cli, nativeWorking: native,
    // Docker Desktop layers can grow its sparse backing file on the host volume.
    // This explicit estimate is not an image-size proof or a guaranteed Docker cap.
    serverWorking: input.wasmRequired ? envelope(input.serverBytes, 4 * GiB) : 0,
    supporting: Math.max(envelope(input.supportingBytes, 0), sum([android, ios, wasm, cli, native])),
    workingAllowance: requireBytes(profile.workingAllowanceBytes), cacheAllowance: requireBytes(profile.cacheAllowanceBytes),
    swapAllowance: requireBytes(profile.swapAllowanceBytes) };
  if (mode === 'replay') {
    for (const key of ['androidWorking', 'androidStashes', 'iosWorking', 'iosStash', 'wasmWorking', 'wasmStash', 'cliWorking', 'nativeWorking', 'serverWorking', 'cacheAllowance', 'swapAllowance']) components[key] = 0;
    components.workingAllowance = GiB; // bounded copying/hash/metadata estimate, no compiler work.
  }
  Object.freeze(components);
  const requiredBytes = sum(Object.values(components)), minimumRemainingBytes = requireBytes(profile.minimumRemainingBytes);
  if (minimumRemainingBytes < 12 * GiB || profile.minimumPhysicalBytes < 16 * GiB || profile.minimumHeadroomBytes < 3 * GiB) throw Error('Preparation profile cannot weaken resource floors');
  return Object.freeze({ version: 1, mode, profileId: profile.id, components, requiredBytes, minimumRemainingBytes,
    minimumFreeBytes: sum([requiredBytes, minimumRemainingBytes]), minimumPhysicalBytes: requireBytes(profile.minimumPhysicalBytes),
    minimumHeadroomBytes: requireBytes(profile.minimumHeadroomBytes), androidConfigurationCount: count,
    androidConfigurationKeys: Object.freeze([...keys]), iosRequired: input.iosRequired, wasmRequired: input.wasmRequired, cliRequired: input.cliRequired });
}
function completedInventory(budget, completed) {
  const configurations = completed.androidConfigurations ?? [];
  if (!Array.isArray(configurations) || new Set(configurations).size !== configurations.length
    || configurations.some(key => !budget.androidConfigurationKeys.includes(key))) throw Error('Invalid completed preparation inventory');
  for (const [key, value] of Object.entries(completed)) {
    if (!['snapshot', 'native', 'cli', 'ios', 'wasm', 'server', 'serverWorkingComplete', 'supporting', 'sealed', 'androidConfigurations'].includes(key)
      || key !== 'androidConfigurations' && typeof value !== 'boolean') throw Error('Invalid preparation completion');
  }
  return configurations;
}
export function assertPreparationResources({ budget, observation, completed = {} }) {
  for (const key of ['freeBytes', 'physicalBytes', 'availableBytes', 'swapUsedBytes', 'swapTotalBytes']) requireBytes(observation?.[key]);
  if (observation.availableBytes > observation.physicalBytes || observation.swapUsedBytes > observation.swapTotalBytes) throw Error('Inconsistent preparation resource observation');
  const configurations = completedInventory(budget, completed), c = budget.components;
  const allAndroid = configurations.length === budget.androidConfigurationCount;
  if (completed.sealed && (!completed.snapshot || !completed.native || budget.cliRequired && !completed.cli || !allAndroid
    || budget.iosRequired && !completed.ios || budget.wasmRequired && (!completed.wasm || !completed.server) || !completed.supporting)) throw Error('Preparation reservation cannot seal before required phases');
  const remaining = { ...c };
  if (completed.snapshot) remaining.snapshot = 0;
  if (completed.native) remaining.nativeWorking = 0;
  if (completed.cli) remaining.cliWorking = 0;
  remaining.androidStashes = requireBytes(c.androidWorking * (budget.androidConfigurationCount - configurations.length));
  if (allAndroid) remaining.androidWorking = 0;
  if (completed.ios) remaining.iosWorking = remaining.iosStash = 0;
  if (completed.wasm) remaining.wasmWorking = remaining.wasmStash = 0;
  if (completed.serverWorkingComplete || completed.server) remaining.serverWorking = 0;
  if (completed.supporting) remaining.supporting = 0;
  if (completed.sealed) remaining.workingAllowance = remaining.cacheAllowance = remaining.swapAllowance = 0;
  const requiredBytes = sum(Object.values(remaining));
  if (observation.physicalBytes < budget.minimumPhysicalBytes) throw Error('Preparation requires at least 16 GiB physical RAM');
  if (observation.availableBytes < budget.minimumHeadroomBytes) throw Error('Preparation cold-page RAM headroom is below the declared floor');
  if (observation.freeBytes < sum([requiredBytes, budget.minimumRemainingBytes])) throw Error('Full preparation disk reservation would breach the 12 GiB preserved floor');
  return { requiredBytes, remainingBytes: observation.freeBytes - requiredBytes, minimumRemainingBytes: budget.minimumRemainingBytes, ...observation };
}
export function createPreparationResourceBudget({ budget, observe, recovery = {} }) {
  if (typeof observe !== 'function') throw Error('Preparation observation port required');
  let completed = { androidConfigurations: [] };
  const now = recovery.now ?? (() => performance.now());
  const wait = recovery.wait ?? (ms => new Promise(resolve => setTimeout(resolve, ms)));
  return {
    mode: budget.mode,
    async check({ recoverHeadroom = false, assertActive = () => {}, signal } = {}) {
      const started = now(), deadline = started + 180000;
      const boundary = () => {
        signal?.throwIfAborted(); assertActive();
        if (recoverHeadroom && now() >= deadline) throw Error('Preparation RAM recovery deadline exceeded');
      };
      for (;;) {
        boundary();
        const observation = await observe();
        boundary();
        try { return assertPreparationResources({ budget, observation, completed }); }
        catch (error) {
          if (!recoverHeadroom || !bytes(observation?.availableBytes)
            || observation.availableBytes >= budget.minimumHeadroomBytes) throw error;
          // A second full validation with ONLY headroom repaired proves that
          // disk/physical/counters/completion are valid before any waiting.
          assertPreparationResources({ budget, observation: { ...observation,
            availableBytes: budget.minimumHeadroomBytes }, completed });
          boundary();
          await wait(Math.min(5000, deadline - now()));
          boundary();
        }
      }
    },
    complete(stage, configuration) {
      const candidate = { ...completed, androidConfigurations: [...completed.androidConfigurations] };
      if (stage === 'android') {
        if (!budget.androidConfigurationKeys.includes(configuration) || candidate.androidConfigurations.includes(configuration)) throw Error('Unknown or duplicate completed Android configuration');
        candidate.androidConfigurations.push(configuration);
      } else {
        if (!['snapshot', 'native', 'cli', 'ios', 'wasm', 'server', 'serverWorkingComplete', 'supporting', 'sealed'].includes(stage) || configuration !== undefined || candidate[stage]) throw Error('Unknown or duplicate preparation completion');
        if (stage === 'serverWorkingComplete' && (!budget.wasmRequired || candidate.server || candidate.sealed)) throw Error('Invalid server working completion');
        candidate[stage] = true;
      }
      completedInventory(budget, candidate);
      if (stage === 'sealed') assertPreparationResources({ budget, completed: candidate, observation: {
        freeBytes: Number.MAX_SAFE_INTEGER, physicalBytes: budget.minimumPhysicalBytes, availableBytes: budget.minimumHeadroomBytes, swapUsedBytes: 0, swapTotalBytes: 0 } });
      completed = candidate;
    },
    snapshot() { return { ...completed, androidConfigurations: [...completed.androidConfigurations] }; },
  };
}
function treeBytes(path) {
  if (!existsSync(path)) return 0;
  const stat = lstatSync(path);
  // The snapshot preserves links verbatim. Count the link itself, never its target.
  if (stat.isSymbolicLink()) return requireBytes(stat.size);
  if (stat.isDirectory()) return sum(readdirSync(path).map(name => treeBytes(resolve(path, name))));
  if (!stat.isFile()) throw Error('Unsupported resource sizing tree');
  return requireBytes(stat.size);
}
export function measurePreparationInputs({ paths, scenarios, measureTreeBytes = treeBytes, sizingIndexPath,
  mode = existsSync(resolve(paths.runDirectory, 'batch-products.json')) ? 'replay' : 'fresh' }) {
  const { coreRoot: core, composeRoot: app } = paths;
  const measure = path => requireBytes(measureTreeBytes(path));
  const targets = batchEnvironmentPaths(core, app), ui = scenarios.filter(({ scenario }) => scenario.web);
  const configurations = enumerateAndroidConfigurations(ui).map(({ configuration }) => configuration.name);
  const iosRequired = ui.some(({ scenario }) => scenario.ios), wasmRequired = ui.length > 0;
  const cliRequired = scenarios.some(({ scenario }) => scenario.cli?.required || scenario.setup?.initiator === 'cli'
    || Array.isArray(scenario.postJoinRecoveryPlan) && scenario.postJoinRecoveryPlan.some(({ sender, approver }) => sender === 'cli' || approver === 'cli'));
  const derived = resolve(core, 'e2eTest/.derivedData/iosApp/Build/Products/Debug-iphonesimulator');
  const ios = existsSync(derived) ? readdirSync(derived).filter(name => name.endsWith('.app')).map(name => resolve(derived, name)) : [];
  const android = ['debug/composeApp-debug.apk', 'androidTest/debug/composeApp-debug-androidTest.apk'].map(name => resolve(app, 'composeApp/build/outputs/apk', name));
  const androidNative = resolve(app, 'composeApp/src/androidMain/jniLibs'), iosNative = resolve(app, 'iosApp/Libs');
  const nativeBindings = [resolve(app, 'iosApp/iosApp/UniffiGenerated'), resolve(app, 'iosApp/iosApp/MetaSecretCoreService/UniffiGenerated')];
  const cargoNative = (target, extension) => resolve(core, 'meta-secret/target', target, 'release', `libmetasecret_mobile.${extension}`);
  const cachedAndroid = configurations.length ? sum(['aarch64-linux-android', 'armv7-linux-androideabi', 'x86_64-linux-android'].map(target => {
    const shared = measure(cargoNative(target, 'so')); return shared || measure(cargoNative(target, 'a'));
  })) : 0;
  const cachedIos = iosRequired ? sum(['aarch64-apple-ios', 'aarch64-apple-ios-sim'].map(target => measure(cargoNative(target, 'a')))) : 0;
  const wasm = resolve(core, 'meta-secret/web-cli/ui/pkg'), cli = resolve(core, 'meta-secret/target/debug/meta-cli');
  const measured = { androidPairBytes: sum(android.map(measure)), iosPairBytes: sum(ios.map(measure)), wasmBytes: measure(wasm),
    cliBytes: measure(cli), nativeBytes: sum([Math.max(measure(androidNative), cachedAndroid),
      Math.max(measure(iosNative), cachedIos), ...nativeBindings.map(measure)]) };
  // Historical sealed bytes are sizing evidence only: never admission/provenance authority.
  if (sizingIndexPath && existsSync(sizingIndexPath)) {
    try {
      const saved = JSON.parse(readFileSync(sizingIndexPath, 'utf8')), base = resolve(dirname(sizingIndexPath), 'products') + sep;
      if (saved.sealed !== true || !Array.isArray(saved.receipts)) throw Error('Invalid sizing evidence');
      for (const row of saved.receipts) {
        const key = { android: 'androidPairBytes', ios: 'iosPairBytes', wasm: 'wasmBytes' }[row.tuple?.product];
        if (!key || !row.products || Object.values(row.products).some(path => typeof path !== 'string' || !resolve(path).startsWith(base))) continue;
        const value = sum(Object.values(row.products).map(measure)); measured[key] = Math.max(measured[key], value);
      }
    } catch { /* Unknown/malformed old evidence grants no sizing reduction; envelopes remain. */ }
  }
  return { mode, snapshotBytes: sum([...new Set([...targets.configs, ...targets.artifacts])].map(measure)),
    androidConfigurationCount: configurations.length, androidConfigurationKeys: configurations, iosRequired, wasmRequired, cliRequired,
    ...measured, supportingBytes: sum(Object.values(measured)) };
}
function parseDarwinCounters(vmStat, swapUsage, physicalBytes, freeBytes) {
  const page = Number(vmStat.match(/page size of (\d+) bytes/)?.[1]);
  const field = label => Number(vmStat.match(new RegExp(`^${label}:\\s+(\\d+)\\.\\s*$`, 'm'))?.[1]);
  // XNU vm_statistics.h documents speculative as part of free_count. Do not add it,
  // purgeable or file-backed counts. Free + inactive is a cold-page estimate only.
  // https://github.com/apple-oss-distributions/xnu/blob/main/osfmk/mach/vm_statistics.h
  if (!positive(page)) throw Error('Unknown Darwin memory page size');
  const availableBytes = sum([requireBytes(field('Pages free') * page), requireBytes(field('Pages inactive') * page)]);
  const swap = label => {
    const match = swapUsage.match(new RegExp(`\\b${label}\\s*=\\s*(\\d+(?:\\.\\d+)?)([KMGT]?)(?=\\s|$)`, 'i'));
    if (!match) throw Error('Unknown Darwin swap counters');
    return requireBytes(Math.round(Number(match[1]) * (1024 ** ({ '': 0, K: 1, M: 2, G: 3, T: 4 }[match[2].toUpperCase()]))));
  };
  const observation = { freeBytes, physicalBytes, availableBytes, swapUsedBytes: swap('used'), swapTotalBytes: swap('total') };
  for (const value of Object.values(observation)) requireBytes(value);
  if (availableBytes > physicalBytes || observation.swapUsedBytes > observation.swapTotalBytes) throw Error('Inconsistent Darwin resource counters');
  return observation;
}
export async function readPreparationObservation({ path, runAndCapture, platform = process.platform, statfs = statfsSync, totalmem: physicalMemory = totalmem }) {
  if (platform !== 'darwin' || typeof runAndCapture !== 'function') throw Error('Supported bounded Darwin resource observation required');
  const options = { timeoutMs: 5000, maxCaptureBytes: 65536 };
  const vm = await runAndCapture('/usr/bin/vm_stat', [], options);
  const swap = await runAndCapture('/usr/sbin/sysctl', ['vm.swapusage'], options);
  if (vm.exitCode != null && vm.exitCode !== 0 || swap.exitCode != null && swap.exitCode !== 0 || typeof vm.stdout !== 'string' || typeof swap.stdout !== 'string') throw Error('Resource observation command failed; raw output omitted');
  const freeBytes = Math.min(...[path, tmpdir()].map(target => {
    const disk = statfs(target); return requireBytes(disk.bavail * disk.bsize);
  }));
  return parseDarwinCounters(vm.stdout, swap.stdout, requireBytes(physicalMemory()), freeBytes);
}
