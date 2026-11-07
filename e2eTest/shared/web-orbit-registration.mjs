const DEFAULT_TIMEOUT = 120_000;
const region = (page) => page.getByRole('region', { name: 'Vault registration', exact: true });
const secrets = (page) => page.getByTestId('liveboard-secrets-open').or(page.locator('.secrets-screen')).first();
const alert = (page) => region(page).getByRole('alert');
const scene = (page) => page.locator('.orbit-registration .orbit-turn:not([inert])').first();

export const webEntryReady = (page) => region(page).or(secrets(page)).first();
export const waitForWebEntry = (page, { timeout = 30_000 } = {}) =>
  webEntryReady(page).waitFor({ state: 'visible', timeout });

async function settledScene(page, timeout) {
  await scene(page).waitFor({ state: 'visible', timeout });
  return scene(page);
}

export async function prepareWebVaultEmail(page, name, timeout = DEFAULT_TIMEOUT) {
  if (typeof name !== 'string' || !name.trim()) throw new Error('Web registration requires a nonempty fixture email');
  await region(page).waitFor({ state: 'visible', timeout });
  let controls = await settledScene(page, timeout);
  let step = await region(page).getAttribute('data-step');
  if (step === '4') {
    await controls.getByRole('button', { name: 'Back', exact: true }).click();
    controls = await settledScene(page, timeout);
    step = await region(page).getAttribute('data-step');
  }
  if (step === '1' || step === '2') {
    await controls.getByRole('button', { name: step === '1' ? 'Enter your email' : 'Enter', exact: true }).click();
    await page.locator('.orbit-registration[data-step="3"]').waitFor({ state: 'visible', timeout });
    controls = await settledScene(page, timeout);
  } else if (step !== '3') throw new Error(`Unexpected registration step before manual entry: ${step}`);
  await controls.getByPlaceholder('Enter your email', { exact: true }).fill(name);
  await controls.getByRole('button', { name: 'OK', exact: true }).click();
  await page.locator('.orbit-registration[data-step="2"]').waitFor({ state: 'visible', timeout });
  controls = await settledScene(page, timeout);
  const availability = region(page).getByRole('status').filter({ hasText: /^This name is (available|taken)$/ });
  await availability.or(alert(page)).first().waitFor({ state: 'visible', timeout });
  if (await alert(page).isVisible()) throw new Error(`Vault availability failed: ${await alert(page).innerText()}`);
  return { controls, availability: await availability.innerText() };
}

export async function createWebVault(page, name, { timeout = DEFAULT_TIMEOUT } = {}) {
  const prepared = await prepareWebVaultEmail(page, name, timeout);
  if (prepared.availability !== 'This name is available') throw new Error('Create requires an available Vault name');
  const completed = secrets(page).or(alert(page)).first().waitFor({ state: 'visible', timeout });
  await Promise.all([completed, prepared.controls.getByRole('button', { name: 'Create', exact: true }).click()]);
  if (await alert(page).isVisible()) throw new Error(`Vault creation failed: ${await alert(page).innerText()}`);
}

async function requestWebVault(page, name, expected, timeout) {
  const prepared = await prepareWebVaultEmail(page, name, timeout);
  if (prepared.availability !== 'This name is taken') throw new Error('Join requires an existing Vault');
  await prepared.controls.getByRole('button', { name: 'Join', exact: true }).click();
  await page.locator('.orbit-registration[data-step="4"]').waitFor({ state: 'visible', timeout });
  const controls = await settledScene(page, timeout);
  // Arm before Request: Awaiting approval text also appears before Core persistence.
  const outcome = page.locator('.orbit-registration[data-step="4"][data-phase="pending"], .orbit-registration[data-step="4"][data-phase="error"]');
  const persisted = outcome.waitFor({ state: 'visible', timeout });
  await Promise.all([persisted, controls.getByRole('button', { name: 'Request', exact: true }).click()]);
  const phase = await outcome.getAttribute('data-phase');
  if (expected === 'pending' && phase !== 'pending') throw new Error(`Web JOIN was rejected: ${await alert(page).innerText()}`);
  if (expected === 'error' && phase !== 'error') throw new Error('Expected rejected Web JOIN, but Core accepted a pending request');
  return alert(page);
}

export const requestWebVaultJoin = (page, name, { timeout = DEFAULT_TIMEOUT } = {}) =>
  requestWebVault(page, name, 'pending', timeout);
export const requestRejectedWebVaultJoin = (page, name, { timeout = DEFAULT_TIMEOUT } = {}) =>
  requestWebVault(page, name, 'error', timeout);

export async function retryWebVaultWithNewEmail(page, name, { timeout = DEFAULT_TIMEOUT } = {}) {
  await page.locator('.orbit-registration[data-step="4"][data-phase="error"]').waitFor({ state: 'visible', timeout });
  const previousName = await region(page).getByRole('heading').innerText();
  if (name.trim() === previousName.trim()) throw new Error('Reset/retry requires a different fixture email');
  const controls = await settledScene(page, timeout);
  await controls.getByRole('button', { name: 'Back', exact: true }).click();
  await settledScene(page, timeout);
  await createWebVault(page, name, { timeout });
}

export async function waitForWebAfterRemoval(page, { timeout = 180_000 } = {}) {
  const removed = page.locator('.orbit-registration[data-step="1"][data-phase="choice"], .orbit-registration[data-step="4"][data-phase="error"]');
  await removed.waitFor({ state: 'visible', timeout });
  await settledScene(page, timeout);
  if (await removed.getAttribute('data-phase') === 'error' && await alert(page).innerText() !== 'Request declined.') {
    throw new Error('Removed Web device did not reach its canonical declined registration state');
  }
}
