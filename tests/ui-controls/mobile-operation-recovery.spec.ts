import { test, expect, type Page } from '@playwright/test';
// These fixtures reconstruct host facts and UI, not a real process or native VPN.
test.use({ viewport: { width: 320, height: 800 } });
const workspaceURL = '/?panel=workspace&fixture=workspace-recovery';
async function workspace(page: Page, kind='local') {
  await page.goto(workspaceURL);
  await expect(page.getByRole('textbox',{name:'来源配置 JSON'})).toBeVisible();
  await page.evaluate(kind => (window as any).workspaceFixture.seedRecovery(kind), kind);
  await page.reload();
  await expect(page.getByLabel('原工作区操作恢复')).toContainText('original-operation');
}
const calls = (page: Page) => page.evaluate(() => (window as any).workspaceFixture.calls as {kind:string;input?:any}[]);
async function check(page: Page) { await page.getByRole('button',{name:'检查原操作恢复',exact:true}).click(); }
async function openCompletion(page: Page) { await page.getByRole('button',{name:'完成原本地发布',exact:true}).click(); await expect(page.getByRole('dialog')).toContainText('完成原操作的本地发布？'); }
async function noRuntimeReplay(page: Page) { expect((await calls(page)).filter(c=>['plan','apply','save'].includes(c.kind))).toHaveLength(0); }

test('host reconstruction restores metadata only; explicit review and confirmation complete exactly the original local publication', async({page})=>{
  await workspace(page); const section=page.getByLabel('原工作区操作恢复');
  await expect(section).toContainText('不代表当前运行态仍有效');
  await expect(page.getByText('运行态已确认',{exact:true})).toHaveCount(0);
  expect((await calls(page)).filter(c=>['review','complete'].includes(c.kind))).toHaveLength(0);
  await check(page); await expect(section).toContainText('已重新核对');
  expect((await page.getByRole('button',{name:'完成原本地发布',exact:true}).boundingBox())!.height).toBeGreaterThanOrEqual(44);
  await openCompletion(page);expect((await page.getByRole('dialog').getByRole('button',{name:'确认完成本地发布',exact:true}).boundingBox())!.height).toBeGreaterThanOrEqual(44);
  await page.getByRole('dialog').getByRole('button',{name:'取消',exact:true}).click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  expect((await calls(page)).filter(c=>c.kind==='complete')).toHaveLength(0);
  await openCompletion(page);await page.getByRole('dialog').getByRole('button',{name:'确认完成本地发布',exact:true}).click();
  await expect(page.getByText('运行态已确认',{exact:true})).toBeVisible();
  await expect(page.getByRole('textbox',{name:'来源配置 JSON'})).toHaveValue(/"reject"/);
  await expect(page.getByRole('textbox',{name:'来源配置 JSON'})).toHaveValue(/18446744073709551615/);
  const submits=(await calls(page)).filter(c=>c.kind==='complete');expect(submits).toHaveLength(1);expect(submits[0].input).toEqual({planId:'original-operation',reviewId:'review-1'});
  await noRuntimeReplay(page);expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
});
for(const kind of ['unknown','stale'])test(`${kind} restored operation stays history without a new plan, local completion or runtime success`,async({page})=>{
  await workspace(page,kind);await check(page);
  await expect(page.getByLabel('原工作区操作恢复')).toContainText(kind==='stale'?'仅保留历史':'原运行结果未确认');
  await expect(page.getByRole('button',{name:'完成原本地发布',exact:true})).toHaveCount(0);
  await expect(page.getByText('运行态已确认',{exact:true})).toHaveCount(0);
  expect((await calls(page)).filter(c=>c.kind==='complete')).toHaveLength(0);await noRuntimeReplay(page);
});
test('dirty recovery inspection and cancelled leave preserve the exact draft; expired review never becomes a submit permit',async({page})=>{
  await workspace(page);const editor=page.getByRole('textbox',{name:'来源配置 JSON'}),draft=(await editor.inputValue())+'\n ';
  await editor.fill(draft);await check(page);await expect(page.getByRole('button',{name:'完成原本地发布',exact:true})).toBeDisabled();
  await page.getByRole('button',{name:'返回上级',exact:true}).click();await page.getByRole('dialog').getByRole('button',{name:'继续编辑',exact:true}).click();await expect(editor).toHaveValue(draft);
  await page.getByRole('button',{name:'重置',exact:true}).click();await page.evaluate(()=>(window as any).workspaceFixture.expireReview=true);await check(page);
  await expect(page.getByLabel('原工作区操作恢复')).toContainText('恢复检查已过期');await expect(page.getByRole('button',{name:'完成原本地发布',exact:true})).toBeDisabled();
  expect((await calls(page)).filter(c=>c.kind==='complete')).toHaveLength(0);await noRuntimeReplay(page);
});
test('pending local completion keeps the existing submit leave guard and storage failure preserves the original barrier without automatic retry',async({page})=>{
  await workspace(page);await check(page);await page.evaluate(()=>{const f=(window as any).workspaceFixture;f.holdComplete=true;f.failComplete=true});await openCompletion(page);
  await page.getByRole('dialog').getByRole('button',{name:'确认完成本地发布',exact:true}).click();await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(page.getByText('原本地发布提交中',{exact:true})).toBeVisible();await expect(page.getByRole('button',{name:'检查原操作恢复',exact:true})).toBeDisabled();
  await page.getByRole('button',{name:'返回上级',exact:true}).click();await expect(page.getByRole('dialog')).toContainText('设置正在提交');
  await page.getByRole('dialog').getByRole('button',{name:'继续编辑',exact:true}).click();await page.evaluate(()=>(window as any).workspaceFixture.releaseComplete());
  await expect(page.getByText('本地发布失败；原屏障保留，请重新检查',{exact:true})).toBeVisible();await expect(page.getByLabel('原工作区操作恢复')).toContainText('original-operation');
  expect((await calls(page)).filter(c=>c.kind==='complete')).toHaveLength(1);await noRuntimeReplay(page);
});

const packetURL='/?panel=packet-pagination&fixture=packet-recovery';
async function packet(page: Page) { await page.goto(packetURL);await expect(page.getByLabel('Packet 分页范围')).toContainText('偏移 0'); }
const control=(page: Page,name:string)=>page.evaluate(name=>(window as any).packetFixture[name](),name);
const first=(page: Page)=>page.locator('.packet-panel article').filter({has:page.getByText('packet:0',{exact:true})}).first();
async function closeFirst(page: Page) { await first(page).getByRole('button',{name:'关闭会话',exact:true}).click();await page.getByRole('dialog').getByRole('button',{name:'确认关闭',exact:true}).click();await expect(page.getByLabel('Packet 关闭提交次数')).toHaveText('1'); }
async function rebuildPacket(page: Page) { await control(page,'prepareHostRebuild');await page.reload();await expect(page.getByLabel('Packet 分页范围')).toContainText('偏移 0'); }
test('host reconstruction retains original Packet operation and only explicit readonly ended receipt clears its durable barrier',async({page})=>{
  await packet(page);await closeFirst(page);await rebuildPacket(page);
  const barrier=page.getByLabel('原 Packet 关闭屏障');await expect(barrier).toContainText('close-operation-1');await expect(barrier).toContainText('从宿主持久记录恢复');await expect(page.getByLabel('Packet 关闭提交次数')).toHaveText('1');
  await expect(first(page).getByRole('button',{name:'关闭会话',exact:true})).toBeDisabled();await expect(page.getByText('这是原操作元资料，不代表会话已关闭，不会自动重放。',{exact:false})).toBeVisible();
  await barrier.getByRole('button',{name:'只读核对原会话',exact:true}).click();await expect(barrier).toBeVisible();await expect(page.getByLabel('Packet 详情目标')).toContainText('"hostSessionId":"host-session-reconstructed"');await expect(page.getByLabel('Packet 详情目标')).toContainText('"closeOperationId":"close-operation-1"');
  await control(page,'endOriginal');await barrier.getByRole('button',{name:'只读核对原会话',exact:true}).click();await expect(barrier).toHaveCount(0);await expect(page.getByLabel('Packet 关闭提交次数')).toHaveText('1');
  expect(await page.locator('.packet-panel').evaluate(p=>p.scrollWidth<=p.clientWidth)).toBe(true);
});
test('restored Packet metadata from a retired core is explicit history and never controls a same-name replacement',async({page})=>{
  await packet(page);await closeFirst(page);await control(page,'changeCore');await rebuildPacket(page);const barrier=page.getByLabel('原 Packet 关闭屏障');await expect(barrier).toContainText('仅作历史');await expect(barrier.getByRole('button',{name:'只读核对原会话',exact:true})).toBeDisabled();await expect(page.getByLabel('Packet 关闭提交次数')).toHaveText('1');
});
test('Packet storage failure stays visible and prevents false ended delivery or repeated close after host reconstruction',async({page})=>{
  await packet(page);await closeFirst(page);await control(page,'blockStorage');await rebuildPacket(page);await expect(page.getByRole('alert').filter({hasText:'关闭账本写入失败'})).toBeVisible();await expect(first(page).getByRole('button',{name:'关闭会话',exact:true})).toBeDisabled();
  await control(page,'endOriginal');await page.getByLabel('原 Packet 关闭屏障').getByRole('button',{name:'只读核对原会话',exact:true}).click();await expect(page.getByLabel('原 Packet 关闭屏障')).toContainText('close-operation-1');await expect(page.getByText('原物理会话已确认结束',{exact:true})).toHaveCount(0);await expect(page.getByLabel('Packet 关闭提交次数')).toHaveText('1');
});
test('a new host with the same numeric generation cannot receive an old confirmation; fresh first-page discovery is explicit',async({page})=>{
  await packet(page);await first(page).getByRole('button',{name:'关闭会话',exact:true}).click();await control(page,'replaceHost');await page.getByRole('dialog').getByRole('button',{name:'确认关闭',exact:true}).click();
  await expect(page.getByLabel('Packet 关闭提交次数')).toHaveText('0');await page.getByRole('button',{name:'返回第一页重新检查',exact:true}).click();await expect(page.getByLabel('Packet 分页范围')).toContainText('偏移 0');await closeFirst(page);await expect(page.getByLabel('Packet 原始关闭目标')).toContainText('"hostSessionId":"different-live-host"');
});
