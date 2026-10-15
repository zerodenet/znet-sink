import {test,expect,type Page} from '@playwright/test';
test.use({viewport:{width:320,height:740}});
const control=(page:Page,fn:(c:any)=>unknown)=>page.evaluate(source=>{const fn=eval(`(${source})`);return fn((window as any).flowCloseFixture);},fn.toString());
const calls=(page:Page)=>control(page,c=>c.calls);
async function open(page:Page){await page.getByRole('button',{name:'更多操作',exact:true}).click();await page.getByRole('menuitem',{name:'选择连接关闭',exact:true}).click();await expect(page.getByLabel('本次真实连接列表')).toBeVisible();}
async function select(page:Page,id:string){await page.getByRole('checkbox',{name:`选择连接 ${id} actual-${id}.test:443`,exact:true}).check();}
async function preview(page:Page,count:number){await page.getByRole('button',{name:'预览所选关闭',exact:true}).click();await expect(page.getByRole('button',{name:`确认关闭 ${count} 条`,exact:true})).toBeVisible();}
const closeModal=(page:Page)=>page.locator('.flow-close-footer').getByRole('button',{name:'关闭',exact:true}).click();

test('old paused row opens canonical inventory with no inherited selection and current results do not suppress event rows',async({page})=>{
 await page.goto('/?panel=flow-close');await page.getByRole('button',{name:'暂停查看',exact:true}).click();
 await page.getByRole('button',{name:'终止连接 old-display-1.test:443',exact:true}).click();
 await expect(page.getByText('原活动行或暂停内容可能过期，不能据其同名 ID 关闭新连接。请在以下真实核对列表重新明确选择。',{exact:true})).toBeVisible();
 await expect(page.getByRole('checkbox',{checked:true})).toHaveCount(0);
 await expect(page.getByRole('button',{name:'预览所选关闭',exact:true})).toBeDisabled();
 await select(page,'1');await select(page,'2');await preview(page,2);
 await page.evaluate(()=>window.dispatchEvent(new Event('fixture-flow-event')));
 await control(page,c=>{c.extra=true;});
 await expect(page.getByLabel('固定关闭目标')).toContainText('#1');await expect(page.getByLabel('固定关闭目标')).not.toContainText('#99');
 await page.getByRole('button',{name:'确认关闭 2 条',exact:true}).click();
 await expect(page.getByLabel('逐项关闭结果')).toContainText('#1 · 已关闭');await expect(page.getByLabel('逐项关闭结果')).toContainText('#2 · 已结束');
 const sent=(await calls(page)).find((c:any)=>c.kind==='preview').input.targets;
 expect(sent).toEqual([{flowId:'1',targetKey:'physical-1-1'},{flowId:'2',targetKey:'physical-1-2'}]);
 await closeModal(page);
 await expect(page.getByRole('button',{name:'查看连接 old-display-1.test:443',exact:true})).toBeVisible();
 await expect(page.getByRole('button',{name:'查看连接 old-display-99.test:443',exact:true})).toHaveCount(0);
 await page.getByRole('button',{name:'继续查看',exact:true}).click();
 await expect(page.getByRole('button',{name:'查看连接 old-display-99.test:443',exact:true})).toBeVisible();
});

test('explicit selection of bounded current inventory reports every mixed outcome and keeps u64 flow IDs',async({page})=>{
 await page.goto('/?panel=flow-close');await open(page);
 await page.getByRole('button',{name:'选择当前核对列表',exact:true}).click();await preview(page,6);
 await page.getByRole('button',{name:'确认关闭 6 条',exact:true}).click();
 const results=page.getByLabel('逐项关闭结果');
 for(const value of ['#1 · 已关闭','#2 · 已结束','#3 · 已拒绝','#4 · 结果未确认','#5 · 已失效','#18446744073709551615 · 未提交'])await expect(results).toContainText(value);
 expect((await calls(page)).filter((c:any)=>c.kind==='submit')).toHaveLength(1);
 expect((await calls(page)).find((c:any)=>c.kind==='preview').input.targets.at(-1).flowId).toBe('18446744073709551615');
 await expect(page.getByText('仅记录原操作回执，不清空统计或本地历史。活动列表仍等待实际观测更新。',{exact:true})).toBeVisible();
});

for(const mode of ['closed-false','missing'])test(`${mode} successful transport never fabricates all-closed and status reads only the original operation`,async({page})=>{
 await page.goto('/?panel=flow-close');await page.evaluate(value=>{(window as any).flowCloseFixture.mode=value;},mode);await open(page);
 await select(page,'1');await select(page,'2');await preview(page,2);await page.getByRole('button',{name:'确认关闭 2 条',exact:true}).click();
 await expect(page.getByLabel('逐项关闭结果')).toContainText('#2 · 结果未确认');
 if(mode==='closed-false')await expect(page.getByLabel('逐项关闭结果')).toContainText('#1 · 结果未确认');
 const original=(await calls(page)).find((c:any)=>c.kind==='submit').input;
 await page.getByRole('button',{name:'只读核对原操作',exact:true}).click();
 expect((await calls(page)).filter((c:any)=>c.kind==='submit')).toHaveLength(1);
 expect((await calls(page)).filter((c:any)=>c.kind==='status').at(-1).input).toBe(original);
});

test('cancel releases unsubmitted plan and a fresh same-ID generation requires a new explicit selection',async({page})=>{
 await page.goto('/?panel=flow-close');await open(page);await select(page,'1');await preview(page,1);await closeModal(page);
 expect((await calls(page)).filter((c:any)=>c.kind==='submit')).toHaveLength(0);
 expect((await calls(page)).filter((c:any)=>c.kind==='discard')).toHaveLength(1);
 await control(page,c=>{c.generation=3;});await open(page);
 await expect(page.getByRole('checkbox',{checked:true})).toHaveCount(0);await expect(page.getByRole('button',{name:'预览所选关闭',exact:true})).toBeDisabled();
 await select(page,'1');await preview(page,1);
 expect((await calls(page)).filter((c:any)=>c.kind==='preview').at(-1).input.targets).toEqual([{flowId:'1',targetKey:'physical-3-1'}]);
});

test('scope changes while preview is in flight preserve original frozen targets and produce only stale receipts',async({page})=>{
 await page.goto('/?panel=flow-close');await open(page);await select(page,'1');await control(page,c=>{c.holdPreview=true;});
 await page.getByRole('button',{name:'预览所选关闭',exact:true}).click();
 await expect(page.getByText('正在预检原固定目标…',{exact:true})).toBeVisible();
 await control(page,c=>{c.generation=2;c.extra=true;c.releasePreview();});
 await expect(page.getByRole('button',{name:'确认关闭 1 条',exact:true})).toBeVisible();await expect(page.getByLabel('固定关闭目标')).not.toContainText('#99');
 await page.getByRole('button',{name:'确认关闭 1 条',exact:true}).click();await expect(page.getByLabel('逐项关闭结果')).toContainText('#1 · 已失效');
 expect((await calls(page)).find((c:any)=>c.kind==='preview').input.targets).toEqual([{flowId:'1',targetKey:'physical-1-1'}]);
 await page.getByRole('button',{name:'重新读取并明确选择',exact:true}).click();await expect(page.getByRole('checkbox',{checked:true})).toHaveCount(0);
});

test('lost submit reply retains original receipt across failed inventory reads without automatic replay',async({page})=>{
 await page.goto('/?panel=flow-close');await control(page,c=>{c.mode='timeout';});await open(page);await select(page,'1');await preview(page,1);
 await page.getByRole('button',{name:'确认关闭 1 条',exact:true}).click();await expect(page.getByLabel('逐项关闭结果')).toContainText('#1 · 结果未确认');
 const original=(await calls(page)).find((c:any)=>c.kind==='submit').input;
 await control(page,c=>{c.inventoryMode='failed';});await page.getByRole('button',{name:'重新读取并明确选择',exact:true}).click();
 await expect(page.getByRole('alert')).toContainText('无法完成本次核对');
 await page.getByText('最近 1 次原操作回执',{exact:true}).click();await page.getByRole('button',{name:`查看原操作 ${original}`,exact:true}).click();
 await expect(page.getByLabel('逐项关闭结果')).toContainText('#1 · 结果未确认');
 await page.getByRole('button',{name:'只读核对原操作',exact:true}).click();
 expect((await calls(page)).filter((c:any)=>c.kind==='submit')).toHaveLength(1);
 expect((await calls(page)).filter((c:any)=>c.kind==='status').at(-1).input).toBe(original);
 await expect(page.locator('body')).not.toContainText('SECRET_FLOW_CANARY');
});

test('view unmount does not cancel submitted host work; remount reveals the original result without replay',async({page})=>{
 await page.goto('/?panel=flow-close');await control(page,c=>{c.holdSubmit=true;c.mode='all-closed';});await open(page);await select(page,'1');await preview(page,1);
 await page.getByRole('button',{name:'确认关闭 1 条',exact:true}).click();await expect(page.getByText('等待原操作逐项回执，不会重复提交…',{exact:true})).toBeVisible();
 await page.evaluate(()=>window.dispatchEvent(new CustomEvent('fixture-flow-mount',{detail:false})));
 await expect(page.getByRole('button',{name:'更多操作',exact:true})).toHaveCount(0);
 await control(page,c=>c.releaseSubmit());
 await page.evaluate(()=>window.dispatchEvent(new CustomEvent('fixture-flow-mount',{detail:true})));
 await page.getByRole('button',{name:'查看关闭结果',exact:true}).click();await expect(page.getByLabel('逐项关闭结果')).toContainText('#1 · 已关闭');
 expect((await calls(page)).filter((c:any)=>c.kind==='submit')).toHaveLength(1);expect((await calls(page)).filter((c:any)=>c.kind==='discard')).toHaveLength(0);
});

for(const mode of ['empty','unsupported'])test(`actual inventory ${mode} is explicit and never creates a preview or submit`,async({page})=>{
 await page.goto('/?panel=flow-close');await page.evaluate(value=>{(window as any).flowCloseFixture.inventoryMode=value;},mode);
 await page.getByRole('button',{name:'更多操作',exact:true}).click();await page.getByRole('menuitem',{name:'选择连接关闭',exact:true}).click();
 await expect(page.getByText(mode==='empty'?'实际快照没有活动连接。':'当前平台或内核未接入真实连接关闭能力。',{exact:true})).toBeVisible();
 expect((await calls(page)).filter((c:any)=>['submit','preview'].includes(c.kind))).toHaveLength(0);await expect(page.locator('body')).not.toContainText('SECRET_FLOW_CANARY');
});

test('320px canonical picker controls stay reachable and list selection has a 44px touch target',async({page})=>{
 await page.goto('/?panel=flow-close');await open(page);
 await page.getByRole('textbox',{name:'搜索待关闭连接',exact:true}).fill('actual-1.test');
 await expect(page.getByRole('checkbox')).toHaveCount(1);await select(page,'1');
 for(const item of await page.locator('.flow-close-choice,.flow-close-footer button,.flow-close-workspace input[type=text]').all()){const b=await item.boundingBox();if(b){expect(b.height).toBeGreaterThanOrEqual(44);expect(b.x).toBeGreaterThanOrEqual(0);expect(b.x+b.width).toBeLessThanOrEqual(320);}}
 expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
 await preview(page,1);await expect(page.getByLabel('固定关闭目标')).toContainText('#1');
});
