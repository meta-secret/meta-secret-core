// Root-only controlled browser prerequisite. Not part of F1 or business UI E2E.
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { chromium } from 'playwright';
import { navigateTest51WebEntry } from './web-navigation.mjs';

const held = new Map();
const document = (defer = '') => `<!doctype html><title>Controlled navigation fixture</title>
<button onclick="window.fixtureClicks++">Create Passkey</button>
<script>window.fixtureClicks=0;window.fixtureDcl=false;document.addEventListener('DOMContentLoaded',()=>window.fixtureDcl=true)</script>${defer}`;
const server = createServer((request, response) => {
  const url = new URL(request.url, 'http://127.0.0.1');
  if (url.pathname === '/held.js') { held.set(url.searchParams.get('token'), response); return; }
  if (url.pathname === '/transport') { request.socket.destroy(); return; }
  if (url.pathname === '/redirect') { response.writeHead(302, { Location: '/plain' }); response.end(); return; }
  if (url.pathname === '/favicon.ico') { response.writeHead(204); response.end(); return; }
  response.writeHead(url.pathname === '/500' ? 500 : 200, { 'Content-Type': 'text/html', 'Cache-Control': 'no-store' });
  if (url.pathname === '/empty') response.end('<!doctype html><title>No auth UI</title>');
  else response.end(document(url.pathname === '/fixture' ? `<script defer src="/held.js?token=${url.searchParams.get('token')}"></script>` : ''));
});
const release = (token) => { const response = held.get(token); assert(response, 'Controlled defer request was not observed'); response.end('window.fixtureDeferredFinished=true'); held.delete(token); };
let browser;
let deadline;
let stage = 'loopback fixture';
let deniedRequests = 0;
const proof = { uiE2E: false, fixtureOnly: true };
try {
  await new Promise((done, fail) => { server.once('error', fail); server.listen(0, '127.0.0.1', done); });
  const origin = `http://127.0.0.1:${server.address().port}`;
  browser = await chromium.launch({ headless: true });
  deadline = setTimeout(() => { void browser.close(); }, 45_000);
  const context = await browser.newContext({ serviceWorkers: 'block' });
  await context.route('**/*', async (route) => {
    if (new URL(route.request().url()).origin === origin) await route.continue();
    else { deniedRequests += 1; await route.abort('blockedbyclient'); }
  });
  stage = 'legacy RED with rendered UI and held DOMContentLoaded';
  const legacy = await context.newPage();
  await assert.rejects(legacy.goto(`${origin}/fixture?token=red`, { waitUntil: 'domcontentloaded', timeout: 2000 }), /Timeout/);
  await legacy.getByRole('button', { name: 'Create Passkey', exact: true }).waitFor({ state: 'visible', timeout: 2000 });
  assert(held.has('red'));
  assert.equal(await legacy.evaluate(() => window.fixtureDcl), false);
  proof.legacyDclTimedOutWithVisibleUi = true;
  release('red'); await legacy.waitForLoadState('domcontentloaded', { timeout: 2000 }); await legacy.close();

  stage = 'helper GREEN before releasing controlled defer';
  const ready = await context.newPage();
  await navigateTest51WebEntry(ready, `${origin}/fixture?token=green`, { timeout: 2000 });
  assert(held.has('green'));
  assert.equal(await ready.getByRole('button', { name: 'Create Passkey', exact: true }).isVisible(), true);
  assert.equal(await ready.evaluate(() => window.fixtureDcl), false);
  assert.equal(await ready.evaluate(() => window.fixtureClicks), 0);
  proof.helperReadyWithVisibleUiBeforeDcl = true;
  proof.authActions = 0;
  release('green'); await ready.waitForLoadState('domcontentloaded', { timeout: 2000 }); await ready.close();

  for (const [path, expected] of [['/500', /HTTP/], ['/empty', /Timeout/], ['/redirect', /URL changed/], ['/transport', /net::|Timeout/]]) {
    stage = `negative prerequisite ${path}`;
    const page = await context.newPage();
    await assert.rejects(navigateTest51WebEntry(page, `${origin}${path}`, { timeout: 2000 }), expected);
    await page.close();
  }
  assert.equal(deniedRequests, 0);
  proof.negativeHttpWithUi = true; proof.negativeMissingUi = true;
  proof.negativeWrongDocument = true; proof.negativeTransport = true;
  console.log(JSON.stringify({ status: 'passed', deniedRequests, ...proof }, null, 2));
} catch (error) {
  console.error(JSON.stringify({ status: 'failed', stage, error: error.name, detail: error.message.slice(0, 800), ...proof }));
  process.exitCode = 1;
} finally {
  clearTimeout(deadline);
  for (const response of held.values()) response.destroy();
  await browser?.close();
  server.closeAllConnections();
  await new Promise((done) => server.close(done));
}
