import { test, expect, type Page } from '@playwright/test';
async function open(page:Page,params='&traffic-v1=1') {
  await page.goto('/?panel=endpoints-shell&tab=endpoints'+params);
  await page.getByRole('navigation',{name:'端点工作区'}).getByRole('radio',{name:'流量统计',exact:true}).click();
}
const scope=(page:Page,name:string)=>page.getByRole('article',{name:`统计 ${name}`,exact:true});

test('statistics footer hides one-page navigation and groups count with working multi-page controls',async({page})=>{
  await open(page);
  const footer=page.locator('.statistics footer');
  await expect(footer).toHaveText('共 1 个范围');
  await expect(page.getByRole('button',{name:'统计下一页',exact:true})).toHaveCount(0);
  await open(page,'&traffic-v1=1&traffic-many-scopes=1');
  await page.getByRole('radio',{name:/^出站\s*27$/}).click();
  await expect(page.locator('.statistics .scope')).toHaveCount(24);
  await expect(footer).toContainText('共 27 个范围');
  await expect(footer.locator('.pagination')).toContainText('1 / 2');
  const gap=await footer.evaluate(el=>{
    const count=el.children[0].getBoundingClientRect(),pages=el.children[1].getBoundingClientRect();
    return pages.left-count.right;
  });
  expect(gap).toBeLessThanOrEqual(13);
  await page.getByRole('button',{name:'统计下一页',exact:true}).click();
  await expect(page.locator('.statistics .scope')).toHaveCount(3);
  await expect(footer.locator('.pagination')).toContainText('2 / 2');
  await expect(page.getByRole('button',{name:'统计下一页',exact:true})).toBeDisabled();
  await page.getByRole('button',{name:'统计上一页',exact:true}).click();
  await expect(page.locator('.statistics .scope')).toHaveCount(24);
  await page.setViewportSize({width:360,height:800});
  expect(await footer.evaluate(el=>el.scrollWidth<=el.clientWidth)).toBe(true);
  await open(page);
  await page.setViewportSize({width:1100,height:800});
  await page.screenshot({path:test.info().outputPath('traffic-footer.png')});
});

test('legacy single-role endpoints show actual Inner rates and activity without inventing missing metrics',async({page})=>{
  await page.goto('/?panel=endpoints-shell&tab=endpoints&traffic-v1=1&legacy-role-stats=1');
  const wg=page.getByRole('article',{name:'端点 wg-a',exact:true}),other=page.getByRole('article',{name:'端点 mesh-b',exact:true});
  await expect(wg.locator('.observation-source')).toHaveText('出站 · Inner');
  await expect(wg.getByLabel('活动连接')).toContainText(/流\s*0/);
  await expect(wg.getByLabel('活动连接')).toContainText(/数据报\s*0/);
  await expect(wg.getByLabel('活动连接')).toContainText(/Packet\s*—/);
  await page.evaluate(()=>window.dispatchEvent(new Event('fixture-traffic-sample')));
  await expect(wg.locator('.rates')).toContainText('RX 1000 B/s');
  await expect(other.locator('.rates')).toContainText('RX —');
  await expect(other.locator('.rates .receive')).toHaveAttribute('title','内核未提供此指标');
  await wg.getByLabel('wg-a 更多操作').click();await wg.getByRole('button',{name:'观测详情',exact:true}).click();
  await expect(page.getByRole('dialog').getByLabel('inner 统计',{exact:true})).toContainText('RX');
  await expect(page.getByRole('dialog').getByLabel('outer 统计',{exact:true})).toContainText('RX—');
});

test('missing statistical scope preserves known endpoint activity and explains unavailable rates',async({page})=>{
  await page.goto('/?panel=endpoints-shell&tab=endpoints&traffic-v1=1&missing-endpoint-stat=1');
  const row=page.getByRole('article',{name:'端点 wg-a',exact:true});
  await expect(row.locator('.observation-source')).toHaveText('暂无端点统计快照');
  await expect(row.getByLabel('活动连接')).toContainText(/流\s*0/);
  await expect(row.getByLabel('活动连接')).toContainText(/数据报\s*2/);
  await expect(row.getByLabel('活动连接')).toContainText(/Packet\s*—/);
  await expect(row.locator('.rates .receive')).toHaveAttribute('title','暂无统计快照');
});

test('five scopes keep Flow Inner Outer separate and plot real fixture deltas',async({page})=>{
  await open(page);
  const global=scope(page,'全局');await expect(global).toContainText('10.7 KB');
  await expect(global.locator('.rates')).toContainText('—');
  await page.evaluate(()=>window.dispatchEvent(new Event('fixture-traffic-sample')));
  await expect(global.locator('.rates')).toContainText('1000 B/s');
  await page.getByRole('radio',{name:'Outer',exact:true}).click();
  await expect(global.locator('.totals')).toContainText('RX—');
  await expect(global.locator('.totals')).toContainText('TX—');
  await page.getByRole('radio',{name:/^入站\s*1$/}).click();
  await page.getByRole('radio',{name:'Flow',exact:true}).click();
  await expect(scope(page,'mixed').locator('.totals')).toContainText('RX');
  await expect(scope(page,'mixed').locator('.totals')).not.toContainText('下载');
  await page.getByRole('radio',{name:/^出站\s*1$/}).click();await expect(scope(page,'proxy-a')).toBeVisible();
  await page.getByRole('radio',{name:/^端点\s*1$/}).click();await expect(scope(page,'resource:wg-a')).toBeVisible();
  await page.getByRole('radio',{name:/^Peer\s*1$/}).click();await expect(scope(page,'peer-1')).toContainText('resource:wg-a');
  await page.screenshot({path:test.info().outputPath('traffic-peer.png')});
});

test('explicit Admin clear is confirmed once, preserves activity and uses returned nonzero counters',async({page})=>{
  await open(page);await page.getByRole('radio',{name:/^端点\s*1$/}).click();
  const row=scope(page,'resource:wg-a');await row.getByRole('checkbox').check();
  await page.getByRole('button',{name:'清空选中统计',exact:true}).click();
  const dialog=page.getByRole('dialog');await expect(dialog).toContainText('其他范围保持原值');await expect(dialog).toContainText('resource:wg-a');
  await dialog.getByRole('button',{name:'确认清空',exact:true}).click();
  await expect(page.getByRole('button',{name:'清空选中统计',exact:true})).toBeDisabled();
  await expect(row.locator('.totals')).toContainText('5 B');
  await expect(row.locator('.activity')).toContainText('2 / 3 / 1');
  const calls=JSON.parse(await page.locator('#traffic-fixture').textContent()??'{}');
  expect(calls.commands).toHaveLength(1);expect(calls.commands[0].targets[0].expected_generation).toBe('1');
  await expect(page.locator('.statistics').getByRole('status')).toContainText('已清空 1 个统计范围');
});

test('lost confirmation queries new period and never repeats clear',async({page})=>{
  await open(page,'&traffic-v1=1&traffic-reset=lost_ack');const global=scope(page,'全局');await global.getByRole('checkbox').check();await page.getByRole('button',{name:'清空选中统计',exact:true}).click();await page.getByRole('dialog').getByRole('button',{name:'确认清空',exact:true}).click();
  await expect(page.locator('.statistics').getByRole('status')).toContainText('不会自动重试');await expect(global.locator('.totals')).toContainText('5 B');
  const calls=JSON.parse(await page.locator('#traffic-fixture').textContent()??'{}');expect(calls.commands).toHaveLength(1);expect(calls.commands[0].targets[0]).not.toHaveProperty('expected_generation');
});

test('permission denial disables reset, query-only and old kernel remain usable',async({page})=>{
  await open(page,'&traffic-v1=1&traffic-reset=permission_denied');await scope(page,'全局').getByRole('checkbox').check();await page.getByRole('button',{name:'清空选中统计',exact:true}).click();await page.getByRole('dialog').getByRole('button',{name:'确认清空',exact:true}).click();await expect(scope(page,'全局').getByRole('checkbox')).toBeDisabled();
  await open(page,'&traffic-v1=1&traffic-poll=1');await expect(page.locator('.statistics .mode')).toContainText('定时查询');
  await open(page,'&version=v002');await expect(page.getByLabel('流量统计',{exact:true})).toContainText('当前内核未提供 Traffic Observation V1');await expect(page.getByRole('button',{name:'清空选中统计',exact:true})).toBeDisabled();
});

test('endpoint chart reuses inventory and detail fits narrow viewport while app sampling survives page exit',async({page})=>{
  await page.goto('/?panel=endpoints-shell&tab=endpoints&traffic-v1=1');
  const row=page.getByRole('article',{name:'端点 wg-a',exact:true});await expect(row.getByLabel('inner 流量曲线')).toBeVisible();await page.evaluate(()=>window.dispatchEvent(new Event('fixture-traffic-sample')));await expect(row.locator('.rates')).toContainText('1000 B/s');
  await row.getByLabel('wg-a 更多操作').click();await row.getByRole('button',{name:'观测详情',exact:true}).click();
  const dialog=page.getByRole('dialog');await expect(dialog.getByLabel('inner 统计',{exact:true})).toContainText('RX');await expect(dialog.getByLabel('outer 统计',{exact:true})).toBeVisible();
  for(const width of [752,360]){await page.setViewportSize({width,height:800});expect(await dialog.evaluate(el=>el.scrollWidth<=el.clientWidth)).toBe(true);await page.screenshot({path:test.info().outputPath(`traffic-detail-${width}.png`)});}
  await dialog.getByRole('button',{name:'关闭',exact:true}).click();await page.getByRole('tab',{name:'节点',exact:true}).click();const calls=JSON.parse(await page.locator('#traffic-fixture').textContent()??'{}');expect(calls.stops).toBe(0);
});

test('scope switches and menu navigation retain curves and continue sampling without duplicate subscription',async({page})=>{
  await open(page);const global=scope(page,'全局');
  for(let i=0;i<3;i++) await page.evaluate(()=>window.dispatchEvent(new Event('fixture-traffic-sample')));
  const curve=global.locator('path.receive');await expect(curve).toHaveAttribute('d',/L/);
  const before=await curve.getAttribute('d');
  await page.getByRole('radio',{name:/^入站\s*1$/}).click();
  await page.getByRole('radio',{name:/^全局\s*1$/}).click();await expect(curve).toHaveAttribute('d',before!);
  await page.getByRole('tab',{name:'节点',exact:true}).click();await expect(page.getByLabel('流量统计',{exact:true})).toHaveCount(0);
  await page.evaluate(()=>window.dispatchEvent(new Event('fixture-traffic-sample')));
  await page.getByRole('tab',{name:'端点',exact:true}).click();await expect(global).toBeVisible();
  await expect(curve).toHaveAttribute('d',/L/);expect(await curve.getAttribute('d')).not.toBe(before);
  await expect(global.locator('.totals')).toContainText('14.6 KB');
  const calls=JSON.parse(await page.locator('#traffic-fixture').textContent()??'{}');expect(calls.stops).toBe(0);expect(calls.subscriptions).toBe(1);
  await page.getByRole('radio',{name:/^出站\s*1$/}).click();
  await page.getByRole('radio',{name:'Inner',exact:true}).click();
  await page.getByRole('textbox',{name:'搜索统计范围'}).fill('proxy-a');
  await page.getByRole('tab',{name:'节点',exact:true}).click();
  await page.getByRole('tab',{name:'端点',exact:true}).click();
  await expect(scope(page,'proxy-a')).toBeVisible();
  await expect(page.getByRole('radio',{name:'Inner',exact:true})).toBeChecked();
  await expect(page.getByRole('textbox',{name:'搜索统计范围'})).toHaveValue('proxy-a');
});

test('deleted scope is removed only after inventory recovery and cannot leave a stuck clear selection',async({page})=>{
  await open(page);await page.getByRole('radio',{name:/^Peer\s*1$/}).click();await scope(page,'peer-1').getByRole('checkbox').check();await expect(page.getByRole('button',{name:'清空选中统计',exact:true})).toContainText('(1)');
  await page.evaluate(()=>window.dispatchEvent(new Event('fixture-traffic-remove-peer')));await expect(scope(page,'peer-1')).toHaveCount(0);await expect(page.getByRole('button',{name:'清空选中统计',exact:true})).not.toContainText('(1)');await expect(page.getByRole('button',{name:'清空选中统计',exact:true})).toBeDisabled();
});
