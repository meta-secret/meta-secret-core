import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { randomUUID } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { preparationCommands, iosSimulatorUdid } from './local-run.mjs';
import * as localRunner from './local-run.mjs';
import { batchEnvironmentPaths, createBatchEnvironmentGuard, environmentTreeHash, preflightBatchEnvironment } from '../shared/batch-environment.mjs';
import { buildRegistrationPlan } from './plan.mjs';
import {createEnvironmentTestRun} from '../shared/environment-test-fixture.mjs';

test('preparation declares each app target once, local endpoints and exact approved iOS destination', () => {
  const commands = preparationCommands({ core: '/fixture/core', compose: '/fixture/compose', e2e: '/fixture/core/e2eTest' });
  assert.deepEqual(commands.slice(0, 2).map(({ args }) => args), [['wasm-local'], ['meta-server']]);
  const android = commands.filter(({ command }) => command === './gradlew');
  assert.equal(android.length, 1);
  assert.deepEqual(android[0].args.slice(0, 2), [':composeApp:assembleDebug', ':composeApp:assembleDebugAndroidTest']);
  assert(android[0].args.includes('-PMETA_SECRET_ENV=local'));
  const ios = commands.filter(({ command }) => command === 'xcodebuild');
  assert.equal(ios.length, 1);
  assert.equal(ios[0].args[0], 'build-for-testing');
  assert(ios[0].args.includes(`id=${iosSimulatorUdid}`));
  assert.equal(ios[0].env.GRADLE_USER_HOME, '/fixture/compose/.gradle-home');
  assert.equal(commands.some(({ args }) => args.includes('installDebug') || args.includes('boot')), false);
});

// All effects below target isolated files/stubs, never installed clients or services.
function preparationFixture(t, { mode = 'remote', participants = ['web'], fail, drift, missing, interrupted = false } = {}) {
  const persistent = createEnvironmentTestRun(); t.after(persistent.cleanup);
  const root = mkdtempSync(resolve(tmpdir(), 'test51-preparation-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const core = resolve(root, 'core'), compose = resolve(root, 'meta-secret-compose'), e2e = resolve(core, 'e2eTest');
  const put = (path, bytes) => { mkdirSync(dirname(path), { recursive: true }); writeFileSync(path, bytes); };
  put(resolve(core, '.gitignore'), 'meta-secret/web-cli/ui/pkg/\nmeta-secret/web-cli/ui/.env.local\n');
  assert.equal(spawnSync('git', ['init', '-q', core]).status, 0);
  const paths = batchEnvironmentPaths(core, compose);
  paths.configs.forEach((path, index) => put(path, index === 0
    ? `pub const SELECTED_SERVER_ENVIRONMENT: ServerEnvironment = ServerEnvironment::${mode === 'local' ? 'Local' : 'Remote'};\n`
    : `${index === 1 ? 'VITE_META_SECRET_ENV' : 'META_SECRET_ENV'}=${mode}\nPUBLIC_SETTING=keep\n`));
  put(resolve(compose, 'iosApp/Configuration/Config.xcconfig'), `META_SECRET_ENV=${mode}\nMETA_SECRET_SOCKET_URL=old-value\nPUBLIC_SETTING=keep\n`);
  const script = resolve(core, 'scripts/set-meta-secret-env.sh');
  put(script, readFileSync(new URL('../../scripts/set-meta-secret-env.sh', import.meta.url)));
  const mobileCalls = resolve(core, 'mobile-calls'); paths.artifacts.push(mobileCalls);
  put(resolve(core, 'meta-secret/mobile/scripts/build-mobile.sh'), `#!/bin/bash\nprintf '%s' "$1" >> '${mobileCalls}'\n`);
  put(resolve(core, 'Taskfile.yml'), 'fixture-taskfile'); put(resolve(core, 'docker-bake.hcl'), 'fixture-bake');
  const nativeOutput = paths.artifacts[0]; put(resolve(nativeOutput, 'library'), 'original-native');
  const wasm = resolve(core, 'meta-secret/web-cli/ui/pkg');
  const calls = [], events = [], stages = [];
  const originalHashes = [...paths.configs, ...paths.artifacts].map(environmentTreeHash);
  const env = { PATH: process.env.PATH, E2E_ANDROID_FFI_PREBUILT: '1', E2E_IOS_FFI_PREBUILT: '1', E2E_META_CLI_PREBUILT: '1', E2E_READINESS_PATH: '/foreign/readiness', PUBLIC_SETTING: 'keep' };
  let guard, isInterrupted = false;
  const runCommand = async (command, args) => {
    calls.push([command, ...args]);
    if (args[0] === 'set-env') {
      const result = spawnSync('bash', [script, ...args.slice(2)], { encoding: 'utf8' });
      assert.equal(result.status, 0, result.stderr);
      if (fail === 'selectors') throw new Error('partial selector failure');
      return { exitCode: 0 };
    }
    if (args[0] === 'wasm-local') {
      for (const name of ['meta_secret_wasm.js', 'meta_secret_wasm_bg.wasm']) if (missing !== name) put(resolve(wasm, name), 'built-' + name);
      if (drift === 'source') put(paths.configs[0], readFileSync(paths.configs[0], 'utf8') + '// unrelated drift\n');
      if (drift === 'taskfile') put(resolve(core, 'Taskfile.yml'), 'changed');
      if (interrupted) isInterrupted = true;
    }
    if (fail === args[0]) throw new Error(`failed ${fail}`);
    if (fail === 'nonzero' && args[0] === 'wasm-local') return { exitCode: 1 };
    if (args[0] === 'info') return { exitCode: 0, stdout: '' };
    if (args[0] === 'inspect') return { exitCode: 1, stderr: 'No such container' };
    if (args[0] === 'image') return { exitCode: 0, stdout: missing === 'image' ? '' : 'sha256:fixture-image' };
    if (command === 'xcrun') return { exitCode: 0, stdout: JSON.stringify({ devices: { fixture: [{ name: 'iPhone 17', udid: iosSimulatorUdid }] } }) };
    return { exitCode: 0, stdout: '' };
  };
  const effects = {
    batchEnvironmentPaths: () => ({ configs: [...paths.configs], artifacts: [...paths.artifacts] }),
    createBatchEnvironmentGuard(options) {
      guard = createBatchEnvironmentGuard({ ...options,
        fingerprint: (repo) => ({ digest: [...paths.configs, ...paths.artifacts, resolve(core, 'Taskfile.yml')].filter((path) => path.startsWith(repo + '/')).map(environmentTreeHash) }),
        snapshotBudget: () => ({ requiredBytes: 0 }),
      });
      return guard;
    },
    discoverAndroidSdk: () => { calls.push(['SDK']); return { pathPrefix: '/fixture/sdk' }; },
    preflightBatchEnvironment: (scenarios, options) => {
      if (fail === 'preflight') throw new Error('fixture ownership failure');
      return preflightBatchEnvironment(scenarios, { ...options,
      sdkDiscovery: () => { throw new Error('native inventory should be the existing native path'); },
      portAvailable: async (host, port) => { calls.push(['port', host, port]); },
      });
    },
    invokeOfficialLocalBuild: async ({ runCommand: run }) => { calls.push(['native-build']); return run('/opt/homebrew/bin/task', ['set-env', '--', 'local']); },
    recordOfficialNativeBuild: () => calls.push(['native-record']),
    verifyOfficialNativeBuild: () => calls.push(['native-verify']),
    prepareIosXcodeBuild: async () => { calls.push(['ios-prepare']); return {}; },
    recordIosXcodeBuild: () => calls.push(['ios-record']),
    verifyIosXcodeBuild: () => calls.push(['ios-verify']),
  };
  if (participants.some((value) => value !== 'web')) effects.preflightBatchEnvironment = async (scenarios) => {
    calls.push(['native-preflight']); assert(scenarios[0].ios && scenarios[0].android);
  };
  const scenario = { server: { url: 'http://127.0.0.1:3000', port: 3000, container: 'test51-owned', image: 'fixture-server' }, web: { url: 'http://localhost:5174' }, ios: { simulatorName: 'iPhone 17' }, android: { avdName: 'Pixel_4a' } };
  const options = { core, compose, e2e, participants: new Set(participants), scenario, env,
    runId: 'fixture', runDirectory: persistent.runDirectory, checkpointPath: resolve(e2e, 'checkpoint.json'),
    runCommand, capture: runCommand, effects, isInterrupted: () => isInterrupted,
    recordStage: (stage) => stages.push(stage), onEvent: (event) => events.push(event),
    onGuard: (value) => { guard = value; },
  };
  return { core, compose, paths, calls, stages, events, env, script, mobileCalls, options,
    get guard() { return guard; },
    assertRestored: () => assert.deepEqual([...paths.configs, ...paths.artifacts].map(environmentTreeHash), originalHashes),
    async prepare() {
      assert.equal(typeof localRunner.prepareRegistrationEnvironment, 'function', 'Runtime-used bounded preparation dispatch must exist');
      return localRunner.prepareRegistrationEnvironment(options);
    },
  };
}

for (const mode of ['remote', 'local']) test(`actual ${mode} Web preparation runs only local WASM/server and restores the full guard`, async (t) => {
  const f = preparationFixture(t, { mode });
  try {
    const result = await f.prepare();
    assert.equal(result.webOnly, true);
    assert(!result.runtimeScenario.ios && !result.runtimeScenario.android);
    assert.match(result.webProof.wasmSha256, /^[a-f0-9]{64}$/);
    assert.equal(result.webProof.serverImageId, 'sha256:fixture-image');
    assert.deepEqual(f.calls.filter((call) => call[0] === '/opt/homebrew/bin/task').map((call) => call.slice(1)), [
      ...(mode === 'remote' ? [['set-env', '--', 'local', '--selectors-only']] : []), ['wasm-local'], ['meta-server'],
    ]);
    assert(!f.calls.some((call) => /SDK|native|ios-|xcodebuild|gradlew|xcrun/.test(call[0])));
    assert.equal(existsSync(f.mobileCalls), false);
    assert(f.calls.some((call) => call[0] === 'port' && call[2] === 5174));
    assert.equal(f.env.PUBLIC_SETTING, 'keep');
    assert(!Object.keys(f.env).some((key) => /PREBUILT|READINESS/.test(key)));
  } finally { await f.guard?.restore(); }
  f.assertRestored();
  assert.equal(f.events.filter((event) => event.includes('exact original')).length, 1);
});

for (const [participants, mode] of [[['ios'], 'remote'], [['android'], 'remote'], [['web', 'ios'], 'remote'], [['web', 'ios', 'android'], 'remote'], [['ios'], 'local']]) test(`native dispatch remains complete for ${participants.join('/')}/${mode}`, async (t) => {
  const f = preparationFixture(t, { participants, mode });
  try {
    const result = await f.prepare(); assert.equal(result.webOnly, false);
    assert.equal(f.calls.filter((call) => call[0] === 'native-build').length, 1);
    assert.equal(f.calls.filter((call) => call[0] === 'native-record').length, 1);
    assert.equal(f.calls.filter((call) => call[0] === 'native-verify').length, 2);
    assert.deepEqual(f.calls.filter((call) => call[0] === '/opt/homebrew/bin/task' || call[0] === './gradlew' || call[0] === 'xcodebuild').map((call) => call[1]), ['set-env', 'wasm-local', 'meta-server', ':composeApp:assembleDebug', 'build-for-testing']);
    assert.equal(readFileSync(f.mobileCalls, 'utf8'), 'all');
    assert.equal(f.env.E2E_ANDROID_FFI_PREBUILT, '1');
  } finally { await f.guard?.restore(); }
  f.assertRestored();
});

for (const [setup, expected] of [
  [{ fail: 'preflight' }, /fixture ownership failure/], [{ mode: 'bad' }, /consistent known environment/],
  [{ fail: 'selectors' }, /partial selector failure/], [{ fail: 'wasm-local' }, /failed wasm-local/],
  [{ fail: 'meta-server' }, /failed meta-server/], [{ fail: 'nonzero' }, /successful exit receipt/],
  [{ drift: 'source' }, /source\/configuration changed/], [{ drift: 'taskfile' }, /source\/configuration changed/],
  [{ missing: 'meta_secret_wasm.js' }, /ENOENT/], [{ missing: 'meta_secret_wasm_bg.wasm' }, /ENOENT/],
  [{ missing: 'image' }, /image identity is missing/], [{ interrupted: true }, /interrupted/],
]) {
  test(`Web preparation fails closed and restores after ${JSON.stringify(setup)}`, async (t) => {
    const f = preparationFixture(t, setup);
    try { await assert.rejects(f.prepare(), expected); }
    finally {
      if (setup.drift === 'taskfile') await assert.rejects(f.guard.restore(), /restoration failed/);
      else await f.guard?.restore();
    }
    f.assertRestored();
    assert(!f.calls.some((call) => /SDK|native|ios-|xcodebuild|gradlew|xcrun/.test(call[0])));
    if (['preflight', 'selectors', 'wasm-local', 'nonzero'].includes(setup.fail)) assert(!f.calls.some((call) => call[1] === 'meta-server'));
  });
}

test('scope rejects empty/unknown participants before any preparation effect', async (t) => {
  for (const participants of [[], ['web', 'unknown']]) {
    const f = preparationFixture(t, { participants });
    await assert.rejects(f.prepare(), /participant/i);
    assert.deepEqual(f.calls, []); f.assertRestored();
  }
});

test('Web JOIN selection retains native approvers and retries', () => {
  const plan = buildRegistrationPlan('fixture');
  for (const entry of plan.cases.filter(({ kind, platform }) => kind === 'join' && platform === 'web')) {
    const actors = new Set([entry.platform, entry.approver, ...(entry.outcome === 'accepted' ? [] : [entry.retryApprover])]);
    assert(actors.has('ios')); if (entry.outcome !== 'accepted') assert(actors.has('android'));
  }
});

test('official script keeps default local/remote builds; explicit flag alone suppresses mobile build', (t) => {
  for (const mode of ['local', 'remote']) for (const flag of [[], ['--selectors-only']]) {
    const f = preparationFixture(t);
    const result = spawnSync('bash', [f.script, mode, ...flag], { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    assert.equal(existsSync(f.mobileCalls), flag.length === 0);
    if (!flag.length) assert.equal(readFileSync(f.mobileCalls, 'utf8'), 'all');
    assert(readFileSync(f.paths.configs[1], 'utf8').includes(`VITE_META_SECRET_ENV=${mode}`));
    assert(readFileSync(f.paths.configs[3], 'utf8').includes('META_SECRET_SOCKET_URL=\n'));
  }
});

test('official script rejects invalid arguments before touching any selector or build', (t) => {
  for (const args of [[], ['bad'], ['local', '--bad'], ['local', '--selectors-only', '--extra']]) {
    const f = preparationFixture(t);
    const result = spawnSync('bash', [f.script, ...args], { encoding: 'utf8' });
    assert.equal(result.status, 2); f.assertRestored();
  }
});

test('validate-only executes pure F1, exits before local preparation and leaves selectors unchanged', () => {
  const script = fileURLToPath(new URL('./local-run.mjs', import.meta.url));
  const selector = new URL('../../meta-secret/core/src/node/app/sync/environment.rs', import.meta.url);
  const before = readFileSync(selector);
  const result = spawnSync(process.execPath, [script, '--validate-only'], { encoding: 'utf8', timeout: 10000 });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /cases: 45/);
  assert.match(result.stdout, /repetitions: 1/);
  assert.doesNotMatch(result.stdout, /official native|preparation passed|real Core UI cases start/);
  assert.deepEqual(readFileSync(selector), before);
});

test('valid full, first, single, selected cases and suffix entry points reach safe preflight and release their heartbeat', () => {
  const allPlatforms = ['android', 'ios', 'web'];
  const selections = [
    { args: [], first: 'create-web-manual', count: 45, expectedPlatforms: allPlatforms },
    { args: ['--first-case=create-ios-manual'], first: 'create-ios-manual', count: 45, expectedPlatforms: allPlatforms },
    { args: ['--case=create-ios-manual'], first: 'create-ios-manual', count: 1, expectedPlatforms: ['ios'] },
    { args: ['--cases=join-web-manual-accepted,join-ios-manual-accepted,join-android-manual-accepted'], first: 'join-web-manual-accepted', count: 3, expectedPlatforms: allPlatforms },
    { args: ['--from-case=join-android-manual-declined-retry'], first: 'join-android-manual-declined-retry', count: 21, expectedPlatforms: allPlatforms },
  ];
  for (const { args, first, count, expectedPlatforms } of selections) {
    const runId = `f1-preflight-${randomUUID()}`;
    const artifacts = new URL(`../.artifacts/runs/${runId}/`, import.meta.url);
    const script = `
      import assert from 'node:assert/strict';
      import { readFileSync } from 'node:fs';
      import { runLocalRegistration } from ${JSON.stringify(new URL('./local-run.mjs', import.meta.url).href)};
      const selector = new URL(${JSON.stringify(new URL('../../meta-secret/core/src/node/app/sync/environment.rs', import.meta.url).href)});
      const before = readFileSync(selector);
      const sentinel = new Error('SAFE_PREFLIGHT_BOUNDARY');
      let reached = false;
      await assert.rejects(runLocalRegistration(${JSON.stringify(args)}, {
        beforePreflight({ selected, participants }) {
          reached = true;
          assert.equal(selected.length, ${count});
          assert.equal(selected[0].id, ${JSON.stringify(first)});
          assert.equal(new Set(selected.map(({ id }) => id)).size, ${count});
          assert.deepEqual([...participants].sort(), ${JSON.stringify(expectedPlatforms)});
          throw sentinel;
        },
      }), (error) => error === sentinel);
      assert(reached);
      const receipt = JSON.parse(readFileSync(new URL('test51-preparation/build-receipt.json', ${JSON.stringify(artifacts.href)}), 'utf8'));
      assert.equal(receipt.requestedCases, ${count});
      assert.equal(receipt.status, 'failed');
      assert.equal(receipt.error, sentinel.message);
      assert.equal(receipt.stages.length, 0);
      assert.deepEqual(readFileSync(selector), before);
      assert.equal(process.listenerCount('SIGINT'), 0);
      assert.equal(process.listenerCount('SIGTERM'), 0);
      console.log('SAFE_PREFLIGHT_FINALLY_PASSED');
    `;
    try {
      const result = spawnSync(process.execPath, ['--input-type=module', '--eval', script], {
        encoding: 'utf8', timeout: 3000, killSignal: 'SIGKILL', env: { ...process.env, E2E_RUN_ID: runId },
      });
      assert.equal(result.error, undefined, 'Valid preflight must exit naturally without a leaked timer');
      assert.equal(result.status, 0, result.stderr);
      assert.match(result.stdout, /SAFE_PREFLIGHT_FINALLY_PASSED/);
      assert.doesNotMatch(result.stdout, /official native|preparation passed|real Core UI cases start/);
    } finally { rmSync(artifacts, { recursive: true, force: true }); }
  }
});
