import {test, expect} from '@playwright/test';
test.use({viewport:{width:320,height:800}});
const url='/?panel=workspace';
test('offline native JSON format and local save preserve u64 text and do not apply',async({page})=>{
 await page.goto(url);const editor=page.getByRole('textbox',{name:'来源配置 JSON'});await expect(editor).toHaveValue(/18446744073709551615/);
 await editor.fill((await editor.inputValue())+'\n');await page.getByRole('button',{name:'格式化',exact:true}).click();await expect(editor).toHaveValue(/18446744073709551615/);
 await page.getByRole('button',{name:'最终生效',exact:true}).click();await expect(page.getByText('这是本地合成候选，尚未确认运行生效；不是已提交成功的运行配置。')).toBeVisible();await page.getByRole('button',{name:'来源编辑',exact:true}).click();
 await page.getByRole('button',{name:'保存本地',exact:true}).click();await expect(page.getByRole('status').filter({hasText:'已保存，待应用；保存不等于内核或 VPN 生效。'})).toBeVisible();
 const calls=await page.evaluate(()=>(window as any).workspaceFixture.calls);expect(calls.filter((c:any)=>c.kind==='save')).toHaveLength(1);expect(calls.filter((c:any)=>c.kind==='apply')).toHaveLength(0);expect(calls.find((c:any)=>c.kind==='save').input.sourceJson).toContain('18446744073709551615');
 expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
 const box=await page.getByRole('button',{name:'保存本地',exact:true}).boundingBox();expect(box!.height).toBeGreaterThanOrEqual(44);
});
test('external same-millisecond source update retains dirty input until explicit refresh',async({page})=>{
 await page.goto(url);const editor=page.getByRole('textbox',{name:'来源配置 JSON'});await expect(editor).toBeVisible();const draft=(await editor.inputValue()).replace('"direct"','"reject"')+'\n ';await editor.fill(draft);
 await page.getByRole('button',{name:'外部同毫秒更新'}).click();await expect(page.getByRole('alert').filter({hasText:'原草稿保留'})).toBeVisible();expect(await editor.inputValue()).toBe(draft);
 await expect(page.getByRole('button',{name:'保存本地',exact:true})).toBeDisabled();await page.getByRole('button',{name:'刷新配置来源'}).click();await expect(editor).toHaveValue(draft);
 await page.getByRole('button',{name:'继续编辑',exact:true}).click();expect(await editor.inputValue()).toBe(draft);
 await page.getByRole('button',{name:'刷新配置来源'}).click();await page.getByRole('button',{name:'放弃草稿并刷新'}).click();await expect(page.getByRole('alertdialog')).toHaveCount(0);await expect(page.getByRole('button',{name:'保存本地',exact:true})).toBeEnabled();
});
test('managed source exposes metadata reason and no source/override/effective text',async({page})=>{
 await page.goto(url);await expect(page.getByRole('textbox',{name:'来源配置 JSON'})).toBeVisible();await page.getByRole('button',{name:'切换托管来源'}).click();
 await expect(page.getByRole('status').filter({hasText:'普通工作区不读取正文'})).toBeVisible();await expect(page.getByRole('textbox',{name:'来源配置 JSON'})).toHaveCount(0);await expect(page.getByRole('button',{name:'保存本地',exact:true})).toBeDisabled();
 for(const name of ['客户端覆盖','最终生效']){await page.getByRole('button',{name,exact:true}).click();await expect(page.getByText('来源正文只读且未提供，请使用来源管理入口。')).toBeVisible();}
 await expect(page.getByText('18446744073709551615')).toHaveCount(0);
});
test('unknown apply retains original operation and user recheck never plans or applies again',async({page})=>{
 await page.goto(url);await expect(page.getByRole('textbox',{name:'来源配置 JSON'})).toBeVisible();await page.getByRole('button',{name:'模拟回执丢失'}).click();await page.getByRole('button',{name:'应用',exact:true}).click();
 await expect(page.getByText('原事务结果未确认；只核对原操作，不会重新提交。')).toBeVisible();await page.getByRole('button',{name:'核对原事务'}).click();
 const calls=await page.evaluate(()=>(window as any).workspaceFixture.calls);expect(calls.filter((c:any)=>c.kind==='apply')).toHaveLength(1);expect(calls.filter((c:any)=>c.kind==='plan')).toHaveLength(1);expect(calls.filter((c:any)=>c.kind==='operation')).toHaveLength(1);
});

test('one route-bound dirty guard retains input on return and discards only after explicit tab or return confirmation',async({page})=>{
 await page.goto(url);
 const editor=page.getByRole('textbox',{name:'来源配置 JSON'});
 await expect(editor).toHaveValue(/18446744073709551615/);
 const draft=(await editor.inputValue())+'\n ';
 await editor.fill(draft);
 await page.getByRole('button',{name:'返回上级',exact:true}).click();
 const dialog=page.getByRole('dialog');
 await expect(dialog).toHaveCount(1);
 await expect(dialog.getByRole('heading',{name:'放弃未保存的设置？'})).toBeVisible();
 await dialog.getByRole('button',{name:'继续编辑',exact:true}).click();
 await expect(page.getByLabel('导航位置')).toHaveText('more:workspace');
 await expect(editor).toHaveValue(draft);
 await page.getByRole('button',{name:'切换节点',exact:true}).click();
 await expect(dialog).toHaveCount(1);
 await dialog.getByRole('button',{name:'放弃修改',exact:true}).click();
 await expect(page.getByLabel('导航位置')).toHaveText('nodes:root');
 await page.getByRole('button',{name:'回到工作区',exact:true}).click();
 await expect(editor).not.toHaveValue(draft);
 // A new return route keeps the same guard semantics after the pane remounts.
 await editor.fill(draft);
 await page.getByRole('button',{name:'返回上级',exact:true}).click();
 await dialog.getByRole('button',{name:'放弃修改',exact:true}).click();
 await expect(page.getByLabel('导航位置')).toHaveText('more:root');
 expect(await page.evaluate(()=>(window as any).workspaceFixture.calls.filter((c:any)=>['save','apply'].includes(c.kind)))).toHaveLength(0);
});

test('busy local save blocks both return and tab leave without discard or queued navigation after the result',async({page})=>{
 await page.goto(url);
 const editor=page.getByRole('textbox',{name:'来源配置 JSON'});
 await expect(editor).toHaveValue(/18446744073709551615/);
 await editor.fill((await editor.inputValue())+'\n ');
 await page.evaluate(()=>(window as any).workspaceFixture.holdSave=true);
 await page.getByRole('button',{name:'保存本地',exact:true}).click();
 await expect(page.getByText('本地保存中',{exact:true})).toBeVisible();
 await page.getByRole('button',{name:'返回上级',exact:true}).click();
 const dialog=page.getByRole('dialog');
 await expect(dialog).toHaveCount(1);
 await expect(dialog.getByRole('heading',{name:'设置正在提交',exact:true})).toBeVisible();
 await expect(dialog.getByRole('button',{name:'放弃修改',exact:true})).toHaveCount(0);
 await dialog.getByRole('button',{name:'继续编辑',exact:true}).click();
 await page.getByRole('button',{name:'切换节点',exact:true}).click();
 await expect(dialog.getByRole('heading',{name:'设置正在提交',exact:true})).toBeVisible();
 await expect(page.getByLabel('导航位置')).toHaveText('more:workspace');
 await page.evaluate(()=>(window as any).workspaceFixture.releaseSave());
 await expect(dialog).toHaveCount(0);
 await expect(page.getByRole('status').filter({hasText:'已保存，待应用；保存不等于内核或 VPN 生效。'})).toBeVisible();
 await expect(page.getByLabel('导航位置')).toHaveText('more:workspace');
 await page.getByRole('button',{name:'切换节点',exact:true}).click();
 await expect(page.getByLabel('导航位置')).toHaveText('nodes:root');
 const calls=await page.evaluate(()=>(window as any).workspaceFixture.calls);
 expect(calls.filter((c:any)=>c.kind==='save')).toHaveLength(1);
 expect(calls.filter((c:any)=>c.kind==='apply')).toHaveLength(0);
});
