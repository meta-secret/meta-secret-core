import assert from 'node:assert/strict';
import { prepareWebVaultEmail } from '../shared/web-orbit-registration.mjs';

const region = (page) => page.getByRole('region', { name: 'Vault registration', exact: true });
const scene = (page) => page.locator('.orbit-registration .orbit-turn:not([inert])').first();
const main = (page) => page.getByRole('link', { name: 'Secrets', exact: true });
const limit = 60_000;

export async function assertWebRegistration01(page) {
  await page.locator('.orbit-registration[data-step="1"][data-phase="choice"]').waitFor({ state: 'visible', timeout: limit });
  for (const name of ['Use Apple email', 'Use Google email', 'Enter your email']) {
    await scene(page).getByRole('button', { name, exact: true }).waitFor({ state: 'visible', timeout: limit });
  }
}

export async function webName(page, actor, emit) {
  if (actor.provider === 'manual') {
    await prepareWebVaultEmail(page, actor.vaultName, limit);
  } else {
    // Real provider SDK/system UI owns consent. Never inject a mock identity or
    // silently replace a provider case with manual entry.
    const button = scene(page).getByRole('button', { name: `Use ${actor.provider === 'apple' ? 'Apple' : 'Google'} email`, exact: true });
    assert(await button.isEnabled(), `Real ${actor.provider} provider is unavailable; this is a prerequisite blocker`);
    await button.click();
    await page.locator('.orbit-registration[data-step="2"][data-phase="exists"], .orbit-registration[data-step="2"][data-phase="available"]').waitFor({ state: 'visible', timeout: 180_000 });
    assert.equal((await region(page).getByRole('heading').innerText()).trim(), actor.vaultName, 'Provider account must match the exact fixture Vault name');
  }
  const create = scene(page).getByRole('button', { name: 'Create', exact: true });
  const join = scene(page).getByRole('button', { name: 'Join', exact: true });
  const phase = await region(page).getAttribute('data-phase');
  if (actor.actor === 'create') {
    assert.equal(phase, 'available', 'Create fixture must start from a clean server DB');
    assert(await create.isEnabled(), 'Available name must enable Create');
    assert(!(await join.isEnabled()), 'Available name must disable Join');
    emit('AVAILABLE');
  } else {
    assert.equal(phase, 'exists', 'Requester must see an existing Vault');
    assert(!(await create.isEnabled()), 'Create must be disabled for an existing Vault');
    assert(await join.isEnabled(), 'An existing Vault must enable Join');
    emit('TAKEN');
  }
}

export async function runWebRegistrationActor({ page, actor, emit, waitGate, captureIncompleteIdentity, proveLocalReset, screenshot }) {
  if (actor.actor === 'create') {
    await assertWebRegistration01(page);
    emit('READY');
    await screenshot?.('01');
    await webName(page, actor, emit);
    await screenshot?.('02-available');
    await scene(page).getByRole('button', { name: 'Create', exact: true }).click();
    await main(page).waitFor({ state: 'visible', timeout: 180_000 });
    emit('CREATED');
    await screenshot?.('main');
    await assertWebGeneratedQr(page, actor.vaultName);
    emit('QR_GENERATED');
    await screenshot?.('qr-generated');
    await page.getByTestId('close-vault-qr').click();
    await page.getByRole('link', { name: 'Secrets', exact: true }).click();
    return;
  }
  if (actor.actor === 'responder') {
    await main(page).waitFor({ state: 'visible', timeout: limit });
    await page.getByRole('link', { name: 'Devices', exact: true }).click();
    assert.equal(await page.getByTestId('pending-device-row').count(), 0, 'Responder starts without a stale pending request');
    emit('READY');
    await waitGate();
    const pending = page.getByTestId('pending-device-row');
    await pending.waitFor({ state: 'visible', timeout: 180_000 });
    assert.equal(await pending.count(), 1, 'Independent clean case has exactly one request');
    const canonicalDeviceName = await pending.locator('p.font-semibold').innerText();
    assert(canonicalDeviceName.trim(), 'Canonical Device name is required; simulator alias is not an identity');
    await pending.click();
    if (actor.decision === 'accepted') await page.getByTestId('accept-join-request').click();
    else await page.getByRole('button', { name: 'Decline', exact: true }).click();
    await pending.waitFor({ state: 'detached', timeout: 180_000 });
    emit('DECIDED');
    return;
  }
  await assertWebRegistration01(page);
  emit('READY');
  await webName(page, actor, emit);
  await scene(page).getByRole('button', { name: 'Join', exact: true }).click();
  await page.locator('.orbit-registration[data-step="4"][data-phase="join"]').waitFor({ state: 'visible', timeout: limit });
  await scene(page).getByRole('button', { name: 'Request', exact: true }).click();
  await page.locator('.orbit-registration[data-step="4"][data-phase="pending"]').waitFor({ state: 'visible', timeout: 180_000 });
  emit('PENDING');
  await screenshot?.('04-pending');
  if (actor.outcome !== 'accepted') {
    if (actor.outcome === 'declined-retry') {
      await page.locator('.orbit-registration[data-step="4"][data-phase="error"]').waitFor({ state: 'visible', timeout: 180_000 });
      assert.equal((await region(page).getByRole('alert').innerText()).trim(), 'Request declined.');
      emit('DECLINED');
      await screenshot?.('04-declined');
    }
    assert(captureIncompleteIdentity, 'Read-only incomplete identity capture is required before Back');
    const incompleteAttempt = await captureIncompleteIdentity(actor.vaultName, actor.outcome === 'declined-retry' ? 'declined' : 'pending');
    await scene(page).getByRole('button', { name: 'Back', exact: true }).click();
    await assertWebRegistration01(page);
    assert(proveLocalReset, 'A read-only local identity oracle is required; UI alone does not prove key cleanup');
    await proveLocalReset(incompleteAttempt);
    emit('RESET01');
    await screenshot?.('01-reset');
    assert.notEqual(actor.retryVaultName, actor.vaultName, 'Retry must join a different pre-created Vault');
    await prepareWebVaultEmail(page, actor.retryVaultName, limit);
    assert.equal(await region(page).getAttribute('data-phase'), 'exists');
    await scene(page).getByRole('button', { name: 'Join', exact: true }).click();
    await page.locator('.orbit-registration[data-step="4"][data-phase="join"]').waitFor({ state: 'visible', timeout: limit });
    await scene(page).getByRole('button', { name: 'Request', exact: true }).click();
    await page.locator('.orbit-registration[data-step="4"][data-phase="pending"]').waitFor({ state: 'visible', timeout: 180_000 });
    emit('RETRY_PENDING');
  }
  await main(page).waitFor({ state: 'visible', timeout: 180_000 });
  emit('MAIN');
  await screenshot?.('main');
}

export async function assertWebGeneratedQr(page, vaultName) {
  await page.getByRole('link', { name: 'Devices', exact: true }).click();
  await page.getByTestId('add-device').click();
  assert.equal((await page.getByTestId('vault-qr-name').innerText()).trim(), vaultName);
  const code = page.getByRole('img', { name: 'Vault name QR code', exact: true });
  await code.locator('svg').waitFor({ state: 'visible', timeout: limit });
  // Decode the actual rendered pixels, as the native QR actors do. Serializing
  // the responsive SVG loses its layout dimensions and can produce a0px image.
  const pixels = await code.screenshot({ type: 'png' });
  const imageUrl = `data:image/png;base64,${pixels.toString('base64')}`;
  const decoded = await page.evaluate(async (pngDataUrl) => {
    const { default: QrScanner } = await import('/node_modules/qr-scanner/qr-scanner.min.js');
    return (await QrScanner.scanImage(pngDataUrl, { returnDetailedScanResult: true })).data;
  }, imageUrl);
  assert.equal(decoded, vaultName, 'Actual UI QR must decode to the exact canonical name');
}

// Self-contained browser callback: only fixed-schema readonly counts leave IndexedDB.
// Exported so helper tests exercise real inventory/schema/count behavior with fake IDB.
export async function readWebCredentialMetadata() {
  if (!indexedDB.databases) throw new Error('Read-only IndexedDB metadata inventory is unavailable');
  const existing = new Set((await indexedDB.databases()).map(({ name }) => name));
  const databases = {};
  for (const name of ['meta-secret', 'meta-secret-server', 'meta-secret-v-device', 'meta-secret-v-device-2']) {
    if (!existing.has(name)) {
      databases[name] = { exists: false, device: 0, user: 0 };
      continue;
    }
    const database = await new Promise((resolveDb, reject) => {
      const request = indexedDB.open(name);
      request.onsuccess = () => resolveDb(request.result);
      request.onerror = () => reject(new Error('Identity metadata database unavailable'));
      request.onupgradeneeded = () => { request.transaction.abort(); reject(new Error('Identity metadata changed during verification')); };
    });
    try {
      if (!database.objectStoreNames.contains('commit_log')) throw new Error('Identity metadata schema unavailable');
      const transaction = database.transaction(['commit_log'], 'readonly');
      const store = transaction.objectStore('commit_log');
      const counts = await Promise.all(['DeviceCreds:index::', 'UserCreds:index::'].map((prefix) => new Promise((resolveCount, reject) => {
        const request = store.count(IDBKeyRange.bound(prefix, `${prefix}\uffff`));
        request.onsuccess = () => resolveCount(request.result);
        request.onerror = () => reject(new Error('Identity metadata count unavailable'));
      })));
      databases[name] = { exists: true, device: counts[0], user: counts[1] };
    } finally { database.close(); }
  }
  return databases;
}

const knownDatabases = ['meta-secret', 'meta-secret-server', 'meta-secret-v-device', 'meta-secret-v-device-2'];

async function readWebIdentityMetadata(page) {
  const canonical = await page.evaluate(async () => {
    const [{ AppState }, { readRegistrationSnapshot }] = await Promise.all([
      import('/src/stores/app-state.ts'), import('/src/features/registration/wasmRegistrationAdapter.ts'),
    ]);
    const state = AppState().currState;
    if (!state) throw new Error('Canonical registration state is unavailable');
    return { snapshot: readRegistrationSnapshot(state), deviceId: state.device_id().wasm_id_str() };
  });
  const databases = await page.evaluate(readWebCredentialMetadata);
  assert(databases && typeof databases === 'object', 'Identity metadata inventory unavailable');
  assert.deepEqual(Object.keys(databases).sort(), [...knownDatabases].sort(), 'Identity metadata inventory must cover the exact known databases');
  for (const value of Object.values(databases)) {
    assert(value && typeof value.exists === 'boolean', 'Identity metadata schema unavailable');
    assert(Number.isSafeInteger(value.device) && value.device >= 0 && Number.isSafeInteger(value.user) && value.user >= 0, 'Identity metadata counts unavailable');
    if (!value.exists) assert.equal(value.device + value.user, 0, 'Absent database cannot contain credential records');
  }
  return { ...canonical, databases };
}

export async function captureWebIncompleteIdentity(page, name, expectedKind) {
  assert(expectedKind === 'pending' || expectedKind === 'declined', 'Only an incomplete attempt may be captured');
  const proof = await readWebIdentityMetadata(page);
  assert.deepEqual(proof.snapshot, { kind: expectedKind, name }, 'Capture must match the exact canonical incomplete Vault');
  assert(typeof proof.deviceId === 'string' && proof.deviceId, 'Incomplete public device identity is unavailable');
  assert(proof.databases['meta-secret'].exists && proof.databases['meta-secret'].device > 0 && proof.databases['meta-secret'].user > 0, 'Incomplete attempt must have persisted credential records before Back');
  return { name, deviceId: proof.deviceId };
}

export async function assertWebLocalIdentityCleared(page, incompleteAttempt) {
  assert(incompleteAttempt && typeof incompleteAttempt.name === 'string' && incompleteAttempt.name && typeof incompleteAttempt.deviceId === 'string' && incompleteAttempt.deviceId, 'Incomplete attempt capture is required for cleanup proof');
  const proof = await readWebIdentityMetadata(page);
  assert.deepEqual(proof.snapshot, { kind: 'local', name: '' }, 'Core must expose an empty local identity after Back');
  assert(typeof proof.deviceId === 'string' && proof.deviceId, 'New Local public device identity is unavailable');
  assert.notEqual(proof.deviceId, incompleteAttempt.deviceId, 'Incomplete device identity must not survive cleanup');
  for (const name of knownDatabases) {
    assert.equal(proof.databases[name].user, 0, 'No old UserCreds record may remain after Back');
    assert.equal(proof.databases[name].device, name === 'meta-secret' ? 1 : 0, 'Only one new primary bootstrap DeviceCreds record may remain');
  }
  assert.equal(proof.databases['meta-secret'].exists, true, 'Initialized primary bootstrap database is required');
}
