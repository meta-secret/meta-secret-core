import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync, existsSync, realpathSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { createBatchEnvironmentGuard, environmentTreeHash, preflightBatchEnvironment, batchEnvironmentPaths, assertSnapshotDiskBudget, assertActiveBatchEnvironmentGuard } from './batch-environment.mjs';
import { provenanceManifestPath, provenanceManifestPaths } from './build-provenance-manifest.mjs';

import {createEnvironmentTestRun} from './environment-test-fixture.mjs';

function fixture(mode = 'remote') {
  const persistent = createEnvironmentTestRun();
  const root = mkdtempSync(resolve(tmpdir(), 'batch-env-test-'));
  const configs = ['environment.rs', '.env.local', 'gradle.properties', 'Config.xcconfig'].map((name) => resolve(root, name));
  const put = (selected) => configs.forEach((path, index) => writeFileSync(path, index === 0
    ? `pub const SELECTED_SERVER_ENVIRONMENT: ServerEnvironment = ServerEnvironment::${selected === 'local' ? 'Local' : 'Remote'};\n`
    : `${index === 1 ? 'VITE_META_SECRET_ENV' : 'META_SECRET_ENV'}=${selected}\ncustom=preserve\n`));
  put(mode); const artifact = resolve(root, 'native'); mkdirSync(artifact); writeFileSync(resolve(artifact, 'library'), 'original-native');
  const missing = resolve(root, 'missing-cli');
  const paths = [...configs, artifact, missing]; const hashes = paths.map(environmentTreeHash);
  const events = [];
  return { root, runDirectory: persistent.runDirectory, configs, artifacts: [artifact, missing], paths, hashes, events, put,
    snapshotBudget: (paths, options) => assertSnapshotDiskBudget(paths, { ...options, freeBytes: 1000000, minimumRemainingBytes: 1000 }),
    assertRestored: () => assert.deepEqual(paths.map(environmentTreeHash), hashes),
    cleanup: () => { rmSync(root, { recursive: true, force: true }); persistent.cleanup(); } };
}

test('preflight precedes one local transition; original mode/products survive multiple builders then exact restore', async () => {
  const f = fixture(); let transitions = 0;
  const guard = createBatchEnvironmentGuard({ ...f,
    preflight: async () => { f.events.push('preflight'); assert(f.configs.every((path) => readFileSync(path, 'utf8').includes('Remote') || readFileSync(path, 'utf8').includes('remote'))); },
    setLocal: async () => { transitions++; f.events.push('local'); f.put('local'); writeFileSync(resolve(f.artifacts[0], 'library'), 'local-library'); },
  });
  try {
    await guard.prepare(); await guard.prepare(); assert.equal(transitions, 1); assert.deepEqual(f.events, ['preflight', 'local']);
    writeFileSync(f.artifacts[1], 'built-cli'); writeFileSync(resolve(f.artifacts[0], 'library'), 'local-later-block');
    assert(readFileSync(f.configs[1], 'utf8').includes('local'));
    await guard.restore(); await guard.restore(); f.assertRestored();
  } finally { f.cleanup(); }
});

test('partial task failure restores rewritten selector and native artifact before rejecting', async () => {
  const f = fixture();
  const guard = createBatchEnvironmentGuard({ ...f, preflight: async () => {}, setLocal: async () => {
    f.put('local'); writeFileSync(resolve(f.artifacts[0], 'library'), 'partial-build'); writeFileSync(f.artifacts[1], 'partial-cli'); throw new Error('task failed');
  } });
  try { await assert.rejects(guard.prepare(), /task failed/); f.assertRestored(); await guard.restore(); } finally { f.cleanup(); }
});

test('already-local selection skips task but still restores artifacts changed by F2 builders', async () => {
  const f = fixture('local');
  const guard = createBatchEnvironmentGuard({ ...f, preflight: async () => {}, setLocal: async () => { throw new Error('redundant transition'); } });
  try { await guard.prepare(); writeFileSync(f.artifacts[1], 'built'); await guard.restore(); f.assertRestored(); } finally { f.cleanup(); }
});

test('inventory rejection and interruption cannot start a mutating transition', async () => {
  const f = fixture(); let mutated = false;
  try {
    for (const options of [{ preflight: async () => { throw new Error('AVD missing'); } }, { preflight: async () => {}, isInterrupted: () => true }]) {
      const guard = createBatchEnvironmentGuard({ ...f, ...options, setLocal: async () => { mutated = true; } });
      await assert.rejects(guard.prepare()); await guard.restore(); f.assertRestored();
    }
    assert.equal(mutated, false);
  } finally { f.cleanup(); }
});

test('interruption after partially successful transition restores exact state', async () => {
  const f = fixture(); let interrupted = false;
  const guard = createBatchEnvironmentGuard({ ...f, preflight: async () => {}, isInterrupted: () => interrupted,
    setLocal: async () => { f.put('local'); interrupted = true; } });
  try { await assert.rejects(guard.prepare(), /interrupted/); await guard.restore(); f.assertRestored(); } finally { f.cleanup(); }
});

test('unrelated user edits remain in place and restoration reports dirty fingerprint mismatch', async () => {
  const f = fixture(); const userFile = resolve(f.root, 'user-source'); writeFileSync(userFile, 'before');
  const guard = createBatchEnvironmentGuard({ ...f, repositories: [f.root], fingerprint: () => readFileSync(userFile, 'utf8'), preflight: async () => {}, setLocal: async () => f.put('local') });
  try { await guard.prepare(); writeFileSync(userFile, 'user edit'); await assert.rejects(guard.restore(), /restoration failed/); f.assertRestored(); assert.equal(readFileSync(userFile, 'utf8'), 'user edit'); } finally { f.cleanup(); }
});

test('range runEntry prepares parent guard before any FFI/CLI/iOS builder', () => {
  const source = readFileSync(new URL('../run-ui-e2e-range.mjs', import.meta.url), 'utf8');
  const entry = source.slice(source.indexOf('async function runEntry('), source.indexOf('async function finishRangeOwnership('));
  const prepared = entry.indexOf("await measure('snapshot', () => environmentGuard.prepare())"); assert(prepared >= 0);
  const ordered = ["await measure('snapshot', () => environmentGuard.prepare())", 'await batchProducts.prepare()', "await measure('platform-readiness'"];
  let previous = -1;
  for (const marker of ordered) {
    const index = entry.indexOf(marker);
    assert(index >= 0 && index > previous, `Missing or reordered owning stage: ${marker}`);
    previous = index;
  }
  for (const fallback of ['await prepareAndroidFfiBeforePlatforms', 'await prepareMetaCliBeforePlatforms', 'await ensureIosFfi'])
    assert.equal(entry.includes(fallback), false, 'Native preparation belongs exclusively to the guarded batch factory: ' + fallback);
  const execute = source.slice(source.indexOf('async function executeRange('));
  assert(execute.includes("setLocal: () => runPreparationCommand('/opt/homebrew/bin/task', ['set-env', '--', 'local', '--selectors-only'],"),
    'The parent environment transaction selects local selectors only; official owning FFI builders follow prepare');
  assert(!execute.includes("['set-env', '--', 'local'],"));
  assert(source.includes('await environmentGuard.restore()'));
});

test('read-only inventory validates all named runtimes and service/port ownership without starting clients', async () => {
  const calls = [];
  const scenario = { android: { avdName: 'Pixel_4a' }, ios: { simulatorName: 'iPhone 17' }, server: { url: 'http://127.0.0.1:3000', port: 3000, container: 'named-e2e' }, web: { url: 'http://localhost:5173' } };
  const runCommand = async (command, args) => {
    calls.push([command, ...args]);
    if (args[0] === '-list-avds') return { stdout: 'Pixel_4a\n' };
    if (command === 'xcrun') return { stdout: JSON.stringify({ devices: { 'com.apple.CoreSimulator.SimRuntime.iOS-26-5': [{ name: 'iPhone 17', udid: 'DB22263E-A8A3-49C5-ADD1-BAE4089E2E30', state: 'Shutdown', isAvailable: true, deviceTypeIdentifier: 'com.apple.CoreSimulator.SimDeviceType.iPhone-17' }] } }) };
    if (command === 'xcode-select') return { stdout: '/Xcode/Contents/Developer\n' };
    if (args[0] === 'inspect') return { exitCode: 1, stderr: 'Error: No such object: named-e2e' };
    return { exitCode: 0, stdout: '' };
  };
  await preflightBatchEnvironment([scenario], { runCommand, sdkDiscovery: () => ({ emulatorPath: '/sdk/emulator' }), visibleUiExists: () => true, portAvailable: async (host, port) => calls.push(['port', host, port]) });
  assert(calls.some((call) => call[0] === 'port' && call[2] === 5180));
  assert(calls.every((call) => !call.includes('boot') && !call.includes('run') && !call.includes('set-env')));
  await assert.rejects(preflightBatchEnvironment([scenario], { runCommand: async () => ({ stdout: 'Wrong_AVD\n' }), sdkDiscovery: () => ({ emulatorPath: '/sdk/emulator' }) }), /AVD/);
});

test('snapshot owns ignored client products as well as all four selectors and native sources', () => {
  const { configs, artifacts } = batchEnvironmentPaths('/core', '/compose');
  assert.equal(configs.length, 4);
  for (const suffix of ['/meta-secret/target/debug/meta-cli', '/meta-secret/web-cli/ui/pkg', '/e2eTest/.derivedData/iosApp', '/composeApp/build/outputs/apk', '/composeApp/build/bin/iosArm64', '/composeApp/build/bin/iosSimulatorArm64']) assert(artifacts.some((path) => path.endsWith(suffix)), suffix);
});

test('parent guard restores pre-existing scoped proofs and removes newly created proofs exactly', async () => {
  for (const missingScope of ['web', 'native']) {
    const f = fixture();
    const directory = resolve(f.root, 'e2eTest/.artifacts');
    const proofs = provenanceManifestPaths(directory);
    const declared = batchEnvironmentPaths(f.root, resolve(f.root, 'compose')).artifacts;
    assert(proofs.every((path) => declared.includes(path)), 'All scoped manifest paths must be owned by the real parent guard');
    const missing = provenanceManifestPath(directory, missingScope);
    mkdirSync(dirname(missing), { recursive: true });
    for (const path of proofs.filter((path) => path !== missing)) writeFileSync(path, `original-proof:${path}`);
    const baseline = proofs.map(environmentTreeHash);
    const guard = createBatchEnvironmentGuard({ ...f, artifacts: proofs, preflight: async () => {}, setLocal: async () => f.put('local') });
    try {
      await guard.prepare();
      for (const path of proofs) writeFileSync(path, 'fresh-run-proof');
      await guard.restore();
      assert.deepEqual(proofs.map(environmentTreeHash), baseline);
      assert.equal(existsSync(missing), false);
      f.assertRestored();
    } finally { f.cleanup(); }
  }
});

test('snapshot disk budget counts artifacts and rejects insufficient reserve before mutation', () => {
  const f = fixture();
  try {
    const budget = assertSnapshotDiskBudget(f.paths, { freeBytes: 10000, minimumRemainingBytes: 1000 });
    assert(budget.requiredBytes > 0); assert(budget.remainingBytes > 1000);
    assert.throws(() => assertSnapshotDiskBudget(f.paths, { freeBytes: 10, minimumRemainingBytes: 1000 }), /snapshot/);
  } finally { f.cleanup(); }
});

test('fresh native products survive scenario boundaries and only the parent restores originals', async () => {
  const f = fixture();
  const parent = createBatchEnvironmentGuard({ ...f, preflight: async () => {}, setLocal: async () => f.put('local') });
  try {
    await parent.prepare();
    writeFileSync(resolve(f.artifacts[0], 'library'), 'fresh-first-block-native');
    const freshHash = environmentTreeHash(f.artifacts[0]);
    // A range child owns configuration checks, while the parent retains native products.
    assert.equal(environmentTreeHash(f.artifacts[0]), freshHash);
    await parent.prepare(); assert.equal(environmentTreeHash(f.artifacts[0]), freshHash);
    await parent.restore(); f.assertRestored();
  } finally { f.cleanup(); }
  const source = readFileSync(new URL('../Test4/orchestrator.mjs', import.meta.url), 'utf8');
  assert(source.includes('const artifactBackup = parentOwnsArtifacts ? null : snapshotEnvironmentArtifactTrees();'));
  assert(source.includes('if (!environmentGuard.parentOwnsArtifacts) restoreEnvironmentArtifactTrees(environmentGuard.artifactBackup);'));
  assert(source.includes('captureRepoDirtyState(coreRoot, { parentOwnsArtifacts: environmentGuard.parentOwnsArtifacts })'));
  assert(source.includes('paths.push(`:(exclude)${name}`)'));
  assert(source.includes('if (environmentGuard.artifactBackup) rmSync(environmentGuard.artifactBackup.backupRoot'));
});


test('fingerprint evidence records baseline/restored hashes and mismatch causes without diff contents or weakening failure', async () => {
  const f = fixture(); const events = []; const evidencePath = resolve(f.root, 'evidence.json');
  let changed = false;
  const fingerprint = () => ({ head: 'same-head', status: ' M source.kt\0', digest: changed ? 'new-digest' : 'old-digest', files: { 'source.kt': changed ? 'new-hash' : 'old-hash' } });
  const guard = createBatchEnvironmentGuard({ ...f, repositories: [f.root], evidencePath, fingerprint, onEvent: (event) => events.push(event), preflight: async () => {}, setLocal: async () => f.put('local') });
  await guard.prepare(); changed = true;
  await assert.rejects(guard.restore(), /restoration failed/);
  const evidence = JSON.parse(readFileSync(evidencePath, 'utf8'));
  assert.equal(evidence.baseline[f.root].files['source.kt'], 'old-hash');
  assert.equal(evidence.restored[f.root].files['source.kt'], 'new-hash');
  assert.deepEqual(evidence.comparisons[f.root], { headMatch: true, statusMatch: true, digestMatch: false, changedPaths: ['source.kt'] });
  assert(events.some((event) => event.includes('statusMatch=true digestMatch=false')));
  f.cleanup();
});

// These baseline controls import only today's preflight. A missing future module
// cannot stand in for the Docker/URL regression reproduced by this invocation.
const http20Scenario = JSON.parse(readFileSync(new URL('../Test20/scenario.json', import.meta.url), 'utf8'));
const http20Profile = {
  kind: 'core-http', testNumber: 20, serverUrl: 'http://127.0.0.1:3000', stateReset: 'child-owned',
  requiredNonPlatformParticipants: ['CLI/Core + local HTTP server', 'isolated local HTTP server'],
};

function httpPreflightFixture() {
  const f = fixture(); const calls = [];
  const manifestPath = resolve(f.root, 'Cargo.toml');
  const migrationPath = resolve(f.root, 'up.sql');
  writeFileSync(manifestPath, '[workspace]\n'); writeFileSync(migrationPath, 'CREATE TABLE test (id INTEGER);\n');
  const options = {
    runtimeProfiles: [http20Profile], coreHttp: { manifestPath, migrationPath },
    sdkDiscovery: () => { throw new Error('Test20 must not discover an Android SDK'); },
    visibleUiExists: () => { throw new Error('Test20 must not inspect Xcode UI'); },
    runCommand: async (file, args) => {
      calls.push([file, ...args]);
      if (file === 'cargo' || file === 'sqlite3') return { exitCode: 0, stdout: `${file} 1.fixture\n` };
      throw new Error(`Unexpected UI inventory command: ${file}`);
    },
    portAvailable: async (host, port) => { calls.push(['port', host, port]); },
  };
  return { ...f, calls, options, migrationPath };
}

test('F2 current: typed Test20 preflight checks cargo/sqlite/migration and exact port without Docker/Web/native inventory', async () => {
  const f = httpPreflightFixture(); const original = JSON.stringify(http20Scenario);
  try {
    await preflightBatchEnvironment([http20Scenario], f.options);
    assert.deepEqual(f.calls.filter(([file]) => file !== 'port'), [['cargo', '--version'], ['sqlite3', '--version']]);
    assert.deepEqual(f.calls.filter(([file]) => file === 'port'), [['port', '127.0.0.1', 3000]]);
    assert.equal(JSON.stringify(http20Scenario), original);
  } finally { f.cleanup(); }
});

test('F2 current: mixed UI and typed Test20 retains Docker, Web ports, SDK and exact simulator inventory', async () => {
  const f = httpPreflightFixture();
  const ui = JSON.parse(readFileSync(new URL('../Test1/scenario.json', import.meta.url), 'utf8'));
  const scenarios = [ui, http20Scenario]; const before = JSON.stringify(scenarios);
  try {
    await preflightBatchEnvironment(scenarios, { ...f.options,
      runtimeProfiles: [{ kind: 'ui', testNumber: 1 }, http20Profile],
      sdkDiscovery: () => ({ emulatorPath: '/fixture/emulator' }), visibleUiExists: () => true,
      runCommand: async (file, args) => {
        f.calls.push([file, ...args]);
        if (file === '/fixture/emulator') return { stdout: 'Pixel_4a\n' };
        if (file === 'xcrun') return { stdout: JSON.stringify({ devices: { 'com.apple.CoreSimulator.SimRuntime.iOS-26-5': [{ name: 'iPhone 17', udid: 'DB22263E-A8A3-49C5-ADD1-BAE4089E2E30', state: 'Shutdown', isAvailable: true, deviceTypeIdentifier: 'com.apple.CoreSimulator.SimDeviceType.iPhone-17' }] } }) };
        if (file === 'xcode-select') return { stdout: '/Xcode/Contents/Developer\n' };
        if (file === 'docker' && args[0] === 'inspect') return { exitCode: 1, stderr: 'No such container: meta-secret-e2e-server' };
        return { exitCode: 0, stdout: `${file} fixture\n` };
      },
    });
    for (const command of [['docker', 'info'], ['docker', 'inspect', ui.server.container], ['cargo', '--version'], ['sqlite3', '--version']]) {
      assert(f.calls.some((call) => JSON.stringify(call) === JSON.stringify(command)), `Missing owning check ${command[0]}`);
    }
    for (const port of [3000, 5173, 5180]) assert(f.calls.some(([file, , value]) => file === 'port' && value === port));
    assert.equal(f.calls.filter(([file, action]) => file === 'docker' && action === 'inspect').length, 1);
    assert.equal(JSON.stringify(scenarios), before);
    assert(!f.calls.some((call) => call.includes('boot') || call.includes('set-env') || call.includes('run')));
  } finally { f.cleanup(); }
});

test('F2 current: typed Test20 prerequisite and occupied-port failures precede selector mutation', async () => {
  for (const cause of ['cargo unavailable', 'sqlite3 unavailable', 'migration missing', 'port occupied']) {
    const f = httpPreflightFixture(); let transitions = 0;
    if (cause === 'migration missing') rmSync(f.migrationPath);
    const options = { ...f.options,
      runCommand: async (file, args) => {
        if (cause.startsWith(`${file} `)) throw new Error(cause);
        return f.options.runCommand(file, args);
      },
      portAvailable: async (host, port) => {
        if (cause === 'port occupied') throw new Error(cause);
        return f.options.portAvailable(host, port);
      },
    };
    const guard = createBatchEnvironmentGuard({ ...f,
      preflight: () => preflightBatchEnvironment([http20Scenario], options),
      setLocal: async () => { transitions++; f.put('local'); },
    });
    try {
      await assert.rejects(guard.prepare(), cause === 'migration missing' ? /migration/i : new RegExp(cause));
      assert.equal(transitions, 0); await guard.restore(); f.assertRestored();
    } finally { f.cleanup(); }
  }
});

test('F2 current: unregistered, conflicting and incomplete runtime profiles reject before inventory or transition', async () => {
  for (const runtimeProfiles of [[], [{ ...http20Profile, testNumber: 19 }], [{ ...http20Profile, kind: 'ui' }], [{ ...http20Profile, serverUrl: 'http://localhost:3000' }]]) {
    const f = httpPreflightFixture();
    try {
      await assert.rejects(preflightBatchEnvironment([http20Scenario], { ...f.options, runtimeProfiles }), /profile|registered|Test.?20/i);
      assert.deepEqual(f.calls, [], 'Invalid typed metadata must fail before any tool or port command');
      f.assertRestored();
    } finally { f.cleanup(); }
  }
});


function activeGuardFixture(t) {
  const persistent = createEnvironmentTestRun();
  const root = realpathSync(mkdtempSync(resolve(tmpdir(), 'actual-guard-capability-')));
  const coreRoot = resolve(root, 'core'), composeRoot = resolve(root, 'compose');
  const paths = batchEnvironmentPaths(coreRoot, composeRoot), targets = [...paths.configs, ...paths.artifacts];
  assert.equal(targets.length, 22);
  const put = mode => paths.configs.forEach((path, index) => {
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, index === 0
      ? 'pub const SELECTED_SERVER_ENVIRONMENT: ServerEnvironment = ServerEnvironment::' + (mode === 'local' ? 'Local' : 'Remote') + ';\n'
      : (index === 1 ? 'VITE_META_SECRET_ENV=' : 'META_SECRET_ENV=') + mode + '\n');
  });
  put('remote'); const hashes = targets.map(environmentTreeHash), guards = [];
  const make = (options = {}) => {
    const guard = createBatchEnvironmentGuard({ ...paths, runDirectory: persistent.runDirectory, preflight: async () => {}, setLocal: async () => put('local'),
      snapshotBudget: (list, options) => assertSnapshotDiskBudget(list, { ...options, freeBytes: 64 * 1024 ** 3 }), ...options });
    guards.push(guard); return guard;
  };
  t.after(async () => { try { for (const guard of guards) await guard.restore(); }
    finally { rmSync(root, { recursive: true, force: true }); persistent.cleanup(); } });
  return { root, coreRoot, composeRoot, paths, targets, put, make,
    assertOriginal: () => assert.deepEqual(targets.map(environmentTreeHash), hashes) };
}

test('actual active guard binds immutable original22 targets and invalidates synchronously at restore entry', async t => {
  const f = activeGuardFixture(t), guard = f.make();
  assert.throws(() => assertActiveBatchEnvironmentGuard(guard, f.targets));
  await guard.prepare();
  const targets = assertActiveBatchEnvironmentGuard(guard, f.targets);
  assert(Object.isFrozen(targets)); assert.deepEqual([...targets], [...f.targets].sort());
  assert.throws(() => targets.push(resolve(f.root, 'foreign')));
  const restoring = guard.restore();
  assert.throws(() => assertActiveBatchEnvironmentGuard(guard, f.targets), 'Revoke before the restore microtask');
  await restoring; f.assertOriginal(); await guard.prepare();
  assert.throws(() => assertActiveBatchEnvironmentGuard(guard, f.targets), 'No-op prepare cannot reopen a closed transaction');
});

test('fake JSON spread proxy and wrong-target guards cannot authorize writes', async t => {
  const f = activeGuardFixture(t), guard = f.make(); await guard.prepare();
  const localBefore = f.targets.map(environmentTreeHash);
  for (const candidate of [undefined, null, {}, { active: true, targets: f.targets }, { ...guard },
    JSON.parse(JSON.stringify(guard)), new Proxy(guard, {})]) {
    assert.throws(() => assertActiveBatchEnvironmentGuard(candidate, f.targets));
  }
  for (const targets of [f.targets.slice(1), [...f.targets, resolve(f.root, 'outside')],
    f.targets.map(path => path.replace('/core/', '/wrong-root/'))]) {
    assert.throws(() => assertActiveBatchEnvironmentGuard(guard, targets));
  }
  assert.deepEqual(f.targets.map(environmentTreeHash), localBefore, 'Read-only admission rejection performs no working writes');
});

for (const stage of ['preflight', 'snapshot', 'transition', 'interrupted', 'final-callback'])
  test('failed or partial actual guard never becomes active (' + stage + ')', async t => {
    const f = activeGuardFixture(t), failure = new Error('controlled guard stage'), events = []; let interrupted = false;
    const guard = f.make({
      preflight: async () => { if (stage === 'preflight') throw failure; },
      setLocal: async () => { f.put('local'); if (stage === 'transition') throw failure; if (stage === 'interrupted') interrupted = true; },
      isInterrupted: () => interrupted,
      onEvent: event => { events.push(event);
        if (stage === 'snapshot' && event.startsWith('preserving environment artifact 2/')) throw failure;
        if (stage === 'final-callback' && event === 'all four local selectors verified before F2 builders') throw failure;
      },
    });
    await assert.rejects(guard.prepare());
    assert.throws(() => assertActiveBatchEnvironmentGuard(guard, f.targets));
    await guard.restore(); f.assertOriginal();
    assert.throws(() => assertActiveBatchEnvironmentGuard(guard, f.targets));
  });

test('pending prepare cannot authorize or reactivate after a concurrent restore', async t => {
  const f = activeGuardFixture(t); let entered, release;
  const atTransition = new Promise(resolve => { entered = resolve; });
  const transition = new Promise(resolve => { release = resolve; });
  const guard = f.make({ setLocal: async () => { f.put('local'); entered(); await transition; } });
  const preparing = guard.prepare(); void preparing.catch(() => {}); await atTransition;
  assert.throws(() => assertActiveBatchEnvironmentGuard(guard, f.targets));
  await guard.restore(); f.assertOriginal(); release();
  await assert.rejects(preparing, /Local selector verification failed/);
  assert.throws(() => assertActiveBatchEnvironmentGuard(guard, f.targets));
  f.assertOriginal();
});
