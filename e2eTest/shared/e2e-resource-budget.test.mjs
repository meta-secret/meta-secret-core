import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync, symlinkSync, lstatSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createBatchProductPreparation } from './batch-product-builders.mjs';
import { GiB, E2E_BUILD_PROFILE, e2eGradleArguments, e2eBuildEnvironment, planPreparationBudget,
  assertPreparationResources, createPreparationResourceBudget, measurePreparationInputs,
  readPreparationObservation } from './e2e-resource-budget.mjs';

function actualOwningBuilder(t, product, resourceBudget = { check: async () => {}, complete() {} }) {
  const root = mkdtempSync(join(tmpdir(), 'e2e-resource-owning-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const paths = { coreRoot: join(root, 'core'), composeRoot: join(root, 'compose'), runDirectory: join(root, 'run') };
  const provenance = join(paths.coreRoot, 'e2eTest/.artifacts/android-ffi-build-provenance.json');
  mkdirSync(join(provenance, '..'), { recursive: true }); writeFileSync(provenance, '{}');
  const captured = new Error('Controlled owning command captured before execution');
  const calls = []; let nativeCalls = 0;
  const graph = createBatchProductPreparation({ paths, runId: 'resource-command-fixture', resourceBudget,
    scenarios: [{ testNumber: 1, scenario: { web: {}, [product]: {}, server: { image: 'controlled-image' } } }],
    prepareNative: async () => { nativeCalls++; return { android: product === 'android', ios: product === 'ios', cli: false }; },
    commands: { checkBudget: () => ({ requiredBytes: 0 }),
      proofPorts: { derivedDataPath: join(root, 'derived'),
        captureInputs: async tuple => ({ version: 3, scope: ['native'], sourceFingerprint: 'controlled-current',
          configuration: tuple.configuration, target: tuple.target, inputHashes: { ffiSource: 'controlled-current', ffiAbis: {} } }),
        inspectProducts: () => assert.fail('Captured command cannot reach output inspection'),
      },
      iosBuild: { prepare: async () => ({ identityRequired: 'YES' }), record: () => assert.fail('No executed Xcode build') },
      runAndWait: async (file, args, options) => { calls.push({ file, args, options }); throw captured; },
      runAndCapture: async () => assert.fail('No real toolchain or server process'),
    },
  });
  return { graph, calls, captured, nativeCalls: () => nativeCalls };
}

test('optimization baseline: actual Android owning command carries bounded shared compiler profile', async t => {
  const f = actualOwningBuilder(t, 'android');
  await assert.rejects(f.graph.prepare(), error => error === f.captured);
  assert.equal(f.calls.length, 1); assert.equal(f.calls[0].file, './gradlew');
  assert(f.calls[0].args.includes('-PMETA_SECRET_ENV=local'));
  for (const argument of ['--no-daemon', '--no-parallel', '--max-workers=1',
    '-Dorg.gradle.jvmargs=-Xmx3072m -XX:MaxMetaspaceSize=768m -Dfile.encoding=UTF-8',
    '-Pkotlin.compiler.execution.strategy=in-process'])
    assert(f.calls[0].args.includes(argument), 'Owning Gradle must retain bounded argument: ' + argument);
});

test('optimization baseline: actual iOS owning command limits jobs and embedded Gradle', async t => {
  const f = actualOwningBuilder(t, 'ios');
  await assert.rejects(f.graph.prepare(), error => error === f.captured);
  assert.equal(f.calls.length, 1); const call = f.calls[0]; assert.equal(call.file, 'xcodebuild');
  assert(call.args.includes('METASECRET_E2E_IDENTITY_REQUIRED=YES'));
  assert.equal(call.args[call.args.indexOf('-jobs') + 1], '1', 'Owning Xcode build must use one job');
  assert.match(call.options.env?.GRADLE_OPTS ?? '', /-Xmx3072m/);
  assert.equal(call.options.env?.['ORG_GRADLE_PROJECT_kotlin.compiler.execution.strategy'], 'in-process');
});

test('optimization baseline: full remaining resource refusal precedes native work and owning commands', async t => {
  const denied = new Error('Controlled whole-F2 RAM/disk reservation refusal');
  const f = actualOwningBuilder(t, 'android', { check: async () => { throw denied; }, complete() {} });
  let actual; try { await f.graph.prepare(); } catch (error) { actual = error; }
  assert.equal(f.nativeCalls(), 0, 'Unsafe preparation cannot enter the native producer');
  assert.equal(f.calls.length, 0, 'Unsafe preparation cannot start any owning command');
  assert.equal(actual, denied, 'Original resource refusal remains primary');
});

const fullInput = () => ({ snapshotBytes: GiB, androidConfigurationCount: 3,
  androidConfigurationKeys: ['defaultLocal', 'network16', 'deletion50'], iosRequired: true, wasmRequired: true, cliRequired: true,
  androidPairBytes: GiB / 4, iosPairBytes: 3 * GiB / 4, wasmBytes: GiB / 8,
  cliBytes: GiB / 8, nativeBytes: GiB, supportingBytes: GiB / 2 });
const observation = freeBytes => ({ freeBytes, physicalBytes: 16 * GiB, availableBytes: 3 * GiB,
  swapUsedBytes: 6 * GiB, swapTotalBytes: 8 * GiB });

test('full F2 reservation includes every stash, support, server, cache and swap above unchanged12GiB', () => {
  const budget = planPreparationBudget(fullInput());
  assert.equal(budget.components.androidStashes, 1.5 * GiB);
  assert.equal(budget.components.iosWorking, 1.5 * GiB); assert.equal(budget.components.iosStash, 1.5 * GiB);
  assert.equal(budget.components.supporting, 4.5 * GiB); assert.equal(budget.components.serverWorking, 4 * GiB);
  assert.equal(budget.components.workingAllowance, 4 * GiB); assert.equal(budget.components.cacheAllowance, 2 * GiB);
  assert.equal(budget.components.swapAllowance, 4 * GiB); assert.equal(budget.minimumRemainingBytes, 12 * GiB);
  assert.equal(budget.requiredBytes, 27.25 * GiB); assert.equal(budget.minimumFreeBytes, 39.25 * GiB);
  assert(Object.isFrozen(budget)); assert(Object.isFrozen(budget.components));
  const result = assertPreparationResources({ budget, observation: observation(budget.minimumFreeBytes) });
  assert.equal(result.remainingBytes, 12 * GiB, 'Exact byte boundary is admitted without rounding');
  assert.throws(() => assertPreparationResources({ budget, observation: observation(budget.minimumFreeBytes - 1) }), /disk|12 GiB/);
});

test('missing size evidence uses disclosed envelopes; malformed sizes and inventories cannot become zero', () => {
  for (const missing of [undefined, null, 0]) {
    const budget = planPreparationBudget({ ...fullInput(), androidPairBytes: missing, nativeBytes: missing });
    assert(budget.components.androidWorking >= GiB / 2); assert(budget.components.nativeWorking >= GiB);
  }
  for (const value of [NaN, Infinity, -1, 0.5, Number.MAX_SAFE_INTEGER]) {
    assert.throws(() => planPreparationBudget({ ...fullInput(), androidPairBytes: value }));
  }
  for (const change of [{ androidConfigurationCount: 4 }, { androidConfigurationCount: NaN },
    { androidConfigurationKeys: ['defaultLocal', 'defaultLocal', 'deletion50'] },
    { androidConfigurationKeys: ['defaultLocal', 'network16', 'foreign'] }, { iosRequired: 'true' }, { snapshotBytes: -1 }])
    assert.throws(() => planPreparationBudget({ ...fullInput(), ...change }));
  for (const change of [{ minimumRemainingBytes: 12 * GiB - 1 }, { minimumPhysicalBytes: 16 * GiB - 1 },
    { minimumHeadroomBytes: 3 * GiB - 1 }])
    assert.throws(() => planPreparationBudget(fullInput(), { profile: { ...E2E_BUILD_PROFILE, ...change } }));
});

test('RAM, swap and unknown resource counters fail closed without launching any external port', () => {
  const budget = planPreparationBudget(fullInput()), good = observation(budget.minimumFreeBytes);
  for (const change of [{ physicalBytes: 16 * GiB - 1 }, { availableBytes: 3 * GiB - 1 },
    { availableBytes: 17 * GiB }, { swapUsedBytes: 9 * GiB }, { freeBytes: NaN },
    { swapTotalBytes: undefined }, { swapUsedBytes: -1 }, { freeBytes: 0.5 }])
    assert.throws(() => assertPreparationResources({ budget, observation: { ...good, ...change } }));
  const lowFreeAfterSwapGrowth = { ...good, freeBytes: good.freeBytes - GiB, swapUsedBytes: 7 * GiB };
  assert.throws(() => assertPreparationResources({ budget, observation: lowFreeAfterSwapGrowth }));
});

test('phase completion tracks exact configurations and keeps allowances until the complete seal', async () => {
  const budget = planPreparationBudget(fullInput()); let sample = observation(budget.minimumFreeBytes);
  const ledger = createPreparationResourceBudget({ budget, observe: async () => sample });
  await ledger.check(); ledger.complete('snapshot'); ledger.complete('native'); ledger.complete('android', 'defaultLocal');
  assert.equal((await ledger.check()).requiredBytes, 23.75 * GiB);
  const copy = ledger.snapshot(); copy.androidConfigurations.push('deletion50'); copy.sealed = true;
  assert.deepEqual(ledger.snapshot().androidConfigurations, ['defaultLocal']);
  assert.throws(() => ledger.complete('android', 'defaultLocal')); assert.throws(() => ledger.complete('android', 'unknown'));
  assert.throws(() => ledger.complete('sealed')); assert.throws(() => ledger.complete('unknown'));
  for (const key of ['network16', 'deletion50']) ledger.complete('android', key);
  for (const stage of ['ios', 'wasm', 'cli', 'server', 'supporting']) ledger.complete(stage);
  sample = observation(22 * GiB); assert.equal((await ledger.check()).requiredBytes, 10 * GiB);
  ledger.complete('sealed'); sample = observation(12 * GiB); assert.equal((await ledger.check()).requiredBytes, 0);
  assert.throws(() => ledger.complete('sealed')); sample = observation(12 * GiB - 1); await assert.rejects(ledger.check());
});

test('replay reserves copies only and cannot silently become a fresh owning build', () => {
  const fresh = planPreparationBudget(fullInput()), replay = planPreparationBudget({ ...fullInput(), mode: 'replay' });
  assert.equal(replay.mode, 'replay'); assert(replay.requiredBytes < fresh.requiredBytes);
  for (const key of ['androidStashes', 'iosStash', 'wasmStash', 'nativeWorking', 'serverWorking', 'cacheAllowance', 'swapAllowance'])
    assert.equal(replay.components[key], 0, 'Replay must not reserve unplanned compilation: ' + key);
  assert.equal(replay.components.snapshot, GiB); assert(replay.components.supporting >= 4.5 * GiB);
  assert.equal(replay.minimumRemainingBytes, 12 * GiB);
  assert.throws(() => planPreparationBudget({ ...fullInput(), mode: 'unknown' }));
});

test('bounded arguments and environment are detached values and never rewrite developer config', () => {
  const first = e2eGradleArguments(), second = e2eGradleArguments(); first.push('foreign');
  assert.equal(second.includes('foreign'), false); assert(second.includes('--no-daemon')); assert(second.includes('--no-parallel'));
  assert(second.includes('--max-workers=1')); assert(second.includes('-Pkotlin.compiler.execution.strategy=in-process'));
  const env = e2eBuildEnvironment(); assert.match(env.GRADLE_OPTS, /-Xmx3072m/);
  assert.match(env.GRADLE_OPTS, /MaxMetaspaceSize=768m/); assert.match(env.GRADLE_OPTS, /org.gradle.workers.max=1/);
  assert.equal(env['ORG_GRADLE_PROJECT_kotlin.compiler.execution.strategy'], 'in-process');
  env.GRADLE_OPTS = 'foreign'; assert.notEqual(e2eBuildEnvironment().GRADLE_OPTS, 'foreign');
});

test('sizing counts exact selected configurations and CLI participation without granting old-proof reuse', t => {
  const root = mkdtempSync(join(tmpdir(), 'e2e-resource-size-')); t.after(() => rmSync(root, { recursive: true, force: true }));
  const paths = { coreRoot: join(root, 'core'), composeRoot: join(root, 'compose'), runDirectory: join(root, 'run') };
  const scenarios = [1, 16, 50].map(testNumber => ({ testNumber, scenario: { android: {}, ios: {}, web: {},
    postJoinRecoveryPlan: [{ sender: 'android', approver: 'ios' }],
    ...(testNumber === 1 ? {} : { networkLoss: { androidProxyPort: testNumber === 16 ? 5181 : 5201 } }) } }));
  const input = measurePreparationInputs({ paths, scenarios, measureTreeBytes: () => 1 });
  assert.equal(input.snapshotBytes, 22); assert.equal(input.androidConfigurationCount, 3);
  assert.deepEqual(input.androidConfigurationKeys, ['defaultLocal', 'network16', 'deletion50']); assert.equal(input.cliRequired, false);
  scenarios[0].scenario.postJoinRecoveryPlan[0].approver = 'cli';
  assert.equal(measurePreparationInputs({ paths, scenarios, measureTreeBytes: () => 1 }).cliRequired, true);
});

test('Darwin observation uses only bounded closed ports and does not double count speculative memory', async () => {
  const calls = [], observation = await readPreparationObservation({ path: '/controlled/artifact-volume', platform: 'darwin',
    statfs: path => { assert(['/controlled/artifact-volume', tmpdir()].includes(path)); return { bavail: 64 * 1024, bsize: 1024 ** 2 }; },
    totalmem: () => 16 * GiB, runAndCapture: async (file, args, options) => {
      calls.push({ file, args, options });
      return { exitCode: 0, stdout: file.endsWith('vm_stat')
        ? 'Mach Virtual Memory Statistics: (page size of 16384 bytes)\nPages free: 131072.\nPages inactive: 65536.\nPages speculative: 999999.\nPages purgeable: 999999.\n'
        : 'vm.swapusage: total = 8192.00M used = 6144.00M free = 2048.00M' };
    } });
  assert.equal(observation.availableBytes, 3 * GiB); assert.equal(observation.freeBytes, 64 * GiB);
  assert.equal(observation.swapUsedBytes, 6 * GiB); assert.equal(observation.swapTotalBytes, 8 * GiB);
  assert.deepEqual(calls.map(call => call.file), ['/usr/bin/vm_stat', '/usr/sbin/sysctl']);
  assert(calls.every(call => call.options.timeoutMs === 5000 && call.options.maxCaptureBytes === 65536));
  await assert.rejects(readPreparationObservation({ path: '/unused', platform: 'linux', runAndCapture: () => assert.fail('No unsupported host probe') }));
});

test('malformed or failed OS observations refuse without exposing raw counters or using a default', async () => {
  const vm = 'Mach Virtual Memory Statistics: (page size of 16384 bytes)\nPages free: 131072.\nPages inactive: 65536.\n';
  const swap = 'vm.swapusage: total = 8192.00M used = 6144.00M free = 2048.00M';
  for (const change of [{ vm: 'RAW_PRIVATE_COUNTER_CANARY' }, { vm: vm.replace('131072', '9999999') },
    { swap: swap.replace('6144.00M', '9000.00M') }, { swap: 'RAW_PRIVATE_COUNTER_CANARY' }, { exitCode: 1 }, { disk: NaN }]) {
    let failure;
    try { await readPreparationObservation({ path: '/controlled/volume', platform: 'darwin', totalmem: () => 16 * GiB,
      statfs: () => ({ bavail: change.disk ?? 64, bsize: GiB }), runAndCapture: async file => ({ exitCode: change.exitCode ?? 0,
        stdout: file.endsWith('vm_stat') ? change.vm ?? vm : change.swap ?? swap }) }); }
    catch (error) { failure = error; }
    assert(failure, 'Malformed observations cannot authorize resource admission');
    assert.equal(failure.message.includes('RAW_PRIVATE_COUNTER_CANARY'), false);
  }
});

test('replay-mode factory without an existing sealed index refuses before every producer', async t => {
  const f = actualOwningBuilder(t, 'android', { mode: 'replay', check: async () => {}, complete() {} });
  await assert.rejects(f.graph.prepare(), /replay|persisted|proof|index/i);
  assert.equal(f.nativeCalls(), 0); assert.equal(f.calls.length, 0);
});

test('failed owning command cannot consume its tuple or seal reservations', async t => {
  const complete = [], f = actualOwningBuilder(t, 'android', { check: async () => {}, complete: (...args) => complete.push(args) });
  await assert.rejects(f.graph.prepare(), error => error === f.captured);
  assert.deepEqual(complete, [['native', undefined]], 'Only the already-successful native producer may release its reservation');
});

test('resource observation keeps the smaller workspace or snapshot-temp volume budget', async () => {
  for (const [workspace, temporary] of [[8, 20], [20, 8]]) {
    const volumes = [];
    const result = await readPreparationObservation({ path: '/controlled/workspace', platform: 'darwin', totalmem: () => 16 * GiB,
      statfs: path => { volumes.push(path); assert(['/controlled/workspace', tmpdir()].includes(path));
        return { bavail: path === '/controlled/workspace' ? workspace : temporary, bsize: GiB }; },
      runAndCapture: async file => ({ exitCode: 0, stdout: file.endsWith('vm_stat')
        ? 'Mach Virtual Memory Statistics: (page size of 16384 bytes)\nPages free: 131072.\nPages inactive: 65536.\n'
        : 'vm.swapusage: total = 8192M used = 6144M free = 2048M' }) });
    assert.equal(result.freeBytes, 8 * GiB); assert.deepEqual(volumes, ['/controlled/workspace', tmpdir()]);
  }
});

test('genuine tiny known WASM bytes reserve doubled measurements rather than unknown-size floors', () => {
  const measured = 6 * 1024 ** 2;
  const budget = planPreparationBudget({ snapshotBytes: 0, androidConfigurationCount: 0, androidConfigurationKeys: [],
    iosRequired: false, wasmRequired: true, cliRequired: false, wasmBytes: measured, supportingBytes: measured });
  assert.equal(budget.components.wasmWorking, 2 * measured); assert.equal(budget.components.wasmStash, 2 * measured);
  assert.equal(budget.components.supporting, 2 * measured); assert.equal(budget.minimumRemainingBytes, 12 * GiB);
});

test('snapshot sizing counts a derived symlink itself without reading its external target', t => {
  const root = mkdtempSync(join(tmpdir(), 'e2e-resource-symlink-')); t.after(() => rmSync(root, { recursive: true, force: true }));
  const paths = { coreRoot: join(root, 'core'), composeRoot: join(root, 'compose'), runDirectory: join(root, 'run') };
  const target = join(root, 'unrelated-source'), directory = join(paths.composeRoot, 'composeApp/build/bin/iosArm64');
  writeFileSync(target, Buffer.alloc(4096)); mkdirSync(directory, { recursive: true });
  const link = join(directory, 'derived-link'); symlinkSync(target, link);
  const input = measurePreparationInputs({ paths, scenarios: [] });
  assert.equal(input.snapshotBytes, lstatSync(link).size); assert(input.snapshotBytes < 4096);
});

function recoveryFixture({ observe, wait } = {}) {
  const budget = planPreparationBudget(fullInput());
  const good = observation(budget.minimumFreeBytes);
  let time = 0, samples = 0;
  const waits = [];
  const ledger = createPreparationResourceBudget({ budget,
    observe: async () => { samples++; return observe ? observe({ good, samples, advance: ms => { time += ms; } }) : { ...good, availableBytes: 0 }; },
    recovery: { now: () => time, wait: async ms => { waits.push(ms); time += ms; await wait?.(ms); } },
  });
  return { ledger, good, waits, samples: () => samples, time: () => time };
}

test('postbuild RAM recovery uses fresh state and one bounded async observation interval', async () => {
  const f = recoveryFixture({ observe: ({ good, samples }) => ({ ...good, availableBytes: samples === 1 ? 0 : 3 * GiB }) });
  const result = await f.ledger.check({ recoverHeadroom: true });
  assert.equal(result.availableBytes, 3 * GiB); assert.equal(f.samples(), 2); assert.deepEqual(f.waits, [5000]);
});

test('per eligible check permanently low RAM stops at180s without busy wait or late observation', async () => {
  const f = recoveryFixture();
  await assert.rejects(f.ledger.check({ recoverHeadroom: true }), /deadline|timed out|timeout/i);
  assert.equal(f.time(), 180000); assert.equal(f.samples(), 36);
  assert.equal(f.waits.length, 36); assert(f.waits.every(ms => ms === 5000));
});

test('RAM recovery never masks concurrent disk physical or invalid observation refusal', async () => {
  for (const change of [{ freeBytes: 0 }, { physicalBytes: 15 * GiB }, { freeBytes: NaN },
    { swapUsedBytes: 9 * GiB }, { availableBytes: undefined }]) {
    const f = recoveryFixture({ observe: ({ good }) => ({ ...good, availableBytes: 0, ...change }) });
    await assert.rejects(f.ledger.check({ recoverHeadroom: true }));
    assert.equal(f.samples(), 1); assert.deepEqual(f.waits, []);
  }
});

test('observation error remains original and does not enter RAM recovery', async () => {
  const failure = new Error('controlled observation failure');
  const f = recoveryFixture({ observe: () => { throw failure; } });
  await assert.rejects(f.ledger.check({ recoverHeadroom: true }), error => error === failure);
  assert.deepEqual(f.waits, []);
});

test('ordinary precompiler check remains failfast under low RAM', async () => {
  const f = recoveryFixture(); await assert.rejects(f.ledger.check(), /RAM headroom/);
  assert.equal(f.samples(), 1); assert.deepEqual(f.waits, []);
});

test('valid observation returning beyond deadline cannot grant admission', async () => {
  const f = recoveryFixture({ observe: ({ good, advance }) => { advance(180001); return good; } });
  await assert.rejects(f.ledger.check({ recoverHeadroom: true }), /deadline|timed out|timeout/i);
  assert.equal(f.samples(), 1); assert.deepEqual(f.waits, []);
});

test('RAM wait checks guard after await before another observation', async () => {
  const failure = new Error('controlled retired guard'); let active = true;
  const f = recoveryFixture({ wait: () => { active = false; } });
  await assert.rejects(f.ledger.check({ recoverHeadroom: true,
    assertActive: () => { if (!active) throw failure; } }), error => error === failure);
  assert.equal(f.samples(), 1); assert.deepEqual(f.waits, [5000]);
});

test('guard retirement during observation rejects even a valid returned observation', async () => {
  let active = true; const failure = new Error('controlled guard retired in probe');
  const f = recoveryFixture({ observe: ({ good }) => { active = false; return good; } });
  await assert.rejects(f.ledger.check({ recoverHeadroom: true,
    assertActive: () => { if (!active) throw failure; } }), error => error === failure);
  assert.deepEqual(f.waits, []);
});

test('cancel during RAM wait permits no subsequent observation', async () => {
  const controller = new AbortController();
  const f = recoveryFixture({ wait: () => controller.abort(new Error('controlled cancel')) });
  await assert.rejects(f.ledger.check({ recoverHeadroom: true, signal: controller.signal }), /cancel|abort/i);
  assert.equal(f.samples(), 1); assert.deepEqual(f.waits, [5000]);
});

test('last RAM wait is capped by remaining monotonic per-check deadline', async () => {
  const f = recoveryFixture({ observe: ({ good, advance }) => { advance(179000); return { ...good, availableBytes: 0 }; } });
  await assert.rejects(f.ledger.check({ recoverHeadroom: true }), /deadline|timed out|timeout/i);
  assert.deepEqual(f.waits, [1000]); assert.equal(f.samples(), 1);
});

test('server compiler working release frees only its exact component, never product preparation or floor', async () => {
  const budget = planPreparationBudget(fullInput()), ledger = createPreparationResourceBudget({ budget,
    observe: async () => sample });
  let sample = observation(budget.minimumFreeBytes);
  const original = await ledger.check();
  sample = observation(budget.minimumFreeBytes - 1);
  await assert.rejects(ledger.check(), /disk|reservation|floor/i);
  ledger.complete('serverWorkingComplete');
  const after = await ledger.check();
  assert.equal(original.requiredBytes - after.requiredBytes, budget.components.serverWorking);
  assert.equal(budget.requiredBytes, original.requiredBytes, 'Initial full budget is unchanged');
  assert.equal(ledger.snapshot().server, undefined); assert.equal(ledger.snapshot().sealed, undefined);
  assert.throws(() => ledger.complete('sealed'));
  assert.throws(() => ledger.complete('serverWorkingComplete'), /duplicate/i);
  sample = observation(after.requiredBytes + budget.minimumRemainingBytes - 1);
  await assert.rejects(ledger.check(), /disk|reservation|floor/i);
  assert.throws(() => assertPreparationResources({ budget, observation: observation(budget.minimumFreeBytes),
    completed: { serverWorkingComplete: 'true' } }));
  assert.throws(() => ledger.complete('unknownWorkingComplete'));
});
