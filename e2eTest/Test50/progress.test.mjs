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
  const start = guard.indexOf("if (process.env.E2E_RANGE_RUN === '1')");
  const branch = guard.slice(start, guard.indexOf("  if (reuseProvenArtifacts) {\n    if (uniqueModes[0]", start));
  assert(branch.includes('if (batchProducts) buildProvenanceVerified = true;'));
  assert(branch.includes('else await verifyOrBootstrapBuildProvenance();'));
  assert(branch.indexOf('await verifyOrBootstrapBuildProvenance()') < branch.indexOf('    return;'));
  const main = source.slice(source.indexOf('async function main()'));
  assert(main.indexOf('batchProducts = await loadBatchProducts(') >= 0);
  assert(main.indexOf('batchProducts = await loadBatchProducts(') < main.indexOf('await prepareLocalEnvironmentGuard()'));
  const consumer = readFileSync(new URL('../shared/batch-product-consumer.mjs', import.meta.url), 'utf8');
  const load = consumer.slice(consumer.indexOf('export async function loadBatchProducts('), consumer.indexOf('export function assertBatchProductsCurrent('));
  assert(load.includes('!index.sealed || index.runId !== process.env.E2E_RUN_ID'));
  assert(load.indexOf('await verifyPreparedProduct(') > 0);
  assert(load.indexOf('await verifyPreparedProduct(') < load.indexOf('selected[product] ='));
  assert(load.indexOf('selected[product] =') < load.indexOf('return { ...selected'));
});
