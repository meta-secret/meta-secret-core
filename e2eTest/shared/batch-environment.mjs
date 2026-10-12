import { constants, cpSync, existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, readlinkSync, rmSync, statfsSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { resolve, dirname } from 'node:path';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';
import { createConnection } from 'node:net';
import { discoverAndroidSdk } from './android-sdk.mjs';
import { iosArtifactRoots } from './ios-ffi-provenance.mjs';

export function environmentModeFromSnapshot(snapshot) {
  if (!snapshot.existed) return null;
  const contents = snapshot.contents.toString('utf8');
  if (snapshot.path.endsWith('environment.rs')) return contents.match(/SELECTED_SERVER_ENVIRONMENT[^\n]*ServerEnvironment::(Local|Remote)/)?.[1]?.toLowerCase() ?? null;
  const key = snapshot.path.endsWith('.env.local') ? 'VITE_META_SECRET_ENV' : 'META_SECRET_ENV';
  return contents.match(new RegExp(`^${key}=(local|remote)$`, 'm'))?.[1] ?? null;
}

export function batchEnvironmentPaths(coreRoot, composeRoot) {
  const configs = ['meta-secret/core/src/node/app/sync/environment.rs', 'meta-secret/web-cli/ui/.env.local'].map((path) => resolve(coreRoot, path))
    .concat(['gradle.properties', 'iosApp/Configuration/Config.xcconfig'].map((path) => resolve(composeRoot, path)));
  const artifacts = [...iosArtifactRoots(composeRoot),
    resolve(composeRoot, 'composeApp/src/androidMain/jniLibs'),
    resolve(composeRoot, 'composeApp/src/androidMain/kotlin/com/metasecret/core/uniffi'),
    resolve(composeRoot, 'composeApp/build/outputs/apk'),
    resolve(composeRoot, 'composeApp/build/bin/iosArm64'),
    resolve(composeRoot, 'composeApp/build/bin/iosSimulatorArm64'),
    resolve(coreRoot, 'meta-secret/web-cli/ui/pkg'), resolve(coreRoot, 'meta-secret/target/debug/meta-cli'),
    resolve(coreRoot, 'e2eTest/.derivedData/iosApp'),
    resolve(composeRoot, 'composeApp/build/intermediates/stripped_native_libs'),
    ...['ios-ffi-build-provenance.json', 'android-ffi-build-provenance.json', 'ui-e2e-build-provenance.json'].map((name) => resolve(coreRoot, 'e2eTest/.artifacts', name)),
  ];
  return { configs, artifacts };
}

export function environmentTreeHash(path) {
  if (!existsSync(path)) return null;
  const digest = createHash('sha256');
  const visit = (current, relative) => {
    const stat = lstatSync(current);
    digest.update(`${relative}\0${stat.mode & 0o7777}\0`);
    if (stat.isSymbolicLink()) digest.update(`link:${readlinkSync(current)}`);
    else if (stat.isDirectory()) for (const name of readdirSync(current).sort()) visit(resolve(current, name), `${relative}/${name}`);
    else if (stat.isFile()) digest.update(readFileSync(current));
    else throw new Error('Unsupported environment artifact kind');
  };
  visit(path, ''); return digest.digest('hex');
}

export function assertSnapshotDiskBudget(paths, { freeBytes, minimumRemainingBytes = 12 * 1024 ** 3 } = {}) {
  const size = (path) => {
    if (!existsSync(path)) return 0;
    const stat = lstatSync(path);
    if (stat.isSymbolicLink()) return 0;
    if (stat.isDirectory()) return readdirSync(path).reduce((total, name) => total + size(resolve(path, name)), 0);
    return stat.size;
  };
  const requiredBytes = [...new Set(paths)].reduce((total, path) => total + size(path), 0);
  if (freeBytes === undefined) { const stats = statfsSync(tmpdir()); freeBytes = stats.bavail * stats.bsize; }
  // Conservatively budget a full copy even when APFS clonefile can share blocks.
  if (freeBytes - requiredBytes < minimumRemainingBytes) throw new Error(`Environment snapshot would leave less than 12 GiB free; snapshot requires ${(requiredBytes / 1024 ** 3).toFixed(2)} GiB. Run the allowlisted disk cleaner before retry`);
  return { requiredBytes, remainingBytes: freeBytes - requiredBytes };
}

export function dirtyRepositoryFingerprint(repoRoot) {
  const git = (args) => {
    const result = spawnSync('git', args, { cwd: repoRoot, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
    if (result.status !== 0) throw new Error('Cannot fingerprint repository changes before/after environment transaction');
    return result.stdout;
  };
  const status = git(['status', '--porcelain=v1', '-z', '--untracked-files=all']);
  const digest = createHash('sha256').update(git(['diff', '--binary', '--no-ext-diff', 'HEAD', '--']));
  for (const name of git(['ls-files', '--others', '--exclude-standard', '-z']).split('\0').filter(Boolean).sort()) {
    digest.update(name); if (existsSync(resolve(repoRoot, name))) digest.update(readFileSync(resolve(repoRoot, name)));
  }
  const paths = [...new Set([...git(['diff', '--name-only', '-z', 'HEAD', '--']).split('\0'), ...git(['ls-files', '--others', '--exclude-standard', '-z']).split('\0')].filter(Boolean))].sort();
  const files = Object.fromEntries(paths.map((name) => [name, environmentTreeHash(resolve(repoRoot, name))]));
  return { head: git(['rev-parse', 'HEAD']).trim(), status, digest: digest.digest('hex'), files };
}

/** Snapshot once, retain local products through the batch, restore exact original state once. */
export function createBatchEnvironmentGuard({ configs, artifacts, repositories = [], fingerprint = dirtyRepositoryFingerprint,
  preflight, setLocal, isInterrupted = () => false, onEvent = () => {}, evidencePath, snapshotBudget = assertSnapshotDiskBudget }) {
  let snapshot; let prepared = false; let restorePromise;
  const evidence = { baseline: {}, restored: {}, comparisons: {} };
  const metadata = (value) => value && typeof value === 'object' ? { head: value.head ?? null, status: value.status ?? null, digest: value.digest ?? null, files: value.files ?? {} }
    : { head: null, status: null, digest: createHash('sha256').update(JSON.stringify(value)).digest('hex'), files: {} };
  const persistEvidence = () => { if (evidencePath) { mkdirSync(dirname(evidencePath), { recursive: true }); writeFileSync(evidencePath, `${JSON.stringify(evidence, null, 2)}\n`); } };
  const restore = () => restorePromise ??= Promise.resolve().then(() => {
    if (!snapshot) return;
    const failures = [];
    for (const entry of snapshot.entries) {
      try {
        rmSync(entry.path, { recursive: true, force: true });
        if (entry.existed) { mkdirSync(dirname(entry.path), { recursive: true }); cpSync(entry.backup, entry.path, { recursive: true, preserveTimestamps: true, verbatimSymlinks: true, mode: constants.COPYFILE_FICLONE }); }
        if (environmentTreeHash(entry.path) !== entry.hash) throw new Error(`Environment restore hash mismatch: ${entry.path}`);
      } catch (error) { failures.push(error); }
    }
    for (const [index, repo] of repositories.entries()) {
      try {
        const restored = fingerprint(repo); const baseline = metadata(snapshot.dirty[index]); const current = metadata(restored);
        evidence.restored[repo] = current;
        const changedPaths = [...new Set([...Object.keys(baseline.files), ...Object.keys(current.files)])].sort().filter((path) => baseline.files[path] !== current.files[path]);
        const comparison = { headMatch: baseline.head === current.head, statusMatch: baseline.status === current.status, digestMatch: baseline.digest === current.digest, changedPaths };
        evidence.comparisons[repo] = comparison;
        persistEvidence();
        if (JSON.stringify(restored) !== JSON.stringify(snapshot.dirty[index])) {
          onEvent(`repository fingerprint mismatch repo=${repo} headMatch=${comparison.headMatch} statusMatch=${comparison.statusMatch} digestMatch=${comparison.digestMatch} changedPaths=${JSON.stringify(changedPaths)}`);
          throw new Error(`Repository dirty fingerprint changed: ${repo}; statusMatch=${comparison.statusMatch} digestMatch=${comparison.digestMatch}`);
        }
      }
      catch (error) { failures.push(error); }
    }
    if (failures.length) throw new AggregateError(failures, `Environment restoration failed; backup retained at ${snapshot.directory}`);
    rmSync(snapshot.directory, { recursive: true, force: true });
    onEvent('exact original selectors/artifact hashes/repository fingerprints restored');
  });
  return {
    async prepare() {
      if (prepared) return;
      if (snapshot) throw new Error('Environment preparation cannot restart a partial transaction');
      // Read-only inventory and ownership must succeed before any build or selector mutation.
      await preflight(); if (isInterrupted()) throw new Error('Environment preparation interrupted before mutation');
      const modes = configs.map((path) => environmentModeFromSnapshot({ path, existed: existsSync(path), contents: existsSync(path) ? readFileSync(path) : null }));
      if (modes.some((mode) => !mode) || new Set(modes).size !== 1) throw new Error('All four selectors must declare one consistent known environment');
      const dirty = repositories.map(fingerprint);
      for (const [index, repo] of repositories.entries()) evidence.baseline[repo] = metadata(dirty[index]);
      persistEvidence();
      const budget = snapshotBudget([...configs, ...artifacts]);
      onEvent(`snapshot resource budget verified; reserve=${(budget.requiredBytes / 1024 ** 3).toFixed(2)} GiB`);
      const directory = mkdtempSync(resolve(tmpdir(), 'metasecret-batch-env-'));
      snapshot = { directory, entries: [], dirty, previousMode: modes[0] };
      try {
        for (const [index, path] of [...new Set([...configs, ...artifacts])].entries()) {
          onEvent(`preserving environment artifact ${index + 1}/${configs.length + artifacts.length}`);
          const existed = existsSync(path); const backup = resolve(directory, String(index)); const hash = environmentTreeHash(path);
          if (existed) cpSync(path, backup, { recursive: true, preserveTimestamps: true, verbatimSymlinks: true, mode: constants.COPYFILE_FICLONE });
          snapshot.entries.push({ path, existed, backup, hash });
        }
      } catch (error) { snapshot = null; rmSync(directory, { recursive: true, force: true }); throw error; }
      onEvent(`batch environment snapshot mode=${modes[0]}; owned artifact paths=${snapshot.entries.length}`);
      try {
        if (modes[0] !== 'local') { if (isInterrupted()) throw new Error('Environment preparation interrupted'); onEvent('applying task set-env -- local once before F2 builds'); await setLocal(); }
        else onEvent('selectors already local; skipping environment transition; F2 still verifies build provenance');
        if (isInterrupted()) throw new Error('Environment preparation interrupted after local transition');
        const local = configs.map((path) => environmentModeFromSnapshot({ path, existed: existsSync(path), contents: existsSync(path) ? readFileSync(path) : null }));
        if (local.some((mode) => mode !== 'local')) throw new Error('Local selector verification failed after transition');
        prepared = true; onEvent('all four local selectors verified before F2 builders');
      } catch (error) { await restore(); throw error; }
    },
    restore,
  };
}

export async function preflightBatchEnvironment(scenarios, { runCommand, sdkDiscovery = discoverAndroidSdk, portAvailable = assertPortAvailable, onEvent = () => {}, visibleUiExists = existsSync } = {}) {
  if (scenarios.some((scenario) => scenario.android)) {
    const sdk = sdkDiscovery(); const avds = (await runCommand(sdk.emulatorPath, ['-list-avds'])).stdout.split(/\r?\n/).map((name) => name.trim());
    for (const scenario of scenarios.filter((value) => value.android)) {
      if (!avds.includes(scenario.android.avdName)) throw new Error(`Android AVD ${scenario.android.avdName} unavailable; create it before E2E preparation`);
      onEvent(`selected Android AVD ${scenario.android.avdName} exists; SDK tools verified`);
    }
  }
  if (scenarios.some((scenario) => scenario.ios)) {
    const output = await runCommand('xcrun', ['simctl', 'list', 'devices', 'available', '-j']);
    let devices;
    try { devices = Object.values(JSON.parse(output.stdout).devices).flat(); } catch { throw new Error('Cannot decode installed iOS simulator inventory (raw output omitted)'); }
    for (const scenario of scenarios.filter((value) => value.ios)) {
      const matches = devices.filter((device) => device.name === scenario.ios.simulatorName);
      if (matches.length !== 1) throw new Error(`iOS simulator ${scenario.ios.simulatorName} unavailable or ambiguous`);
      onEvent(`selected iOS simulator ${matches[0].name} state=${matches[0].state}`);
    }
    const developer = (await runCommand('xcode-select', ['-p'])).stdout.trim();
    if (![resolve(developer, '../Applications/DeviceHub.app'), resolve(developer, 'Applications/Simulator.app')].some(visibleUiExists)) throw new Error('Visible Xcode DeviceHub/Simulator app unavailable');
  }
  await runCommand('docker', ['info']);
  for (const scenario of scenarios) {
    const loopback = ['127.0.0.1', 'localhost', '::1', '[::1]'];
    if (!loopback.includes(new URL(scenario.server.url).hostname) || !loopback.includes(new URL(scenario.web.url).hostname) || (scenario.server.environment && scenario.server.environment !== 'local')) throw new Error('E2E server and Web targets must be local loopback');
    const inspected = await runCommand('docker', ['inspect', scenario.server.container], { allowFailure: true });
    if (inspected.exitCode === 0) throw new Error(`E2E server container ${scenario.server.container} is already owned`);
    if (!/No such (?:object|container)/i.test(inspected.stderr)) throw new Error('Cannot prove E2E server container absence');
  }
  const ports = new Map();
  for (const scenario of scenarios) {
    const web = new URL(scenario.web.url);
    for (const [host, port] of [['127.0.0.1', scenario.server.port], [web.hostname, Number(web.port || 80)], ['127.0.0.1', 5180],
      ...[scenario.networkLoss?.androidProxyPort, scenario.networkLoss?.iosProxyPort].filter(Boolean).map((port) => ['127.0.0.1', port])]) ports.set(`${host}:${port}`, { host, port });
  }
  for (const { host, port } of ports.values()) await portAvailable(host, port);
}

function assertPortAvailable(host, port) {
  return new Promise((resolveReady, reject) => {
    const socket = createConnection({ host, port });
    socket.setTimeout(1500, () => { socket.destroy(); reject(new Error(`Cannot prove port ${host}:${port} ownership: probe timed out`)); });
    socket.once('connect', () => { socket.destroy(); reject(new Error(`Port ${host}:${port} is already owned`)); });
    socket.once('error', (error) => { socket.destroy(); if (error.code === 'ECONNREFUSED') resolveReady(); else reject(new Error(`Cannot prove port ${host}:${port} absence: ${error.code}`)); });
  });
}
