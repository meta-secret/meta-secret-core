import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createScopedSecretIdCollector } from './scoped-secret-id.mjs';
const key = 'ANDROID_DELETION_ID_4_run-one-android-recreated-show-android';

test('identity survives 27695 chars of later output and remains exact after actor completion', async () => {
  const collector = createScopedSecretIdCollector();
  collector.push('stdout', `E2E: ${key}:fresh_id-1\n`);
  collector.push('stdout', `${'unrelated log text '.repeat(2000)}\n`);
  collector.close(new Error('actor ended'));
  assert.equal(await collector.wait(key), 'fresh_id-1');
});
test('split marker/payload is captured only when the complete line arrives', async () => {
  const collector = createScopedSecretIdCollector(); let done = false;
  const result = collector.wait(key).then((id) => { done = true; return id; });
  collector.push('stdout', `10-11 19:27:48.333 5116 5132 I MetaSecretE2E: E2E: ${key.slice(0, 18)}`);
  collector.push('stdout', `${key.slice(18)}:fresh_`); await Promise.resolve(); assert.equal(done, false);
  collector.push('stdout', 'id-2\n'); assert.equal(await result, 'fresh_id-2');
});
test('a foreign scope cannot satisfy the consumer; conflicting IDs fail without echoing payloads', async () => {
  const collector = createScopedSecretIdCollector();
  collector.push('stdout', 'E2E: ANDROID_DELETION_ID_4_foreign-scope:foreign_id\n');
  const requested = collector.wait(key); const failure = new Error('actor ended before exact identity'); collector.close(failure);
  await assert.rejects(requested, /exact identity/);
  const other = createScopedSecretIdCollector(); other.push('stdout', `E2E: ${key}:old_id_1\n`);
  assert.throws(() => other.push('stdout', `E2E: ${key}:new_id_2\n`), /Conflicting/);
});

test('late missing-payload waits reject immediately after the owned actor closes', async () => {
  const collector = createScopedSecretIdCollector(); collector.close(new Error('owned actor ended'));
  await assert.rejects(collector.wait(key), /owned actor ended/);
});
