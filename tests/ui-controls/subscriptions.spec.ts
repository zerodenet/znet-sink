import { expect, test } from '@playwright/test';

test('managed subscription card shows aggregate quota, ratio, expiry and ownership', async ({ page }) => {
  await page.goto('/?panel=subscriptions&mode=managed-usage');

  await expect(page.getByText('狗梯 / Developer Licenses', { exact: true })).toBeVisible();
  await expect(page.getByText('由 Connect 托管', { exact: true })).toBeVisible();
  await expect(page.getByText('来源: 狗梯', { exact: true })).toBeVisible();
  await expect(page.getByText(/已用 375 B \/ 总量 1000 B/)).toBeVisible();
  await expect(page.getByText('37.5%', { exact: true })).toBeVisible();
  await expect(page.getByText(/到期:/)).toBeVisible();
  await expect(page.getByText(/剩 \d+ 天/)).toBeVisible();
  await expect(page.getByRole('button', { name: '由来源插件刷新' })).toBeDisabled();
});

test('mounted subscription page refreshes the sync time on background completion', async ({ page }) => {
  await page.goto('/?panel=subscriptions&mode=managed-usage');
  const syncTime = page.getByText(/^同步:/);
  await expect(syncTime).toBeVisible();
  const before = await syncTime.textContent();
  await page.evaluate(() => {
    window.dispatchEvent(new CustomEvent('fixture-subscription-synced', { detail: 1790568600000 }));
    window.dispatchEvent(new CustomEvent('subscriptions:updated', { detail: { subscriptionId:'managed-connect' } }));
  });
  await expect(syncTime).not.toHaveText(before!);
  await expect(page.getByText(/已用 375 B \/ 总量 1000 B/)).toBeVisible();
});
