import { test } from 'node:test';
import assert from 'node:assert/strict';
import { exactIdentity, secretValueMatches, hideWebSecret, openWebLiveboard } from './web-liveboard-actions.mjs';
import { validateIncomingHead, validateIncomingRemoval } from './web-liveboard-inbox.mjs';

test('canonical selectors reject aliases, missing and same-name ambiguity; comma remains opaque', () => {
  const rows = [{ id: 'a', name: 'iPhone18,3' }, { id: 'b', name: 'Secret' }];
  assert.deepEqual(exactIdentity(rows, { name: 'iPhone18,3' }), rows[0]);
  assert.throws(() => exactIdentity(rows, { name: 'iPhone18' }), /unique/);
  assert.throws(() => exactIdentity([...rows, { id: 'c', name: 'Secret' }], { name: 'Secret' }), /unique/);
  assert.throws(() => exactIdentity(rows, { id: 'foreign', name: 'Secret' }), /unique/);
});

test('plaintext/seed comparison preserves exact value and rejects absent, extra or reordered words', () => {
  assert.equal(secretValueMatches({ plain: ' exact ', words: [] }, ' exact '), true);
  assert.equal(secretValueMatches({ plain: null, words: ['one', 'two'] }, 'one two'), true);
  assert.equal(secretValueMatches({ plain: null, words: ['two', 'one'] }, 'one two'), false);
  assert.equal(secretValueMatches({ plain: null, words: ['one', 'two', 'extra'] }, 'one two'), false);
  assert.equal(secretValueMatches({ plain: null, words: [] }, ''), false);
});

test('Hide never starts a fresh Show to conceal loss of the original disclosure', async () => {
  let observed;
  const node = { filter() { return this; }, first() { return this; }, async isVisible() { return true; }, async waitFor() {},
    async evaluateAll() { return [{ id: 'opaque', name: 'Secret' }]; } };
  const page = { locator: () => node,
    async waitForFunction(_, args) { observed = args; throw new Error('Original disclosed value unavailable'); } };
  await assert.rejects(hideWebSecret(page, 'Secret', 'expected'), /Original disclosed/);
  assert.equal(observed.allowShow, false);
});

test('FIFO head needs exact kind, ID, subject and queue count; foreign head is never auto-decided', () => {
  const head = { kind: 'recover', requestId: 'uuid-1', requester: 'iPhone18,3', subject: 'Secret', count: 2 };
  assert.deepEqual(validateIncomingHead(head, { kind: 'recover', requestId: 'uuid-1', subject: 'Secret', count: 2 }), head);
  for (const mismatch of [{ kind: 'join' }, { requestId: 'uuid-2' }, { subject: 'Other' }, { count: 1 }]) {
    assert.throws(() => validateIncomingHead(head, { kind: 'recover', subject: 'Secret', ...mismatch }), /FIFO|count|identity/);
  }
  assert.throws(() => validateIncomingHead({ ...head, requester: '' }, { kind: 'recover' }), /identity/);
});

test('busy/hidden action alone is not terminal and a reused ID cannot pass removal', () => {
  const head = { kind: 'recover', requestId: 'uuid-1', count: 2 };
  assert.equal(validateIncomingRemoval(head, { state: 'sending', count: 1, requestId: null }), false);
  assert.equal(validateIncomingRemoval(head, { state: 'actions', count: 1, requestId: 'uuid-1' }), false);
  assert.equal(validateIncomingRemoval(head, { state: 'request', count: 2, requestId: null }), false);
  assert.equal(validateIncomingRemoval(head, { state: 'request', count: 1, requestId: null }), false, 'Collapsed/receipt copy cannot prove which ID survived');
  assert.equal(validateIncomingRemoval(head, { state: 'actions', count: 1, requestId: 'uuid-2' }), true);
  assert.equal(validateIncomingRemoval(head, { state: 'empty', count: 0, requestId: null }), true);
});

function navigationFixture({ home = false, back = false, onWait, onClick, onProbe, lookupError } = {}) {
  const state = { home, back }, waits = [], clicks = [], lookups = [];
  const absent = new Error('controlled readiness timeout'), finalAbsent = new Error('controlled final Home timeout');
  const waitFor = stage => async options => {
    assert.equal(options.state, 'visible'); waits.push({ stage, timeout: options.timeout });
    if (onWait) await onWait(stage, state, options);
    if (stage === 'choice' && !state.home && !state.back) throw absent;
    if (stage === 'home' && !state.home) throw finalAbsent;
  };
  const backLocator = { async isVisible() { if (onProbe) await onProbe('back'); return state.back; },
    async click(options) { clicks.push(options); if (onClick) await onClick(state, options); else { state.home = true; state.back = false; } } };
  const homeLocator = { async isVisible() { if (onProbe) await onProbe('home'); return state.home; },
    waitFor: waitFor('home'),
    or(other) {
      assert.equal(other, backLocator);
      return { filter(options) {
        assert.deepEqual(options, { visible: true });
        return { first: () => ({ waitFor: waitFor('choice') }) };
      } };
    } };
  const page = { getByTestId(id) {
    assert.equal(id, 'liveboard-secrets-open'); lookups.push('home');
    if (lookupError) throw lookupError; return homeLocator;
  }, getByRole(role, options) {
    assert.equal(role, 'link'); assert.deepEqual(options, { name: 'Back to Liveboard', exact: true });
    lookups.push('back'); return backLocator;
  } };
  return { page, state, waits, clicks, lookups, absent, finalAbsent };
}

test('late Home readiness never attempts Back', async () => {
  const f = navigationFixture({ onWait: (_stage, state) => { state.home = true; } });
  await openWebLiveboard(f.page, 120_000);
  assert.equal(f.clicks.length, 0, 'Late Home must not commit navigation to Back');
  assert.deepEqual(f.waits.map(row => row.stage), ['choice', 'home']);
});

test('already Home and simultaneous Home plus Back prefer Home without a Back action', async () => {
  for (const back of [false, true]) {
    const f = navigationFixture({ home: true, back }); await openWebLiveboard(f.page, 1000);
    assert.equal(f.clicks.length, 0); assert.deepEqual(f.lookups, ['home']);
    assert.deepEqual(f.waits.map(row => row.stage), ['home']);
  }
  const both = navigationFixture({ onWait: (_stage, state) => { state.home = true; state.back = true; } });
  await openWebLiveboard(both.page, 1000); assert.equal(both.clicks.length, 0);
});

test('hidden Home with visible exact Back clicks once and still waits for final Home', async () => {
  const f = navigationFixture({ back: true }); await openWebLiveboard(f.page, 1000);
  assert.equal(f.clicks.length, 1); assert.deepEqual(f.lookups, ['home', 'back']);
  assert.deepEqual(f.waits.map(row => row.stage), ['choice', 'home']);
});

test('both routes disappearing after readiness waits for final Home without blind Back', async () => {
  const f = navigationFixture({ back: true, onWait(stage, state) {
    if (stage === 'home') state.home = true;
  }, onProbe(stage) { if (stage === 'back') f.state.back = false; } });
  await openWebLiveboard(f.page, 1000);
  assert.equal(f.clicks.length, 0); assert.deepEqual(f.waits.map(row => row.stage), ['choice', 'home']);
});

test('neither Home nor Back during registration fails within the configured readiness wait', async () => {
  const f = navigationFixture(); await assert.rejects(openWebLiveboard(f.page, 750), error => error === f.absent);
  assert.equal(f.clicks.length, 0); assert.equal(f.waits.length, 1);
  assert(f.waits[0].timeout > 0 && f.waits[0].timeout <= 750);
});

test('one positive deadline spans readiness click and final Home and never passes exhausted zero', async t => {
  t.mock.timers.enable({ apis: ['Date'], now: 1000 });
  const f = navigationFixture({ back: true,
    onProbe: stage => { if (stage === 'home') t.mock.timers.tick(10); },
    onWait: stage => { if (stage === 'choice') t.mock.timers.tick(20); },
    onClick: state => { t.mock.timers.tick(20); state.home = true; } });
  await openWebLiveboard(f.page, 100);
  assert.deepEqual(f.waits.map(row => row.timeout), [90, 40]); assert.equal(f.clicks[0].timeout, 60);
  const exhausted = navigationFixture({ onWait(_stage, state) { t.mock.timers.tick(100); state.home = true; } });
  await assert.rejects(openWebLiveboard(exhausted.page, 100), /Timed out waiting for Web Liveboard readiness/);
  assert.equal(exhausted.waits.length, 1); assert.equal(exhausted.clicks.length, 0);
  assert(exhausted.waits.every(row => row.timeout > 0));
});

test('explicit zero preserves the existing disabled Playwright timeout semantics', async () => {
  const f = navigationFixture({ back: true }); await openWebLiveboard(f.page, 0);
  assert.deepEqual(f.waits.map(row => row.timeout), [0, 0]); assert.equal(f.clicks[0].timeout, 0);
});

test('lookup closed-page wait click and absent final Home errors remain the exact primary', async () => {
  for (const stage of ['lookup', 'probe', 'wait', 'click']) {
    const primary = new Error(`controlled ${stage} failure`);
    const f = navigationFixture({ back: true,
      lookupError: stage === 'lookup' ? primary : undefined,
      onProbe: stage === 'probe' ? () => { throw primary; } : undefined,
      onWait: stage === 'wait' ? () => { throw primary; } : undefined,
      onClick: stage === 'click' ? () => { throw primary; } : undefined });
    await assert.rejects(openWebLiveboard(f.page, 1000), error => error === primary);
  }
  const missing = navigationFixture({ back: true, onClick() {} });
  await assert.rejects(openWebLiveboard(missing.page, 1000), error => error === missing.finalAbsent);
  assert.equal(missing.clicks.length, 1); assert.equal(missing.waits.at(-1).stage, 'home');
});
