import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { createBatchEnvironmentGuard, environmentTreeHash, preflightBatchEnvironment, batchEnvironmentPaths, assertSnapshotDiskBudget } from './batch-environment.mjs';

function fixture(mode = 'remote') {
  const root = mkdtempSync(resolve(tmpdir(), 'batch-env-test-'));
  const configs = ['environment.rs', '.env.local', 'gradle.properties', 'Config.xcconfig'].map((name) => resolve(root, name));
  const put = (selected) => configs.forEach((path, index) => writeFileSync(path, index === 0
    ? `pub const SELECTED_SERVER_ENVIRONMENT: ServerEnvironment = ServerEnvironment::${selected === 'local' ? 'Local' : 'Remote'};\n`
    : `${index === 1 ? 'VITE_META_SECRET_ENV' : 'META_SECRET_ENV'}=${selected}\ncustom=preserve\n`));
  put(mode); const artifact = resolve(root, 'native'); mkdirSync(artifact); writeFileSync(resolve(artifact, 'library'), 'original-native');
  const missing = resolve(root, 'missing-cli');
  const paths = [...configs, artifact, missing]; const hashes = paths.map(environmentTreeHash);
  const events = [];
  return { root, configs, artifacts: [artifact, missing], paths, hashes, events, put,
    snapshotBudget: (paths) => assertSnapshotDiskBudget(paths, { freeBytes: 1000000, minimumRemainingBytes: 1000 }),
    assertRestored: () => assert.deepEqual(paths.map(environmentTreeHash), hashes),
    cleanup: () => rmSync(root, { recursive: true, force: true }) };
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
  const entry = source.slice(source.indexOf('async function runEntry('), source.indexOf('async function main()'));
  const prepared = entry.indexOf('await environmentGuard.prepare()'); assert(prepared >= 0);
  for (const marker of ['await prepareAndroidFfiBeforePlatforms', 'await prepareMetaCliBeforePlatforms', 'await ensureIosFfi', 'await prepareScenarioPlatforms']) assert(entry.indexOf(marker) > prepared, marker);
  assert(source.includes('await environmentGuard.restore()'));
});

test('read-only inventory validates all named runtimes and service/port ownership without starting clients', async () => {
  const calls = [];
  const scenario = { android: { avdName: 'Pixel_4a' }, ios: { simulatorName: 'iPhone 17' }, server: { url: 'http://127.0.0.1:3000', port: 3000, container: 'named-e2e' }, web: { url: 'http://localhost:5173' } };
  const runCommand = async (command, args) => {
    calls.push([command, ...args]);
    if (args[0] === '-list-avds') return { stdout: 'Pixel_4a\n' };
    if (command === 'xcrun') return { stdout: JSON.stringify({ devices: { runtime: [{ name: 'iPhone 17', state: 'Shutdown' }] } }) };
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
