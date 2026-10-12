import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { buildDeletionPlan, validateDeletionPlan } from './plan.mjs';
import { deletionMarkerLedger } from './markers.mjs';

test('full matrix covers every platform policy and role exactly three repetitions per variant', () => {
  const plan = buildDeletionPlan('f1');
  assert.equal(validateDeletionPlan(plan).cycles, 60);
  for (const block of plan.blocks) for (const [index] of block.variants.entries()) {
    assert.deepEqual(block.cycles.filter((cycle) => cycle.variant === index + 1).map((cycle) => cycle.repetition), [1, 2, 3]);
  }
  const broken = structuredClone(plan); broken.blocks.pop();
  assert.throws(() => validateDeletionPlan(broken));
});

test('marker ledger has one scoped producer and explicit consumer for every action', () => {
  const { ledger, produced, consumed, diagnostics } = deletionMarkerLedger(buildDeletionPlan('f1'));
  assert.equal(produced, 621); assert.equal(consumed, 541); assert.equal(diagnostics, 80);
  assert.equal(new Set(ledger.map(({ key }) => key)).size, produced);
  assert(ledger.every(({ producer, consumer }) => producer && consumer));
  const passive = ledger.filter(({ producer }) => producer.includes('cachedAppState'));
  assert(passive.every(({ consumer }) => consumer.includes('diagnostic') || consumer.includes('identity metadata')));
});

test('all orchestrated native class/method names exist and agreed marker producers match', () => {
  const android = readFileSync(new URL('../../../meta-secret-compose/composeApp/src/androidTest/kotlin/metasecret/project/com/CaseEightAndroidBothOfflineTest.kt', import.meta.url), 'utf8');
  const ios = readFileSync(new URL('../../../meta-secret-compose/iosApp/iosAppUITests/CaseEightIosBothOfflineUITest.swift', import.meta.url), 'utf8');
  for (const [source, keyword, className] of [[android, 'fun', 'CaseEightAndroidBothOfflineTest'], [ios, 'func', 'CaseEightIosBothOfflineUITest']]) {
    assert(source.includes(`class ${className}`));
    for (const method of ['handleSecretDeletionStep', 'createVaultForTest50', 'createSecretForTest50']) assert(new RegExp(`${keyword} ${method}\\(`).test(source), `Missing ${className}.${method}`);
  }
  assert(android.includes('ANDROID_TEST50_SECRET_CREATED_'));
  assert(ios.includes('IOS_TEST50_SECRET_CREATED_'));
  assert(ios.includes('let expectRequest: Bool?'));
});
