import { test, expect } from '@playwright/test';

test('UDP upstream idle timeout validates and saves a profile-owned runtime edit', async ({ page }) => {
  await page.goto('/?panel=runtime-network', { waitUntil: 'domcontentloaded' });
  const input = page.getByRole('spinbutton', { name: 'UDP 上游空闲时间' });
  await expect(input).toHaveValue('30');
  await input.fill('0');
  await page.getByRole('button', { name: '应用', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('大于 0');
  await expect(page.getByLabel('保存结果')).toBeEmpty();
  await input.fill('90');
  await page.getByRole('button', { name: '应用', exact: true }).click();
  await expect(page.getByLabel('保存结果')).toContainText('"runtime.udpUpstreamIdleTimeoutSeconds":90');
  await expect(page.getByRole('button', { name: '已应用', exact: true })).toBeVisible();
});
