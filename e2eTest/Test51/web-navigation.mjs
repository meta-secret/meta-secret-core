/** Initial navigation only. Visible UI permits the existing Passkey gate; it is not authentication. */
export async function navigateTest51WebEntry(page, target, { timeout = 30_000 } = {}) {
  const expected = new URL(target);
  if (!['http:', 'https:'].includes(expected.protocol) ||
    !['localhost', '127.0.0.1', '[::1]'].includes(expected.hostname) ||
    expected.username || expected.password || expected.hash) throw new Error('Test51 navigation requires the exact configured loopback URL');
  const response = await page.goto(expected.href, { waitUntil: 'commit', timeout });
  if (!response || !response.ok()) throw new Error('Test51 initial document HTTP response failed');
  const exact = (url) => new URL(url).href === expected.href;
  if (!exact(response.url()) || !exact(page.url())) throw new Error('Test51 initial document URL changed unexpectedly');
  await page.getByRole('button', { name: 'Create Passkey', exact: true })
    .or(page.getByRole('button', { name: 'Authenticate with Passkey', exact: true }))
    .or(page.getByRole('region', { name: 'Vault registration', exact: true }))
    .first().waitFor({ state: 'visible', timeout });
  if (!exact(page.url())) throw new Error('Test51 document URL changed before authentication');
  return response;
}
