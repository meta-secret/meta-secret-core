// Lifecycle mutations begin only after the previous cycle's actors settle.
// Recycle before the next Secret is created, while the existing name set is
// still authoritative; retain the established long-run boundaries as well.
export function webPageRefreshCycles(recovery = {}) {
  const boundary = recovery.createSecret3AfterCycle;
  const phases = recovery.webPageRefreshAfterCycles ?? [];
  if (!Array.isArray(phases) || phases.some((cycle) => !Number.isInteger(cycle) || cycle <= 0)) {
    throw new Error('Web refresh phase boundaries must be positive completed cycle numbers');
  }
  return [...new Set([
    21, 30,
    ...phases,
    ...(Number.isInteger(boundary) && boundary > 0 ? [boundary] : []),
  ])].sort((left, right) => left - right);
}

// Replace only a completed scenario boundary. Keep browser-context storage
// and migrate the target-scoped authenticator before closing the old target.
export async function recycleWebPage({ page, configurePage, navigatePage, cloneAuthenticator, unlockPage, waitUntilReady }) {
  const nextPage = await page.context().newPage();
  try {
    await configurePage(nextPage);
    await navigatePage(nextPage);
    const authenticator = await cloneAuthenticator(nextPage);
    await unlockPage(nextPage);
    await waitUntilReady(nextPage);
    await page.close();
    return { page: nextPage, authenticator };
  } catch (error) {
    // Keep the old page/authenticator authoritative if replacement fails.
    await nextPage.close().catch(() => {});
    throw error;
  }
}
