import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import vm from 'node:vm';
import test from 'node:test';
import { iosSerialTestArguments } from './platform-supervisor.mjs';
import { e2eBuildEnvironment } from './e2e-resource-budget.mjs';

const udid = 'DB22263E-A8A3-49C5-ADD1-BAE4089E2E30'; // Pure command fixture; never queried or launched.
function actual(file, start, end) {
  const source = readFileSync(new URL(file, import.meta.url), 'utf8');
  const first = source.indexOf(start);
  const last = end ? source.indexOf(end, first + start.length) : source.length;
  assert.ok(first >= 0 && last > first, 'Actual function boundary missing');
  return source.slice(first, last).replace(/^export /, '');
}
function fixture() {
  const stop = new Error('Controlled owning build failure');
  const calls = []; let prepared = 0;
  const unexpected = () => assert.fail('Failed build must not record, install or start an actor');
  const context = vm.createContext({
    resolve, console: { log() {} }, iosSerialTestArguments, e2eBuildEnvironment,
    coreRoot: '/fixture/core', composeRoot: '/fixture/compose',
    iosProjectPath: '/fixture/compose/iosApp/iosApp.xcodeproj',
    iosDerivedDataPath: '/fixture/derived', iosGradleUserHome: '/fixture/gradle',
    batchProducts: null, assertBatchProductsCurrent() {},
    scenario: { vault: { name: 'controlled-vault' }, secret: { name: 'controlled-name', value: 'controlled-value' } },
    recoveryCycles: [], iosRecoveryApprovalCycles: [], networkLossConfig: null,
    networkProxyUrl: () => 'http://127.0.0.1:5201',
    prepareIosXcodeBuild: async options => {
      assert.equal(options.identityRequired, true); prepared++; return { currentLink: true };
    },
    recordIosXcodeBuild: unexpected, installBuiltIosTestApps: unexpected,
    verifyIosXcodeBuild: unexpected, writeDiagnostic: unexpected,
    runAndCapture: unexpected,
    runAndWait: async (file, args, options) => {
      calls.push({ file, args: [...args], options }); throw stop;
    },
  });
  return { context, calls, stop, prepared: () => prepared };
}
function assertOwningCommand(f, destination) {
  assert.equal(f.prepared(), 1, 'Official current native/link preparation remains required');
  assert.equal(f.calls.length, 1, 'No owning build retry');
  const { file, args } = f.calls[0];
  assert.equal(file, 'xcodebuild'); assert.equal(args[0], 'build-for-testing');
  assert.equal(args.filter(value => value === '-sdk').length, 1, 'Simulator SDK must be explicit exactly once');
  assert.equal(args[args.indexOf('-sdk') + 1], 'iphonesimulator');
  assert.deepEqual(args.filter(value => value.startsWith('ARCHS=')), ['ARCHS=arm64']);
  assert.deepEqual(args.filter(value => value.startsWith('ONLY_ACTIVE_ARCH=')), ['ONLY_ACTIVE_ARCH=YES']);
  assert.deepEqual(args.filter(value => value.startsWith('METASECRET_E2E_IDENTITY_REQUIRED=')), ['METASECRET_E2E_IDENTITY_REQUIRED=YES']);
  assert.equal(args[args.indexOf('-destination') + 1], destination);
  assert.equal(args.some(value => /^(CODE_SIGN|DEVELOPMENT_TEAM|PROVISIONING_PROFILE)/.test(value)), false, 'Default signing must remain');
}

test('actual batch owning iOS builder requests only the admitted arm64 simulator target', async () => {
  const f = fixture(); let builders;
  Object.assign(f.context, {
    existsSync: () => false,
    createBatchBuildHandoff: options => { builders = options.builders; return {}; },
  });
  vm.runInContext(actual('./batch-product-builders.mjs', 'export function createBatchProductPreparation('), f.context);
  f.context.createBatchProductPreparation({
    paths: { coreRoot: '/fixture/core', composeRoot: '/fixture/compose', runDirectory: '/fixture/run' },
    commands: { runAndWait: f.context.runAndWait, runAndCapture: f.context.runAndCapture,
      proofPorts: { derivedDataPath: '/fixture/derived' },
      iosBuild: { prepare: f.context.prepareIosXcodeBuild, record: f.context.recordIosXcodeBuild } },
    scenarios: [], runId: 'controlled-batch', prepareNative: () => assert.fail('Not a native build test'),
  });
  await assert.rejects(builders.ios({}), error => error === f.stop);
  assertOwningCommand(f, 'generic/platform=iOS Simulator');
});

const readiness = () => actual('./platform-readiness.mjs', 'export async function prepareIosTestApps(');
test('actual shared Test1/Test3 fallback keeps destination, selector and custom settings with simulator build flags', async () => {
  const f = fixture(); vm.runInContext(readiness(), f.context);
  await assert.rejects(f.context.prepareIosTestApps({
    simulatorUdid: udid, projectPath: f.context.iosProjectPath, derivedDataPath: '/fixture/derived',
    testSelector: 'Fixture/actualMethod', buildSettings: ['META_SECRET_ENV=local'],
    runAndWait: f.context.runAndWait, runAndCapture: f.context.runAndCapture,
  }), error => error === f.stop);
  assertOwningCommand(f, `id=${udid}`);
  assert.ok(f.calls[0].args.includes('-only-testing:Fixture/actualMethod'));
  assert.ok(f.calls[0].args.includes('META_SECRET_ENV=local'));
});

test('actual shared verified-product path installs without requesting another build', async () => {
  const f = fixture(); const proof = { controlled: true }; let installed = 0;
  f.context.installBuiltIosTestApps = async options => { assert.equal(options.verifiedBuild, proof); installed++; return 'installed'; };
  vm.runInContext(readiness(), f.context);
  assert.equal(await f.context.prepareIosTestApps({ simulatorUdid: udid, verifiedBuild: proof }), 'installed');
  assert.equal(installed, 1); assert.equal(f.prepared(), 0); assert.equal(f.calls.length, 0);
});

test('actual Test2 owning fallback keeps its serial policy, required marker and original selector', async () => {
  const f = fixture();
  vm.runInContext(actual('../Test2/orchestrator.mjs', 'async function startIosJoinTest(', 'async function prepareAndroidEmulator('), f.context);
  await assert.rejects(f.context.startIosJoinTest(udid), error => error === f.stop);
  assertOwningCommand(f, `id=${udid}`);
  for (const arg of iosSerialTestArguments(udid)) assert.ok(f.calls[0].args.includes(arg));
  assert.ok(f.calls[0].args.includes('-only-testing:iosAppUITests/CaseTwoIosInitiatorUITest/testCreateVaultThenApproveAndroidAndWeb'));
});

test('actual Test4 owning fallback keeps serial policy and local network configuration', async () => {
  const f = fixture(); f.context.networkLossConfig = { controlled: true };
  vm.runInContext(actual('../Test4/orchestrator.mjs', 'async function prepareIosTestBundle(', '// Existing actor config protocol'), f.context);
  await assert.rejects(f.context.prepareIosTestBundle(udid), error => error === f.stop);
  assertOwningCommand(f, `id=${udid}`);
  for (const arg of iosSerialTestArguments(udid)) assert.ok(f.calls[0].args.includes(arg));
  assert.ok(f.calls[0].args.includes('META_SECRET_SOCKET_URL=http://127.0.0.1:5201'));
  assert.ok(f.calls[0].args.includes('META_SECRET_ENV=local'));
  assert.equal(f.calls[0].options.env.GRADLE_USER_HOME, '/fixture/gradle');
});
