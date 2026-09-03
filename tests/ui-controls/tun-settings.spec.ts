import { test, expect } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.route('**/*', (route) => {
    if (new URL(route.request().url()).hostname === '127.0.0.1') return route.continue();
    throw new Error(`Unexpected external request: ${route.request().url()}`);
  });
});

test('running TUN settings can be edited and applied without manual toggling', async ({ page }) => {
  await page.goto('/?panel=tun');
  const exclusions = page.getByRole('textbox', { name: 'TUN 排除网段' });
  await expect(exclusions).toBeEnabled();
  await expect(page.getByText('保存后会自动重建 TUN', { exact: false })).toBeVisible();
  await exclusions.fill('16.0.0.0/8\n203.0.113.10/32');
  await page.getByRole('button', { name: '保存并应用' }).click();
  await expect(page.getByRole('button', { name: '应用中...' })).toBeDisabled();
  await expect(exclusions).toBeDisabled();
  await expect(page.getByRole('button', { name: '已保存' })).toBeEnabled();
  await expect(page.getByLabel('保存结果')).toContainText('203.0.113.10/32');
  await expect(exclusions).toBeEnabled();
});

test('apply failure remains visible and does not report saved', async ({ page }) => {
  await page.goto('/?panel=tun&mode=failure');
  await page.getByRole('textbox', { name: 'TUN 排除网段' }).fill('203.0.113.10/32');
  await page.getByRole('button', { name: '保存并应用' }).click();
  await expect(page.getByRole('alert')).toContainText('已恢复旧 TUN 配置');
  await expect(page.getByRole('button', { name: '已保存' })).toHaveCount(0);
  await expect(page.getByRole('button', { name: '保存并应用' })).toBeEnabled();
  await expect(page.getByLabel('保存结果')).toBeEmpty();
});

test('profile ownership stays visible and local defaults remain editable', async ({ page }) => {
  await page.goto('/?panel=tun&mode=profile');
  await expect(page.getByText('下方内容仅作为 ZNet-Sink 缺省值', { exact: false })).toBeVisible();
  await expect(page.getByRole('textbox', { name: 'TUN 排除网段' })).toBeEnabled();
  await expect(page.getByText('保存后会自动重建 TUN', { exact: false })).toHaveCount(0);
});
