import { test, expect } from '@playwright/test';

test('actual Lite and Pro overviews share mode choice, active configuration and session totals', async ({ page }) => {
  await page.goto('/?panel=mode-overview');
  await expect(page.getByRole('status', {name:'代理运行状态'})).toContainText('系统代理与 TUN 已开启');
  await expect(page.getByText('日常网络配置', {exact:true})).toBeVisible();
  const total = await page.getByLabel('会话累计').textContent();
  await page.getByRole('radio', {name:'全局',exact:true}).click();
  await page.getByRole('button', {name:'专业视图',exact:true}).click();
  await expect(page.getByRole('radio', {name:'全局',exact:true})).toBeChecked();
  await page.getByRole('button', {name:'当前配置',exact:true}).click();
  await page.getByRole('option', {name:'工作配置',exact:true}).click();
  await expect(page.getByRole('button', {name:'当前配置',exact:true})).toContainText('工作配置');
  await page.getByRole('button', {name:'简约视图',exact:true}).click();
  await expect(page.getByText('工作配置', {exact:true})).toBeVisible();
  await expect(page.getByRole('radio', {name:'全局',exact:true})).toBeChecked();
  await expect(page.getByLabel('会话累计')).toHaveText(total!);
});

test('stale observations disable mode changes in both views while capture cleanup stays available', async ({ page }) => {
  await page.goto('/?panel=mode-overview');
  await page.getByRole('button', {name:'状态过期',exact:true}).click();
  await expect(page.getByRole('status', {name:'代理运行状态'})).toContainText('运行状态待确认');
  await expect(page.getByRole('radio', {name:'全局',exact:true})).toBeDisabled();
  await expect(page.getByRole('button', {name:'关闭代理',exact:true})).toBeEnabled();
  await expect(page.locator('.lite-live-rates')).toHaveText(/—.*—/);
  await page.getByRole('button', {name:'专业视图',exact:true}).click();
  await expect(page.getByRole('radio', {name:'全局',exact:true})).toBeDisabled();
  await expect(page.getByRole('button', {name:'重启内核',exact:true})).toBeEnabled();
  await expect(page.getByText('内核未就绪，暂停展示实时速率', {exact:true})).toBeVisible();
});

test('partial and unhealthy capture remain visible and Lite power closes the existing path', async ({ page }) => {
  for (const scenario of ['partial','failure']) {
    await page.goto(`/?panel=mode-overview&mode=${scenario}`);
    await expect(page.getByRole('status', {name:'代理运行状态'})).toContainText(scenario === 'partial' ? '仅 TUN 已开启' : 'TUN 运行异常');
    await page.getByRole('button', {name:'关闭代理',exact:true}).click();
    await expect(page.getByLabel('模式操作')).toHaveText('disconnect');
    await expect(page.getByRole('status', {name:'代理运行状态'})).toContainText('代理已关闭');
  }
});

test('unsupported modes and expired rate samples have the same presentation boundaries', async ({ page }) => {
  await page.goto('/?panel=mode-overview&mode=limited');
  await expect(page.getByRole('radio', {name:'全局',exact:true})).toBeDisabled();
  await page.getByRole('button', {name:'采样过期',exact:true}).click();
  await expect(page.locator('.lite-live-rates')).toHaveText(/—.*—/);
  await page.getByRole('button', {name:'专业视图',exact:true}).click();
  await expect(page.getByRole('radio', {name:'全局',exact:true})).toBeDisabled();
  await expect(page.getByText('流量采样已过期，等待恢复', {exact:true})).toBeVisible();
  await page.goto('/?panel=mode-overview&mode=busy');
  await expect(page.getByRole('button', {name:'关闭代理',exact:true})).toBeDisabled();
  await page.getByRole('button', {name:'专业视图',exact:true}).click();
  await expect(page.getByRole('button', {name:'重启内核',exact:true})).toBeDisabled();
});

test('unconfirmed mode changes remain visible and preserve the confirmed selection in both views', async ({ page }) => {
  await page.goto('/?panel=mode-overview&mode=mode-failure');
  for (const view of ['简约视图', '专业视图']) {
    await page.getByRole('button', {name:view,exact:true}).click();
    await page.getByRole('radio', {name:'全局',exact:true}).click();
    await expect(page.getByText('模式请求已提交，但尚未确认生效，请重新检查', {exact:true})).toBeVisible();
    await expect(page.getByRole('radio', {name:'规则',exact:true})).toBeChecked();
  }
});

test('rejected Lite source selection keeps the actual active source and Pro configuration', async ({ page }) => {
  await page.goto('/?panel=mode-overview&mode=source-failure');
  const source = page.getByRole('button', {name:'切换配置来源'});
  await expect(source).toContainText('订阅-日常网络配置');
  await source.click();
  await page.getByRole('option', {name:'订阅-工作配置',exact:true}).click();
  await expect(source).toContainText('订阅-日常网络配置');
  await page.getByRole('button', {name:'专业视图',exact:true}).click();
  await expect(page.getByRole('button', {name:'当前配置',exact:true})).toContainText('日常网络配置');
});

test('both views agree on optional IPv6 and required IPv6 failure', async ({page}) => {
  for (const required of [false,true]) {
    await page.goto(`/?panel=mode-overview&mode=${required ? 'ipv6-required' : 'ipv4-only-network'}`);
    await expect(page.getByRole('status',{name:'代理运行状态'})).toContainText(required ? 'TUN 运行异常' : '系统代理与 TUN 已开启');
    await page.getByRole('button',{name:'专业视图',exact:true}).click();
    if (required) await expect(page.getByRole('region',{name:'需要处理'})).toContainText('IPv6 出口不可用');
    else { await expect(page.getByRole('region',{name:'需要处理'})).toHaveCount(0); await expect(page.locator('.egress-summary')).toHaveText('IPv4 出口可用'); }
    await page.getByRole('button',{name:'简约视图',exact:true}).click();
    await expect(page.getByRole('status',{name:'代理运行状态'})).toContainText(required ? 'TUN 运行异常' : '系统代理与 TUN 已开启');
  }
});
