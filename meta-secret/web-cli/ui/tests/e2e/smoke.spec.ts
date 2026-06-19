import { test, expect } from '@playwright/test';

// Public routes that don't require authentication
const publicRoutes = [
  { path: '/info', name: 'information page' },
  { path: '/settings', name: 'settings page' },
  { path: '/tools/split', name: 'split tool' },
  { path: '/tools/recover', name: 'recover tool' },
  { path: '/tools/docs', name: 'documentation' },
  { path: '/contact', name: 'contact page' },
];

test('home page loads without JS errors', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (err) => errors.push(err.message));

  await page.goto('/');
  await page.waitForLoadState('networkidle');

  expect(errors).toHaveLength(0);
  await expect(page.locator('body')).toBeVisible();
});

for (const route of publicRoutes) {
  test(`${route.name} loads`, async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', (err) => errors.push(err.message));

    await page.goto(route.path);
    await page.waitForLoadState('networkidle');

    expect(errors).toHaveLength(0);
    await expect(page.locator('body')).toBeVisible();
    // No 404 page
    await expect(page).not.toHaveURL(/\/404/);
  });
}

test('404 page renders for unknown route', async ({ page }) => {
  await page.goto('/this-does-not-exist');
  await page.waitForLoadState('networkidle');
  await expect(page).toHaveURL(/\/404/);
  await expect(page.locator('body')).toBeVisible();
});
