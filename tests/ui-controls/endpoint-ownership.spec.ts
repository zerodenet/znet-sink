import {test,expect} from '@playwright/test';

test('endpoint and global routing views manage the same record',async({page},testInfo)=>{
 await page.goto('/?panel=endpoints-shell&tab=endpoints&preview=1');
 const card=page.getByRole('article',{name:'端点 wg-home',exact:true});
 await card.getByLabel('wg-home 更多操作',{exact:true}).click();
 await card.getByRole('button',{name:'路由规则',exact:true}).click();
 let dialog=page.getByRole('dialog');
 await expect(dialog.getByRole('region',{name:'wg-home 路由规则列表'})).toContainText('还没有');
 await dialog.getByRole('button',{name:'添加路由规则',exact:true}).click();
 await dialog.getByLabel('匹配内容').fill('internal.example.test');
 await expect(dialog.getByRole('textbox',{name:'目标端点'})).toHaveValue('wg-home');
 await dialog.getByRole('button',{name:'保存并应用',exact:true}).click();
 await expect(dialog.getByRole('button',{name:'internal.example.test → wg-home',exact:true})).toBeVisible();
 await dialog.getByRole('switch',{name:'启用路由 internal.example.test → wg-home'}).click();
 await expect(dialog.getByRole('switch',{name:'启用路由 internal.example.test → wg-home'})).toHaveAttribute('aria-checked','false');
 await dialog.getByRole('button',{name:'internal.example.test → wg-home',exact:true}).click();
 await dialog.getByLabel('匹配内容').fill('internal.example.test\napi.example.test');
 await dialog.getByRole('button',{name:'保存并应用',exact:true}).click();
 await expect(dialog.getByRole('switch')).toHaveAttribute('aria-checked','false');
 await page.getByRole('button',{name:'关闭提示',exact:true}).click();
 await page.screenshot({path:testInfo.outputPath('endpoint-routes-manager.png'),fullPage:true,animations:'disabled'});
 await dialog.locator('.dm-footer').getByRole('button',{name:'关闭',exact:true}).click();
 await page.getByRole('tab',{name:'规则',exact:true}).click();
 await expect(page.getByRole('radio',{name:'规则集',exact:true})).toHaveAttribute('aria-checked','true');
 await page.getByRole('radio',{name:'路由规则',exact:true}).click();
 const routes=page.getByRole('region',{name:'路由规则列表',exact:true});
 await expect(routes.getByRole('switch')).toHaveCount(1);
 await expect(routes.getByRole('switch')).toHaveAttribute('aria-checked','false');
 await page.screenshot({path:testInfo.outputPath('global-route-list.png'),fullPage:true,animations:'disabled'});
 await page.getByRole('radio',{name:'规则集',exact:true}).click();
 await expect(page.getByRole('region',{name:'路由规则列表',exact:true})).toHaveCount(0);
 await expect(page.getByRole('button',{name:/internal.example.test/})).toHaveCount(0);
 await page.getByRole('radio',{name:'路由规则',exact:true}).click();
 await routes.getByRole('button',{name:/删除路由/}).click();
 dialog=page.getByRole('dialog');await dialog.getByRole('button',{name:'确认删除',exact:true}).click();
 await expect(routes).toContainText('还没有');
 await page.getByRole('tab',{name:'端点',exact:true}).click();
 await card.getByLabel('wg-home 更多操作',{exact:true}).click();
 await card.getByRole('button',{name:'路由规则',exact:true}).click();
 await expect(page.getByRole('dialog')).toContainText('还没有使用此端点的路由规则');
});

test('rejected global route toggle restores its confirmed switch state',async({page})=>{
 await page.goto('/?rules-view=routes&preview=1&route-toggle-fails=1');
 await page.getByRole('button',{name:'添加路由',exact:true}).click();
 const dialog=page.getByRole('dialog');
 await dialog.getByLabel('匹配内容').fill('private.example.test');
 await dialog.getByRole('button',{name:'保存并应用',exact:true}).click();
 const routes=page.getByRole('region',{name:'路由规则列表',exact:true});
 await routes.getByRole('switch').click();
 await expect(page.locator('.error-banner[role=alert]')).toContainText('路由更新被拒绝');
 await expect(routes.getByRole('switch')).toHaveAttribute('aria-checked','true');
});

test('DNS endpoint source is a compact context row',async({page},testInfo)=>{
 await page.goto('/?panel=dns&endpoint-dns-source=1');
 const row=page.locator('.endpoint-dns-source');
 await expect(row).toContainText('Starters');
 const box=await row.boundingBox();expect(box!.height).toBeLessThan(60);
 await expect(row).toHaveCSS('text-align','start');
 await expect(row.getByRole('button',{name:'在端点中管理'})).toBeVisible();
 await page.screenshot({path:testInfo.outputPath('dns-source-row.png'),fullPage:true});
});

test('profile hover preserves contrast and managed rows are not disabled controls',async({page},testInfo)=>{
 await page.goto('/?panel=profiles&mode=managed-config');
 const managed=page.locator('.row-open.read-only');
 await expect(managed).toContainText('狗梯 - company');await expect(managed).toHaveCSS('opacity','1');
 const editable=page.getByRole('button',{name:'编辑代理配置 工作配置',exact:true});
 const color=await editable.evaluate(el=>getComputedStyle(el).color);
 await editable.hover();await expect(editable).toHaveCSS('color',color);await expect(editable).toHaveCSS('opacity','1');
 await page.screenshot({path:testInfo.outputPath('profile-hover.png'),fullPage:true});
});

test('select hover keeps selected and highlighted states distinct',async({page},testInfo)=>{
 await page.goto('/?rules-view=routes&preview=1');
 await page.getByRole('button',{name:'添加路由',exact:true}).click();
 await page.getByRole('button',{name:'匹配方式'}).click();
 const selected=page.getByRole('option',{name:'域名后缀',exact:true});
 const hovered=page.getByRole('option',{name:'精确域名',exact:true});
 const normalColor=await hovered.evaluate(el=>getComputedStyle(el).color);
 await hovered.hover();
 await expect(hovered).toHaveAttribute('data-highlighted','');
 await expect(selected).not.toHaveAttribute('data-highlighted');
 await expect(hovered).toHaveCSS('color',normalColor);
 await page.screenshot({path:testInfo.outputPath('neutral-select-hover.png'),fullPage:true,animations:'disabled'});
});

test('rejected endpoint route toggle restores its confirmed switch state',async({page})=>{
 await page.goto('/?panel=endpoints-shell&tab=endpoints&preview=1&route-toggle-fails=1');
 const card=page.getByRole('article',{name:'端点 wg-home',exact:true});
 await card.getByLabel('wg-home 更多操作',{exact:true}).click();await card.getByRole('button',{name:'路由规则',exact:true}).click();
 const dialog=page.getByRole('dialog');await dialog.getByRole('button',{name:'添加路由规则',exact:true}).click();
 await dialog.getByLabel('匹配内容').fill('private.example.test');await dialog.getByRole('button',{name:'保存并应用',exact:true}).click();
 await dialog.getByRole('switch').click();
 await expect(dialog.getByRole('alert')).toContainText('路由更新被拒绝');
 await expect(dialog.getByRole('switch')).toHaveAttribute('aria-checked','true');
});
