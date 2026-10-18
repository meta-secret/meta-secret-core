import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';
import { buildRegistrationPlan, validateRegistrationPlan, caseBlockers } from './plan.mjs';
import { selectRegistrationExecution } from './execution-selection.mjs';

const directory = dirname(fileURLToPath(import.meta.url));
const runId = process.env.E2E_RUN_ID ?? `registration-${Date.now()}`;
const plan = buildRegistrationPlan(runId);
console.log('📋 Test51 F1 expanded registration matrix:', validateRegistrationPlan(plan));
const selected = selectRegistrationExecution(plan.cases, process.argv.slice(2));
if (process.argv.includes('--validate-only')) process.exit(0);
if (process.env.E2E_RANGE_RUN !== '1') throw new Error('Run via the existing local environment coordinator; it owns the single snapshot/restore');
const artifactDirectory = resolve(directory, '..', '.artifacts', runId, 'test51-config');
mkdirSync(artifactDirectory, { recursive: true });
const base = JSON.parse(readFileSync(resolve(directory, 'scenario.json'), 'utf8'));
const results = [];
let activeChild;
let interrupted = false;
for (const signal of ['SIGINT', 'SIGTERM']) process.once(signal, () => { interrupted = true; activeChild?.kill(signal); });
const save = () => writeFileSync(resolve(artifactDirectory, 'result.json'), `${JSON.stringify({
  runId, requestedCases: 45, selectedCases: selected.length, requestedRepetitions: 1,
  businessAcceptance: selected.length === 45 && results.length === 45 && results.every(({ status }) => status === 'passed'),
  results,
}, null, 2)}\n`);
let completedCases = 0;
const provenScopes = new Set();
for (const planned of selected) {
  if (interrupted) break;
  const blockers = caseBlockers(planned);
  if (blockers.length) {
    console.log(`🟡 Test51 ${planned.id}: BLOCKED — ${blockers.join('; ')}`);
    results.push({ id: planned.id, status: 'blocked', blockers }); save(); continue;
  }
  const providerEmail = process.env[`E2E_REAL_${planned.platform.toUpperCase()}_${planned.provider.toUpperCase()}_EMAIL`];
  const entry = { ...planned, runId, vaultName: planned.provider === 'manual' ? planned.vaultName : providerEmail };
  const registrationPlatforms = entry.kind === 'create' ? [entry.platform] : [entry.platform, entry.approver,
    ...(entry.outcome !== 'accepted' ? [entry.retryApprover] : [])];
  if (registrationPlatforms.some((platform) => platform !== 'web') && process.env.E2E_REUSE_PROVEN_ARTIFACTS !== '1') {
    results.push({ id: entry.id, status: 'blocked', blockers: ['Parent one-time F2 native/app/test package preparation is required before any native clients start'] });
    save(); continue;
  }
  const scenario = { ...base, runId: `${runId}-${entry.id}`, registrationCase: entry, registrationPlatforms,
    vault: { name: entry.vaultName },
  };
  const path = resolve(artifactDirectory, `scenario-${entry.id}.json`);
  writeFileSync(path, `${JSON.stringify(scenario, null, 2)}\n`);
  console.log(`🚀 Test51 ${entry.id}: START; fresh DB and identity; one pass; actors=${registrationPlatforms.join('/')}`);
  const scope = registrationPlatforms.every((platform) => platform === 'web') ? 'web' : 'native';
  const result = await new Promise((resolveResult, reject) => {
    activeChild = spawn(process.execPath, [resolve(directory, '..', 'Test4', 'orchestrator.mjs'), path, '--exit-on-success'], {
      cwd: resolve(directory, '..'), stdio: 'inherit',
      // A Web-only result does not establish native provenance. Native cases
      // require the parent's full one-time F2 handoff, never build in a cycle.
      env: { ...process.env, ...((completedCases && scope === 'web') ? { E2E_REUSE_PROVEN_ARTIFACTS: '1' } : {}),
        E2E_BOOTSTRAP_BUILD_PROVENANCE: process.env.E2E_BOOTSTRAP_BUILD_PROVENANCE === '1' && !provenScopes.has(scope) ? '1' : '0',
      },
    });
    activeChild.once('error', reject);
    activeChild.once('exit', (code, signal) => resolveResult({ code, signal }));
  });
  activeChild = null;
  const status = result.code === 0 ? 'passed' : interrupted ? 'interrupted' : 'failed';
  results.push({ id: entry.id, status, ...result }); save();
  if (status !== 'passed') {
    console.log(`❌ Test51 ${entry.id}: ${status}; queue paused for diagnosis; production edits require owner approval`);
    process.exitCode = 1; break;
  }
  completedCases += 1;
  provenScopes.add(scope);
}
for (const entry of selected.filter(({ id }) => !results.some((result) => result.id === id))) {
  results.push({ id: entry.id, status: 'not-run', blockers: caseBlockers(entry) });
}
save();
const passed = results.filter(({ status }) => status === 'passed').length;
const blocked = results.filter(({ status }) => status === 'blocked').length;
console.log(`📦 Test51: ${passed} passed / ${blocked} blocked / ${results.filter(({ status }) => status === 'not-run').length} not run; expanded full matrix45; selected${selected.length}; no mocked providers/cameras`);
if (blocked && !process.exitCode) process.exitCode = 2;
