import { test, expect } from '@playwright/test';

test('WireGuard DNS detour is editable and persists as a global client DNS edit', async ({ page }) => {
  await page.goto('/?panel=dns', { waitUntil: 'domcontentloaded' });
  const save = page.getByRole('button', { name: '保存并应用', exact: true });
  await expect(save).toBeEnabled();
  await expect(page.getByText('出站 wg-a 已不存在', { exact: false })).toHaveCount(0);
  await page.getByRole('button', { name: '编辑 wg-a-dns', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await dialog.getByRole('button', { name: 'DNS 上游经由出站' }).click();
  await expect(page.getByRole('option', { name: '出站 · wg-a (wireguard)', exact: true })).toBeVisible();
  await expect(page.getByRole('option', { name: '出站 · node-a (socks5)', exact: true })).toBeVisible();
  await expect(page.getByRole('option', { name: '策略组 · Proxy', exact: true })).toBeVisible();
  await expect(page.getByRole('option', { name: 'wg-a（已失效）', exact: true })).toHaveCount(0);
  await page.keyboard.press('Escape');
  await dialog.getByRole('spinbutton', { name: '端口', exact: true }).fill('5353');
  await dialog.getByRole('button', { name: '保存修改', exact: true }).click();
  await save.click();
  const saved = page.getByLabel('保存结果');
  await expect(saved).toContainText('"dns"');
  const result = JSON.parse((await saved.textContent())!);
  expect(result.changes.dns.config.servers['wg-a-dns']).toMatchObject({ detour: 'wg-a', port: 5353 });
  expect(result.changes.dns.config.dispatch[0].server).toBe('wg-a-dns');
  expect(Object.keys(result.changes)).toEqual(['dns']);
});

test('missing concrete detours still block save', async ({ page }) => {
  await page.goto('/?panel=dns&mode=missing-target', { waitUntil: 'domcontentloaded' });
  await expect(page.getByText('servers.wg-a-dns.detour：出站 wg-a 已不存在或未进入活动配置')).toBeVisible();
  await expect(page.getByRole('button', { name: '保存并应用', exact: true })).toBeDisabled();
});

test('unavailable target inventory does not misreport an existing detour as deleted', async ({ page }) => {
  await page.goto('/?panel=dns&mode=target-read-failure', { waitUntil: 'domcontentloaded' });
  await expect(page.getByRole('button', { name: '保存并应用', exact: true })).toBeEnabled();
  await expect(page.getByText('出站 wg-a 已不存在', { exact: false })).toHaveCount(0);
});
