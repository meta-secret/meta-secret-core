import { webSecretCard } from './web-liveboard-actions.mjs';
import { installWebAuthnAbort, abortWebAuthn, disposeWebAuthnAbort, installWebOperationObserver, readWebOperationObserver, disposeWebOperationObserver } from './web-operation-observer.mjs';

/** A genuine browser WebAuthn API abort. This does not claim a physical/native OS Cancel-button interaction. */
export async function cancelWebDeletionAuthentication(page, authenticator, { name, id, scope }) {
  if (!authenticator?.cdp || !authenticator.authenticatorId) throw new Error('Browser cancellation requires the same existing virtual authenticator');
  const authScope = `__METASECRET_E2E_AUTH_ABORT_${scope}`;
  const operationScope = `cancel-delete-${scope}`;
  const { card } = await webSecretCard(page, name, { id });
  await authenticator.cdp.send('WebAuthn.setAutomaticPresenceSimulation', { authenticatorId: authenticator.authenticatorId, enabled: false });
  try {
    await page.evaluate(installWebAuthnAbort, { scope: authScope });
    await page.evaluate(installWebOperationObserver, { scope: operationScope, method: 'delete_secret_checked', expectedId: id });
    await card.getByTestId(`delete-secret-${name}`).click();
    await page.waitForFunction((scope) => globalThis[scope]?.phase === 'entered', authScope, { timeout: 15_000 });
    await page.evaluate(abortWebAuthn, { scope: authScope });
    await page.waitForFunction((scope) => globalThis[scope]?.phase === 'settled', authScope, { timeout: 15_000 });
    const outcome = await page.evaluate((scope) => globalThis[scope].outcome, authScope);
    if (outcome !== 'AbortError') throw new Error('Browser WebAuthn did not report its genuine AbortError');
    await page.waitForFunction((id) => {
      const card = [...document.querySelectorAll('article.secret-card')].find((node) => node.dataset.secretId === id);
      const remove = card?.querySelector('[data-testid^="delete-secret-"]');
      return card?.dataset.phase === 'masked' && remove && !remove.disabled;
    }, id, { timeout: 30_000 });
    const observed = await page.evaluate(readWebOperationObserver, { scope: operationScope });
    if (observed.calls !== 0 || observed.phase !== 'armed') throw new Error('Cancelled browser authentication entered deletion Core');
    return { kind: 'browser-webauthn-api-abort', actualApiEntered: true, outcome, operationCalls: 0 };
  } finally {
    await page.evaluate(disposeWebAuthnAbort, { scope: authScope });
    await page.evaluate(disposeWebOperationObserver, { scope: operationScope });
    await authenticator.cdp.send('WebAuthn.setAutomaticPresenceSimulation', { authenticatorId: authenticator.authenticatorId, enabled: true });
  }
}
