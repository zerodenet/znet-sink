import { test, expect } from '@playwright/test';

test('professional overview shows one control context and actionable navigation', async ({ page }) => {
  await page.goto('/?panel=overview');
  const overview = page.getByRole('region', { name: '当前运行上下文' });
  await expect(page.getByRole('heading', { name: '内核控制面就绪' })).toBeVisible();
  await expect(overview.getByText('0.0.16-rc.202609051609', { exact: true })).toHaveCount(1);
  await expect(page.getByText('实时速率', { exact: true })).toHaveCount(1);
  await page.getByRole('button', { name: '管理配置' }).click();
  await expect(page.getByLabel('概览操作')).toHaveText('profiles');
  await page.getByRole('button', { name: '节点与测速' }).click();
  await expect(page.getByLabel('概览操作')).toHaveText('nodes');
  await page.getByText('就绪检查明细', { exact: false }).click();
  await expect(page.getByText('控制接口已响应')).toBeVisible();
});

test('unhealthy capture is prominent and links to its settings', async ({ page }) => {
  await page.goto('/?panel=overview&mode=failure');
  await expect(page.getByRole('heading', { name: '需要处理运行异常' })).toBeVisible();
  const findings = page.getByRole('region', { name: '需要关注' });
  await expect(findings.getByText('TUN 已开启但不健康')).toBeVisible();
  await findings.getByRole('button', { name: '去处理' }).first().click();
  await expect(page.getByLabel('概览操作')).toHaveText('tun');
});

test('stale state does not claim active capture or display stale rates as live', async ({ page }) => {
  await page.goto('/?panel=overview&mode=stale');
  await expect(page.getByText('流量采样已过期，等待恢复')).toBeVisible();
  await expect(page.locator('.facts dd').filter({ hasText: '状态待确认' })).toHaveCount(2);
  await expect(page.getByRole('button', { name: '重启内核' })).toBeDisabled();
});

test('overview fits narrow and dark layouts without horizontal clipping', async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 640, height: 850 });
  await page.goto('/?panel=overview');
  await page.getByRole('button', { name: '切换主题' }).click();
  await expect(page.getByRole('heading', { name: '内核控制面就绪' })).toBeVisible();
  const overflow = await page.getByLabel('专业运行概览').evaluate((element) => element.scrollWidth > element.clientWidth + 1);
  expect(overflow).toBe(false);
  await page.getByLabel('专业运行概览').screenshot({ path: testInfo.outputPath('overview-dark.png') });
});
