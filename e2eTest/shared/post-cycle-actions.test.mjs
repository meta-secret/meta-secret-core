import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { createPostCycleActions } from './post-cycle-actions.mjs';

test('Test23 full timeline creates Secret2 after closed cycle6 and Secret3 after closed cycle30', async () => {
  const scenario = JSON.parse(readFileSync(new URL('../Test23/scenario.json', import.meta.url), 'utf8'));
  const cycles = scenario.recovery.cyclePlan.flatMap((block) => Array.from({ length: block.count }, () => block));
  const events = [];
  const secrets = new Set(['secret1']);
  const afterCompletedCycle = createPostCycleActions([
    { name: 'Secret2', afterCycle: scenario.recovery.createSecret2AfterCycle,
      async run(number) { events.push(`create-secret2-${number}`); secrets.add('secret2'); } },
    { name: 'Secret3', afterCycle: scenario.recovery.createSecret3AfterCycle,
      async run(number) { events.push(`create-secret3-${number}`); secrets.add('secret3'); } },
  ]);
  assert.equal(cycles.length, scenario.recovery.expectedCycles);
  assert.equal(cycles.filter((cycle) => cycle.mode !== 'direct-show').length, scenario.recovery.expectedRecoveryCycles);
  for (let index = 0; index < cycles.length; index++) {
    const cycle = cycles[index];
    const number = index + 1;
    events.push(`start-${number}`);
    for (const sender of cycle.senders) assert.equal(secrets.has(sender.secret), true, `secret exists before cycle ${number}`);
    events.push(`reveal-closed-${number}`);
    events.push(`actors-completed-${number}`);
    await afterCompletedCycle(number);
  }
  for (const [secret, boundary] of [['secret2', 6], ['secret3', 30]]) {
    const actionIndex = events.indexOf(`create-${secret}-${boundary}`);
    assert.ok(actionIndex > events.indexOf(`actors-completed-${boundary}`));
    assert.ok(actionIndex < events.indexOf(`start-${boundary + 1}`));
    assert.equal(events.filter((event) => event.startsWith(`create-${secret}`)).length, 1);
  }
  assert.equal(events.filter((event) => event.startsWith('reveal-closed')).length, 39);
});

test('post-cycle action must finish its confirmation before caller starts the next cycle', async () => {
  let confirm;
  const confirmed = new Promise((resolve) => { confirm = resolve; });
  let boundaryFinished = false;
  const afterCompletedCycle = createPostCycleActions([
    { name: 'create', afterCycle: 6, run: async () => confirmed },
  ]);
  const boundary = afterCompletedCycle(6).then(() => { boundaryFinished = true; });
  await Promise.resolve();
  assert.equal(boundaryFinished, false);
  confirm();
  await boundary;
  assert.equal(boundaryFinished, true);
  await assert.rejects(afterCompletedCycle(6), /already handled/);
});
