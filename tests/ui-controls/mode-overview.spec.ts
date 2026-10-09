import { test, expect } from '@playwright/test';

test('actual Lite and Pro overviews share mode choice, active configuration and kernel cumulative totals', async ({ page }) => {
  await page.goto('/?panel=mode-overview');
  await expect(page.getByRole('status', {name:'代理运行状态'})).toContainText('系统代理与 TUN 已开启');
  await expect(page.locator('.lite-entry-current').first()).toHaveText('日常网络配置');
  await expect(page.locator('.lite-total-up')).toContainText('13.0 MB');
  await expect(page.locator('.lite-total-down')).toContainText('322.0 MB');
  await expect(page.locator('.lite-total-traffic')).toContainText('335.0 MB');
  await page.getByRole('radio', {name:'全局',exact:true}).click();
  await page.getByRole('button', {name:'专业视图',exact:true}).click();
  await expect(page.getByRole('radio', {name:'全局',exact:true})).toBeChecked();
  await expect(page.locator('.chart-stats .stat-val.up')).toHaveText('13.0 MB');
  await expect(page.locator('.chart-stats .stat-val.down')).toHaveText('322.0 MB');
  await page.getByRole('button', {name:'当前配置',exact:true}).click();
  await page.getByRole('option', {name:'工作配置',exact:true}).click();
  await expect(page.getByRole('button', {name:'当前配置',exact:true})).toContainText('工作配置');
  await page.getByRole('button', {name:'简约视图',exact:true}).click();
  await expect(page.locator('.lite-entry-current').first()).toHaveText('工作配置');
  await expect(page.getByRole('radio', {name:'全局',exact:true})).toBeChecked();
  await expect(page.locator('.lite-total-up')).toContainText('13.0 MB');
  await expect(page.locator('.lite-total-down')).toContainText('322.0 MB');
});

test('stale observations disable mode changes in both views while capture cleanup stays available', async ({ page }) => {
  await page.goto('/?panel=mode-overview');
  await page.getByRole('button', {name:'状态过期',exact:true}).click();
  await expect(page.getByRole('status', {name:'代理运行状态'})).toContainText('运行状态待确认');
  await expect(page.getByRole('radio', {name:'全局',exact:true})).toBeDisabled();
  await expect(page.getByRole('button', {name:'关闭代理',exact:true})).toBeEnabled();
  await expect(page.locator('.lite-live-rates')).toHaveText(/—.*—/);
  await expect(page.locator('.lite-total-up')).toContainText('—');
  await expect(page.locator('.lite-total-down')).toContainText('—');
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
  await expect(page.locator('.lite-total-up')).toContainText('—');
  await expect(page.locator('.lite-total-down')).toContainText('—');
  await page.getByRole('button', {name:'专业视图',exact:true}).click();
  await expect(page.getByRole('radio', {name:'全局',exact:true})).toBeDisabled();
  await expect(page.getByText('流量采样已过期，等待恢复', {exact:true})).toBeVisible();
  await expect(page.locator('.chart-stats .stat-val.up')).toHaveText('—');
  await expect(page.locator('.chart-stats .stat-val.down')).toHaveText('—');
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
  await expect(source).toContainText('日常网络配置');
  await source.click();
  await page.getByRole('option', {name:'工作配置',exact:true}).click();
  await expect(source).toContainText('日常网络配置');
  await page.getByRole('button', {name:'专业视图',exact:true}).click();
  await expect(page.getByRole('button', {name:'当前配置',exact:true})).toContainText('日常网络配置');
});

test('Lite includes a local active configuration alongside subscriptions and activates cached profiles', async ({page}) => {
  await page.goto('/?panel=mode-overview&mode=local-with-subscriptions');
  const source = page.getByRole('button',{name:'切换配置来源'});
  await expect(source).toContainText('日常网络配置');
  await source.click();
  await expect(page.getByRole('option',{name:'日常网络配置 · 当前',exact:true})).toBeVisible();
  await page.getByRole('option',{name:'工作配置',exact:true}).click();
  await expect(source).toContainText('工作配置');
  await expect(page.locator('.lite-entry-current').first()).toHaveText('工作配置');
  await page.getByRole('button',{name:'专业视图',exact:true}).click();
  await expect(page.getByRole('button',{name:'当前配置',exact:true})).toContainText('工作配置');
});

test('managed configurations show their actual name and source without a subscription target match', async ({page}, testInfo) => {
  await page.goto('/?panel=mode-overview&mode=managed-config');
  await expect(page.getByRole('button',{name:'切换配置来源'})).toContainText('狗梯 - company');
  await expect(page.locator('.lite-entry-meta').first()).toHaveText('JSON · 狗梯');
  await expect(page.getByText('本地/专业配置',{exact:false})).toHaveCount(0);
  await page.screenshot({path:testInfo.outputPath('lite-managed-source.png'),fullPage:true});
});

test('unsynced sources sync before activation and disabled sources remain disabled', async ({page}) => {
  await page.goto('/?panel=mode-overview&mode=unsynced-source');
  const source = page.getByRole('button',{name:'切换配置来源'});
  await source.click();
  await expect(page.getByRole('option',{name:'停用订阅 · 未同步',exact:true})).toHaveAttribute('data-disabled','');
  await page.getByRole('option',{name:'新订阅 · 未同步',exact:true}).click();
  await expect(source).toContainText('新订阅配置');
  await expect(page.locator('.lite-entry-current').first()).toHaveText('新订阅配置');
  await source.click();
  await expect(page.getByRole('option',{name:'新订阅配置 · 当前',exact:true})).toBeVisible();
  await expect(page.getByRole('option',{name:'新订阅 · 未同步',exact:true})).toHaveCount(0);
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


test('Lite can retry a failed TUN without turning capture off', async ({page}) => {
  await page.goto('/?panel=mode-overview&mode=failure');
  await page.getByRole('button',{name:'立即重试 TUN 网络恢复'}).click();
  await expect(page.getByLabel('模式操作')).toHaveText('tun-recover');
  await expect(page.getByRole('status',{name:'代理运行状态'})).toContainText('系统代理与 TUN 已开启');
  await expect(page.getByRole('button',{name:'关闭代理',exact:true})).toBeEnabled();
});


test('Lite recovery has pending feedback and prevents duplicate or conflicting operations', async ({page}, testInfo) => {
  await page.addInitScript(() => {
    (window as any).recoveryRequests=[];
    window.addEventListener('fixture-recovery-request', event => (window as any).recoveryRequests.push((event as CustomEvent).detail));
  });
  await page.goto('/?panel=mode-overview&mode=failure&recovery-pending=1');
  const retry = page.getByRole('button',{name:'立即重试 TUN 网络恢复'});
  await expect(retry).toHaveText('重试恢复');
  await page.locator('.lite-root').screenshot({path:testInfo.outputPath('lite-recovery-warning.png')});
  await retry.click();
  await expect(retry).toBeDisabled();
  await expect(retry).toHaveAttribute('aria-busy','true');
  await expect(retry).toHaveText('恢复中…');
  await expect(page.getByRole('button',{name:'关闭代理',exact:true})).toBeDisabled();
  await expect(page.getByRole('radio',{name:'全局',exact:true})).toBeDisabled();
  await expect(page.getByRole('button',{name:'切换配置来源'})).toBeDisabled();
  await retry.evaluate(button => (button as HTMLButtonElement).click());
  expect(await page.evaluate(() => (window as any).recoveryRequests)).toEqual([{count:1,options:{notify:false}}]);
  await page.locator('.lite-root').screenshot({path:testInfo.outputPath('lite-recovery-pending.png')});
  await page.evaluate(() => window.dispatchEvent(new Event('fixture-finish-recovery')));
  await expect(retry).toBeEnabled();
  await expect(retry).toHaveAttribute('aria-busy','false');
  await expect(page.getByRole('status',{name:'代理运行状态'})).toContainText('系统代理与 TUN 已开启');
  await expect(page.locator('.lite-recovery-feedback.success')).toHaveText('TUN 路由检查通过');
});

for (const [scenario, message] of [['recovery-rejected','出口恢复失败，请检查本地网络'], ['recovery-error','控制接口暂时不可用']]) {
  test(`Lite recovery keeps the confirmed state and exposes failure: ${scenario}`, async ({page}) => {
    await page.goto(`/?panel=mode-overview&mode=${scenario}`);
    const retry = page.getByRole('button',{name:'立即重试 TUN 网络恢复'});
    await retry.click();
    await expect(page.getByRole('alert')).toHaveText(message);
    await expect(retry).toBeEnabled();
    await expect(page.getByRole('status',{name:'代理运行状态'})).toContainText('TUN 运行异常');
    await expect(page.locator('.lite-recovery-feedback.success')).toHaveCount(0);
  });
}

test('Lite capture controls remain compact and keyboard accessible', async ({page}, testInfo) => {
  await page.goto('/?panel=mode-overview');
  const retry = page.getByRole('button',{name:'立即重试 TUN 网络恢复'});
  for (const width of [900, 420]) {
    await page.setViewportSize({width,height:720});
    await expect(retry).toBeVisible();
    await expect(page.locator('.lite-capture-badge.healthy')).toHaveText('系统代理与 TUN 已开启');
    expect(await page.locator('.lite-capture-state').evaluate(el => el.scrollWidth <= el.clientWidth)).toBe(true);
    await page.locator('.lite-root').screenshot({path:testInfo.outputPath(`lite-capture-${width}.png`)});
  }
  await retry.focus();
  await retry.press('Enter');
  await expect(page.getByLabel('模式操作')).toHaveText('tun-recover');
  await expect(page.locator('.lite-recovery-feedback.success')).toBeVisible();
});


test('overview cumulative counters and rates agree across updates, counter resets and mode switches', async ({page}) => {
  await page.goto('/?panel=mode-overview');
  for (const sample of [
    {up:1_180_000_000,down:3_590_000_000,upRate:193_000,downRate:32_000,upText:'1.18 GB',downText:'3.59 GB',upSpeed:'193 KB/s',downSpeed:'32.0 KB/s'},
    {up:128,down:0,upRate:64,downRate:0,upText:'128 B',downText:'0 B',upSpeed:'<1 KB/s',downSpeed:'0 KB/s'},
  ]) {
    await page.getByRole('button',{name:'简约视图',exact:true}).click();
    await page.evaluate(sample => window.dispatchEvent(new CustomEvent('fixture-traffic-sample',{detail:sample})),sample);
    await expect(page.locator('.lite-total-up')).toContainText(sample.upText);
    await expect(page.locator('.lite-total-down')).toContainText(sample.downText);
    await expect(page.locator('.lite-live-up')).toContainText(sample.upSpeed);
    await expect(page.locator('.lite-live-down')).toContainText(sample.downSpeed);
    await page.getByRole('button',{name:'专业视图',exact:true}).click();
    await expect(page.locator('.chart-stats .stat-val.up')).toHaveText(sample.upText);
    await expect(page.locator('.chart-stats .stat-val.down')).toHaveText(sample.downText);
    await expect(page.locator('.speed-item.up .speed-val')).toHaveText(sample.upSpeed);
    await expect(page.locator('.speed-item.down .speed-val')).toHaveText(sample.downSpeed);
  }
});

test('overview and policy choices share automatic probes without changing manual selections', async ({page}) => {
  await page.goto('/?panel=mode-overview&mode=shared-probe');
  await page.getByRole('button',{name:'专业视图',exact:true}).click();
  const shortcut=page.getByRole('button',{name:'查看与切换策略组'});
  await expect(shortcut).toContainText('节点选择 → 自动选择 → US');
  await expect(shortcut).toContainText('356 ms');
  await shortcut.click();
  const dialog=page.getByRole('dialog');
  const youtube=dialog.locator('.policy-edit').filter({has:page.locator('.policy-name strong',{hasText:'YouTube'})});
  const manual=dialog.locator('.policy-edit').filter({has:page.locator('.policy-name strong',{hasText:'AI Suite'})});
  await expect(youtube.getByRole('button',{name:'YouTube 当前出口'})).toContainText('356 ms');
  await expect(manual.getByRole('button',{name:'AI Suite 当前出口'})).toContainText('US · 356 ms');
  await expect(manual).toContainText('最近探测成功');
  await expect(manual.locator('.policy-name')).toContainText('可切换');
  await page.evaluate(() => {
    const checked=Date.now();
    window.dispatchEvent(new CustomEvent('fixture-policies',{detail:[
      {name:'YouTube',kind:'selector',selected:'节点选择',outbounds:[{tag:'节点选择',type:'selector'}]},
      {name:'节点选择',kind:'selector',selected:'自动选择',outbounds:[{tag:'自动选择',type:'urltest'}]},
      {name:'AI Suite',kind:'selector',selected:'US',outbounds:[{tag:'US',type:'unknown'}]},
      {name:'自动选择',kind:'urltest',selected:'JP',outbounds:[{tag:'US',type:'unknown',alive:false,lastCheckedUnixMs:checked},{tag:'JP',type:'unknown',alive:true,delayMs:20,lastCheckedUnixMs:checked}]},
    ]}));
  });
  await expect(youtube.getByRole('button',{name:'YouTube 当前出口'})).toContainText('节点选择 → 自动选择 → JP · 20 ms');
  await expect(manual.getByRole('button',{name:'AI Suite 当前出口'})).toContainText('US · 探测失败');
  await expect(manual).toContainText('最近探测失败');
  await expect(manual).not.toContainText('356 ms');
  await dialog.getByRole('button',{name:'完成',exact:true}).click();
  await expect(shortcut).toContainText('US');
  await expect(shortcut).toContainText('探测失败');
});
