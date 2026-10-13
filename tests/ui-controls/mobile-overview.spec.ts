import { test, expect } from '@playwright/test';

test('actual mobile overview has one first-screen statistics entry and opens global Flow', async ({ page }) => {
  await page.setViewportSize({ width: 320, height: 640 });
  await page.goto('/mobile.html?fixture=overview&traffic-v1');
  await expect(page.getByRole('button', { name: '统计', exact: true })).toHaveCount(1);
  const button = (await page.getByRole('button', { name: '统计', exact: true }).boundingBox())!;
  expect(button.y).toBeLessThan(100); expect(button.width).toBeGreaterThanOrEqual(44); expect(button.height).toBeGreaterThanOrEqual(44);
  await expect(page.getByRole('button', { name: '开启 VPN', exact: true })).toBeDisabled();
  await expect(page.locator('.vpn-feedback')).toContainText('VPN 适配待完成');
  await page.getByRole('button', { name: '统计', exact: true }).click();
  await expect(page.locator('.secondary-header h1')).toHaveText('流量统计');
  await expect(page.getByRole('textbox', { name: '搜索统计范围' })).toHaveValue('');
  await expect(page.locator('.scope')).toHaveCount(1);
  await expect(page.locator('.scope')).toContainText('全局');
  await expect(page.locator('.plane-controls')).toContainText('Flow');
  await page.getByRole('button', { name: '返回', exact: true }).click();
  await expect(page.getByRole('button', { name: '统计', exact: true })).toBeVisible();
  await expect(page.getByRole('navigation', { name: '主导航' }).getByRole('button', { name: '概览' })).toHaveAttribute('aria-current', 'page');
});

test('rejected actual overview mode submission preserves confirmed selection', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 720 });
  await page.goto('/mobile.html?fixture=overview&traffic-v1&global-failure');
  const rule = page.getByRole('button', { name: '规则', exact: true });
  await expect(rule).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByRole('button', { name: '直连', exact: true })).toBeEnabled();
  await page.getByRole('button', { name: '直连', exact: true }).click();
  await expect(page.getByRole('status').filter({ hasText: '全局出口应用失败' })).toBeVisible();
  await expect(rule).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByRole('button', { name: '直连', exact: true })).toHaveAttribute('aria-pressed', 'false');
});

test('actual overview preserves u64 cumulative data and shows unknown rates after observation gap', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 720 });
  await page.goto('/mobile.html?fixture=overview&traffic-v1');
  await expect(page.locator('#traffic-fixture')).toBeAttached();
  await expect(page.locator('.cumulative')).toContainText('累计上传 2.0 KB');
  await page.evaluate(() => window.dispatchEvent(new CustomEvent('fixture-traffic-step', { detail: { ms: 1000, bytes: '9007199254740993' } })));
  await expect(page.locator('.cumulative')).toContainText('累计上传 8.0 PB');
  await expect(page.locator('.session-total')).toHaveText('—');
  await page.evaluate(() => window.dispatchEvent(new CustomEvent('fixture-traffic-recovery', { detail: { delay: 2000, error: true } })));
  await expect(page.locator('.rates strong')).toHaveText(['—', '—']);
  await expect(page.locator('.cumulative')).toContainText('累计下载 8.0 PB');
  await expect(page.getByText(/累计快照采样于/)).toBeVisible();
});


test('actual overview shortcuts mount the target bottom tab', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 720 });
  await page.goto('/mobile.html?fixture=overview&traffic-v1');
  await expect(page.getByRole('button', { name: /当前配置/ })).toContainText('日常网络配置');
  await page.getByRole('button', { name: /当前配置/ }).click();
  await expect(page.getByRole('textbox', { name: '搜索配置' })).toBeVisible();
  await expect(page.getByRole('navigation', { name: '主导航' }).getByRole('button', { name: '配置' })).toHaveAttribute('aria-current', 'page');
});
