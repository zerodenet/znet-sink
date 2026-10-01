import { test, expect, type Page } from '@playwright/test';
async function open(page:Page,params='&traffic-v1=1') {
  await page.goto('/?panel=endpoints-shell&tab=endpoints'+params);
  await page.getByRole('navigation',{name:'端点工作区'}).getByRole('radio',{name:'流量统计',exact:true}).click();
}
const scope=(page:Page,name:string)=>page.getByRole('article',{name:`统计 ${name}`,exact:true});

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
  expect(calls.commands).toHaveLength(1);expect(calls.commands[0].targets[0].expected_generation).toBe('9007199254740994');
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

test('endpoint chart reuses inventory, detail fits narrow viewport and page exit cleans its listeners',async({page})=>{
  await page.goto('/?panel=endpoints-shell&tab=endpoints&traffic-v1=1');
  const row=page.getByRole('article',{name:'端点 wg-a',exact:true});await expect(row.getByLabel('inner 流量曲线')).toBeVisible();await page.evaluate(()=>window.dispatchEvent(new Event('fixture-traffic-sample')));await expect(row.locator('.rates')).toContainText('1000 B/s');
  await row.getByLabel('wg-a 更多操作').click();await row.getByRole('button',{name:'观测详情',exact:true}).click();
  const dialog=page.getByRole('dialog');await expect(dialog.getByLabel('inner 统计',{exact:true})).toContainText('RX');await expect(dialog.getByLabel('outer 统计',{exact:true})).toBeVisible();
  for(const width of [752,360]){await page.setViewportSize({width,height:800});expect(await dialog.evaluate(el=>el.scrollWidth<=el.clientWidth)).toBe(true);await page.screenshot({path:test.info().outputPath(`traffic-detail-${width}.png`)});}
  await dialog.getByRole('button',{name:'关闭',exact:true}).click();await page.getByRole('tab',{name:'节点',exact:true}).click();const calls=JSON.parse(await page.locator('#traffic-fixture').textContent()??'{}');expect(calls.stops).toBe(1);
});

test('deleted scope is removed only after inventory recovery and cannot leave a stuck clear selection',async({page})=>{
  await open(page);await page.getByRole('radio',{name:/^Peer\s*1$/}).click();await scope(page,'peer-1').getByRole('checkbox').check();await expect(page.getByRole('button',{name:'清空选中统计',exact:true})).toContainText('(1)');
  await page.evaluate(()=>window.dispatchEvent(new Event('fixture-traffic-remove-peer')));await expect(scope(page,'peer-1')).toHaveCount(0);await expect(page.getByRole('button',{name:'清空选中统计',exact:true})).not.toContainText('(1)');await expect(page.getByRole('button',{name:'清空选中统计',exact:true})).toBeDisabled();
});
