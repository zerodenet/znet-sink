import { expect, test, type Page } from '@playwright/test';
test.use({ viewport: { width: 320, height: 720 } });
test.afterEach(async ({page}) => {
 const bounds = await page.locator('.packet-panel').evaluate(panel => ({width:panel.clientWidth,scroll:panel.scrollWidth}));
 expect(bounds.scroll).toBeLessThanOrEqual(bounds.width);
 const buttons = await page.getByRole('navigation',{name:'Packet 分页',exact:true}).getByRole('button').evaluateAll(nodes=>nodes.map(node=>({width:node.getBoundingClientRect().width,height:node.getBoundingClientRect().height})));
 for(const button of buttons) { expect(button.width).toBeGreaterThanOrEqual(44); expect(button.height).toBeGreaterThanOrEqual(44); }
});
const open = async (page: Page, mode='') => { await page.goto(`/?panel=packet-pagination&mode=${mode}`); await expect(page.getByLabel('Packet 分页范围')).toContainText('偏移 0'); };
const control = (page: Page, method: string) => page.evaluate(name => (window as unknown as {packetFixture:Record<string,()=>unknown>}).packetFixture[name](),method);
const firstRow = (page: Page) => page.locator('.packet-panel article').filter({has:page.getByText('packet:0',{exact:true})}).first();
const confirmClose = async (page: Page) => { await firstRow(page).getByRole('button',{name:'关闭会话',exact:true}).click(); await page.getByRole('dialog').getByRole('button',{name:'确认关闭',exact:true}).click(); await expect(page.getByLabel('Packet 关闭提交次数')).toHaveText('1'); };

test('bounded true pages reach the last item, return to the previous page, and retain lossless identifiers',async({page})=>{
 await open(page);
 await page.getByRole('button',{name:'下一页',exact:true}).click();await expect(page.getByLabel('Packet 分页范围')).toContainText('偏移 500');
 await page.getByRole('button',{name:'下一页',exact:true}).click();await expect(page.getByLabel('Packet 分页范围')).toContainText('偏移 1000');
 await expect(page.getByText('packet:1000',{exact:true})).toBeVisible();await expect(page.getByRole('button',{name:'下一页',exact:true})).toBeDisabled();
 await page.getByRole('button',{name:'详情',exact:true}).click();await expect(page.getByLabel('Packet 会话详情')).toContainText('18446744073709551615');
 await page.getByRole('button',{name:'上一页',exact:true}).click();await expect(page.getByLabel('Packet 分页范围')).toContainText('偏移 500');await expect(page.getByLabel('Packet 会话详情')).toHaveCount(0);
 await page.getByRole('button',{name:'重新检查',exact:true}).click();await expect(page.getByLabel('Packet 分页范围')).toContainText('偏移 500');
 await expect(page.getByLabel('Packet 后端请求偏移')).toContainText('0,500,1000,500,500');
});
test('search explicitly filters only the current page and does not change the bounded request',async({page})=>{
 await open(page);await page.getByRole('textbox',{name:'仅筛选本页 Packet 会话'}).fill('packet:1000');await expect(page.getByText('本页没有匹配的 Packet 会话；可清除筛选后翻页',{exact:true})).toBeVisible();
 await expect(page.getByText('仅筛选本页；每页独立读取实时会话，各页之间可能新增、结束或移位，不是冻结的完整历史。',{exact:true})).toBeVisible();
 await page.getByRole('button',{name:'下一页',exact:true}).click();await page.getByRole('button',{name:'下一页',exact:true}).click();await expect(page.getByText('packet:1000',{exact:true})).toBeVisible();
});
test('failed next page retains the old page stale, blocks old controls and retries the exact offset',async({page})=>{
 await open(page,'failed-next');await page.getByRole('button',{name:'下一页',exact:true}).click();await expect(page.getByRole('button',{name:'重试偏移 500',exact:true})).toBeVisible();await expect(page.getByLabel('Packet 分页范围')).toContainText('偏移 0');
 await expect(firstRow(page).getByRole('button',{name:'关闭会话',exact:true})).toBeDisabled();await expect(firstRow(page).getByRole('button',{name:'详情',exact:true})).toBeDisabled();
 await control(page,'allowNext');await page.getByRole('button',{name:'重试偏移 500',exact:true}).click();await expect(page.getByLabel('Packet 分页范围')).toContainText('偏移 500');
});
test('pending page blocks duplicate navigation and clears old details before publishing its result',async({page})=>{
 await open(page,'pending-next');await firstRow(page).getByRole('button',{name:'详情',exact:true}).click();await expect(page.getByLabel('Packet 会话详情')).toBeVisible();
 await page.getByRole('button',{name:'下一页',exact:true}).click();await expect(page.getByLabel('Packet 会话详情')).toHaveCount(0);await expect(page.getByRole('button',{name:'下一页',exact:true})).toBeDisabled();
 await control(page,'release');await expect(page.getByLabel('Packet 分页范围')).toContainText('偏移 500');
});
test('ordinary observation preserves a confirmation, but client generation ABA invalidates it',async({page})=>{
 await open(page);await firstRow(page).getByRole('button',{name:'关闭会话',exact:true}).click();await control(page,'observe');await expect(page.getByRole('dialog')).toBeVisible();
 await control(page,'changeScope');await control(page,'changeScope');await expect(page.getByRole('dialog')).toHaveCount(0);await expect(page.getByLabel('Packet 关闭提交次数')).toHaveText('0');await expect(page.getByLabel('Packet 分页范围')).toContainText('偏移 0');
});
test('unknown physical target remains blocked across pages, ABA and Panel remount until its fixed ended detail',async({page})=>{
 await open(page);await confirmClose(page);await expect(page.getByLabel('原 Packet 关闭屏障')).toContainText('packet:0');
 await page.getByRole('button',{name:'下一页',exact:true}).click();await expect(page.getByLabel('原 Packet 关闭屏障')).toContainText('packet:0');
 await control(page,'changeScope');await control(page,'changeScope');await control(page,'remount');await expect(page.getByLabel('Packet 分页范围')).toContainText('偏移 0');
 await expect(firstRow(page).getByRole('button',{name:'关闭会话',exact:true})).toBeDisabled();await expect(page.getByLabel('Packet 关闭提交次数')).toHaveText('1');
 await control(page,'endOriginal');await page.getByRole('button',{name:'只读核对原会话',exact:true}).click();await expect(page.getByLabel('原 Packet 关闭屏障')).toHaveCount(0);await expect(page.getByLabel('Packet 关闭提交次数')).toHaveText('1');
});
test('full physical ledger blocks new close risk while real pages remain readable',async({page})=>{
 await open(page,'full');await expect(firstRow(page).getByRole('button',{name:'关闭会话',exact:true})).toBeDisabled();await expect(page.getByText('关闭目标预算已满，暂停新增关闭操作；读取、筛选和翻页仍可用，请先核对原目标。',{exact:true})).toBeVisible();
 await page.getByRole('button',{name:'下一页',exact:true}).click();await expect(page.getByLabel('Packet 分页范围')).toContainText('偏移 500');await expect(page.getByLabel('Packet 关闭提交次数')).toHaveText('0');
});
test('unsupported capability is explained and is not presented as an empty available page',async({page})=>{
 await page.goto('/?panel=packet-pagination&mode=unsupported');await expect(page.getByText(/当前内核未声明 Packet 路径管理能力/)).toBeVisible();await expect(page.getByLabel('Packet 分页范围')).toHaveCount(0);await expect(page.getByText('当前没有 Packet 会话',{exact:true})).toHaveCount(0);
});
