import { test, expect } from '@playwright/test';

test('four fixed mobile tabs preserve search and scroll at 320px and large text', async ({ page }) => {
  await page.setViewportSize({ width: 320, height: 640 });
  await page.goto('/mobile.html');
  const nav = page.getByRole('navigation', { name: '主导航' });
  await expect(nav.getByRole('button')).toHaveCount(4);
  await expect(nav).toHaveText(/概览.*节点.*配置.*更多/);
  await expect(page.locator('.mobile-shell')).toHaveAttribute('data-platform', 'desktop');
  await expect(page.locator('.mobile-shell')).toHaveAttribute('data-responsive-preview', 'true');
  await page.getByRole('textbox', { name: '搜索概览' }).fill('保留输入');
  await page.locator('.overview-root').evaluate(element => element.scrollTop = 230);
  await nav.getByRole('button', { name: '节点' }).click();
  await page.getByRole('textbox', { name: '搜索节点' }).fill('节点筛选');
  await nav.getByRole('button', { name: '概览' }).click();
  await expect(page.getByRole('textbox', { name: '搜索概览' })).toHaveValue('保留输入');
  expect(await page.locator('.tab-pane:not(.inactive) .overview-root').evaluate(element => element.scrollTop)).toBe(230);
  await page.evaluate(() => document.documentElement.style.fontSize = '22px');
  expect(await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth)).toBe(0);
  for (const button of await nav.getByRole('button').all()) {
    const box = (await button.boundingBox())!;
    expect(box.height).toBeGreaterThanOrEqual(44); expect(box.width).toBeGreaterThanOrEqual(44);
  }
  const box = (await nav.boundingBox())!; expect(box.y + box.height).toBeLessThanOrEqual(640);
});

test('more keeps PRD order and secondary pages return to their originating bottom tab', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 720 });
  await page.goto('/mobile.html');
  const nav = page.getByRole('navigation', { name: '主导航' });
  await nav.getByRole('button', { name: '更多' }).click();
  const menu = page.locator('.mobile-menu');
  await expect(menu.getByRole('button')).toHaveText(['端点', '流量统计', '连接', 'Packet 路径', '插件', '日志', '诊断', '内核管理', '客户端升级', '设置', '关于']);
  await menu.getByRole('button', { name: '设置', exact: true }).click();
  await expect(page.locator('.secondary-header h1')).toHaveText('设置');
  await expect(nav.getByRole('button')).toHaveCount(4);
  await expect(page.getByRole('button', { name: 'VPN 与接管', exact: true })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('heading', { name: '更多', exact: true })).toBeVisible();
  await nav.getByRole('button', { name: '概览' }).click();
  await page.getByRole('button', { name: '打开设置' }).click();
  await page.getByRole('button', { name: '返回', exact: true }).click();
  await expect(page.getByRole('textbox', { name: '搜索概览' })).toBeVisible();
  await expect(nav.getByRole('button', { name: '概览' })).toHaveAttribute('aria-current', 'page');
});

test('browser back closes one secondary page without changing the source tab', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 720 });
  await page.goto('/mobile.html');
  await page.getByRole('button', { name: '打开设置' }).click();
  await expect(page.locator('.secondary-header h1')).toHaveText('设置');
  await page.goBack();
  await expect(page.getByRole('textbox', { name: '搜索概览' })).toBeVisible();
});

test('Android and iOS presentation is detected independently of viewport width', async ({ browser }) => {
  for (const [platform, userAgent] of [
    ['android', 'Mozilla/5.0 (Linux; Android 7.0; SM-G950F) AppleWebKit/537.36 Chrome/120.0.0.0 Mobile Safari/537.36'],
    ['ios', 'Mozilla/5.0 (iPhone; CPU iPhone OS 15_0 like Mac OS X) AppleWebKit/605.1.15 Mobile/15E148'],
  ]) {
    const context = await browser.newContext({ userAgent, viewport: { width: 900, height: 700 } });
    const page = await context.newPage(); await page.goto('/mobile.html');
    await expect(page.locator('.mobile-shell')).toHaveAttribute('data-platform', platform);
    await expect(page.locator('.mobile-shell')).toHaveAttribute('data-responsive-preview', 'false');
    await context.close();
  }
});

test('kernel page separates bundled policy from missing installed mobile artifact', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 720 });
  await page.goto('/mobile.html');
  await page.getByRole('navigation', { name: '主导航' }).getByRole('button', { name: '更多' }).click();
  await page.getByRole('button', { name: '内核管理', exact: true }).click();
  await expect(page.locator('.version-page')).toContainText('计划随应用分发');
  await expect(page.locator('.version-page')).toContainText('随包内核产物未接入');
  await expect(page.locator('.version-page')).toContainText('适配待完成');
  await expect(page.getByRole('button', { name: '通过客户端升级更新内核' })).toBeVisible();
});

test('native back consumes a secondary route and lets an unguarded root finish', async ({page}) => {
  await page.goto('/mobile.html');
  const dispatch = () => page.evaluate(() => window.dispatchEvent(new CustomEvent('mobile:back',{cancelable:true})));
  expect(await dispatch()).toBe(true);
  await page.getByRole('button',{name:'打开设置'}).click();
  await expect(page.locator('.secondary-header h1')).toHaveText('设置');
  expect(await dispatch()).toBe(false);
  await expect(page.getByRole('textbox',{name:'搜索概览'})).toBeVisible();
  expect(await dispatch()).toBe(true);
});
