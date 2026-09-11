import {test,expect} from '@playwright/test';
test('editing a port applies only that field and restoring follows the source',async({page})=>{
  await page.goto('/?panel=endpoint&mode=precedence');
  const port=page.getByRole('textbox',{name:'代理监听端口'});
  await expect(port).toHaveValue('7891');
  await expect(page.getByRole('textbox',{name:'代理监听地址'})).toHaveValue('127.0.0.2');
  await expect(page.getByRole('region',{name:'配置优先级'})).toHaveCount(0);
  await port.fill('7877');
  await page.getByRole('button',{name:'应用',exact:true}).click();
  await expect(page.getByLabel('保存结果')).toContainText('"localProxy.port":7877');
  await expect(page.getByLabel('保存结果')).not.toContainText('localProxy.host');
  await expect(page.getByText('本地修改 · 仅当前配置')).toBeVisible();
  await page.getByRole('button',{name:'恢复配置值',exact:true}).click();
  await expect(port).toHaveValue('7891');
  await expect(page.getByRole('button',{name:'恢复配置值',exact:true})).toBeDisabled();
});
test('failed apply does not label the draft as a saved local edit',async({page})=>{
  await page.goto('/?panel=endpoint&mode=precedence&failure=apply');
  await page.getByRole('textbox',{name:'代理监听端口'}).fill('7877');
  await page.getByRole('button',{name:'应用',exact:true}).click();
  await expect(page.getByRole('alert')).toContainText('端口占用');
  await expect(page.getByRole('button',{name:'已应用',exact:true})).toHaveCount(0);
  await expect(page.getByRole('button',{name:'恢复配置值',exact:true})).toBeDisabled();
});
test('saving while stopped explains that the port takes effect at startup',async({page})=>{
  await page.goto('/?panel=endpoint&mode=precedence&stopped=1');
  await page.getByRole('textbox',{name:'代理监听端口'}).fill('7877');
  await page.getByRole('button',{name:'应用',exact:true}).click();
  await expect(page.getByRole('button',{name:'已保存，启动后生效',exact:true})).toBeVisible();
});
