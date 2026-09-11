import { test, expect } from '@playwright/test';

for (const width of [800, 1280, 1920]) {
  test(`CIDR descriptions retain readable width at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 1000 });
    await page.goto('/?panel=tun');
    for (const name of ['TUN 接管网段']) {
      const editor = page.getByRole('textbox', { name });
      const label = editor.locator('..').locator('.config-row-label');
      const a = await label.boundingBox();
      const b = await editor.boundingBox();
      expect(a).not.toBeNull(); expect(b).not.toBeNull();
      expect(a!.width).toBeGreaterThan(240);
      expect(b!.y).toBeGreaterThanOrEqual(a!.y + a!.height);
      expect(Math.abs(a!.width - b!.width)).toBeLessThan(2);
    }
  });
}

test.beforeEach(async ({ page }) => {
  await page.route('**/*', (route) => {
    if (new URL(route.request().url()).hostname === '127.0.0.1') return route.continue();
    throw new Error(`Unexpected external request: ${route.request().url()}`);
  });
});

test('running TUN settings can be edited and applied without manual toggling', async ({ page }) => {
  await page.goto('/?panel=tun');
  const exclusions = page.getByRole('textbox', { name: 'TUN 接管网段' });
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
  await page.getByRole('textbox', { name: 'TUN 接管网段' }).fill('203.0.113.10/32');
  await page.getByRole('button', { name: '保存并应用' }).click();
  await expect(page.getByRole('alert')).toContainText('已恢复旧 TUN 配置');
  await expect(page.getByRole('button', { name: '已保存' })).toHaveCount(0);
  await expect(page.getByRole('button', { name: '保存并应用' })).toBeEnabled();
  await expect(page.getByLabel('保存结果')).toBeEmpty();
});

test('client TUN settings stay authoritative with a legacy profile observation', async ({ page }) => {
  await page.goto('/?panel=tun&mode=profile');
  await expect(page.getByText('下方内容仅作为 ZNet-Sink 缺省值', { exact: false })).toHaveCount(0);
  await expect(page.getByRole('textbox', { name: 'TUN 接管网段' })).toBeEnabled();
  await expect(page.getByText('保存后会自动重建 TUN', { exact: false })).toBeVisible();
});

test('TUN exposes one link to the shared bypass policy instead of a second editor', async ({page}) => {
  await page.goto('/?panel=tun');
  await expect(page.getByRole('textbox', {name:'TUN 排除网段'})).toHaveCount(0);
  await expect(page.getByRole('button', {name:'管理绕过规则'})).toBeVisible();
});
