import { test, expect } from '@playwright/test';

test('nested policy selection agrees between overview, selector and confirmed details', async ({ page }) => {
  await page.goto('/?panel=overview&mode=nested-policy');
  const shortcut = page.getByRole('button', { name: '查看与切换策略组' });
  await expect(shortcut).toContainText('自动选择 → 新加坡 01');
  await expect(shortcut).toContainText('38 ms');
  await shortcut.click();
  await expect(page.getByRole('button', { name: '节点选择 当前出口' })).toContainText('自动选择 → 新加坡 01 · 38 ms');
  await expect(page.getByRole('dialog')).toContainText('已确认：自动选择 → 新加坡 01 · 最近探测成功');
});

test('professional overview keeps capture controls and one traffic chart with useful details', async ({ page }) => {
  await page.goto('/?panel=overview');
  await expect(page.getByRole('button', { name: '当前配置', exact: true })).toContainText('日常网络配置');
  await expect(page.getByText('实时速率', { exact: true })).toHaveCount(1);
  await expect(page.getByRole('switch', { name: '关闭 TUN 并取消自动恢复' })).toBeChecked();
  await page.getByRole('button', { name: '查看 TUN 接管详情' }).click();
  await expect(page.getByRole('dialog')).toContainText('MTU 1500');
  await page.getByRole('dialog').getByRole('button', { name: '相关设置' }).click();
  await expect(page.getByLabel('概览操作')).toHaveText('tun');
  await page.getByRole('button', { name: '自测通过' }).click();
  await expect(page.getByRole('dialog')).toContainText('控制接口已响应');
});

test('automatic policy groups are read only while selector changes wait for confirmation', async ({ page }) => {
  await page.goto('/?panel=overview');
  await page.getByRole('button', { name: '查看与切换策略组' }).click();
  await expect(page.getByRole('button', { name: '自动选择 当前出口' })).toHaveCount(0);
  const select = page.getByRole('button', { name: '工作网络 当前出口' });
  await select.click(); await page.getByRole('option', { name: '备用节点 · 52 ms', exact: true }).click();
  await expect(select).toBeDisabled();
  await expect(select).toContainText('日本 02');
  await expect(select).toBeEnabled();
  await expect(select).toContainText('备用节点');
  await page.getByRole('button', { name: '节点与测速' }).click();
  await expect(page.getByLabel('概览操作')).toHaveText('nodes');
});

test('unhealthy capture exposes the actual error and an independent disable action', async ({ page }) => {
  await page.goto('/?panel=overview&mode=failure');
  await expect(page.getByRole('region', { name: '需要处理' })).toContainText('默认路由已变化，出口恢复失败');
  await expect(page.getByRole('region', { name: 'TUN 与网络栈' })).toContainText('已开启 · 异常');
  await page.getByRole('button', { name: '关闭 TUN', exact: true }).click();
  await expect(page.getByRole('switch', { name: '开启 TUN', exact: true })).not.toBeChecked();
  await expect(page.getByRole('button', { name: '关闭系统代理', exact: true })).toBeVisible();
});

test('failed profile, mode and TUN requests preserve confirmed control values', async ({ page }) => {
  await page.goto('/?panel=overview&mode=action-failure');
  const profile = page.getByRole('button', { name: '当前配置', exact: true });
  await profile.click(); await page.getByRole('option', { name: '工作配置', exact: true }).click();
  await expect(profile).toBeDisabled(); await expect(profile).toContainText('日常网络配置');
  await expect(page.getByLabel('应用提示').getByRole('alert')).toContainText('内核拒绝');
  await expect(profile).toContainText('日常网络配置');
  await page.getByRole('button', {name:'关闭提示'}).click();
  await page.getByRole('radio', { name: '全局', exact: true }).click();
  await expect(page.getByRole('radio', { name: '规则', exact: true })).toBeChecked();
  await expect(page.getByLabel('应用提示').getByRole('alert')).toContainText('内核拒绝');
  await page.getByRole('button', {name:'关闭提示'}).click();
  await page.getByRole('switch', { name: '关闭 TUN 并取消自动恢复' }).click();
  await expect(page.getByLabel('应用提示').getByRole('alert')).toContainText('内核拒绝');
  await expect(page.getByRole('switch', { name: '关闭 TUN 并取消自动恢复' })).toBeChecked();
});

test('stale state protects traffic and mode changes while preserving managed restart and capture cleanup', async ({ page }) => {
  await page.goto('/?panel=overview&mode=stale');
  await expect(page.getByText('流量采样已过期，等待恢复')).toBeVisible();
  await expect(page.getByRole('region', { name: 'TUN 与网络栈' })).toContainText('状态待确认');
  await expect(page.getByRole('button', { name: '重启内核' })).toBeEnabled();
  await page.getByRole('button', { name: '重启内核' }).click();
  await expect(page.getByLabel('概览操作')).toHaveText('restart');
  await expect(page.getByRole('radio', { name: '全局', exact: true })).toBeDisabled();
  await expect(page.getByRole('button', { name: '关闭系统代理' })).toBeEnabled();
  await expect(page.getByRole('button', { name: '自测通过' })).toHaveCount(0);
});

test('real core card blocks duplicate operations and never restarts an external kernel', async ({ page }) => {
  await page.goto('/?panel=overview&mode=busy');
  await expect(page.getByRole('button', { name: '重启内核' })).toBeDisabled();
  await page.goto('/?panel=overview&mode=external');
  await expect(page.getByRole('button', { name: '外部内核' })).toBeDisabled();
});

test('empty configuration and missing self test stay actionable without green success claims', async ({ page }) => {
  await page.goto('/?panel=overview&mode=empty');
  await expect(page.getByRole('button', { name: '当前配置', exact: true })).toBeDisabled();
  await expect(page.getByRole('button', { name: '自测待确认' })).toBeVisible();
  await page.getByRole('button', { name: '添加代理配置 →' }).click();
  await expect(page.getByLabel('概览操作')).toHaveText('profiles');
});

test('overview fits narrow and dark layouts without horizontal clipping', async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 640, height: 850 });
  await page.goto('/?panel=overview');
  await page.getByRole('button', { name: '切换主题' }).click();
  await expect(page.getByRole('button', { name: '查看与切换策略组' })).toBeVisible();
  const overflow = await page.getByLabel('专业运行概览').evaluate(element => element.scrollWidth > element.clientWidth + 1);
  expect(overflow).toBe(false);
  await page.getByLabel('专业运行概览').screenshot({ path: testInfo.outputPath('overview-dark.png') });
});

test('IPv4 preferred without IPv6 keeps a neutral capability detail and no alert banner', async ({page}) => {
  await page.goto('/?panel=overview&mode=ipv4-only-network');
  await expect(page.getByRole('region',{name:'需要处理'})).toHaveCount(0);
  await expect(page.locator('.egress-summary')).toHaveText('IPv4 出口可用');
  await expect(page.getByRole('button',{name:'自测通过'})).toBeVisible();
  await page.getByRole('button',{name:'查看 TUN 接管详情'}).click();
  await expect(page.getByRole('dialog')).toContainText('IPv6');
  await expect(page.getByRole('dialog')).toContainText('不可用');
  await page.goto('/?panel=overview&mode=ipv6-required');
  await expect(page.getByRole('region',{name:'需要处理'})).toContainText('IPv6 出口不可用');
});

test('missing self test uses neutral styling while awaiting evidence', async ({page}) => {
  await page.goto('/?panel=overview&mode=empty');
  const check=page.getByRole('button',{name:'自测待确认'});
  await expect(check).not.toHaveClass(/warning|passed/);
});

for (const failed of [false,true]) test(`system proxy feedback never moves cards or traffic on ${failed ? 'failure' : 'success'}`, async ({page}) => {
  await page.clock.install();
  await page.goto(`/?panel=overview${failed ? '&mode=action-failure' : ''}`);
  const geometry = () => page.locator('.control-grid,.core-card,.mode-card,.feature-card,.traffic-panel').evaluateAll(elements => elements.map(element => {
    const {x,y,width,height}=element.getBoundingClientRect(); return {x,y,width,height};
  }));
  await page.evaluate(() => document.fonts.ready);
  const before=await geometry();
  const unchanged = async () => {
    const after=await geometry();expect(after.length).toBe(before.length);
    after.forEach((rect,i) => { for (const key of ['x','y','width','height'] as const) expect(Math.abs(rect[key]-before[i][key]), JSON.stringify({index:i,key,before:before[i],after:rect})).toBeLessThanOrEqual(1); });
  };
  await page.getByRole('button',{name:'关闭系统代理',exact:true}).click();
  await expect(page.getByRole('button',{name:'切换中',exact:true})).toBeDisabled();
  await unchanged();
  await page.clock.runFor(600);
  await expect(page.getByLabel('应用提示').locator('.toast-item')).toHaveCount(1);
  await expect(page.getByLabel('应用提示')).toContainText(failed ? '内核拒绝' : '设置已生效');
  await expect(page.getByRole('button',{name:failed ? '关闭系统代理' : '开启系统代理',exact:true})).toBeEnabled();
  await unchanged();
  await page.clock.runFor(4200);
  await expect(page.getByLabel('应用提示')).toHaveCount(0);
  await unchanged();
});


test('manual network recovery keeps TUN enabled and does not restart the kernel', async ({page}) => {
  await page.goto('/?panel=overview&mode=failure');
  const retry = page.getByRole('button', {name:'立即重试 TUN 网络恢复'});
  await retry.click();
  await expect(retry).toBeDisabled();
  await expect(page.getByLabel('应用提示')).toContainText('设置已生效');
  await expect(page.getByRole('switch', {name:'关闭 TUN 并取消自动恢复'})).toBeChecked();
  await expect(page.getByLabel('TUN 与网络栈').locator('.feature-main')).toContainText('已开启 · 健康');
});
