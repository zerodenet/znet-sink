import {test,expect} from '@playwright/test';
test('batch database IPs select an endpoint and save a binding in the same request',async({page},testInfo)=>{
 await page.goto('/?rules-view=routes&preview=1');
 await page.getByRole('button',{name:'添加路由',exact:true}).click();
 const dialog=page.getByRole('dialog');
 await dialog.getByRole('button',{name:'匹配方式'}).click();
 await page.getByRole('option',{name:'IPv4 网段',exact:true}).click();
 await dialog.getByLabel('匹配内容').fill('192.168.1.235\n16.0.0.0/8');
 await dialog.getByRole('button',{name:'目标端点'}).click();
 await page.getByRole('option',{name:/wg-home/}).click();
 await page.screenshot({path:testInfo.outputPath('route-editor-default.png'),fullPage:true});
 await dialog.getByRole('button',{name:'保存并应用',exact:true}).click();
 await expect(dialog).toHaveCount(0);
 const saved=JSON.parse(await page.getByLabel('保存结果').textContent() ?? '{}');
 expect(saved.commonBinding.action).toEqual({endpoint:'wg-home'});
 expect(saved.commonBinding.enabled).toBe(true);
 expect(saved.semanticIr.rules).toEqual([{type:'ipv4_cidr',value:'192.168.1.235/32'},{type:'ipv4_cidr',value:'16.0.0.0/8'}]);
});
test('failed save keeps editor and entered domain visible with the error',async({page})=>{
 await page.goto('/?rules-view=routes&preview=1&route-save-fails=1');
 await page.getByRole('button',{name:'添加路由',exact:true}).click();
 const dialog=page.getByRole('dialog');
 await dialog.getByLabel('匹配内容').fill('starmerx.com');
 await dialog.getByRole('button',{name:'保存并应用',exact:true}).click();
 await expect(dialog.getByRole('alert')).toContainText('与提前绕过冲突');
 await expect(dialog.getByLabel('匹配内容')).toHaveValue('starmerx.com');
});

test('rules workspace keeps routing actions in its toolbar without a resident test form',async({page},testInfo)=>{
 await page.goto('/?rules-view=routes&preview=1');
 const toolbar=page.locator('.rules-root > .panel-header');
 await expect(toolbar.getByRole('button',{name:'添加路由',exact:true})).toBeVisible();
 await expect(page.getByRole('region',{name:'端点路由',exact:true})).toHaveCount(0);
 await expect(page.getByRole('textbox',{name:'路由测试目标'})).toHaveCount(0);
 await expect(page.getByRole('button',{name:'测试路由',exact:true})).toHaveCount(0);
 await expect(page.getByRole('dialog')).toHaveCount(0);
 expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
 await page.screenshot({path:testInfo.outputPath('rules-workspace.png'),fullPage:true});
});

test('route modal keeps its footer visible in short windows and restores keyboard focus',async({page},testInfo)=>{
 await page.setViewportSize({width:650,height:480});
 await page.goto('/?rules-view=routes&preview=1&theme=dark');
 const trigger=page.getByRole('button',{name:'添加路由',exact:true});
 await trigger.click();
 const dialog=page.getByRole('dialog');
 await expect(dialog.getByRole('button',{name:'匹配方式'})).toBeEnabled();
 const cancel=dialog.getByRole('button',{name:'取消',exact:true});
 const save=dialog.getByRole('button',{name:'保存并应用',exact:true});
 await expect(cancel).toBeInViewport();await expect(save).toBeInViewport();
 await expect(dialog.locator('.dm-footer')).toContainText('取消');
 await expect(dialog.locator('.dm-body')).toHaveCSS('overflow-y','auto');
 await dialog.getByRole('button',{name:'匹配方式'}).click();
 await page.keyboard.press('Escape');await expect(dialog).toBeVisible();
 await page.screenshot({path:testInfo.outputPath('route-editor-dark-short.png'),fullPage:true});
 await page.keyboard.press('Escape');await expect(dialog).toHaveCount(0);
 await expect(trigger).toBeFocused();
});

test('a saved endpoint route stays in the existing list and reuses the editor',async({page})=>{
 await page.goto('/?rules-view=routes&preview=1');
 await page.getByRole('button',{name:'添加路由',exact:true}).click();
 let dialog=page.getByRole('dialog');
 await dialog.getByLabel('匹配内容').fill('starmerx.com');
 await dialog.getByRole('button',{name:'保存并应用',exact:true}).click();
 await expect(dialog).toHaveCount(0);
 const row=page.getByRole('button',{name:/^starmerx\.com →/});
 await expect(row).toHaveCount(1);await row.click();
 dialog=page.getByRole('dialog');
 await expect(dialog.getByLabel('匹配内容')).toHaveValue('starmerx.com');
 await expect(dialog.getByRole('button',{name:'目标端点'})).toContainText('wg-home');
});

test('endpoint menu opens the shared route editor and returns focus to its menu trigger',async({page})=>{
 await page.goto('/?panel=endpoints-shell&tab=endpoints&preview=1');
 const card=page.getByRole('article',{name:'端点 wg-home',exact:true});
 const menu=card.getByLabel('wg-home 更多操作',{exact:true});
 await menu.click();
 await card.getByRole('button',{name:'路由规则',exact:true}).click();
 const manager=page.getByRole('dialog');
 await manager.getByRole('button',{name:'添加路由规则',exact:true}).click();
 const dialog=page.getByRole('dialog');
 await expect(dialog.getByRole('textbox',{name:'目标端点'})).toHaveValue('wg-home');
 await expect(card.locator('details.options')).not.toHaveAttribute('open','');
 await page.keyboard.press('Escape');
 await expect(page.getByRole('region',{name:'wg-home 路由规则列表'})).toBeVisible();
 await page.keyboard.press('Escape');await expect(dialog).toHaveCount(0);await expect(menu).toBeFocused();
});
