import assert from 'node:assert/strict';
import test from 'node:test';
import { createPreparationMetrics } from './preparation-metrics.mjs';

const sample = { freeBytes: 64, swapUsedBytes: 6, swapTotalBytes: 8 };
const keys = ['sequence', 'phase', 'durationMs', 'freeBeforeBytes', 'freeAfterBytes', 'swapBeforeBytes',
  'swapAfterBytes', 'swapTotalBeforeBytes', 'swapTotalAfterBytes', 'outcome', 'measurementErrors'];

test('phase metrics record fixed scalar values and preserve the identical successful result', async () => {
  const values = [{ ...sample, secretCanary: 'UNLOGGED_CANARY' }, { freeBytes: 60, swapUsedBytes: 7, swapTotalBytes: 8 }];
  const times = [1000000n, 4000000n], written = [], value = { untouched: true };
  const metrics = createPreparationMetrics({ observe: async () => values.shift(), now: () => times.shift(), write: record => written.push(record) });
  assert.equal(await metrics.measure('snapshot', async () => value), value);
  assert.deepEqual(metrics.snapshot(), [{ sequence: 1, phase: 'snapshot', durationMs: 3,
    freeBeforeBytes: 64, freeAfterBytes: 60, swapBeforeBytes: 6, swapAfterBytes: 7,
    swapTotalBeforeBytes: 8, swapTotalAfterBytes: 8, outcome: 0, measurementErrors: 0 }]);
  assert.deepEqual(written, metrics.snapshot()); assert.deepEqual(Object.keys(written[0]), keys);
  assert.equal(JSON.stringify(written).includes('UNLOGGED_CANARY'), false);
});

test('primary operation Error survives failing observation, clock and persistence unchanged', async () => {
  const failure = new Error('Controlled primary operation failure');
  const metrics = createPreparationMetrics({ observe: async () => { throw new Error('Unlogged observation canary'); },
    now: () => { throw new Error('Unlogged clock canary'); }, write: async () => { throw new Error('Unlogged persistence canary'); } });
  await assert.rejects(metrics.measure('native', async () => { throw failure; }), error => error === failure);
  const [record] = metrics.snapshot(); assert.equal(record.outcome, 1); assert.equal(record.measurementErrors, 5);
  for (const key of ['freeBeforeBytes', 'freeAfterBytes', 'swapBeforeBytes', 'swapAfterBytes', 'swapTotalBeforeBytes', 'swapTotalAfterBytes'])
    assert.equal(record[key], -1, 'Unavailable diagnostics must not become zero');
  assert.equal(record.durationMs, 0); assert.equal(JSON.stringify(record).includes('canary'), false);
});

test('measurement failures cannot synthesize a business failure after successful work', async () => {
  const value = Symbol('unchanged result');
  const metrics = createPreparationMetrics({ observe: async () => null, now: () => 10n,
    write: async () => { throw new Error('Controlled write failure'); } });
  assert.equal(await metrics.measure('wasm-build', () => value), value);
  const [record] = metrics.snapshot(); assert.equal(record.outcome, 0); assert.equal(record.measurementErrors, 3);
  assert.equal(record.freeBeforeBytes, -1); assert.equal(record.swapAfterBytes, -1);
});

test('all approved phase labels sequence monotonically without caller-controlled payload fields', async () => {
  let time = 0n;
  const metrics = createPreparationMetrics({ observe: async () => sample, now: () => ++time });
  const phases = ['snapshot', 'native', 'android-build', 'ios-build', 'wasm-build', 'server-build',
    'supporting-capture', 'product-replay', 'preparation-command', 'f1-validation', 'platform-readiness',
    'state-reset', 'business-scenario', 'scenario-postcheck', 'environment-restore', 'runtime-release', 'runtime-cleanup'];
  for (const phase of phases) await metrics.measure(phase, async () => undefined);
  assert.deepEqual(metrics.snapshot().map(row => row.sequence), phases.map((_, index) => index + 1));
  assert.deepEqual(metrics.snapshot().map(row => row.phase), phases);
  for (const row of metrics.snapshot()) { assert.deepEqual(Object.keys(row), keys); assert(row.durationMs >= 0); }
});

test('unknown phase and invalid operation refuse before observing or invoking any caller action', async () => {
  let calls = 0;
  const metrics = createPreparationMetrics({ observe: async () => { calls++; return sample; } });
  for (const phase of ['native/PRIVATE_CANARY', '', null, {}, 'android-build\nextra'])
    await assert.rejects(metrics.measure(phase, () => { calls++; }));
  await assert.rejects(metrics.measure('native', null)); assert.equal(calls, 0); assert.deepEqual(metrics.snapshot(), []);
});

test('backward or malformed clocks are qualified without changing result or primary Error', async () => {
  for (const times of [[2n, 1n], [1, 2], [1n, 'later']]) {
    const metrics = createPreparationMetrics({ observe: async () => sample, now: () => times.shift() });
    assert.equal(await metrics.measure('ios-build', () => 17), 17);
    const [record] = metrics.snapshot(); assert.equal(record.durationMs, 0); assert(record.measurementErrors >= 1);
    assert.equal(record.outcome, 0);
  }
});

test('writer completion is joined and snapshot copies cannot mutate recorded scalar history', async () => {
  let entered, release;
  const started = new Promise(resolve => { entered = resolve; }), barrier = new Promise(resolve => { release = resolve; });
  const metrics = createPreparationMetrics({ observe: async () => sample, now: () => 1n,
    write: async () => { entered(); await barrier; } });
  let finished = false; const pending = metrics.measure('supporting-capture', () => 23).then(value => { finished = true; return value; });
  await started;
  try { assert.equal(finished, false); assert.deepEqual(metrics.snapshot(), []); }
  finally { release(); assert.equal(await pending, 23); }
  const copy = metrics.snapshot(); copy[0].sequence = -1; copy.push({ secret: 'UNLOGGED' });
  assert.equal(metrics.snapshot().length, 1); assert.equal(metrics.snapshot()[0].sequence, 1);
});

test('nonfinite or negative diagnostic counters remain unavailable and preserve the operation', async () => {
  for (const value of [NaN, Infinity, -1, 0.5]) {
    const metrics = createPreparationMetrics({ observe: async () => ({ ...sample, freeBytes: value }), now: () => 1n });
    assert.equal(await metrics.measure('preparation-command', () => 0), 0);
    const [record] = metrics.snapshot(); assert.equal(record.freeBeforeBytes, -1); assert.equal(record.freeAfterBytes, -1);
    assert.equal(record.measurementErrors, 2); assert.equal(record.outcome, 0);
  }
});

test('nested owning phase and command records use monotonic completion order', async () => {
  let now = 0n; const written = [];
  const metrics = createPreparationMetrics({ observe: async () => sample, now: () => ++now,
    write: record => written.push(record) });
  const result = await metrics.measure('native', () => metrics.measure('preparation-command', async () => 29));
  assert.equal(result, 29);
  assert.deepEqual(metrics.snapshot().map(row => [row.sequence, row.phase]), [[1, 'preparation-command'], [2, 'native']]);
  assert.deepEqual(written.map(row => row.sequence), [1, 2]);
  assert(metrics.snapshot().every(row => row.outcome === 0 && row.measurementErrors === 0));
});
