import { test, expect } from '@playwright/test';
import { execSync, spawn } from 'child_process';
import path from 'path';
import { fileURLToPath } from 'url';
import fs from 'fs';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

function getNextRunNumber(): number {
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

test('Three Device (Web + Android + iOS) - vault create, join, approve, secret sync, recovery', async ({ browser }) => {
  test.setTimeout(600000); // 10 minutes - Android + iOS tests + repeated web recovery polling

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
  page.on('console', (message) => {
    const text = message.text();
    const type = message.type();
    const shouldLog =
      type === 'error' ||
      type === 'warning' ||
      /show_recovered failed|Invalid share|\[recover v2\]|\[recovery_request v2\]/i.test(text);

    if (shouldLog) {
      console.log(`[WEB ${type}] ${text}`);
    }
  });
  page.on('pageerror', (error) => {
    console.error(`[WEB pageerror] ${error.message}`);
  });

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

  // ===== PHASE 1: Web - Create vault and secret (same as two-device test) =====
  console.log('✅ Phase 1: Web - Navigate and create vault and secret');
  await page.goto('/');
  await page.waitForLoadState('networkidle');

  // Create passkey
  const createPasskeyButton = page.getByRole('button', { name: 'Create Passkey' });
  await expect(createPasskeyButton).toBeVisible({ timeout: 10000 });
  console.log('✅ Create Passkey button found');

  await createPasskeyButton.click();
  console.log('⏳ Creating passkey...');

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

  const freeStatusBanner = page.getByText('Vault name is free!');
  await expect(freeStatusBanner).toBeVisible({ timeout: 15000 });
  console.log('✅ Vault name is free');

  // Create vault
  const createButton = page.getByRole('button', { name: 'Create' });
  await expect(createButton).toBeVisible({ timeout: 5000 });
  await createButton.click();
  console.log('⏳ Creating vault...');

  const vaultNameValue = page.getByText(vaultName);
  await expect(vaultNameValue).toBeVisible({ timeout: 60000 });
  console.log('✅ Vault created');

  // Add secret
  const addSecretButton = page.getByRole('button', { name: '+ Add Secret' });
  await expect(addSecretButton).toBeVisible({ timeout: 10000 });
  await addSecretButton.click();
  console.log('✅ Add Secret dialog opened');

  const secretNameInput = page.getByPlaceholder('Secret name');
  const secretValueInput = page.getByPlaceholder('Enter your secret');

  await expect(secretNameInput).toBeVisible({ timeout: 5000 });
  await expect(secretValueInput).toBeVisible({ timeout: 5000 });

  await secretNameInput.fill(secretName);
  await secretValueInput.fill(secretValue);
  console.log(`✅ Secret entered: name="${secretName}", value="${secretValue}"`);

  const submitButton = page.getByRole('button', { name: 'Add Secret', exact: true });
  await expect(submitButton).toBeVisible({ timeout: 5000 });
  await submitButton.click();
  console.log('⏳ Submitting secret...');

  const addSecretDialog = page.locator('[role="dialog"]');
  await expect(addSecretDialog).not.toBeVisible({ timeout: 30000 });
  console.log('✅ Secret created');

  const secretListItem = page.getByText(secretName);
  await expect(secretListItem).toBeVisible({ timeout: 10000 });
  console.log(`✅ Secret found in list`);

  // ===== PHASE 2: Verify initial Devices state =====
  console.log('\n✅ Phase 2: Web - Check Devices tab (initial state)');
  const devicesLink = page.getByRole('link', { name: 'Devices' });
  await expect(devicesLink).toBeVisible({ timeout: 5000 });
  await devicesLink.click();
  console.log('✅ Switched to Devices tab');

  const currentBadge = page.getByText('Current');
  await expect(currentBadge).toBeVisible({ timeout: 5000 });
  console.log('✅ One device with "Current" status visible');

  // ===== PHASE 3: Start Android process =====
  console.log('\n✅ Phase 3: Starting Android join process');

  const metaSecretComposeRoot =
    process.env.META_SECRET_COMPOSE_ROOT ||
    '/Users/dmitrykuklin/Documents/Projects/MetaSecret/meta-secret-compose';
  console.log(`📂 Compose repo root: ${metaSecretComposeRoot}`);

  const gradlewPath = path.join(metaSecretComposeRoot, 'gradlew');
  if (!fs.existsSync(gradlewPath)) {
    throw new Error(`gradlew not found at ${gradlewPath}`);
  }

  const androidOutput: string[] = [];
  const iosOutput: string[] = [];

  async function waitForProcessLog(
    output: string[],
    expectedText: string,
    label: string,
    timeoutMs = 120000
  ) {
    const start = Date.now();
    while (Date.now() - start < timeoutMs) {
      if (output.join('').includes(expectedText)) {
        console.log(`✅ ${label}`);
        return;
      }
      await page.waitForTimeout(500);
    }

    throw new Error(`Timed out waiting for ${label}: "${expectedText}"`);
  }

  async function waitForMobileRecoveryAppeared(round: number) {
    console.log(`⏳ Recovery #${round}: waiting for iOS alert to appear...`);
    await waitForProcessLog(
      iosOutput,
      `TEST: Recovery request alert #${round} appeared`,
      `iOS alert #${round} appeared`
    );
  }

  async function waitForMobileRecoveryDismissed(round: number) {
    console.log(`⏳ Recovery #${round}: waiting for iOS alert to disappear...`);
    await waitForProcessLog(
      iosOutput,
      `TEST: Recovery request alert #${round} disappeared`,
      `iOS alert #${round} disappeared`
    );
  }

  let androidProcessPromise: Promise<{ success: boolean; error?: string }>;

  try {
    androidProcessPromise = new Promise((resolve) => {
      const gradleArgs = [
        ':composeApp:connectedDebugAndroidTest',
        `-Pandroid.testInstrumentationRunnerArguments.class=ui.ThreeDeviceTest`,
        `-Pandroid.testInstrumentationRunnerArguments.email=${vaultName}`,
        `-Pandroid.testInstrumentationRunnerArguments.secretName=${secretName}`,
        `-Pandroid.testInstrumentationRunnerArguments.secretValue=${secretValue}`,
      ];

      console.log(`⏳ Spawning Gradle Android test`);

      const androidProcess = spawn(gradlewPath, gradleArgs, {
        cwd: metaSecretComposeRoot,
        stdio: ['inherit', 'pipe', 'pipe'],
      });

      androidProcess.stdout?.on('data', (data) => {
        const text = data.toString();
        androidOutput.push(text);
        process.stdout.write(`[ANDROID] ${text}`);
      });

      androidProcess.stderr?.on('data', (data) => {
        const text = data.toString();
        androidOutput.push(text);
        process.stderr.write(`[ANDROID] ${text}`);
      });

      androidProcess.on('exit', (code) => {
        if (code === 0) {
          console.log('✅ Android process completed successfully');
          resolve({ success: true });
        } else {
          console.error(`❌ Android process exited with code ${code}`);
          resolve({ success: false, error: `Android test failed with exit code ${code}` });
        }
      });
    });
  } catch (e) {
    console.error(`❌ Failed to spawn Android process: ${e}`);
    androidProcessPromise = Promise.resolve({ success: false, error: `Failed to spawn: ${e}` });
  }

  // ===== PHASE 4: Poll for Pending device from Android =====
  console.log('\n✅ Phase 4: Web - Poll for Pending device from Android');

  console.log('⏳ Waiting for Pending device (app polls every 5s)...');
  const pendingBadge = page.getByText('Pending');
  await expect(pendingBadge).toBeVisible({ timeout: 90000 });
  console.log('✅ Pending device appeared (Android joined)!');

  // ===== PHASE 5: Click Pending device and approve =====
  console.log('\n✅ Phase 5: Web - Click Pending device row and approve');

  const deviceRows = page.locator('[role="button"]').filter({ hasText: 'Pending' });
  await expect(deviceRows.first()).toBeVisible({ timeout: 5000 });
  await deviceRows.first().click();
  console.log('✅ Pending device row clicked');

  const joinConfirmTitle = page.getByText(/Are you sure you want to join/);
  await expect(joinConfirmTitle).toBeVisible({ timeout: 10000 });
  console.log('✅ Join confirmation dialog visible');

  const acceptButton = page.getByRole('button', { name: 'Accept' });
  await expect(acceptButton).toBeVisible({ timeout: 5000 });
  await acceptButton.click();
  console.log('⏳ Approval sent to Android device...');

  // Wait for device status to change to Member
  console.log('⏳ Waiting for device status to change to Member...');
  const pendingBadge2 = page.getByText('Pending');
  await expect(pendingBadge2).not.toBeVisible({ timeout: 60000 });
  const memberBadge = page.getByText('Member');
  await expect(memberBadge).toBeVisible({ timeout: 5000 });
  console.log('✅ Android device status changed to Member!');

  // ===== PHASE 6: Start iOS process =====
  console.log('\n✅ Phase 6: Starting iOS join process');

  let iosProcessPromise: Promise<{ success: boolean; error?: string }>;

  try {
    iosProcessPromise = new Promise((resolve) => {
      const iosAppPath = path.join(metaSecretComposeRoot, 'iosApp');

      // Delete app from simulator before running test
      console.log(`⏳ Removing old app from iOS Simulator...`);
      const iosSimulatorId = process.env.IOS_SIMULATOR_ID || 'B151420F-59B0-44DD-AC50-324529BEC660';
      const iosBundleId = process.env.IOS_BUNDLE_ID || 'org.metasecret.vault';
      try {
        execSync(`xcrun simctl uninstall '${iosSimulatorId}' ${iosBundleId}`, { stdio: 'ignore' });
        console.log('✅ Old app removed from simulator');
      } catch (e) {
        console.log('ℹ️ App was not installed or already removed');
      }

      const iosParamsPath = '/tmp/metasecret-three-device-ui-test.json';
      fs.writeFileSync(
        iosParamsPath,
        JSON.stringify({ email: vaultName, secretName, secretValue }),
        'utf-8'
      );

      const xcodebuildCmd = `xcodebuild test -scheme iosApp -sdk iphonesimulator -destination 'platform=iOS Simulator,name=iPhone 16,OS=18.5' -only-testing:iosAppUITests/ThreeDeviceUITest/testThreeDeviceJoinFlow`;

      console.log(`⏳ Spawning iOS test via xcodebuild with email="${vaultName}"`);

      const iosProcess = spawn('/bin/bash', ['-c', xcodebuildCmd], {
        cwd: iosAppPath,
        stdio: ['inherit', 'pipe', 'pipe'],
      });

      iosProcess.stdout?.on('data', (data) => {
        const text = data.toString();
        iosOutput.push(text);
        process.stdout.write(`[iOS] ${text}`);
      });

      iosProcess.stderr?.on('data', (data) => {
        const text = data.toString();
        iosOutput.push(text);
        process.stderr.write(`[iOS] ${text}`);
      });

      iosProcess.on('exit', (code) => {
        if (code === 0) {
          console.log('✅ iOS process completed successfully');
          resolve({ success: true });
        } else {
          console.error(`❌ iOS process exited with code ${code}`);
          resolve({ success: false, error: `iOS test failed with exit code ${code}` });
        }
      });
    });
  } catch (e) {
    console.error(`❌ Failed to spawn iOS process: ${e}`);
    iosProcessPromise = Promise.resolve({ success: false, error: `Failed to spawn: ${e}` });
  }

  // ===== PHASE 7: Show secret on web (Android present) =====
  console.log('\n✅ Phase 7: Web - Show secret (Android now Member)');

  const secretsLink2 = page.getByRole('link', { name: 'Secrets' });
  await expect(secretsLink2).toBeVisible({ timeout: 5000 });
  await secretsLink2.click();
  console.log('✅ Switched to Secrets tab');

  const secretRow2 = page.locator(`li:has-text("${secretName}")`);
  await expect(secretRow2).toBeVisible({ timeout: 5000 });
  console.log('✅ Secret row visible');

  // With 2 devices (web + android), button should still be "Show"
  const showButton2 = secretRow2.getByRole('button', { name: 'Show' });
  await expect(showButton2).toBeVisible({ timeout: 5000 });
  console.log('✅ "Show" button present (2 devices)');

  await showButton2.click();
  console.log('⏳ Revealing secret...');

  const secretLabel2 = page.getByText('Secret:');
  const revealedValue2 = page.getByText(secretValue, { exact: true });
  await expect(secretLabel2).toBeVisible({ timeout: 15000 });
  await expect(revealedValue2).toBeVisible({ timeout: 15000 });
  console.log(`✅ Secret revealed correctly: "${secretValue}"`);

  // Close the secret dialog
  console.log('⏳ Closing secret dialog...');
  const closeButton = page.getByRole('button', { name: 'Close' });
  if (await closeButton.isVisible({ timeout: 5000 }).catch(() => false)) {
    await closeButton.click();
  } else {
    // Fallback: press Escape
    await page.keyboard.press('Escape');
  }
  console.log('✅ Secret dialog closed');

  // ===== PHASE 8: Go back to Devices tab to wait for iOS =====
  console.log('\n✅ Phase 8: Web - Back to Devices, waiting for iOS');

  const devicesLink2 = page.getByRole('link', { name: 'Devices' });
  await expect(devicesLink2).toBeVisible({ timeout: 5000 });
  await devicesLink2.click();
  console.log('✅ Switched back to Devices tab');

  // ===== PHASE 9: Poll for second Pending device from iOS =====
  console.log('\n✅ Phase 9: Web - Poll for Pending device from iOS');

  console.log('⏳ Waiting for Pending device (app auto-updates via socket)...');
  // Wait for Pending badge - iOS device will trigger socket update
  const iosPendingBadge = page.getByText('Pending').first();
  await expect(iosPendingBadge).toBeVisible({ timeout: 90000 });
  console.log('✅ Pending device appeared (iOS)!');

  // ===== PHASE 10: Click Pending device (iOS) and approve =====
  console.log('\n✅ Phase 10: Web - Approve iOS device');

  const pendingRows = page.locator('[role="button"]').filter({ hasText: 'Pending' });
  const rowCount = await pendingRows.count();

  if (rowCount > 0) {
    // Click the last Pending device (should be iOS)
    await pendingRows.nth(rowCount - 1).click();
    console.log('✅ iOS Pending device row clicked');

    await expect(joinConfirmTitle).toBeVisible({ timeout: 10000 });
    console.log('✅ Join confirmation dialog visible');

    await expect(acceptButton).toBeVisible({ timeout: 5000 });
    await acceptButton.click();
    console.log('⏳ Approval sent to iOS device...');

    // Wait for all Pending badges to disappear
    console.log('⏳ Waiting for all devices to become Members...');
    const allPendingBadges = page.getByText('Pending');
    await expect(allPendingBadges).toHaveCount(0, { timeout: 60000 });
    console.log('✅ All devices are now Members!');

    console.log('⏳ Waiting 15s for mobile devices to sync after iOS approval...');
    await page.waitForTimeout(15000);
  }

  // ===== PHASE 11: Verify Secrets tab - should have "Recover" button now (3 devices!) =====
  console.log('\n✅ Phase 11: Web - Verify secret on Secrets tab');

  const secretsLink = page.getByRole('link', { name: 'Secrets' });
  await expect(secretsLink).toBeVisible({ timeout: 5000 });
  await secretsLink.click();
  console.log('✅ Switched to Secrets tab');

  const secretRow = page.locator(`li:has-text("${secretName}")`);
  await expect(secretRow).toBeVisible({ timeout: 5000 });
  console.log(`✅ Secret row visible`);

  // With 3 devices, button should be "Recover" not "Show"
  const recoverButton = secretRow.getByRole('button', { name: 'Recover' });
  await expect(recoverButton).toBeVisible({ timeout: 5000 });
  console.log('✅ "Recover" button present (3 devices!)');

  async function closeRecoveredSecretDialog(round: number) {
    console.log(`⏳ Recovery #${round}: closing recovered secret dialog...`);
    const closeRecoveredButton = page.getByRole('button', { name: 'Close' });
    if (await closeRecoveredButton.isVisible({ timeout: 5000 }).catch(() => false)) {
      await closeRecoveredButton.click();
    } else {
      await page.keyboard.press('Escape');
    }

    await expect(page.getByText('Secret:')).not.toBeVisible({ timeout: 10000 });
    console.log(`✅ Recovery #${round}: recovered secret dialog closed`);
    console.log(`⏳ Recovery #${round}: waiting 5s before next recovery request...`);
    await page.waitForTimeout(5000);
  }

  async function requestRecoveryAndWait(round: number, approver: 'iOS' | 'Android') {
    console.log(`\n✅ Phase 12.${round}: Web - Request recovery #${round} (${approver} approves)`);

    const currentSecretRow = page.locator(`li:has-text("${secretName}")`);
    await expect(currentSecretRow).toBeVisible({ timeout: 10000 });

    const currentRecoverButton = currentSecretRow.getByRole('button', { name: 'Recover' });
    await expect(currentRecoverButton).toBeVisible({ timeout: 10000 });
    await currentRecoverButton.click();
    console.log(`⏳ Recovery #${round}: request sent, waiting for ${approver} approval...`);
    await waitForMobileRecoveryAppeared(round);

    const recoveredSecretLabel = page.getByText('Secret:');
    const recoveredSecretValue = page.getByText(secretValue, { exact: true });
    await expect(recoveredSecretLabel).toBeVisible({ timeout: 120000 });
    await expect(recoveredSecretValue).toBeVisible({ timeout: 120000 });
    console.log(`✅ Recovery #${round}: recovered secret shown on Web: "${secretValue}"`);
  }

  // ===== PHASE 12: Repeated recovery requests =====
  await requestRecoveryAndWait(1, 'iOS');
  await waitForMobileRecoveryDismissed(1);
  await closeRecoveredSecretDialog(1);
  await requestRecoveryAndWait(2, 'Android');
  await waitForMobileRecoveryDismissed(2);
  await closeRecoveredSecretDialog(2);
  await requestRecoveryAndWait(3, 'iOS');
  await waitForMobileRecoveryDismissed(3);
  await closeRecoveredSecretDialog(3);
  await requestRecoveryAndWait(4, 'Android');
  await waitForMobileRecoveryDismissed(4);

  // ===== PHASE 13: Await processes before stopping =====
  console.log('\n✅ Phase 13: Waiting for Android and iOS processes to complete');

  const androidResult = await androidProcessPromise;
  if (!androidResult.success) {
    console.error(`Android test failed: ${androidResult.error}`);
  }

  // Wait for iOS with a timeout (5 min max)
  const iosTimeoutPromise = new Promise((resolve) => {
    setTimeout(() => resolve({ success: false, error: 'iOS test timeout after 300s' }), 300000);
  });
  const iosResult = await Promise.race([iosProcessPromise, iosTimeoutPromise]);
  if (!iosResult.success) {
    console.error(`iOS test failed: ${iosResult.error}`);
  }

  console.log('\n🎉 Four three-device recovery requests accepted and secrets recovered!\n');

  await context.close();
});
