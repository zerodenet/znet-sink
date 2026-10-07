import { test, expect } from '@playwright/test';

test('rule mode exposes WireGuard outbounds that belong to no policy group', async ({ page }) => {
  await page.goto('/?panel=nodes&endpoints=wireguard');
  const sidebar = page.locator('.group-sidebar');
  const endpoints = sidebar.getByRole('button', {name:'WireGuard 端点',exact:true});
  await expect(endpoints).toBeVisible();
  await expect(endpoints.locator('.group-count')).toHaveText('2');
  await expect(page.locator('.node-panel').getByText('wg-a',{exact:true})).toHaveCount(0);
  await endpoints.click();
  await expect(page.locator('.grid-card-name')).toHaveText(['wg-a','wg-multi']);
  await expect(page.locator('.wireguard-endpoint')).toHaveText(['本机 IP 10.0.0.2/32 · fd00::2/128','本机 IP 10.1.0.2/32']);
  await expect(page.locator('.grid-card').first()).toBeDisabled();
  await page.screenshot({path:test.info().outputPath('wireguard-endpoints.png')});
  await expect(page.getByLabel('保存结果')).toBeEmpty();
  await page.getByRole('radio', {name:'列表视图'}).click();
  await expect(page.locator('.node-name')).toHaveText(['wg-a','wg-multi']);
  await expect(page.locator('.wireguard-endpoint').first()).toHaveText('本机 IP 10.0.0.2/32 · fd00::2/128');
  await expect(page.locator('.node-main').first()).toBeDisabled();
  await sidebar.getByRole('button', {name:/全部节点/}).click();
  await expect(page.locator('.node-name')).toHaveCount(4);
  await expect(sidebar.locator('.group-item.active')).toContainText('全部节点');
  await sidebar.getByRole('button', {name:/proxy/}).click();
  await expect(page.locator('.node-name')).toHaveText(['node-a','node-b']);
});

test('WireGuard remains selectable only when it is a declared selector member', async ({ page }) => {
  await page.goto('/?panel=nodes&endpoints=wireguard&mode=wg-in-group');
  const card = page.locator('.grid-card').filter({has:page.getByText('wg-a',{exact:true})});
  await expect(card).toBeEnabled();
  await card.click();
  await expect(page.getByLabel('保存结果')).toContainText('"selected":"wg-a"');
  await page.getByRole('button',{name:'WireGuard 端点',exact:true}).click();
  await expect(page.locator('.grid-card').first()).toBeDisabled();
});

test('older configurations retain normal nodes without a fabricated endpoint entry', async ({ page }) => {
  await page.goto('/?panel=nodes&endpoints=wireguard&version=v002');
  await expect(page.locator('.grid-card-name')).toHaveText(['node-a','node-b']);
  await expect(page.getByRole('button',{name:'WireGuard 端点',exact:true})).toHaveCount(0);
});

test('missing local tunnel addresses do not fall back to a remote peer address', async ({ page }) => {
  await page.goto('/?panel=nodes&endpoints=wireguard&no-local-address=1');
  await page.getByRole('button', { name: 'WireGuard 端点', exact: true }).click();
  await expect(page.locator('.wireguard-endpoint')).toHaveText(['本机 IP —', '本机 IP —']);
  await expect(page.locator('.wireguard-endpoint').first()).toHaveAttribute('title', '配置中未提供本机隧道地址');
  await expect(page.locator('.node-panel')).not.toContainText('2001:db8::1');
});
