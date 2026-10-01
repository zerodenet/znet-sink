import { test, expect } from '@playwright/test';
async function open(page: import('@playwright/test').Page, query='') {
  await page.goto('/?panel=endpoints-shell'+query);
  await page.getByRole('tab',{name:'端点',exact:true}).click();
}

async function menu(row: import('@playwright/test').Locator, action: string) {
  const tag = (await row.getAttribute('aria-label'))!.slice(3);
  await row.getByLabel(`${tag} 更多操作`).click();
  await row.getByRole('button', {name:action,exact:true}).click();
}

test('generic endpoint management renders actual facts and only confirmed changes', async ({page})=>{
  await open(page,'&delayed=1');
  const wg=page.getByRole('article',{name:'端点 wg-a',exact:true});
  const mesh=page.getByRole('article',{name:'端点 mesh-b',exact:true});
  await expect(mesh).toContainText('future_mesh');
  await expect(wg.getByLabel('活动连接')).toContainText(/数据报\s*2/);
  await wg.getByRole('switch',{name:'停用端点',exact:true}).click();
  await expect(wg.getByRole('switch',{name:'停用端点',exact:true})).toBeDisabled();
  await expect(wg.getByText('运行中',{exact:true})).toBeVisible();
  await expect(wg.getByText('已停止',{exact:true})).toBeVisible();
  await expect(mesh.getByText('运行中',{exact:true})).toBeVisible();
  await expect(page.getByLabel('保存结果')).toContainText('"expectedIntentRevision":7');
  await wg.getByRole('switch',{name:'启用端点',exact:true}).click();
  await expect(wg.getByText('运行中',{exact:true})).toBeVisible();
  await menu(wg, '协议详情');
  await expect(page.getByLabel('端点协议详情')).toContainText('example.test:51820');
  await expect(page.getByLabel('端点协议详情')).toContainText('认证远端未提供');
  await page.getByRole('dialog').getByRole('button', {name:'关闭',exact:true}).click();
  await page.locator('.endpoints-panel').evaluate(element => { element.scrollTop = 0; });
  await page.screenshot({ path: test.info().outputPath('endpoint-management.png'), fullPage: true });
});

test('directions apply on click, disable while pending, and respect kernel limits',async({page})=>{
  await open(page,'&delayed=1');
  const wg=page.getByRole('article',{name:'端点 wg-a',exact:true});
  const outbound=wg.getByLabel('wg-a 允许出站');
  await expect(outbound).toBeEnabled();
  await expect(outbound).toHaveAttribute('title',/重启此端点并断开现有连接/);
  await outbound.click();
  await expect(outbound).toBeDisabled();
  await expect(outbound).toBeChecked();
  await expect(wg.getByLabel('正在应用')).toBeVisible();
  await expect(wg.getByRole('button',{name:'应用方向',exact:true})).toHaveCount(0);
  await expect(outbound).not.toBeChecked();
  await expect(wg).toContainText('仅入站');
  await expect(wg.getByText('运行中',{exact:true})).toBeVisible();
  await expect(page.getByLabel('保存结果')).toContainText('"operation":"set_directions"');
  await expect(page.getByLabel('保存结果')).not.toContainText('persistence');
});

test('old cores and observation-only protocols have explicit unavailable controls',async({page})=>{
  await open(page,'&version=v002');
  await expect(page.getByLabel('端点管理', {exact:true}).last()).toContainText('当前内核未声明兼容的端点目录能力');
  await expect(page.getByRole('switch',{name:'停用端点',exact:true})).toHaveCount(0);
  await open(page,'&observe-only=1');
  await expect(page.getByRole('switch',{name:'停用端点',exact:true}).first()).toBeDisabled();
  await page.getByRole('article', {name: '端点 wg-a', exact:true}).getByLabel('wg-a 更多操作').click();
  await expect(page.getByRole('button',{name:'协议详情',exact:true}).first()).toBeEnabled();
});

test('conflicts remain visible after refresh without changing the intent',async({page})=>{
  await open(page,'&conflict=1');
  await page.getByRole('article',{name:'端点 wg-a',exact:true}).getByRole('switch',{name:'停用端点',exact:true}).click();
  await expect(page.getByRole('alert')).toContainText('端点意图已变化');
  await expect(page.getByRole('article',{name:'端点 wg-a',exact:true}).getByText('运行中',{exact:true})).toBeVisible();
});

test('local enable and direction preferences merge and survive configuration reload', async ({page})=>{
  await open(page);
  const row=page.getByRole('article',{name:'端点 wg-a',exact:true});
  await row.getByLabel('wg-a 允许入站').click();
  await expect(row.getByLabel('wg-a 允许入站')).not.toBeChecked();
  await row.getByRole('switch',{name:'停用端点',exact:true}).click();
  await expect(row.getByText('已停止',{exact:true})).toBeVisible();
  await expect(row).toContainText('本地');
  await row.getByLabel('wg-a 更多操作').click();
  await expect(row.getByRole('button',{name:/保存到配置|恢复配置意图/})).toHaveCount(0);
  await row.getByLabel('wg-a 更多操作').click();
  await page.evaluate(()=>window.dispatchEvent(new Event('fixture-reload-config')));
  await page.getByRole('button',{name:'刷新端点',exact:true}).click();
  await expect(row.getByText('已停止',{exact:true})).toBeVisible();
  await expect(row.getByLabel('wg-a 允许入站')).not.toBeChecked();
  await expect(row.getByLabel('wg-a 允许出站')).toBeChecked();
  await expect(page.getByRole('article',{name:'端点 mesh-b',exact:true}).getByText('运行中',{exact:true})).toBeVisible();
});

test('periodic protocol observation refreshes an already opened detail',async({page})=>{
  await page.clock.install();
  await open(page);
  const row=page.getByRole('article',{name:'端点 wg-a',exact:true});
  await menu(row, '协议详情');
  await expect(page.getByLabel('端点协议详情')).toContainText('example.test:51820');
  // The fixture updates a public fact between directory refreshes; the view
  // must query the detail again, without another click on Protocol details.
  await page.evaluate(()=>window.dispatchEvent(new Event('fixture-change-peer')));
  await page.clock.runFor(6000);
  await expect(page.getByLabel('端点协议详情')).toContainText('changed.test:51820');
});

test('restart does not clear saved local preferences',async({page})=>{
  await open(page);
  const row=page.getByRole('article',{name:'端点 wg-a',exact:true});
  await row.getByLabel('wg-a 允许入站').click();
  await expect(row.getByLabel('wg-a 允许入站')).not.toBeChecked();
  await menu(row, '重启端点');
  await expect(page.getByLabel('保存结果')).toContainText('"operation":"restart"');
  await expect(row.getByText('运行中',{exact:true})).toBeVisible();
  await expect(row.getByLabel('wg-a 允许入站')).not.toBeChecked();
});

test('host scope failure leaves observations visible and controls unavailable',async({page})=>{
  await open(page,'&host-failure=1');
  const row=page.getByRole('article',{name:'端点 wg-a',exact:true});
  await expect(row.getByText('运行中',{exact:true})).toBeVisible();
  await expect(row.getByRole('switch',{name:'停用端点',exact:true})).toBeDisabled();
  await expect(page.getByRole('alert')).toContainText('客户端状态未确认');
});


test('endpoint lifecycle has an independent page and polling ends on leaving it', async ({ page }) => {
  await page.clock.install();
  await page.goto('/?panel=endpoints-shell');
  await expect(page.locator('.nodes-root')).toBeVisible();
  await expect(page.getByLabel('端点目录查询次数')).toHaveText('0');
  await expect(page.getByRole('button', { name: '端点管理', exact: true })).toHaveCount(0);
  await page.getByRole('tab', { name: '端点', exact: true }).click();
  await expect(page.locator('.endpoints-workspace')).toBeVisible();
  await expect(page.locator('.nodes-root')).toHaveCount(0);
  await expect(page.getByRole('article', { name: '端点 mesh-b', exact: true })).toBeVisible();
  await page.getByRole('tab', { name: '节点', exact: true }).click();
  await expect(page.locator('.nodes-root')).toBeVisible();
  await expect(page.locator('.endpoints-workspace')).toHaveCount(0);
  const reads = await page.getByLabel('端点目录查询次数').textContent();
  await page.clock.runFor(6000);
  await expect(page.getByLabel('端点目录查询次数')).toHaveText(reads!);
  await page.getByRole('tab', { name: '端点', exact: true }).click();
  await expect(page.getByRole('article', { name: '端点 wg-a', exact: true })).toBeVisible();
  await expect(page.getByLabel('端点目录查询次数')).not.toHaveText(reads!);
});

test('lite navigation does not expose or query endpoint management', async ({ page }) => {
  await page.goto('/?panel=endpoints-shell&mode=lite');
  await expect(page.locator('.nodes-root')).toBeVisible();
  await expect(page.getByRole('tab', { name: '端点', exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: '端点管理', exact: true })).toHaveCount(0);
  await expect(page.getByLabel('端点目录查询次数')).toHaveText('0');
});


test('compact cards plot counter deltas without save or apply steps', async ({page}) => {
  await open(page);
  const row = page.getByRole('article', {name:'端点 wg-a',exact:true});
  await expect(row.getByLabel('端点实时流量')).toBeVisible();
  await expect(row.locator('.save-options')).toHaveCount(0);
  await expect(row.getByRole('button', {name:'重启端点',exact:true})).toBeHidden();
  await page.getByRole('button', {name:'刷新端点',exact:true}).click();
  await expect(row.locator('.traffic path.rx')).toHaveCount(1);
  await expect(row.locator('.rates')).not.toContainText('—');
  await row.getByLabel('wg-a 更多操作').click();
  await expect(row.getByRole('button',{name:/保存到配置|应用方向/})).toHaveCount(0);
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(page.getByLabel('保存结果')).toBeEmpty();
});

test('missing byte counters do not become a fake zero rate or decorative chart', async ({page}) => {
  await open(page,'&missing-counters=1');
  const row = page.getByRole('article', {name:'端点 wg-a',exact:true});
  await expect(row.getByLabel('端点实时流量')).toContainText('暂无流量数据');
  await expect(row.locator('.traffic path.rx')).toHaveCount(0);
  await expect(row.locator('.rates')).toContainText('—');
});


test('stale counter observations hide the live rate and restart its baseline on recovery', async ({page}) => {
  await page.clock.install();
  await open(page, '&frozen-samples=1');
  const row = page.getByRole('article', {name:'端点 wg-a',exact:true});
  await page.clock.runFor(6000);
  await expect(row.locator('.rates')).not.toContainText('—');
  await page.clock.runFor(20000);
  await expect(row.getByLabel('端点实时流量')).toContainText('采样中断');
  await expect(row.locator('.rates')).toContainText('—');
  await page.evaluate(() => window.dispatchEvent(new Event('fixture-resume-traffic')));
  await page.getByRole('button', {name:'刷新端点',exact:true}).click();
  await expect(row.locator('.rates')).toContainText('—');
  await page.clock.runFor(6000);
  await expect(row.locator('.rates')).not.toContainText('—');
  await expect(row.getByLabel('端点实时流量')).not.toContainText('采样中断');
});

test('direction failure retains confirmed state and unmatched endpoint declarations are observation-only',async({page})=>{
  await open(page,'&conflict=1');
  const row=page.getByRole('article',{name:'端点 wg-a',exact:true});
  await row.getByLabel('wg-a 允许入站').click();
  await expect(page.getByRole('alert')).toContainText('端点意图已变化');
  await expect(row.getByLabel('wg-a 允许入站')).toBeChecked();
  await open(page,'&unowned=1');
  await expect(row.getByRole('switch',{name:'停用端点',exact:true})).toBeDisabled();
  await expect(row.getByLabel('wg-a 允许入站')).toBeDisabled();
});

test('failed composite direction switch restores the original enable and direction state',async({page})=>{
  await open(page,'&restart-failure=1');
  const row=page.getByRole('article',{name:'端点 wg-a',exact:true});
  await row.getByLabel('wg-a 允许出站').click();
  await expect(page.getByRole('alert')).toContainText('已恢复原状态');
  await expect(row.getByLabel('wg-a 允许出站')).toBeChecked();
  await expect(row.getByRole('switch',{name:'停用端点',exact:true})).toBeChecked();
  await expect(row.getByText('运行中',{exact:true})).toBeVisible();
  await expect(row).not.toContainText('本地');
  await open(page,'&old-preconditions=1');
  await expect(row.getByLabel('wg-a 允许出站')).toBeDisabled();
  await expect(row.getByLabel('wg-a 允许出站')).toHaveAttribute('title',/升级内核/);
});


test('endpoint details retain long public facts at narrow widths and distinguish missing counters from zero', async ({page}) => {
  await open(page, '&long-details=1&zero-counters=1');
  const row = page.getByRole('article', {name:'端点 wg-a', exact:true});
  await menu(row, '协议详情');
  const dialog = page.getByRole('dialog');
  await expect(dialog).toContainText('q3b2T3yN7WvK5sR8dM1xP4cL6jH9aF0uE2nG5zB7kVQ=');
  await expect(dialog).toContainText('fd00:1234:5678:9abc::/64');
  for (const width of [752, 360]) {
    await page.setViewportSize({width, height:800});
    await expect(page.getByRole('region', {name:'Peer 1', exact:true})).toBeVisible();
    expect(await dialog.evaluate(el => el.scrollWidth <= el.clientWidth)).toBe(true);
    expect(await page.getByRole('region', {name:'Peer 1', exact:true}).evaluate(el => el.scrollWidth <= el.clientWidth)).toBe(true);
    await page.screenshot({path:test.info().outputPath(`protocol-detail-${width}.png`)});
  }
  await dialog.getByRole('button', {name:'关闭',exact:true}).click();
  await menu(row, '观测详情');
  await expect(dialog.getByLabel('内层累计流量')).toContainText(/接收\s*0 B/);
  await expect(dialog.getByLabel('内层累计流量')).toContainText(/发送\s*0 B/);
  await expect(dialog.getByLabel('外层累计流量')).toContainText(/接收\s*—/);
  await expect(dialog).toContainText('健康未知');
  expect(await dialog.getByLabel('端点观测详情').evaluate(el => el.scrollWidth <= el.clientWidth)).toBe(true);
  await page.screenshot({path:test.info().outputPath('observation-detail-360.png')});
});
