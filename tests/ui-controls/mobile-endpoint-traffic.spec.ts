import { test, expect, type Page } from '@playwright/test';

async function openDetails(page: Page, options = '', mobile = true, mode = '观测详情') {
  await page.setViewportSize({ width: mobile ? 320 : 900, height: 760 });
  await page.goto('/?panel=endpoints-shell&tab=endpoints&traffic-v1=1' + (mobile ? '&mobile-endpoint-return=1' : '') + options);
  if (mobile) {
    await page.getByRole('navigation', { name: '主导航' }).getByRole('button', { name: '更多' }).click();
    await page.locator('.mobile-menu').getByRole('button', { name: '端点', exact: true }).click();
  }
  const card = page.getByRole('article', { name: '端点 wg-a', exact: true });
  await expect(card).toBeVisible();
  await card.getByLabel('wg-a 更多操作').click();
  await card.getByRole('button', { name: mode, exact: true }).click();
  await expect(page.getByRole('dialog')).toContainText('wg-a · ' + mode);
  const statistics = page.getByRole('button', { name: '查看此端点统计', exact: true });
  if (mobile) { const box=(await statistics.boundingBox())!; expect(box.height).toBeGreaterThanOrEqual(44); }
  await statistics.click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(page.getByLabel('端点统计定位')).toContainText('wg-a');
}

for (const mobile of [true, false]) {
  test(`${mobile ? 'mobile source stack' : 'desktop mounted workspace'} opens actual endpoint registry and restores original protocol details`, async ({ page }) => {
    await openDetails(page, '', mobile, '协议详情');
    await expect(page.getByRole('article', { name: '统计 resource:wg-a', exact: true })).toBeVisible();
    await expect(page.locator('.statistics .scope')).toHaveCount(1);
    expect(await page.locator('#traffic-fixture').textContent()).toContain('"stops":0');
    if (mobile) await page.getByRole('button', { name: '返回', exact: true }).click();
    else await page.getByRole('button', { name: '返回原端点详情', exact: true }).click();
    await expect(page.getByRole('dialog')).toContainText('wg-a · 协议详情');
    await expect(page.getByRole('dialog')).toContainText('public-only');
    expect(await page.locator('#traffic-fixture').textContent()).toContain('"subscriptions":1');
    expect(await page.locator('#traffic-fixture').textContent()).toContain('"stops":0');
  });
}

test('mobile browser back restores observation detail and another bottom tab retains the statistics visit', async ({ page }) => {
  await openDetails(page);
  const nav = page.getByRole('navigation', { name: '主导航' });
  await nav.getByRole('button', { name: '概览' }).click();
  await expect(page.getByLabel('端点统计定位')).not.toBeVisible();
  await nav.getByRole('button', { name: '更多' }).click();
  await expect(page.getByLabel('端点统计定位')).toBeVisible();
  await page.goBack();
  await expect(page.getByRole('dialog')).toContainText('wg-a · 观测详情');
});

for (const change of ['replacement', 'removal'] as const) {
  test(`late statistics return refuses ${change} instead of reopening an old controllable detail`, async ({ page }) => {
    await openDetails(page);
    await page.evaluate(change => {
      window.dispatchEvent(new Event(change === 'removal' ? 'fixture-endpoint-remove' : 'fixture-reload-config'));
      window.dispatchEvent(new Event(change === 'removal' ? 'fixture-traffic-remove-endpoint' : 'fixture-traffic-replace-endpoint'));
      window.dispatchEvent(new Event('focus'));
    }, change);
    await expect(page.getByLabel('端点统计定位')).toContainText(change === 'removal' ? '未登记此端点' : 'generation 已变化');
    await expect(page.locator('.statistics .scope')).toHaveCount(0);
    await page.getByRole('button', { name: '返回', exact: true }).click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    await expect(page.getByRole('region', { name: '端点管理' })).toContainText('未恢复旧详情');
    if (change === 'removal') await expect(page.getByRole('article', { name: '端点 wg-a', exact: true })).toHaveCount(0);
  });
}

test('a missing actual registry entry stays explained without substituting global or same-name role counters', async ({ page }) => {
  await openDetails(page, '&missing-endpoint-stat=1');
  await expect(page.getByLabel('端点统计定位')).toContainText('未登记此端点');
  await expect(page.locator('.statistics .scope')).toHaveCount(0);
  await expect(page.getByRole('button', { name: '刷新统计', exact: true })).toBeEnabled();
});

test('legacy endpoint navigation uses only actual declared role and labels its accounting limit', async ({ page }) => {
  await openDetails(page, '&legacy-role-stats=1');
  await expect(page.getByLabel('端点统计定位')).toContainText('实际出站角色统计，不代表端点总计');
  await expect(page.getByRole('article', { name: '统计 wg-a', exact: true })).toBeVisible();
  await expect(page.locator('.statistics .scope')).toHaveCount(1);
});

test('unsupported previous plane chooses actual endpoint plane and preserves full u64 in details', async ({ page }) => {
  await openDetails(page, '&endpoint-inner-only=1&endpoint-large-counter=1');
  await expect(page.getByLabel('端点统计定位')).toContainText('原统计平面 flow 不适用于此资源');
  await expect(page.getByRole('group', { name: '统计平面', exact: true }).getByRole('radio')).toHaveCount(1);
  await expect(page.getByRole('radio', { name: 'Inner', exact: true })).toHaveAttribute('aria-checked', 'true');
  await expect(page.locator('.statistics .totals dd').first()).toHaveAttribute('title', '18446744073709551615');
  await page.getByRole('button', { name: '查看 resource:wg-a 统计详情', exact: true }).click();
  await expect(page.getByRole('dialog')).toContainText('18446744073709551615');
  await page.keyboard.press('Escape');
  await page.evaluate(() => window.dispatchEvent(new Event('mobile:back', { cancelable: true })));
  await expect(page.getByRole('dialog')).toContainText('wg-a · 观测详情');
});

for (const kind of ['browser', 'system'] as const) {
  test(`${kind} return ignores a closed statistics portal retained by its exit animation`, async ({ page }) => {
    await openDetails(page);
    // Keep the real closed Portal mounted so Chromium and WebKit exercise the
    // same second-return boundary, independently of their animation timing.
    await page.addStyleTag({ content: '@keyframes fixture-retain-closed { from { opacity: 1; } to { opacity: .99; } } [role="dialog"][data-state="closed"] { animation: fixture-retain-closed 2s linear !important; }' });
    await page.getByRole('button', { name: '查看 resource:wg-a 统计详情', exact: true }).click();
    const portal = page.getByRole('dialog');
    await expect(portal).toContainText('resource:wg-a');
    await portal.evaluate(element => element.setAttribute('data-fixture-closing-portal', 'true'));
    await page.keyboard.press('Escape');
    const closing = page.locator('[data-fixture-closing-portal="true"]');
    await expect(closing).toHaveAttribute('data-state', 'closed');
    await expect(closing).toBeVisible();
    if (kind === 'browser') await page.goBack();
    else await page.evaluate(() => window.dispatchEvent(new Event('mobile:back', { cancelable: true })));
    await expect(page.getByRole('dialog')).toContainText('wg-a · 观测详情');
  });
}


test('client generation ABA cannot restore a previous visit even when endpoint fields return unchanged', async ({ page }) => {
  await openDetails(page);
  await page.evaluate(() => {
    for (const generation of [2,3]) window.dispatchEvent(new CustomEvent('client-core:updated',{detail:{revision:generation,scopeGeneration:generation,scope:{profileId:'fixture',configRevision:1,coreInstanceId:1},sourceStatus:'ready',activeProbeJobs:[]}}));
  });
  await expect(page.getByLabel('端点统计定位')).toContainText('原端点、配置或客户端运行代次已变化');
  await page.getByRole('button', { name: '返回', exact: true }).click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(page.getByRole('region', { name: '端点管理' })).toContainText('未恢复旧详情');
});

test('endpoint generation replacement after reset preview cannot clear a newly created resource', async ({ page }) => {
  await openDetails(page);
  await page.getByRole('checkbox', { name: '选择 resource:wg-a', exact: true }).check();
  await page.getByRole('button', { name: '清空选中统计', exact: true }).click();
  await expect(page.getByRole('dialog')).toContainText('清空所选统计');
  await page.evaluate(()=>window.dispatchEvent(new Event('fixture-traffic-replace-endpoint')));
  await expect(page.getByLabel('端点统计定位')).toContainText('generation 已变化');
  await page.getByRole('button', { name: '确认清空', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('未提交清空');
  const report=JSON.parse((await page.locator('#traffic-fixture').textContent())!);
  expect(report.commands).toEqual([]);
});


for (const kind of ['browser', 'system'] as const) {
  test(`${kind} back closes the statistics portal before returning to the source detail`, async ({ page }) => {
    await openDetails(page);
    await page.getByRole('button', { name: '查看 resource:wg-a 统计详情', exact: true }).click();
    await expect(page.getByRole('dialog')).toHaveCount(1);
    if (kind === 'browser') await page.goBack();
    else await page.evaluate(()=>window.dispatchEvent(new Event('mobile:back', {cancelable:true})));
    await expect(page.getByRole('dialog')).toHaveCount(0);
    await expect(page.getByRole('heading', {name:'流量统计',exact:true})).toHaveText('流量统计');
    await page.evaluate(()=>window.dispatchEvent(new Event('mobile:back', {cancelable:true})));
    await expect(page.getByRole('dialog')).toContainText('wg-a · 观测详情');
  });
}

test('a late return directory read cannot overwrite a subsequently opened endpoint detail', async ({ page }) => {
  await openDetails(page);
  await page.evaluate(()=>{
    (window as any).endpointHeld=false;(window as any).endpointCompleted=false;
    window.addEventListener('fixture-endpoint-held',()=>{(window as any).endpointHeld=true;},{once:true});
    window.addEventListener('fixture-endpoint-catalog-complete',()=>{(window as any).endpointCompleted=true;},{once:true});
    window.dispatchEvent(new Event('fixture-endpoint-hold-read'));
  });
  await page.getByRole('button', { name: '返回', exact: true }).click();
  await expect.poll(()=>page.evaluate(()=>(window as any).endpointHeld)).toBe(true);
  const mesh=page.getByRole('article',{name:'端点 mesh-b',exact:true});
  await mesh.getByLabel('mesh-b 更多操作').click();
  await mesh.getByRole('button',{name:'观测详情',exact:true}).click();
  await expect(page.getByRole('dialog')).toContainText('mesh-b · 观测详情');
  await page.evaluate(()=>window.dispatchEvent(new Event('fixture-endpoint-release-read')));
  await expect.poll(()=>page.evaluate(()=>(window as any).endpointCompleted)).toBe(true);
  await expect(page.getByRole('dialog')).toHaveCount(1);
  await expect(page.getByRole('dialog')).toContainText('mesh-b · 观测详情');
});

test('reset confirmation re-reads host generation even when no client scope event was delivered', async ({ page }) => {
  await openDetails(page);
  await page.getByRole('checkbox',{name:'选择 resource:wg-a',exact:true}).check();
  await page.getByRole('button',{name:'清空选中统计',exact:true}).click();
  await page.evaluate(()=>window.dispatchEvent(new CustomEvent('fixture-client-snapshot-generation',{detail:3})));
  await page.getByRole('button',{name:'确认清空',exact:true}).click();
  await expect(page.getByRole('alert')).toContainText('未提交清空');
  expect(JSON.parse((await page.locator('#traffic-fixture').textContent())!).commands).toEqual([]);
});

for (const generation of [undefined, 9007199254740992]) {
  test(`missing or unsafe client generation ${generation} grants no source return or reset eligibility`, async ({ page }) => {
    await openDetails(page);
    await page.evaluate(generation=>window.dispatchEvent(new CustomEvent('client-core:updated',{detail:{revision:2,scopeGeneration:generation,scope:{profileId:'fixture',configRevision:1,coreInstanceId:1},sourceStatus:'ready',activeProbeJobs:[]}})),generation);
    await expect(page.getByLabel('端点统计定位')).toContainText('原端点、配置或客户端运行代次已变化');
    await expect(page.getByRole('button',{name:'清空选中统计',exact:true})).toBeDisabled();
    await page.getByRole('button',{name:'返回',exact:true}).click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    await expect(page.getByRole('region',{name:'端点管理'})).toContainText('未恢复旧详情');
  });
}


test('hidden endpoint and statistics panes retain state without polling or replacing active global preferences', async ({ page }) => {
  await page.clock.install();
  await openDetails(page);
  const reads=Number(await page.getByLabel('端点目录查询次数').textContent());
  const nav=page.getByRole('navigation',{name:'主导航'});
  await nav.getByRole('button',{name:'概览'}).click();
  await page.getByRole('button',{name:'打开全局统计',exact:true}).click();
  await expect(page.getByRole('article',{name:'统计 全局',exact:true})).toBeVisible();
  await page.evaluate(()=>window.dispatchEvent(new Event('fixture-traffic-sample')));
  await page.clock.fastForward(6000);
  await page.evaluate(()=>{window.dispatchEvent(new Event('focus'));window.dispatchEvent(new Event('fixture-read-traffic-preferences'));});
  await expect(page.getByLabel('当前统计浏览偏好')).toContainText('"kind":"global"');
  expect(Number(await page.getByLabel('端点目录查询次数').textContent())).toBe(reads);
  await nav.getByRole('button',{name:'更多'}).click();
  await expect(page.getByRole('article',{name:'统计 resource:wg-a',exact:true})).toBeVisible();
  await page.evaluate(()=>window.dispatchEvent(new Event('fixture-read-traffic-preferences')));
  await expect(page.getByLabel('当前统计浏览偏好')).toContainText('"kind":"endpoint"');
  expect(JSON.parse((await page.locator('#traffic-fixture').textContent())!).stops).toBe(0);
});
