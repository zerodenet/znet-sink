import {test,expect,type Page} from '@playwright/test';

test.use({viewport:{width:320,height:740}});
const active=(page:Page)=>page.locator('.tab-pane:not(.inactive)');
const nav=(page:Page)=>page.getByRole('navigation',{name:'主导航'});
const tab=(page:Page,name:string)=>nav(page).getByRole('button',{name,exact:true});
const secondary=(page:Page)=>active(page).locator('.secondary-pane:not(.inactive)');
const profileCard=(page:Page)=>active(page).getByRole('button',{name:/^当前配置/});
const outletCard=(page:Page)=>active(page).getByRole('button',{name:/当前有效出口/});
async function open(page:Page,mode='global',target='outer-auto'){
  await page.goto(`/mobile.html?fixture=mobile-location&traffic-v1&mode=${mode}&target=${target}`);
  await expect(profileCard(page)).toContainText('同名配置');
  await expect(profileCard(page)).toBeEnabled();
}
async function facts(page:Page,change:Record<string,string|boolean>){
  await page.evaluate(value=>Object.assign((window as unknown as {mobileLocationFixture:Record<string,unknown>}).mobileLocationFixture,value),change);
}
async function release(page:Page,kind:'releaseLocation'|'releaseNodes'){
  await page.evaluate(name=>(window as unknown as {mobileLocationFixture:Record<string,()=>void>}).mobileLocationFixture[name](),kind);
}
async function visitProfile(page:Page){await profileCard(page).click();await expect(page.getByRole('region',{name:'概览配置定位'})).toBeVisible();}
async function visitNodes(page:Page){await outletCard(page).click();await expect(page.getByRole('region',{name:'概览出口定位'})).toBeVisible();}
async function requestOverview(page:Page){await page.evaluate(()=>(window as unknown as {mobileLocationNavigation:{selectTab:(tab:'overview')=>void}}).mobileLocationNavigation.selectTab('overview'));}

test('profile shortcut preserves old destination stack, subscription segment and original root search',async({page})=>{
  await open(page);await tab(page,'配置').click();
  await page.getByRole('textbox',{name:'搜索配置'}).fill('work');
  await page.getByRole('radio',{name:'订阅',exact:true}).click();
  await page.getByRole('button',{name:'配置工作区',exact:true}).click();
  await expect(secondary(page).locator('h1')).toHaveText('配置工作区');
  await tab(page,'概览').click();await visitProfile(page);
  await expect(secondary(page).locator('h1')).toHaveText('配置');
  await expect(secondary(page).locator('.config-row.location-target')).toHaveAttribute('data-profile-id','main');
  await page.getByRole('button',{name:'返回',exact:true}).click();
  await expect(tab(page,'概览')).toHaveAttribute('aria-current','page');
  await tab(page,'配置').click();await expect(secondary(page).locator('h1')).toHaveText('配置工作区');
  await page.getByRole('button',{name:'返回',exact:true}).click();
  await expect(page.getByRole('radio',{name:'订阅',exact:true})).toHaveAttribute('aria-checked','true');
  await page.getByRole('radio',{name:'配置',exact:true}).click();
  await expect(page.getByRole('textbox',{name:'搜索配置'})).toHaveValue('work');
});

test('duplicate profile names locate only the fixed ID and never automatically open or save its editor',async({page})=>{
  await open(page);await visitProfile(page);
  const target=secondary(page).locator('.config-row.location-target');
  await expect(target).toHaveCount(1);await expect(target).toHaveAttribute('data-profile-id','main');
  await expect(page.getByLabel('配置 JSON 原文')).toHaveCount(0);
  await target.getByRole('button',{name:'编辑代理配置 同名配置',exact:true}).click();
  await expect(page.getByLabel('配置 JSON 原文')).toBeVisible();
  const calls=await page.evaluate(()=>(window as unknown as {profileEditorFixture:{calls:{kind:string;input?:unknown}[]}}).profileEditorFixture.calls);
  expect(calls.filter(call=>call.kind==='snapshot').at(-1)?.input).toBe('main');
  expect(calls.filter(call=>call.kind==='save')).toHaveLength(0);
});

test('global shortcut shows the real nested selection chain and does not create an extra bottom tab',async({page})=>{
  await open(page);await visitNodes(page);
  const location=page.getByRole('region',{name:'概览出口定位'});
  await expect(location).toContainText('outer-auto');await expect(location).toContainText('inner-auto');await expect(location).toContainText('node-a');
  await expect(secondary(page).locator('#mobile-group-inner-auto')).toBeVisible();
  await expect(nav(page).getByRole('button')).toHaveCount(4);
  for(const button of await nav(page).getByRole('button').all()){const box=await button.boundingBox();expect(box!.height).toBeGreaterThanOrEqual(44);expect(box!.width).toBeGreaterThanOrEqual(44);}
  expect(await page.evaluate(()=>document.documentElement.scrollWidth)).toBeLessThanOrEqual(320);
});

test('rule mode explains per-rule selection without presenting the old global target as the unique effective leaf',async({page})=>{
  await open(page,'rule');await expect(outletCard(page)).toContainText('按规则选择出口');
  await expect(outletCard(page)).not.toContainText('outer-auto');await visitNodes(page);
  await expect(page.getByRole('region',{name:'概览出口定位'})).toContainText('按规则选择出口');
  await expect(secondary(page).getByRole('button',{name:'确认应用',exact:true})).toHaveCount(0);
});

test('direct mode is explained without manufacturing a direct node absent from the actual inventory',async({page})=>{
  await open(page,'direct');await visitNodes(page);
  await expect(page.getByRole('region',{name:'概览出口定位'})).toContainText('直连模式');
  await expect(secondary(page).getByRole('button',{name:'选择 direct',exact:true})).toHaveCount(0);
  await expect(secondary(page).getByRole('button',{name:'测速节点 direct',exact:true})).toHaveCount(0);
});

test('node location does not replace the mounted root search or expansion when returning',async({page})=>{
  await open(page,'global','independent');await tab(page,'节点').click();
  await page.getByLabel('隐藏已确认离线').check();
  await expect(active(page).locator('[data-node-tag="independent"]')).toHaveCount(0);
  await page.getByRole('textbox',{name:'搜索组和节点'}).fill('manual');
  await tab(page,'概览').click();await visitNodes(page);
  await expect(secondary(page).getByRole('textbox',{name:'搜索组和节点'})).not.toHaveValue('manual');
  await expect(secondary(page).getByLabel('隐藏已确认离线')).toBeChecked();
  await expect(secondary(page).locator('[data-node-tag="independent"]')).toBeVisible();
  await page.getByRole('button',{name:'返回',exact:true}).click();await tab(page,'节点').click();
  await expect(page.getByRole('textbox',{name:'搜索组和节点'})).toHaveValue('manual');
  await expect(page.getByLabel('隐藏已确认离线')).toBeChecked();
  await expect(active(page).locator('[data-node-tag="independent"]')).toHaveCount(0);
});

for(const method of ['header','system','escape','browser'] as const)test(`${method} return reaches the exact overview after a target visit`,async({page})=>{
  await open(page);await visitProfile(page);
  if(method==='header')await page.getByRole('button',{name:'返回',exact:true}).click();
  else if(method==='system')await page.evaluate(()=>window.dispatchEvent(new CustomEvent('mobile:back',{cancelable:true})));
  else if(method==='escape')await page.keyboard.press('Escape');else await page.goBack();
  await expect(tab(page,'概览')).toHaveAttribute('aria-current','page');await expect(profileCard(page)).toBeVisible();
});

test('dirty editor continue preserves the original draft and explicit discard returns through the visit',async({page})=>{
  await open(page);await visitProfile(page);
  await secondary(page).locator('[data-profile-id="main"]').getByRole('button',{name:'编辑代理配置 同名配置',exact:true}).click();
  const body=page.getByLabel('配置 JSON 原文');await body.fill('DIRTY LOCATION DRAFT');
  await requestOverview(page);
  const guard=page.getByRole('dialog',{name:'放弃未保存的修改？',exact:true});await expect(guard).toBeVisible();
  await guard.getByRole('button',{name:'继续编辑',exact:true}).click();await expect(body).toHaveValue('DIRTY LOCATION DRAFT');
  await expect(tab(page,'配置')).toHaveAttribute('aria-current','page');
  await requestOverview(page);await guard.getByRole('button',{name:'放弃修改',exact:true}).click();
  await expect(tab(page,'概览')).toHaveAttribute('aria-current','page');await expect(body).toHaveCount(0);
});

test('pending save blocks navigation without later automatic return or duplicate save',async({page})=>{
  await open(page);await visitProfile(page);
  await secondary(page).locator('[data-profile-id="main"]').getByRole('button',{name:'编辑代理配置 同名配置',exact:true}).click();
  await page.getByLabel('配置 JSON 原文').fill('{"route":{"final":{"type":"direct"}}}');
  await page.evaluate(()=>(window as unknown as {profileEditorFixture:{holdSave:boolean}}).profileEditorFixture.holdSave=true);
  await page.getByRole('button',{name:'仅保存本地配置',exact:true}).click();await requestOverview(page);
  await expect(page.getByRole('dialog',{name:'配置正在保存',exact:true})).toBeVisible();
  await page.evaluate(()=>(window as unknown as {profileEditorFixture:{releaseSave:()=>void}}).profileEditorFixture.releaseSave());
  await expect(page.getByText('已保存，待应用；本地保存不代表内核或 VPN 生效。',{exact:true})).toBeVisible();
  await expect(tab(page,'配置')).toHaveAttribute('aria-current','page');
  expect(await page.evaluate(()=>(window as unknown as {profileEditorFixture:{calls:{kind:string}[]}}).profileEditorFixture.calls.filter(call=>call.kind==='save').length)).toBe(1);
});

test('host replacement with the same profile and core counters refuses an old location instead of opening a same-name target',async({page})=>{
  await open(page);await facts(page,{holdLocation:true});
  const reads=await page.evaluate(()=>(window as unknown as {mobileLocationFixture:{calls:unknown[]}}).mobileLocationFixture.calls.length);
  await profileCard(page).click();
  await expect.poll(()=>page.evaluate(()=>(window as unknown as {mobileLocationFixture:{calls:unknown[]}}).mobileLocationFixture.calls.length)).toBeGreaterThan(reads);
  await facts(page,{host:'host-b'});await release(page,'releaseLocation');
  await expect(page.getByRole('region',{name:'概览配置定位'})).toBeVisible();
  await expect(page.getByRole('region',{name:'概览配置定位'})).toContainText('定位身份已变化');
  await expect(secondary(page).locator('.config-row.location-target')).toHaveCount(0);
  await expect(page.getByLabel('配置 JSON 原文')).toHaveCount(0);
  await page.getByRole('button',{name:'浏览最新配置列表',exact:true}).click();
  await expect(secondary(page).locator('[data-profile-id="main"]')).toBeVisible();
});

test('late directory read cannot switch back or deliver a target after the user selects another bottom tab',async({page})=>{
  await open(page);await facts(page,{holdNodes:true});
  const reads=await page.evaluate(()=>(window as unknown as {mobileLocationFixture:{calls:{kind:string}[]}}).mobileLocationFixture.calls.filter(call=>call.kind==='nodes').length);
  await visitNodes(page);
  await expect.poll(()=>page.evaluate(()=>(window as unknown as {mobileLocationFixture:{calls:{kind:string}[]}}).mobileLocationFixture.calls.filter(call=>call.kind==='nodes').length)).toBeGreaterThan(reads);
  await tab(page,'更多').click();await facts(page,{host:'host-b',generation:'3'});await release(page,'releaseNodes');
  await expect(tab(page,'更多')).toHaveAttribute('aria-current','page');await expect(page.getByRole('heading',{name:'更多',exact:true})).toBeVisible();
  await expect(page.getByRole('dialog')).toHaveCount(0);
});

test('missing current profile opens an ordinary latest list with no fabricated selection or editor',async({page})=>{
  await open(page);await facts(page,{noProfile:true});
  await active(page).getByRole('button',{name:'重新检查',exact:true}).click();
  await profileCard(page).click();await expect(secondary(page).getByRole('button',{name:'新建',exact:true})).toBeVisible();
  await expect(secondary(page).locator('.config-row.location-target')).toHaveCount(0);await expect(page.getByLabel('配置 JSON 原文')).toHaveCount(0);
});

test('a late original-profile admission cannot open its editor over a newer destination',async({page})=>{
  await open(page);await visitProfile(page);await expect(secondary(page).locator('.config-row.location-target')).toHaveAttribute('data-profile-id','main');
  await facts(page,{holdLocation:true});
  const reads=await page.evaluate(()=>(window as unknown as {mobileLocationFixture:{calls:unknown[]}}).mobileLocationFixture.calls.length);
  await secondary(page).locator('[data-profile-id="main"]').getByRole('button',{name:'编辑代理配置 同名配置',exact:true}).click();
  await expect.poll(()=>page.evaluate(()=>(window as unknown as {mobileLocationFixture:{calls:unknown[]}}).mobileLocationFixture.calls.length)).toBeGreaterThan(reads);
  await tab(page,'更多').click();await release(page,'releaseLocation');
  await expect(page.getByRole('heading',{name:'更多',exact:true})).toBeVisible();await expect(page.getByLabel('配置 JSON 原文')).toHaveCount(0);
  expect(await page.evaluate(()=>(window as unknown as {profileEditorFixture:{calls:{kind:string}[]}}).profileEditorFixture.calls.filter(call=>call.kind==='snapshot').length)).toBe(0);
});

test('failed fresh host read preserves the overview and does not fall back to matching profile names',async({page})=>{
  await open(page);await facts(page,{failRead:true});await profileCard(page).click();
  await expect(tab(page,'概览')).toHaveAttribute('aria-current','page');await expect(active(page)).toContainText('定位读取失败');
  await expect(page.getByLabel('配置 JSON 原文')).toHaveCount(0);await expect(page.getByRole('region',{name:'概览配置定位'})).toHaveCount(0);
});

test('leaving and returning to the same overview root retires its original pending location',async({page})=>{
  await open(page);await facts(page,{holdLocation:true});
  const reads=await page.evaluate(()=>(window as unknown as {mobileLocationFixture:{calls:unknown[]}}).mobileLocationFixture.calls.length);
  await profileCard(page).click();
  await expect.poll(()=>page.evaluate(()=>(window as unknown as {mobileLocationFixture:{calls:unknown[]}}).mobileLocationFixture.calls.length)).toBeGreaterThan(reads);
  await tab(page,'更多').click();await expect(page.getByRole('heading',{name:'更多',exact:true})).toBeVisible();
  await tab(page,'概览').click();await release(page,'releaseLocation');
  // Let the released host promise and its DOM effects finish, without a time delay.
  await page.evaluate(()=>new Promise<void>(resolve=>requestAnimationFrame(()=>requestAnimationFrame(()=>resolve()))));
  await expect(tab(page,'概览')).toHaveAttribute('aria-current','page');
  await expect(profileCard(page)).toBeEnabled();
  await expect(page.getByRole('region',{name:'概览配置定位'})).toHaveCount(0);
});

test('returning from the workspace rechecks the original profile binding before restoring its highlight',async({page})=>{
  await open(page);await visitProfile(page);
  const original=secondary(page).locator('.config-row.location-target');
  await expect(original).toHaveAttribute('data-profile-id','main');
  await original.getByRole('button',{name:'前往工作区预检/应用',exact:true}).click();
  await expect(secondary(page).locator('.secondary-header h1')).toHaveText('配置工作区');
  await facts(page,{host:'host-b'});
  await page.getByRole('button',{name:'返回',exact:true}).click();
  await expect(secondary(page).locator('.secondary-header h1')).toHaveText('配置');
  await expect(page.getByRole('region',{name:'概览配置定位'})).toContainText('定位身份已变化');
  await expect(secondary(page).locator('.config-row.location-target')).toHaveCount(0);
  await expect(page.getByLabel('配置 JSON 原文')).toHaveCount(0);
  await page.getByRole('button',{name:'返回',exact:true}).click();
  await expect(tab(page,'概览')).toHaveAttribute('aria-current','page');
});

test('offline current outlet opens the actual configuration directory with an unconfirmed explanation instead of a rule claim',async({page})=>{
  await page.goto('/mobile.html?fixture=mobile-location&traffic-v1&mode=global&offline');
  await expect(profileCard(page)).toContainText('同名配置');await expect(profileCard(page)).toBeEnabled();
  await visitNodes(page);
  const location=page.getByRole('region',{name:'概览出口定位'});
  await expect(location).toContainText('出口未确认');await expect(location).not.toContainText('按规则选择出口');
  await expect(secondary(page).locator('[data-group-tag="outer-auto"]')).toBeVisible();
  await expect(secondary(page).locator('.location-target')).toHaveCount(0);
  await page.getByRole('button',{name:'返回',exact:true}).click();await expect(tab(page,'概览')).toHaveAttribute('aria-current','page');
});
