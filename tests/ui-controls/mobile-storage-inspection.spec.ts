import { test, expect, type Page } from '@playwright/test';
// Simulated host metadata only; these tests do not verify a real process/VPN.
test.use({viewport:{width:320,height:800}});
const panel=(page:Page,kind:'workspace'|'packet')=>page.getByRole('region',{name:kind==='workspace'?'工作区存储只读检查':'Packet 账本只读检查',exact:true});
const state=(page:Page)=>page.evaluate(()=>(window as any).storageInspectionFixture.calls);
async function click(page:Page,kind:'workspace'|'packet'){await panel(page,kind).getByRole('button',{name:'查看本地恢复记录',exact:true}).click();}
async function open(page:Page,kind:'workspace'|'packet'){await page.goto(kind==='workspace'?'/?panel=workspace':'/?panel=packet-pagination&fixture=packet-recovery');await expect(panel(page,kind)).toBeVisible();if(kind==='workspace')await expect(page.getByRole('textbox',{name:'来源配置 JSON'})).toBeVisible();else await expect(page.getByLabel('Packet 分页范围')).toContainText('偏移 0');}
for(const kind of ['workspace','packet'] as const){
 test(`${kind}: no automatic inspection; explicit metadata read stays bounded and grants no runtime success`,async({page})=>{
  await open(page,kind);expect(await state(page)).toEqual([]);
  const button=panel(page,kind).getByRole('button',{name:'查看本地恢复记录',exact:true});expect((await button.boundingBox())!.height).toBeGreaterThanOrEqual(44);
  await click(page,kind);await expect(panel(page,kind).getByLabel('本地存储检查结果')).toBeVisible();expect(await state(page)).toEqual([kind]);
  await expect(panel(page,kind)).toContainText(kind==='workspace'?'候选正文和运行结果未在此校验':'不代表原会话已经结束');await expect(panel(page,kind)).not.toContainText('HOST_SECRET_CANARY');
  if(kind==='workspace'){await expect(panel(page,kind)).toContainText('18446744073709551615');expect(await page.evaluate(()=>(window as any).workspaceFixture.calls.filter((c:any)=>['apply','plan','save','complete','review'].includes(c.kind)))).toEqual([]);}
  else await expect(page.getByLabel('Packet 关闭提交次数')).toHaveText('0');
  expect(await panel(page,kind).evaluate(p=>p.scrollWidth<=p.clientWidth)).toBe(true);expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
 });
 test(`${kind}: absent storage, blocked storage and read failure are distinct; failure cannot erase evidence`,async({page})=>{
  await open(page,kind);await page.evaluate(()=>(window as any).storageInspectionFixture.mode='absent');await click(page,kind);await expect(panel(page,kind)).toContainText('尚无');await expect(panel(page,kind).getByRole('alert')).toHaveCount(0);
  await page.evaluate(()=>(window as any).storageInspectionFixture.mode='blocked');await click(page,kind);await expect(panel(page,kind)).toContainText('系统拒绝');
  await page.evaluate(()=>(window as any).storageInspectionFixture.fail=true);await click(page,kind);await expect(panel(page,kind).getByRole('alert')).toContainText('屏障保留');await expect(panel(page,kind)).toContainText('历史检查结果');await expect(panel(page,kind)).toContainText('系统拒绝');await expect(page.locator('body')).not.toContainText('HOST_SECRET_CANARY');
 });
 test(`${kind}: changing client generation rejects a held old check; only another explicit click reads again`,async({page})=>{
  await open(page,kind);await page.evaluate(()=>(window as any).storageInspectionFixture.hold=true);await click(page,kind);await expect(panel(page,kind)).toContainText('正在只读检查');await expect(panel(page,kind).getByRole('button')).toBeDisabled();
  await page.evaluate(()=>window.dispatchEvent(new CustomEvent('client-core:updated',{detail:{revision:2,scopeGeneration:3,scope:{profileId:'replacement',configRevision:2,coreInstanceId:2},sourceStatus:'ready',activeProbeJobs:[]}})));
  await page.evaluate(()=>(window as any).storageInspectionFixture.release());await expect(panel(page,kind).getByRole('button')).toBeEnabled();await expect(panel(page,kind).getByLabel('本地存储检查结果')).toHaveCount(0);expect(await state(page)).toEqual([kind]);
  await page.evaluate(()=>(window as any).storageInspectionFixture.hold=false);await click(page,kind);await expect(panel(page,kind).getByLabel('本地存储检查结果')).toBeVisible();expect(await state(page)).toEqual([kind,kind]);
 });
}
test('workspace read does not consume dirty draft or replace an existing validation/apply state',async({page})=>{
 await open(page,'workspace');const editor=page.getByRole('textbox',{name:'来源配置 JSON'}),draft=(await editor.inputValue())+'\n  ';await editor.fill(draft);await click(page,'workspace');await expect(editor).toHaveValue(draft);
 await page.getByRole('button',{name:'返回上级',exact:true}).click();await expect(page.getByRole('dialog')).toContainText('未保存');await page.getByRole('dialog').getByRole('button',{name:'继续编辑',exact:true}).click();await expect(editor).toHaveValue(draft);
 expect(await page.evaluate(()=>(window as any).workspaceFixture.calls.filter((c:any)=>['apply','plan','save','complete'].includes(c.kind)))).toEqual([]);
});
test('Packet inspection failure neither reconciles nor clears the original durable close barrier',async({page})=>{
 await open(page,'packet');const first=page.locator('.packet-panel article').filter({has:page.getByText('packet:0',{exact:true})}).first();await first.getByRole('button',{name:'关闭会话',exact:true}).click();await page.getByRole('dialog').getByRole('button',{name:'确认关闭',exact:true}).click();await expect(page.getByLabel('原 Packet 关闭屏障')).toContainText('close-operation-1');
 const before=await page.getByLabel('Packet 详情目标').textContent();await page.evaluate(()=>(window as any).storageInspectionFixture.fail=true);await click(page,'packet');await expect(panel(page,'packet').getByRole('alert')).toBeVisible();await expect(page.getByLabel('原 Packet 关闭屏障')).toContainText('close-operation-1');await expect(page.getByLabel('Packet 关闭提交次数')).toHaveText('1');expect(await page.getByLabel('Packet 详情目标').textContent()).toEqual(before);
});

test('workspace WAL-active result explains the unsupported live check without inventing empty counts or consuming dirty input',async({page})=>{
 await open(page,'workspace');const editor=page.getByRole('textbox',{name:'来源配置 JSON'}),draft=(await editor.inputValue())+'\n ';await editor.fill(draft);await page.evaluate(()=>(window as any).storageInspectionFixture.mode='wal');await click(page,'workspace');
 const result=panel(page,'workspace').getByLabel('本地存储检查结果');await expect(result).toContainText('不会忽略待合入数据');await expect(result.locator('dt').filter({hasText:'元资料记录'}).locator('+ dd')).toHaveText('—');await expect(result.locator('dt').filter({hasText:'待处理记录'}).locator('+ dd')).toHaveText('—');await expect(result).not.toContainText('元资料可读');await expect(editor).toHaveValue(draft);expect(await page.evaluate(()=>(window as any).workspaceFixture.calls.filter((c:any)=>['apply','plan','save','complete','review'].includes(c.kind)))).toEqual([]);
});
