import { test, expect } from '@playwright/test';
import { spawn } from 'child_process';
import path from 'path';
import { fileURLToPath } from 'url';
import fs from 'fs';
import { getNextRunNumber } from './utils/runCounter';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

test.setTimeout(120000); // 2 minutes - Android test + web polling
test('Two Device (Web + Android) - vault create, join, approve, secret sync', async ({ browser }) => {
  const n = getNextRunNumber();
  const vaultName = `web_${n}@test.com`;
  const secretName = `Secret${n}`;
  const secretValue = String(n);

  console.log(
    `\n📝 Test run #${n}: vault="${vaultName}", secret="${secretName}", value="${secretValue}"\n`
  );

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

  // ===== PHASE 1: Web side - Vault creation and secret =====
  console.log('✅ Phase 1: Web - Navigate and create vault and secret');
  await page.goto('/');
  await page.waitForLoadState('networkidle');

  // Wait for page to reload and show registration
  await page.waitForLoadState('networkidle');
  await expect(page.locator('dialog')).not.toBeVisible({ timeout: 15000 });

  // Create passkey (modal is forced open)
  const createPasskeyButton = page.getByRole('button', { name: 'Create Passkey' });
  await expect(createPasskeyButton).toBeVisible({ timeout: 10000 });
  console.log('✅ Create Passkey button found');

  await createPasskeyButton.click();
  console.log('⏳ Creating passkey...');

  // Wait for passkey creation and auto-authentication
  await expect(page.locator('dialog')).not.toBeVisible({ timeout: 15000 });
  console.log('✅ Passkey created and authenticated');

  // Registration screen - enter vault name
  const vaultNameInput = page.getByPlaceholder('vault name');
  await expect(vaultNameInput).toBeVisible({ timeout: 10000 });
  await vaultNameInput.fill(vaultName);
  console.log(`✅ Vault name entered: ${vaultName}`);

  // Check vault name availability
  const setVaultNameButton = page.getByRole('button', { name: 'Set Vault Name' });
  await expect(setVaultNameButton).toBeEnabled({ timeout: 5000 });
  await setVaultNameButton.click();
  console.log('⏳ Checking vault name...');

  // Wait for the "free" status to appear
  const freeStatusBanner = page.getByText('Vault name is free!');
  await expect(freeStatusBanner).toBeVisible({ timeout: 15000 });
  console.log('✅ Vault name is free (not a join scenario)');

  // Create vault
  const createButton = page.getByRole('button', { name: 'Create' });
  await expect(createButton).toBeVisible({ timeout: 5000 });
  await createButton.click();
  console.log('⏳ Creating vault...');

  // Wait for home screen to appear
  const vaultNameValue = page.getByText(vaultName);
  await expect(vaultNameValue).toBeVisible({ timeout: 60000 });
  console.log('✅ Vault created, home screen loaded');

  // Add secret
  const addSecretButton = page.getByRole('button', { name: '+ Add Secret' });
  await expect(addSecretButton).toBeVisible({ timeout: 10000 });
  await addSecretButton.click();
  console.log('✅ Add Secret dialog opened');

  // Fill secret details
  const secretNameInput = page.getByPlaceholder('Secret name');
  const secretValueInput = page.getByPlaceholder('Enter your secret');

  await expect(secretNameInput).toBeVisible({ timeout: 5000 });
  await expect(secretValueInput).toBeVisible({ timeout: 5000 });

  await secretNameInput.fill(secretName);
  await secretValueInput.fill(secretValue);
  console.log(`✅ Secret details entered: name="${secretName}", value="${secretValue}"`);

  // Submit the form
  const submitButton = page.getByRole('button', { name: 'Add Secret', exact: true });
  await expect(submitButton).toBeVisible({ timeout: 5000 });
  await submitButton.click();
  console.log('⏳ Submitting secret...');

  // Wait for dialog to close (indicates success)
  const addSecretDialog = page.locator('[role="dialog"]');
  await expect(addSecretDialog).not.toBeVisible({ timeout: 30000 });
  console.log('✅ Secret created successfully');

  // Verify secret appears in the list
  const secretListItem = page.getByText(secretName);
  await expect(secretListItem).toBeVisible({ timeout: 10000 });
  console.log(`✅ Secret "${secretName}" found in the list`);

  // ===== PHASE 2: Navigate to Devices tab and verify initial state =====
  console.log('\n✅ Phase 2: Web - Check Devices tab (initial state)');

  const devicesLink = page.getByRole('link', { name: 'Devices' });
  await expect(devicesLink).toBeVisible({ timeout: 5000 });
  await devicesLink.click();
  console.log('✅ Switched to Devices tab');

  // Verify exactly one device with "Current" badge
  const currentBadge = page.getByText('Current');
  await expect(currentBadge).toBeVisible({ timeout: 5000 });
  console.log('✅ One device with "Current" status visible');

  // ===== PHASE 3: Start Android process (non-blocking) =====
  console.log('\n✅ Phase 3: Starting Android two-device join process');

  // Resolve compose repo path
  // For now, use hardcoded path (can be overridden with env var)
  const metaSecretComposeRoot =
    process.env.META_SECRET_COMPOSE_ROOT ||
    '/Users/dmitrykuklin/Documents/Projects/MetaSecret/meta-secret-compose';
  console.log(`📂 Compose repo root: ${metaSecretComposeRoot}`);

  // Verify gradlew exists
  const gradlewPath = path.join(metaSecretComposeRoot, 'gradlew');
  if (!fs.existsSync(gradlewPath)) {
    throw new Error(`gradlew not found at ${gradlewPath}`);
  }

  let androidProcessPromise: Promise<{ success: boolean; error?: string }>;

  try {
    androidProcessPromise = new Promise((resolve) => {
      const gradleArgs = [
        ':composeApp:connectedDebugAndroidTest',
        `-Pandroid.testInstrumentationRunnerArguments.class=ui.TwoDeviceTest`,
        `-Pandroid.testInstrumentationRunnerArguments.email=${vaultName}`,
        `-Pandroid.testInstrumentationRunnerArguments.secretName=${secretName}`,
        `-Pandroid.testInstrumentationRunnerArguments.secretValue=${secretValue}`,
      ];

      console.log(`⏳ Spawning Gradle test (check adb devices to ensure emulator is running)`);

      const androidProcess = spawn(gradlewPath, gradleArgs, {
        cwd: metaSecretComposeRoot,
        stdio: ['inherit', 'pipe', 'pipe'],
      });

      const buffers: string[] = [];

      androidProcess.stdout?.on('data', (data) => {
        const text = data.toString();
        buffers.push(text);
        process.stdout.write(`[ANDROID] ${text}`);
      });

      androidProcess.stderr?.on('data', (data) => {
        const text = data.toString();
        buffers.push(text);
        process.stderr.write(`[ANDROID] ${text}`);
      });

      androidProcess.on('exit', (code) => {
        const fullOutput = buffers.join('');
        if (code === 0) {
          console.log('✅ Android process completed successfully');
          resolve({ success: true });
        } else {
          console.error(`❌ Android process exited with code ${code}`);
          resolve({
            success: false,
            error: `Android process failed with exit code ${code}.\nOutput: ${fullOutput}`,
          });
        }
      });
    });
  } catch (e) {
    console.error(`❌ Failed to spawn Android process: ${e}`);
    androidProcessPromise = Promise.resolve({
      success: false,
      error: `Failed to spawn: ${e}`,
    });
  }

  // ===== PHASE 4: Poll for Pending device on web side =====
  console.log('\n✅ Phase 4: Web - Poll for Pending device from Android');

  // Wait for Pending device to appear (app auto-updates via socket polling)
  // No need to reload - the app will update UI automatically
  console.log('⏳ Waiting for Pending device (app polls every 5s)...');
  const pendingBadge = page.getByText('Pending');
  await expect(pendingBadge).toBeVisible({ timeout: 90000 }); // Wait up to 90 seconds
  console.log('✅ Pending device appeared!');

  // ===== PHASE 5: Click Pending device and approve it =====
  console.log('\n✅ Phase 5: Web - Click Pending device row and approve');

  // Find the pending device row and click it
  const deviceRows = page.locator('[role="button"]').filter({ hasText: 'Pending' });
  await expect(deviceRows.first()).toBeVisible({ timeout: 5000 });
  await deviceRows.first().click();
  console.log('✅ Pending device row clicked, join confirmation dialog should appear');

  // Wait for and verify the join confirmation dialog
  const joinConfirmTitle = page.getByText(/Are you sure you want to join/);
  await expect(joinConfirmTitle).toBeVisible({ timeout: 10000 });
  console.log('✅ Join confirmation dialog visible');

  // Click Accept button
  const acceptButton = page.getByRole('button', { name: 'Accept' });
  await expect(acceptButton).toBeVisible({ timeout: 5000 });
  await acceptButton.click();
  console.log('⏳ Approval sent to Android device...');

  // Wait for device status to change from "Pending" to "Member" (app auto-updates via socket)
  console.log('⏳ Waiting for device status to change to Member...');
  // First, ensure the Pending badge disappears
  const pendingBadge2 = page.getByText('Pending');
  await expect(pendingBadge2).not.toBeVisible({ timeout: 60000 });
  // Then verify Member badge appears
  const memberBadge = page.getByText('Member');
  await expect(memberBadge).toBeVisible({ timeout: 5000 });
  console.log('✅ Device status changed to Member!');

  // ===== PHASE 6: Verify no recovery dialog appeared =====
  console.log('\n✅ Phase 6: Web - Verify no recovery request dialog');

  const recoveryDialog = page.getByText('Recover secret?');
  await expect(recoveryDialog).not.toBeVisible({ timeout: 1000 });
  console.log('✅ No recovery dialog appeared (as expected for 2 devices)');

  // ===== PHASE 7: Go to Secrets tab and verify =====
  console.log('\n✅ Phase 7: Web - Verify secret on Secrets tab');

  const secretsLink = page.getByRole('link', { name: 'Secrets' });
  await expect(secretsLink).toBeVisible({ timeout: 5000 });
  await secretsLink.click();
  console.log('✅ Switched to Secrets tab');

  // Verify the secret is visible
  const secretRow = page.locator(`li:has-text("${secretName}")`);
  await expect(secretRow).toBeVisible({ timeout: 5000 });
  console.log(`✅ Secret row "${secretName}" is visible`);

  // Verify the button is still "Show" (not "Recover")
  const showButton = secretRow.getByRole('button', { name: 'Show' });
  await expect(showButton).toBeVisible({ timeout: 5000 });
  console.log('✅ "Show" button present (not "Recover" — device count still ≤2)');

  // Click Show to reveal the secret
  await showButton.click();
  console.log('⏳ Revealing secret...');

  // Verify the revealed value
  const secretLabel = page.getByText('Secret:');
  const revealedValue = page.getByText(secretValue, { exact: true });

  await expect(secretLabel).toBeVisible({ timeout: 15000 });
  await expect(revealedValue).toBeVisible({ timeout: 15000 });
  console.log(`✅ Secret revealed correctly: "${secretValue}"`);

  // ===== PHASE 8: Await Android process completion =====
  console.log('\n✅ Phase 8: Wait for Android process to complete');

  const androidResult = await androidProcessPromise;
  if (!androidResult.success) {
    throw new Error(`Android test failed: ${androidResult.error}`);
  }

  console.log('\n🎉 Full two-device flow completed successfully!');
  console.log(`✅ Web: Vault created, secret added and revealed, device pair approved`);
  console.log(`✅ Android: Joined vault, received approval via biometry bypass, secret synced and revealed`);
  console.log(`✅ No recovery dialog interference on web side\n`);

  await context.close();
});
