import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, existsSync } from 'node:fs';
import { parseUiE2eRangeOptions, selectUiE2eCommand } from './ui-e2e-range-selection.mjs';

test('default selects all49defined IDs1–50, skips absent18 and excludes registration51', () => {
  const options = parseUiE2eRangeOptions([]);
  assert.deepEqual([options.from, options.to], [1, 50]);
  const scripts = JSON.parse(readFileSync(new URL('../package.json', import.meta.url))).scripts;
  const selected = [];
  for (let number = options.from; number <= options.to; number++) {
    if (selectUiE2eCommand(number, scripts, existsSync(new URL(`../Test${number}/orchestrator.mjs`, import.meta.url)))) selected.push(number);
  }
  assert.equal(selected.length, 49); assert(!selected.includes(18)); assert(!selected.includes(51));
  assert.equal(selectUiE2eCommand(50, scripts, true), 'test-50:blocks');
});

test('declared block priority, missing participant and invalid ranges still fail before runtime', () => {
  assert.equal(selectUiE2eCommand(4, { 'test-4': 'one', 'test-4:blocks': 'all' }, true), 'test-4:blocks');
  assert.throws(() => selectUiE2eCommand(17, {}, false), /registered/);
  assert.throws(() => selectUiE2eCommand(50, { 'test-50': 'one' }, false), /orchestrator/);
  for (const args of [['--from', '0'], ['--from', '50', '--to', '49'], ['--to', '52'], ['--run-id', 'unsafe value']]) assert.throws(() => parseUiE2eRangeOptions(args));
  assert.equal(parseUiE2eRangeOptions(['--from=51', '--to=51']).to, 51, 'Explicit registration-only use is preserved');
});

test('maintenance pause is opt-in, canonical and unavailable to diagnostic blocks', () => {
  assert.equal(parseUiE2eRangeOptions([]).pauseAfterEachCase, false);
  const options = parseUiE2eRangeOptions(['--from', '26', '--to', '49', '--run-id', 'maintenance-owner', '--preserve-evidence', '--pause-after-each-case']);
  assert.equal(options.pauseAfterEachCase, true); assert.equal(options.preserveEvidence, true);
  assert.deepEqual([options.from, options.to, options.runId], [26, 49, 'maintenance-owner']);
  for (const args of [ ['--pause-after-each-case', '--pause-after-each-case'],
    ['--pause-after-each-case=true'], ['--pause-after-each-case', 'true'],
    ['--from', '9', '--to', '9', '--run-id', 'diagnostic-owner', '--diagnostic-block', '2', '--pause-after-each-case'] ]) {
    assert.throws(() => parseUiE2eRangeOptions(args));
  }
});
