/** Component render evidence only: synthetic inputs; never Core UI E2E. */
import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { createServer as createTcpServer } from 'node:net';
import { readFile, mkdir, writeFile } from 'node:fs/promises';
import { resolve, dirname, extname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { probeMotion } from './motion-probes.mjs';
import { probeAffected } from './affected-probes.mjs';
const here = dirname(fileURLToPath(import.meta.url));
const ui = resolve(here, '../../..');
const workspace = resolve(ui, '../../../..');
const output = resolve(
  workspace,
  process.env.METASECRET_RENDER_OUTPUT ?? 'meta-secret-core/.ai/artifacts/run/MS-20261019210543-005c-web-render',
);
const actualBase = 'http://127.0.0.1:5181/tests/render/preflight/';
const referenceBase = 'http://127.0.0.1:5182/UxRevolution/prototypes/liveboard-device-tiles/';
const evidence = {
  kind: 'synthetic component render, not Core UI E2E',
  started: new Date().toISOString(),
  pid: process.pid,
  ports: [5181, 5182],
  states: [],
  errors: [],
  cleanup: {},
};
await mkdir(output, { recursive: true });
async function free(port) {
  const check = createTcpServer();
  await new Promise((yes, no) => check.once('error', no).listen(port, '127.0.0.1', yes));
  await new Promise((yes) => check.close(yes));
}
await free(5181);
await free(5182);
const mime = {
  '.html': 'text/html',
  '.css': 'text/css',
  '.js': 'text/javascript',
  '.svg': 'image/svg+xml',
  '.otf': 'font/otf',
};
const reference = createServer(async (request, response) => {
  const path = decodeURIComponent(new URL(request.url, referenceBase).pathname);
  if (
    !/^\/(UxRevolution\/prototypes\/(liveboard-device-tiles|liveboard-one-device)\/|design-system\/(assets\/fonts|brand)\/)/.test(
      path,
    ) ||
    path.includes('..')
  ) {
    response.writeHead(404).end();
    return;
  }
  try {
    const target = resolve(workspace, `.${path}${path.endsWith('/') ? 'index.html' : ''}`);
    response
      .writeHead(200, { 'Content-Type': mime[extname(target)] ?? 'application/octet-stream' })
      .end(await readFile(target));
  } catch {
    response.writeHead(404).end();
  }
});
await new Promise((yes) => reference.listen(5182, '127.0.0.1', yes));
const vite = spawn(
  process.execPath,
  [resolve(ui, 'node_modules/vite/bin/vite.js'), '--config', resolve(here, 'vite.config.ts')],
  { cwd: ui, stdio: ['ignore', 'pipe', 'pipe'] },
);
evidence.vitePid = vite.pid;
let viteLog = '';
vite.stdout.on('data', (chunk) => {
  viteLog += chunk;
});
vite.stderr.on('data', (chunk) => {
  viteLog += chunk;
});
let browser;
const selectors = [
  '.app-header',
  '.board',
  '.tile-vault',
  '.vault-heading',
  '.add-device-trigger',
  '.device-slots',
  '.device-art',
  '.storage-info',
  '.tile-secrets',
  '.tile-create',
  '.tile-notifications',
  '.notification-icon',
  '.notification-copy',
  '.notification-title',
  '.notification-detail',
  '.notification-actions',
  '.tile-profile',
  '.secrets-header',
  '.secrets-search',
  '.secret-grid',
  '.secret-card',
  '.secret-name',
  '.secret-payload',
  '.secret-card-actions',
  '.create-dialog',
  '.create-mode-switch',
  '.create-form',
  '.create-fields',
  '.create-seed-grid',
  '.create-actions',
  '.invite-popover',
  '.invite-code',
  '.profile-header',
  '.profile-grid',
];
async function ready(page) {
  await page.waitForSelector('.app');
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(850);
}
async function metrics(page) {
  return page.evaluate((selectors) => {
    const root = document.querySelector('.app').getBoundingClientRect();
    const rect = (el) => {
      const r = el.getBoundingClientRect(),
        s = getComputedStyle(el);
      return {
        x: r.x - root.x,
        y: r.y - root.y,
        width: r.width,
        height: r.height,
        font: s.font,
        gap: s.gap,
        color: s.color,
        filter: s.filter,
        backdropFilter: s.backdropFilter,
        display: s.display,
      };
    };
    return {
      viewport: { width: innerWidth, height: innerHeight },
      app: root.toJSON(),
      fontReady: document.fonts.check('16px Manrope'),
      overflowX: document.documentElement.scrollWidth > innerWidth,
      selectors: Object.fromEntries(
        selectors.map((key) => [
          key,
          [...document.querySelectorAll(key)]
            .filter((el) => el.getBoundingClientRect().width && el.getBoundingClientRect().height)
            .map(rect),
        ]),
      ),
    };
  }, selectors);
}
async function save(page, name, root = '.app') {
  await page.mouse.move(0, 0);
  await page.locator(root).screenshot({ path: resolve(output, `${name}.png`), animations: 'disabled' });
  return await metrics(page);
}
async function visit(page, base, query = '') {
  await page.goto(`${base}${query}`);
  await ready(page);
}
try {
  for (let i = 0; i < 50; i++) {
    if (vite.exitCode !== null) throw new Error(`Owned Vite exited ${vite.exitCode}`);
    try {
      if ((await fetch(actualBase)).ok) break;
    } catch {}
    await new Promise((yes) => setTimeout(yes, 100));
  }
  browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ viewport: { width: 1280, height: 900 }, deviceScaleFactor: 1 });
  const actual = await context.newPage(),
    prototype = await context.newPage();
  for (const [surface, page] of [
    ['actual', actual],
    ['prototype', prototype],
  ])
    page.on('pageerror', (error) => evidence.errors.push({ surface, message: error.message }));
  if (process.env.METASECRET_AFFECTED_ONLY !== '1') {
    // The prototype has an editor frame outside the product. Normalize only the
    // actual viewport to its measured product-container width; neither DOM/CSS changes.
    for (const width of [390, 768, 1280, 1440]) {
      await prototype.setViewportSize({ width, height: 1000 });
      await visit(prototype, referenceBase);
      const contentWidth = await prototype.locator('.viewport').evaluate((el) => el.getBoundingClientRect().width);
      await actual.setViewportSize({ width: contentWidth, height: 1000 });
      await visit(actual, actualBase);
      evidence.states.push({
        scene: `home-content-${width}`,
        normalization: { prototypeOuter: width, productContainer: contentWidth },
        reference: await save(prototype, `reference-home-${width}`),
        actual: await save(actual, `actual-home-${width}`),
      });
    }
    await actual.setViewportSize({ width: 1280, height: 900 });
    await prototype.setViewportSize({ width: 1280, height: 900 });
    for (const [name, query, refQuery] of [
      ['home-raw', '', ''],
      ['notification', '?notice=1', '?join=incoming'],
      ['error', '?notice=1&error=1', '?join=incoming'],
      ['secrets', '?scene=secrets', '?screen=secrets&presentation=web'],
      ['profile', '?scene=profile', '?screen=profile'],
    ]) {
      await visit(actual, actualBase, query);
      await visit(prototype, referenceBase, refQuery);
      if (name === 'error') {
        await prototype.locator('#simulate-error').click();
        await prototype.mouse.move(0, 0);
        await prototype.waitForTimeout(400);
      }
      evidence.states.push({
        scene: name,
        reference: await save(prototype, `reference-${name}`),
        actual: await save(actual, `actual-${name}`),
      });
      if (name === 'notification') {
        await prototype.locator('#notification-hit').click();
        await actual.locator('.notification-hit').click();
        await actual.waitForTimeout(400);
        await prototype.waitForTimeout(400);
        evidence.states.push({
          scene: 'notification-actions',
          reference: await save(prototype, 'reference-notification-actions'),
          actual: await save(actual, 'actual-notification-actions'),
        });
      }
      if (name === 'secrets') {
        await prototype.locator('[data-secret-id="apple"] [data-secret-action="show"]').click();
        await actual.getByTestId('secret-primary-action-Apple ID').click();
        await actual.waitForTimeout(900);
        await prototype.waitForTimeout(900);
        evidence.states.push({
          scene: 'secrets-revealed',
          reference: await save(prototype, 'reference-secrets-revealed'),
          actual: await save(actual, 'actual-secrets-revealed'),
        });
        await actual.getByTestId('secret-primary-action-Apple ID').click();
        evidence.hideStart = await actual.locator('[data-secret-id="render-0"]').evaluate((el) => ({
          text: el.textContent,
          copy: Boolean(el.querySelector('[data-secret-action="copy"]')),
          particles: el.querySelectorAll('.cipher-morph-particle').length,
        }));
        await actual.waitForTimeout(800);
        evidence.hideEnd = await actual.locator('[data-secret-id="render-0"]').evaluate((el) => ({
          text: el.textContent,
          copy: Boolean(el.querySelector('[data-secret-action="copy"]')),
          particles: el.querySelectorAll('.cipher-morph-particle').length,
        }));
      }
    }
    for (const [name, refQuery] of [
      ['create-secret', '?create=secret'],
      ['create-12', '?create=seed'],
      ['create-24', '?create=seed&words=24'],
    ]) {
      await visit(actual, actualBase, '?scene=create');
      await visit(prototype, referenceBase, refQuery);
      if (name !== 'create-secret') await actual.getByRole('button', { name: 'Seed phrase', exact: true }).click();
      if (name === 'create-24') await actual.getByRole('button', { name: '24', exact: true }).click();
      await actual.waitForTimeout(400);
      evidence.states.push({
        scene: name,
        reference: await save(prototype, `reference-${name}`, '.create-dialog'),
        actual: await save(actual, `actual-${name}`, '.create-dialog'),
      });
    }
    await visit(actual, actualBase);
    await visit(prototype, referenceBase);
    // Diagnostic-only compatibility setup approved by root. Frozen source has a
    // missing DOM collection, not a missing visual/domain result. Preserve the
    // original unmodified ReferenceError evidence from the first capture.
    evidence.referenceQrCompatibility = await prototype.evaluate(() => {
      window.tiles = [...document.querySelectorAll('[data-tile]')];
      return { setup: 'window.tiles=[...document.querySelectorAll("[data-tile]")]', count: window.tiles.length };
    });
    await actual.getByTestId('add-device').click();
    await prototype.locator('#add-device').click();
    await actual.waitForTimeout(850);
    await prototype.waitForTimeout(850);
    evidence.states.push({
      scene: 'qr',
      reference: await save(prototype, 'reference-qr-diagnostic-dom-shim', 'body'),
      actual: await save(actual, 'actual-qr', 'body'),
    });
    evidence.qrFocus = await actual.evaluate(() => ({
      focused: document.activeElement?.outerHTML,
      dialogOpen: document.querySelector('.invite-popover')?.open,
    }));
    await actual.keyboard.press('Escape');
    await actual.waitForTimeout(600);
    evidence.qrFocusAfterEscape = await actual.evaluate(() => ({
      focused: document.activeElement?.outerHTML,
      dialogOpen: document.querySelector('.invite-popover')?.open,
    }));
    await actual.emulateMedia({ reducedMotion: 'reduce' });
    await visit(actual, actualBase, '?notice=1');
    await actual.locator('.notification-hit').click();
    evidence.reduced = {
      metrics: await metrics(actual),
      animations: await actual.evaluate(() =>
        document
          .getAnimations()
          .filter((a) => a.playState === 'running')
          .map((a) => ({ name: a.animationName ?? null, duration: a.effect?.getTiming().duration })),
      ),
    };
    await actual.screenshot({ path: resolve(output, 'actual-reduced-actions.png'), fullPage: true });
    await probeMotion({ actual, output, actualBase, evidence });
  }
  if (process.env.METASECRET_AFFECTED_REVIEW === '1') await probeAffected({ actual, output, actualBase, evidence });
  await context.close();
} catch (error) {
  evidence.errors.push(error.stack ?? String(error));
  process.exitCode = 1;
} finally {
  if (browser) await browser.close();
  vite.kill('SIGTERM');
  await new Promise((yes) => (vite.exitCode !== null ? yes() : vite.once('exit', yes)));
  await new Promise((yes) => reference.close(yes));
  evidence.cleanup = {
    browserClosed: true,
    ownedViteExited: vite.exitCode,
    ownedReferenceClosed: !reference.listening,
    ended: new Date().toISOString(),
  };
  await writeFile(resolve(output, 'evidence.json'), JSON.stringify(evidence, null, 2));
  await writeFile(resolve(output, 'vite.log'), viteLog);
  console.log(
    JSON.stringify({ output, states: evidence.states.length, errors: evidence.errors, cleanup: evidence.cleanup }),
  );
}
