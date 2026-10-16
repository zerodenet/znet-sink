import {test,expect} from '@playwright/test';
test('native connection export confirms fixed completed snapshot and privacy before one system delivery',async({page})=>{
 await page.setViewportSize({width:320,height:640});await page.goto('/?panel=connection-export');const dialog=page.getByRole('dialog');
 await expect(dialog).toContainText('2 条');await expect(dialog).toContainText('来源与目标 IP');expect(await dialog.evaluate(el=>el.scrollWidth<=el.clientWidth+1)).toBe(true);
 await expect(dialog.getByRole('button',{name:'保存文件',exact:true})).toBeDisabled();
 await dialog.getByRole('checkbox').check();await dialog.getByRole('button',{name:'保存文件',exact:true}).click();
 await expect(dialog).toContainText('系统文件写入已确认');await expect(dialog.getByRole('button',{name:'系统分享',exact:true})).toBeDisabled();
 await expect(page.locator('#connection-export-calls')).toHaveText(JSON.stringify([JSON.stringify({planId:'fixed-test-snapshot',delivery:'save'})]));
});
test('unknown native connection delivery only reports checking without retry or false success',async({page})=>{
 await page.goto('/?panel=connection-export&export-result=result-unknown');const dialog=page.getByRole('dialog');
 await dialog.getByRole('checkbox').check();await dialog.getByRole('button',{name:'系统分享',exact:true}).click();
 await expect(dialog).toContainText('不会自动重试');await expect(dialog.getByRole('button',{name:'保存文件',exact:true})).toBeDisabled();await expect(dialog.getByRole('button',{name:'系统分享',exact:true})).toBeDisabled();await expect(dialog).not.toContainText('系统文件写入已确认');
});
test('cancelled connection save preserves preview and keeps delivery single-use',async({page})=>{
 await page.goto('/?panel=connection-export&export-result=cancelled');const dialog=page.getByRole('dialog');
 await dialog.getByRole('checkbox').check();await dialog.getByRole('button',{name:'保存文件',exact:true}).click();
 await expect(dialog).toContainText('已取消，原有草稿和数据保留');await expect(dialog.getByRole('button',{name:'保存文件',exact:true})).toBeDisabled();
});
test('background filter changes after preview do not silently expand confirmed target snapshot',async({page})=>{
 await page.goto('/?panel=connection-export');const dialog=page.getByRole('dialog');await expect(dialog).toContainText('2 条');
 const first=await page.locator('#connection-export-previews').textContent();
 await page.evaluate(()=>window.dispatchEvent(new Event('fixture-connection-export-filter')));
 await expect(page.locator('#connection-export-previews')).toHaveText(first!);
 await dialog.getByRole('checkbox').check();await dialog.getByRole('button',{name:'保存文件',exact:true}).click();
 await expect(page.locator('#connection-export-calls')).toContainText('fixed-test-snapshot');
});
test('active export has authoritative instance and revision and freezes original filters',async({page})=>{
 await page.goto('/?panel=connection-export&export-source=active');const dialog=page.getByRole('dialog');
 await expect(dialog).toContainText('导出筛选活动连接');await expect(dialog).toContainText('test-core-instance');await expect(dialog).toContainText('9007199254740993');
 const first=await page.locator('#connection-export-previews').textContent();await page.evaluate(()=>window.dispatchEvent(new Event('fixture-connection-export-filter')));await expect(page.locator('#connection-export-previews')).toHaveText(first!);
 await expect(dialog.getByRole('button',{name:'保存文件',exact:true})).toBeDisabled();await dialog.getByRole('checkbox').check();await dialog.getByRole('button',{name:'保存文件',exact:true}).click();
 await expect(page.locator('#connection-export-calls')).toHaveText(JSON.stringify([JSON.stringify({planId:'fixed-active-snapshot',delivery:'save'})]));await expect(dialog.getByRole('button',{name:'系统分享',exact:true})).toBeDisabled();
});
test('missing mobile runtime remains explained and cannot deliver an invented empty active snapshot',async({page})=>{
 await page.goto('/?panel=connection-export&export-source=active&runtime-unavailable=1');const dialog=page.getByRole('dialog');
 await expect(dialog.getByRole('alert')).toContainText('移动运行服务控制桥尚未适配');await expect(dialog.getByRole('checkbox')).toHaveCount(0);await expect(dialog.getByRole('button',{name:'保存文件',exact:true})).toBeDisabled();await expect(page.locator('#connection-export-calls')).toHaveText('[]');
});
test('active export unknown receipt never replays or fetches new matching records',async({page})=>{
 await page.goto('/?panel=connection-export&export-source=active&export-result=result-unknown');const dialog=page.getByRole('dialog');await expect(dialog).toContainText('test-core-instance');
 await dialog.getByRole('checkbox').check();await dialog.getByRole('button',{name:'系统分享',exact:true}).click();await expect(dialog).toContainText('不会自动重试');
 await page.evaluate(()=>window.dispatchEvent(new Event('fixture-connection-export-filter')));await expect(page.locator('#connection-export-previews')).toHaveText(JSON.stringify([JSON.stringify({search:'first-filter',protocol:'tcp'})]));await expect(dialog.getByRole('button',{name:'系统分享',exact:true})).toBeDisabled();
});
