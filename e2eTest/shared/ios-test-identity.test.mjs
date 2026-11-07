import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, readFileSync, existsSync, rmSync, symlinkSync, chmodSync, mkdirSync, statSync, lstatSync } from 'node:fs';
import { realpathSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { resetE2eNativeAppState, resetE2eScenarioState } from './e2e-state-reset.mjs';
import { runEntry } from '../run-ui-e2e-range.mjs';
import { createDiagnosticRedactor } from './diagnostic-redaction.mjs';
import { buildDeletionPlan } from '../Test50/plan.mjs';

// Public destination: shared/ios-test-identity.test.mjs. SOURCE PREVIEW ONLY.
// Baseline selection imports only EXISTING production functions. New helper
// imports are inside later test bodies: absence cannot manufacture baseline RED.
const identity = () => import('./ios-test-identity.mjs');
const udid = 'DB22263E-A8A3-49C5-ADD1-BAE4089E2E30';
const runtime = 'com.apple.CoreSimulator.SimRuntime.iOS-26-5';
const bundleId = 'org.metasecret.vault';
const selected = [{ platform: 'iOS', simulatorName: 'iPhone 17', udid }];
const scenario = { ios: { simulatorName: 'iPhone 17', bundleId } };
const selector = 'iosAppUITests/CaseEightIosBothOfflineUITest/joinVaultForTest50';
const hashFile = path => createHash('sha256').update(readFileSync(path)).digest('hex');

function fixture(t) {
  const runRoot = realpathSync(mkdtempSync(resolve(tmpdir(), 'f2-ios-scope-control-')));
  t.after(() => rmSync(runRoot, { recursive: true, force: true }));
  const scenarioPath = resolve(runRoot, 'scenario.json');
  writeFileSync(scenarioPath, JSON.stringify(scenario));
  const context = { runRoot, runId: 'controlled-parent', testNumber: 1, attempt: 1,
    scenarioKey: 'file:' + scenarioPath, udid, bundleId };
  const inputs = { runRoot, runId: context.runId, testNumber: 1, attempt: 1,
    scenarioPath, scenario, udid };
  return { runRoot, scenarioPath, context, inputs };
}

function appPort(f) {
  const legacy = resolve(f.runRoot, 'synthetic-legacy-container-sentinel');
  writeFileSync(legacy, 'controlled prior app data');
  let installed = true;
  const calls = [];
  return { legacy, calls, runCommand: async (file, args) => {
    calls.push([file, ...args]);
    assert.equal(file, 'xcrun');
    if (args[1] === 'get_app_container') {
      if (!installed) throw Error('Application is not installed');
      return { stdout: f.runRoot };
    }
    if (args[1] === 'uninstall') {
      installed = false; rmSync(legacy); return { stdout: '' };
    }
    if (args[1] === 'terminate') return { stdout: '' };
    if (args[1] === 'list') return { stdout: JSON.stringify({ devices: { [runtime]: [{
      name: 'iPhone 17', udid, state: 'Booted', isAvailable: true,
      deviceTypeIdentifier: 'com.apple.CoreSimulator.SimDeviceType.iPhone-17',
    }] } }) };
    assert.fail('Unexpected SDK command in a port-only control');
  } };
}

test('namespace reset preserves legacy container and only terminates the existing iOS app', async t => {
  const f = fixture(t), port = appPort(f);
  const result = await resetE2eNativeAppState({ scenario, platforms: selected,
    iosIdentityContext: f.context, runCommand: port.runCommand });
  // On the unchanged old helper, real uninstallIos reaches the controlled
  // uninstall port, removes this sentinel, and THIS preservation assertion fails.
  assert.equal(existsSync(port.legacy), true, 'Upgrade/scope reset must preserve prior app-container data');
  assert.deepEqual(result.platformsReset, ['iOS']);
  assert.equal(port.calls.some(call => call.includes('uninstall')), false);
  assert.equal(port.calls.filter(call => call[2] === 'terminate').length, 1);
  assert.equal(port.calls.some(call => call.some(x => ['create', 'clone', 'delete', 'erase', 'keychain'].includes(x))), false);
});

test('actual scope functions retain raw-parent boundary across decorated actors and rotate only blocks or attempts', async t => {
  const f = fixture(t), api = await identity();
  const first = api.createIosIdentityContext(f.inputs);
  const saved = api.prepareIosIdentityBoundary(first);
  assert.equal(first.runId, 'controlled-parent');
  for (const decoration of ['controlled-parent-block-a', 'actor-restart', 'cycle-60']) {
    const next = api.createIosIdentityContext({ ...f.inputs, scenario: { ...scenario, runId: decoration } });
    assert.deepEqual(next, first);
    assert.ok(api.requireIosIdentityBoundary(next).namespace === saved.namespace);
    assert.ok(api.prepareIosIdentityBoundary(next).namespace === saved.namespace);
  }
  const blocks = buildDeletionPlan('controlled-parent').blocks;
  assert.equal(blocks.length, 8);
  const scopes = blocks.map(block => api.prepareIosIdentityBoundary(api.createIosIdentityContext({
    ...f.inputs, testNumber: 50, scenario: { ...scenario, runId: 'deliberately-decorated', deletionBlock: block },
  })));
  assert.equal(new Set(scopes.map(value => value.namespace)).size, 8);
  const retry = api.prepareIosIdentityBoundary(api.createIosIdentityContext({ ...f.inputs, attempt: 2 }));
  assert.ok(retry.namespace !== saved.namespace);
  assert.ok(api.requireIosIdentityBoundary(first).namespace === saved.namespace);
  assert.equal(existsSync(saved.recordPath), true);
  const environmentKeys = ['E2E_RANGE_RUN', 'E2E_RUN_ID', 'E2E_IOS_IDENTITY_ROOT', 'E2E_TEST_ATTEMPT'];
  const old = Object.fromEntries(environmentKeys.map(key => [key, process.env[key]]));
  try {
    Object.assign(process.env, { E2E_RANGE_RUN: '1', E2E_RUN_ID: f.context.runId,
      E2E_IOS_IDENTITY_ROOT: f.runRoot, E2E_TEST_ATTEMPT: '1' });
    const caller = () => api.prepareIosCallerIdentity({ scenario: { ...scenario, runId: 'decorated-child' },
      scenarioPath: f.scenarioPath, testNumber: 1, attemptToken: 'unused', artifactsDirectory: f.runRoot });
    assert.ok(caller().scope.namespace === saved.namespace);
    rmSync(saved.recordPath); assert.throws(caller, /record.*missing|invalid/i);
  } finally {
    for (const key of environmentKeys) if (old[key] === undefined) delete process.env[key]; else process.env[key] = old[key];
  }
});

test('actual scope storage refuses missing resumed, foreign, malformed and nonprivate records without replacement', async t => {
  const f = fixture(t), api = await identity(), context = api.createIosIdentityContext(f.inputs);
  assert.throws(() => api.prepareIosIdentityBoundary(context, { resume: true }));
  const good = api.prepareIosIdentityBoundary(context);
  const raw = readFileSync(good.recordPath);
  const record = JSON.parse(raw);
  for (const mutation of [
    value => { value.namespace = 'f2-NOT-CANONICAL'; },
    value => { value.boundary.udid = 'FOREIGN'; },
    value => { value.boundary.runId = 'foreign-owner'; },
    value => { value.version = 999; },
  ]) {
    const changed = structuredClone(record); mutation(changed);
    writeFileSync(good.recordPath, JSON.stringify(changed), { mode: 0o600 });
    assert.throws(() => api.requireIosIdentityBoundary(context));
    assert.throws(() => api.prepareIosIdentityBoundary(context));
    assert.deepEqual(JSON.parse(readFileSync(good.recordPath)), changed);
  }
  writeFileSync(good.recordPath, raw); chmodSync(good.recordPath, 0o644);
  assert.throws(() => api.requireIosIdentityBoundary(context));
  chmodSync(good.recordPath, 0o600);
  const other = resolve(f.runRoot, 'other-record'); writeFileSync(other, raw, { mode: 0o600 });
  rmSync(good.recordPath); symlinkSync(other, good.recordPath);
  assert.throws(() => api.requireIosIdentityBoundary(context));
  assert.throws(() => api.prepareIosIdentityBoundary(context));
  assert.deepEqual(readFileSync(other), raw);
});

test('actual reset refuses absent or foreign scope before server or app mutation', async t => {
  const f = fixture(t), calls = [];
  for (const iosIdentityContext of [undefined, { ...f.context, udid: 'FOREIGN' }, { ...f.context, runId: '' }]) {
    await assert.rejects(resetE2eScenarioState({
      scenario: { ...scenario, server: { container: 'controlled-only', image: 'controlled-only' } },
      platforms: selected, iosIdentityContext,
      runCommand: async (...args) => { calls.push(args); throw Error('No mutation before admission'); },
    }));
    assert.deepEqual(calls, []);
  }
});

function collector(f) {
  const logPath = resolve(f.runRoot, 'controlled.log'); writeFileSync(logPath, '');
  return { runDirectory: f.runRoot, redactor: createDiagnosticRedactor(), startTest: () => logPath,
    writeEvent: () => {}, writeOutput: () => {}, finishTest: async () => {} };
}

test('actual range caller refuses Test51 before ports and forwards undecorated context at the real reset seam', async t => {
  const f = fixture(t);
  for (const value of [{ testNumber: 51, mode: undefined }, { testNumber: 1, mode: 'registration-only' }]) {
    const calls = [], s = { ...scenario, ...(value.mode ? { mode: value.mode } : {}) };
    const result = await runEntry({ testNumber: value.testNumber, script: 'controlled-only',
      scenarios: [{ path: f.scenarioPath, value: s }] },
    { attempt: 1, collector: collector(f), runId: 'controlled-parent' },
    { environmentGuard: { prepare: async () => { calls.push('environment'); throw Error('No admission'); } } })
      .catch(error => ({ status: 'readiness-failed', error: error.message }));
    assert.equal(result.status, 'readiness-failed');
    assert.match(result.error, /namespace|identity|ineligible|Test51|registration/i);
    assert.deepEqual(calls, []);
  }
  let forwarded;
  const reportPath = resolve(f.runRoot, 'readiness.json');
  writeFileSync(reportPath, JSON.stringify({ runId: 'controlled-parent', platforms: selected }));
  const stopAtReset = Error('Controlled stop after inspecting actual reset arguments');
  const result = await runEntry({ testNumber: 1, script: 'controlled-only',
    scenarios: [{ path: f.scenarioPath, value: { ...scenario, runId: 'decorated-child-value',
      server: { url: 'http://127.0.0.1:3000', container: 'controlled-only', port: 3000 },
      web: { url: 'http://127.0.0.1:8080' } } }] },
  { attempt: 1, collector: collector(f), runId: 'controlled-parent' },
  { environmentGuard: { prepare: async () => {} },
    batchProducts: { hasPersistedProof: true, prepare: async () => '/controlled/products',
      nativePreparation: () => ({}) } },
  { prepareScenarioPlatforms: async () => ({ platforms: selected, reportPath }),
    resetE2eScenarioState: async options => { forwarded = options.iosIdentityContext; throw stopAtReset; },
    discoverAttemptArtifacts: () => [], spawn: () => assert.fail('No child after controlled reset stop') });
  assert.equal(result.status, 'readiness-failed');
  assert.equal(forwarded.runId, 'controlled-parent');
  assert.equal(forwarded.udid, udid);
  const api = await identity();
  assert.deepEqual(forwarded, api.createIosIdentityContext(f.inputs));
});

// Serialization is fixture support only. Production policy is invoked solely
// through prepareScopedIosInvocation; no copied namespace or auth resolver.
function templateFixture(f, environments = {}) {
  const derivedDataPath = mkdtempSync(resolve(f.runRoot, 'derived-')), products = resolve(derivedDataPath, 'Build', 'Products');
  mkdirSync(products, { recursive: true });
  const original = resolve(products, 'controlled.xctestrun');
  execFileSync('python3', ['-I', '-c',
    'import sys,json,plistlib; p=json.loads(sys.argv[2]); plistlib.dump(p,open(sys.argv[1],\"wb\"))',
    original, JSON.stringify({ TestConfigurations: [{ TestTargets: [{ BlueprintName: 'iosAppUITests', ...environments }] }] })]);
  const bytes = readFileSync(original);
  return { derivedDataPath, original, bytes };
}

test('controlled-business actual copier preserves true Core routing and dual-env inverse', async t => {
  const f = fixture(t), api = await identity(), context = { ...f.context, testNumber: 50 };
  const scope = api.prepareIosIdentityBoundary(context), original = templateFixture(f);
  const invoke = (extra = {}) => api.prepareScopedIosInvocation({ identityContext: context, selector,
    templateProof: { original: original.original, sha256: hashFile(original.original) },
    hostEnvironment: { E2E_ROLE: 'cancel' }, authPolicy: 'controlled-business',
    privateConfiguration: { requireCoreRouting: true, authProfile: 'controlled-business' },
    outputDirectory: f.runRoot, ambientEnvironment: {}, ...extra,
  });
  const result = invoke();
  assert.equal(statSync(result.xctestrun).mode & 0o777, 0o600);
  assert.equal(readFileSync(original.original).equals(original.bytes), true);
  const read = path => JSON.parse(execFileSync('python3', ['-I', '-c',
    'import sys,json,plistlib; print(json.dumps(plistlib.load(open(sys.argv[1],"rb"))))', path], { encoding: 'utf8' }));
  const copied = read(result.xctestrun), target = copied.TestConfigurations[0].TestTargets[0];
  for (const field of ['EnvironmentVariables', 'UITargetAppEnvironmentVariables']) {
    assert.equal(target[field].METASECRET_E2E_IDENTITY_NAMESPACE === scope.namespace, true);
    assert.equal(Object.hasOwn(target[field], 'METASECRET_UI_TEST_MODE'), false);
    delete target[field].METASECRET_E2E_IDENTITY_NAMESPACE;
  }
  assert.equal(target.EnvironmentVariables.E2E_ROLE, 'cancel');
  delete target.EnvironmentVariables.E2E_ROLE;
  delete target.EnvironmentVariables; delete target.UITargetAppEnvironmentVariables;
  assert.equal(JSON.stringify(copied) === JSON.stringify(read(original.original)), true);
  for (const key of ['METASECRET_UI_TEST_MODE', 'SIMCTL_CHILD_METASECRET_UI_TEST_MODE']) for (const value of ['', 'false']) {
    assert.throws(() => invoke({ ambientEnvironment: { [key]: value } }));
    const altered = templateFixture(f, { Auxiliary: { TestingEnvironmentVariables: { [key]: value } } });
    assert.throws(() => invoke({ templateProof: { original: altered.original, sha256: hashFile(altered.original) } }));
    assert.equal(readFileSync(altered.original).equals(altered.bytes), true);
  }
  for (const extra of [
    { privateConfiguration: { requireCoreRouting: false, authProfile: 'controlled-business' } },
    { privateConfiguration: { requireCoreRouting: true, authProfile: 'real-os' } },
    { identityContext: { ...context, udid: 'foreign' } },
    { selector: 'iosAppUITests/CaseOneJoinFromIosUITest/testJoinApprovedVaultAndShowSecret' },
  ]) assert.throws(() => invoke(extra));
});

test('actual dual-env copier preserves original and refuses every real-OS contamination including empty or false', async t => {
  const f = fixture(t), api = await identity();
  const context = api.createIosIdentityContext({ ...f.inputs, testNumber: 50,
    scenario: { ...scenario, mode: 'vault-secret-deletion' } });
  const scope = api.prepareIosIdentityBoundary(context);
  const good = templateFixture(f);
  const invoke = extra => api.prepareScopedIosInvocation({
    identityContext: context, templateProof: { original: good.original,
      sha256: hashFile(good.original) }, selector,
    hostEnvironment: { E2E_VAULT_NAME: 'synthetic-owned-fixture' }, authPolicy: 'real-os',
    outputDirectory: mkdtempSync(resolve(f.runRoot, 'copy-output-')), ambientEnvironment: {},
    privateConfiguration: { requireCoreRouting: true }, ...extra,
  });
  const result = invoke({});
  assert.equal(statSync(result.xctestrun).mode & 0o777, 0o600);
  assert.deepEqual(readFileSync(good.original), good.bytes);
  const copied = JSON.parse(execFileSync('python3', ['-I', '-c',
    'import sys,json,plistlib; print(json.dumps(plistlib.load(open(sys.argv[1],\"rb\"))))', result.xctestrun], { encoding: 'utf8' }));
  const target = copied.TestConfigurations[0].TestTargets[0];
  assert.ok(target.EnvironmentVariables.METASECRET_E2E_IDENTITY_NAMESPACE === scope.namespace);
  assert.ok(target.UITargetAppEnvironmentVariables.METASECRET_E2E_IDENTITY_NAMESPACE === scope.namespace);
  delete target.EnvironmentVariables.METASECRET_E2E_IDENTITY_NAMESPACE;
  delete target.EnvironmentVariables.E2E_VAULT_NAME;
  delete target.UITargetAppEnvironmentVariables.METASECRET_E2E_IDENTITY_NAMESPACE;
  delete target.EnvironmentVariables; delete target.UITargetAppEnvironmentVariables;
  const originalObject = JSON.parse(execFileSync('python3', ['-I', '-c',
    'import sys,json,plistlib; print(json.dumps(plistlib.load(open(sys.argv[1],\"rb\"))))', good.original], { encoding: 'utf8' }));
  assert.deepEqual(copied, originalObject);
  for (const key of ['METASECRET_UI_TEST_MODE', 'SIMCTL_CHILD_METASECRET_UI_TEST_MODE']) {
    for (const value of ['', 'false', 'true']) {
      assert.throws(() => invoke({ ambientEnvironment: { [key]: value } }));
      assert.throws(() => invoke({ hostEnvironment: { [key]: value } }));
      assert.throws(() => invoke({ privateConfiguration: { requireCoreRouting: true, [key]: value } }));
      for (const field of ['EnvironmentVariables', 'UITargetAppEnvironmentVariables', 'TestingEnvironmentVariables']) {
        const altered = templateFixture(f, { [field]: { [key]: value } });
        assert.throws(() => invoke({ templateProof: { original: altered.original,
          sha256: hashFile(altered.original) } }));
        assert.deepEqual(readFileSync(altered.original), altered.bytes);
      }
      const nested = templateFixture(f, { Auxiliary: { TestingEnvironmentVariables: { [key]: value } } });
      assert.throws(() => invoke({ templateProof: { original: nested.original, sha256: hashFile(nested.original) } }));
    }
  }
  for (const foreign of ['iosAppUITests/Foreign/testSomething', 'other/CaseOneJoinFromIosUITest/testJoinApprovedVaultAndShowSecret',
    'iosAppUITests/CaseEightIosBothOfflineUITest/not-a-method']) {
    assert.throws(() => invoke({ selector: foreign }));
  }
  for (const mismatch of [
    { selector: 'iosAppUITests/CaseOneJoinFromIosUITest/testJoinApprovedVaultAndShowSecret' },
    { authPolicy: 'controlled' }, { authPolicy: 'automatic' },
    { privateConfiguration: undefined }, { privateConfiguration: { requireCoreRouting: false } },
  ]) assert.throws(() => invoke(mismatch));
  for (const key of ['METASECRET_E2E_IDENTITY_NAMESPACE', 'LIVEBOARD_RENDERER_SECRET_ID']) {
    for (const field of ['EnvironmentVariables', 'UITargetAppEnvironmentVariables']) {
      const altered = templateFixture(f, { [field]: { [key]: 'foreign' } });
      assert.throws(() => invoke({ templateProof: { original: altered.original,
        sha256: hashFile(altered.original) } }));
    }
  }
});

test('actual signed-marker admission rejects absent NO malformed and signature failure before install', async () => {
  const api = await identity(), calls = [];
  for (const marker of [undefined, 'NO', '', 'true', true]) {
    assert.throws(() => api.requireIosIdentityBuild('/controlled/MetaSecret.app', {
      readMarker: () => marker, verifySignature: () => { calls.push('signed'); },
    }));
  }
  assert.throws(() => api.requireIosIdentityBuild('/controlled/MetaSecret.app', {
    readMarker: () => 'YES', verifySignature: () => { throw Error('Controlled signature refusal'); },
  }));
  assert.equal(api.requireIosIdentityBuild('/controlled/MetaSecret.app', {
    readMarker: () => 'YES', verifySignature: () => { calls.push('signed'); },
  }), 'YES');
  assert.equal(calls.length, 1);
});

test('actual diagnostic redactor registers scope before streams and hides bare split token env and argv', () => {
  const value = 'f2-11111111-1111-4111-8111-111111111111';
  const redactor = createDiagnosticRedactor();
  redactor.addSensitiveValues([value]);
  const stream = redactor.createStream();
  const output = stream.push('domain=MetaSecret.E2E.' + value.slice(0, 18))
    + stream.push(value.slice(18) + '\n') + stream.flush();
  assert.equal(output.includes(value), false);
  assert.equal(redactor.redactText(value).includes(value), false);
  assert.equal(redactor.redactEnvironment({ METASECRET_E2E_IDENTITY_NAMESPACE: value })
    .METASECRET_E2E_IDENTITY_NAMESPACE.includes(value), false);
  assert.equal(redactor.redactArguments(['--METASECRET_E2E_IDENTITY_NAMESPACE', value])[1].includes(value), false);
  const unseen = 'f2-22222222-2222-4222-8222-222222222222';
  const keyOnly = createDiagnosticRedactor();
  assert.equal(keyOnly.redactEnvironment({ METASECRET_E2E_IDENTITY_NAMESPACE: unseen })
    .METASECRET_E2E_IDENTITY_NAMESPACE, '[REDACTED]');
  assert.equal(keyOnly.redactArguments(['--METASECRET_E2E_IDENTITY_NAMESPACE', unseen])[1], '[REDACTED]');
  assert.equal(keyOnly.redactText('METASECRET_E2E_IDENTITY_NAMESPACE=' + unseen).includes(unseen), false);
});

test('actual relocated copier resolves owning products and loader paths while preserving other macros and exact inverse', async t => {
  const f = fixture(t), api = await identity();
  const scope = api.prepareIosIdentityBoundary(f.context), original = templateFixture(f);
  const products = resolve(original.derivedDataPath, 'Build/Products');
  const app = 'Debug-iphonesimulator/MetaSecret.app', runner = 'Debug-iphonesimulator/iosAppUITests-Runner.app';
  const bundle = runner + '/PlugIns/iosAppUITests.xctest';
  for (const path of [app, runner, bundle]) mkdirSync(resolve(products, path), { recursive: true });
  const target = { BlueprintName: 'iosAppUITests', TestHostPath: '__TESTROOT__/' + runner,
    TestBundlePath: '__TESTHOST__/PlugIns/iosAppUITests.xctest', UITargetAppPath: '__TESTROOT__/' + app,
    DependentProductPaths: [app, runner, bundle].map(path => '__TESTROOT__/' + path),
    TestingEnvironmentVariables: {
      DYLD_FRAMEWORK_PATH: '__TESTROOT__/Debug-iphonesimulator:__PLATFORMS__/iPhoneSimulator.platform/Developer/Library/Frameworks',
      DYLD_LIBRARY_PATH: '__TESTROOT__/Debug-iphonesimulator:__PLATFORMS__/iPhoneSimulator.platform/Developer/usr/lib',
      __XCODE_BUILT_PRODUCTS_DIR_PATHS: '__TESTROOT__/Debug-iphonesimulator',
      LITERAL_ABSOLUTE: products, LITERAL_TEXT: 'unchanged', __TESTROOT__KEY: '__PLATFORMS__/unchanged',
    },
    UITargetAppEnvironmentVariables: { DYLD_LIBRARY_PATH: '__TESTROOT__/Debug-iphonesimulator' },
  };
  const object = { TestConfigurations: [{ TestTargets: [target] }], Unrelated: { Literal: products, Flag: true, Count: 2 } };
  execFileSync('python3', ['-I', '-c',
    'import sys,json,plistlib; plistlib.dump(json.loads(sys.argv[2]),open(sys.argv[1],"wb"))', original.original, JSON.stringify(object)]);
  const bytes = readFileSync(original.original), originalHash = hashFile(original.original);
  const result = api.prepareScopedIosInvocation({ identityContext: f.context,
    templateProof: { original: original.original, sha256: originalHash },
    selector: 'iosAppUITests/CaseOneJoinFromIosUITest/testJoinApprovedVaultAndShowSecret',
    hostEnvironment: { E2E_CONTROLLED_VALUE: '__TESTROOT__/injected-literal' }, authPolicy: 'controlled',
    outputDirectory: resolve(f.runRoot, 'separate-invocations'), ambientEnvironment: {},
  });
  const copied = JSON.parse(execFileSync('python3', ['-I', '-c',
    'import sys,json,plistlib; print(json.dumps(plistlib.load(open(sys.argv[1],"rb"))))', result.xctestrun], { encoding: 'utf8' }));
  const actual = copied.TestConfigurations[0].TestTargets[0];
  const copyRoot = resolve(result.xctestrun, '..');
  const host = actual.TestHostPath.replaceAll('__TESTROOT__', copyRoot);
  assert.equal(existsSync(host), true, 'Relocated invocation must resolve the genuine owning runner product');
  assert.equal(host, resolve(products, runner));
  assert.equal(actual.UITargetAppPath, resolve(products, app)); assert.ok(existsSync(actual.UITargetAppPath));
  assert.equal(actual.TestBundlePath, target.TestBundlePath);
  assert.ok(existsSync(actual.TestBundlePath.replaceAll('__TESTHOST__', host)));
  assert.deepEqual(actual.DependentProductPaths, [app, runner, bundle].map(path => resolve(products, path)));
  for (const path of actual.DependentProductPaths) assert.ok(existsSync(path));
  for (const key of ['DYLD_FRAMEWORK_PATH', 'DYLD_LIBRARY_PATH', '__XCODE_BUILT_PRODUCTS_DIR_PATHS']) {
    const expected = target.TestingEnvironmentVariables[key].replaceAll('__TESTROOT__', products);
    assert.equal(actual.TestingEnvironmentVariables[key], expected);
    assert.ok(existsSync(actual.TestingEnvironmentVariables[key].split(':')[0]));
  }
  assert.equal(actual.UITargetAppEnvironmentVariables.DYLD_LIBRARY_PATH, resolve(products, 'Debug-iphonesimulator'));
  for (const key of ['LITERAL_ABSOLUTE', 'LITERAL_TEXT', '__TESTROOT__KEY'])
    assert.equal(actual.TestingEnvironmentVariables[key], target.TestingEnvironmentVariables[key]);
  assert.deepEqual(copied.Unrelated, object.Unrelated);
  assert.equal(actual.EnvironmentVariables.E2E_CONTROLLED_VALUE, '__TESTROOT__/injected-literal');
  for (const field of ['EnvironmentVariables', 'UITargetAppEnvironmentVariables'])
    assert.ok(actual[field].METASECRET_E2E_IDENTITY_NAMESPACE === scope.namespace);
  assert.equal(result.proof.semanticInverseExact, true); assert.equal(result.proof.originalSha256, originalHash);
  assert.equal(result.proof.copySha256, hashFile(result.xctestrun));
  assert.deepEqual(readFileSync(original.original), bytes); assert.equal(statSync(result.xctestrun).mode & 0o777, 0o600);
});

async function declaredBlockFixture(t) {
  const f = fixture(t), api = await identity(), calls = [], registered = [], resets = [];
  const rows = ['scenario.json', 'scenario-block1.json', 'scenario-block2.json', 'scenario-block3.json'].map((name, index) => {
    const value = { ...scenario, runId: `decorated-child-${index}`,
      server: { url: 'http://127.0.0.1:3000', container: 'controlled-only', port: 3000 }, web: { url: 'http://127.0.0.1:8080' } };
    const path = resolve(f.runRoot, name); writeFileSync(path, JSON.stringify(value)); return { path, value };
  });
  const expected = (row, attempt = 1) => api.createIosIdentityContext({ ...f.inputs, testNumber: 4, attempt,
    scenarioPath: row.path, scenario: row.value });
  const reportPath = resolve(f.runRoot, 'block-readiness.json');
  const run = async (attempt = 1) => {
    writeFileSync(reportPath, JSON.stringify({ runId: 'controlled-parent', platforms: selected }));
    const output = collector(f), register = output.redactor.addSensitiveValues;
    output.redactor.addSensitiveValues = values => { registered.push(...values); register(values); };
    return runEntry({ testNumber: 4, script: 'controlled-blocks', scenarios: rows },
      { attempt, collector: output, runId: 'controlled-parent' },
      { environmentGuard: { prepare: async () => { calls.push('environment'); } },
        batchProducts: { hasPersistedProof: true, prepare: async () => { calls.push('products'); return '/controlled/products'; },
          nativePreparation: () => ({}) } },
      { prepareScenarioPlatforms: async options => {
          calls.push('platforms'); assert.equal(options.scenario, rows[0].value); return { platforms: selected, reportPath };
        }, resetE2eScenarioState: async options => {
          calls.push('reset'); resets.push(options.iosIdentityContext); throw Error('controlled parent reset stop');
        }, discoverAttemptArtifacts: () => [], spawn: () => { calls.push('spawn'); assert.fail('No SDK/child in identity control'); } });
  };
  const caller = (row, attempt = 1, extraEnvironment = {}) => {
    const values = { E2E_RANGE_RUN: '1', E2E_RUN_ID: 'controlled-parent', E2E_IOS_IDENTITY_ROOT: f.runRoot,
      E2E_TEST_ATTEMPT: String(attempt), ...extraEnvironment };
    const prior = Object.fromEntries(Object.keys(values).map(key => [key, process.env[key]]));
    try {
      Object.assign(process.env, values);
      return api.prepareIosCallerIdentity({ scenario: row.value, scenarioPath: row.path, testNumber: 4,
        attemptToken: 'unused', artifactsDirectory: f.runRoot });
    } finally {
      for (const key of Object.keys(values)) if (prior[key] === undefined) delete process.env[key]; else process.env[key] = prior[key];
    }
  };
  return { ...f, api, rows, calls, registered, resets, expected, run, caller };
}

test('actual parent prepares aggregate and every exact declared iOS block before strict child admission', async t => {
  const f = await declaredBlockFixture(t), result = await f.run();
  assert.equal(result.status, 'readiness-failed'); assert.match(result.error, /controlled parent reset stop/);
  assert.deepEqual(f.resets[0], f.expected(f.rows[0]));
  let admitted;
  assert.doesNotThrow(() => { admitted = f.rows.slice(1).map(row => f.caller(row)); },
    'Owning parent must prepare each exact child block identity');
  const scopes = [f.api.requireIosIdentityBoundary(f.expected(f.rows[0])), ...admitted.map(value => value.scope)];
  assert.equal(new Set(scopes.map(value => value.recordPath)).size, 4);
  assert.equal(new Set(scopes.map(value => value.namespace)).size, 4);
  for (const [index, scope] of scopes.entries()) {
    assert.equal(statSync(scope.recordPath).mode & 0o777, 0o600);
    assert.equal(f.registered.includes(scope.namespace), true);
    assert.equal(JSON.parse(readFileSync(scope.recordPath)).boundary.scenarioKey, 'file:' + f.rows[index].path);
    assert.equal(JSON.parse(readFileSync(scope.recordPath)).boundary.runId, 'controlled-parent');
  }
  assert.deepEqual(f.calls, ['environment', 'products', 'platforms', 'reset']);
  const nonIosFirst = await declaredBlockFixture(t);
  delete nonIosFirst.rows[0].value.ios;
  writeFileSync(nonIosFirst.rows[0].path, JSON.stringify(nonIosFirst.rows[0].value));
  await nonIosFirst.run();
  assert.equal(nonIosFirst.resets[0], undefined, 'Later iOS blocks must not replace the literal first reset context');
  for (const row of nonIosFirst.rows.slice(1)) assert.doesNotThrow(() => nonIosFirst.caller(row));
});

test('actual parent block records stay stable per attempt and strict children never adopt another boundary', async t => {
  const f = await declaredBlockFixture(t); await f.run();
  const original = f.rows.map(row => f.api.requireIosIdentityBoundary(f.expected(row)));
  await f.run();
  for (const [index, row] of f.rows.entries()) {
    const current = f.api.requireIosIdentityBoundary(f.expected(row));
    assert.equal(current.recordSha256, original[index].recordSha256);
    assert.equal(current.namespace === original[index].namespace, true);
  }
  await f.run(2);
  for (const [index, row] of f.rows.entries()) assert.equal(f.caller(row, 2).scope.namespace === original[index].namespace, false);
  assert.deepEqual(f.resets[2], f.expected(f.rows[0], 2));
  for (const [row, attempt, env] of [[f.rows[1], 3, {}], [f.rows[1], 1, { E2E_RUN_ID: 'foreign-parent' }]]) {
    assert.throws(() => f.caller(row, attempt, env), /missing|foreign|invalid/i);
  }
  const undeclaredPath = resolve(f.runRoot, 'undeclared-scenario.json'); writeFileSync(undeclaredPath, JSON.stringify(scenario));
  assert.throws(() => f.caller({ path: undeclaredPath, value: scenario }), /missing|foreign|invalid/i);
  assert.throws(() => f.caller({ ...f.rows[1], value: { ...scenario, ios: { ...scenario.ios, bundleId: 'foreign.bundle' } } }), /target mismatch/i);
  assert.throws(() => f.api.requireIosIdentityBoundary({ ...f.expected(f.rows[1]), udid: 'foreign-device' }), /Invalid/i);
  rmSync(original[1].recordPath);
  assert.throws(() => f.caller(f.rows[1]), /missing|foreign|invalid/i);
  assert.equal(existsSync(original[1].recordPath), false, 'Strict child must not recreate a missing exact block record');
});

test('actual parent refuses invalid existing block records before environment build reset or spawn without replacement', async t => {
  for (const kind of ['foreign', 'malformed', 'nonprivate', 'symlink']) {
    const f = await declaredBlockFixture(t), saved = f.api.prepareIosIdentityBoundary(f.expected(f.rows[1]));
    if (kind === 'foreign') {
      const value = JSON.parse(readFileSync(saved.recordPath)); value.boundary.runId = 'foreign-owner'; writeFileSync(saved.recordPath, JSON.stringify(value));
    } else if (kind === 'malformed') writeFileSync(saved.recordPath, '{invalid');
    else if (kind === 'nonprivate') chmodSync(saved.recordPath, 0o644);
    else {
      const target = resolve(f.runRoot, 'controlled-symlink-target'); writeFileSync(target, readFileSync(saved.recordPath), { mode: 0o600 });
      rmSync(saved.recordPath); symlinkSync(target, saved.recordPath);
    }
    const bytes = readFileSync(saved.recordPath), stat = lstatSync(saved.recordPath);
    await assert.rejects(f.run(), /missing|foreign|invalid/i);
    assert.deepEqual(f.calls, []);
    assert.deepEqual(readFileSync(saved.recordPath), bytes);
    assert.equal(lstatSync(saved.recordPath).mode, stat.mode);
    assert.equal(lstatSync(saved.recordPath).isSymbolicLink(), stat.isSymbolicLink());
  }
});
