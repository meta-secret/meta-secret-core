import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { syncBuiltinESMExports } from 'node:module';
import { resolve, dirname } from 'node:path';
import { tmpdir } from 'node:os';
import { createBatchEnvironmentGuard, environmentTreeHash, assertSnapshotDiskBudget,
  assertActiveBatchEnvironmentGuard } from './batch-environment.mjs';

import {createEnvironmentTestRun} from './environment-test-fixture.mjs';

function fixture(t, { artifactCount = 1, allowRestoreFailure = false } = {}) {
  const persistent = createEnvironmentTestRun();
  const root = fs.realpathSync(fs.mkdtempSync(resolve(tmpdir(), 'batch-restore-observation-')));
  const configs = ['environment.rs', '.env.local', 'gradle.properties', 'Config.xcconfig'].map(name => resolve(root, name));
  const put = mode => configs.forEach((path, index) => fs.writeFileSync(path, index === 0
    ? `pub const SELECTED_SERVER_ENVIRONMENT: ServerEnvironment = ServerEnvironment::${mode === 'local' ? 'Local' : 'Remote'};\n`
    : `${index === 1 ? 'VITE_META_SECRET_ENV' : 'META_SECRET_ENV'}=${mode}\n`));
  put('remote');
  const artifacts = Array.from({ length: artifactCount }, (_, index) => resolve(root, `native-${index}`));
  for (const path of artifacts) { fs.mkdirSync(path); fs.writeFileSync(resolve(path, 'library'), 'original-native'); }
  const missing = resolve(root, 'missing'), paths = [...configs, ...artifacts, missing];
  const originals = new Map(), backups = new Set(), copies = [], events = [];
  const f = { root, configs, artifacts: [...artifacts, missing], paths, backups, copies, events, put,
    evidencePath: resolve(root, 'evidence.json'), hashes: paths.map(environmentTreeHash), transitions: 0, guard: undefined };
  f.hook = (name, wrapper) => {
    assert(!originals.has(name), 'One closed wrapper per fs export');
    originals.set(name, fs[name]); fs[name] = wrapper(fs[name]); syncBuiltinESMExports();
  };
  f.make = options => (f.guard = createBatchEnvironmentGuard({ configs, artifacts: f.artifacts, runDirectory: persistent.runDirectory,
    preflight: async () => {}, setLocal: async () => { f.transitions++; put('local'); },
    onEvent: value => events.push(value), evidencePath: f.evidencePath,
    snapshotBudget: (targets, options) => assertSnapshotDiskBudget(targets, { ...options, freeBytes: 1024 ** 3, minimumRemainingBytes: 1024 }),
    ...options }));
  f.evidence = () => JSON.parse(fs.readFileSync(f.evidencePath, 'utf8'));
  t.after(async () => {
    for (const [name, original] of originals) fs[name] = original;
    syncBuiltinESMExports();
    let cleanupError;
    try { await f.guard?.restore(); } catch (error) { if (!allowRestoreFailure) cleanupError = error; }
    try {
      for (const backup of backups) fs.rmSync(backup, { recursive: true, force: true });
      fs.rmSync(root, { recursive: true, force: true }); persistent.cleanup();
    } finally { if (cleanupError) throw cleanupError; }
  });
  f.hook('cpSync', original => (source, destination, options) => {
    if (paths.includes(source)) backups.add(dirname(destination));
    copies.push({ source, destination });
    return f.copyFault ? f.copyFault(source, destination, options, original) : original(source, destination, options);
  });
  return f;
}

function projection(evidence, stage, targetIndex, code) {
  const rows = evidence.failures ?? [];
  assert(rows.length > 0, 'Expected a fixed failure projection, not an absent diagnostic API');
  assert(rows.length <= 33, 'Only projections may be capped');
  const row = rows.find(value => value.stage === stage && value.targetIndex === targetIndex);
  assert(row, 'Exact original stage and target index must be observable');
  assert.equal(row.code, code);
  assert.deepEqual(Object.keys(row).sort(), ['stage', 'targetIndex', 'code', 'message'].sort());
  assert.equal(typeof row.message, 'string'); assert(row.message.length <= 96);
  assert(row.stage.length <= 32 && row.code.length <= 32);
  return row;
}

async function rejection(operation) {
  let observed;
  try { await operation(); } catch (error) { observed = error; }
  assert(observed, 'The actual owning operation must reject');
  return { error: observed };
}

test('actual guard records source-ordered initial copied and restored hashes including absent targets', async t => {
  const f = fixture(t), guard = f.make(); await guard.prepare();
  assert.equal(f.transitions, 1); assertActiveBatchEnvironmentGuard(guard, f.paths);
  const prepared = f.evidence();
  assert.equal(prepared.targets?.length ?? 0, f.paths.length, 'Expected one indexed snapshot record per configured target');
  assert.deepEqual(prepared.targets.map(row => [row.index, row.path, row.existed]),
    f.paths.map((path, index) => [index, path, f.hashes[index] !== null]));
  for (const row of prepared.targets) {
    assert.equal(row.initialHash, f.hashes[row.index]); assert.equal(row.copyHash, row.initialHash);
  }
  fs.writeFileSync(resolve(f.artifacts[0], 'library'), 'changed-native'); fs.writeFileSync(f.paths.at(-1), 'new-cli');
  await guard.restore(); await guard.restore();
  assert.deepEqual(f.paths.map(environmentTreeHash), f.hashes);
  assert.throws(() => assertActiveBatchEnvironmentGuard(guard, f.paths));
  const restored = f.evidence();
  assert.deepEqual(restored.targets.map(row => row.restoredHash), f.hashes);
  assert.deepEqual(restored.failures ?? [], []);
  assert([...f.backups].every(path => !fs.existsSync(path)));
});

for (const corruption of ['bytes', 'mode', 'type', 'membership']) test('actual snapshot refuses successful but corrupt copy before local authority: ' + corruption, async t => {
  const f = fixture(t, { allowRestoreFailure: true }), index = corruption === 'membership' ? 4 : 0;
  f.copyFault = (source, destination, options, original) => {
    original(source, destination, options);
    if (source === f.paths[index]) {
      if (corruption === 'bytes') fs.appendFileSync(destination, 'corrupt-copy');
      if (corruption === 'mode') fs.chmodSync(destination, (fs.statSync(destination).mode & 0o7777) ^ 0o100);
      if (corruption === 'type') { fs.rmSync(destination); fs.mkdirSync(destination); }
      if (corruption === 'membership') fs.writeFileSync(resolve(destination, 'unexpected'), 'copy-only');
    }
  };
  const guard = f.make(); await rejection(() => guard.prepare());
  assert.equal(f.transitions, 0, 'Corrupt backup cannot reach the original local transition');
  assert.throws(() => assertActiveBatchEnvironmentGuard(guard, f.paths));
  assert.deepEqual(f.paths.map(environmentTreeHash), f.hashes, 'Working inputs must remain original');
  const evidence = f.evidence(); projection(evidence, 'snapshot-verify', index, 'UNKNOWN');
  assert.equal(evidence.targets[index].initialHash, f.hashes[index]);
  assert.notEqual(evidence.targets[index].copyHash, f.hashes[index]);
});

for (const stage of ['snapshot-hash', 'snapshot-copy', 'snapshot-verify']) test('actual snapshot I/O failure preserves exact cause: ' + stage, async t => {
  const f = fixture(t), failure = Object.assign(Error('controlled snapshot I/O'), { code: 'EACCES' });
  let backup;
  if (stage !== 'snapshot-copy') f.hook('readFileSync', original => (path, options) => {
    const armed = stage !== 'snapshot-hash' || f.events.some(event => event.startsWith('preserving environment artifact 1/'));
    if (armed && path === (stage === 'snapshot-hash' ? f.configs[0] : backup)) throw failure;
    return original(path, options);
  });
  f.copyFault = (source, destination, options, original) => {
    if (source === f.configs[0]) {
      backup = destination; if (stage === 'snapshot-copy') throw failure;
    }
    return original(source, destination, options);
  };
  const guard = f.make(); assert.equal((await rejection(() => guard.prepare())).error, failure);
  assert.equal(f.transitions, 0); assert.throws(() => assertActiveBatchEnvironmentGuard(guard, f.paths));
  projection(f.evidence(), stage, 0, 'EACCES');
});

for (const stage of ['restore-remove', 'restore-copy', 'restore-hash']) test('actual restore retains failure backup and continues remaining targets: ' + stage, async t => {
  const f = fixture(t, { allowRestoreFailure: true }); let fingerprints = 0;
  const guard = f.make({ repositories: [resolve(f.root, 'repo')], fingerprint: () => { fingerprints++; return 'fixed'; } });
  await guard.prepare();
  const failure = Object.assign(Error('controlled restore I/O'), { code: 'EACCES' });
  if (stage === 'restore-remove') f.hook('rmSync', original => (path, options) => {
    if (path === f.configs[0]) throw failure; return original(path, options);
  });
  if (stage === 'restore-hash') f.hook('readFileSync', original => (path, options) => {
    if (path === f.configs[0]) throw failure; return original(path, options);
  });
  if (stage === 'restore-copy') f.copyFault = (source, destination, options, original) => {
    if (destination === f.configs[0]) throw failure; return original(source, destination, options);
  };
  const { error: observed } = await rejection(() => guard.restore());
  assert(observed instanceof AggregateError); assert(observed.errors.includes(failure));
  assert(f.copies.some(row => row.destination === f.configs.at(-1)), 'Later target restore still runs');
  assert.equal(environmentTreeHash(f.artifacts[0]), f.hashes[4]);
  assert([...f.backups].every(path => fs.existsSync(path)), 'Failed original backup remains available');
  assert.throws(() => assertActiveBatchEnvironmentGuard(guard, f.paths));
  const evidence = f.evidence(); projection(evidence, stage, 0, 'EACCES');
  assert.equal(evidence.targets[0].restoredHash, null, 'A failed restore/hash cannot manufacture equality');
  assert.equal(fingerprints, 2, 'Repository validation still runs after a target failure');
  assert.equal((await rejection(() => guard.restore())).error, observed, 'Cached restoration preserves the original Aggregate');
});

test('post-copy hash mismatch stays fatal against original hash rather than adopting changed bytes', async t => {
  const f = fixture(t, { allowRestoreFailure: true }), guard = f.make(); await guard.prepare();
  f.copyFault = (source, destination, options, original) => {
    original(source, destination, options);
    if (destination === f.configs[0]) fs.appendFileSync(destination, 'wrong-restored-bytes');
  };
  const { error: observed } = await rejection(() => guard.restore());
  assert(observed instanceof AggregateError);
  assert(observed.errors.some(error => error.message.startsWith('Environment restore hash mismatch:')));
  const evidence = f.evidence(); projection(evidence, 'restore-hash', 0, 'UNKNOWN');
  assert.equal(evidence.targets[0].initialHash, f.hashes[0]);
  assert.notEqual(evidence.targets[0].restoredHash, f.hashes[0]);
  assert([...f.backups].every(path => fs.existsSync(path)));
});

for (const kind of ['mismatch', 'io']) test('repository evidence and original rejection remain authoritative: ' + kind, async t => {
  const f = fixture(t, { allowRestoreFailure: true }); let calls = 0;
  const repo = resolve(f.root, 'repo'), failure = Object.assign(Error('controlled repository I/O'), { code: 'EACCES' });
  const fingerprint = () => {
    if (++calls > 1 && kind === 'io') throw failure;
    return { head: 'fixed', status: '', digest: calls === 1 ? 'original' : 'changed', files: {} };
  };
  const guard = f.make({ repositories: [repo], fingerprint }); await guard.prepare();
  const { error: observed } = await rejection(() => guard.restore()); assert(observed instanceof AggregateError);
  if (kind === 'io') assert(observed.errors.includes(failure));
  else {
    assert(observed.errors.some(error => error.message.startsWith('Repository dirty fingerprint changed:')));
    assert.equal(f.evidence().comparisons[repo].digestMatch, false);
  }
  projection(f.evidence(), 'repository-fingerprint', null, kind === 'io' ? 'EACCES' : 'UNKNOWN');
  assert.deepEqual(f.paths.map(environmentTreeHash), f.hashes);
  assert([...f.backups].every(path => fs.existsSync(path)));
});

for (const kind of ['getters', 'proxy', 'unknown-code']) test('malicious error and failed diagnostic flush preserve the primary: ' + kind, async t => {
  const f = fixture(t), canary = 'PRIVATE_RESTORE_CANARY_93'; let getters = 0, descriptorTraps = 0, primaryRaised = false;
  const target = {}, flushFailure = Object.assign(Error('controlled evidence write'), { code: 'ENOSPC' }), attempted = [];
  if (kind === 'unknown-code') Object.defineProperty(target, 'code', { value: canary });
  for (const key of kind === 'unknown-code' ? ['message', 'stack', 'path'] : ['code', 'message', 'stack', 'path']) {
    Object.defineProperty(target, key, { get() { getters++; throw Error(canary); }, configurable: true });
  }
  const failure = kind === 'proxy' ? new Proxy(target, {
    get() { getters++; throw Error(canary); },
    getOwnPropertyDescriptor() { descriptorTraps++; throw Error(canary); },
  }) : target;
  f.hook('writeFileSync', original => (path, value, options) => {
    if (path === f.evidencePath && primaryRaised) { attempted.push(String(value)); throw flushFailure; }
    return original(path, value, options);
  });
  f.copyFault = source => { assert.equal(source, f.configs[0]); primaryRaised = true; throw failure; };
  const guard = f.make(); assert.equal((await rejection(() => guard.prepare())).error, failure);
  assert.equal(getters, 0); assert.equal(f.transitions, 0);
  assert.throws(() => assertActiveBatchEnvironmentGuard(guard, f.paths));
  assert(attempted.length > 0, 'Added best-effort diagnostics must attempt persistence without replacing the primary');
  projection(JSON.parse(attempted.at(-1)), 'snapshot-copy', 0, 'UNKNOWN');
  assert(!JSON.stringify([attempted, f.evidence(), f.events]).includes(canary)); assert.equal(getters, 0);
  assert(descriptorTraps <= 1, 'A failed descriptor lookup must not cause recursive diagnostic retry');
});

test('original strict evidence-write failure cannot become an optional successful diagnostic', async t => {
  const f = fixture(t), failure = Object.assign(Error('controlled original evidence failure'), { code: 'ENOSPC' });
  f.hook('writeFileSync', original => (path, value, options) => {
    if (path === f.evidencePath) throw failure; return original(path, value, options);
  });
  const guard = f.make(); assert.equal((await rejection(() => guard.prepare())).error, failure);
  assert.equal(f.transitions, 0); assert.equal(f.copies.length, 0);
  assert.throws(() => assertActiveBatchEnvironmentGuard(guard, f.paths));
});

test('projection cap never caps original errors target attempts or idempotent revoked cleanup', async t => {
  const f = fixture(t, { artifactCount: 40, allowRestoreFailure: true }), guard = f.make(); await guard.prepare();
  const failures = new Map(f.artifacts.slice(0, -1).map(path => [path, Object.assign(Error('controlled per-target fault'), { code: 'EACCES' })]));
  f.copyFault = (source, destination, options, original) => {
    if (failures.has(destination)) throw failures.get(destination); return original(source, destination, options);
  };
  const { error: observed } = await rejection(() => guard.restore());
  assert(observed instanceof AggregateError); assert.equal(observed.errors.length, 40);
  for (const failure of failures.values()) assert(observed.errors.includes(failure));
  assert.equal(f.copies.filter(row => failures.has(row.destination)).length, 40);
  const rows = f.evidence().failures ?? [];
  assert.equal(rows.length, 33, '32 fixed projections plus exactly one CAPPED control');
  assert.equal(rows.at(-1).code, 'CAPPED'); assert.equal(rows.at(-1).count, 8);
  assert.equal((await rejection(() => guard.restore())).error, observed);
  assert.throws(() => assertActiveBatchEnvironmentGuard(guard, f.paths));
  assert([...f.backups].every(path => fs.existsSync(path)));
});

test('same guard snapshot retry preserves failed capture history and restores only current attempt records', async t => {
  const f = fixture(t), failure = Object.assign(Error('controlled first capture fault'), { code: 'EACCES' });
  let failFirst = true;
  f.copyFault = (source, destination, options, original) => {
    if (failFirst && source === f.configs[0]) { failFirst = false; throw failure; }
    return original(source, destination, options);
  };
  const guard = f.make(); assert.equal((await rejection(() => guard.prepare())).error, failure);
  const failed = f.evidence(), failedCount = failed.targets?.length ?? 0;
  assert(failedCount > 0, 'First failed capture must have retained target records');
  f.copyFault = undefined;
  await guard.prepare(); assert.equal(f.transitions, 1); assertActiveBatchEnvironmentGuard(guard, f.paths);
  assert.equal(f.evidence().targets.length, failedCount + f.paths.length);
  fs.writeFileSync(resolve(f.artifacts[0], 'library'), 'changed-after-second-capture');
  await guard.restore(); assert.deepEqual(f.paths.map(environmentTreeHash), f.hashes);
  const restored = f.evidence();
  assert.deepEqual(restored.targets.slice(0, failedCount), failed.targets,
    'Later restore must never overwrite earlier failed-capture facts');
  const current = restored.targets.slice(failedCount);
  assert.deepEqual(current.map(row => row.restoredHash), f.hashes);
  assert(failed.targets.every(row => row.captureAttempt === 1));
  assert(current.every(row => row.captureAttempt === 2));
  assert.deepEqual(restored.failures.slice(0, failed.failures.length), failed.failures);
});
