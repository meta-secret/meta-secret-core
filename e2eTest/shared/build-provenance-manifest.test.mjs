import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdtempSync, readFileSync, rmSync, existsSync, mkdirSync, writeFileSync, chmodSync, symlinkSync, readdirSync, lstatSync, realpathSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as crypto from 'node:crypto';
import { createHash } from 'node:crypto';
import { runInNewContext } from 'node:vm';
import * as handoff from './batch-build-handoff.mjs';
import * as proof from './batch-product-proof.mjs';
import * as environment from './batch-environment.mjs';
import * as ios from './ios-ffi-provenance.mjs';
import * as resources from './e2e-resource-budget.mjs';
import { IOS_E2E_CONFIGURATION, requireIosIdentityBuild } from './ios-test-identity.mjs';
import { hashArtifactTree } from './artifact-tree.mjs';
import { captureRunnerIdentity, productRunnerIdentity, runnerAuditCommandSpecs, validateRunnerAudit, verifyRunnerAuditLineage,
  RUNNER_RECIPE_PATHS, RUNNER_NONCOMPILED_PATHS } from './batch-replay-identity.mjs';
import { provenanceManifestPath, recordArtifactProvenance, verifyArtifactProvenance } from './build-provenance-manifest.mjs';
import { createProductProofPorts } from './batch-product-proof.mjs';
import { createRunnerAudit } from './runner-validation-audit.mjs';
import { assertAcceptedRunnerLineage } from '../run-ui-e2e-range.mjs';

import {createEnvironmentTestRun} from './environment-test-fixture.mjs';

const native = { version: 3, sourceFingerprint: 'native-source', artifacts: {
  native: { archive: 'native-bytes' }, androidFfiSourceFingerprint: 'ffi-source', androidJniLibs: 'jni-bytes',
  wasm: 'wasm-bytes', iosBuild: 'ios-bytes', androidAppApk: 'app-bytes', androidTestApk: 'test-bytes', serverImageId: 'server-image',
} };
const web = { version: 2, scope: ['web'], sourceFingerprint: 'web-source',
  artifacts: { wasm: 'wasm-bytes', serverImageId: 'server-image' } };
const withStore = (run) => {
  const directory = mkdtempSync(join(tmpdir(), 'e2e-provenance-scope-'));
  try { run(directory); } finally { rmSync(directory, { recursive: true, force: true }); }
};

test('actual native → Web → native manifest recording preserves both exact proof records', () => withStore((directory) => {
  const nativePath = provenanceManifestPath(directory, 'native');
  const webPath = provenanceManifestPath(directory, 'web');
  verifyArtifactProvenance({ path: nativePath, current: native, bootstrap: true, evidence: '/approved/native-build-receipt.json' });
  const beforeWeb = readFileSync(nativePath);
  verifyArtifactProvenance({ path: webPath, current: web, bootstrap: true, evidence: '/approved/web-build-receipt.json' });
  assert.notEqual(nativePath, webPath);
  assert.deepEqual(readFileSync(nativePath), beforeWeb, 'Web proof cannot overwrite the earlier native proof');
  const verified = verifyArtifactProvenance({ path: nativePath, current: native });
  assert.equal(verified.version, 3);
  assert.equal(verified.sourceFingerprint, native.sourceFingerprint);
  assert.deepEqual(verified.artifacts, native.artifacts);
  assert.equal(verifyArtifactProvenance({ path: webPath, current: web }).version, 2);
}));

test('source, version and every recorded artifact still reject drift after crossing scopes', () => withStore((directory) => {
  const path = provenanceManifestPath(directory, 'native');
  recordArtifactProvenance(path, native, 'full local build completed');
  for (const key of ['version', 'sourceFingerprint', ...Object.keys(native.artifacts)]) {
    const changed = structuredClone(native);
    if (key === 'version') changed.version = 2;
    else if (key === 'sourceFingerprint') changed.sourceFingerprint = 'changed-source';
    else changed.artifacts[key] = 'changed-artifact';
    assert.throws(() => verifyArtifactProvenance({ path, current: changed }), /Build artifact provenance mismatch/, key);
  }
  assert.throws(() => verifyArtifactProvenance({ path, current: web }), /Build artifact provenance mismatch/);
}));

test('missing proof and bootstrap without the parent evidence still fail closed', () => withStore((directory) => {
  const path = provenanceManifestPath(directory, 'native');
  assert.throws(() => verifyArtifactProvenance({ path, current: native }), /manifest not found/);
  assert.throws(() => verifyArtifactProvenance({ path, current: native, bootstrap: true }), /E2E_BUILD_PROVENANCE_EVIDENCE is required/);
  assert.equal(existsSync(path), false);
}));

test('mismatch diagnostics identify scope/version/source and changed artifact keys without dumping values', () => withStore((directory) => {
  const path = provenanceManifestPath(directory, 'native');
  recordArtifactProvenance(path, native, 'approved build');
  const changed = structuredClone(native);
  changed.artifacts.iosBuild = 'new-ios-build-hash';
  assert.throws(() => verifyArtifactProvenance({ path, current: changed, scope: 'native' }), (error) => {
    assert.match(error.message, /scope=native; version=3->3/);
    assert.match(error.message, /expected source=native-source current source=native-source/);
    assert.match(error.message, /artifact keys=iosBuild$/);
    assert.doesNotMatch(error.message, /new-ios-build-hash|ios-bytes|native-bytes/);
    return true;
  });
}));

test('legacy full runs keep their existing manifest path and scope cannot select arbitrary filenames', () => withStore((directory) => {
  assert.equal(provenanceManifestPath(directory, 'full'), join(directory, 'ui-e2e-build-provenance.json'));
  assert.throws(() => provenanceManifestPath(directory, '../other'), /Unknown build provenance scope/);
  const fullPath = provenanceManifestPath(directory, 'full');
  recordArtifactProvenance(fullPath, native, 'legacy full build');
  assert.deepEqual(verifyArtifactProvenance({ path: fullPath, current: native }).artifacts, native.artifacts);
}));

function runnerFixture(t) {
  const directory = realpathSync(mkdtempSync(join(tmpdir(), 'e2e-runner-identity-')));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const paths = { coreRoot: join(directory, 'core'), runDirectory: join(directory, 'run') };
  const names = [...new Set([...RUNNER_RECIPE_PATHS, ...RUNNER_NONCOMPILED_PATHS,
    'Test1/scenario.json', 'shared/unknown-input.mjs', 'unknown-build-input.md',
    'shared/web-liveboard-actions.mjs', 'shared/ios-consumer-provenance.test.mjs'])].map(name => 'e2eTest/' + name);
  const put = (name, bytes) => { const path = join(paths.coreRoot, name); mkdirSync(dirname(path), { recursive: true }); writeFileSync(path, bytes); return path; };
  for (const name of names) put(name, name.endsWith('.json') ? '{"original":true}\n' : 'controlled original bytes\n');
  const ports = { listFiles: () => [...names] }, capture = () => captureRunnerIdentity(paths, ports);
  const digest = bytes => createHash('sha256').update(bytes).digest('hex');
  let sequence = 0;
  const audit = (recordChange = () => {}) => {
    const base = join(paths.runDirectory, 'runner-audits', String(++sequence)); mkdirSync(base, { recursive: true });
    const output = label => { const path = join(base, label); writeFileSync(path, 'controlled closed-port output\n', { mode: 0o600 }); return { path, sha256: digest(readFileSync(path)) }; };
    const identity = capture();
    // These are controlled schema fixtures, never evidence that F1 commands actually ran.
    const record = { version: 1, runId: 'controlled-run', runnerPid: process.pid, before: identity, after: identity,
      commands: runnerAuditCommandSpecs(paths).map((spec, index) => ({ ...spec, naturalExitCode: 0, signal: null,
        stdout: output(index + '.stdout'), stderr: output(index + '.stderr') })) };
    recordChange(record); const path = join(base, 'audit.json'); writeFileSync(path, JSON.stringify(record), { mode: 0o600 });
    return { path, sha256: digest(readFileSync(path)) };
  };
  return { paths, names, ports, put, capture, audit, digest,
    validate: (reference, extra = {}) => validateRunnerAudit({ paths, runId: 'controlled-run', audit: reference, ...extra }, ports) };
}

test('V5 product identity permits only listed noncompiled content changes and always invalidates the old current-runner audit', t => {
  const f = runnerFixture(t), original = f.capture(), product = productRunnerIdentity(original), reference = f.audit();
  for (const name of ['README.md', 'Test4/orchestrator.mjs', 'shared/platform-supervisor.mjs']) {
    const path = 'e2eTest/' + name, bytes = readFileSync(join(f.paths.coreRoot, path));
    f.put(path, 'controlled repaired noncompiled content\n');
    assert.notEqual(f.capture().digest, original.digest, 'Fresh F1 must see current executable harness content');
    assert.deepEqual(productRunnerIdentity(f.capture()), product, 'Product binary identity separates only the exact allowlisted content');
    assert.throws(() => f.validate(reference), /current identity mismatch/);
    assert.deepEqual(f.validate(f.audit()), f.capture(), 'A new exact current-runner audit remains mandatory');
    f.put(path, bytes);
  }
});

test('V5 recipes, unknown inputs, scenarios and all inventory membership remain byte strict', t => {
  const f = runnerFixture(t), original = productRunnerIdentity(f.capture());
  for (const relative of [...RUNNER_RECIPE_PATHS, 'shared/unknown-input.mjs', 'unknown-build-input.md', 'Test1/scenario.json',
    'shared/web-liveboard-actions.mjs', 'shared/ios-consumer-provenance.test.mjs']) {
    const name = 'e2eTest/' + relative, bytes = readFileSync(join(f.paths.coreRoot, name));
    f.put(name, 'changed controlled input\n');
    assert.notDeepEqual(productRunnerIdentity(f.capture()), original, relative);
    f.put(name, bytes);
  }
  for (const name of ['e2eTest/README.md', 'e2eTest/shared/unknown-input.mjs']) {
    const index = f.names.indexOf(name); f.names.splice(index, 1);
    assert.notDeepEqual(productRunnerIdentity(f.capture()), original, 'Removing even a listed noncompiled file changes membership');
    f.names.splice(index, 0, name);
  }
  f.names.push('e2eTest/new-scenario.json'); f.put('e2eTest/new-scenario.json', '{}');
  assert.notDeepEqual(productRunnerIdentity(f.capture()), original, 'A new JSON scenario cannot hide in a noncompiled scope');
});

test('runner identity refuses missing required recipe, noncanonical paths and symlink inputs', t => {
  const f = runnerFixture(t), required = 'e2eTest/' + RUNNER_RECIPE_PATHS[0];
  f.names.splice(f.names.indexOf(required), 1); assert.throws(f.capture, /Required runner recipe/); f.names.push(required);
  for (const name of ['../outside', 'e2eTest/../outside']) {
    f.put(name, 'controlled outside'); f.names.push(name); assert.throws(f.capture, /canonical/); f.names.pop();
  }
  const link = 'e2eTest/shared/symlink-input.mjs'; symlinkSync(join(f.paths.coreRoot, required), join(f.paths.coreRoot, link));
  f.names.push(link); assert.throws(f.capture, /noncanonical/);
});

test('current-runner audit binds run, process, exact before-after identity and every fixed successful F1 command', t => {
  const f = runnerFixture(t); assert.deepEqual(f.validate(f.audit()), f.capture());
  for (const change of [record => { record.runId = 'foreign-run'; }, record => { record.runnerPid++; },
    record => { record.before.digest = 'changed'; }, record => { record.after.strict = {}; },
    record => { record.commands.pop(); }, record => { record.commands.reverse(); },
    record => { record.commands[0].naturalExitCode = 1; }, record => { record.commands[0].signal = 'SIGTERM'; },
    record => { record.commands[0].args.push('--unreviewed'); }, record => { record.commands[0].file = '/foreign/node'; }]) {
    assert.throws(() => f.validate(f.audit(change)), /origin|identity|command inventory|command receipts/);
  }
  assert.throws(() => f.validate(f.audit(), { expectedRunnerPid: process.pid + 1 }), /identity mismatch/);
  assert.throws(() => f.validate(undefined), /audit.*required/i);
});

test('current-runner audit refuses altered, public, linked or external evidence without executing any command', t => {
  const f = runnerFixture(t), reference = f.audit();
  const record = JSON.parse(readFileSync(reference.path));
  writeFileSync(record.commands[0].stdout.path, 'changed controlled stdout');
  assert.throws(() => f.validate(reference), /output evidence changed/);
  const changed = f.audit(); writeFileSync(changed.path, '{}'); assert.throws(() => f.validate(changed), /immutable bytes changed/);
  const publicAudit = f.audit(); chmodSync(publicAudit.path, 0o644); assert.throws(() => f.validate(publicAudit), /Private current runner audit/);
  const outside = join(f.paths.coreRoot, 'outside-audit.json'); writeFileSync(outside, '{}', { mode: 0o600 });
  assert.throws(() => f.validate({ path: outside, sha256: f.digest(readFileSync(outside)) }), /Private current runner audit/);
  const linked = f.audit(), link = join(dirname(linked.path), 'linked.json'); symlinkSync(linked.path, link);
  assert.throws(() => f.validate({ ...linked, path: link }), /Private current runner audit/);
});

test('actual accepted-PASS lineage binds its executed helper and own orchestrator while preserving permitted binary reuse', t => {
  const f = runnerFixture(t), manifest = { tests: [{ test: 1, status: 'passed', runnerAudit: f.audit() }] };
  const before = JSON.stringify(manifest), check = () => assertAcceptedRunnerLineage({ paths: f.paths,
    runId: 'controlled-run', manifest, currentIdentity: f.capture() });
  check();
  for (const name of ['README.md', 'shared/platform-supervisor.test.mjs', 'Test2/orchestrator.mjs']) {
    const relative = 'e2eTest/' + name, bytes = readFileSync(join(f.paths.coreRoot, relative));
    f.put(relative, 'permitted current-runner repair'); check(); f.put(relative, bytes);
  }
  for (const name of ['shared/platform-supervisor.mjs', 'Test1/orchestrator.mjs', 'shared/web-liveboard-actions.mjs',
    'run-ui-e2e-range.mjs', 'Test1/scenario.json', 'shared/unknown-input.mjs']) {
    const relative = 'e2eTest/' + name, bytes = readFileSync(join(f.paths.coreRoot, relative));
    f.put(relative, 'changed accepted execution input'); assert.throws(check, /Accepted Test #1 execution inputs changed/); f.put(relative, bytes);
  }
  const index = f.names.indexOf('e2eTest/README.md'); f.names.splice(index, 1);
  assert.throws(check, /execution inputs changed/); f.names.splice(index, 0, 'e2eTest/README.md');
  assert.throws(() => assertAcceptedRunnerLineage({ paths: f.paths, runId: 'controlled-run', currentIdentity: f.capture(),
    manifest: { tests: [{ test: 1, status: 'passed' }] } }), /audit lineage required/);
  const delegated = { tests: [{ test: 5, status: 'passed', runnerAudit: f.audit() }] };
  const shared = 'e2eTest/Test4/orchestrator.mjs', sharedBytes = readFileSync(join(f.paths.coreRoot, shared));
  f.put(shared, 'changed common Test4 execution dependency');
  assert.throws(() => assertAcceptedRunnerLineage({ paths: f.paths, runId: 'controlled-run', manifest: delegated,
    currentIdentity: f.capture() }), /Accepted Test #5 execution inputs changed/);
  f.put(shared, sharedBytes);
  assert.equal(JSON.stringify(manifest), before, 'A denial never rewrites or recertifies historical PASS evidence');
});

test('actual V5 replay capture preserves every compiled input and compiler environment while legacy V4 keeps its harness hash', async t => {
  const f = runnerFixture(t); f.paths.composeRoot = join(f.paths.coreRoot, '..', 'compose');
  const coreInputs = ['Taskfile.yml', 'docker-bake.hcl', 'meta-secret/src/current.rs'];
  const composeInputs = ['gradle.properties', 'composeApp/src/commonMain/Current.kt', 'composeApp/src/androidTest/Hook.kt', 'iosApp/Config.xcconfig'];
  for (const name of coreInputs) f.put(name, 'controlled compiled original');
  for (const name of composeInputs) { const path = join(f.paths.composeRoot, name); mkdirSync(dirname(path), { recursive: true }); writeFileSync(path, 'controlled compiled original'); }
  const ports = createProductProofPorts({ ...f.paths, runAndCapture: async (file, args, options) => {
    if (file !== 'git') { assert(['cargo', 'rustc', 'java'].includes(file)); return { stdout: 'controlled toolchain 1\n' }; }
    assert.deepEqual(args.slice(0, 5), ['ls-files', '-co', '--exclude-standard', '-z', '--']);
    const names = options.cwd === f.paths.composeRoot ? composeInputs : args[5] === 'e2eTest' ? f.names : coreInputs;
    return { stdout: names.join('\0') + '\0' };
  } });
  const tuple = { product: 'android' }, baseline = await ports.captureProductReplayInputs(tuple), legacy = await ports.captureReplayInputs(tuple);
  assert.equal(Object.hasOwn(baseline, 'harness'), false); assert.equal(typeof baseline.compilerEnvironmentSha256, 'string');
  f.put('e2eTest/README.md', 'controlled documentation change');
  assert.deepEqual(await ports.captureProductReplayInputs(tuple), baseline);
  assert.notEqual((await ports.captureReplayInputs(tuple)).harness, legacy.harness, 'Legacy V4 remains strict; no implicit upgrade');
  for (const [root, names] of [[f.paths.coreRoot, coreInputs], [f.paths.composeRoot, composeInputs]]) for (const name of names) {
    const path = join(root, name), original = readFileSync(path); writeFileSync(path, 'changed compiled input');
    assert.notDeepEqual(await ports.captureProductReplayInputs(tuple), baseline, name); writeFileSync(path, original);
  }
  const key = 'CARGO_BUILD_JOBS', previous = process.env[key];
  try { process.env[key] = previous === '2' ? '1' : '2';
    assert.notEqual((await ports.captureProductReplayInputs(tuple)).compilerEnvironmentSha256, baseline.compilerEnvironmentSha256);
  } finally { if (previous === undefined) delete process.env[key]; else process.env[key] = previous; }
});

test('actual audit executor runs the exact fixed command inventory and stores private redacted hash-bound evidence', async t => {
  const f = runnerFixture(t), calls = [], specs = runnerAuditCommandSpecs(f.paths);
  const audit = await createRunnerAudit({ paths: f.paths, runId: 'controlled-run', identityPorts: f.ports,
    redactText: text => text.replaceAll('PRIVATE_CANARY', '[redacted]'),
    runAndCapture: async (file, args, options) => { calls.push({ file, args, cwd: options.cwd });
      assert.equal(options.timeoutMs, 120000); assert.equal(options.maxCaptureBytes, 16 * 1024 * 1024);
      return { exitCode: 0, signal: null, stdout: 'PRIVATE_CANARY stdout', stderr: 'PRIVATE_CANARY stderr' }; } });
  assert.deepEqual(calls, specs.map(({ file, args, cwd }) => ({ file, args, cwd })));
  assert(Object.isFrozen(audit)); assert.deepEqual(f.validate(audit), f.capture());
  const record = JSON.parse(readFileSync(audit.path)); assert.equal(record.runnerPid, process.pid);
  assert.equal(lstatSync(dirname(audit.path)).mode & 0o077, 0); assert.equal(lstatSync(audit.path).mode & 0o077, 0);
  for (const command of record.commands) for (const stream of ['stdout', 'stderr']) {
    const output = command[stream], bytes = readFileSync(output.path);
    assert.equal(f.digest(bytes), output.sha256); assert.equal(lstatSync(output.path).mode & 0o077, 0);
    assert.match(bytes.toString(), /\[redacted\]/); assert.doesNotMatch(bytes.toString(), /PRIVATE_CANARY/);
  }
});

test('actual audit executor never mints acceptance for nonzero, signalled or unknown command completion', async t => {
  for (const result of [{ exitCode: 1, signal: null }, { exitCode: 0, signal: 'SIGTERM' }, { exitCode: undefined, signal: null }]) {
    const f = runnerFixture(t); let calls = 0;
    await assert.rejects(createRunnerAudit({ paths: f.paths, runId: 'controlled-run', identityPorts: f.ports,
      runAndCapture: async () => { calls++; return result; } }), /Runner F1 failed/);
    assert.equal(calls, 1); assert.equal(readdirSync(join(f.paths.runDirectory, 'runner-audits'), { recursive: true }).some(name => name.endsWith('audit.json')), false);
  }
});

test('actual audit executor preserves a thrown timeout identity and refuses source drift after successful closed commands', async t => {
  const f = runnerFixture(t), primary = Object.assign(Error('controlled timeout'), { code: 'ETIMEDOUT' });
  await assert.rejects(createRunnerAudit({ paths: f.paths, runId: 'controlled-run', identityPorts: f.ports,
    runAndCapture: async () => { throw primary; } }), error => error === primary);
  let calls = 0;
  await assert.rejects(createRunnerAudit({ paths: f.paths, runId: 'controlled-run', identityPorts: f.ports,
    runAndCapture: async () => { calls++; f.put('e2eTest/README.md', 'changed during actual F1 schema fixture'); return { exitCode: 0, signal: null }; } }), /changed during F1/);
  assert.equal(calls, runnerAuditCommandSpecs(f.paths).length);
  assert.equal(readdirSync(join(f.paths.runDirectory, 'runner-audits'), { recursive: true }).some(name => name.endsWith('audit.json')), false);
});

test('V5 actual guarded seal → exact restore → current-audited replay reuses original WASM/server/CLI bytes without builders', async t => {
  const f = runnerFixture(t), persistent = createEnvironmentTestRun();
  f.paths.runDirectory = persistent.runDirectory; t.after(persistent.cleanup);
  f.paths.composeRoot = join(f.paths.coreRoot, '..', 'compose');
  mkdirSync(f.paths.composeRoot);
  const targets = environment.batchEnvironmentPaths(f.paths.coreRoot, f.paths.composeRoot), guards = [], commands = [];
  const put = (name, bytes) => { mkdirSync(dirname(name), { recursive: true }); writeFileSync(name, bytes); };
  targets.configs.forEach((name, index) => put(name, index === 0
    ? 'pub const SELECTED_SERVER_ENVIRONMENT: ServerEnvironment = ServerEnvironment::Local;\n'
    : (index === 1 ? 'VITE_META_SECRET_ENV' : 'META_SECRET_ENV') + '=local\n'));
  const cli = join(f.paths.coreRoot, 'meta-secret/target/debug/meta-cli'), wasm = join(f.paths.coreRoot, 'meta-secret/web-cli/ui/pkg');
  put(cli, 'original different CLI'); put(join(wasm, 'module.wasm'), 'original different WASM');
  const allTargets = [...targets.configs, ...targets.artifacts], original = allTargets.map(environment.environmentTreeHash);
  const coreInputs = ['Taskfile.yml', 'docker-bake.hcl', 'meta-secret/src/current.rs']; coreInputs.forEach(name => f.put(name, 'original compiled source'));
  const capture = async (file, args, options) => {
    args = Array.from(args);
    if (file === 'git') { assert.equal(options.cwd, f.paths.coreRoot); return { stdout: (args[5] === 'e2eTest' ? f.names : coreInputs).join('\0') + '\0' }; }
    if (file === 'docker') { assert.deepEqual(args, ['image', 'inspect', 'controlled-image', '--format', '{{.Id}}']); return { stdout: 'sha256:controlled-image\n' }; }
    assert(['cargo', 'rustc'].includes(file)); return { stdout: 'controlled toolchain 1\n' };
  };
  // Exact production module bodies; only git inventory is closed through the actual validator's supported port.
  const validate = options => validateRunnerAudit(options, f.ports);
  const moduleBody = (name, exports, bindings) => runInNewContext(readFileSync(new URL('./' + name, import.meta.url), 'utf8')
    .replace(/^import [\s\S]*?;\r?\n/gm, '').replace(/^export /gm, '') + '\n({' + exports.join(',') + '})', bindings);
  const actualProof = moduleBody('batch-product-proof.mjs', ['createProductProofPorts', 'captureStoredProductBundle', 'materializeStoredProductBundle', 'verifyStoredProductBundle'], {
    ...proof, ...environment, ...ios, ...fs, ...path, ...crypto, verifyArtifactProvenance, hashArtifactTree,
    captureRunnerIdentity, productRunnerIdentity, validateRunnerAudit: validate,
    verifyRunnerAuditLineage, requireIosIdentityBuild,
    execFileSync: () => assert.fail('No external process port may escape'), process, Buffer,
  });
  const create = moduleBody('batch-product-builders.mjs', ['createBatchProductPreparation'], {
    ...handoff, ...proof, ...actualProof, ...environment, ...ios, ...resources, ...fs, ...path, ...crypto,
    IOS_E2E_CONFIGURATION, hashArtifactTree, validateRunnerAudit: validate,
  }).createBatchProductPreparation;
  const ports = actualProof.createProductProofPorts({ ...f.paths, runAndCapture: capture }); let nativeCalls = 0;
  const makeGuard = async () => { const guard = environment.createBatchEnvironmentGuard({ ...targets, runDirectory: f.paths.runDirectory,
    preflight: async () => {}, setLocal: async () => assert.fail('Already local fixture'), snapshotBudget: () => ({ requiredBytes: 0 }) });
    guards.push(guard); await guard.prepare(); return guard; };
  const prepare = (guard, audit) => create({ paths: f.paths, runId: 'controlled-run', environmentGuard: guard, runnerAudit: audit, newProofVersion: 5,
    scenarios: [{ testNumber: 1, scenario: { web: {}, server: { image: 'controlled-image' } } }],
    prepareNative: async () => { nativeCalls++; put(cli, 'genuine controlled prepared CLI'); return { android: false, ios: false, cli: true }; },
    commands: { proofPorts: ports, runAndCapture: capture, checkBudget: () => ({ requiredBytes: 0 }),
      runAndWait: async (file, args) => { assert.equal(file, '/opt/homebrew/bin/task'); assert(['wasm-local', 'meta-server'].includes(args[0]));
        commands.push([...args]); if (args[0] === 'wasm-local') put(join(wasm, 'module.wasm'), 'genuine controlled compiled WASM'); return { exitCode: 0 }; } } });
  try {
    const first = await makeGuard(), firstAudit = f.audit();
    for (const name of ['products', 'owning-builds', 'supporting-products']) {
      const directory = join(f.paths.runDirectory, name), marker = join(directory, name === 'owning-builds' ? 'build-1.json' : 'original-partial');
      put(marker, 'retained partial evidence'); const bytes = readFileSync(marker);
      await assert.rejects(prepare(first, firstAudit).prepare(), /unsealed|partial|restart|preexisting/i);
      assert.deepEqual(readFileSync(marker), bytes, 'An absent index never permits replacing partial evidence');
      assert.equal(nativeCalls, 0); assert.deepEqual(commands, []); assert.equal(existsSync(join(f.paths.runDirectory, 'batch-products.json')), false);
      rmSync(directory, { recursive: true });
    }
    const indexPath = await prepare(first, firstAudit).prepare();
    const indexBytes = readFileSync(indexPath), index = JSON.parse(indexBytes), bundle = join(f.paths.runDirectory, 'supporting-products/manifest.json');
    const bundleBytes = readFileSync(bundle); assert.equal(index.version, 5); assert.equal(index.receipts.length, 2);
    assert.equal(nativeCalls, 1); assert.deepEqual(commands, [['wasm-local'], ['meta-server']]);
    await first.restore(); assert.deepEqual(allTargets.map(environment.environmentTreeHash), original);
    const next = await makeGuard(), workingBefore = allTargets.map(environment.environmentTreeHash);
    f.put('e2eTest/README.md', 'reviewed current noncompiled repair');
    await assert.rejects(prepare(next, firstAudit).prepare(), /identity mismatch/);
    assert.deepEqual(allTargets.map(environment.environmentTreeHash), workingBefore, 'Stale audit cannot authorize materialization');
    const currentAudit = f.audit(), source = join(f.paths.coreRoot, coreInputs[2]), sourceBytes = readFileSync(source);
    writeFileSync(source, 'changed compiled source'); await assert.rejects(prepare(next, currentAudit).prepare(), /input drift/); writeFileSync(source, sourceBytes);
    const product = index.receipts.find(row => row.tuple.product === 'wasm').products.wasm, productFile = join(product, 'module.wasm');
    const productBytes = readFileSync(productFile); writeFileSync(productFile, 'corrupted immutable WASM');
    await assert.rejects(prepare(next, currentAudit).prepare(), /immutable bytes|provenance|mismatch/i); writeFileSync(productFile, productBytes);
    assert.equal(await prepare(next, currentAudit).prepare(), indexPath);
    assert.equal(readFileSync(cli, 'utf8'), 'genuine controlled prepared CLI');
    assert.equal(readFileSync(join(wasm, 'module.wasm'), 'utf8'), 'genuine controlled compiled WASM');
    assert.equal(nativeCalls, 1); assert.deepEqual(commands, [['wasm-local'], ['meta-server']], 'Every replay keeps all producers at zero extra calls');
    assert.deepEqual(readFileSync(indexPath), indexBytes); assert.deepEqual(readFileSync(bundle), bundleBytes, 'No old proof upgrade or reseal');
    await next.restore(); assert.deepEqual(allTargets.map(environment.environmentTreeHash), original);
  } finally { for (const guard of guards) await guard.restore(); }
});
