import { expect, test } from '@playwright/test';

test('history explains retention and exports the filter independently of loaded pages', async ({page}) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.addInitScript(() => {
    (window as any).__TAURI_INTERNALS__ = {
      invoke: async (command: string, args: unknown) => {
        if (command !== 'gui_connection_history_export') throw new Error(`unexpected command: ${command}`);
        (window as any).__historyExport = args;
        return { path: '/fixture/connection-records.json', records: 120 };
      },
    };
  });
  await page.goto('/?panel=connections');
  await page.getByRole('tab', {name: '连接记录', exact: true}).click();
  await expect(page.getByRole('button', {name: '查看连接 records.test:443'}).first()).toBeVisible();
  await page.getByText('记录范围与保留限制', {exact: false}).click();
  await expect(page.getByRole('note')).toContainText('缺失数量未知');
  await expect(page.getByRole('note')).toContainText('10,000');
  await page.getByRole('searchbox', {name: '搜索连接'}).fill('records.test');
  await page.getByLabel('记录开始时间').fill('2026-09-11T08:00');
  await page.waitForTimeout(350);
  await page.getByRole('button', {name: '导出筛选记录', exact: true}).click();
  await expect(page.getByText('/fixture/connection-records.json', {exact: true})).toBeVisible();
  const request = await page.evaluate(() => (window as any).__historyExport.query);
  expect(request.search).toBe('records.test');
  expect(request.capturedAfterMs).toBeGreaterThan(0);
  expect(request.beforeId).toBeUndefined();
  expect(request.limit).toBeUndefined();
  expect(errors).toEqual([]);
  await page.setViewportSize({width: 390, height: 650});
  await expect(page.getByRole('button', {name: '导出筛选记录', exact: true})).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.getByRole('button', {name: '查看连接 records.test:443'}).first().scrollIntoViewIfNeeded();
  await page.getByRole('button', {name: '查看连接 records.test:443'}).first().click();
  await expect(page.getByRole('dialog')).toBeVisible();
  await page.screenshot({path: '/tmp/znet-connection-records-mobile.png', fullPage: true});
});
