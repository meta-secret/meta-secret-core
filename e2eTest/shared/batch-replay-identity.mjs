import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { lstatSync, readFileSync, realpathSync } from 'node:fs';
import { resolve, sep } from 'node:path';

const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const equal = (a, b) => JSON.stringify(a) === JSON.stringify(b);
export const RUNNER_RECIPE_PATHS = Object.freeze([
  'run-ui-e2e-range.mjs', ...['batch-product-builders', 'batch-product-proof', 'batch-product-consumer',
    'batch-build-handoff', 'batch-environment', 'batch-replay-identity', 'runner-validation-audit',
    'e2e-resource-budget', 'build-provenance-manifest', 'ios-ffi-provenance', 'ios-test-identity',
    'artifact-tree', 'platform-readiness'].map(name => `shared/${name}.mjs`),
]);
// CONTENT only. Missing/new files, recipes, JSON and all unknown inputs stay strict.
export const RUNNER_NONCOMPILED_PATHS = Object.freeze([
  'README.md', ...[1, 2, 3, 4].map(n => `Test${n}/orchestrator.mjs`),
  'shared/platform-supervisor.mjs', 'shared/preparation-metrics.mjs',
  ...['platform-supervisor', 'preparation-metrics', 'range-coordinator-caller', 'build-provenance-manifest',
    'e2e-resource-budget', 'batch-environment', 'platform-prebuilt-install'].map(name => `shared/${name}.test.mjs`),
]);
export function captureRunnerIdentity(paths, { listFiles = root => execFileSync('git',
  ['ls-files', '-co', '--exclude-standard', '-z', '--', 'e2eTest'],
  { cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, timeout: 30_000 }).split('\0').filter(Boolean) } = {}) {
  const core = realpathSync(paths.coreRoot), files = [...new Set(listFiles(core))].sort();
  const strict = {}, noncompiled = {}, scenario = {};
  for (const name of files) {
    if (!name.startsWith('e2eTest/') || /[\\\0]/.test(name)
      || name.split('/').some(part => !part || part === '.' || part === '..')) throw Error('Runner inventory relative path is not canonical');
    const path = resolve(core, name);
    if (!name.startsWith('e2eTest/') || path !== resolve(core, 'e2eTest', name.slice(8))
      || realpathSync(path) !== path || !lstatSync(path).isFile()) throw Error('Runner inventory path missing or noncanonical');
    const relative = name.slice(8), digest = hash(readFileSync(path));
    if (name.endsWith('.json')) scenario[name] = digest;
    else if (RUNNER_NONCOMPILED_PATHS.includes(relative) && !RUNNER_RECIPE_PATHS.includes(relative)) noncompiled[name] = digest;
    else strict[name] = digest;
  }
  for (const path of RUNNER_RECIPE_PATHS) if (!Object.hasOwn(strict, `e2eTest/${path}`)) throw Error('Required runner recipe missing from inventory');
  const identity = { version: 1, membership: files, strict, noncompiled, scenario };
  return { ...identity, digest: hash(JSON.stringify(identity)) };
}
export function runnerAuditCommandSpecs(paths) {
  const cwd = resolve(paths.coreRoot, 'e2eTest');
  const syntax = [...new Set([...RUNNER_RECIPE_PATHS, ...[1, 2, 3, 4].map(n => `Test${n}/orchestrator.mjs`),
    'shared/platform-supervisor.mjs', 'shared/preparation-metrics.mjs'])];
  return [...syntax.map(path => ({ label: `syntax:${path}`, file: process.execPath, args: ['--check', path], cwd })),
    { label: 'related-node', file: process.execPath, args: ['--test', ...['artifact-tree', 'e2e-resource-budget', 'preparation-metrics',
      'platform-supervisor', 'range-coordinator-caller', 'build-provenance-manifest', 'batch-environment',
      'batch-environment-restore', 'batch-stripped-support', 'platform-prebuilt-install'].map(name => `shared/${name}.test.mjs`),
      'Test50/ios-same-request-show.test.mjs', 'Test50/ios-global-privacy-snapshot.test.mjs'], cwd }];
}
function privateAuditPath(path, paths) {
  if (realpathSync(paths.runDirectory) !== resolve(paths.runDirectory)) throw Error('Canonical runner audit root required');
  const base = resolve(paths.runDirectory, 'runner-audits'), actual = resolve(path);
  if (!actual.startsWith(base + sep) || realpathSync(actual) !== actual || !lstatSync(actual).isFile()
    || (lstatSync(actual).mode & 0o077) !== 0) throw Error('Private current runner audit path required');
  return actual;
}
export function verifyRunnerAuditLineage({ paths, runId, audit }) {
  if (!audit) throw Error('Runner audit lineage required');
  const path = privateAuditPath(audit.path, paths), bytes = readFileSync(path);
  if (hash(bytes) !== audit.sha256) throw Error('Runner audit immutable bytes changed');
  const record = JSON.parse(bytes);
  if (record.version !== 1 || record.runId !== runId || !Number.isSafeInteger(record.runnerPid) || record.runnerPid <= 0
    || !equal(record.before, record.after)) throw Error('Runner audit origin or stable identity missing');
  const specs = runnerAuditCommandSpecs(paths);
  if (!Array.isArray(record.commands) || record.commands.length !== specs.length) throw Error('Exact F1 command inventory required');
  for (const [i, spec] of specs.entries()) {
    const receipt = record.commands[i];
    if (!equal({ label: receipt.label, file: receipt.file, args: receipt.args, cwd: receipt.cwd }, spec)
      || receipt.naturalExitCode !== 0 || receipt.signal !== null) throw Error('Actual successful fixed F1 command receipts required');
    for (const output of [receipt.stdout, receipt.stderr]) {
      if (!output || hash(readFileSync(privateAuditPath(output.path, paths))) !== output.sha256) throw Error('Actual F1 output evidence changed');
    }
  }
  return record;
}
export function validateRunnerAudit({ paths, runId, audit, expectedRunnerPid = process.pid }, ports = {}) {
  if (!Number.isSafeInteger(expectedRunnerPid) || expectedRunnerPid <= 0) throw Error('Fresh runner audit handoff required');
  const record = verifyRunnerAuditLineage({ paths, runId, audit }), current = captureRunnerIdentity(paths, ports);
  if (record.runnerPid !== expectedRunnerPid || !equal(record.before, current) || !equal(record.after, current)) throw Error('Runner audit origin or current identity mismatch');
  return current;
}
export function productRunnerIdentity(identity) {
  return { version: identity.version, membership: identity.membership, strict: identity.strict,
    scenario: identity.scenario, noncompiled: Object.keys(identity.noncompiled) };
}
