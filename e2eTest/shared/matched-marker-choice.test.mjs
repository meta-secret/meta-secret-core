import test from 'node:test';
import assert from 'node:assert/strict';
import { createMatchedMarkerChoices } from './matched-marker-choice.mjs';

function clock() {
  const tasks = new Map(); let sequence = 0;
  return { tasks, setTimeout(fn) { const id = ++sequence; tasks.set(id, fn); return id; }, clearTimeout(id) { tasks.delete(id); }, fire() { for (const [id, fn] of [...tasks]) { tasks.delete(id); fn(); } } };
}
const choices = [{ marker: 'E2E: IOS_INCOMING_VISIBLE_10_1 claimId=claim-a', value: { logicalStep: 1, requestId: 'claim-a' } }, { marker: 'E2E: IOS_INCOMING_VISIBLE_10_2 claimId=claim-b', value: { logicalStep: 2, requestId: 'claim-b' } }];

test('one actual matching completed native line returns the same allowed step/UUID, including chunk splits', async () => {
  const timers = clock(); const registry = createMatchedMarkerChoices({ timers });
  const wait = registry.wait(choices, { timeoutMs: 50 });
  assert.equal(registry.activeCount, 1); assert.equal(timers.tasks.size, 1);
  registry.push('stdout', 'log prefix E2E: IOS_INCOMING_VISIBLE_10_2 claimId=cl');
  registry.push('stderr', 'unrelated actor diagnostics\n');
  registry.push('stdout', 'aim-b'); assert.equal(registry.activeCount, 1);
  registry.push('stdout', '\n');
  assert.equal(await wait, choices[1].value); assert.equal(registry.activeCount, 0); assert.equal(timers.tasks.size, 0);
});

test('foreign cycle/UUID, partial marker and step prefix never choose an approval', async () => {
  const timers = clock(); const registry = createMatchedMarkerChoices({ timers }); const wait = registry.wait(choices, { timeoutMs: 10 });
  for (const line of ['xE2E: IOS_INCOMING_VISIBLE_10_2 claimId=claim-b', 'E2E: IOS_INCOMING_VISIBLE_1_2 claimId=claim-b', 'E2E: IOS_INCOMING_VISIBLE_10_20 claimId=claim-b', 'E2E: IOS_INCOMING_VISIBLE_10_2 claimId=foreign', 'E2E: ANDROID_INCOMING_VISIBLE_10_2 claimId=claim-b']) registry.push('stdout', `${line}\n`);
  assert.equal(registry.activeCount, 1); timers.fire(); await assert.rejects(wait, /Timed out.*matched/i); assert.equal(registry.activeCount, 0);
});

test('cancelling one choice removes only its own listener/deadline, never another owned wait', async () => {
  const timers = clock(); const registry = createMatchedMarkerChoices({ timers }); const abort = new AbortController();
  const first = registry.wait(choices, { timeoutMs: 10, signal: abort.signal });
  const second = registry.wait([{ marker: 'E2E: IOS_OTHER_READY', value: 'other' }], { timeoutMs: 10 });
  assert.equal(registry.activeCount, 2); abort.abort(); await assert.rejects(first, /abort|cancel/i);
  assert.equal(registry.activeCount, 1); assert.equal(timers.tasks.size, 1);
  registry.push('stdout', 'E2E: IOS_OTHER_READY\n'); assert.equal(await second, 'other'); assert.equal(timers.tasks.size, 0);
});

test('actual actor failure rejects all its active choices, preserves the error and closes future waits', async () => {
  const timers = clock(); const registry = createMatchedMarkerChoices({ timers }); const failure = new Error('native actor exited');
  const first = registry.wait(choices, { timeoutMs: 10 }); const second = registry.wait([{ marker: 'E2E: OTHER', value: 3 }], { timeoutMs: 10 });
  registry.fail(failure); await assert.rejects(first, (error) => error === failure); await assert.rejects(second, (error) => error === failure);
  assert.equal(registry.activeCount, 0); assert.equal(timers.tasks.size, 0);
  await assert.rejects(registry.wait(choices, { timeoutMs: 10 }), (error) => error === failure);
});

test('initial history is actor-scoped and ambiguous alternatives reject rather than invent order', async () => {
  const initial = () => ({ stdout: `${choices[1].marker}\n`, stderr: '' }); const registry = createMatchedMarkerChoices({ initialStreams: initial });
  assert.equal(await registry.wait(choices), choices[1].value); assert.equal(registry.activeCount, 0);
  const ambiguous = createMatchedMarkerChoices({ initialStreams: () => ({ stdout: `${choices[0].marker}\n`, stderr: `${choices[1].marker}\n` }) });
  await assert.rejects(ambiguous.wait(choices), /ambiguous/i);
  await assert.rejects(registry.wait([choices[0], choices[0]]), /duplicate/i);
});
