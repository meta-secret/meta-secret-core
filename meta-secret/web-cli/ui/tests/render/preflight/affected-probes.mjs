import assert from 'node:assert/strict';
import { resolve } from 'node:path';

/** Independent synthetic component inputs only; never Core business success. */
export async function probeAffected({ actual, output, actualBase, evidence }) {
  const load = async (query) => {
    await actual.goto(`${actualBase}${query}`);
    await actual.waitForSelector('.app');
    await actual.evaluate(() => document.fonts.ready);
    await actual.waitForTimeout(800);
  };
  const snapshot = () =>
    actual.locator('[data-secret-id="render-0"]').evaluate((el) => {
      const svg = el.querySelector('.secret-cipher');
      const circles = [...svg.querySelectorAll('circle')];
      return {
        plain: Boolean(el.querySelector('.secret-value')),
        copy: Boolean(el.querySelector('[data-secret-action="copy"]')),
        viewBox: svg.getAttribute('viewBox'),
        circles: circles.length,
        inverseEffects: circles
          .flatMap((dot) => dot.getAnimations())
          .filter((effect) => effect.playState === 'running' && effect.effect.getTiming().duration === 640).length,
        anyCircleEffects: circles.flatMap((dot) => dot.getAnimations()).length,
        transformed: circles.filter((dot) => getComputedStyle(dot).transform !== 'none').length,
      };
    });
  await actual.emulateMedia({ reducedMotion: 'no-preference' });
  await actual.setViewportSize({ width: 1280, height: 900 });
  await load('?scene=create&probe=1');
  await actual.getByRole('button', { name: 'Seed phrase', exact: true }).click();
  await actual.getByRole('button', { name: '24', exact: true }).click();
  await actual.waitForTimeout(500);
  const create = () =>
    actual.locator('.create-dialog').evaluate((el) => ({
      rect: el.getBoundingClientRect().toJSON(),
      maxHeight: el.style.getPropertyValue('--create-max'),
      width: el.style.width,
      left: el.style.left,
      right: el.style.right,
      top: el.style.top,
      bottom: el.style.bottom,
      innerHeight,
      innerWidth,
      visualHeight: visualViewport.height,
      wordPlaceholders: [...el.querySelectorAll('.create-seed-grid input')].map((input) => input.placeholder),
    }));
  evidence.affected = { createDesktop: await create() };
  assert.equal(evidence.affected.createDesktop.rect.height, 868);
  assert.equal(evidence.affected.createDesktop.maxHeight, '868px');
  assert.equal(evidence.affected.createDesktop.top, '');
  assert.equal(evidence.affected.createDesktop.bottom, '');
  assert(evidence.affected.createDesktop.wordPlaceholders.every((word) => word === 'Word'));
  await actual.locator('.create-dialog').screenshot({ path: resolve(output, 'affected-create24-868.png') });
  const cdp = await actual.context().newCDPSession(actual);
  try {
    // Real browser visualViewport resize via pinch-zoom emulation, not a fake keyboard.
    await cdp.send('Emulation.setPageScaleFactor', { pageScaleFactor: 2 });
    await actual.waitForFunction(() => visualViewport.height < innerHeight);
    await actual.waitForFunction(() => {
      const el = document.querySelector('.create-dialog');
      return parseFloat(el.style.getPropertyValue('--create-max')) === Math.max(210, visualViewport.height - 32);
    });
    evidence.affected.createVisualViewport = await create();
  } finally {
    await cdp.send('Emulation.setPageScaleFactor', { pageScaleFactor: 1 });
    await cdp.detach();
  }
  await actual.evaluate(() => {
    const frame = document.querySelector('.ms-liveboard');
    frame.style.width = '390px';
    frame.style.marginLeft = '100px';
    window.dispatchEvent(new Event('resize'));
  });
  await actual.waitForTimeout(400);
  evidence.affected.createEmbedded = await create();
  assert.equal(evidence.affected.createEmbedded.width, '366px');
  assert.equal(evidence.affected.createEmbedded.left, '112px');
  assert.equal(evidence.affected.createEmbedded.right, 'auto');
  await actual.screenshot({ path: resolve(output, 'affected-create24-embedded.png') });
  await actual.setViewportSize({ width: 390, height: 844 });
  await actual.waitForTimeout(400);
  evidence.affected.createWindowResize = await create();
  assert.equal(evidence.affected.createWindowResize.width, '358px');
  assert.equal(evidence.affected.createWindowResize.left, '0px');
  assert.equal(evidence.affected.createWindowResize.right, '0px');

  await actual.setViewportSize({ width: 1280, height: 900 });
  await load('?operation=waiting&kind=remove-device&probe=1');
  evidence.affected.pendingRemove = await actual.locator('.tile-vault').evaluate((el) => ({
    request: Boolean(el.querySelector('.recovery-links')),
    panel: Boolean(el.querySelector('.vault-recovery')),
    controls: [...el.querySelectorAll('.vault-storage-actions button')].map((button) => ({
      text: button.textContent.trim(),
      disabled: button.disabled,
      busy: button.getAttribute('aria-busy'),
      icon: button.querySelector('svg')?.dataset.icon,
    })),
  }));
  assert.equal(evidence.affected.pendingRemove.request, true);
  assert.equal(evidence.affected.pendingRemove.panel, true);
  assert.deepEqual(
    evidence.affected.pendingRemove.controls.map((item) => [item.text, item.disabled]),
    [
      ['Current', true],
      ['Remove', true],
    ],
  );
  assert.equal(evidence.affected.pendingRemove.controls[1].busy, 'true');
  assert.equal(evidence.affected.pendingRemove.controls[1].icon, '1-9');
  await actual.locator('.tile-vault').screenshot({ path: resolve(output, 'affected-remove-pending.png') });
  await load('?probe=1');
  await actual.getByTestId('add-device').click();
  await actual.waitForSelector('.invite-code svg');
  await actual.evaluate(() => window.__liveboardRenderInputs.clearQrName());
  await actual.waitForFunction(() => !document.querySelector('.invite-code svg'));
  evidence.affected.invalidQr = await actual.locator('.invite-code').evaluate((el) => ({
    svg: Boolean(el.querySelector('svg')),
    ariaLabel: el.getAttribute('aria-label'),
  }));
  await actual.keyboard.press('Escape');
  await load('?scene=secrets&probe=1');
  const show = () => actual.getByTestId('secret-primary-action-Apple ID').click();
  await show();
  await actual.waitForTimeout(850);
  await show();
  await actual.waitForTimeout(100);
  evidence.affected.hideIntermediate = await snapshot();
  assert.equal(evidence.affected.hideIntermediate.plain, false);
  assert.equal(evidence.affected.hideIntermediate.copy, false);
  assert.equal(evidence.affected.hideIntermediate.circles, 135);
  assert.equal(evidence.affected.hideIntermediate.inverseEffects, 135);
  await actual.locator('[data-secret-id="render-0"]').screenshot({
    path: resolve(output, 'affected-hide-inverse.png'),
    animations: 'allow',
  });
  await actual.waitForTimeout(750);
  evidence.affected.hideExpired = await snapshot();
  assert.equal(evidence.affected.hideExpired.viewBox, '0 0 300 28');
  assert.equal(evidence.affected.hideExpired.anyCircleEffects, 0);

  await show();
  await actual.waitForTimeout(850);
  await show();
  await actual.evaluate(() => {
    for (const effect of document.querySelector('.secret-cipher').getAnimations({ subtree: true })) effect.pause();
  });
  await actual.waitForTimeout(750);
  evidence.affected.pausedConcealDeadline = await snapshot();
  assert.equal(evidence.affected.pausedConcealDeadline.viewBox, '0 0 300 28');
  assert.equal(evidence.affected.pausedConcealDeadline.anyCircleEffects, 0);
  evidence.affected.privacy = [];
  for (const barrier of ['invalidate', 'filter', 'tombstone', 'pagehide']) {
    await load('?scene=secrets&probe=1');
    await show();
    await actual.waitForTimeout(850);
    await show();
    await actual.waitForTimeout(50);
    await actual.evaluate((barrier) => {
      if (barrier === 'pagehide') window.dispatchEvent(new Event('pagehide'));
      else window.__liveboardRenderInputs[barrier]();
    }, barrier);
    await actual.waitForTimeout(25);
    if (barrier === 'tombstone' || barrier === 'filter') {
      assert.equal(await actual.locator('[data-secret-id="render-0"]').count(), 0);
      evidence.affected.privacy.push({ barrier, cardRemoved: true });
    } else {
      const state = await snapshot();
      assert.equal(state.plain, false);
      assert.equal(state.copy, false);
      assert.equal(state.viewBox, '0 0 300 28');
      assert.equal(state.anyCircleEffects, 0);
      evidence.affected.privacy.push({ barrier, ...state });
    }
  }
}
