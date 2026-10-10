import assert from 'node:assert/strict';
import test from 'node:test';
import { recycleWebPage, webPageRefreshCycles } from './web-page-recycle.mjs';

test('declared later Secret creation has a completed recycle boundary before creation', () => {
  assert.deepEqual(webPageRefreshCycles({ createSecret3AfterCycle: 18 }), [18, 21, 30]);
  assert.deepEqual(webPageRefreshCycles({ createSecret3AfterCycle: 30 }), [21, 30]);
});

test('initial Secret creation and absent lifecycle mutations retain established boundaries', () => {
  assert.deepEqual(webPageRefreshCycles({ createSecret3AfterCycle: 0 }), [21, 30]);
  assert.deepEqual(webPageRefreshCycles(), [21, 30]);
});

test('completed membership/recovery phases merge with established mutation boundaries', () => {
  assert.deepEqual(webPageRefreshCycles({ webPageRefreshAfterCycles: [6, 15, 24] }), [6, 15, 21, 24, 30]);
  assert.deepEqual(webPageRefreshCycles({ webPageRefreshAfterCycles: [9, 18] }), [9, 18, 21, 30]);
  assert.deepEqual(webPageRefreshCycles({ webPageRefreshAfterCycles: [18, 21, 18], createSecret3AfterCycle: 18 }), [18, 21, 30]);
  for (const invalid of [[0], [-1], [1.5], ['9'], '9']) {
    assert.throws(() => webPageRefreshCycles({ webPageRefreshAfterCycles: invalid }), /positive completed cycle/);
  }
});

test('fresh page retains context and moves passkey before old target closes at readiness', async () => {
  const events = [];
  const authenticator = { authenticatorId: 'same-device-passkey-new-target' };
  const context = { storage: { device: 'same-web-device' }, async newPage() { events.push('new'); return next; } };
  const old = { context: () => context, async close() { events.push('close-old'); } };
  const next = { context: () => context, async close() { assert.fail('ready replacement must stay open'); } };
  const step = (name) => async (page) => {
    assert.equal(page, next);
    assert.equal(page.context(), context);
    events.push(name);
  };
  const result = await recycleWebPage({
    page: old,
    configurePage: step('configure'), navigatePage: step('navigate'),
    cloneAuthenticator: async (page) => { await step('clone-passkey')(page); return authenticator; },
    unlockPage: step('unlock'), waitUntilReady: step('assert-secrets'),
  });
  assert.deepEqual(events, ['new', 'configure', 'navigate', 'clone-passkey', 'unlock', 'assert-secrets', 'close-old']);
  assert.equal(result.page, next);
  assert.equal(result.authenticator, authenticator);
  assert.equal(context.storage.device, 'same-web-device');
});

test('failed replacement readiness retains old target and rejects without switching ownership', async () => {
  let oldClosed = false;
  let nextClosed = false;
  const next = { async close() { nextClosed = true; } };
  const old = { context: () => ({ newPage: async () => next }), async close() { oldClosed = true; } };
  const failure = new Error('expected secret missing');
  await assert.rejects(recycleWebPage({
    page: old, configurePage: async () => {}, navigatePage: async () => {},
    cloneAuthenticator: async () => ({}), unlockPage: async () => {},
    waitUntilReady: async () => { throw failure; },
  }), (error) => error === failure);
  assert.equal(oldClosed, false);
  assert.equal(nextClosed, true);
});
