import { openWebLiveboard } from './web-liveboard-actions.mjs';
import { decideWebIncoming, openWebIncoming } from './web-liveboard-inbox.mjs';
import { installWebOperationObserver, waitWebOperation, disposeWebOperationObserver } from './web-operation-observer.mjs';
let removalSequence = 0;

export function selectCanonicalMember(members, category, { current, expectedDeviceName } = {}) {
  const canonicalCategory = category === 'iPhone' || category === 'iOS' ? 'Iphone' : category;
  const found = members.filter((member) => member.category === canonicalCategory && (current == null || member.current === current) && (!expectedDeviceName || member.name === expectedDeviceName));
  if (found.length !== 1 || !found[0].name || !found[0].deviceId) throw new Error('Expected one exact canonical member/category match');
  return found[0];
}

async function canonicalMembers(page) {
  // Read-only identity discovery from the SAME current snapshot; no refresh/RPC/command.
  // Every returned identity is then asserted against the actual visible slot.
  return page.evaluate(async () => {
    const { AppState } = await import('/src/stores/app-state.ts');
    const { readOwnedMember } = await import('/src/features/liveboard/wasmOwnership.ts');
    const { readCoreProjection } = await import('/src/features/liveboard/coreProjection.ts');
    return readOwnedMember({ state: AppState().currState }, (member) => readCoreProjection({ member, identity: 'e2e-read-only-identity-discovery' }).devices);
  });
}

export async function inspectWebLiveboardMember(page, category, options = {}) {
  await openWebLiveboard(page);
  const selected = selectCanonicalMember(await canonicalMembers(page), category, options);
  const row = page.locator('.device-slot[data-device-id]').filter({ has: page.locator('.device-name').filter({ hasText: new RegExp(`^${selected.name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}$`) }) });
  await row.waitFor({ state: 'visible', timeout: 120_000 });
  if (await row.count() !== 1 || await row.getAttribute('data-device-id') !== selected.deviceId) throw new Error('Canonical member does not match one visible device slot');
  const state = (await row.locator('.device-state').textContent()).trim();
  if (state !== (selected.current ? 'This device' : 'Connected')) throw new Error('Visible member current/connected state mismatch');
  if (options.controls === false) return { row, deviceName: selected.name, deviceId: selected.deviceId, category, current: selected.current };
  if (selected.current) {
    const current = page.getByRole('button', { name: 'Current', exact: true });
    if (await current.isVisible().catch(() => false)) {
      if (await current.isEnabled()) await current.click();
      else {
        if (await page.getByTestId('remove-device').isEnabled().catch(() => false)) throw new Error('Locked preview exposes enabled Remove');
        return { row, deviceName: selected.name, deviceId: selected.deviceId, category, current: true };
      }
    }
  }
  await row.click();
  const remove = page.getByTestId('remove-device');
  if (selected.current) {
    await remove.waitFor({ state: 'hidden', timeout: 30_000 });
  } else {
    await remove.waitFor({ state: 'visible', timeout: 30_000 });
    if (selected.canRemove && !await remove.isEnabled()) throw new Error('Ready removable member has disabled real Remove');
  }
  return { row, deviceName: selected.name, deviceId: selected.deviceId, category, current: selected.current };
}

export async function assertWebLiveboardMembers(page, categories, names = {}) {
  await openWebLiveboard(page);
  const current = await canonicalMembers(page);
  if (current.length !== categories.length) throw new Error('Canonical membership count mismatch');
  if (await page.locator('.device-slot[data-device-id]').count() !== categories.length) throw new Error('Visible connected membership count mismatch');
  const result = {};
  for (const category of categories) {
    const platform = { Web: 'web', Android: 'android', iPhone: 'ios', Cli: 'cli' }[category];
    const member = await inspectWebLiveboardMember(page, category, { current: platform === 'web', expectedDeviceName: names[platform], controls: false });
    result[platform] = member.deviceName;
    const canonical = selectCanonicalMember(current, category, { current: platform === 'web', expectedDeviceName: names[platform] });
    if (await member.row.isEnabled() && (canonical.current || canonical.canRemove)) {
      await inspectWebLiveboardMember(page, category, { current: canonical.current, expectedDeviceName: canonical.name });
    }
  }
  return result;
}

export async function removeWebLiveboardMember(page, { targetCategory, expectedTargetDeviceName, waitForTargetGone, label, requestVisibleMarker, completionMarker, decision }) {
  if (!expectedTargetDeviceName) throw new Error('Remove requires the exact observed canonical target');
  const target = await inspectWebLiveboardMember(page, targetCategory, { current: false, expectedDeviceName: expectedTargetDeviceName, controls: decision !== 'approve' });
  if (decision === 'approve') {
    const head = await openWebIncoming(page, { kind: 'remove-device', subject: expectedTargetDeviceName, count: 1 });
    if (requestVisibleMarker) console.log(`E2E: ${requestVisibleMarker}`);
    await decideWebIncoming(page, { kind: head.kind, requestId: head.requestId, subject: head.subject, count: 1 }, true);
    if (waitForTargetGone) await target.row.waitFor({ state: 'hidden', timeout: 180_000 });
    if (completionMarker) console.log(`E2E: ${completionMarker}`);
    return { ...target, label };
  }
  // The selected real slot and preview replace the old confirmation's target identity.
  if (await target.row.getAttribute('aria-pressed') !== 'true') throw new Error('Removal preview is not pinned to the exact target');
  if (requestVisibleMarker) console.log(`E2E: ${requestVisibleMarker}`);
  const scope = `member-remove-${++removalSequence}`;
  await page.evaluate(installWebOperationObserver, { scope, method: 'remove_member_checked', expectedId: target.deviceId, borrowed: true });
  try {
    await page.getByTestId('remove-device').click();
    const result = await waitWebOperation(page, scope);
    if (!result.borrowedAlive) throw new Error('Remove consumed the actual borrowed Device before its await completed');
  } finally { await page.evaluate(disposeWebOperationObserver, { scope }); }
  if (waitForTargetGone || completionMarker) await target.row.waitFor({ state: 'hidden', timeout: 180_000 });
  if (completionMarker) console.log(`E2E: ${completionMarker}`);
  return { ...target, label };
}

export async function declineWebLiveboardRemoval(page, { expectedTargetDeviceName, requestVisibleMarker, completionMarker }) {
  if (!expectedTargetDeviceName) throw new Error('Decline requires the exact canonical target');
  const head = await openWebIncoming(page, { kind: 'remove-device', subject: expectedTargetDeviceName, count: 1 });
  if (requestVisibleMarker) console.log(`E2E: ${requestVisibleMarker}`);
  await decideWebIncoming(page, { kind: head.kind, requestId: head.requestId, subject: head.subject, count: 1 }, false);
  if (completionMarker) console.log(`E2E: ${completionMarker}`);
}
