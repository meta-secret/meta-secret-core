import { test } from 'node:test';
import assert from 'node:assert/strict';
import { parseCliEnvelope } from './cli-json.mjs';
test('Core marker prefixes cannot be mistaken for the deletion-requests JSON array', () => {
  assert.deepEqual(parseCliEnvelope('[CORE_DIAG] hidden\n[\n {"requestId":"exact"}\n]\n'), [{ requestId: 'exact' }]);
  assert.deepEqual(parseCliEnvelope('diagnostic\n{"secrets":[]}\n'), { secrets: [] });
  assert.throws(() => parseCliEnvelope('hidden-sensitive-payload'), (error) => !error.message.includes('hidden-sensitive'));
});
