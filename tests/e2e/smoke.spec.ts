import { test, expect } from '@playwright/test';

test('mounts standard view and avoids error boundaries', async ({ page }) => {
  await page.goto('/');

  // Expect fundamental DOM availability
  await expect(page.locator('body')).not.toBeEmpty();
  
  // Verify that an uncaught error didn't force the Error Boundary to take over
  const boundaryText = page.getByText(/Aletheia Encountered a System Error/i);
  await expect(boundaryText).toHaveCount(0);
});
