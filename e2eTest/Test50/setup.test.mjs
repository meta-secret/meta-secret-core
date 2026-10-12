import { test } from 'node:test';
import { EventEmitter } from 'node:events';
import assert from 'node:assert/strict';
import { buildDeletionPlan } from './plan.mjs';
import { runDeletionBlock } from './business.mjs';

test('every full Web block reaches cycle one after waiting only for the already-created setup seed', async () => {
  const plan = buildDeletionPlan('setup-regression');
  for (const block of plan.blocks.filter(({ members }) => members.includes('web'))) {
    const seed = { name: `${block.runId}-seed`, value: 'synthetic-seed' };
    const created = new Set(); const selected = [];
    const actor = () => ({ waitForMarker: async () => {}, result: Promise.resolve() });
    const h = {
      block, scenario: { secret: seed, secrets: { seed, ...Object.fromEntries(block.cycles.map((cycle) => [cycle.secretName, { name: cycle.secretName, value: 'synthetic-cycle' }])) }, android: { testClass: 'Android' }, ios: { testClass: 'IOS' } },
      page: new EventEmitter(), approvalCoordinator: {},
      setJoinExpectedSecretNames: (names) => assert.deepEqual(names, [seed.name]),
      createWebInitiatedVault: async () => created.add(seed.name),
      startAndroidJoinTest: async (_, options) => { assert.deepEqual(options.expectedSecretNames, [seed.name]); return actor(); },
      startIosJoinTest: actor,
      approveJoinRequestOnWeb: async () => {},
      waitForWebSecrets: async () => { throw new Error('Generic future-Secret wait must not be used during setup'); },
      waitForWebSecretNames: async (_, names) => { selected.push(names); for (const name of names) assert(created.has(name), 'Cannot wait for a future Secret before its creation boundary'); },
      assertWebMembership: async (_, categories, names) => { assert.deepEqual(names, [seed.name]); assert.equal(categories.length, block.members.length); },
      onCycle: () => { throw new Error('Hermetic fixture reached the first business boundary'); },
    };
    await assert.rejects(runDeletionBlock(h), /Hermetic fixture reached the first business boundary/);
    assert.deepEqual(selected, [[seed.name]]);
    assert.equal(block.cycles.length, block.members.length === 3 ? 18 : block.members.length === 2 ? 6 : 3);
  }
});
