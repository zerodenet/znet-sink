import {test,expect,type Page} from '@playwright/test';
const u64='18446744073709551615';
const raw=`{ "runtime": { "udp_upstream_idle_timeout_seconds": ${u64} }, "route": { "final": {"type":"direct"} } }`;
const fixture=(page:Page,fn:(control:any)=>unknown)=>page.evaluate(fnSource=>{const fn=eval(`(${fnSource})`);return fn((window as any).profileEditorFixture);},fn.toString());
const calls=(page:Page)=>fixture(page,control=>control.calls);
const openExisting=async(page:Page,id='日常网络配置')=>{await page.getByRole('button',{name:`编辑代理配置 ${id}`,exact:true}).click();await expect(page.getByLabel('配置 JSON 原文')).toBeVisible();};
test.use({viewport:{width:320,height:740}});

test('raw existing source preserves u64 independently of corrupted listing Value through format and local save',async({page})=>{
 await page.goto('/?panel=mobile-configuration&fixture=profile-editor');
 await openExisting(page);
 const body=page.getByLabel('配置 JSON 原文');
 await expect(body).toHaveValue(new RegExp(u64));
 expect(await fixture(page,control=>control.listingNumber)).not.toBe(u64);
 await body.fill(raw+'\n');
 await page.getByRole('button',{name:'校验并格式化',exact:true}).click();
 await expect(body).toHaveValue(raw);
 await page.getByRole('button',{name:'仅保存本地配置',exact:true}).click();
 await expect(page.getByText('已保存，待应用；本地保存不代表内核或 VPN 生效。',{exact:true})).toBeVisible();
 const sent=await calls(page);
 const save=sent.find((item:any)=>item.kind==='save').input;
 expect(save.sourceJson).toBe(raw);expect(save.active).toBeUndefined();expect(save.content).toBeUndefined();
 expect(sent.filter((item:any)=>item.kind==='activate')).toHaveLength(0);
 await expect(page.getByRole('button',{name:'前往工作区预检/应用',exact:true}).last()).toBeVisible();
 await page.getByRole('button',{name:'前往工作区预检/应用',exact:true}).last().click();
 await expect(page.getByLabel('移动配置导航')).toHaveText('workspace');
 await expect(body).toHaveCount(0);
});

test('new null host body opens editable inline text; saving is local and enabling requires separate user action',async({page})=>{
 await page.goto('/?panel=mobile-configuration&fixture=profile-editor');
 await page.getByRole('button',{name:'新建',exact:true}).click();
 const name=page.getByLabel('配置名称'),body=page.getByLabel('配置 JSON 原文');
 await expect(body).toHaveValue('{}');await name.fill('新的原文配置');await body.fill(raw);
 await page.getByRole('button',{name:'仅保存本地配置',exact:true}).click();
 await expect(page.getByRole('button',{name:'启用此配置',exact:true})).toBeVisible();
 expect((await calls(page)).filter((item:any)=>item.kind==='activate')).toHaveLength(0);
 await expect(page.getByText('当前生效',{exact:true})).toHaveCount(0);
 await page.getByRole('button',{name:'启用此配置',exact:true}).click();
 await expect.poll(async()=>(await calls(page)).filter((item:any)=>item.kind==='activate').length).toBe(1);
 expect((await calls(page)).filter((item:any)=>item.kind==='save')).toHaveLength(1);
});

test('desktop chooser reads file as raw u64 text and cancellation preserves it without save',async({page})=>{
 await page.goto('/?panel=profiles&fixture=profile-editor');
 await page.getByRole('button',{name:'新建',exact:true}).click();
 await page.getByRole('button',{name:'选择配置文件',exact:true}).click();
 const body=page.getByLabel('配置 JSON 原文');await expect(body).toHaveValue(new RegExp(u64));
 const original=await body.inputValue();
 await fixture(page,control=>{control.filePath=null;});
 await page.getByRole('button',{name:'选择配置文件',exact:true}).click();
 await expect(page.getByText('已取消文件选择，草稿保留。',{exact:true})).toBeVisible();
 await expect(body).toHaveValue(original);
 expect((await calls(page)).filter((item:any)=>item.kind==='save')).toHaveLength(0);
});

for(const kind of ['managed','linked'])test(`${kind} source is read only with no legacy fallback or save controls`,async({page})=>{
 await page.goto('/?panel=mobile-configuration&fixture=profile-editor');
 await page.evaluate(value=>{(window as any).profileEditorFixture.readOnly=value;},kind);
 await page.getByRole('button',{name:'编辑代理配置 日常网络配置',exact:true}).click();
 await expect(page.getByRole('button',{name:'查看来源管理',exact:true})).toBeVisible();
 await expect(page.getByRole('button',{name:'仅保存本地配置',exact:true})).toHaveCount(0);
 await expect(page.getByRole('button',{name:'选择配置文件',exact:true})).toHaveCount(0);
 if(kind==='managed')await expect(page.getByLabel('配置 JSON 原文')).toHaveCount(0);else{await expect(page.getByLabel('配置 JSON 原文')).toHaveValue(new RegExp(u64));await expect(page.getByLabel('配置 JSON 原文')).toHaveAttribute('readonly','');}
 expect((await calls(page)).filter((item:any)=>['save','validate','read-file'].includes(item.kind))).toHaveLength(0);
});

test('native field failure preserves source and fixed field message while a late formatter never replaces edits',async({page})=>{
 await page.goto('/?panel=mobile-configuration&fixture=profile-editor');await openExisting(page);
 const body=page.getByLabel('配置 JSON 原文');await body.fill(raw);
 await fixture(page,control=>{control.invalid=true;});
 await page.getByRole('button',{name:'仅保存本地配置',exact:true}).click();
 await expect(page.getByLabel('配置字段错误')).toContainText('runtime');
 await expect(body).toHaveValue(raw);await expect(page.getByRole('button',{name:'仅保存本地配置',exact:true})).toBeEnabled();
 await expect(page.locator('body')).not.toContainText('SECRET_EDITOR_CANARY');
 await fixture(page,control=>{control.invalid=false;control.holdValidate=true;});
 await page.getByRole('button',{name:'校验并格式化',exact:true}).click();
 await body.fill(raw+'  ');await fixture(page,control=>control.releaseValidate());
 await expect(body).toHaveValue(raw+'  ');
});

test('unknown create reads fixed original target metadata without replay; closing explains uncertain prior save',async({page})=>{
 await page.goto('/?panel=mobile-configuration&fixture=profile-editor');
 await page.getByRole('button',{name:'新建',exact:true}).click();
 await page.getByLabel('配置名称').fill('未确认草稿');await page.getByLabel('配置 JSON 原文').fill(raw);
 await fixture(page,control=>{control.failSave=true;});
 await page.getByRole('button',{name:'仅保存本地配置',exact:true}).click();
 await expect(page.getByRole('button',{name:'仅保存本地配置',exact:true})).toBeDisabled();
 await page.getByRole('button',{name:'只读核对本地版本',exact:true}).click();
 await expect(page.getByLabel('本地版本核对资料')).toContainText('manual-fixture-create');
 await expect(page.getByLabel('本地版本核对资料')).toContainText('不代表原操作成功或运行生效');
 expect((await calls(page)).filter((item:any)=>item.kind==='save')).toHaveLength(1);
 expect((await calls(page)).filter((item:any)=>item.kind==='snapshot').at(-1).input).toBe('manual-fixture-create');
 await page.locator('.profile-editor-footer').getByRole('button',{name:'关闭',exact:true}).click();
 await expect(page.getByText('放弃当前草稿并关闭，原本地保存结果仍未确认，不会撤回或重放。',{exact:true})).toBeVisible();
 await page.getByRole('button',{name:'继续编辑',exact:true}).click();await expect(page.getByLabel('配置 JSON 原文')).toHaveValue(raw);
});

test('dirty close cancellation keeps exact text and pending save blocks departure; 320px controls stay reachable',async({page})=>{
 await page.goto('/?panel=mobile-configuration&fixture=profile-editor');await openExisting(page,'工作配置');
 const body=page.getByLabel('配置 JSON 原文');await body.fill(raw);
 await page.keyboard.press('Escape');await expect(page.getByRole('heading',{name:'放弃未保存的修改？'})).toBeVisible();
 await page.getByRole('button',{name:'继续编辑',exact:true}).click();await expect(body).toHaveValue(raw);
 await fixture(page,control=>{control.holdSave=true;});await page.getByRole('button',{name:'仅保存本地配置',exact:true}).click();
 await expect(page.getByRole('button',{name:'保存中…',exact:true})).toBeDisabled();
 await expect(page.locator('.profile-editor-footer').getByRole('button',{name:'关闭',exact:true})).toBeDisabled();
 await page.keyboard.press('Escape');await expect(body).toBeVisible();
 await page.evaluate(()=>(window as any).profileEditorNavigation.selectTab('nodes'));
 await expect(page.getByRole('heading',{name:'配置正在保存'})).toBeVisible();
 expect(await page.evaluate(()=>(window as any).profileEditorNavigation.activeTab)).toBe('configuration');
 await page.getByRole('button',{name:'继续编辑',exact:true}).click();
 await fixture(page,control=>control.releaseSave());await expect(page.getByRole('button',{name:'启用此配置',exact:true})).toBeVisible();
 const controls=page.locator('.profile-editor-form input, .profile-editor-footer button');
 for(const item of await controls.all()){const box=await item.boundingBox();if(box){expect(box.height).toBeGreaterThanOrEqual(44);expect(box.x).toBeGreaterThanOrEqual(0);expect(box.x+box.width).toBeLessThanOrEqual(320);}}
 expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
 await page.locator('.profile-editor-footer').getByRole('button',{name:'关闭',exact:true}).click();await expect(body).toHaveCount(0);
 await page.evaluate(()=>(window as any).profileEditorNavigation.selectTab('nodes'));
 expect(await page.evaluate(()=>(window as any).profileEditorNavigation.activeTab)).toBe('nodes');
});
