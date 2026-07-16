import { test, expect } from '@playwright/test';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

function getNextRunNumber(): number {
  const __dirname = path.dirname(fileURLToPath(import.meta.url));
  const counterFile = path.join(__dirname, '.run-counter');
  let current = 0;

  if (fs.existsSync(counterFile)) {
    const content = fs.readFileSync(counterFile, 'utf-8').trim();
    const parsed = parseInt(content, 10);
    if (!isNaN(parsed)) {
      current = parsed;
    }
  }

  const next = current + 1;
  fs.writeFileSync(counterFile, next.toString(), 'utf-8');
  return next;
}

test('One Device - should create vault, add secret, and recover it', async ({ browser }) => {
  const n = getNextRunNumber();
  const vaultName = `web_${n}@test.com`;
  const secretName = `Secret${n}`;
  const secretValue = String(n);
  console.log(`\n📝 Test run #${n}: vault="${vaultName}", secret="${secretName}", value="${secretValue}"\n`);
    // Create a new context with WebAuthn support
    const context = await browser.newContext();
    const page = await context.newPage();

    // Enable WebAuthn virtual authenticator
    const client = await context.newCDPSession(page);
    await client.send('WebAuthn.enable');
    await client.send('WebAuthn.addVirtualAuthenticator', {
      options: {
        protocol: 'ctap2',
        transport: 'internal',
        hasResidentKey: true,
        hasUserVerification: true,
        isUserVerified: true,
      },
    });

    // Navigate to the app
    await page.goto('/');
    await page.waitForLoadState('networkidle');

    console.log('✅ Page loaded, PasskeyAuth modal should be visible');

    // Step 1: Create passkey (modal is forced open)
    const createPasskeyButton = page.getByRole('button', { name: 'Create Passkey' });
    await expect(createPasskeyButton).toBeVisible({ timeout: 10000 });
    console.log('✅ "Create Passkey" button found');

    await createPasskeyButton.click();
    console.log('⏳ Creating passkey...');

    // Wait for passkey creation and auto-authentication
    // The modal should close once authenticated
    await expect(page.locator('dialog')).not.toBeVisible({ timeout: 15000 });
    console.log('✅ Passkey created and authenticated, modal closed');

    // Step 2: Registration screen - enter vault name
    const vaultNameInput = page.getByPlaceholder('vault name');
    await expect(vaultNameInput).toBeVisible({ timeout: 10000 });
    console.log('✅ Vault name input found');

    await vaultNameInput.fill(vaultName);
    console.log(`✅ Vault name entered: ${vaultName}`);

    // Step 3: Check vault name availability
    const setVaultNameButton = page.getByRole('button', { name: 'Set Vault Name' });
    await expect(setVaultNameButton).toBeEnabled({ timeout: 5000 });
    await setVaultNameButton.click();
    console.log('⏳ Checking vault name...');

    // Wait for the "free" status to appear
    const freeStatusBanner = page.getByText('Vault name is free!');
    await expect(freeStatusBanner).toBeVisible({ timeout: 15000 });
    console.log('✅ Vault name is available (not a join scenario)');

    // Step 4: Create vault
    const createButton = page.getByRole('button', { name: 'Create' });
    await expect(createButton).toBeVisible({ timeout: 5000 });
    await createButton.click();
    console.log('⏳ Creating vault (this may take a while)...');

    // Wait for home screen to appear (generous timeout for network)
    // Look for the actual vault name value displayed (not just the label)
    const vaultNameValue = page.getByText(vaultName);
    await expect(vaultNameValue).toBeVisible({ timeout: 60000 });
    console.log('✅ Vault created, home screen loaded');

    // Step 5: Add secret - click "Add Secret" button
    const addSecretButton = page.getByRole('button', { name: '+ Add Secret' });
    await expect(addSecretButton).toBeVisible({ timeout: 10000 });
    await addSecretButton.click();
    console.log('✅ "Add Secret" dialog opened');

    // Step 6: Fill secret details
    const secretNameInput = page.getByPlaceholder('Secret name');
    const secretValueInput = page.getByPlaceholder('Enter your secret');

    await expect(secretNameInput).toBeVisible({ timeout: 5000 });
    await expect(secretValueInput).toBeVisible({ timeout: 5000 });

    await secretNameInput.fill(secretName);
    await secretValueInput.fill(secretValue);
    console.log(`✅ Secret details entered: name="${secretName}", value="${secretValue}"`);

    // Step 7: Submit the form
    const submitButton = page.getByRole('button', { name: 'Add Secret', exact: true });
    await expect(submitButton).toBeVisible({ timeout: 5000 });
    await submitButton.click();
    console.log('⏳ Submitting secret...');

    // Wait for dialog to close (indicates success)
    const addSecretDialog = page.locator('[role="dialog"]');
    await expect(addSecretDialog).not.toBeVisible({ timeout: 30000 });
    console.log('✅ Secret created successfully');

    // Step 8: Verify secret appears in the list
    const secretListItem = page.getByText(secretName);
    await expect(secretListItem).toBeVisible({ timeout: 10000 });
    console.log(`✅ Secret "${secretName}" found in the list`);

    // Step 9: Click "Show" to reveal the secret
    // Find the row containing the secret name, then click the Show button in that row
    const secretRow = page.locator(`li:has-text("${secretName}")`);
    const showButton = secretRow.getByRole('button', { name: 'Show' });

    await expect(secretRow).toBeVisible({ timeout: 5000 });
    await expect(showButton).toBeVisible({ timeout: 5000 });
    await showButton.click();
    console.log('⏳ Revealing secret...');

    // Step 10: Verify the revealed value
    const secretLabel = page.getByText('Secret:');
    const revealedValue = page.getByText(secretValue, { exact: true });

    await expect(secretLabel).toBeVisible({ timeout: 15000 });
    await expect(revealedValue).toBeVisible({ timeout: 15000 });
    console.log(`✅ Secret revealed correctly: "${secretValue}"`);

    console.log(`\n🎉 Full lifecycle test passed for run #${n}\n`);

    await context.close();
});
