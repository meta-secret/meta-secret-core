import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { navigateTest51WebEntry } from './web-navigation.mjs';

function fakePage({ status = 200, pageUrl = 'http://localhost:5174/', responseUrl = pageUrl, response = true,
  visible = true, navigationError } = {}) {
  const calls = [];
  const roles = [];
  const control = {
    or() { return this; }, first() { return this; },
    async waitFor(options) { calls.push(['visible', options]); if (!visible) throw new Error('Actual auth/Orbit UI unavailable'); },
  };
  return {
    calls, roles,
    url: () => pageUrl,
    async goto(url, options) {
      calls.push(['goto', url, options]);
      if (navigationError) throw navigationError;
      return response ? { ok: () => status >= 200 && status < 300, status: () => status, url: () => responseUrl } : null;
    },
    getByRole(role, options) { roles.push([role, options]); return control; },
    click() { assert.fail('Navigation readiness must not create/authenticate a Passkey'); },
    reload() { assert.fail('Navigation helper must not retry/refresh'); },
  };
}

test('navigation commits the normalized exact local document and waits for actual UI without auth actions', async () => {
  const page = fakePage();
  await navigateTest51WebEntry(page, 'http://localhost:5174');
  assert.deepEqual(page.calls, [
    ['goto', 'http://localhost:5174/', { waitUntil: 'commit', timeout: 30_000 }],
    ['visible', { state: 'visible', timeout: 30_000 }],
  ]);
  assert.deepEqual(page.roles, [
    ['button', { name: 'Create Passkey', exact: true }],
    ['button', { name: 'Authenticate with Passkey', exact: true }],
    ['region', { name: 'Vault registration', exact: true }],
  ]);
});

test('null, HTTP error and transport failures cannot become readiness despite visible auth', async () => {
  for (const options of [{ response: false }, { status: 404 }, { status: 500 }, { navigationError: new Error('transport failed') }]) {
    const page = fakePage(options);
    await assert.rejects(navigateTest51WebEntry(page, 'http://localhost:5174/'));
    assert.equal(page.calls.filter(([action]) => action === 'goto').length, 1);
    assert(!page.calls.some(([action]) => action === 'visible'));
  }
});

test('unexpected response/page origin, path, query or hash fail before auth UI interaction', async () => {
  for (const wrong of ['http://127.0.0.1:5174/', 'http://localhost:5174/wrong', 'http://localhost:5174/?wrong=1', 'http://localhost:5174/#wrong']) {
    for (const options of [{ responseUrl: wrong }, { pageUrl: wrong, responseUrl: 'http://localhost:5174/' }]) {
      const page = fakePage(options);
      await assert.rejects(navigateTest51WebEntry(page, 'http://localhost:5174/'));
      assert(!page.calls.some(([action]) => action === 'visible'));
    }
  }
});

test('commit without actual auth/Orbit UI fails and unsupported target is rejected without navigation', async () => {
  await assert.rejects(navigateTest51WebEntry(fakePage({ visible: false }), 'http://localhost:5174/'), /UI unavailable/);
  for (const target of ['https://example.com/', 'file:///tmp/index.html', 'http://user@localhost:5174/', 'http://localhost:5174/#wrong']) {
    const page = fakePage();
    await assert.rejects(navigateTest51WebEntry(page, target));
    assert.equal(page.calls.length, 0);
  }
});

test('only registrationOnly initial navigation changes; historical navigation and Passkey gates remain', () => {
  const source = readFileSync(new URL('../Test4/orchestrator.mjs', import.meta.url), 'utf8');
  const branch = source.slice(source.indexOf('async function preparePrimaryWebClient(page)'), source.indexOf('async function runWebInitiatedSetup'));
  assert(branch.includes('if (registrationOnly) await navigateTest51WebEntry(page, scenario.web.url);'));
  assert(branch.includes("else await page.goto(scenario.web.url, { waitUntil: 'domcontentloaded' });"));
  assert(branch.indexOf('navigateTest51WebEntry') < branch.indexOf('await unlockWithPasskeyIfNeeded(page)'));
  assert(branch.includes('waitForWebEntry(page)'));
});
