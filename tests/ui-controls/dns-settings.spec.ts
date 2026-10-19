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

test('unavailable rule inventory reports an unknown reference instead of a deleted rule', async ({ page }) => {
  await page.goto('/?panel=dns&mode=rule-read-failure', { waitUntil: 'domcontentloaded' });
  await expect(page.getByText('规则集引用暂未确认：Rule inventory unavailable')).toBeVisible();
  await expect(page.getByText('以下 DNS 分流引用已失效：', { exact: false })).toHaveCount(0);
  await expect(page.getByRole('button', { name: '保存并应用', exact: true })).toBeEnabled();
  await page.getByRole('button', { name: '重试读取', exact: true }).click();
  await expect(page.getByText('以下 DNS 分流引用已失效：', { exact: false })).toHaveCount(0);
});

test('confirmed missing rules still block applying DNS configuration', async ({ page }) => {
  await page.goto('/?panel=dns&mode=rule-missing', { waitUntil: 'domcontentloaded' });
  await expect(page.getByText('以下 DNS 分流引用已失效：dler-domestic。请选择现有规则集或删除对应分流。')).toBeVisible();
  await expect(page.getByRole('button', { name: '保存并应用', exact: true })).toBeDisabled();
});

test('DNS dispatch can select client-managed and subscription rule sets and save their stable tags', async ({ page }) => {
  await page.goto('/?panel=dns&mode=client-dns-rules', { waitUntil: 'domcontentloaded' });
  await page.getByRole('button', { name: '新增 DNS 分流', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await dialog.getByRole('button', { name: 'DNS 分流条件类型' }).click();
  await page.getByRole('option', { name: '规则集', exact: true }).click();
  await dialog.getByRole('button', { name: 'DNS 分流规则集' }).click();
  await expect(page.getByRole('option', { name: '订阅媒体 · 订阅 · subscription-media', exact: true })).toBeVisible();
  await expect(page.getByRole('option', { name: '办公域名 · 本地 · gui-common-office', exact: true })).toBeVisible();
  await page.getByRole('option', { name: '中国大陆域名 · 内置 · gui-common-domestic', exact: true }).click();
  await expect(dialog.getByText('DNS 引用不依赖公共路由规则开关。', { exact: false })).toBeVisible();
  await expect(dialog.getByRole('button', { name: '管理规则集', exact: true })).toBeVisible();
  await dialog.getByRole('button', { name: '添加分流', exact: true }).click();
  await page.getByRole('button', { name: '保存并应用', exact: true }).click();
  const saved = JSON.parse((await page.getByLabel('保存结果').textContent())!);
  expect(saved.changes.dns.config.dispatch[1].condition).toEqual({ type: 'rule_set', tag: 'gui-common-domestic' });
  expect(Object.keys(saved.changes)).toEqual(['dns']);
});
