import { validateRuntimeProfiles } from './scenario-runtime-profile.mjs';
import { selectExistingIosSimulator, assertExistingDeviceRun } from './platform-supervisor.mjs';
import { constants, cpSync, existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, readlinkSync, rmSync, realpathSync, statfsSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { resolve, dirname, basename, isAbsolute, sep } from 'node:path';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';
import { createConnection } from 'node:net';
import { types } from 'node:util';
import { discoverAndroidSdk } from './android-sdk.mjs';
import { iosArtifactRoots } from './ios-ffi-provenance.mjs';
import { provenanceManifestPaths } from './build-provenance-manifest.mjs';

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
    resolve(coreRoot, 'meta-secret/Cargo.lock'),
    resolve(coreRoot, 'e2eTest/.derivedData/iosApp'),
    resolve(composeRoot, 'composeApp/build/intermediates/stripped_native_libs'),
    ...['ios-ffi-build-provenance.json', 'android-ffi-build-provenance.json'].map((name) => resolve(coreRoot, 'e2eTest/.artifacts', name)),
    ...provenanceManifestPaths(resolve(coreRoot, 'e2eTest/.artifacts')),
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

export function assertSnapshotDiskBudget(paths, { freeBytes, minimumRemainingBytes = 12 * 1024 ** 3, destinationDirectory } = {}) {
  const size = (path) => {
    if (!existsSync(path)) return 0;
    const stat = lstatSync(path);
    if (stat.isSymbolicLink()) return 0;
    if (stat.isDirectory()) return readdirSync(path).reduce((total, name) => total + size(resolve(path, name)), 0);
    return stat.size;
  };
  const requiredBytes = [...new Set(paths)].reduce((total, path) => total + size(path), 0);
  if (freeBytes === undefined) { const stats = statfsSync(destinationDirectory === undefined ? tmpdir() : destinationDirectory); freeBytes = stats.bavail * stats.bsize; }
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

function snapshotPathContains(parent, child) { return child === parent || child.startsWith(parent.endsWith(sep) ? parent : parent + sep); }
function persistentSnapshotRoot(runDirectory, targets) {
  if (typeof runDirectory !== 'string' || !isAbsolute(runDirectory) || resolve(runDirectory) !== runDirectory
    || realpathSync(runDirectory) !== runDirectory) throw new Error('Canonical persistent snapshot run directory required');
  const owner = process.getuid();
  for (let current = runDirectory;; current = dirname(current)) {
    const info = lstatSync(current);
    if (info.isSymbolicLink() || !info.isDirectory() || (info.mode & 0o022) !== 0
      || current === runDirectory && info.uid !== owner || current !== runDirectory && ![owner, 0].includes(info.uid)) {
      throw new Error('Owned canonical snapshot ancestry required');
    }
    if (dirname(current) === current) break;
  }
  if (snapshotPathContains(realpathSync(tmpdir()), runDirectory)) throw new Error('Snapshot run directory must be outside OS TMP');
  for (const target of targets) {
    const lexical = resolve(target), missing = [];
    let ancestor = lexical;
    while (!existsSync(ancestor)) { missing.unshift(basename(ancestor)); ancestor = dirname(ancestor); }
    const canonical = resolve(realpathSync(ancestor), ...missing);
    if ([lexical, canonical].some(value => snapshotPathContains(value, runDirectory) || snapshotPathContains(runDirectory, value))) {
      throw new Error('Snapshot run directory overlaps an original target');
    }
  }
  return runDirectory;
}
function snapshotDirectoryIdentity(directory, runDirectory, expected) {
  const info = lstatSync(directory);
  if (dirname(directory) !== runDirectory || !basename(directory).startsWith('environment-snapshot-')
    || !info.isDirectory() || info.isSymbolicLink() || info.uid !== process.getuid() || (info.mode & 0o7777) !== 0o700
    || realpathSync(directory) !== directory || expected && (info.dev !== expected.dev || info.ino !== expected.ino)) {
    throw new Error('Private snapshot directory identity changed');
  }
  return { dev: info.dev, ino: info.ino };
}

const activeEnvironmentGuards = new WeakMap();
/** Read-only current-process authority; serialized/spread objects never qualify. */
export function assertActiveBatchEnvironmentGuard(guard, expectedTargets) {
  const state = activeEnvironmentGuards.get(guard);
  const expected = [...new Set(expectedTargets.map(path => resolve(path)))].sort();
  if (!state?.active || JSON.stringify(state.targets) !== JSON.stringify(expected)) {
    throw new Error('Exact active environment guard and snapshotted targets required');
  }
  return state.targets;
}

const environmentFailureMessages = Object.freeze({
  'snapshot-hash': 'Cannot hash original environment target',
  'snapshot-copy': 'Cannot copy environment target snapshot',
  'snapshot-verify': 'Environment snapshot copy verification failed',
  'restore-remove': 'Cannot remove environment restore target',
  'restore-copy': 'Cannot copy original environment target',
  'restore-hash': 'Environment restore verification failed',
  'repository-fingerprint': 'Repository fingerprint verification failed',
  'evidence-write': 'Cannot persist environment evidence',
});
const environmentFilesystemCodes = new Set(['EACCES', 'EPERM', 'ENOENT', 'ENOSPC', 'EIO', 'ENOTDIR',
  'EISDIR', 'EEXIST', 'EMFILE', 'ENFILE', 'ENOMEM', 'EROFS', 'EBUSY', 'ENAMETOOLONG', 'ELOOP',
  'EINVAL', 'EXDEV', 'ENOTEMPTY', 'EFBIG', 'ENOTSUP', 'EOPNOTSUPP']);
function environmentFailureCode(error) {
  try {
    if (types.isProxy(error)) return 'UNKNOWN';
    const descriptor = Object.getOwnPropertyDescriptor(error, 'code');
    return descriptor && Object.hasOwn(descriptor, 'value') && typeof descriptor.value === 'string'
      && environmentFilesystemCodes.has(descriptor.value) ? descriptor.value : 'UNKNOWN';
  } catch { return 'UNKNOWN'; }
}

/** Snapshot once, retain local products through the batch, restore exact original state once. */
export function createBatchEnvironmentGuard({ configs, artifacts, repositories = [], fingerprint = dirtyRepositoryFingerprint,
  preflight, setLocal, isInterrupted = () => false, onEvent = () => {}, evidencePath, snapshotBudget = assertSnapshotDiskBudget, runDirectory }) {
  let snapshot; let prepared = false; let restorePromise; let captureAttempt = 0;
  const authority = { active: false, targets: null };
  const evidence = { baseline: {}, restored: {}, comparisons: {}, targets: [], failures: [] };
  const metadata = (value) => value && typeof value === 'object' ? { head: value.head ?? null, status: value.status ?? null, digest: value.digest ?? null, files: value.files ?? {} }
    : { head: null, status: null, digest: createHash('sha256').update(JSON.stringify(value)).digest('hex'), files: {} };
  const persistEvidence = () => { if (evidencePath) { mkdirSync(dirname(evidencePath), { recursive: true }); writeFileSync(evidencePath, `${JSON.stringify(evidence, null, 2)}\n`); } };
  const recordFailure = (stage, targetIndex, error) => {
    try {
      if (evidence.failures.length < 32) evidence.failures.push({ stage, targetIndex,
        code: environmentFailureCode(error), message: environmentFailureMessages[stage] });
      else if (evidence.failures.length === 32) evidence.failures.push({ stage: 'failure-cap', targetIndex: null,
        code: 'CAPPED', message: 'Additional environment failures omitted', count: 1 });
      else evidence.failures[32].count++;
    } catch { /* Diagnostic projection cannot replace an owning failure. */ }
  };
  const flushAddedEvidence = () => {
    try { persistEvidence(); } catch (error) { recordFailure('evidence-write', null, error); }
  };
  const restore = () => {
    authority.active = false;
    return restorePromise ??= Promise.resolve().then(() => {
    if (!snapshot) return;
    const failures = [];
    for (const entry of snapshot.entries) {
      let stage = 'restore-remove';
      try {
        rmSync(entry.path, { recursive: true, force: true });
        stage = 'restore-copy';
        if (entry.existed) { mkdirSync(dirname(entry.path), { recursive: true }); cpSync(entry.backup, entry.path, { recursive: true, preserveTimestamps: true, verbatimSymlinks: true, mode: constants.COPYFILE_FICLONE }); }
        stage = 'restore-hash';
        const restoredHash = environmentTreeHash(entry.path);
        entry.record.restoredHash = restoredHash;
        if (restoredHash !== entry.hash) throw new Error(`Environment restore hash mismatch: ${entry.path}`);
      } catch (error) { failures.push(error); recordFailure(stage, entry.index, error); }
    }
    for (const [index, repo] of repositories.entries()) {
      let stage = 'repository-fingerprint';
      try {
        const restored = fingerprint(repo); const baseline = metadata(snapshot.dirty[index]); const current = metadata(restored);
        evidence.restored[repo] = current;
        const changedPaths = [...new Set([...Object.keys(baseline.files), ...Object.keys(current.files)])].sort().filter((path) => baseline.files[path] !== current.files[path]);
        const comparison = { headMatch: baseline.head === current.head, statusMatch: baseline.status === current.status, digestMatch: baseline.digest === current.digest, changedPaths };
        evidence.comparisons[repo] = comparison;
        stage = 'evidence-write';
        persistEvidence();
        stage = 'repository-fingerprint';
        if (JSON.stringify(restored) !== JSON.stringify(snapshot.dirty[index])) {
          onEvent(`repository fingerprint mismatch repo=${repo} headMatch=${comparison.headMatch} statusMatch=${comparison.statusMatch} digestMatch=${comparison.digestMatch} changedPaths=${JSON.stringify(changedPaths)}`);
          throw new Error(`Repository dirty fingerprint changed: ${repo}; statusMatch=${comparison.statusMatch} digestMatch=${comparison.digestMatch}`);
        }
      }
      catch (error) { failures.push(error); recordFailure(stage, null, error); }
    }
    flushAddedEvidence();
    if (failures.length) throw new AggregateError(failures, `Environment restoration failed; backup retained at ${snapshot.directory}`);
    snapshotDirectoryIdentity(snapshot.directory, runDirectory, snapshot.directoryIdentity);
    rmSync(snapshot.directory, { recursive: true, force: true });
    onEvent('exact original selectors/artifact hashes/repository fingerprints restored');
    });
  };
  const guard = {
    async prepare() {
      if (prepared) return;
      if (snapshot) throw new Error('Environment preparation cannot restart a partial transaction');
      // Read-only inventory and ownership must succeed before any build or selector mutation.
      await preflight(); if (isInterrupted()) throw new Error('Environment preparation interrupted before mutation');
      const snapshotRoot = persistentSnapshotRoot(runDirectory, [...configs, ...artifacts]);
      const modes = configs.map((path) => environmentModeFromSnapshot({ path, existed: existsSync(path), contents: existsSync(path) ? readFileSync(path) : null }));
      if (modes.some((mode) => !mode) || new Set(modes).size !== 1) throw new Error('All four selectors must declare one consistent known environment');
      const dirty = repositories.map(fingerprint);
      for (const [index, repo] of repositories.entries()) evidence.baseline[repo] = metadata(dirty[index]);
      try { persistEvidence(); } catch (error) { recordFailure('evidence-write', null, error); throw error; }
      const budget = snapshotBudget([...configs, ...artifacts], { destinationDirectory: snapshotRoot });
      onEvent(`snapshot resource budget verified; reserve=${(budget.requiredBytes / 1024 ** 3).toFixed(2)} GiB`);
      const directory = mkdtempSync(resolve(snapshotRoot, 'environment-snapshot-'));
      // Do not adopt or remove a node replaced between creation and admission.
      const directoryIdentity = snapshotDirectoryIdentity(directory, snapshotRoot);
      snapshot = { directory, directoryIdentity, entries: [], dirty, previousMode: modes[0], captureAttempt: ++captureAttempt };
      try {
        for (const [index, path] of [...new Set([...configs, ...artifacts])].entries()) {
          onEvent(`preserving environment artifact ${index + 1}/${configs.length + artifacts.length}`);
          const existed = existsSync(path); const backup = resolve(directory, String(index));
          const record = { captureAttempt: snapshot.captureAttempt, index, path, existed, initialHash: null, copyHash: null, restoredHash: null };
          evidence.targets.push(record);
          let stage = 'snapshot-hash';
          try {
            const hash = environmentTreeHash(path); record.initialHash = hash;
            stage = 'snapshot-copy';
            if (existed) cpSync(path, backup, { recursive: true, preserveTimestamps: true, verbatimSymlinks: true, mode: constants.COPYFILE_FICLONE });
            stage = 'snapshot-verify'; record.copyHash = environmentTreeHash(backup);
            if (record.copyHash !== hash) throw new Error('Environment snapshot copy hash mismatch');
            snapshot.entries.push({ path, existed, backup, hash, index, record });
          } catch (error) { recordFailure(stage, index, error); throw error; }
        }
      } catch (error) {
        flushAddedEvidence(); snapshot = null;
        // Keep the original capture error; never remove a replacement node.
        try { snapshotDirectoryIdentity(directory, snapshotRoot, directoryIdentity); } catch { throw error; }
        rmSync(directory, { recursive: true, force: true }); throw error;
      }
      flushAddedEvidence();
      onEvent(`batch environment snapshot mode=${modes[0]}; owned artifact paths=${snapshot.entries.length}`);
      try {
        if (modes[0] !== 'local') { if (isInterrupted()) throw new Error('Environment preparation interrupted'); onEvent('applying task set-env -- local once before F2 builds'); await setLocal(); }
        else onEvent('selectors already local; skipping environment transition; F2 still verifies build provenance');
        if (isInterrupted()) throw new Error('Environment preparation interrupted after local transition');
        const local = configs.map((path) => environmentModeFromSnapshot({ path, existed: existsSync(path), contents: existsSync(path) ? readFileSync(path) : null }));
        if (local.some((mode) => mode !== 'local')) throw new Error('Local selector verification failed after transition');
        prepared = true; onEvent('all four local selectors verified before F2 builders');
        if (!restorePromise && !isInterrupted()) {
          authority.targets = Object.freeze([...new Set(snapshot.entries.map(entry => resolve(entry.path)))].sort());
          authority.active = true;
        }
      } catch (error) { await restore(); throw error; }
    },
    restore,
  };
  activeEnvironmentGuards.set(guard, authority);
  return guard;
}

export async function preflightBatchEnvironment(scenarios, { runCommand, sdkDiscovery = discoverAndroidSdk, portAvailable = assertPortAvailable, onEvent = () => {}, visibleUiExists = existsSync, runtimeProfiles, coreHttp = {}, ownedIosProfile = false } = {}) {
  if (ownedIosProfile) throw new Error('Run-owned device preflight is forbidden; reuse existing targets');
  assertExistingDeviceRun({ scenarios });
  const profiles = validateRuntimeProfiles(scenarios, runtimeProfiles);
  const uiScenarios = scenarios.filter((_, index) => profiles[index].kind === 'ui');
  const httpProfiles = profiles.filter(({ kind }) => kind === 'core-http');
  if (httpProfiles.length) {
    for (const tool of ['cargo', 'sqlite3']) {
      const result = await runCommand(tool, ['--version']);
      if (result.exitCode && result.exitCode !== 0 || !result.stdout?.trim()) throw new Error(`${tool} prerequisite unavailable`);
    }
    const core = resolve(new URL('../../meta-secret/', import.meta.url).pathname);
    for (const [label, path] of [['manifest', coreHttp.manifestPath ?? resolve(core, 'Cargo.toml')], ['migration', coreHttp.migrationPath ?? resolve(core, 'db/sqlite/migrations/2023-04-22-065820_create_commit_log/up.sql')]]) if (!existsSync(path)) throw new Error(`Test20 ${label} prerequisite missing`);
  }
  if (scenarios.some((scenario) => scenario.android)) {
    const sdk = sdkDiscovery(); const avds = (await runCommand(sdk.emulatorPath, ['-list-avds'])).stdout.split(/\r?\n/).map((name) => name.trim());
    for (const scenario of scenarios.filter((value) => value.android)) {
      if (!avds.includes(scenario.android.avdName)) throw new Error(`Android AVD ${scenario.android.avdName} unavailable; create it before E2E preparation`);
      onEvent(`selected Android AVD ${scenario.android.avdName} exists; SDK tools verified`);
    }
  }
  if (scenarios.some((scenario) => scenario.ios)) {
    const output = await runCommand('xcrun', ['simctl', 'list', 'devices', 'available', '-j']);
    let inventory;
    try { inventory = JSON.parse(output.stdout); } catch { throw new Error('Cannot decode installed iOS simulator inventory (raw output omitted)'); }
    for (const scenario of scenarios.filter(value => value.ios)) {
      const selected = selectExistingIosSimulator(scenario.ios.simulatorName, inventory);
      onEvent('selected existing iOS simulator ' + selected.name + ' ' + selected.udid + ' state=' + selected.state);
    }
    const developer = (await runCommand('xcode-select', ['-p'])).stdout.trim();
    if (![resolve(developer, '../Applications/DeviceHub.app'), resolve(developer, 'Applications/Simulator.app')].some(visibleUiExists)) throw new Error('Visible Xcode DeviceHub/Simulator app unavailable');
  }
  if (uiScenarios.length) await runCommand('docker', ['info']);
  for (const scenario of uiScenarios) {
    const loopback = ['127.0.0.1', 'localhost', '::1', '[::1]'];
    if (!loopback.includes(new URL(scenario.server.url).hostname) || !loopback.includes(new URL(scenario.web.url).hostname) || (scenario.server.environment && scenario.server.environment !== 'local')) throw new Error('E2E server and Web targets must be local loopback');
    const inspected = await runCommand('docker', ['inspect', scenario.server.container], { allowFailure: true });
    if (inspected.exitCode === 0) throw new Error(`E2E server container ${scenario.server.container} is already owned`);
    if (!/No such (?:object|container)/i.test(inspected.stderr)) throw new Error('Cannot prove E2E server container absence');
  }
  const ports = new Map();
  if (httpProfiles.length) ports.set('127.0.0.1:3000', { host: '127.0.0.1', port: 3000 });
  for (const scenario of uiScenarios) {
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
