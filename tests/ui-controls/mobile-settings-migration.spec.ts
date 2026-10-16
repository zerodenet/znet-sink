import { test, expect } from '@playwright/test';
const material=JSON.stringify({schemaVersion:'znet.settings-transfer.v1',clientEdits:{'tun.mtu':1400}});
async function openMigration(page: import('@playwright/test').Page, query='') {
  await page.setViewportSize({width:320,height:700});await page.goto(`/mobile.html${query}`);
  await page.getByRole('navigation',{name:'主导航'}).getByRole('button',{name:'更多'}).click();
  await page.getByRole('button',{name:'设置',exact:true}).click();
  await page.getByRole('button',{name:'设置迁移',exact:true}).click();
}
test('paste and preview do not mutate; fixed scope requires explicit confirmation then pending runtime',async({page})=>{
 await openMigration(page);
 await page.getByRole('textbox',{name:'粘贴迁移 JSON'}).fill(material);
 await page.getByRole('button',{name:'校验并预览'}).click();
 await expect(page.getByRole('article',{name:'固定导入范围'})).toContainText('tun.mtu');
 await expect(page.locator('#settings-transfer-fixture')).toHaveText('["preview"]');
 await page.getByRole('button',{name:'确认导入范围'}).click();
 await expect(page.getByRole('dialog')).toContainText('不会自动开启 VPN');
 await page.getByRole('button',{name:'继续编辑',exact:true}).click();
 await expect(page.locator('#settings-transfer-fixture')).toHaveText('["preview"]');
 await page.getByRole('button',{name:'确认导入范围'}).click();
 await page.getByRole('button',{name:'确认本地导入'}).click();
 await expect(page.getByRole('status').filter({hasText:'已保存本地设置'})).toBeVisible();
 await expect(page.getByRole('button',{name:'进入工作区预检运行应用'})).toBeVisible();
 await expect(page.locator('#settings-transfer-fixture')).toHaveText('["preview","commit:native-fixed-plan"]');
});
test('cancelled native file selection preserves the draft and forbidden fields do not produce preview',async({page})=>{
 await openMigration(page);
 const input=page.getByRole('textbox',{name:'粘贴迁移 JSON'});await input.fill(material);
 await page.getByRole('button',{name:'从系统文件读取'}).click();await expect(input).toHaveValue(material);
 await expect(page.getByRole('status').filter({hasText:'已取消文件选择'})).toBeVisible();
 await input.fill(JSON.stringify({schemaVersion:'znet.settings-transfer.v1',clientEdits:{'tun.enabled':true}}));
 await page.getByRole('button',{name:'校验并预览'}).click();await expect(page.getByRole('alert')).toContainText('敏感');
 await expect(page.getByRole('button',{name:'确认导入范围'})).toHaveCount(0);
});
test('missing save receipt only checks original operation and export distinguishes share acceptance',async({page})=>{
 await openMigration(page,'?migration-unknown');await page.getByRole('textbox',{name:'粘贴迁移 JSON'}).fill(material);
 await page.getByRole('button',{name:'校验并预览'}).click();await page.getByRole('button',{name:'确认导入范围'}).click();await page.getByRole('button',{name:'确认本地导入'}).click();
 await expect(page.getByRole('button',{name:'核对原操作'})).toBeVisible();await expect(page.getByRole('alert')).not.toContainText('raw-secret-token');
 await page.getByRole('button',{name:'核对原操作'}).click();await expect(page.locator('#settings-transfer-fixture')).toHaveText('["preview","commit:native-fixed-plan","status:native-fixed-plan"]');
 await page.getByRole('button',{name:'预览可迁移设置'}).click();await expect(page.getByText('已整体排除：dns。',{exact:false})).toBeVisible();
 await page.getByRole('button',{name:'系统分享',exact:true}).click();await expect(page.getByRole('status').filter({hasText:'不代表接收方已保存'})).toBeVisible();
 expect(await page.evaluate(()=>document.documentElement.scrollWidth-document.documentElement.clientWidth)).toBe(0);
});

test('native back closes a confirmation first and protects the unsaved migration draft',async({page})=>{
  await openMigration(page);
  const input=page.getByRole('textbox',{name:'粘贴迁移 JSON'});
  await input.fill(material); await page.getByRole('button',{name:'校验并预览'}).click();
  await page.getByRole('button',{name:'确认导入范围'}).click();
  expect(await page.evaluate(()=>window.dispatchEvent(new CustomEvent('mobile:back',{cancelable:true})))).toBe(false);
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(input).toHaveValue(material);
  expect(await page.evaluate(()=>window.dispatchEvent(new CustomEvent('mobile:back',{cancelable:true})))).toBe(false);
  await expect(page.getByRole('dialog')).toContainText('离开未保存的迁移草稿');
  await page.getByRole('button',{name:'继续编辑',exact:true}).click();
  await expect(input).toHaveValue(material);
});

test('unknown original save survives leaving and reopening the migration page',async({page})=>{
  await openMigration(page,'?migration-unknown');
  await page.getByRole('textbox',{name:'粘贴迁移 JSON'}).fill(material);
  await page.getByRole('button',{name:'校验并预览'}).click();
  await page.getByRole('button',{name:'确认导入范围'}).click();
  await page.getByRole('button',{name:'确认本地导入'}).click();
  await expect(page.getByRole('button',{name:'核对原操作'})).toBeVisible();
  await page.getByRole('button',{name:'返回',exact:true}).click();
  await page.getByRole('button',{name:'保留原操作并离开'}).click();
  await page.getByRole('button',{name:'设置迁移',exact:true}).click();
  await expect(page.getByRole('button',{name:'核对原操作'})).toBeVisible();
  await page.getByRole('button',{name:'核对原操作'}).click();
  await expect(page.locator('#settings-transfer-fixture')).toHaveText('["preview","commit:native-fixed-plan","status:native-fixed-plan"]');
});
