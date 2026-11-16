import { openWebLiveboard } from './web-liveboard-actions.mjs';
const observed = new WeakMap();

export function validateIncomingHead(head, expected) {
  if (!head.requestId || !head.kind || !head.requester || !Number.isInteger(head.count) || head.count < 1) throw new Error('Incoming request identity is missing');
  for (const key of ['kind', 'requestId', 'requester', 'subject']) {
    if (expected[key] != null && head[key] !== expected[key]) throw new Error(`Unsupported FIFO head identity: expected ${key} is not the visible head`);
  }
  if (expected.count != null && head.count !== expected.count) throw new Error('Exact visible incoming queue count mismatch; per-secret count cannot substitute for FIFO total');
  return head;
}

export function validateIncomingRemoval(previous, current) {
  return current.state !== 'sending' && current.state !== 'error' && (current.count === 0 || Boolean(current.requestId)) && current.requestId !== previous.requestId && current.count < previous.count;
}

export async function openWebIncoming(page, expected, timeout = 120_000, budget) {
  const perform = async operation => budget ? budget.perform(operation) : operation(timeout);
  await openWebLiveboard(page, timeout, budget);
  const inbox = page.getByTestId('liveboard-notifications');
  await perform(timeout => inbox.waitFor({ state: 'visible', timeout }));
  await perform(timeout => page.waitForFunction(() => ['request', 'actions', 'error'].includes(document.querySelector('[data-testid="liveboard-notifications"]')?.dataset.state), null, { timeout }));
  const state = await perform(() => inbox.getAttribute('data-state'));
  if (state === 'error') throw new Error('Manual04 error must be explicitly dismissed before inspecting the preserved request');
  if (state === 'request') await (budget ? budget.click(inbox.locator('button.notification-hit')) : inbox.locator('button.notification-hit').click());
  const actions = inbox.locator('.notification-actions');
  await perform(timeout => actions.waitFor({ state: 'visible', timeout }));
  const head = await perform(() => inbox.evaluate((node) => ({
    kind: node.querySelector('.notification-actions')?.dataset.requestKind,
    requestId: node.querySelector('.notification-actions')?.dataset.requestId,
    requester: node.querySelector('.request-device')?.textContent?.trim(),
    subject: node.querySelector('.request-subject')?.textContent?.trim() ?? '',
    count: Number(node.querySelector('.notification-count')?.textContent),
  })));
  validateIncomingHead(head, expected);
  const states = observed.get(page) ?? new Map(); states.set(`${head.kind}/${head.subject}`, head); observed.set(page, states);
  return { ...head, inbox, actions };
}

export function observedWebIncoming(page, kind, subject = '') {
  const head = observed.get(page)?.get(`${kind}/${subject}`);
  if (!head) throw new Error('Exact incoming request must be visibly observed before its terminal barrier');
  return head;
}

export async function waitWebIncomingGone(page, kind, subject, timeout = 120_000) {
  const previous = observedWebIncoming(page, kind, subject);
  if (previous.terminal) return; // SAME identity already crossed the strict visible terminal boundary.
  await openWebLiveboard(page, timeout);
  await page.waitForFunction(({ id, count }) => {
    const inbox = document.querySelector('[data-testid="liveboard-notifications"]');
    if (!inbox || ['sending', 'error'].includes(inbox.dataset.state)) return false;
    const current = inbox.querySelector('.notification-actions')?.dataset.requestId;
    return current !== id && Number(inbox.querySelector('.notification-count')?.textContent ?? '0') < count;
  }, { id: previous.requestId, count: previous.count }, { timeout });
  const inbox = page.getByTestId('liveboard-notifications');
  // A count drop cannot by itself prove which request disappeared. If privacy/
  // reconciliation collapsed04, inspect the actual new head before accepting it.
  if (await inbox.locator('.notification-count').count()) {
    await page.waitForFunction(() => {
      const inbox = document.querySelector('[data-testid="liveboard-notifications"]');
      return inbox?.querySelector('.notification-actions') || inbox?.dataset.state === 'request';
    }, null, { timeout });
    if (await inbox.getAttribute('data-state') === 'request') await inbox.locator('button.notification-hit').click();
    await inbox.locator('.notification-actions').waitFor({ state: 'visible', timeout });
  }
  const current = await inbox.evaluate((node) => ({ state: node.dataset.state, count: Number(node.querySelector('.notification-count')?.textContent ?? '0'), requestId: node.querySelector('.notification-actions')?.dataset.requestId ?? null }));
  if (!validateIncomingRemoval(previous, current)) throw new Error('Queue count changed without the exact observed request disappearing');
  previous.terminal = true;
}

export async function waitWebIncomingActionOrGone(page, kind, subject, timeout = 120_000) {
  const previous = observedWebIncoming(page, kind, subject);
  await openWebLiveboard(page, timeout);
  const result = await page.waitForFunction(({ id, count }) => {
    const inbox = document.querySelector('[data-testid="liveboard-notifications"]');
    if (!inbox || ['sending', 'error'].includes(inbox.dataset.state)) return false;
    const actions = inbox.querySelector('.notification-actions');
    if (actions?.dataset.requestId === id) return 'pending';
    const remaining = Number(inbox.querySelector('.notification-count')?.textContent ?? '0');
    if (remaining < count) return 'terminal';
    return actions ? 'foreign' : false;
  }, { id: previous.requestId, count: previous.count }, { timeout }).then((handle) => handle.jsonValue());
  if (result === 'foreign') throw new Error('Unsupported FIFO head while awaiting the exact first-response result');
  if (result === 'terminal') await waitWebIncomingGone(page, kind, subject, timeout);
  return result;
}

export async function decideWebIncoming(page, expected, accepted, { waitForGone = true, timeout = 120_000 } = {}) {
  const head = await openWebIncoming(page, expected, timeout);
  const action = head.actions.getByTestId(`${accepted ? 'approve' : 'decline'}-${head.kind}-request`);
  await action.click();
  if (waitForGone) await waitWebIncomingGone(page, head.kind, head.subject, timeout);
  return head.requestId;
}

export async function assertWebInboxEmpty(page, timeout = 30_000) {
  await openWebLiveboard(page, timeout);
  await page.waitForFunction(() => {
    const inbox = document.querySelector('[data-testid="liveboard-notifications"]');
    return inbox && !['sending', 'error'].includes(inbox.dataset.state) && !inbox.querySelector('.notification-count') && !inbox.querySelector('.notification-actions');
  }, null, { timeout });
}

export async function assertWebNoStaleIncomingKinds(page, kinds, timeout = 30_000) {
  await openWebLiveboard(page, timeout);
  const inbox = page.getByTestId('liveboard-notifications');
  const state = await inbox.getAttribute('data-state');
  if (state === 'error' || state === 'sending') throw new Error('Membership boundary retains an unresolved04 state');
  if (await inbox.locator('.notification-count').count() === 0) return;
  // The original membership check permits a legitimate pending Remove request.
  // With only one visible head, count>1 cannot prove absence of a hidden stale kind.
  const head = await openWebIncoming(page, { count: 1 }, timeout);
  if (kinds.includes(head.kind)) throw new Error('Membership boundary retains a stale Join/recovery request');
}

export async function approveWebJoin(page, requesterHint, timeout = 120_000) {
  // Setup joins are serialized by the existing native REQUEST_SENT barrier.
  // The hint is a platform label; discover the canonical requester from real04.
  const head = await openWebIncoming(page, { kind: 'join', count: 1 }, timeout);
  await decideWebIncoming(page, { kind: 'join', requestId: head.requestId, requester: head.requester, count: 1 }, true, { timeout });
  const member = page.locator('.device-slot[data-device-id]').filter({ has: page.locator('.device-name').filter({ hasText: new RegExp(`^${head.requester.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}$`) }) });
  await member.waitFor({ state: 'visible', timeout });
  if (await member.count() !== 1) throw new Error(`Approved ${requesterHint} requester is not one canonical connected slot`);
  return head.requester;
}
