import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { deletionCycleProgress } from './progress.mjs';
import { scenarioProgressLine } from '../shared/scenario-progress.mjs';
import { buildDeletionPlan } from './plan.mjs';

test('all 60 real cycle role messages are explicit live milestones', () => {
  for (const message of ['🧪 Test50 cycle=1 winner=web outcome=accepted persisted-and-observed=true', '✅ Test50 block=one-web cycle=1 terminal PASS']) assert.equal(scenarioProgressLine(message)?.kind, 'explicit');
  for (const block of buildDeletionPlan('progress').blocks) for (const cycle of block.cycles) {
    const message = deletionCycleProgress(block, cycle);
    assert.equal(scenarioProgressLine(message)?.kind, 'explicit');
    assert(message.includes(`${cycle.requester}:requester`));
    if (cycle.responder) assert(message.includes(`${cycle.responder}:${cycle.outcome === 'accepted' ? 'approver' : 'decliner'}`));
    if (cycle.offline) assert(message.includes(`${cycle.offline}:offline`));
  }
});

test('every new range child verifies full provenance before reuse early return', () => {
  const source = readFileSync(new URL('../Test4/orchestrator.mjs', import.meta.url), 'utf8');
  const guard = source.slice(source.indexOf('async function prepareLocalEnvironmentGuard()'), source.indexOf('async function restoreEnvironmentGuard()'));
  const branch = guard.slice(guard.indexOf("if (process.env.E2E_RANGE_RUN === '1')"), guard.indexOf('  if (reuseProvenArtifacts) {'));
  assert(branch.includes('if (reuseProvenArtifacts) await verifyOrBootstrapBuildProvenance();'));
  assert(branch.indexOf('await verifyOrBootstrapBuildProvenance()') < branch.indexOf('    return;'));
});
