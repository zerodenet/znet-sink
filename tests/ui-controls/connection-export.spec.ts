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
