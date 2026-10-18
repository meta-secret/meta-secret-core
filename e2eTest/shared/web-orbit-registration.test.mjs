import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import {
  createWebVault,
  requestWebVaultJoin,
  requestRejectedWebVaultJoin,
  retryWebVaultWithNewEmail,
  waitForWebAfterRemoval,
} from './web-orbit-registration.mjs';

function fakePage({ taken = false, outcome = 'pending', step = '1', message = 'This Vault already has 3 devices.' } = {}) {
  const calls = [];
  const pending = [];
  const state = { step, phase: step === '4' ? 'error' : 'choice', name: 'previous@example.com', taken, outcome };
  let started;
  const requestStarted = new Promise((resolve) => { started = resolve; });
  const settle = () => { if (['pending', 'error', 'member'].includes(state.phase)) pending.splice(0).forEach((resolve) => resolve()); };
  const matches = (name) => name === 'outcome' ? ['pending', 'error'].includes(state.phase) : name === 'main-or-error' ? ['member', 'error'].includes(state.phase) : name === 'after-removal' ? (state.step === '1' || (state.step === '4' && state.phase === 'error')) : true;
  function locator(name) {
    return {
      locator: (selector) => locator(selector.includes('pending') ? 'outcome' : selector.includes('choice') ? 'after-removal' : 'scene'),
      getByRole: (role, options = {}) => locator(`${role}:${options.name ?? ''}`),
      getByPlaceholder: (placeholder) => locator(`field:${placeholder}`),
      filter: () => locator('status:availability'),
      or: () => locator(name === 'link:Secrets' ? 'main-or-error' : name === 'outcome' ? 'outcome' : 'availability-or-error'),
      first: () => locator(name),
      async waitFor() {
        calls.push(`wait:${name}`);
        if (!matches(name)) await new Promise((resolve) => pending.push(resolve));
      },
      async getAttribute(attribute) { return attribute === 'data-step' ? state.step : state.phase; },
      async innerText() { return name === 'heading:' ? state.name : name.startsWith('alert:') ? message : state.taken ? 'This name is taken' : 'This name is available'; },
      async isVisible() { return name.startsWith('alert:') && state.phase === 'error'; },
      async fill(value) { calls.push(`fill:${name}`); state.name = value; },
      async click() {
        calls.push(`click:${name}`);
        if (name === 'button:Enter your email' || name === 'button:Enter') state.step = '3';
        else if (name === 'button:OK') { state.step = '2'; state.phase = state.taken ? 'exists' : 'available'; }
        else if (name === 'button:Join') { state.step = '4'; state.phase = 'join'; }
        else if (name === 'button:Back') { state.step = '2'; state.phase = 'exists'; }
        else if (name === 'button:Create') { state.phase = 'member'; settle(); }
        else if (name === 'button:Request') { started(); state.phase = state.outcome === 'defer' ? 'requesting' : state.outcome; settle(); }
      },
    };
  }
  const page = { getByRole: (role, options = {}) => locator(`${role}:${options.name ?? ''}`), locator: (selector) => locator(selector.includes('pending') ? 'outcome' : selector.includes('choice') ? 'after-removal' : 'scene') };
  return { page, calls, state, requestStarted, persistPending() { state.phase = 'pending'; settle(); } };
}

test('Create follows manual03→02 and waits for existing main', async () => {
  const { page, calls } = fakePage();
  await createWebVault(page, 'owner@example.com');
  assert.deepEqual(calls.filter((call) => call.startsWith('click:')), ['click:button:Enter your email', 'click:button:OK', 'click:button:Create']);
  assert.ok(calls.includes('wait:main-or-error'));
  assert.ok(!calls.some((call) => call.includes('Passkey')));
});

test('Join opens04 Request and pre-arms canonical Pending before clicking', async () => {
  const fixture = fakePage({ taken: true });
  await requestWebVaultJoin(fixture.page, 'owner@example.com');
  assert.deepEqual(fixture.calls.filter((call) => call.startsWith('click:')), ['click:button:Enter your email', 'click:button:OK', 'click:button:Join', 'click:button:Request']);
  assert.ok(fixture.calls.indexOf('wait:outcome') < fixture.calls.indexOf('click:button:Request'));
});

test('a Request click alone does not complete the request helper', async () => {
  const fixture = fakePage({ taken: true, outcome: 'defer' });
  let finished = false;
  const request = requestWebVaultJoin(fixture.page, 'owner@example.com').then(() => { finished = true; });
  await fixture.requestStarted;
  assert.ok(fixture.calls.includes('click:button:Request'));
  assert.equal(finished, false);
  fixture.persistPending();
  await request;
  assert.equal(finished, true);
});

test('Create never silently becomes Join when the fixture Vault already exists', async () => {
  const { page, calls } = fakePage({ taken: true });
  await assert.rejects(createWebVault(page, 'owner@example.com'), /available/);
  assert.ok(!calls.includes('click:button:Create'));
  assert.ok(!calls.includes('click:button:Request'));
});

test('fourth-device rejection is explicit, and ordinary Join rejects an error outcome', async () => {
  const rejected = fakePage({ taken: true, outcome: 'error' });
  const alert = await requestRejectedWebVaultJoin(rejected.page, 'owner@example.com');
  assert.match(await alert.innerText(), /3 devices/);
  await assert.rejects(requestWebVaultJoin(fakePage({ taken: true, outcome: 'error' }).page, 'owner@example.com'), /rejected/);
});

test('retry uses Back→Enter→different email, with no reload or direct database action', async () => {
  const fixture = fakePage({ step: '4' });
  await retryWebVaultWithNewEmail(fixture.page, 'new@example.com');
  assert.deepEqual(fixture.calls.filter((call) => call.startsWith('click:')), ['click:button:Back', 'click:button:Enter', 'click:button:OK', 'click:button:Create']);
});

test('own-removal assertion accepts the registration surface without performing a mutation', async () => {
  const fixture = fakePage({ step: '1' });
  await waitForWebAfterRemoval(fixture.page);
  assert.ok(!fixture.calls.some((call) => call.startsWith('click:')));
});

test('all owning scripts use the shared entry rather than legacy registration controls', () => {
  for (const number of [1, 2, 3, 4]) {
    const source = readFileSync(new URL(`../Test${number}/orchestrator.mjs`, import.meta.url), 'utf8');
    assert.ok(source.includes('../shared/web-orbit-registration.mjs'));
    assert.doesNotMatch(source, /getByPlaceholder\('vault name'\)|Set Vault Name|Vault name is free!|Reset & Create New|signup-error/);
  }
});

test('own-removal accepts Core-derived Declined and rejects an unrelated error', async () => {
  await waitForWebAfterRemoval(fakePage({ step: '4', message: 'Request declined.' }).page);
  await assert.rejects(waitForWebAfterRemoval(fakePage({ step: '4', message: 'Network error.' }).page), /canonical declined/);
});

test('reset/retry rejects the same name before taking any new action', async () => {
  const fixture = fakePage({ step: '4' });
  await assert.rejects(retryWebVaultWithNewEmail(fixture.page, 'previous@example.com'), /different fixture email/);
  assert.ok(!fixture.calls.some((call) => call.startsWith('click:')));
});
