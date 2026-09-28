import { test, expect } from '@playwright/test';

test('v0.0.2 without peer health stays usable and does not invent device state', async ({ page }) => {
  await page.goto('/?panel=capabilities&version=v002');
  await expect(page.getByText('稳定契约', { exact: true })).toBeVisible();
  await expect(page.getByRole('table', { name: '隧道设备状态' })).toHaveCount(0);
  await expect(page.getByText('隧道设备状态暂不可用', { exact: false })).toHaveCount(0);
});

test('v0.0.3 displays peer state independently from protocol support and clears stale health', async ({ page }) => {
  await page.goto('/?panel=capabilities&fail-health');
  const table = page.getByRole('table', { name: '隧道设备状态' });
  await expect(table).toContainText('wg-a / 1');
  await expect(table).toContainText('等待握手');
  await expect(table).toContainText('wg-b / 2');
  await expect(table).toContainText('近期收到认证数据');
  await expect(table).toContainText('最近端点解析失败');
  await expect(table).toContainText('不足 1 秒前');
  await expect(page.getByText('wireguard', { exact: true })).toHaveAttribute('title', 'gotatun_v0.9.2');
  await expect(page.getByText('实验', { exact: true })).toBeVisible();
  for (const width of [1280, 900, 390]) {
    await page.setViewportSize({ width, height: 900 });
    expect(await page.locator('body').evaluate(element => element.scrollWidth <= element.clientWidth + 1)).toBe(true);
  }
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.screenshot({ path: '/tmp/znet-kernel-compatibility-health.png', fullPage: true });
  await page.getByRole('button', { name: '刷新', exact: true }).click();
  await expect(table).toHaveCount(0);
  await expect(page.getByText('隧道设备状态暂不可用', { exact: false })).toContainText('connection_closed');
  await expect(page.getByText('wireguard', { exact: true })).toBeVisible();
});

test('an opt-in protocol not compiled into the kernel is shown explicitly', async ({ page }) => {
  await page.goto('/?panel=capabilities&compiled=false');
  const protocol = page.getByRole('row').filter({ has: page.getByText('wireguard', { exact: true }) });
  await expect(protocol).toContainText('实验');
  await expect(protocol.getByRole('cell').nth(2)).toHaveText('否');
  await expect(protocol.getByRole('cell').nth(3)).toHaveText('—');
});
