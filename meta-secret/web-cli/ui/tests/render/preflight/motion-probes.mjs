import { resolve } from 'node:path';

/** Synthetic presentation probes only; does not simulate a successful Core operation. */
export async function probeMotion(options) {
  const { actual, output, actualBase, evidence } = options;
  const load = async (query) => {
    await actual.goto(`${actualBase}${query}`);
    await actual.waitForSelector('.app');
    await actual.evaluate(() => document.fonts.ready);
    await actual.waitForTimeout(800);
  };
  const particles = async () =>
    actual.locator('[data-secret-id="render-0"]').evaluate((el) => ({
      text: el.textContent,
      copy: Boolean(el.querySelector('[data-secret-action="copy"]')),
      circles: el.querySelectorAll('.secret-cipher circle').length,
      transforms: [...el.querySelectorAll('.secret-cipher circle')]
        .slice(0, 3)
        .map((dot) => getComputedStyle(dot).transform),
      animations: el
        .getAnimations({ subtree: true })
        .filter((a) => a.playState === 'running')
        .map((a) => ({
          duration: a.effect?.getTiming().duration,
          delay: a.effect?.getTiming().delay,
        })),
    }));
  await actual.emulateMedia({ reducedMotion: 'no-preference' });
  await actual.setViewportSize({ width: 1280, height: 900 });
  await load('?scene=secrets');
  await actual.getByTestId('secret-primary-action-Apple ID').click();
  await actual.waitForTimeout(160);
  evidence.revealIntermediate = await particles();
  await actual
    .locator('[data-secret-id="render-0"]')
    .screenshot({ path: resolve(output, 'actual-reveal-intermediate.png'), animations: 'allow' });
  await actual.waitForTimeout(900);
  await actual.getByTestId('secret-primary-action-Apple ID').click();
  await actual.waitForTimeout(120);
  evidence.concealIntermediate = await particles();
  await actual
    .locator('[data-secret-id="render-0"]')
    .screenshot({ path: resolve(output, 'actual-conceal-intermediate.png'), animations: 'allow' });
  await actual.waitForTimeout(800);
  evidence.concealExpired = await particles();
  evidence.mapMotion = [];
  for (const phase of ['waiting', 'approved', 'declined']) {
    await load(`?operation=${phase}`);
    const details = await actual.locator('.recovery-links').evaluate((el) => ({
      links: el.querySelectorAll('path').length,
      packets: [...el.querySelectorAll('.recovery-packet')].map((dot) => ({
        fill: getComputedStyle(dot).fill,
        motion: dot.querySelector('animateMotion')?.outerHTML,
        transform: dot.getCTM()?.toString(),
      })),
    }));
    await actual
      .locator('.tile-vault')
      .screenshot({ path: resolve(output, `actual-map-${phase}.png`), animations: 'allow' });
    evidence.mapMotion.push({ phase, ...details });
  }
  await load('');
  await actual.getByTestId('add-device').click();
  await actual.waitForTimeout(110);
  evidence.qrIntermediate = await actual.evaluate(() => {
    const button = document.querySelector('.add-device-trigger').getBoundingClientRect();
    const popup = document.querySelector('.invite-popover').getBoundingClientRect();
    return {
      button: button.toJSON(),
      popup: popup.toJSON(),
      topDelta: popup.top - button.top,
      rightDelta: popup.right - button.right,
    };
  });
  await actual.screenshot({ path: resolve(output, 'actual-qr-intermediate.png'), animations: 'allow' });
  await actual.waitForTimeout(650);
  evidence.qrBackdrop = await actual.evaluate(() => {
    const el = document.querySelector('.invite-popover');
    return {
      popupBlur: getComputedStyle(el).backdropFilter,
      screenBlur: getComputedStyle(el, '::backdrop').backdropFilter,
    };
  });
  await actual.keyboard.press('Escape');
  await actual.waitForTimeout(500);
  evidence.responsive = [];
  for (const size of [
    { width: 320, height: 740 },
    { width: 844, height: 390 },
    { width: 768, height: 1024 },
    { width: 1024, height: 768 },
  ]) {
    await actual.setViewportSize(size);
    await load('?scene=create');
    await actual.getByRole('button', { name: 'Seed phrase', exact: true }).click();
    await actual.getByRole('button', { name: '24', exact: true }).click();
    await actual.waitForTimeout(500);
    const state = await actual.evaluate(() => ({
      overflow: document.documentElement.scrollWidth > innerWidth,
      dialog: document.querySelector('.create-dialog').getBoundingClientRect().toJSON(),
      fields: document.querySelector('.create-fields').getBoundingClientRect().toJSON(),
      focused: document.activeElement?.getAttribute('id'),
      focusable: [...document.querySelector('.create-dialog').querySelectorAll('input,button,textarea')].filter(
        (el) => !el.disabled,
      ).length,
    }));
    await actual.screenshot({
      path: resolve(output, `actual-create24-${size.width}x${size.height}.png`),
      animations: 'disabled',
    });
    evidence.responsive.push({ ...size, ...state });
  }
}
