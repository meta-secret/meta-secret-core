import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import test from 'node:test';
import { bindIosStepSecret, createStepEnvironment } from './ios-step-secret.mjs';

const secrets = [{ name: 'first-secret', value: 'first-value' }, { name: 'second-secret', value: 'second-value' }];
const valueForName = (name) => secrets.find((secret) => secret.name === name)?.value;

test('a stale default environment cannot override an explicit later-cycle secret', () => {
  assert.throws(() => bindIosStepSecret({
    secretName: secrets[1].name, secretValue: secrets[1].value,
    environment: { E2E_SECRET_NAME: secrets[0].name }, defaultSecret: secrets[0], valueForName,
  }), (error) => error.message.includes('selected-secret conflict') && !error.message.includes(secrets[0].name)
    && !error.message.includes(secrets[1].name) && !error.message.includes('value'));
});

test('explicit/environment values must match each other and the selected scenario secret', () => {
  for (const args of [
    { secretName: secrets[1].name, secretValue: secrets[1].value, environment: { E2E_SECRET_VALUE: secrets[0].value } },
    { secretName: secrets[1].name, environment: { E2E_SECRET_NAME: secrets[1].name, E2E_SECRET_VALUE: secrets[0].value } },
  ]) {
    assert.throws(() => bindIosStepSecret({ ...args, defaultSecret: secrets[0], valueForName }), (error) =>
      error.message.includes('value') && !error.message.includes(secrets[0].value) && !error.message.includes(secrets[1].value));
  }
});

test('environment-only and explicit-only selection bind matching values and identical launch/config fields', () => {
  const neutralStep = createStepEnvironment({ role: 'sender', cycle: 25, step: 1 });
  assert.equal(Object.hasOwn(neutralStep, 'E2E_SECRET_NAME'), false);
  for (const args of [
    { secretName: secrets[1].name, environment: neutralStep },
    { environment: { ...neutralStep, E2E_SECRET_NAME: secrets[1].name } },
  ]) {
    const result = bindIosStepSecret({ ...args, defaultSecret: secrets[0], valueForName });
    assert.equal(result.name, secrets[1].name);
    assert.equal(result.value, secrets[1].value);
    assert.equal(result.environment.E2E_SECRET_NAME, result.name);
    assert.equal(result.environment.E2E_SECRET_VALUE, result.value);
  }
});

test('every declared multi-secret scenario selection binds consistently for all native roles', () => {
  let scenarios = 0;
  let bindings = 0;
  const root = new URL('../', import.meta.url);
  for (const directory of readdirSync(root).filter((name) => /^Test\d+$/.test(name))) {
    for (const file of readdirSync(new URL(`${directory}/`, root)).filter((name) => /^scenario.*\.json$/.test(name))) {
      const scenario = JSON.parse(readFileSync(new URL(`${directory}/${file}`, root), 'utf8'));
      const declared = Object.values(scenario.secrets ?? {});
      if (declared.length < 2) continue;
      scenarios++;
      for (const secret of declared) {
        for (const role of ['sender', 'receiver', 'show', 'creator', 'redistribution-check']) {
          const environment = createStepEnvironment({ role, cycle: 25, step: 1, secretName: secret.name });
          const result = bindIosStepSecret({ secretName: secret.name, environment,
            defaultSecret: declared[0], valueForName: (name) => declared.find((entry) => entry.name === name)?.value });
          assert.equal(result.name, secret.name);
          assert.equal(result.value, secret.value ?? '');
          bindings++;
        }
      }
    }
  }
  assert.ok(scenarios >= 10, 'cover the registered multi-secret scenario matrix');
  assert.ok(bindings >= 100);
});

test('registered shared multi-secret factories pass an explicit selection to the step environment', () => {
  const source = readFileSync(new URL('../Test4/orchestrator.mjs', import.meta.url), 'utf8');
  let audited = 0;
  for (const match of source.matchAll(/stepEnvironment\(\{([\s\S]*?)\}\)/g)) {
    const prefix = source.slice(0, match.index);
    const factory = [...prefix.matchAll(/async function (\w+)/g)].at(-1)?.[1] ?? '';
    if (!/MultiSecret|SecondaryCreator|DistributedCreator/.test(factory)) continue;
    assert.match(match[1], /\bsecretName\b/, `${factory} must carry the selected Secret into its actor environment`);
    audited++;
  }
  assert.ok(audited >= 24);
});
