import assert from 'node:assert/strict';
import test from 'node:test';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { buildRegistrationPlan, registrationMarkers } from './plan.mjs';
import { selectRegistrationExecution } from './execution-selection.mjs';

test('first-case moves exactly one case to the front without changing the approved plan or markers', () => {
  const plan = buildRegistrationPlan('order');
  const original = structuredClone(plan);
  const selected = selectRegistrationExecution(plan.cases, ['--first-case=create-ios-manual']);
  assert.equal(selected[0].id, 'create-ios-manual');
  assert.equal(selected.length, 45);
  assert.equal(new Set(selected.map(({ id }) => id)).size, 45);
  assert.deepEqual(selected.slice(1), plan.cases.filter(({ id }) => id !== 'create-ios-manual'));
  assert.deepEqual(plan, original);
  for (const entry of selected) assert.equal(entry, plan.cases.find(({ id }) => id === entry.id));
  assert.deepEqual(selected.flatMap((entry) => registrationMarkers(plan, entry)).map(({ key }) => key).sort(),
    plan.cases.flatMap((entry) => registrationMarkers(plan, entry)).map(({ key }) => key).sort());
  assert(selected.every(({ repetition, cleanBoundary }) => repetition === 1 && cleanBoundary === original.cases[0].cleanBoundary));
});

test('default order and explicit single-case selection retain their existing behavior', () => {
  const { cases } = buildRegistrationPlan('default');
  assert.deepEqual(selectRegistrationExecution(cases, []), cases);
  assert.deepEqual(selectRegistrationExecution(cases, ['--case=create-ios-manual']), [cases.find(({ id }) => id === 'create-ios-manual')]);
});

const balancedCases = ['join-web-manual-accepted', 'join-ios-manual-accepted', 'join-android-manual-accepted'];

test('explicit cases retain canonical objects, selected order and the exact 24-marker role matrix', () => {
  const plan = buildRegistrationPlan('balanced');
  const before = structuredClone(plan);
  const selected = selectRegistrationExecution(plan.cases, [`--cases=${balancedCases.join(',')}`]);
  assert.deepEqual(selected.map(({ id }) => id), balancedCases);
  assert.deepEqual(selected.map(({ platform, approver }) => [platform, approver]), [
    ['web', 'ios'], ['ios', 'android'], ['android', 'web'],
  ]);
  for (const entry of selected) assert.equal(entry, plan.cases.find(({ id }) => id === entry.id));
  const markers = selected.flatMap((entry) => registrationMarkers(plan, entry));
  assert.equal(markers.length, 24);
  assert.equal(new Set(markers.map(({ key }) => key)).size, 24);
  assert(selected.every(({ repetition, provider, outcome }) => repetition === 1 && provider === 'manual' && outcome === 'accepted'));
  assert.deepEqual(plan, before);
  assert.equal(plan.cases.flatMap((entry) => registrationMarkers(plan, entry)).length, 479);
  assert.deepEqual(selectRegistrationExecution(plan.cases, [`--cases=${[...balancedCases].reverse().join(',')}`]), [...selected].reverse());
  assert.deepEqual(selectRegistrationExecution(plan.cases, [`--cases=${balancedCases[0]}`]), [selected[0]]);
});

test('cases rejects empty segments, duplicate/unknown IDs and conflicting selectors before both CLI preflights', () => {
  const { cases } = buildRegistrationPlan('cases-invalid');
  const invalid = [
    ['--cases='], ['--cases=,'], ['--cases=missing'],
    [`--cases=,${balancedCases[0]}`], [`--cases=${balancedCases[0]},`],
    [`--cases=${balancedCases[0]},,${balancedCases[1]}`],
    [`--cases=${balancedCases[0]},${balancedCases[0]}`],
    [`--cases=${balancedCases[0]},missing`],
    [`--cases=${balancedCases[0]}`, `--cases=${balancedCases[1]}`],
    ...['--case', '--first-case', '--from-case'].map((selector) => [`--cases=${balancedCases[0]}`, `${selector}=${balancedCases[1]}`]),
  ];
  for (const args of invalid) assert.throws(() => selectRegistrationExecution(cases, args), undefined, args.join(' '));
  for (const entry of ['./local-run.mjs', './orchestrator.mjs']) {
    const command = fileURLToPath(new URL(entry, import.meta.url));
    const valid = spawnSync(process.execPath, [command, `--cases=${balancedCases.join(',')}`, '--validate-only'], { encoding: 'utf8', timeout: 10_000 });
    assert.equal(valid.status, 0, valid.stderr);
    assert.match(valid.stdout, /cases: 45/);
    assert.doesNotMatch(valid.stdout, /official native|preparation passed|real Core UI cases start/);
    for (const args of invalid) {
      const result = spawnSync(process.execPath, [command, ...args, '--validate-only'], { encoding: 'utf8', timeout: 10_000 });
      assert.notEqual(result.status, 0, args.join(' '));
      assert.doesNotMatch(result.stdout, /official native|preparation passed|real Core UI cases start/);
    }
  }
});

test('conflicting, repeated, empty, unsupported and unknown selectors reject before execution', () => {
  const { cases } = buildRegistrationPlan('invalid');
  for (const args of [
    ['--case=create-web-manual', '--first-case=create-ios-manual'],
    ['--first-case=missing'], ['--case=missing'], ['--first-case='], ['--case='],
    ['--first-case=create-ios-manual', '--first-case=create-web-manual'], ['--unexpected'],
  ]) assert.throws(() => selectRegistrationExecution(cases, args), undefined, args.join(' '));
});

test('both CLI entry points validate first-case before any environment preparation', () => {
  for (const entry of ['./local-run.mjs', './orchestrator.mjs']) {
    const command = fileURLToPath(new URL(entry, import.meta.url));
    const run = (args) => spawnSync(process.execPath, [command, ...args, '--validate-only'], { encoding: 'utf8', timeout: 10_000 });
    const valid = run(['--first-case=create-ios-manual']);
    assert.equal(valid.status, 0, valid.stderr);
    assert.match(valid.stdout, /cases: 45/);
    assert.doesNotMatch(valid.stdout, /official native|preparation passed|real Core UI cases start/);
    for (const args of [['--first-case=missing'], ['--case=create-web-manual', '--first-case=create-ios-manual']]) {
      const invalid = run(args);
      assert.notEqual(invalid.status, 0);
      assert.doesNotMatch(invalid.stdout, /official native|preparation passed|real Core UI cases start/);
    }
  }
});


test('from-case selects canonical25 through45 without mutation or inventing completion', () => {
  const plan = buildRegistrationPlan('suffix');
  const before = structuredClone(plan);
  const selected = selectRegistrationExecution(plan.cases, ['--from-case=join-android-manual-declined-retry']);
  assert.equal(plan.cases[24].id, 'join-android-manual-declined-retry');
  assert.equal(selected.length, 21);
  assert.deepEqual(selected, plan.cases.slice(24));
  for (let index = 0; index < selected.length; index += 1) assert.equal(selected[index], plan.cases[index + 24]);
  assert.deepEqual(plan, before);
  assert.equal(plan.cases.flatMap((entry) => registrationMarkers(plan, entry)).length, 479);
  assert.deepEqual(selectRegistrationExecution(plan.cases, [`--from-case=${plan.cases[0].id}`]), plan.cases);
  assert.deepEqual(selectRegistrationExecution(plan.cases, [`--from-case=${plan.cases.at(-1).id}`]), [plan.cases.at(-1)]);
});

test('from-case rejects empty/unknown/duplicate/conflicting selectors before CLI preflight', () => {
  const { cases } = buildRegistrationPlan('suffix-invalid');
  const invalid = [['--from-case='], ['--from-case=missing'],
    ['--from-case=create-web-manual', '--from-case=create-ios-manual'],
    ['--from-case=create-web-manual', '--case=create-web-manual'],
    ['--from-case=create-web-manual', '--first-case=create-ios-manual']];
  for (const args of invalid) assert.throws(() => selectRegistrationExecution(cases, args));
  for (const entry of ['./local-run.mjs', './orchestrator.mjs']) {
    const command = fileURLToPath(new URL(entry, import.meta.url));
    const valid = spawnSync(process.execPath, [command, '--from-case=join-android-manual-declined-retry', '--validate-only'], { encoding: 'utf8', timeout: 10_000 });
    assert.equal(valid.status, 0, valid.stderr);
    for (const args of invalid) {
      const result = spawnSync(process.execPath, [command, ...args, '--validate-only'], { encoding: 'utf8', timeout: 10_000 });
      assert.notEqual(result.status, 0);
      assert.doesNotMatch(result.stdout, /official native|preparation passed|real Core UI cases start/);
    }
  }
});
