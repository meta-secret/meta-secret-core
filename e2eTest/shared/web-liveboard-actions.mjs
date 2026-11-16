const TIMEOUT = 120_000;
const exact = (value) => new RegExp(`^${value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}$`);
const attr = (value) => JSON.stringify(value).replace(/\\n/g, '\\a ').replace(/\\r/g, '\\d ');

export function exactIdentity(rows, expected) {
  const found = rows.filter((row) => (!expected.id || row.id === expected.id) && (!expected.name || row.name === expected.name));
  if (found.length !== 1 || !found[0].id || !found[0].name) throw new Error('Expected one unique canonical UI identity');
  return found[0];
}

export function secretValueMatches(observed, expected) {
  if (observed.plain !== null) return observed.plain === expected;
  return observed.words.length > 0 && observed.words.join(' ') === expected;
}

export async function openWebLiveboard(page, timeout = TIMEOUT, budget) {
  const deadline = timeout === 0 ? null : Date.now() + timeout;
  const remaining = () => {
    if (timeout === 0) return 0;
    const milliseconds = deadline - Date.now();
    if (!Number.isFinite(milliseconds) || milliseconds <= 0) throw new Error('Timed out waiting for Web Liveboard readiness');
    return milliseconds;
  };
  const perform = async operation => budget ? budget.perform(operation) : operation(remaining());
  const home = page.getByTestId('liveboard-secrets-open');
  if (!await perform(() => home.isVisible())) {
    const back = page.getByRole('link', { name: 'Back to Liveboard', exact: true });
    await perform(timeout => home.or(back).filter({ visible: true }).first().waitFor({ state: 'visible', timeout }));
    if (!await perform(() => home.isVisible()) && await perform(() => back.isVisible())) await (budget ? budget.click(back) : back.click({ timeout: remaining() }));
  }
  await perform(timeout => home.waitFor({ state: 'visible', timeout }));
}

export async function openWebSecrets(page, timeout = TIMEOUT) {
  if (!await page.locator('.secrets-screen').isVisible().catch(() => false)) {
    await openWebLiveboard(page, timeout);
    await page.getByTestId('liveboard-secrets-open').click();
  }
  await page.locator('.secrets-screen').waitFor({ state: 'visible', timeout });
}

export async function webSecretCard(page, name, { id, timeout = TIMEOUT } = {}) {
  await openWebSecrets(page, timeout);
  const byName = page.locator('article.secret-card').filter({ has: page.locator('.secret-name').filter({ hasText: exact(name) }) });
  await byName.first().waitFor({ state: 'visible', timeout });
  const rows = await byName.evaluateAll((nodes) => nodes.map((node) => ({ id: node.dataset.secretId, name: node.querySelector('.secret-name')?.textContent })));
  const selected = exactIdentity(rows, { name, id });
  return { id: selected.id, card: page.locator(`article.secret-card[data-secret-id=${attr(selected.id)}]`) };
}

export async function waitWebCardAction(page, name, label, { id, timeout = TIMEOUT } = {}) {
  const selected = await webSecretCard(page, name, { id, timeout });
  await selected.card.getByTestId(`secret-primary-action-${name}`).filter({ hasText: exact(label) }).waitFor({ state: 'visible', timeout });
  await page.waitForFunction(({ id, label }) => {
    const card = [...document.querySelectorAll('article.secret-card')].find((node) => node.dataset.secretId === id);
    const action = card?.querySelector('[data-testid^="secret-primary-action-"]');
    return action && !action.disabled && action.textContent.trim() === label;
  }, { id: selected.id, label }, { timeout });
  return selected;
}

export async function createWebLiveboardSecret(page, { name, value }, timeout = TIMEOUT) {
  await openWebLiveboard(page, timeout);
  await page.getByTestId('create-secret-open').click({ timeout });
  const modal = page.locator('dialog.create-dialog');
  await modal.waitFor({ state: 'visible', timeout });
  await modal.locator('#create-secret-name').fill(name);
  await modal.locator('#create-secret-value').fill(value);
  await modal.getByTestId('create-secret-submit').click();
  await modal.waitFor({ state: 'hidden', timeout });
  return webSecretCard(page, name, { timeout });
}

export async function waitWebSecretValue(page, name, expected, { id, timeout = TIMEOUT, showWhenReady = true } = {}) {
  const selected = await webSecretCard(page, name, { id, timeout });
  // Accepted recovery now exposes an explicit Show action. Waiting itself never reveals.
  const state = await page.waitForFunction(({ id, allowShow }) => {
    const card = [...document.querySelectorAll('article.secret-card')].find((node) => node.dataset.secretId === id);
    if (!card) return false;
    if (card.dataset.phase === 'revealed' && card.querySelector('.secret-value')) return 'revealed';
    const primary = card.querySelector('[data-testid^="secret-primary-action-"]');
    return allowShow && primary && !primary.disabled && primary.textContent.trim() === 'Show' ? 'show' : false;
  }, { id: selected.id, allowShow: showWhenReady }, { timeout }).then((handle) => handle.jsonValue());
  if (state === 'show') await selected.card.getByTestId(`secret-primary-action-${name}`).click();
  await selected.card.locator('.secret-value').waitFor({ state: 'visible', timeout });
  const observed = await selected.card.evaluate((node) => ({ plain: node.querySelector('.secret-plain')?.textContent ?? null, words: [...node.querySelectorAll('.secret-seed li span')].map((word) => word.textContent) }));
  if (!secretValueMatches(observed, expected)) throw new Error('Exact revealed Secret value mismatch (payload omitted)');
  return selected;
}

export async function hideWebSecret(page, name, expected, options = {}) {
  const selected = await waitWebSecretValue(page, name, expected, { ...options, showWhenReady: false });
  await selected.card.locator('[data-secret-action="hide"]').click();
  await Promise.all([
    selected.card.locator('.secret-value').waitFor({ state: 'hidden', timeout: options.timeout ?? 30_000 }),
    selected.card.locator('[data-secret-action="copy"]').waitFor({ state: 'hidden', timeout: options.timeout ?? 30_000 }),
  ]);
  if (await selected.card.locator('.secret-value').isVisible()) throw new Error('Hide retained disclosed Secret content');
  return selected;
}

export async function startWebLiveboardRecovery(page, name, { duplicate = false, timeout = TIMEOUT } = {}) {
  const selected = await waitWebCardAction(page, name, 'Recover', { timeout });
  const action = selected.card.getByTestId(`secret-primary-action-${name}`);
  await action.click();
  await page.waitForFunction((id) => {
    const card = [...document.querySelectorAll('article.secret-card')].find((node) => node.dataset.secretId === id);
    const action = card?.querySelector('[data-testid^="secret-primary-action-"]');
    return card?.dataset.phase === 'waiting' && action?.disabled;
  }, selected.id, { timeout });
  if (await selected.card.getByTestId(`delete-secret-${name}`).isEnabled()) throw new Error('Pending recovery left Delete enabled');
  if (duplicate) {
    // Attempt only the same actual UI control. A disabled clock is the duplicate guard.
    if (await action.isEnabled()) await action.click({ timeout: 1_000 });
  }
  return selected;
}

export async function assertWebSecretConcealed(page, name, timeout = 30_000) {
  const selected = await webSecretCard(page, name, { timeout });
  await Promise.all([
    selected.card.locator('.secret-value').waitFor({ state: 'hidden', timeout }),
    selected.card.locator('[data-secret-action="copy"]').waitFor({ state: 'hidden', timeout }),
  ]);
  return selected;
}
