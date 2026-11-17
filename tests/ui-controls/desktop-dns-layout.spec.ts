import {test,expect} from '@playwright/test';

for(const theme of ['light','dark']){
  test(`rule sets come first and repeated view switching keeps toolbar geometry in ${theme}`,async({page},testInfo)=>{
    await page.goto(`/?panel=endpoints-shell&tab=rules&theme=${theme}`);
    await page.getByRole('tab',{name:'规则',exact:true}).click();
    const view=page.getByRole('group',{name:'规则视图',exact:true});
    await expect(view.getByRole('radio').first()).toHaveText('规则集');
    await expect(view.getByRole('radio',{name:'规则集',exact:true})).toHaveAttribute('aria-checked','true');
    const header=page.locator('.rules-root > .panel-header');
    const before=await header.boundingBox();
    for(let i=0;i<3;i++){
      await view.getByRole('radio',{name:'路由规则',exact:true}).click();
      await expect(page.getByRole('button',{name:'添加路由',exact:true})).toBeVisible();
      const after=await header.boundingBox();
      expect(Math.abs(after!.height-before!.height)).toBeLessThan(1);
      await view.getByRole('radio',{name:'规则集',exact:true}).click();
    }
    await expect(page.locator('.summary-strip')).toHaveCount(0);
    expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
    await page.locator('.rules-root').screenshot({path:testInfo.outputPath(`rules-${theme}.png`),animations:'disabled'});
  });
}

test('DNS lists endpoint-owned resolvers and marks the effective default without copying them to base settings',async({page},testInfo)=>{
  await page.goto('/?panel=dns&endpoint-dns-source=1');
  const row=page.getByRole('article',{name:'端点 DNS Starters 192.168.1.180',exact:true});
  await expect(row).toContainText('来自端点');
  await expect(row).toContainText('默认');
  await expect(row.getByRole('button',{name:'管理端点',exact:true})).toBeVisible();
  await expect(row.getByRole('button',{name:/删除|编辑/})).toHaveCount(0);
  await expect(page.getByText('当前默认服务器',{exact:true})).toBeVisible();
  await page.getByRole('button',{name:'保存并应用',exact:true}).click();
  const saved=JSON.parse((await page.getByLabel('保存结果').textContent())!);
  expect(JSON.stringify(saved.changes)).not.toContain('client-endpoint-dns-fixture');
  await row.screenshot({path:testInfo.outputPath('endpoint-dns-source.png'),animations:'disabled'});
});

test('imported resolvers can be used by DNS dispatch without duplicating their definitions',async({page})=>{
  await page.goto('/?panel=dns&endpoint-dns-source=available');
  const row=page.getByRole('article',{name:'端点 DNS Starters 192.168.1.180',exact:true});
  await expect(row).not.toContainText('默认');
  await page.getByRole('button',{name:'新增 DNS 分流',exact:true}).click();
  const dialog=page.getByRole('dialog');
  await dialog.getByRole('textbox',{name:'匹配值',exact:false}).fill('starmerx.com');
  await dialog.getByRole('button',{name:'分流 DNS 服务器',exact:true}).click();
  await page.getByRole('option',{name:'Starters · 192.168.1.180',exact:true}).click();
  await dialog.getByRole('button',{name:'添加分流',exact:true}).click();
  await page.getByRole('button',{name:'保存并应用',exact:true}).click();
  const saved=JSON.parse((await page.getByLabel('保存结果').textContent())!);
  expect(saved.changes.dns.config.dispatch.at(-1).server).toBe('client-endpoint-dns-fixture-0');
  expect(saved.changes.dns.config.servers['client-endpoint-dns-fixture-0']).toBeUndefined();
});
